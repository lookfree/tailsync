pub mod pairs;
pub mod excludes;
pub mod rsync;
pub mod tailscale;
pub mod env_check;
pub mod sync;
pub mod remote;
pub mod errors;
pub mod commands;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use crate::pairs::DirectoryPair;
use crate::sync::SyncManager;

pub struct AppState {
    pub pairs: Mutex<Vec<DirectoryPair>>,
    pub pairs_path: PathBuf,
    pub sync_manager: Arc<SyncManager>,
}
