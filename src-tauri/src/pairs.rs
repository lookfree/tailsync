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
}
