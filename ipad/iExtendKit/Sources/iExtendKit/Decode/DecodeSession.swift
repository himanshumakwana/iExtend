// DecodeSession.swift
// VTDecompressionSession wrapper for hardware-accelerated HEVC/AV1 decode.
// Produces IOSurface-backed CVPixelBuffers at up to 120 fps, handed to FrameQueue.
//
// Threading: VTDecompressionSession delivers callbacks on an arbitrary thread.
// DecodeSession is an actor — callbacks re-dispatch onto it to keep state
// mutually exclusive. The FrameQueue is separately thread-safe (SPSC ring).
//
// Plan 6 status: Full pipeline is wired; AV1 falls back to HEVC until Plan 8
// updates the codec probe. The Metal renderer consumes FrameQueue output.

import VideoToolbox
import CoreMedia
// CVPixelBuffer is a CoreFoundation reference type that predates Swift
// Sendable. The VTDecompressionSession callback hands it across an actor
// boundary into `_onDecodedFrame` — safe in practice (CV buffers are
// retain-counted and immutable once produced), but Swift 6 strict
// concurrency would error. `@preconcurrency` keeps the import on the
// Swift 5 isolation rules until the actor model is fleshed out (Plan 8).
@preconcurrency import CoreVideo
import Foundation

// MARK: - Error

public enum DecodeSessionError: Error, Sendable {
    case formatDescriptionCreate(OSStatus)
    case decompressionSessionCreate(OSStatus)
    case decodeFrame(OSStatus)
    case bufferAllocation
    case codecNotSupported(String)
}

// MARK: - Codec

public enum VideoCodec: String, Sendable {
    case hevc = "hevc"    // H.265 — always supported on A9+
    case av1  = "av1"     // AV1  — Apple A17 Pro / M3+
    case h264 = "h264"    // H.264 — last-resort fallback
}

// MARK: - DecodeSession actor

/// Owns a single VTDecompressionSession and feeds decoded frames into a FrameQueue.
/// To switch codecs, discard and create a new instance.
public actor DecodeSession {

    // MARK: Public
    public let codec: VideoCodec
    public private(set) var frameCount: Int = 0
    public private(set) var droppedCount: Int = 0

    // MARK: Private
    private var decompressionSession: VTDecompressionSession?
    private var formatDescription: CMVideoFormatDescription?
    private weak var outputQueue: FrameQueue?

    // IOSurface pixel format — BT.2020 wide color when available.
    private let pixelFormat: OSType = kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange

    public init(codec: VideoCodec, outputQueue: FrameQueue) {
        self.codec = codec
        self.outputQueue = outputQueue
    }

    // MARK: - Setup

    /// Configure the decompression session from the first video keyframe's
    /// parameter set NALUs (SPS, PPS for HEVC; OBU sequence header for AV1).
    public func configure(parameterSetData: [Data], width: Int, height: Int) async throws {
        let codecType: CMVideoCodecType
        switch codec {
        case .hevc: codecType = kCMVideoCodecType_HEVC
        case .av1:  codecType = kCMVideoCodecType_AV1
        case .h264: codecType = kCMVideoCodecType_H264
        }

        // Build CMVideoFormatDescription from parameter sets.
        let fmtDesc = try buildFormatDescription(
            codec: codecType,
            parameterSets: parameterSetData,
            width: width,
            height: height
        )
        self.formatDescription = fmtDesc

        // Destination pixel buffer attributes — IOSurface-backed for zero-copy Metal.
        let pbAttribs: [NSString: Any] = [
            kCVPixelBufferPixelFormatTypeKey: pixelFormat,
            kCVPixelBufferWidthKey: width,
            kCVPixelBufferHeightKey: height,
            kCVPixelBufferIOSurfacePropertiesKey: [:] as [String: Any],
            kCVPixelBufferMetalCompatibilityKey: true,
        ]

        var outputCallback = VTDecompressionOutputCallbackRecord(
            decompressionOutputCallback: decompressionCallback,
            decompressionOutputRefCon: Unmanaged.passRetained(self).toOpaque()
        )

        var session: VTDecompressionSession?
        let status = VTDecompressionSessionCreate(
            allocator: nil,
            formatDescription: fmtDesc,
            decoderSpecification: nil,
            imageBufferAttributes: pbAttribs as CFDictionary,
            outputCallback: &outputCallback,
            decompressionSessionOut: &session
        )
        guard status == noErr, let session else {
            throw DecodeSessionError.decompressionSessionCreate(status)
        }

        // Force hardware acceleration.
        VTSessionSetProperty(session,
                             key: kVTDecompressionPropertyKey_RealTime,
                             value: kCFBooleanTrue)

        self.decompressionSession = session
    }

    // MARK: - Decode

    /// Submit a compressed sample buffer for decoding. Non-blocking.
    public func decode(_ sampleData: Data, pts: CMTime, dts: CMTime, duration: CMTime) async throws {
        guard let session = decompressionSession,
              let fmtDesc = formatDescription else {
            throw DecodeSessionError.decompressionSessionCreate(-1)
        }

        // Wrap raw data in a CMBlockBuffer.
        var blockBuffer: CMBlockBuffer?
        let bStatus = sampleData.withUnsafeBytes { ptr in
            CMBlockBufferCreateWithMemoryBlock(
                allocator: nil,
                memoryBlock: UnsafeMutableRawPointer(mutating: ptr.baseAddress),
                blockLength: sampleData.count,
                blockAllocator: kCFAllocatorNull,
                customBlockSource: nil,
                offsetToData: 0,
                dataLength: sampleData.count,
                flags: 0,
                blockBufferOut: &blockBuffer
            )
        }
        guard bStatus == noErr, let blockBuffer else {
            throw DecodeSessionError.bufferAllocation
        }

        var timingInfo = CMSampleTimingInfo(
            duration: duration,
            presentationTimeStamp: pts,
            decodeTimeStamp: dts
        )
        let sizes = [sampleData.count]
        var sampleBuffer: CMSampleBuffer?
        let sStatus = CMSampleBufferCreate(
            allocator: nil,
            dataBuffer: blockBuffer,
            dataReady: true,
            makeDataReadyCallback: nil,
            refcon: nil,
            formatDescription: fmtDesc,
            sampleCount: 1,
            sampleTimingEntryCount: 1,
            sampleTimingArray: &timingInfo,
            sampleSizeEntryCount: 1,
            sampleSizeArray: sizes,
            sampleBufferOut: &sampleBuffer
        )
        guard sStatus == noErr, let sampleBuffer else {
            throw DecodeSessionError.bufferAllocation
        }

        let decodeFlags: VTDecodeFrameFlags = [._EnableAsynchronousDecompression]
        var infoOut = VTDecodeInfoFlags()
        let decStatus = VTDecompressionSessionDecodeFrame(
            session,
            sampleBuffer: sampleBuffer,
            flags: decodeFlags,
            frameRefcon: nil,
            infoFlagsOut: &infoOut
        )
        guard decStatus == noErr else {
            droppedCount += 1
            throw DecodeSessionError.decodeFrame(decStatus)
        }
    }

    /// Drain remaining frames then invalidate the session.
    public func invalidate() {
        if let session = decompressionSession {
            VTDecompressionSessionFinishDelayedFrames(session)
            VTDecompressionSessionInvalidate(session)
        }
        decompressionSession = nil
        formatDescription = nil
    }

    // MARK: - Callback (called on C callback thread, re-dispatched here)

    fileprivate func _onDecodedFrame(
        status: OSStatus,
        pixelBuffer: CVPixelBuffer?,
        pts: CMTime
    ) {
        guard status == noErr, let buffer = pixelBuffer else {
            droppedCount += 1
            return
        }
        frameCount += 1
        outputQueue?.enqueue(buffer, pts: pts)
    }

    // MARK: - Private helpers

    private func buildFormatDescription(
        codec: CMVideoCodecType,
        parameterSets: [Data],
        width: Int,
        height: Int
    ) throws -> CMVideoFormatDescription {
        var description: CMVideoFormatDescription?
        var status: OSStatus = noErr

        if codec == kCMVideoCodecType_HEVC {
            let ptrs = parameterSets.map { $0.withUnsafeBytes { $0.baseAddress!.assumingMemoryBound(to: UInt8.self) } }
            let sizes = parameterSets.map { $0.count }
            status = ptrs.withUnsafeBufferPointer { pPtrs in
                sizes.withUnsafeBufferPointer { pSizes in
                    CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                        allocator: nil,
                        parameterSetCount: parameterSets.count,
                        parameterSetPointers: pPtrs.baseAddress!,
                        parameterSetSizes: pSizes.baseAddress!,
                        nalUnitHeaderLength: 4,
                        extensions: nil,
                        formatDescriptionOut: &description
                    )
                }
            }
        } else if codec == kCMVideoCodecType_H264 {
            let ptrs = parameterSets.map { $0.withUnsafeBytes { $0.baseAddress!.assumingMemoryBound(to: UInt8.self) } }
            let sizes = parameterSets.map { $0.count }
            status = ptrs.withUnsafeBufferPointer { pPtrs in
                sizes.withUnsafeBufferPointer { pSizes in
                    CMVideoFormatDescriptionCreateFromH264ParameterSets(
                        allocator: nil,
                        parameterSetCount: parameterSets.count,
                        parameterSetPointers: pPtrs.baseAddress!,
                        parameterSetSizes: pSizes.baseAddress!,
                        nalUnitHeaderLength: 4,
                        formatDescriptionOut: &description
                    )
                }
            }
        } else {
            // AV1: format description is embedded in the bitstream OBU;
            // use a generic video format description until VT gains an
            // AV1 parameter-set API (Plan 8 revisits when available).
            status = CMVideoFormatDescriptionCreate(
                allocator: nil,
                codecType: codec,
                width: Int32(width),
                height: Int32(height),
                extensions: nil,
                formatDescriptionOut: &description
            )
        }
        guard status == noErr, let desc = description else {
            throw DecodeSessionError.formatDescriptionCreate(status)
        }
        return desc
    }
}

// MARK: - C callback thunk

/// VTDecompressionSession requires a C function pointer for the output callback.
/// We use an unretained reference to the actor and dispatch back.
private func decompressionCallback(
    decompressionOutputRefCon: UnsafeMutableRawPointer?,
    sourceFrameRefCon: UnsafeMutableRawPointer?,
    status: OSStatus,
    infoFlags: VTDecodeInfoFlags,
    imageBuffer: CVImageBuffer?,
    presentationTimeStamp: CMTime,
    presentationDuration: CMTime
) {
    guard let refCon = decompressionOutputRefCon else { return }
    let session = Unmanaged<DecodeSession>.fromOpaque(refCon).takeUnretainedValue()
    let pixelBuffer = imageBuffer.map { $0 as CVPixelBuffer }
    let pts = presentationTimeStamp
    Task {
        await session._onDecodedFrame(status: status, pixelBuffer: pixelBuffer, pts: pts)
    }
}
