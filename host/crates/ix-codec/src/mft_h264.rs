//! Media Foundation H.264 encoder (Windows).
//!
//! Wraps the `IMFTransform` returned by `MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,
//! H264 output, NV12 input)`. With `MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_
//! SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER` the OS returns hardware encoders
//! first (NVENC / AMD / Intel Quick Sync), falling back to the in-box
//! Microsoft software H.264 encoder. We just take the first activator, log
//! its friendly name, and feed frames.
//!
//! This is the path that solves the "OpenH264 single-threaded → 15 fps at
//! 1080p" bottleneck without per-vendor SDK bring-up.
//!
//! # Frame format
//!
//! Input: NV12 (semi-planar: Y plane then interleaved UV plane). The
//! capture path produces I420 (planar) so we re-interleave on the way in.
//! Y bytes are identical between I420 and NV12; only the chroma layout
//! changes.
//!
//! Output: Annex-B H.264 NAL units (SPS/PPS prepended on IDR slices).
//! The same RFC 6184 packetizer that handles X264Sw bitstreams accepts
//! these unchanged.
//!
//! # Threading
//!
//! MF objects are COM. `MFStartup` is reference-counted per process and
//! safe to call repeatedly. We initialize COM in MULTITHREADED mode so
//! the encoder thread (a `std::thread`) can drive the transform without
//! a window message pump.
//!
//! `IMFTransform` is **not** thread-safe for concurrent calls; we keep
//! it confined to the one screen-share encoder thread.
//!
//! `#[cfg(windows)]` is applied at the `mod mft_h264;` declaration in
//! `lib.rs`; repeating it as an inner attribute here would trigger
//! `clippy::duplicated_attributes`.

use crate::common::SharedConfig;
use crate::{
    CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, Profile,
};
use ix_display::{DamageRect, GpuFrame};
use std::collections::VecDeque;
use std::ptr;
use tracing::info;

use windows::core::{Interface, PWSTR, VARIANT};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11Multithread, D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
    D3D11_SDK_VERSION,
};
use windows::Win32::Media::MediaFoundation::{
    eAVEncCommonRateControlMode_CBR, eAVEncH264VProfile_Base, CODECAPI_AVEncCommonLowLatency,
    CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonQualityVsSpeed,
    CODECAPI_AVEncCommonRateControlMode, CODECAPI_AVEncCommonRealTime,
    CODECAPI_AVEncMPVDefaultBPictureCount, CODECAPI_AVEncVideoForceKeyFrame, ICodecAPI,
    IMFActivate, IMFDXGIDeviceManager, IMFMediaEventGenerator, IMFSample, IMFTransform,
    METransformHaveOutput, METransformNeedInput, MFCreateDXGIDeviceManager, MFCreateMediaType,
    MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video, MFSampleExtension_CleanPoint,
    MFStartup, MFTEnumEx, MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_H264, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive, MFSTARTUP_LITE, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM,
    MFT_MESSAGE_SET_D3D_MANAGER, MFT_OUTPUT_DATA_BUFFER, MFT_OUTPUT_STREAM_INFO,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFT_REGISTER_TYPE_INFO, MF_EVENT_FLAG_NO_WAIT,
    MF_E_NO_EVENTS_AVAILABLE, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE,
    MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC,
    MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};

/// One-shot per-process Media Foundation startup. MFStartup itself is
/// refcounted; this just makes the COM mode deterministic.
fn ensure_mf_initialized() -> Result<(), CodecError> {
    unsafe {
        // CoInitializeEx returns S_FALSE if already initialized in the same
        // mode and RPC_E_CHANGED_MODE if a different mode was selected on
        // this thread. Both are non-fatal for our purposes.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_LITE)
            .map_err(|e| CodecError::Init(format!("MFStartup: {e}")))?;
    }
    Ok(())
}

/// Encode FPS → MF packed UINT64 (high 32 = numerator, low 32 = denominator).
#[inline]
fn pack_u64(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | (lo as u64)
}

/// Read an IMFActivate's friendly name as a Rust String. Returns "<unnamed>"
/// on lookup failure so logging never panics.
fn activate_friendly_name(act: &IMFActivate) -> String {
    unsafe {
        let mut ptr: PWSTR = PWSTR::null();
        let mut len: u32 = 0;
        if act
            .GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut ptr, &mut len)
            .is_err()
            || ptr.0.is_null()
        {
            return "<unnamed>".to_string();
        }
        let slice = std::slice::from_raw_parts(ptr.0, len as usize);
        let name = String::from_utf16_lossy(slice);
        CoTaskMemFree(Some(ptr.0 as _));
        name
    }
}

/// Convert I420 (planar Y, U, V) → NV12 (planar Y, interleaved UV).
/// Y plane is identical; only chroma needs reordering.
fn i420_to_nv12(yuv: &[u8], w: u32, h: u32, out: &mut Vec<u8>) {
    let w = w as usize;
    let h = h as usize;
    let y_size = w * h;
    let uv_w = w / 2;
    let uv_h = h / 2;
    let uv_plane = uv_w * uv_h;

    out.resize(y_size + uv_plane * 2, 0);
    out[..y_size].copy_from_slice(&yuv[..y_size]);

    let u_src = &yuv[y_size..y_size + uv_plane];
    let v_src = &yuv[y_size + uv_plane..y_size + uv_plane * 2];
    let uv_dst = &mut out[y_size..];
    for i in 0..uv_plane {
        uv_dst[i * 2] = u_src[i];
        uv_dst[i * 2 + 1] = v_src[i];
    }
}

/// D3D11 device + DXGI device manager. Hardware H.264 MFTs require this:
/// without an `IMFDXGIDeviceManager` set via `MFT_MESSAGE_SET_D3D_MANAGER`
/// they refuse `ActivateObject` (D3DERR_INVALIDCALL, 0x8876086C) because
/// they need GPU access during initialization.
///
/// Drop order matters — the device manager must outlive the transform; we
/// achieve that by storing this struct AFTER the transform in `MftH264`
/// (Rust drops fields in declaration order, so the transform is released
/// first).
struct D3DContext {
    _device: ID3D11Device,
    manager: IMFDXGIDeviceManager,
}

/// Hardware-accelerated H.264 encoder via Media Foundation.
pub struct MftH264 {
    cfg: SharedConfig,
    transform: IMFTransform,
    /// Set on async MFTs (NVENC, AMD VCN). When true, per-frame encoding
    /// is event-driven: wait for `METransformNeedInput` before
    /// `ProcessInput`, then for `METransformHaveOutput` before
    /// `ProcessOutput`. Sync MFTs (the in-box Microsoft software
    /// encoder) use the simple poll-based path.
    is_async: bool,
    /// Cached event generator. Only meaningful when `is_async` is true.
    event_gen: Option<IMFMediaEventGenerator>,
    /// Async-only: samples waiting to be fed. We push every input here
    /// then drain `METransformNeedInput` events without blocking, popping
    /// from the front. This keeps NVENC's pipeline full (depth ≥ 2) so it
    /// doesn't stall waiting for more input before producing output.
    input_queue: VecDeque<IMFSample>,
    // Held for lifetime — see comment on D3DContext.
    _d3d: D3DContext,
    output_provides_samples: bool,
    output_sample_size: u32,
    nv12_buf: Vec<u8>,
    out_buf: Vec<u8>,
    sample_index: u32,
    pending_keyframe: bool,
    current_kbps: u32,
}

impl MftH264 {
    /// Create the encoder. Selects the highest-priority MFT that accepts
    /// NV12 input + H.264 output for the given resolution. Returns
    /// `CodecError::NotAvailable` if MF can't find one (extremely rare on
    /// Windows 10+, since the in-box software encoder always satisfies the
    /// query).
    pub fn new(cfg: SharedConfig) -> Result<Self, CodecError> {
        ensure_mf_initialized()?;

        let d3d = create_d3d_context()?;
        let (transform, encoder_name, is_async) = pick_encoder(&cfg, &d3d.manager)?;
        let event_gen = if is_async {
            transform.cast::<IMFMediaEventGenerator>().ok()
        } else {
            None
        };

        // Some encoders need the output sample provided by the caller; the
        // hardware ones generally provide their own. Cache the answer once.
        let info: MFT_OUTPUT_STREAM_INFO = unsafe {
            transform
                .GetOutputStreamInfo(0)
                .map_err(|e| CodecError::Init(format!("GetOutputStreamInfo: {e}")))?
        };
        let output_provides_samples =
            (info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32) != 0;

        unsafe {
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(|e| CodecError::Init(format!("BEGIN_STREAMING: {e}")))?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|e| CodecError::Init(format!("START_OF_STREAM: {e}")))?;
        }

        info!(
            encoder = %encoder_name,
            width = cfg.width,
            height = cfg.height,
            fps = cfg.fps_num,
            bitrate_kbps = cfg.initial_bitrate_kbps,
            provides_samples = output_provides_samples,
            is_async,
            "MF H.264 encoder ready"
        );

        let current_kbps = cfg.initial_bitrate_kbps;
        Ok(Self {
            cfg,
            transform,
            is_async,
            event_gen,
            input_queue: VecDeque::new(),
            _d3d: d3d,
            output_provides_samples,
            output_sample_size: info.cbSize,
            nv12_buf: Vec::new(),
            out_buf: Vec::new(),
            sample_index: 0,
            pending_keyframe: false,
            current_kbps,
        })
    }

    /// Encode a YUV420P (I420) buffer matching the X264Sw signature so
    /// `screen_share.rs` can use either encoder interchangeably.
    ///
    /// Returns `Ok(None)` if the encoder has buffered the input but hasn't
    /// produced output yet — this happens on the first 1-2 frames with
    /// some hardware encoders. The caller should treat None as "skip this
    /// pulse, no peer frame to broadcast" rather than as an error.
    pub fn encode_yuv420(
        &mut self,
        yuv: Vec<u8>,
        width: u32,
        height: u32,
        pts_us: u64,
    ) -> Result<Option<EncodedSlice>, CodecError> {
        if width != self.cfg.width || height != self.cfg.height {
            return Err(CodecError::EncodeFailed(format!(
                "frame size {width}x{height} != configured {}x{}",
                self.cfg.width, self.cfg.height
            )));
        }

        i420_to_nv12(&yuv, width, height, &mut self.nv12_buf);

        // Build an IMFSample wrapping the NV12 bytes.
        let sample = unsafe { create_input_sample(&self.nv12_buf, pts_us, self.cfg.fps_num)? };

        // Mark the next sample as a keyframe boundary if requested. The
        // Microsoft software encoder honors `CleanPoint`; hardware encoders
        // usually need the ICodecAPI route, so we do both.
        if self.pending_keyframe {
            unsafe {
                let _ = sample.SetUINT32(&MFSampleExtension_CleanPoint, 1);
            }
            if let Ok(codec_api) = self.transform.cast::<ICodecAPI>() {
                let var = VARIANT::from(true);
                let _ = unsafe {
                    codec_api.SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &var as *const _)
                };
            }
            self.pending_keyframe = false;
        }

        self.out_buf.clear();
        let mut is_keyframe = false;

        if self.is_async {
            self.drive_async(sample, &mut is_keyframe)?;
        } else {
            self.drive_sync(&sample, &mut is_keyframe)?;
        }

        if self.out_buf.is_empty() {
            return Ok(None);
        }

        let slice_index = self.sample_index;
        self.sample_index = self.sample_index.wrapping_add(1);
        Ok(Some(EncodedSlice {
            data: std::mem::take(&mut self.out_buf),
            is_keyframe,
            pts_us: pts_us as i64,
            slice_index,
        }))
    }

    /// Sync path: feed input, then drain ProcessOutput until it reports
    /// NEED_MORE_INPUT. Used by the Microsoft in-box software MFT.
    fn drive_sync(&mut self, sample: &IMFSample, is_keyframe: &mut bool) -> Result<(), CodecError> {
        unsafe {
            self.transform
                .ProcessInput(0, sample, 0)
                .map_err(|e| CodecError::EncodeFailed(format!("ProcessInput: {e}")))?;
        }
        loop {
            match self.process_one_output(is_keyframe)? {
                ProcessOutputStep::Got => continue,
                ProcessOutputStep::NeedInput => return Ok(()),
                ProcessOutputStep::StreamChange => {
                    set_output_type(&self.transform, &self.cfg)?;
                }
            }
        }
    }

    /// Async path: queue the input sample, then drain whatever events are
    /// pending without blocking. Returns when no more events are queued —
    /// at that point either we have output (steady state) or we don't yet
    /// (encoder is still filling its pipeline; `encode_yuv420` returns
    /// None and the broadcast loop continues).
    ///
    /// Critically this **does not block**. Earlier versions called
    /// `GetEvent(0)` which serialized encoding at ~11 fps on NVENC
    /// because the encoder emits multiple `NeedInput` events ahead of
    /// `HaveOutput` and blocking-wait on each event throttled the
    /// pipeline.
    fn drive_async(&mut self, sample: IMFSample, is_keyframe: &mut bool) -> Result<(), CodecError> {
        let event_gen = self
            .event_gen
            .as_ref()
            .ok_or_else(|| CodecError::EncodeFailed("async MFT without event gen".into()))?
            .clone();

        self.input_queue.push_back(sample);

        loop {
            let event = unsafe { event_gen.GetEvent(MF_EVENT_FLAG_NO_WAIT) };
            let event = match event {
                Ok(ev) => ev,
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => break,
                Err(e) => return Err(CodecError::EncodeFailed(format!("GetEvent: {e}"))),
            };
            let event_type = unsafe { event.GetType() }
                .map_err(|e| CodecError::EncodeFailed(format!("GetType: {e}")))?
                as i32;

            if event_type == METransformNeedInput.0 {
                if let Some(next) = self.input_queue.pop_front() {
                    unsafe {
                        self.transform
                            .ProcessInput(0, &next, 0)
                            .map_err(|e| CodecError::EncodeFailed(format!("ProcessInput: {e}")))?;
                    }
                }
                // If queue empty, leave the NeedInput unanswered — the
                // next encode call will push a sample and re-poll.
            } else if event_type == METransformHaveOutput.0 {
                match self.process_one_output(is_keyframe)? {
                    ProcessOutputStep::Got | ProcessOutputStep::NeedInput => {}
                    ProcessOutputStep::StreamChange => {
                        set_output_type(&self.transform, &self.cfg)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// One round-trip through ProcessOutput. Used by both sync and async
    /// paths. Returns whether the call extracted a sample, signalled
    /// NEED_MORE_INPUT, or asked for a stream-change re-init.
    fn process_one_output(
        &mut self,
        is_keyframe: &mut bool,
    ) -> Result<ProcessOutputStep, CodecError> {
        let output_sample = if self.output_provides_samples {
            None
        } else {
            Some(unsafe { allocate_output_sample(self.output_sample_size)? })
        };

        let mut data_buffers = [MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: std::mem::ManuallyDrop::new(output_sample),
            dwStatus: 0,
            pEvents: std::mem::ManuallyDrop::new(None),
        }];
        let mut status: u32 = 0;
        let result = unsafe {
            self.transform
                .ProcessOutput(0, &mut data_buffers, &mut status)
        };
        match result {
            Ok(()) => {}
            Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                // Free any output sample we pre-allocated.
                unsafe {
                    std::mem::ManuallyDrop::drop(&mut data_buffers[0].pSample);
                    std::mem::ManuallyDrop::drop(&mut data_buffers[0].pEvents);
                }
                return Ok(ProcessOutputStep::NeedInput);
            }
            Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                unsafe {
                    std::mem::ManuallyDrop::drop(&mut data_buffers[0].pSample);
                    std::mem::ManuallyDrop::drop(&mut data_buffers[0].pEvents);
                }
                return Ok(ProcessOutputStep::StreamChange);
            }
            Err(e) => {
                unsafe {
                    std::mem::ManuallyDrop::drop(&mut data_buffers[0].pSample);
                    std::mem::ManuallyDrop::drop(&mut data_buffers[0].pEvents);
                }
                return Err(CodecError::EncodeFailed(format!("ProcessOutput: {e}")));
            }
        }

        let out_sample = unsafe { std::mem::ManuallyDrop::take(&mut data_buffers[0].pSample) }
            .ok_or_else(|| CodecError::EncodeFailed("ProcessOutput: no sample".into()))?;
        unsafe {
            std::mem::ManuallyDrop::drop(&mut data_buffers[0].pEvents);
        }
        unsafe {
            if out_sample
                .GetUINT32(&MFSampleExtension_CleanPoint)
                .unwrap_or(0)
                != 0
            {
                *is_keyframe = true;
            }
            append_sample_bytes(&out_sample, &mut self.out_buf)?;
        }
        Ok(ProcessOutputStep::Got)
    }
}

enum ProcessOutputStep {
    /// Got a sample; bytes appended to `self.out_buf`.
    Got,
    /// Encoder needs more input — drain loop should stop.
    NeedInput,
    /// Encoder renegotiated output type — caller should re-apply.
    StreamChange,
}

impl Encoder for MftH264 {
    fn kind(&self) -> EncoderKind {
        EncoderKind::MfH264
    }

    fn negotiate(&mut self, _peer: &PeerCaps) -> Negotiated {
        Negotiated {
            profile: Profile::H264UlllFallback,
            color: ColorSpace::Bt709Sdr,
        }
    }

    fn encode(
        &mut self,
        src: &GpuFrame,
        _dirty: &[DamageRect],
    ) -> Result<EncodedSlice, CodecError> {
        // The trait-shaped path isn't used by screen_share (which calls
        // `encode_yuv420` directly), but we keep the impl honest by
        // surfacing a clear error: trait-mode requires a CPU readback the
        // caller hasn't done yet.
        let _ = src;
        Err(CodecError::EncodeFailed(
            "MftH264 trait encode() requires GpuFrame→I420 readback; \
             call encode_yuv420 from the capture pipeline instead"
                .to_string(),
        ))
    }

    fn force_keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    fn set_bitrate(&mut self, kbps: u32) {
        let clamped = kbps.clamp(self.cfg.min_bitrate_kbps, self.cfg.max_bitrate_kbps);
        self.current_kbps = clamped;
        if let Ok(codec_api) = self.transform.cast::<ICodecAPI>() {
            let var = VARIANT::from(clamped * 1000);
            let _ =
                unsafe { codec_api.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &var as *const _) };
        }
    }
}

impl Drop for MftH264 {
    fn drop(&mut self) {
        // Best-effort flush; ignore errors — the next instance can re-init.
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
        }
    }
}

// ── helper functions ───────────────────────────────────────────────────────

/// Build a D3D11 device (with `VIDEO_SUPPORT`) and wrap it in an
/// `IMFDXGIDeviceManager` so hardware MFTs can accept
/// `MFT_MESSAGE_SET_D3D_MANAGER`. Required for NVENC / AMF / QSV
/// activation on most driver/hardware combinations.
fn create_d3d_context() -> Result<D3DContext, CodecError> {
    unsafe {
        let feature_levels = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
        let mut d3d_device: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut d3d_device),
            None,
            None,
        )
        .map_err(|e| CodecError::Init(format!("D3D11CreateDevice (video): {e}")))?;
        let d3d_device = d3d_device
            .ok_or_else(|| CodecError::Init("D3D11CreateDevice returned no device".into()))?;

        // MF requires the device to be in multi-threaded protected mode so
        // its background threads can safely share the immediate context.
        if let Ok(mt) = d3d_device.cast::<ID3D11Multithread>() {
            let _ = mt.SetMultithreadProtected(true);
        }

        let mut reset_token: u32 = 0;
        let mut manager: Option<IMFDXGIDeviceManager> = None;
        MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)
            .map_err(|e| CodecError::Init(format!("MFCreateDXGIDeviceManager: {e}")))?;
        let manager = manager.ok_or_else(|| {
            CodecError::Init("MFCreateDXGIDeviceManager returned no manager".into())
        })?;
        manager
            .ResetDevice(&d3d_device, reset_token)
            .map_err(|e| CodecError::Init(format!("DeviceManager.ResetDevice: {e}")))?;

        Ok(D3DContext {
            _device: d3d_device,
            manager,
        })
    }
}

/// Enumerate H.264 MFTs that accept NV12 input. Iterate every activator,
/// trying ActivateObject → SetD3DManager → configure for each. Return the
/// first that successfully accepts the full pipeline.
///
/// This iteration matters: a hardware MFT may be listed first by
/// `MFT_ENUM_FLAG_SORTANDFILTER` but fail to activate (missing driver
/// support, GPU contention, etc.). Falling through to the next candidate
/// — including the in-box Microsoft software MFT — keeps the daemon
/// running on hardware MF fails rather than disappearing back to openh264.
fn pick_encoder(
    cfg: &SharedConfig,
    d3d_manager: &IMFDXGIDeviceManager,
) -> Result<(IMFTransform, String, bool), CodecError> {
    let input_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };
    let output_info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };

    let mut activators: *mut Option<IMFActivate> = ptr::null_mut();
    let mut count: u32 = 0;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&input_info),
            Some(&output_info),
            &mut activators,
            &mut count,
        )
        .map_err(|e| CodecError::NotAvailable(format!("MFTEnumEx: {e}")))?;
    }

    if count == 0 || activators.is_null() {
        return Err(CodecError::NotAvailable(
            "no Media Foundation H.264 encoder accepts NV12 input on this host".into(),
        ));
    }

    // Take ownership so CoTaskMemFree runs even on early returns.
    let activators_vec: Vec<Option<IMFActivate>> = unsafe {
        let slice = std::slice::from_raw_parts(activators, count as usize);
        let v = slice.to_vec();
        CoTaskMemFree(Some(activators as _));
        v
    };

    let manager_ptr = d3d_manager.as_raw() as usize;
    let mut last_err: Option<String> = None;
    // Some vendors (AMD) register the same MFT twice — once per surface
    // type. Both fail with the same error in our setup, so skip duplicates
    // after the first failure to cut the noise.
    let mut tried_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for activator_opt in activators_vec {
        let Some(activator) = activator_opt else {
            continue;
        };
        let name = activate_friendly_name(&activator);
        if !tried_names.insert(name.clone()) {
            continue;
        }

        // Pre-unlock async MFTs. AMD's H.264 encoder fails ActivateObject
        // outright without this set on the activator; setting it has no
        // effect on sync MFTs so it's safe to do unconditionally.
        let _ = unsafe { activator.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1) };

        let transform: IMFTransform = match unsafe { activator.ActivateObject() } {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(encoder = %name, err = %e, "MF activator ActivateObject failed; trying next");
                last_err = Some(format!("[{name}] ActivateObject: {e}"));
                continue;
            }
        };

        // Hand the D3D manager to the transform. Software MFTs reject this
        // — that's fine, we ignore the result.
        let _ = unsafe { transform.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager_ptr) };

        // Async MFTs also need the unlock attribute set on the transform
        // itself before any SetXxxType call, otherwise SetInputType returns
        // MF_E_TRANSFORM_ASYNC_LOCKED (0xC00D6D77).
        let is_async = unsafe {
            match transform.GetAttributes() {
                Ok(attrs) => {
                    let async_flag = attrs.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0;
                    if async_flag {
                        let _ = attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1);
                    }
                    async_flag
                }
                Err(_) => false,
            }
        };

        if let Err(e) = configure_transform(&transform, cfg) {
            tracing::warn!(encoder = %name, err = %e, "MF activator configure failed; trying next");
            last_err = Some(format!("[{name}] configure: {e}"));
            continue;
        }

        return Ok((transform, name, is_async));
    }

    Err(CodecError::NotAvailable(format!(
        "no MF activator could be initialized — last error: {}",
        last_err.unwrap_or_else(|| "(none)".to_string())
    )))
}

/// Set output H.264 type, then input NV12 type. Order matters — MFT
/// encoders reject input types until an output type is established.
fn configure_transform(transform: &IMFTransform, cfg: &SharedConfig) -> Result<(), CodecError> {
    set_output_type(transform, cfg)?;
    set_input_type(transform, cfg)?;

    // Latency-tuned encoder knobs. Most are best-effort — sync MFTs ignore
    // CODECAPI_*Speed flags; hardware MFTs need them to skip lookahead and
    // multi-pass. Without these, NVENC defaults to ~80 ms/frame.
    if let Ok(codec_api) = transform.cast::<ICodecAPI>() {
        unsafe {
            let v = VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32);
            let _ = codec_api.SetValue(&CODECAPI_AVEncCommonRateControlMode, &v as *const _);
            let v = VARIANT::from(0u32);
            let _ = codec_api.SetValue(&CODECAPI_AVEncMPVDefaultBPictureCount, &v as *const _);
            let v = VARIANT::from(cfg.initial_bitrate_kbps * 1000);
            let _ = codec_api.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &v as *const _);
            // Single-pass, no look-ahead. Required for hardware encoders to
            // produce one output per input on the same frame.
            let v = VARIANT::from(true);
            let _ = codec_api.SetValue(&CODECAPI_AVEncCommonLowLatency, &v as *const _);
            // Real-time hint — encoder favors speed and CPU/GPU yields.
            let v = VARIANT::from(true);
            let _ = codec_api.SetValue(&CODECAPI_AVEncCommonRealTime, &v as *const _);
            // 0 = max speed, 100 = max quality. NVENC's "P1" preset.
            let v = VARIANT::from(0u32);
            let _ = codec_api.SetValue(&CODECAPI_AVEncCommonQualityVsSpeed, &v as *const _);
        }
    }
    Ok(())
}

fn set_output_type(transform: &IMFTransform, cfg: &SharedConfig) -> Result<(), CodecError> {
    unsafe {
        // IMFMediaType derefs to IMFAttributes, so SetGUID/SetUINT32/SetUINT64
        // are reachable directly via auto-deref.
        let media_type =
            MFCreateMediaType().map_err(|e| CodecError::Init(format!("MFCreateMediaType: {e}")))?;
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|e| CodecError::Init(format!("set major type: {e}")))?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
            .map_err(|e| CodecError::Init(format!("set H264 subtype: {e}")))?;
        media_type
            .SetUINT32(&MF_MT_AVG_BITRATE, cfg.initial_bitrate_kbps * 1000)
            .map_err(|e| CodecError::Init(format!("set bitrate: {e}")))?;
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|e| CodecError::Init(format!("set interlace: {e}")))?;
        media_type
            .SetUINT64(&MF_MT_FRAME_SIZE, pack_u64(cfg.width, cfg.height))
            .map_err(|e| CodecError::Init(format!("set frame size: {e}")))?;
        media_type
            .SetUINT64(&MF_MT_FRAME_RATE, pack_u64(cfg.fps_num, cfg.fps_den))
            .map_err(|e| CodecError::Init(format!("set frame rate: {e}")))?;
        media_type
            .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_u64(1, 1))
            .map_err(|e| CodecError::Init(format!("set par: {e}")))?;
        media_type
            .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)
            .map_err(|e| CodecError::Init(format!("set H264 profile: {e}")))?;

        transform
            .SetOutputType(0, &media_type, 0)
            .map_err(|e| CodecError::Init(format!("SetOutputType: {e}")))?;
    }
    Ok(())
}

fn set_input_type(transform: &IMFTransform, cfg: &SharedConfig) -> Result<(), CodecError> {
    unsafe {
        let media_type =
            MFCreateMediaType().map_err(|e| CodecError::Init(format!("MFCreateMediaType: {e}")))?;
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|e| CodecError::Init(format!("set major type: {e}")))?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
            .map_err(|e| CodecError::Init(format!("set NV12 subtype: {e}")))?;
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(|e| CodecError::Init(format!("set interlace: {e}")))?;
        media_type
            .SetUINT64(&MF_MT_FRAME_SIZE, pack_u64(cfg.width, cfg.height))
            .map_err(|e| CodecError::Init(format!("set frame size: {e}")))?;
        media_type
            .SetUINT64(&MF_MT_FRAME_RATE, pack_u64(cfg.fps_num, cfg.fps_den))
            .map_err(|e| CodecError::Init(format!("set frame rate: {e}")))?;
        media_type
            .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_u64(1, 1))
            .map_err(|e| CodecError::Init(format!("set par: {e}")))?;

        transform
            .SetInputType(0, &media_type, 0)
            .map_err(|e| CodecError::Init(format!("SetInputType: {e}")))?;
    }
    Ok(())
}

/// Build an IMFSample wrapping `bytes`. Sets pts/duration in 100-ns ticks.
unsafe fn create_input_sample(
    bytes: &[u8],
    pts_us: u64,
    fps: u32,
) -> Result<IMFSample, CodecError> {
    let buffer = MFCreateMemoryBuffer(bytes.len() as u32)
        .map_err(|e| CodecError::EncodeFailed(format!("MFCreateMemoryBuffer: {e}")))?;

    let mut data: *mut u8 = ptr::null_mut();
    let mut max_len: u32 = 0;
    buffer
        .Lock(&mut data, Some(&mut max_len), None)
        .map_err(|e| CodecError::EncodeFailed(format!("buffer.Lock: {e}")))?;
    ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
    buffer
        .Unlock()
        .map_err(|e| CodecError::EncodeFailed(format!("buffer.Unlock: {e}")))?;
    buffer
        .SetCurrentLength(bytes.len() as u32)
        .map_err(|e| CodecError::EncodeFailed(format!("SetCurrentLength: {e}")))?;

    let sample =
        MFCreateSample().map_err(|e| CodecError::EncodeFailed(format!("MFCreateSample: {e}")))?;
    sample
        .AddBuffer(&buffer)
        .map_err(|e| CodecError::EncodeFailed(format!("AddBuffer: {e}")))?;

    let pts_100ns = (pts_us as i64) * 10;
    sample
        .SetSampleTime(pts_100ns)
        .map_err(|e| CodecError::EncodeFailed(format!("SetSampleTime: {e}")))?;
    let duration_100ns = if fps > 0 {
        10_000_000i64 / fps as i64
    } else {
        0
    };
    sample
        .SetSampleDuration(duration_100ns)
        .map_err(|e| CodecError::EncodeFailed(format!("SetSampleDuration: {e}")))?;
    Ok(sample)
}

unsafe fn allocate_output_sample(size: u32) -> Result<IMFSample, CodecError> {
    let buffer = MFCreateMemoryBuffer(size.max(1))
        .map_err(|e| CodecError::EncodeFailed(format!("output buffer: {e}")))?;
    let sample =
        MFCreateSample().map_err(|e| CodecError::EncodeFailed(format!("output sample: {e}")))?;
    sample
        .AddBuffer(&buffer)
        .map_err(|e| CodecError::EncodeFailed(format!("output AddBuffer: {e}")))?;
    Ok(sample)
}

/// Read all bytes out of an IMFSample (which may contain multiple buffers
/// strung together) and append them to `out`.
unsafe fn append_sample_bytes(sample: &IMFSample, out: &mut Vec<u8>) -> Result<(), CodecError> {
    let buffer = sample
        .ConvertToContiguousBuffer()
        .map_err(|e| CodecError::EncodeFailed(format!("ConvertToContiguousBuffer: {e}")))?;
    let mut data: *mut u8 = ptr::null_mut();
    let mut max_len: u32 = 0;
    let mut current_len: u32 = 0;
    buffer
        .Lock(&mut data, Some(&mut max_len), Some(&mut current_len))
        .map_err(|e| CodecError::EncodeFailed(format!("out buffer Lock: {e}")))?;
    let slice = std::slice::from_raw_parts(data, current_len as usize);
    out.extend_from_slice(slice);
    buffer
        .Unlock()
        .map_err(|e| CodecError::EncodeFailed(format!("out buffer Unlock: {e}")))?;
    Ok(())
}

// SAFETY: MftH264 confines the IMFTransform to a single owner — the
// screen-share encoder thread. COM objects can move between threads as
// long as only one thread accesses them at a time, which is what `&mut
// self` on every Encoder trait method enforces. We intentionally do NOT
// implement Sync.
unsafe impl Send for MftH264 {}
