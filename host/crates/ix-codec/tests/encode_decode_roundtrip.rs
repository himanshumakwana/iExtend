//! Roundtrip the OpenH264 software encoder + decoder.
//!
//! Ensures Plan A's encoder produces a valid, decodable H.264 bitstream
//! when fed YUV420P from the DXGI capture path. PSNR threshold of 35 dB
//! is conservative — real screen content usually clears 40+ dB at a
//! 5 Mbps bitrate.

#![cfg(feature = "sw-only")]

use ix_codec::{
    trait_::{ColorSpace, Profile},
    x264_sw::X264Sw,
    SharedConfig,
};
use openh264::decoder::{Decoder, DecoderConfig};
use openh264::formats::YUVSource;
use openh264::OpenH264API;

/// Build a synthetic 320x240 YUV420P frame with a smooth horizontal
/// gradient on Y. Constant chroma. Predictable input so PSNR is easy to
/// reason about and small enough to keep the test fast.
fn make_test_frame(width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut buf = vec![0u8; y_size + 2 * uv_size];

    // Y: horizontal gradient 16 → 235 (limited range).
    for row in 0..height {
        for col in 0..width {
            buf[row * width + col] = (16 + (219 * col / width) as u8).min(235);
        }
    }
    // U + V: constant 128 (greyscale).
    for px in &mut buf[y_size..] {
        *px = 128;
    }
    buf
}

/// Compute peak signal-to-noise ratio between two YUV420P frames. We
/// only compare Y planes — chroma is constant in our test frame.
fn psnr_y(orig: &[u8], dec_y: &[u8]) -> f64 {
    assert_eq!(orig.len(), dec_y.len());
    let mse: f64 = orig
        .iter()
        .zip(dec_y.iter())
        .map(|(a, b)| {
            let d = *a as f64 - *b as f64;
            d * d
        })
        .sum::<f64>()
        / orig.len() as f64;
    if mse == 0.0 {
        return 99.0;
    }
    10.0 * (255.0_f64.powi(2) / mse).log10()
}

#[test]
fn openh264_encode_decode_roundtrip_psnr_above_35db() {
    let width = 320usize;
    let height = 240usize;

    let cfg = SharedConfig {
        width: width as u32,
        height: height as u32,
        fps_num: 30,
        fps_den: 1,
        initial_bitrate_kbps: 5_000,
        min_bitrate_kbps: 1_000,
        max_bitrate_kbps: 10_000,
        profile: Profile::H264UlllFallback,
        color: ColorSpace::Bt709Sdr,
        intra_refresh_rows: 4,
    };

    let mut enc = X264Sw::new(cfg).expect("encoder init");
    let yuv_in = make_test_frame(width, height);

    // First frame is forced IDR — encoder needs an IDR to bootstrap a
    // decode session, so this is the right starting point.
    let slice = enc
        .encode_yuv420(yuv_in.clone(), width as u32, height as u32, 0)
        .expect("encode");
    assert!(!slice.data.is_empty(), "encoder produced no bytes");

    // Hex-dump the first 32 bytes so a regression in encoder behavior
    // (e.g. losing the SPS/PPS prefix) shows up as a diff in test output.
    let preview: String = slice
        .data
        .iter()
        .take(32)
        .map(|b| format!("{b:02x}"))
        .collect();
    eprintln!(
        "encoded preview ({} bytes total): {preview}",
        slice.data.len()
    );

    let mut dec = Decoder::with_api_config(OpenH264API::from_source(), DecoderConfig::new())
        .expect("decoder init");
    let decoded = dec
        .decode(&slice.data)
        .expect("decode")
        .expect("decoder returned None — the encoded stream had no IDR");

    let (dw, dh) = decoded.dimensions();
    assert_eq!((dw, dh), (width, height), "decoded dimensions mismatch");

    // Compare Y planes. DecodedYUV exposes y/u/v as &[u8] with strides;
    // copy-into-Vec to handle stride normalization.
    let y_stride = decoded.strides().0;
    let mut y_packed = Vec::with_capacity(width * height);
    let y_src = decoded.y();
    for row in 0..height {
        y_packed.extend_from_slice(&y_src[row * y_stride..row * y_stride + width]);
    }

    let psnr = psnr_y(&yuv_in[..width * height], &y_packed);
    eprintln!("Y-plane PSNR: {psnr:.2} dB");
    assert!(
        psnr > 35.0,
        "PSNR below 35 dB ({psnr:.2}) — encoder/decoder roundtrip lost too much fidelity"
    );
}
