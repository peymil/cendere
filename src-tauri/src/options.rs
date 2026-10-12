use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub lossless: Lossless,
    pub alpha: Alpha,
    pub resize: Resize,
    pub palette: Palette,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Lossless {
    pub enabled: bool,
    pub level: u8,
    pub strip_metadata: bool,
    pub remove_interlace: bool,
    pub zopfli: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Alpha {
    pub enabled: bool,
    pub low: u8,
    pub high: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Resize {
    pub enabled: bool,
    pub mode: ResizeMode,
    pub scale: u8,
    pub width: u32,
    pub height: u32,
    pub keep_aspect: bool,
    pub filter: ResizeFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResizeMode {
    Scale,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResizeFilter {
    Lanczos3,
    Nearest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Palette {
    pub enabled: bool,
    pub mode: PaletteMode,
    pub colors: u32,
    pub dithering: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaletteMode {
    Auto,
    Fixed,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            lossless: Lossless::default(),
            alpha: Alpha::default(),
            resize: Resize::default(),
            palette: Palette::default(),
        }
    }
}

impl Default for Lossless {
    fn default() -> Self {
        Self { enabled: true, level: 2, strip_metadata: true, remove_interlace: true, zopfli: false }
    }
}

impl Default for Alpha {
    fn default() -> Self {
        Self { enabled: false, low: 8, high: 247 }
    }
}

impl Default for Resize {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: ResizeMode::Scale,
            scale: 2,
            width: 1024,
            height: 1024,
            keep_aspect: true,
            filter: ResizeFilter::Lanczos3,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self { enabled: true, mode: PaletteMode::Auto, colors: 256, dithering: false }
    }
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        if self.lossless.level > 6 {
            bail!("Lossless level must be 0-6");
        }
        if self.alpha.enabled && self.alpha.low >= self.alpha.high {
            bail!("Alpha low threshold must be lower than high threshold");
        }
        if self.resize.enabled {
            match self.resize.mode {
                ResizeMode::Scale => {
                    if !(1..=4).contains(&self.resize.scale) {
                        bail!("Scale must be 1x-4x");
                    }
                }
                ResizeMode::Custom => {
                    if self.resize.width == 0 || self.resize.height == 0 {
                        bail!("Custom width and height must be at least 1");
                    }
                }
            }
        }
        if self.palette.enabled
            && self.palette.mode == PaletteMode::Fixed
            && !(2..=256).contains(&self.palette.colors)
        {
            bail!("Palette color count must be 2-256");
        }
        Ok(())
    }

    /// Target size for an image of the given size; equals the input when no resize applies.
    pub fn target_size(&self, width: u32, height: u32) -> (u32, u32) {
        if !self.resize.enabled {
            return (width, height);
        }
        match self.resize.mode {
            ResizeMode::Scale => {
                let f = self.resize.scale as f64 / 4.0;
                (scaled(width, f), scaled(height, f))
            }
            ResizeMode::Custom => {
                if self.resize.keep_aspect {
                    let f = (self.resize.width as f64 / width as f64)
                        .min(self.resize.height as f64 / height as f64);
                    (scaled(width, f), scaled(height, f))
                } else {
                    (self.resize.width, self.resize.height)
                }
            }
        }
    }
}

fn scaled(v: u32, f: f64) -> u32 {
    ((v as f64 * f).round() as u32).max(1)
}
