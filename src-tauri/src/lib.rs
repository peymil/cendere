mod analysis;
mod apply;
mod commands;
mod job;
mod options;
mod pipeline;
mod png_io;
mod preview;
mod scan;

use job::JobState;
use std::fs;
use tauri::{Manager, RunEvent};

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let root = app.path().app_cache_dir()?.join("staging");
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root)?;
            app.manage(JobState::new(root));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_dir,
            commands::analyze_png,
            commands::thumbnail,
            commands::image_data_url,
            commands::start_process,
            commands::cancel_process,
            commands::get_job,
            commands::discard_results,
            commands::discard_all,
            commands::apply_results,
        ])
        .build(tauri::generate_context!())
        .expect("error while building cendere");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<JobState>() {
                if let Ok(inner) = state.inner.lock() {
                    if let Some(r) = &inner.running {
                        r.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                let _ = fs::remove_dir_all(&state.staging_root);
            }
        }
    });
}
