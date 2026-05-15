use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection { Push, Pull }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus { Success, Failed, Interrupted }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LastSync {
    pub direction: SyncDirection,
    pub timestamp: i64,
    pub status: SyncStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirectoryPair {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub remote_host: String,
    pub remote_user: String,
    pub remote_path: String,
    pub excludes: Vec<String>,
    pub bandwidth_limit_kbps: Option<u32>,
    pub mirror_mode: bool,
    pub last_sync: Option<LastSync>,
}

use std::path::Path;
use std::io::Write;

pub fn load_pairs(path: &Path) -> std::io::Result<Vec<DirectoryPair>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path)?;
    let pairs: Vec<DirectoryPair> = serde_json::from_slice(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(pairs)
}

pub fn save_pairs(path: &Path, pairs: &[DirectoryPair]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        let bytes = serde_json::to_vec_pretty(pairs)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_pair_round_trips_through_json() {
        let pair = DirectoryPair {
            id: "abc-123".to_string(),
            name: "读书笔记".to_string(),
            local_path: "/Users/me/Documents/notes".to_string(),
            remote_host: "mac-mini".to_string(),
            remote_user: "me".to_string(),
            remote_path: "/Users/me/sync/notes".to_string(),
            excludes: vec!["*.tmp".to_string(), ".git/".to_string()],
            bandwidth_limit_kbps: Some(5000),
            mirror_mode: true,
            last_sync: Some(LastSync {
                direction: SyncDirection::Push,
                timestamp: 1715000000,
                status: SyncStatus::Success,
                message: "完成".to_string(),
            }),
        };

        let json = serde_json::to_string(&pair).unwrap();
        let back: DirectoryPair = serde_json::from_str(&json).unwrap();
        assert_eq!(pair, back);
    }

    #[test]
    fn last_sync_serializes_enums_as_snake_case() {
        let ls = LastSync {
            direction: SyncDirection::Pull,
            timestamp: 0,
            status: SyncStatus::Interrupted,
            message: String::new(),
        };
        let json = serde_json::to_value(&ls).unwrap();
        assert_eq!(json["direction"], "pull");
        assert_eq!(json["status"], "interrupted");
    }

    #[test]
    fn load_returns_empty_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.json");
        let pairs = load_pairs(&path).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pairs.json");
        let pairs = vec![DirectoryPair {
            id: "x".into(),
            name: "n".into(),
            local_path: "/a".into(),
            remote_host: "h".into(),
            remote_user: "u".into(),
            remote_path: "/b".into(),
            excludes: vec![],
            bandwidth_limit_kbps: None,
            mirror_mode: false,
            last_sync: None,
        }];
        save_pairs(&path, &pairs).unwrap();
        let back = load_pairs(&path).unwrap();
        assert_eq!(pairs, back);
    }

    #[test]
    fn save_creates_missing_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b/c/pairs.json");
        save_pairs(&nested, &[]).unwrap();
        assert!(nested.exists());
    }
}
