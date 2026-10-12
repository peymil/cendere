use crate::options::{Alpha, Lossless, Options, PaletteMode, Palette, ResizeFilter};
use crate::png_io::{self, Rgba8};
use anyhow::{bail, Context, Result};
use fast_image_resize as fir;
use fir::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use std::num::NonZeroU8;

pub struct Output {
    pub bytes: Vec<u8>,
    pub resized: bool,
    pub notes: Vec<String>,
}

/// Runs the fixed pipeline: lossless -> alpha -> resize -> palette -> final lossless.
pub fn run(input: &[u8], opts: &Options) -> Result<Output> {
    opts.validate()?;
    let info = png_io::read_info(input)?;
    if png_io::decoded_bytes(info.width, info.height) > png_io::MAX_DECODED_BYTES {
        bail!("Image is larger than 1 GB when decoded");
    }
    if info.is_apng {
        bail!("Animated PNG (APNG) is not supported");
    }

    let mut notes = Vec::new();
    let data = if opts.lossless.enabled {
        lossless(input, &opts.lossless)?
    } else {
        input.to_vec()
    };

    let (target_w, target_h) = opts.target_size(info.width, info.height);
    let resized = (target_w, target_h) != (info.width, info.height);
    let pixel_work = opts.alpha.enabled || resized || opts.palette.enabled;
    if !pixel_work {
        return Ok(Output { bytes: data, resized: false, notes });
    }

    let mut img = png_io::decode_rgba(&data)?;
    if opts.alpha.enabled {
        alpha_cleanup(&mut img.data, &opts.alpha);
    }
    if resized {
        img = resize(img, target_w, target_h, opts.resize.filter)?;
    }

    let encoded = if opts.palette.enabled {
        match quantize(&img, &opts.palette)? {
            Quantized::Indexed { palette, indices } => {
                png_io::encode_indexed(img.width, img.height, &indices, &palette)?
            }
            Quantized::Skipped(reason) => {
                notes.push(reason);
                png_io::encode_rgba(&img)?
            }
        }
    } else {
        png_io::encode_rgba(&img)?
    };

    let bytes = lossless(&encoded, &opts.lossless)?;
    Ok(Output { bytes, resized, notes })
}

pub fn lossless(bytes: &[u8], o: &Lossless) -> Result<Vec<u8>> {
    let mut opts = oxipng::Options::from_preset(o.level);
    opts.strip = if o.strip_metadata {
        oxipng::StripChunks::Safe
    } else {
        oxipng::StripChunks::None
    };
    opts.interlace = if o.remove_interlace {
        Some(oxipng::Interlacing::None)
    } else {
        None
    };
    if o.zopfli {
        opts.deflate = oxipng::Deflaters::Zopfli {
            iterations: NonZeroU8::new(15).expect("non-zero"),
        };
    }
    oxipng::optimize_from_memory(bytes, &opts).context("Lossless optimization failed")
}

/// Sets alpha <= low to fully transparent (RGB zeroed) and alpha >= high to opaque.
pub fn alpha_cleanup(data: &mut [u8], o: &Alpha) {
    for px in data.chunks_exact_mut(4) {
        if px[3] <= o.low {
            px.copy_from_slice(&[0, 0, 0, 0]);
        } else if px[3] >= o.high {
            px[3] = 255;
        }
    }
}

pub fn resize(img: Rgba8, width: u32, height: u32, filter: ResizeFilter) -> Result<Rgba8> {
    let src = fir::images::Image::from_vec_u8(img.width, img.height, img.data, PixelType::U8x4)?;
    let mut dst = fir::images::Image::new(width, height, PixelType::U8x4);
    let alg = match filter {
        ResizeFilter::Lanczos3 => ResizeAlg::Convolution(FilterType::Lanczos3),
        ResizeFilter::Nearest => ResizeAlg::Nearest,
    };
    Resizer::new().resize(&src, &mut dst, &ResizeOptions::new().resize_alg(alg))?;
    Ok(Rgba8 { width, height, data: dst.into_vec() })
}

pub enum Quantized {
    Indexed { palette: Vec<[u8; 4]>, indices: Vec<u8> },
    Skipped(String),
}

pub fn quantize(img: &Rgba8, o: &Palette) -> Result<Quantized> {
    let mut attr = imagequant::new();
    match o.mode {
        PaletteMode::Auto => attr.set_quality(50, 85)?,
        PaletteMode::Fixed => {
            attr.set_quality(0, 100)?;
            attr.set_max_colors(o.colors)?;
        }
    }
    let pixels: Vec<imagequant::RGBA> = img
        .data
        .chunks_exact(4)
        .map(|p| imagequant::RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();
    let mut image = attr.new_image(pixels, img.width as usize, img.height as usize, 0.0)?;
    let mut result = match attr.quantize(&mut image) {
        Ok(r) => r,
        Err(imagequant::Error::QualityTooLow) => {
            return Ok(Quantized::Skipped(
                "Palette skipped: quality below 50, original colors kept".into(),
            ))
        }
        Err(e) => return Err(e.into()),
    };
    result.set_dithering_level(if o.dithering { 1.0 } else { 0.0 })?;
    let (palette, indices) = result.remapped(&mut image)?;
    Ok(Quantized::Indexed {
        palette: palette.iter().map(|c| [c.r, c.g, c.b, c.a]).collect(),
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{Options, PaletteMode, ResizeMode};

    fn gradient(w: u32, h: u32) -> Rgba8 {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[
                    (x * 255 / w.max(1)) as u8,
                    (y * 255 / h.max(1)) as u8,
                    ((x + y) * 3 % 256) as u8,
                    255,
                ]);
            }
        }
        Rgba8 { width: w, height: h, data }
    }

    fn png_of(img: &Rgba8) -> Vec<u8> {
        png_io::encode_rgba(img).unwrap()
    }

    #[test]
    fn lossless_keeps_pixels_identical() {
        let img = gradient(37, 23);
        let src = png_of(&img);
        let out = lossless(&src, &Lossless::default()).unwrap();
        assert!(out.len() <= src.len());
        assert_eq!(png_io::decode_rgba(&out).unwrap().data, img.data);
    }

    #[test]
    fn alpha_cleanup_thresholds() {
        let mut data = vec![10, 20, 30, 0, 10, 20, 30, 8, 10, 20, 30, 9, 10, 20, 30, 247, 10, 20, 30, 248];
        let o = Alpha { enabled: true, low: 8, high: 247 };
        alpha_cleanup(&mut data, &o);
        assert_eq!(&data[0..4], &[0, 0, 0, 0]);
        assert_eq!(&data[4..8], &[0, 0, 0, 0]);
        assert_eq!(&data[8..12], &[10, 20, 30, 9]);
        assert_eq!(&data[12..16], &[10, 20, 30, 255]);
        assert_eq!(&data[16..20], &[10, 20, 30, 255]);
    }

    #[test]
    fn resize_scale_sizes() {
        let mut o = Options::default();
        o.resize.enabled = true;
        o.resize.mode = ResizeMode::Scale;
        for (scale, expected) in [(1u8, (25, 13)), (2, (50, 25)), (3, (75, 38)), (4, (100, 50))] {
            o.resize.scale = scale;
            let (w, h) = o.target_size(100, 50);
            assert_eq!((w, h), (expected.0, expected.1), "scale {scale}");
            let out = resize(gradient(100, 50), w, h, ResizeFilter::Lanczos3).unwrap();
            assert_eq!((out.width, out.height), (w, h));
            assert_eq!(out.data.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn resize_custom_keep_aspect_and_exact() {
        let mut o = Options::default();
        o.resize.enabled = true;
        o.resize.mode = ResizeMode::Custom;
        o.resize.width = 50;
        o.resize.height = 50;
        o.resize.keep_aspect = true;
        assert_eq!(o.target_size(200, 100), (50, 25));
        o.resize.keep_aspect = false;
        assert_eq!(o.target_size(200, 100), (50, 50));
    }

    #[test]
    fn fixed_palette_limits_colors() {
        let img = gradient(64, 64);
        let mut o = Palette { enabled: true, mode: PaletteMode::Fixed, colors: 8, dithering: false };
        match quantize(&img, &o).unwrap() {
            Quantized::Indexed { palette, indices } => {
                assert!(palette.len() <= 8);
                assert_eq!(indices.len(), 64 * 64);
            }
            Quantized::Skipped(r) => panic!("unexpected skip: {r}"),
        }
        o.colors = 256;
        assert!(matches!(quantize(&img, &o).unwrap(), Quantized::Indexed { .. }));
    }

    #[test]
    fn full_pipeline_reduces_palette_and_size() {
        let src = png_of(&gradient(128, 128));
        let mut o = Options::default();
        o.palette.mode = PaletteMode::Fixed;
        o.palette.colors = 16;
        let out = run(&src, &o).unwrap();
        assert!(!out.resized);
        assert!(out.bytes.len() < src.len());
        let info = png_io::read_info(&out.bytes).unwrap();
        assert_eq!(info.color_type, 3);
        assert!(info.palette_entries.unwrap() <= 16);
    }

    #[test]
    fn pipeline_resize_changes_dimensions() {
        let src = png_of(&gradient(80, 40));
        let mut o = Options::default();
        o.resize.enabled = true;
        o.resize.scale = 1;
        o.palette.enabled = false;
        let out = run(&src, &o).unwrap();
        assert!(out.resized);
        let info = png_io::read_info(&out.bytes).unwrap();
        assert_eq!((info.width, info.height), (20, 10));
    }

    #[test]
    fn options_validation() {
        let mut o = Options::default();
        o.palette.mode = PaletteMode::Fixed;
        o.palette.colors = 1;
        assert!(o.validate().is_err());
        o.palette.colors = 256;
        assert!(o.validate().is_ok());
        o.alpha.enabled = true;
        o.alpha.low = 250;
        assert!(o.validate().is_err());
    }
}
