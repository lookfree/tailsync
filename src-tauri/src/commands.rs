use crate::errors::AppResult;
use tauri::State;

#[tauri::command] pub fn list_pairs() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn add_pair() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn update_pair() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn delete_pair() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn list_tailnet_devices() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn env_check() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn probe_remote_path() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn create_remote_dir() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn dry_run_sync() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn start_sync() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn cancel_sync() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn open_full_disk_access() -> AppResult<()> { Ok(()) }
