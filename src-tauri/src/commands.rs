use crate::env_check::{check_environment, EnvCheckResult};
use crate::errors::{AppError, AppResult};
use crate::pairs::{save_pairs, DirectoryPair};
use crate::remote::{create_remote_dir as remote_mkdir, probe_remote_path as remote_probe, PathProbeResult};
use crate::tailscale::{fetch_status, TailnetDevice};
use crate::AppState;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn list_pairs(state: State<AppState>) -> AppResult<Vec<DirectoryPair>> {
    Ok(state.pairs.lock().unwrap().clone())
}

#[tauri::command]
pub fn add_pair(state: State<AppState>, mut pair: DirectoryPair) -> AppResult<DirectoryPair> {
    if pair.id.is_empty() {
        pair.id = Uuid::new_v4().to_string();
    }
    let mut guard = state.pairs.lock().unwrap();
    guard.push(pair.clone());
    save_pairs(&state.pairs_path, &guard)?;
    Ok(pair)
}

#[tauri::command]
pub fn update_pair(state: State<AppState>, pair: DirectoryPair) -> AppResult<DirectoryPair> {
    let mut guard = state.pairs.lock().unwrap();
    let idx = guard
        .iter()
        .position(|p| p.id == pair.id)
        .ok_or_else(|| AppError::NotFound(format!("pair {}", pair.id)))?;
    guard[idx] = pair.clone();
    save_pairs(&state.pairs_path, &guard)?;
    Ok(pair)
}

#[tauri::command]
pub fn delete_pair(state: State<AppState>, id: String) -> AppResult<()> {
    let mut guard = state.pairs.lock().unwrap();
    let before = guard.len();
    guard.retain(|p| p.id != id);
    if guard.len() == before {
        return Err(AppError::NotFound(format!("pair {}", id)));
    }
    save_pairs(&state.pairs_path, &guard)?;
    Ok(())
}

#[tauri::command]
pub async fn list_tailnet_devices() -> AppResult<Vec<TailnetDevice>> {
    let (me, mut peers) = fetch_status()?;
    peers.insert(0, me);
    Ok(peers)
}

#[tauri::command]
pub async fn env_check() -> AppResult<EnvCheckResult> {
    Ok(check_environment())
}

#[tauri::command]
pub async fn probe_remote_path(user: String, host: String, path: String) -> AppResult<PathProbeResult> {
    Ok(remote_probe(&user, &host, &path).await)
}

#[tauri::command]
pub async fn create_remote_dir(user: String, host: String, path: String) -> AppResult<()> {
    remote_mkdir(&user, &host, &path).await.map_err(AppError::Ssh)
}
#[tauri::command] pub fn dry_run_sync() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn start_sync() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn cancel_sync() -> AppResult<()> { Ok(()) }
#[tauri::command] pub fn open_full_disk_access() -> AppResult<()> { Ok(()) }
