use crate::apply::{atomic_replace, sha256_hex};
use crate::options::Options;
use crate::pipeline;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ItemStatus {
    Ready,
    NoGain,
    Failed,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JobItem {
    pub path: String,
    pub status: ItemStatus,
    pub source_size: u64,
    pub result_size: u64,
    pub staged_path: Option<String>,
    pub resized: bool,
    pub notes: Vec<String>,
    pub error: Option<String>,
    #[serde(skip)]
    pub source_sha: String,
    #[serde(skip)]
    pub result_sha: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessFinished {
    pub id: String,
    pub cancelled: bool,
    pub ready: usize,
    pub no_gain: usize,
    pub failed: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub applied: Vec<String>,
    pub skipped: Vec<SkipInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkipInfo {
    pub path: String,
    pub reason: String,
}

pub struct Job {
    pub id: String,
    pub dir: PathBuf,
    pub items: Vec<JobItem>,
}

pub struct Running {
    pub cancel: AtomicBool,
}

#[derive(Default)]
pub struct Inner {
    pub job: Option<Job>,
    pub running: Option<Arc<Running>>,
}

pub struct JobState {
    pub staging_root: PathBuf,
    pub inner: Mutex<Inner>,
}

impl JobState {
    pub fn new(staging_root: PathBuf) -> Self {
        Self { staging_root, inner: Mutex::new(Inner::default()) }
    }
}

fn process_one(job_dir: &Path, index: usize, path: &str, opts: &Options) -> JobItem {
    let mut item = JobItem {
        path: path.to_string(),
        status: ItemStatus::Failed,
        source_size: 0,
        result_size: 0,
        staged_path: None,
        resized: false,
        notes: Vec::new(),
        error: None,
        source_sha: String::new(),
        result_sha: String::new(),
    };
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            item.error = Some(format!("Cannot read file: {e}"));
            return item;
        }
    };
    item.source_size = bytes.len() as u64;
    item.source_sha = sha256_hex(&bytes);
    match pipeline::run(&bytes, opts) {
        Err(e) => item.error = Some(format!("{:#}", e)),
        Ok(out) => {
            item.resized = out.resized;
            item.notes = out.notes;
            item.result_size = out.bytes.len() as u64;
            if !out.resized && out.bytes.len() >= bytes.len() {
                item.status = ItemStatus::NoGain;
            } else {
                let staged = job_dir.join(format!("{:05}.png", index));
                match fs::write(&staged, &out.bytes) {
                    Ok(()) => {
                        item.status = ItemStatus::Ready;
                        item.staged_path = Some(staged.to_string_lossy().into_owned());
                        item.result_sha = sha256_hex(&out.bytes);
                    }
                    Err(e) => item.error = Some(format!("Cannot write staged file: {e}")),
                }
            }
        }
    }
    item
}

/// Runs the pipeline on every path in parallel. Items not started before cancel are dropped.
pub fn process_files(
    job_dir: &Path,
    paths: &[String],
    opts: &Options,
    cancel: &AtomicBool,
    on_progress: impl Fn(usize, usize) + Sync,
) -> Vec<JobItem> {
    let total = paths.len();
    let done = AtomicUsize::new(0);
    let mut items: Vec<JobItem> = paths
        .par_iter()
        .enumerate()
        .filter_map(|(i, p)| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let item = process_one(job_dir, i, p, opts);
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            on_progress(d, total);
            Some(item)
        })
        .collect();
    items.sort_by(|a, b| a.path.cmp(&b.path));
    items
}

pub fn apply_items(items: &[JobItem], selected: &[String]) -> ApplyReport {
    let selected: HashSet<&str> = selected.iter().map(|s| s.as_str()).collect();
    let results: Vec<Result<String, SkipInfo>> = items
        .par_iter()
        .filter(|i| selected.contains(i.path.as_str()))
        .map(apply_one)
        .collect();
    let mut report = ApplyReport::default();
    for r in results {
        match r {
            Ok(p) => report.applied.push(p),
            Err(s) => report.skipped.push(s),
        }
    }
    report.applied.sort();
    report.skipped.sort_by(|a, b| a.path.cmp(&b.path));
    report
}

fn apply_one(item: &JobItem) -> Result<String, SkipInfo> {
    let skip = |reason: &str| SkipInfo { path: item.path.clone(), reason: reason.to_string() };
    if item.status != ItemStatus::Ready {
        return Err(skip("No changed result to apply"));
    }
    let staged = item.staged_path.as_deref().ok_or_else(|| skip("Staged result is missing"))?;
    let current = fs::read(&item.path).map_err(|e| skip(&format!("Cannot read file: {e}")))?;
    if sha256_hex(&current) != item.source_sha {
        return Err(skip("File changed since Process; not applied"));
    }
    let bytes = fs::read(staged).map_err(|_| skip("Staged result is missing"))?;
    if sha256_hex(&bytes) != item.result_sha {
        return Err(skip("Staged result is corrupted; not applied"));
    }
    atomic_replace(Path::new(&item.path), &bytes)
        .map_err(|e| skip(&format!("Write failed: {:#}", e)))?;
    Ok(item.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png_io::{self, Rgba8};

    fn noisy_png(w: u32, h: u32, seed: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        let mut s = seed.wrapping_mul(2654435761).wrapping_add(1);
        for _ in 0..w * h {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            let v = (s >> 16) as u8;
            data.extend_from_slice(&[v, v / 2, 200, 255]);
        }
        png_io::encode_rgba(&Rgba8 { width: w, height: h, data }).unwrap()
    }

    #[test]
    fn process_then_apply_replaces_only_selected_and_detects_changes() {
        let work = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let a = work.path().join("a.png");
        let b = work.path().join("sub_b.png");
        fs::write(&a, noisy_png(64, 64, 1)).unwrap();
        fs::write(&b, noisy_png(64, 64, 2)).unwrap();
        let paths = vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()];
        let a_before = fs::read(&a).unwrap();

        let mut opts = Options::default();
        opts.palette.mode = crate::options::PaletteMode::Fixed;
        opts.palette.colors = 16;
        let cancel = AtomicBool::new(false);
        let items = process_files(staging.path(), &paths, &opts, &cancel, |_, _| {});
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.status == ItemStatus::Ready));
        assert_eq!(fs::read(&a).unwrap(), a_before, "Process must not touch originals");

        fs::write(&b, b"changed by user").unwrap();
        let report = apply_items(&items, &[paths[0].clone(), paths[1].clone()]);
        assert_eq!(report.applied, vec![paths[0].clone()]);
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].reason.contains("changed"));

        let applied = fs::read(&a).unwrap();
        let info = png_io::read_info(&applied).unwrap();
        assert_eq!(info.color_type, 3, "applied file is the staged indexed result");
        assert_eq!(fs::read(&b).unwrap(), b"changed by user");
    }

    #[test]
    fn cancel_before_start_returns_no_items() {
        let work = tempfile::tempdir().unwrap();
        let a = work.path().join("a.png");
        fs::write(&a, noisy_png(8, 8, 3)).unwrap();
        let cancel = AtomicBool::new(true);
        let items = process_files(work.path(), &[a.to_string_lossy().into_owned()], &Options::default(), &cancel, |_, _| {});
        assert!(items.is_empty());
    }

    #[test]
    fn no_gain_is_reported_without_staged_file() {
        let work = tempfile::tempdir().unwrap();
        let a = work.path().join("tiny.png");
        fs::write(&a, noisy_png(2, 2, 4)).unwrap();
        let mut opts = Options::default();
        opts.lossless.level = 0;
        opts.palette.enabled = false;
        let cancel = AtomicBool::new(false);
        let items = process_files(work.path(), &[a.to_string_lossy().into_owned()], &opts, &cancel, |_, _| {});
        assert_eq!(items.len(), 1);
        if items[0].status == ItemStatus::NoGain {
            assert!(items[0].staged_path.is_none());
        }
        assert_ne!(items[0].status, ItemStatus::Failed, "{:?}", items[0].error);
    }
}
