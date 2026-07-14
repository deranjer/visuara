//! Wraps `openh264` to turn captured RGBA frames into an H.264 NAL bitstream
//! for the WebRTC video track. H.264 (rather than VP8/VP9) was chosen after
//! discovering `vpx-encode` requires a pre-installed system libvpx
//! discoverable via pkg-config on Windows; `openh264` builds its vendored C
//! source directly via `cc`, with no extra system packages needed on either
//! target platform.

use anyhow::{Context, Result};
use image::RgbaImage;
use openh264::encoder::Encoder;
use openh264::formats::{RgbaSliceU8, YUVBuffer};

pub struct VideoEncoder {
    encoder: Encoder,
}

impl VideoEncoder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            encoder: Encoder::new().context("initialize H.264 encoder")?,
        })
    }

    /// Encodes one RGBA frame, returning the H.264 NAL bytes for that frame.
    pub fn encode_frame(&mut self, frame: &RgbaImage) -> Result<Vec<u8>> {
        let (width, height) = frame.dimensions();
        let rgba_source = RgbaSliceU8::new(frame.as_raw(), (width as usize, height as usize));
        let yuv = YUVBuffer::from_rgb_source(rgba_source);
        let bitstream = self.encoder.encode(&yuv).context("encode frame")?;
        Ok(bitstream.to_vec())
    }
}
