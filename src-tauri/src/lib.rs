mod commands;
mod models;
mod settings;
mod tracker;
mod riot {
    pub mod lockfile;
    pub mod local;
    pub mod client;
    pub mod parse;
    pub mod ratelimit;
    pub mod content;
}
mod henrik;
mod store {
    pub mod profile;
}

use std::sync::Arc;
use tokio::sync::Mutex;
use tracker::Tracker;

pub type AppTracker = Arc<Mutex<Tracker>>;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tracker = Arc::new(Mutex::new(Tracker::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(tracker.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::refresh_now,
            commands::refresh_profile,
            commands::profile_pull_start,
            commands::profile_pull_status,
            commands::profile_pull_cancel,
            commands::profile_stats,
            commands::profile_delete_oldest,
            commands::lookup,
            commands::get_settings,
            commands::save_settings,
            commands::set_always_on_top,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let t = tracker.clone();
            tauri::async_runtime::spawn(async move {
                tracker::run_loop(t, handle).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
