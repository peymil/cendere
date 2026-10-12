use crate::{pipeline, png_io};
use anyhow::Result;
use base64::Engine;
use std::fs;
use std::path::Path;

fn data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// Downscaled PNG preview that fits inside `max` x `max`, as a data URL.
pub fn thumbnail(path: &Path, max: u32) -> Result<String> {
    let bytes = fs::read(path)?;
    let info = png_io::read_info(&bytes)?;
    let img = png_io::decode_rgba(&bytes)?;
    let f = (max as f64 / info.width as f64).min(max as f64 / info.height as f64);
    if f >= 1.0 {
        return Ok(data_url(&png_io::encode_rgba(&img)?));
    }
    let w = ((info.width as f64 * f).round() as u32).max(1);
    let h = ((info.height as f64 * f).round() as u32).max(1);
    let small = pipeline::resize(img, w, h, crate::options::ResizeFilter::Lanczos3)?;
    Ok(data_url(&png_io::encode_rgba(&small)?))
}

/// Full image as a data URL (used by the compare view).
pub fn image_data_url(path: &Path) -> Result<String> {
    Ok(data_url(&fs::read(path)?))
}
