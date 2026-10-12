use crate::analysis::{self, Analysis};
use crate::job::{self, ApplyReport, Job, JobItem, JobState, ProcessFinished, Progress, Running};
use crate::options::Options;
use crate::preview;
use crate::scan::{self, ScanItem};
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

type CmdResult<T> = Result<T, String>;

fn msg(e: anyhow::Error) -> String {
    format!("{:#}", e)
}

async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(msg)
}

fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("job-{}-{}", std::process::id(), nanos)
}

fn lock_err<T>(_: T) -> String {
    "Internal state lock failed".into()
}

#[tauri::command]
pub async fn scan_dir(dir: String) -> CmdResult<Vec<ScanItem>> {
    blocking(move || scan::scan_dir(Path::new(&dir))).await
}

#[tauri::command]
pub async fn analyze_png(path: String) -> CmdResult<Analysis> {
    blocking(move || analysis::analyze(Path::new(&path))).await
}

#[tauri::command]
pub async fn thumbnail(path: String, max: u32) -> CmdResult<String> {
    blocking(move || preview::thumbnail(Path::new(&path), max.clamp(16, 512))).await
}

#[tauri::command]
pub async fn image_data_url(path: String) -> CmdResult<String> {
    blocking(move || preview::image_data_url(Path::new(&path))).await
}

#[tauri::command]
pub fn start_process(
    app: AppHandle,
    state: State<'_, JobState>,
    paths: Vec<String>,
    options: Options,
) -> CmdResult<String> {
    options.validate().map_err(msg)?;
    if paths.is_empty() {
        return Err("No files selected".into());
    }
    let id = new_id();
    let dir = state.staging_root.join(&id);
    let running = Arc::new(Running { cancel: AtomicBool::new(false) });
    {
        let mut inner = state.inner.lock().map_err(lock_err)?;
        if inner.running.is_some() {
            return Err("A process is already running".into());
        }
        if let Some(old) = inner.job.take() {
            let _ = fs::remove_dir_all(&old.dir);
        }
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        inner.running = Some(running.clone());
    }

    let returned_id = id.clone();
    let worker_app = app.clone();
    std::thread::spawn(move || {
        let progress_app = worker_app.clone();
        let items = job::process_files(&dir, &paths, &options, &running.cancel, move |done, total| {
            let _ = progress_app.emit("process-progress", Progress { done, total });
        });
        let cancelled = running.cancel.load(Ordering::Relaxed);
        let count = |s: job::ItemStatus| items.iter().filter(|i| i.status == s).count();
        let summary = ProcessFinished {
            id: id.clone(),
            cancelled,
            ready: count(job::ItemStatus::Ready),
            no_gain: count(job::ItemStatus::NoGain),
            failed: count(job::ItemStatus::Failed),
        };
        let state = worker_app.state::<JobState>();
        if let Ok(mut inner) = state.inner.lock() {
            inner.running = None;
            inner.job = Some(Job { id, dir, items });
        }
        let _ = worker_app.emit("process-finished", summary);
    });
    Ok(returned_id)
}

#[tauri::command]
pub fn cancel_process(state: State<'_, JobState>) -> CmdResult<()> {
    let inner = state.inner.lock().map_err(lock_err)?;
    if let Some(r) = &inner.running {
        r.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: Option<String>,
    pub running: bool,
    pub items: Vec<JobItem>,
}

#[tauri::command]
pub fn get_job(state: State<'_, JobState>) -> CmdResult<JobView> {
    let inner = state.inner.lock().map_err(lock_err)?;
    Ok(JobView {
        id: inner.job.as_ref().map(|j| j.id.clone()),
        running: inner.running.is_some(),
        items: inner.job.as_ref().map(|j| j.items.clone()).unwrap_or_default(),
    })
}

#[tauri::command]
pub fn discard_results(state: State<'_, JobState>, paths: Vec<String>) -> CmdResult<()> {
    let mut inner = state.inner.lock().map_err(lock_err)?;
    if inner.running.is_some() {
        return Err("Wait for the process to finish first".into());
    }
    if let Some(job) = inner.job.as_mut() {
        for item in job.items.iter().filter(|i| paths.contains(&i.path)) {
            if let Some(staged) = &item.staged_path {
                let _ = fs::remove_file(staged);
            }
        }
        job.items.retain(|i| !paths.contains(&i.path));
    }
    Ok(())
}

#[tauri::command]
pub fn discard_all(state: State<'_, JobState>) -> CmdResult<()> {
    let mut inner = state.inner.lock().map_err(lock_err)?;
    if inner.running.is_some() {
        return Err("Wait for the process to finish first".into());
    }
    if let Some(job) = inner.job.take() {
        let _ = fs::remove_dir_all(&job.dir);
    }
    Ok(())
}

fn apply_blocking(app: AppHandle, paths: Vec<String>) -> Result<ApplyReport> {
    let state = app.state::<JobState>();
    let job = {
        let mut inner = state.inner.lock().map_err(|_| anyhow!("Internal state lock failed"))?;
        if inner.running.is_some() {
            bail!("Wait for the process to finish first");
        }
        inner.job.take().context("No processed results. Run Process first.")?
    };
    let report = job::apply_items(&job.items, &paths);
    let _ = fs::remove_dir_all(&job.dir);
    Ok(report)
}

#[tauri::command]
pub async fn apply_results(app: AppHandle, paths: Vec<String>) -> CmdResult<ApplyReport> {
    blocking(move || apply_blocking(app, paths)).await
}
