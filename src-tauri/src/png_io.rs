use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::io::Cursor;

pub const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
pub const MAX_DECODED_BYTES: u64 = 1 << 30;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    pub color_type_name: &'static str,
    pub interlaced: bool,
    pub is_apng: bool,
    pub has_icc: bool,
    pub has_trns: bool,
    pub palette_entries: Option<usize>,
    pub chunks: Vec<String>,
}

pub fn color_type_name(code: u8) -> &'static str {
    match code {
        0 => "Grayscale",
        2 => "RGB",
        3 => "Indexed",
        4 => "Gray + Alpha",
        6 => "RGBA",
        _ => "Unknown",
    }
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

pub fn read_info(bytes: &[u8]) -> Result<PngInfo> {
    if bytes.len() < 8 || bytes[..8] != SIGNATURE {
        bail!("Not a valid PNG file");
    }
    let mut pos = 8usize;
    let mut header: Option<(u32, u32, u8, u8, bool)> = None;
    let mut palette_entries = None;
    let mut chunks: Vec<String> = Vec::new();
    let mut is_apng = false;
    let mut has_icc = false;
    let mut has_trns = false;

    while pos + 12 <= bytes.len() {
        let len = be32(&bytes[pos..pos + 4]) as usize;
        let kind = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(len)
            .context("Invalid PNG chunk length")?;
        if data_end + 4 > bytes.len() {
            bail!("Truncated PNG: chunk {} is incomplete", String::from_utf8_lossy(kind));
        }
        let data = &bytes[data_start..data_end];
        let name = String::from_utf8_lossy(kind).into_owned();
        if !chunks.contains(&name) {
            chunks.push(name);
        }
        match kind {
            b"IHDR" => {
                if len != 13 {
                    bail!("Invalid IHDR chunk");
                }
                header = Some((be32(&data[0..4]), be32(&data[4..8]), data[8], data[9], data[12] != 0));
            }
            b"PLTE" => palette_entries = Some(len / 3),
            b"tRNS" => has_trns = true,
            b"iCCP" => has_icc = true,
            b"acTL" => is_apng = true,
            b"IEND" => break,
            _ => {}
        }
        pos = data_end + 4;
    }

    let (width, height, bit_depth, color_type, interlaced) =
        header.context("PNG has no IHDR chunk")?;
    if width == 0 || height == 0 {
        bail!("PNG has zero width or height");
    }
    Ok(PngInfo {
        width,
        height,
        bit_depth,
        color_type,
        color_type_name: color_type_name(color_type),
        interlaced,
        is_apng,
        has_icc,
        has_trns,
        palette_entries,
        chunks,
    })
}

pub fn decoded_bytes(width: u32, height: u32) -> u64 {
    width as u64 * height as u64 * 4
}

#[derive(Clone, Debug)]
pub struct Rgba8 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub fn decode_rgba(bytes: &[u8]) -> Result<Rgba8> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().context("Cannot read PNG")?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).context("Cannot decode PNG")?;
    buf.truncate(frame.buffer_size());
    let data = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => bail!("Unexpected indexed output after expansion"),
    };
    Ok(Rgba8 { width: frame.width, height: frame.height, data })
}

pub fn encode_rgba(img: &Rgba8) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(&img.data)?;
    writer.finish()?;
    Ok(out)
}

pub fn encode_indexed(width: u32, height: u32, indices: &[u8], palette: &[[u8; 4]]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Eight);
    let rgb: Vec<u8> = palette.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
    enc.set_palette(rgb);
    if let Some(last) = palette.iter().rposition(|c| c[3] != 255) {
        let trns: Vec<u8> = palette[..=last].iter().map(|c| c[3]).collect();
        enc.set_trns(trns);
    }
    let mut writer = enc.write_header()?;
    writer.write_image_data(indices)?;
    writer.finish()?;
    Ok(out)
}
