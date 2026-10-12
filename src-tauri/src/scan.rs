use crate::png_io::{self, PngInfo};
use anyhow::{bail, Result};
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanItem {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub color_type: String,
    pub bit_depth: u8,
    pub skip_reason: Option<String>,
}

fn is_png_name(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// Lists every regular .png file under `root` (recursive, symlinks not followed), sorted by path.
pub fn scan_dir(root: &Path) -> Result<Vec<ScanItem>> {
    if !root.is_dir() {
        bail!("Not a directory: {}", root.display());
    }
    let mut paths: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_png_name(e.path()))
        .map(|e| e.into_path())
        .collect();
    paths.sort();
    Ok(paths.par_iter().map(|p| inspect(p)).collect())
}

fn inspect(path: &Path) -> ScanItem {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut item = ScanItem {
        path: path.to_string_lossy().into_owned(),
        name,
        size,
        width: 0,
        height: 0,
        color_type: String::new(),
        bit_depth: 0,
        skip_reason: None,
    };
    match fs::read(path).map_err(anyhow::Error::from).and_then(|b| png_io::read_info(&b)) {
        Ok(info) => fill(&mut item, &info),
        Err(e) => item.skip_reason = Some(e.to_string()),
    }
    item
}

fn fill(item: &mut ScanItem, info: &PngInfo) {
    item.width = info.width;
    item.height = info.height;
    item.color_type = info.color_type_name.to_string();
    item.bit_depth = info.bit_depth;
    item.skip_reason = if info.is_apng {
        Some("Animated PNG (APNG) is not supported".into())
    } else if png_io::decoded_bytes(info.width, info.height) > png_io::MAX_DECODED_BYTES {
        Some("Image is larger than 1 GB when decoded".into())
    } else {
        None
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = crate::png_io::Rgba8 {
            width: w,
            height: h,
            data: vec![120; (w * h * 4) as usize],
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, crate::png_io::encode_rgba(&img).unwrap()).unwrap();
    }

    #[test]
    fn scan_is_recursive_and_flags_skips() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_png(&root.join("a.png"), 10, 10);
        write_png(&root.join("sub/deep/B.PNG"), 4, 4);
        fs::write(root.join("notes.txt"), b"x").unwrap();
        fs::write(root.join("broken.png"), b"not a png").unwrap();

        let base = crate::png_io::encode_rgba(&crate::png_io::Rgba8 {
            width: 2,
            height: 2,
            data: vec![0; 16],
        })
        .unwrap();
        let mut apng = base[..33].to_vec();
        apng.extend_from_slice(&8u32.to_be_bytes());
        apng.extend_from_slice(b"acTL");
        apng.extend_from_slice(&[0; 8 + 4]);
        apng.extend_from_slice(&base[33..]);
        fs::write(root.join("anim.png"), apng).unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("a.png"), root.join("link.png")).unwrap();

        let items = scan_dir(root).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"a.png") && names.contains(&"B.PNG"));
        assert!(!names.contains(&"notes.txt"));
        #[cfg(unix)]
        assert!(!names.contains(&"link.png"));

        let by = |n: &str| items.iter().find(|i| i.name == n).unwrap();
        assert!(by("a.png").skip_reason.is_none());
        assert_eq!((by("a.png").width, by("a.png").height), (10, 10));
        assert!(by("broken.png").skip_reason.is_some());
        assert!(by("anim.png").skip_reason.as_deref().unwrap().contains("APNG"));
    }
}
