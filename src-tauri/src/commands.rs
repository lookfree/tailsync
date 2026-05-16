use crate::env_check::{check_environment, EnvCheckResult};
use crate::errors::{AppError, AppResult};
use crate::excludes::merged_excludes;
use crate::pairs::{save_pairs, DirectoryPair, LastSync, SyncDirection, SyncStatus};
use crate::remote::{create_remote_dir as remote_mkdir, probe_remote_path as remote_probe, PathProbeResult};
use crate::rsync::{ProgressUpdate, RsyncConfig};
use crate::sync::{run_dry_run, spawn_sync, DryRunSummary, SyncResult};
use crate::tailscale::{fetch_status, TailnetDevice};
use crate::AppState;
use serde::Deserialize;
use std::io::Write;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction { Push, Pull }

#[derive(Debug, Clone, Deserialize)]
pub struct SyncRequest {
    pub pair_id: String,
    pub direction: Direction,
}

fn build_config_for(pair: &DirectoryPair, dir: &Direction, dry_run: bool) -> std::io::Result<(RsyncConfig, std::path::PathBuf)> {
    // Write merged excludes to a tempfile.
    let merged = merged_excludes(&pair.excludes);
    let tmp = std::env::temp_dir().join(format!("tailsync-excludes-{}.txt", Uuid::new_v4()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        for line in &merged {
            writeln!(f, "{}", line)?;
        }
    }

    let local = ensure_trailing_slash(&pair.local_path);
    let remote = format!("{}@{}:{}", pair.remote_user, pair.remote_host, ensure_trailing_slash(&pair.remote_path));

    let (source, destination) = match dir {
        Direction::Push => (local, remote),
        Direction::Pull => (remote, local),
    };

    Ok((
        RsyncConfig {
            source,
            destination,
            excludes_file: Some(tmp.to_string_lossy().into_owned()),
            bandwidth_limit_kbps: pair.bandwidth_limit_kbps,
            mirror_mode: pair.mirror_mode,
            dry_run,
            timeout_seconds: 300,
        },
        tmp,
    ))
}

fn ensure_trailing_slash(p: &str) -> String {
    if p.ends_with('/') { p.to_string() } else { format!("{}/", p) }
}

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
#[tauri::command]
pub async fn dry_run_sync(state: State<'_, AppState>, req: SyncRequest) -> AppResult<DryRunSummary> {
    let pair = {
        let guard = state.pairs.lock().unwrap();
        guard.iter().find(|p| p.id == req.pair_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(req.pair_id.clone()))?
    };
    let (cfg, tmp) = build_config_for(&pair, &req.direction, true)?;
    let result = run_dry_run(&cfg).await.map_err(|e| AppError::Rsync(e.to_string()));
    let _ = std::fs::remove_file(&tmp);
    result
}

#[tauri::command]
pub async fn start_sync(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    req: SyncRequest,
) -> AppResult<String> {
    let pair = {
        let guard = state.pairs.lock().unwrap();
        guard.iter().find(|p| p.id == req.pair_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(req.pair_id.clone()))?
    };
    let (cfg, tmp) = build_config_for(&pair, &req.direction, false)?;
    let task_id = Uuid::new_v4().to_string();

    let app_for_progress = app.clone();
    let task_id_clone = task_id.clone();
    let progress: Arc<dyn Fn(ProgressUpdate) + Send + Sync> = Arc::new(move |p| {
        let _ = app_for_progress.emit(&format!("sync-progress:{}", task_id_clone), p);
    });

    let (child, stderr_buf) = spawn_sync(&cfg, progress).await
        .map_err(|e| AppError::Rsync(e.to_string()))?;

    state.sync_manager.register(task_id.clone(), child).await;

    // Background task waits for completion, emits final event, removes from manager.
    let app_for_done = app.clone();
    let manager_handle = Arc::clone(&state.sync_manager);
    let task_id_done = task_id.clone();
    let direction_for_record = req.direction.clone();
    let pair_id_for_record = pair.id.clone();
    tokio::spawn(async move {
        let exit = manager_handle.wait_and_remove(&task_id_done).await;
        let stderr = stderr_buf.lock().unwrap().clone();
        let result = SyncResult {
            exit_code: exit.and_then(|s| s.code()).unwrap_or(-1),
            message: if exit.map(|s| s.success()).unwrap_or(false) { "完成".into() } else { "失败".into() },
            stderr_tail: stderr,
        };

        // Update pair's last_sync.
        let last = LastSync {
            direction: match direction_for_record {
                Direction::Push => SyncDirection::Push,
                Direction::Pull => SyncDirection::Pull,
            },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            status: if exit.map(|s| s.success()).unwrap_or(false) {
                SyncStatus::Success
            } else if exit.is_none() {
                SyncStatus::Interrupted
            } else {
                SyncStatus::Failed
            },
            message: result.message.clone(),
        };
        {
            let state_handle = app_for_done.state::<AppState>();
            let mut guard = state_handle.pairs.lock().unwrap();
            if let Some(p) = guard.iter_mut().find(|p| p.id == pair_id_for_record) {
                p.last_sync = Some(last);
            }
            let _ = save_pairs(&state_handle.pairs_path, &guard);
        }

        let _ = app_for_done.emit(&format!("sync-done:{}", task_id_done), result);
        let _ = std::fs::remove_file(&tmp);
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn cancel_sync(state: State<'_, AppState>, task_id: String) -> AppResult<bool> {
    Ok(state.sync_manager.cancel(&task_id).await)
}
#[tauri::command]
pub async fn open_full_disk_access() -> AppResult<()> {
    let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";
    std::process::Command::new("open")
        .arg(url)
        .status()
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}
