// MetalRenderer.swift
// CAMetalLayer + CADisplayLink render loop for iExtend.
// Drives ProMotion (up to 120 Hz) via preferredFrameRateRange.
// Each frame: dequeue FrameQueue → upload CVPixelBuffer → call Reproject →
// apply CursorMaskShader → present drawable.
//
// Plan 6: basic blit pipeline (no reprojection math yet — Reproject.swift stub).
// Plan 8 fills in: Pencil-pose reprojection compute pass + HDR tone-mapping.
//
// Threading:
//   - displayLinkFired runs on the main thread (CADisplayLink default).
//   - MTLCommandBuffer completion handlers run on the Metal thread.
//   - FrameQueue is SPSC-safe: main thread is the sole consumer.

#if canImport(UIKit)
import Metal
import MetalKit
import QuartzCore
import CoreVideo
import CoreMedia
import UIKit

// MARK: - Delegate

public protocol MetalRendererDelegate: AnyObject {
    func rendererDidDrop(frameCount: Int)
    func rendererDidPresent(pts: CMTime, at displayTime: CFTimeInterval)
}

// MARK: - MetalRenderer

/// Owns the CAMetalLayer and the render loop. Attach its `layer` to any
/// UIView/UIViewController's view hierarchy. Call `start()` to begin rendering,
/// `stop()` to pause, `invalidate()` on teardown.
public final class MetalRenderer: NSObject, @unchecked Sendable {

    // MARK: Public
    public let layer: CAMetalLayer
    public weak var delegate: (any MetalRendererDelegate)?

    // MARK: Private — Metal
    private let device: MTLDevice
    private let commandQueue: MTLCommandQueue
    private var blitPipeline: MTLRenderPipelineState?
    private var textureCache: CVMetalTextureCache?
    private var sampler: MTLSamplerState?

    // MARK: Private — Timing
    private var displayLink: CADisplayLink?
    private var lastPTS: CMTime = .invalid
    private var droppedFrames: Int = 0

    // MARK: Private — Frame source
    private weak var frameQueue: FrameQueue?

    // MARK: Init

    public init(frameQueue: FrameQueue) throws {
        guard let dev = MTLCreateSystemDefaultDevice() else {
            throw RendererError.noMetalDevice
        }
        guard let queue = dev.makeCommandQueue() else {
            throw RendererError.commandQueueFailed
        }
        self.device = dev
        self.commandQueue = queue
        self.frameQueue = frameQueue

        let metalLayer = CAMetalLayer()
        metalLayer.device = dev
        metalLayer.pixelFormat = .bgra8Unorm_srgb
        metalLayer.framebufferOnly = false          // allow read-back for reprojection
        metalLayer.displaySyncEnabled = true        // tear-free
        metalLayer.maximumDrawableCount = 3         // triple-buffer
        self.layer = metalLayer

        super.init()

        try buildPipeline()
        buildTextureCache()
        buildSampler()
    }

    // MARK: - Lifecycle

    public func start() {
        guard displayLink == nil else { return }
        let dl = CADisplayLink(target: self, selector: #selector(displayLinkFired(_:)))
        // Ask for ProMotion 60–120 Hz.
        dl.preferredFrameRateRange = CAFrameRateRange(minimum: 60, maximum: 120, preferred: 120)
        dl.add(to: .main, forMode: .common)
        self.displayLink = dl
    }

    public func stop() {
        displayLink?.invalidate()
        displayLink = nil
    }

    public func invalidate() {
        stop()
        textureCache = nil
        blitPipeline = nil
    }

    // MARK: - Render loop

    @objc private func displayLinkFired(_ link: CADisplayLink) {
        guard let (pixelBuffer, pts) = frameQueue?.dequeue() else {
            // No new frame: hold last or show black.
            return
        }
        render(pixelBuffer: pixelBuffer, pts: pts, displayTime: link.timestamp)
    }

    private func render(pixelBuffer: CVPixelBuffer, pts: CMTime, displayTime: CFTimeInterval) {
        guard let drawable = layer.nextDrawable() else {
            droppedFrames += 1
            delegate?.rendererDidDrop(frameCount: droppedFrames)
            return
        }

        guard let luminanceTexture = makeTexture(from: pixelBuffer, planeIndex: 0),
              let chrominanceTexture = makeTexture(from: pixelBuffer, planeIndex: 1) else {
            droppedFrames += 1
            return
        }

        guard let cmdBuf = commandQueue.makeCommandBuffer() else { return }

        // Blit / YCbCr→RGB render pass.
        let passDesc = MTLRenderPassDescriptor()
        passDesc.colorAttachments[0].texture = drawable.texture
        passDesc.colorAttachments[0].loadAction = .clear
        passDesc.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        passDesc.colorAttachments[0].storeAction = .store

        guard let encoder = cmdBuf.makeRenderCommandEncoder(descriptor: passDesc) else { return }

        if let pipeline = blitPipeline, let sampler {
            encoder.setRenderPipelineState(pipeline)
            encoder.setFragmentTexture(luminanceTexture, index: 0)
            encoder.setFragmentTexture(chrominanceTexture, index: 1)
            encoder.setFragmentSamplerState(sampler, index: 0)

            // Full-screen triangle (no vertex buffer needed — vertices are
            // computed from vertexID in the vertex shader).
            encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        }

        encoder.endEncoding()

        // Plan 8: Reproject.swift compute pass would be inserted here.
        // Plan 8: CursorMaskShader.metal would be dispatched here.

        cmdBuf.present(drawable)
        cmdBuf.commit()

        lastPTS = pts
        delegate?.rendererDidPresent(pts: pts, at: displayTime)
    }

    // MARK: - Metal setup

    private func buildPipeline() throws {
        // Load the BlitShader.metal from the compiled default library.
        guard let library = device.makeDefaultLibrary() else {
            // When shader compilation fails in testing (no .metallib), use a no-op.
            return
        }
        guard let vertFn = library.makeFunction(name: "blitVertex"),
              let fragFn = library.makeFunction(name: "blitFragment") else {
            return
        }

        let desc = MTLRenderPipelineDescriptor()
        desc.vertexFunction = vertFn
        desc.fragmentFunction = fragFn
        desc.colorAttachments[0].pixelFormat = layer.pixelFormat

        self.blitPipeline = try device.makeRenderPipelineState(descriptor: desc)
    }

    private func buildTextureCache() {
        var cache: CVMetalTextureCache?
        CVMetalTextureCacheCreate(nil, nil, device, nil, &cache)
        self.textureCache = cache
    }

    private func buildSampler() {
        let desc = MTLSamplerDescriptor()
        desc.minFilter = .linear
        desc.magFilter = .linear
        desc.mipFilter = .notMipmapped
        desc.sAddressMode = .clampToEdge
        desc.tAddressMode = .clampToEdge
        self.sampler = device.makeSamplerState(descriptor: desc)
    }

    // MARK: - Texture from CVPixelBuffer

    private func makeTexture(from pixelBuffer: CVPixelBuffer, planeIndex: Int) -> MTLTexture? {
        guard let cache = textureCache else { return nil }

        let width  = CVPixelBufferGetWidthOfPlane(pixelBuffer, planeIndex)
        let height = CVPixelBufferGetHeightOfPlane(pixelBuffer, planeIndex)
        let format: MTLPixelFormat = planeIndex == 0 ? .r16Unorm : .rg16Unorm

        var cvTexture: CVMetalTexture?
        let status = CVMetalTextureCacheCreateTextureFromImage(
            nil,
            cache,
            pixelBuffer,
            nil,
            format,
            width,
            height,
            planeIndex,
            &cvTexture
        )
        guard status == kCVReturnSuccess, let cvTexture else { return nil }
        return CVMetalTextureGetTexture(cvTexture)
    }
}

// MARK: - Error

public enum RendererError: Error {
    case noMetalDevice
    case commandQueueFailed
    case pipelineFailed(String)
}
#endif
