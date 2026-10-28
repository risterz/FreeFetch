pub mod core;
pub mod engine;
pub mod extractor;
pub mod ipc;
pub mod media;
pub mod service;
pub mod storage;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

use crate::core::config::AppSettings;
use crate::engine::queue::QueueManager;
use crate::engine::EngineManager;
use crate::extractor::ExtractorRegistry;
use crate::ipc::commands::*;
use crate::ipc::AppState;
use crate::storage::initialize_database;

fn get_database_path() -> PathBuf {
    if let Some(app_data) = std::env::var_os("APPDATA") {
        PathBuf::from(app_data)
            .join("FreeFetch")
            .join("freefetch.db")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".freefetch").join("freefetch.db")
    } else {
        PathBuf::from("freefetch.db")
    }
}

pub fn run() {
    let db_path = get_database_path();
    let repo = initialize_database(&db_path).expect("Failed to initialize SQLite database");

    // Load persisted settings or fallback to default
    let settings = if let Ok(Some(json)) = repo.get_setting("app_settings") {
        serde_json::from_str::<AppSettings>(&json).unwrap_or_default()
    } else {
        AppSettings::default()
    };

    let settings_arc = Arc::new(Mutex::new(settings));
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry.clone());
    let queue = QueueManager::new(repo.clone(), engine.clone(), settings_arc.clone());

    let state = AppState {
        repo,
        engine: engine.clone(),
        queue: queue.clone(),
        registry,
        settings: settings_arc,
    };

    let queue_for_setup = queue.clone();
    let engine_for_setup = engine.clone();

    tauri::Builder::default()
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let app_h1 = app_handle.clone();
            queue_for_setup.set_event_sink(Arc::new(move |event, payload| {
                let _ = app_h1.emit(event, payload);
            }));
            let app_h2 = app_handle.clone();
            engine_for_setup.set_event_sink(Arc::new(move |event, payload| {
                let _ = app_h2.emit(event, payload);
            }));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_url,
            start_download,
            pause_download,
            resume_download,
            cancel_download,
            delete_download,
            get_downloads,
            get_settings,
            update_settings,
            check_ffmpeg_status,
            check_ytdlp_status,
            diagnose_ytdlp,
            update_ytdlp,
            open_downloaded_file,
            open_containing_folder,
            queue_add,
            queue_remove,
            queue_start,
            queue_pause,
            queue_resume,
            queue_cancel,
            queue_retry,
            queue_reorder,
            queue_move_up,
            queue_move_down,
            queue_get,
            queue_clear_completed,
            queue_clear_failed,
            queue_set_concurrency,
            get_queue_summary,
            parse_batch_urls_text,
            analyze_batch_urls,
            add_batch_to_queue,
            detect_playlist_url,
            analyze_playlist,
            add_playlist_to_queue,
            get_download_history,
            get_download_history_item,
            delete_download_history_item,
            clear_download_history,
            get_download_statistics,
            export_download_history,
            retry_history_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FreeFetch Tauri application");
}
