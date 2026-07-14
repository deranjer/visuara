//! Decodes an incoming H.264 NAL bitstream (from the WebRTC video track)
//! back into RGBA frames for rendering.

use anyhow::{Context, Result};
use image::RgbaImage;
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

pub struct VideoDecoder {
    decoder: Decoder,
}

impl VideoDecoder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            decoder: Decoder::new().context("initialize H.264 decoder")?,
        })
    }

    /// Feeds one packet of H.264 data in; returns a decoded RGBA frame if the
    /// decoder had enough data to produce one.
    pub fn decode(&mut self, packet: &[u8]) -> Result<Option<RgbaImage>> {
        let Some(yuv) = self.decoder.decode(packet).context("decode H.264 packet")? else {
            return Ok(None);
        };
        let (width, height) = yuv.dimensions();
        let mut rgba = vec![0u8; yuv.rgba8_len()];
        yuv.write_rgba8(&mut rgba);
        let image = RgbaImage::from_raw(width as u32, height as u32, rgba)
            .context("assemble decoded RGBA image")?;
        Ok(Some(image))
    }
}
