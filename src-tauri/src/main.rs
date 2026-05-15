#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tailsync_lib::sync::SyncManager;
use tailsync_lib::AppState;

fn main() {
    let pairs_path = config_dir().join("pairs.json");
    let pairs = tailsync_lib::pairs::load_pairs(&pairs_path).unwrap_or_default();

    let state = AppState {
        pairs: Mutex::new(pairs),
        pairs_path,
        sync_manager: Arc::new(SyncManager::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            tailsync_lib::commands::list_pairs,
            tailsync_lib::commands::add_pair,
            tailsync_lib::commands::update_pair,
            tailsync_lib::commands::delete_pair,
            tailsync_lib::commands::list_tailnet_devices,
            tailsync_lib::commands::env_check,
            tailsync_lib::commands::probe_remote_path,
            tailsync_lib::commands::create_remote_dir,
            tailsync_lib::commands::dry_run_sync,
            tailsync_lib::commands::start_sync,
            tailsync_lib::commands::cancel_sync,
            tailsync_lib::commands::open_full_disk_access,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/Application Support/tailsync")
}
