use crate::png_io::{self, PngInfo};
use anyhow::Result;
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorCount {
    pub hex: String,
    pub count: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub path: String,
    pub file_size: u64,
    pub info: PngInfo,
    pub bits_per_pixel: f64,
    pub raw_ratio: f64,
    pub unique_colors: usize,
    pub opaque_pixels: u64,
    pub semi_pixels: u64,
    pub transparent_pixels: u64,
    pub alpha_histogram: Vec<u64>,
    pub red_histogram: Vec<u64>,
    pub green_histogram: Vec<u64>,
    pub blue_histogram: Vec<u64>,
    pub top_colors: Vec<ColorCount>,
}

const TOP_COLORS: usize = 12;
const ALPHA_BINS: usize = 16;

pub fn analyze(path: &Path) -> Result<Analysis> {
    let bytes = fs::read(path)?;
    let info = png_io::read_info(&bytes)?;
    let img = png_io::decode_rgba(&bytes)?;

    let mut counts: FxHashMap<u32, u64> = FxHashMap::default();
    let mut alpha_histogram = vec![0u64; ALPHA_BINS];
    let (mut r_h, mut g_h, mut b_h) = (vec![0u64; 256], vec![0u64; 256], vec![0u64; 256]);
    let (mut opaque, mut semi, mut transparent) = (0u64, 0u64, 0u64);

    for px in img.data.chunks_exact(4) {
        let key = u32::from_be_bytes([px[0], px[1], px[2], px[3]]);
        *counts.entry(key).or_insert(0) += 1;
        r_h[px[0] as usize] += 1;
        g_h[px[1] as usize] += 1;
        b_h[px[2] as usize] += 1;
        let a = px[3];
        alpha_histogram[(a as usize * ALPHA_BINS) / 256] += 1;
        match a {
            255 => opaque += 1,
            0 => transparent += 1,
            _ => semi += 1,
        }
    }

    let mut ranked: Vec<(u32, u64)> = counts.iter().map(|(&k, &v)| (k, v)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let top_colors = ranked
        .iter()
        .take(TOP_COLORS)
        .map(|&(k, count)| ColorCount {
            hex: format!("#{:08X}", k),
            count,
        })
        .collect();

    let pixels = img.width as f64 * img.height as f64;
    let file_size = bytes.len() as u64;
    Ok(Analysis {
        path: path.to_string_lossy().into_owned(),
        file_size,
        info,
        bits_per_pixel: file_size as f64 * 8.0 / pixels,
        raw_ratio: file_size as f64 / (pixels * 4.0),
        unique_colors: counts.len(),
        opaque_pixels: opaque,
        semi_pixels: semi,
        transparent_pixels: transparent,
        alpha_histogram,
        red_histogram: r_h,
        green_histogram: g_h,
        blue_histogram: b_h,
        top_colors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png_io::{self, Rgba8};

    #[test]
    fn counts_colors_and_alpha() {
        let data = vec![
            255, 0, 0, 255, //
            255, 0, 0, 255, //
            0, 255, 0, 128, //
            9, 9, 9, 0,
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.png");
        std::fs::write(&path, png_io::encode_rgba(&Rgba8 { width: 2, height: 2, data }).unwrap()).unwrap();
        let a = analyze(&path).unwrap();
        assert_eq!(a.unique_colors, 3);
        assert_eq!((a.opaque_pixels, a.semi_pixels, a.transparent_pixels), (2, 1, 1));
        assert_eq!(a.top_colors[0].count, 2);
        assert_eq!(a.top_colors[0].hex, "#FF0000FF");
        assert_eq!(a.red_histogram[255], 2);
    }
}
