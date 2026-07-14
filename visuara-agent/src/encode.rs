//! Wraps `openh264` to turn captured RGBA frames into an H.264 NAL bitstream
//! for the WebRTC video track. H.264 (rather than VP8/VP9) was chosen after
//! discovering `vpx-encode` requires a pre-installed system libvpx
//! discoverable via pkg-config on Windows; `openh264` builds its vendored C
//! source directly via `cc`, with no extra system packages needed on either
//! target platform.

use anyhow::{Context, Result};
use image::RgbaImage;
use openh264::OpenH264API;
use openh264::encoder::{Encoder, EncoderConfig, IntraFramePeriod};
use openh264::formats::{RgbaSliceU8, YUVBuffer};

pub struct VideoEncoder {
    encoder: Encoder,
}

impl VideoEncoder {
    pub fn new() -> Result<Self> {
        // Without a periodic keyframe, the encoder only ever emits SPS/PPS on
        // the very first frame; if those initial packets are lost (e.g. a
        // race with ICE/DTLS still stabilizing right as the video track
        // starts), the decoder has no parameter sets and every subsequent
        // frame fails to decode for the rest of the session ("OpenH264
        // encountered an error. Native:16" = dsNoParamSets). A ~3s GOP at the
        // capture loop's 10fps lets the stream self-heal within a few
        // seconds instead of staying broken forever.
        let config = EncoderConfig::new().intra_frame_period(IntraFramePeriod::from_num_frames(30));
        Ok(Self {
            encoder: Encoder::with_api_config(OpenH264API::from_source(), config)
                .context("initialize H.264 encoder")?,
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
