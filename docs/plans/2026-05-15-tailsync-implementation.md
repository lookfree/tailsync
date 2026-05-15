# tailsync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS desktop app that lets two Macs in the same Tailscale tailnet sync user-defined directory pairs in either direction, using rsync over Tailscale SSH under the hood.

**Architecture:** Tauri 2 app with Rust backend and Vue 3 + TypeScript frontend. Backend spawns `rsync` and `ssh` subprocesses, parses their output, and bridges progress events to the frontend via Tauri's event system. Two installed apps do not communicate with each other — each is independently a "sender" that drives rsync against the other side's sshd.

**Tech Stack:**
- Tauri 2.x (Rust runtime + WebView frontend)
- Rust: tokio, serde, serde_json, uuid, anyhow, thiserror
- Frontend: Vue 3 + Composition API + TypeScript + Pinia + Vite
- Tauri plugins: `tauri-plugin-dialog`, `tauri-plugin-shell`, `tauri-plugin-os`
- Test: `cargo test` for Rust, Vitest + `@vue/test-utils` for Vue
- External binaries (system installed): `rsync`, `ssh`, `tailscale`

---

## File Structure

```
tailsync/
├── Cargo.toml                          # workspace root
├── package.json                        # frontend deps
├── vite.config.ts
├── tsconfig.json
├── index.html
├── src/                                # Vue frontend
│   ├── main.ts                         # Vue + Pinia bootstrap
│   ├── App.vue                         # root, env-check guard, route
│   ├── types.ts                        # shared TS types (mirror Rust models)
│   ├── lib/
│   │   ├── tauri.ts                    # invoke + listen wrappers
│   │   └── format.ts                   # bytes / time / rate formatters
│   ├── stores/
│   │   └── pairs.ts                    # Pinia store: pair list CRUD
│   └── components/
│       ├── EnvCheck.vue                # first-run gate
│       ├── StatusBar.vue               # top bar: hostname + tailscale status
│       ├── PairList.vue                # main directory pair table
│       ├── PairForm.vue                # add/edit modal
│       ├── SyncDialog.vue              # dry-run preview + progress
│       └── ErrorDialog.vue             # error with stderr collapse
├── src-tauri/                          # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── icons/                          # app icons
│   └── src/
│       ├── main.rs                     # entry, register commands
│       ├── lib.rs                      # module exports
│       ├── errors.rs                   # AppError type
│       ├── pairs.rs                    # DirectoryPair model + JSON I/O
│       ├── excludes.rs                 # default exclude list
│       ├── rsync.rs                    # arg builder + progress parser
│       ├── tailscale.rs                # tailscale status parser + CLI calls
│       ├── env_check.rs                # first-run env detection
│       ├── sync.rs                     # spawn rsync + emit progress events
│       ├── remote.rs                   # ssh ls / mkdir helpers
│       └── commands.rs                 # #[tauri::command] bridge layer
└── docs/
    ├── specs/
    │   └── 2026-05-15-tailscale-sync-tool-design.md
    └── plans/
        └── 2026-05-15-tailsync-implementation.md
```

**Module responsibilities:**
- Pure logic (no IO, fully unit-tested): `pairs.rs` model, `excludes.rs`, `rsync.rs`, `tailscale.rs` parser
- IO-bound (integration-tested): `pairs.rs` JSON I/O, `tailscale.rs` CLI, `env_check.rs`, `sync.rs`, `remote.rs`
- Glue layer (smoke-tested via UI): `commands.rs`

**Type contract (used across all tasks — keep names consistent):**
- `DirectoryPair { id, name, local_path, remote_host, remote_user, remote_path, excludes, bandwidth_limit_kbps, mirror_mode, last_sync }`
- `LastSync { direction, timestamp, status, message }`
- `SyncDirection` enum: `Push | Pull`
- `SyncStatus` enum: `Success | Failed | Interrupted`
- `RsyncConfig { source, destination, excludes, bandwidth_limit_kbps, mirror_mode, dry_run }`
- `ProgressUpdate { bytes_transferred, total_bytes, percent, rate_bps, eta_seconds, current_file }`
- `DryRunSummary { files_to_copy, files_to_delete, files_to_update, total_bytes, file_list }`
- `TailnetDevice { hostname, tailscale_ip, user, os, online, is_self, ssh_enabled }`
- `EnvCheckResult { tailscale_installed, tailscale_logged_in, tailscale_ssh_enabled }`

---

## Phase 0: Scaffold

### Task 1: Initialize Tauri + Vue project

**Files:**
- Create: entire project skeleton via `npm create tauri-app@latest`

- [ ] **Step 1: Scaffold the project**

Run from `~/Documents/projects/tailsync/`:

```bash
cd ~/Documents/projects/tailsync
npm create tauri-app@latest . -- --template vue-ts --identifier com.wuhoujin.tailsync --manager npm
```

When prompted:
- Project name: `tailsync`
- Identifier: `com.wuhoujin.tailsync`
- Frontend language: TypeScript / JavaScript
- UI template: Vue
- UI flavor: TypeScript

If the directory is non-empty (it has `docs/` from the spec), the scaffolder will refuse. Workaround: scaffold into a temp dir then merge:

```bash
cd /tmp && npm create tauri-app@latest tailsync-scaffold -- --template vue-ts --identifier com.wuhoujin.tailsync --manager npm
rsync -a --exclude docs/ /tmp/tailsync-scaffold/ ~/Documents/projects/tailsync/
rm -rf /tmp/tailsync-scaffold
cd ~/Documents/projects/tailsync && npm install
```

- [ ] **Step 2: Install Tauri plugins and frontend deps**

Add Rust deps to `src-tauri/Cargo.toml` `[dependencies]`:

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4", "serde"] }
anyhow = "1"
thiserror = "1"
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
tauri-plugin-os = "2"
```

Add frontend deps:

```bash
npm install pinia
npm install -D vitest @vue/test-utils @vitest/ui jsdom
```

- [ ] **Step 3: Verify dev server boots**

```bash
npm run tauri dev
```

Expected: a window opens showing the default Tauri+Vue welcome page. Close the window.

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "chore: scaffold Tauri 2 + Vue 3 + TS project"
```

---

## Phase 1: Rust Pure Logic (TDD)

### Task 2: DirectoryPair data model

**Files:**
- Create: `src-tauri/src/pairs.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod pairs;`)
- Test: inline `#[cfg(test)]` block in `src-tauri/src/pairs.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/pairs.rs`:

```rust
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
```

In `src-tauri/src/lib.rs` (create the file if missing) add:

```rust
pub mod pairs;
```

- [ ] **Step 2: Run test, verify failure**

```bash
cd src-tauri && cargo test --lib pairs::tests
```

Expected: FAIL — module/struct missing or compile errors.

- [ ] **Step 3: Code is already in Step 1**

The test and impl are colocated. The "implementation" is the struct definitions above the `#[cfg(test)]` block.

- [ ] **Step 4: Run tests, verify pass**

```bash
cd src-tauri && cargo test --lib pairs::tests
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pairs.rs src-tauri/src/lib.rs
git commit -m "feat(pairs): DirectoryPair model with serde JSON support"
```

---

### Task 3: Default exclude rules

**Files:**
- Create: `src-tauri/src/excludes.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod excludes;`)

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/excludes.rs`:

```rust
pub const DEFAULT_EXCLUDES: &[&str] = &[
    ".DS_Store",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    ".TemporaryItems",
    "._*",
    ".rsync-partial/",
];

/// Combine the global default excludes with per-pair excludes,
/// preserving order and deduplicating exact matches.
pub fn merged_excludes(per_pair: &[String]) -> Vec<String> {
    let mut out: Vec<String> = DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect();
    for e in per_pair {
        if !out.iter().any(|x| x == e) {
            out.push(e.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_always_present() {
        let merged = merged_excludes(&[]);
        assert!(merged.contains(&".DS_Store".to_string()));
        assert!(merged.contains(&"._*".to_string()));
        assert_eq!(merged.len(), DEFAULT_EXCLUDES.len());
    }

    #[test]
    fn user_excludes_appended() {
        let merged = merged_excludes(&["node_modules/".to_string(), "*.tmp".to_string()]);
        assert!(merged.contains(&"node_modules/".to_string()));
        assert!(merged.contains(&"*.tmp".to_string()));
    }

    #[test]
    fn duplicates_dropped() {
        let merged = merged_excludes(&[".DS_Store".to_string()]);
        let count = merged.iter().filter(|x| *x == ".DS_Store").count();
        assert_eq!(count, 1);
    }
}
```

Append to `src-tauri/src/lib.rs`:

```rust
pub mod excludes;
```

- [ ] **Step 2: Run test, verify pass**

```bash
cd src-tauri && cargo test --lib excludes::tests
```

Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/excludes.rs src-tauri/src/lib.rs
git commit -m "feat(excludes): default exclude list + per-pair merge"
```

---

### Task 4: pairs.json atomic read/write

**Files:**
- Modify: `src-tauri/src/pairs.rs` (append load/save functions + tests)

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/pairs.rs` (above the `#[cfg(test)]` block):

```rust
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
```

Inside the `#[cfg(test)]` block append:

```rust
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
```

Add to `src-tauri/Cargo.toml` `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 2: Run tests, verify failure**

```bash
cd src-tauri && cargo test --lib pairs::tests
```

Expected: FAIL initially if functions absent; after step 1 they should pass.

- [ ] **Step 3: Run tests, verify pass**

```bash
cd src-tauri && cargo test --lib pairs::tests
```

Expected: 5 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/pairs.rs src-tauri/Cargo.toml
git commit -m "feat(pairs): atomic save/load of pairs.json"
```

---

### Task 5: rsync command builder

**Files:**
- Create: `src-tauri/src/rsync.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod rsync;`)

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/rsync.rs`:

```rust
#[derive(Debug, Clone)]
pub struct RsyncConfig {
    /// Source path. For push: local dir. For pull: "user@host:remote/path/".
    /// Caller is responsible for trailing slash semantics.
    pub source: String,
    pub destination: String,
    pub excludes_file: Option<String>,
    pub bandwidth_limit_kbps: Option<u32>,
    pub mirror_mode: bool,
    pub dry_run: bool,
    pub timeout_seconds: u32,
}

impl Default for RsyncConfig {
    fn default() -> Self {
        Self {
            source: String::new(),
            destination: String::new(),
            excludes_file: None,
            bandwidth_limit_kbps: None,
            mirror_mode: false,
            dry_run: false,
            timeout_seconds: 300,
        }
    }
}

pub fn build_rsync_args(c: &RsyncConfig) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-az".into(),
        "--partial".into(),
        "--partial-dir=.rsync-partial".into(),
        "--info=progress2".into(),
        "--stats".into(),
        format!("--timeout={}", c.timeout_seconds),
    ];
    if let Some(file) = &c.excludes_file {
        args.push(format!("--exclude-from={}", file));
    }
    if c.mirror_mode {
        args.push("--delete".into());
    }
    if let Some(kbps) = c.bandwidth_limit_kbps {
        if kbps > 0 {
            args.push(format!("--bwlimit={}", kbps));
        }
    }
    if c.dry_run {
        args.push("--dry-run".into());
    }
    args.push(c.source.clone());
    args.push(c.destination.clone());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_args_present() {
        let cfg = RsyncConfig {
            source: "/a/".into(),
            destination: "u@h:/b/".into(),
            ..Default::default()
        };
        let a = build_rsync_args(&cfg);
        assert!(a.contains(&"-az".into()));
        assert!(a.contains(&"--partial".into()));
        assert!(a.contains(&"--partial-dir=.rsync-partial".into()));
        assert!(a.contains(&"--info=progress2".into()));
        assert!(a.contains(&"--stats".into()));
        assert!(a.contains(&"--timeout=300".into()));
        assert!(!a.iter().any(|x| x == "--delete"));
        assert!(!a.iter().any(|x| x.starts_with("--bwlimit")));
        assert!(!a.iter().any(|x| x == "--dry-run"));
        assert_eq!(a.last(), Some(&"u@h:/b/".to_string()));
        assert_eq!(a[a.len() - 2], "/a/");
    }

    #[test]
    fn mirror_mode_adds_delete() {
        let cfg = RsyncConfig { mirror_mode: true, ..Default::default() };
        let a = build_rsync_args(&cfg);
        assert!(a.contains(&"--delete".into()));
    }

    #[test]
    fn bwlimit_added_when_set_nonzero() {
        let cfg = RsyncConfig { bandwidth_limit_kbps: Some(2048), ..Default::default() };
        let a = build_rsync_args(&cfg);
        assert!(a.contains(&"--bwlimit=2048".into()));
    }

    #[test]
    fn bwlimit_skipped_when_zero() {
        let cfg = RsyncConfig { bandwidth_limit_kbps: Some(0), ..Default::default() };
        let a = build_rsync_args(&cfg);
        assert!(!a.iter().any(|x| x.starts_with("--bwlimit")));
    }

    #[test]
    fn dry_run_adds_flag() {
        let cfg = RsyncConfig { dry_run: true, ..Default::default() };
        let a = build_rsync_args(&cfg);
        assert!(a.contains(&"--dry-run".into()));
    }

    #[test]
    fn excludes_file_added_when_present() {
        let cfg = RsyncConfig { excludes_file: Some("/tmp/ex".into()), ..Default::default() };
        let a = build_rsync_args(&cfg);
        assert!(a.contains(&"--exclude-from=/tmp/ex".into()));
    }
}
```

Append to `src-tauri/src/lib.rs`:

```rust
pub mod rsync;
```

- [ ] **Step 2: Run tests, verify pass**

```bash
cd src-tauri && cargo test --lib rsync::tests
```

Expected: 6 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/rsync.rs src-tauri/src/lib.rs
git commit -m "feat(rsync): build args from RsyncConfig"
```

---

### Task 6: rsync progress line parser

**Files:**
- Modify: `src-tauri/src/rsync.rs` (append parser + tests)

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/rsync.rs` above its `#[cfg(test)]` block:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProgressUpdate {
    pub bytes_transferred: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<f32>,
    pub rate_bps: Option<u64>,
    pub eta_seconds: Option<u64>,
    pub current_file: Option<String>,
}

/// Parse one fragment of rsync output. Returns None if the fragment
/// is not a recognized progress or filename line.
///
/// `--info=progress2` prints lines that look like:
///   "       32,768   0%    0.00kB/s    0:00:00"
///   "    1,048,576  50%    1.23MB/s    0:00:02 (xfr#1, to-chk=2/4)"
/// Filenames appear on their own lines (no leading whitespace + digit).
pub fn parse_progress_line(line: &str) -> Option<ProgressUpdate> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return None;
    }

    // Heuristic: progress lines start with whitespace + a digit (after removing commas).
    let leading = trimmed.trim_start();
    let first_char = leading.chars().next()?;
    if !first_char.is_ascii_digit() {
        // Treat as a filename line (rsync prints filenames in verbose mode)
        return Some(ProgressUpdate {
            bytes_transferred: 0,
            total_bytes: None,
            percent: None,
            rate_bps: None,
            eta_seconds: None,
            current_file: Some(trimmed.to_string()),
        });
    }

    // Tokenize. Strip commas in numbers for portability.
    let tokens: Vec<&str> = leading.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }

    let bytes_str = tokens[0].replace(',', "");
    let bytes_transferred = bytes_str.parse::<u64>().ok()?;

    let percent = tokens[1]
        .trim_end_matches('%')
        .parse::<f32>()
        .ok();

    let rate_bps = parse_rate(tokens[2]);

    let eta_seconds = parse_eta(tokens[3]);

    Some(ProgressUpdate {
        bytes_transferred,
        total_bytes: None,
        percent,
        rate_bps,
        eta_seconds,
        current_file: None,
    })
}

fn parse_rate(s: &str) -> Option<u64> {
    // examples: "1.23MB/s", "512kB/s", "0.00kB/s"
    let s = s.trim_end_matches("/s");
    let (num_str, unit) = s
        .find(|c: char| c.is_alphabetic())
        .map(|i| s.split_at(i))?;
    let n: f64 = num_str.parse().ok()?;
    let mult = match unit {
        "B" => 1.0,
        "kB" | "KB" => 1_024.0,
        "MB" => 1_024.0 * 1_024.0,
        "GB" => 1_024.0 * 1_024.0 * 1_024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

fn parse_eta(s: &str) -> Option<u64> {
    // example: "0:01:23"
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let sec: u64 = parts[2].parse().ok()?;
    Some(h * 3600 + m * 60 + sec)
}
```

Append to the existing `mod tests` block:

```rust
    #[test]
    fn parses_basic_progress() {
        let line = "    1,048,576  50%    1.23MB/s    0:00:02";
        let p = parse_progress_line(line).unwrap();
        assert_eq!(p.bytes_transferred, 1_048_576);
        assert_eq!(p.percent, Some(50.0));
        assert!(p.rate_bps.unwrap() > 1_000_000);
        assert_eq!(p.eta_seconds, Some(2));
        assert!(p.current_file.is_none());
    }

    #[test]
    fn parses_progress_with_kb_rate() {
        let line = "       32,768   0%    512kB/s    0:00:10";
        let p = parse_progress_line(line).unwrap();
        assert_eq!(p.bytes_transferred, 32_768);
        assert_eq!(p.rate_bps, Some(512 * 1024));
        assert_eq!(p.eta_seconds, Some(10));
    }

    #[test]
    fn returns_filename_for_non_numeric_line() {
        let p = parse_progress_line("Documents/notes/abc.md").unwrap();
        assert_eq!(p.current_file.as_deref(), Some("Documents/notes/abc.md"));
    }

    #[test]
    fn returns_none_for_empty_line() {
        assert!(parse_progress_line("").is_none());
        assert!(parse_progress_line("\r\n").is_none());
    }
```

- [ ] **Step 2: Run tests, verify pass**

```bash
cd src-tauri && cargo test --lib rsync::tests
```

Expected: 10 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/rsync.rs
git commit -m "feat(rsync): parse --info=progress2 lines into ProgressUpdate"
```

---

### Task 7: Tailscale status JSON parser

**Files:**
- Create: `src-tauri/src/tailscale.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod tailscale;`)

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/tailscale.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TailnetDevice {
    pub hostname: String,
    pub tailscale_ip: String,
    pub user: String,
    pub os: String,
    pub online: bool,
    pub is_self: bool,
    pub ssh_enabled: bool,
}

/// Parse the JSON output of `tailscale status --json`.
/// Returns (self_device, peers).
pub fn parse_tailscale_status(json_str: &str) -> Result<(TailnetDevice, Vec<TailnetDevice>), serde_json::Error> {
    let v: serde_json::Value = serde_json::from_str(json_str)?;

    let self_obj = &v["Self"];
    let self_dev = parse_device(self_obj, true);

    let mut peers = Vec::new();
    if let Some(peer_map) = v["Peer"].as_object() {
        for (_id, p) in peer_map {
            peers.push(parse_device(p, false));
        }
    }
    // Sort peers by hostname for stable UI ordering.
    peers.sort_by(|a, b| a.hostname.cmp(&b.hostname));

    Ok((self_dev, peers))
}

fn parse_device(v: &serde_json::Value, is_self: bool) -> TailnetDevice {
    let hostname = v["HostName"].as_str().unwrap_or("").to_string();
    let tailscale_ip = v["TailscaleIPs"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let user = v["UserID"].as_u64().map(|n| n.to_string()).unwrap_or_default();
    let os = v["OS"].as_str().unwrap_or("").to_string();
    let online = v["Online"].as_bool().unwrap_or(false);
    let ssh_enabled = v["sshHostKeys"]
        .as_array()
        .map(|a| !a.is_empty())
        .or_else(|| {
            // some versions emit "Capabilities" containing "https://tailscale.com/cap/ssh"
            v["Capabilities"]
                .as_array()
                .map(|a| a.iter().any(|c| c.as_str().unwrap_or("").contains("/ssh")))
        })
        .unwrap_or(false);

    TailnetDevice {
        hostname,
        tailscale_ip,
        user,
        os,
        online,
        is_self,
        ssh_enabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> &'static str {
        r#"{
          "Self": {
            "HostName": "macbook-pro-2",
            "TailscaleIPs": ["100.127.149.33"],
            "UserID": 1234,
            "OS": "macOS",
            "Online": true,
            "sshHostKeys": ["ssh-ed25519 AAAA..."]
          },
          "Peer": {
            "n1": {
              "HostName": "wuhoujins-mac-mini",
              "TailscaleIPs": ["100.72.185.45"],
              "UserID": 1234,
              "OS": "macOS",
              "Online": true,
              "sshHostKeys": ["ssh-ed25519 BBBB..."]
            },
            "n2": {
              "HostName": "old-laptop",
              "TailscaleIPs": ["100.99.99.99"],
              "UserID": 1234,
              "OS": "linux",
              "Online": false,
              "sshHostKeys": []
            }
          }
        }"#
    }

    #[test]
    fn parses_self_and_peers() {
        let (me, peers) = parse_tailscale_status(fixture()).unwrap();
        assert_eq!(me.hostname, "macbook-pro-2");
        assert!(me.is_self);
        assert!(me.ssh_enabled);
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn peers_sorted_by_hostname() {
        let (_, peers) = parse_tailscale_status(fixture()).unwrap();
        assert_eq!(peers[0].hostname, "old-laptop");
        assert_eq!(peers[1].hostname, "wuhoujins-mac-mini");
    }

    #[test]
    fn ssh_disabled_when_no_keys() {
        let (_, peers) = parse_tailscale_status(fixture()).unwrap();
        let off = peers.iter().find(|p| p.hostname == "old-laptop").unwrap();
        assert!(!off.ssh_enabled);
    }
}
```

Append to `src-tauri/src/lib.rs`:

```rust
pub mod tailscale;
```

- [ ] **Step 2: Run tests, verify pass**

```bash
cd src-tauri && cargo test --lib tailscale::tests
```

Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tailscale.rs src-tauri/src/lib.rs
git commit -m "feat(tailscale): parse 'tailscale status --json' output"
```

---

## Phase 2: Rust Integration (subprocess + filesystem)

### Task 8: Tailscale CLI invocation

**Files:**
- Modify: `src-tauri/src/tailscale.rs` (append `fetch_status`)

- [ ] **Step 1: Append the function**

Append to `src-tauri/src/tailscale.rs` (above its `#[cfg(test)]` block):

```rust
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum TailscaleError {
    #[error("tailscale CLI not found in PATH")]
    NotInstalled,
    #[error("tailscale exited with status {0}: {1}")]
    NonZeroExit(i32, String),
    #[error("failed to invoke tailscale: {0}")]
    InvokeFailed(#[from] std::io::Error),
    #[error("failed to parse tailscale output: {0}")]
    ParseFailed(#[from] serde_json::Error),
}

/// Invoke `tailscale status --json` and parse it.
pub fn fetch_status() -> Result<(TailnetDevice, Vec<TailnetDevice>), TailscaleError> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                TailscaleError::NotInstalled
            } else {
                TailscaleError::InvokeFailed(e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(TailscaleError::NonZeroExit(
            output.status.code().unwrap_or(-1),
            stderr,
        ));
    }

    let json = String::from_utf8_lossy(&output.stdout);
    Ok(parse_tailscale_status(&json)?)
}
```

- [ ] **Step 2: Manual smoke test**

```bash
cd src-tauri && cargo build
```

Then write a tiny binary check:

```bash
cd src-tauri && cat > /tmp/ts_check.rs <<'EOF'
fn main() {
    match tailsync_lib::tailscale::fetch_status() {
        Ok((me, peers)) => {
            println!("self: {} ({})", me.hostname, me.tailscale_ip);
            for p in peers { println!("peer: {} online={}", p.hostname, p.online); }
        }
        Err(e) => eprintln!("error: {}", e),
    }
}
EOF
```

Skip the manual binary — instead verify by adding a `#[ignore]` integration test:

In `src-tauri/src/tailscale.rs` `#[cfg(test)] mod tests {}` append:

```rust
    #[test]
    #[ignore = "requires tailscale installed and logged in"]
    fn fetch_real_status() {
        let (me, peers) = fetch_status().expect("tailscale status failed");
        println!("self: {}, peers: {}", me.hostname, peers.len());
        assert!(!me.hostname.is_empty());
    }
```

Run it deliberately:

```bash
cd src-tauri && cargo test --lib tailscale::tests::fetch_real_status -- --ignored --nocapture
```

Expected: prints your own hostname and peer count.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/tailscale.rs
git commit -m "feat(tailscale): fetch_status invokes the CLI"
```

---

### Task 9: Environment check

**Files:**
- Create: `src-tauri/src/env_check.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod env_check;`)

- [ ] **Step 1: Implement and test**

In `src-tauri/src/env_check.rs`:

```rust
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct EnvCheckResult {
    pub tailscale_installed: bool,
    pub tailscale_logged_in: bool,
    pub tailscale_ssh_enabled: bool,
    pub self_hostname: Option<String>,
    pub error_detail: Option<String>,
}

pub fn check_environment() -> EnvCheckResult {
    let installed = which_tailscale();
    if !installed {
        return EnvCheckResult {
            tailscale_installed: false,
            tailscale_logged_in: false,
            tailscale_ssh_enabled: false,
            self_hostname: None,
            error_detail: Some("tailscale CLI not found in PATH".into()),
        };
    }

    match crate::tailscale::fetch_status() {
        Ok((me, _)) => EnvCheckResult {
            tailscale_installed: true,
            tailscale_logged_in: !me.hostname.is_empty() && !me.tailscale_ip.is_empty(),
            tailscale_ssh_enabled: me.ssh_enabled,
            self_hostname: Some(me.hostname),
            error_detail: None,
        },
        Err(e) => EnvCheckResult {
            tailscale_installed: true,
            tailscale_logged_in: false,
            tailscale_ssh_enabled: false,
            self_hostname: None,
            error_detail: Some(e.to_string()),
        },
    }
}

fn which_tailscale() -> bool {
    Command::new("which")
        .arg("tailscale")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires real environment"]
    fn check_runs_without_panic() {
        let r = check_environment();
        println!("{:?}", r);
    }
}
```

Append to `src-tauri/src/lib.rs`:

```rust
pub mod env_check;
```

- [ ] **Step 2: Verify it compiles**

```bash
cd src-tauri && cargo build
```

Expected: builds clean.

- [ ] **Step 3: Run the ignored test manually**

```bash
cd src-tauri && cargo test --lib env_check::tests -- --ignored --nocapture
```

Expected: prints `EnvCheckResult { tailscale_installed: true, ... }` reflecting your real state.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/env_check.rs src-tauri/src/lib.rs
git commit -m "feat(env_check): detect tailscale install / login / SSH"
```

---

### Task 10: Sync executor (spawn rsync, emit progress events)

**Files:**
- Create: `src-tauri/src/sync.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod sync;`)

- [ ] **Step 1: Write the executor**

In `src-tauri/src/sync.rs`:

```rust
use crate::rsync::{build_rsync_args, parse_progress_line, ProgressUpdate, RsyncConfig};
use serde::Serialize;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

#[derive(Debug, Clone, Serialize)]
pub struct DryRunSummary {
    pub files_to_copy: u32,
    pub files_to_delete: u32,
    pub files_to_update: u32,
    pub total_bytes: u64,
    pub file_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub exit_code: i32,
    pub message: String,
    pub stderr_tail: String,
}

pub type ProgressCallback = Arc<dyn Fn(ProgressUpdate) + Send + Sync>;

/// Run rsync in dry-run mode, parse the --stats output into a DryRunSummary.
pub async fn run_dry_run(config: &RsyncConfig) -> std::io::Result<DryRunSummary> {
    let mut cfg = config.clone();
    cfg.dry_run = true;
    let args = build_rsync_args(&cfg);

    let output = Command::new("rsync")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("rsync dry-run failed: {}", stderr),
        ));
    }

    Ok(parse_dry_run_output(&stdout))
}

pub fn parse_dry_run_output(s: &str) -> DryRunSummary {
    let mut summary = DryRunSummary {
        files_to_copy: 0,
        files_to_delete: 0,
        files_to_update: 0,
        total_bytes: 0,
        file_list: Vec::new(),
    };

    for line in s.lines() {
        let line = line.trim_end();
        // rsync -i style itemized lines start with "*deleting" or ">f+++++++"
        if line.starts_with("*deleting") {
            summary.files_to_delete += 1;
            if let Some(name) = line.split_whitespace().nth(1) {
                summary.file_list.push(format!("D {}", name));
            }
        } else if line.starts_with(">f+++++++++") {
            summary.files_to_copy += 1;
            if let Some(name) = line.split_whitespace().nth(1) {
                summary.file_list.push(format!("+ {}", name));
            }
        } else if line.starts_with(">f") {
            summary.files_to_update += 1;
            if let Some(name) = line.split_whitespace().nth(1) {
                summary.file_list.push(format!("M {}", name));
            }
        } else if let Some(rest) = line.strip_prefix("Total transferred file size:") {
            // example: "Total transferred file size: 1,234,567 bytes"
            let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            summary.total_bytes = digits.parse().unwrap_or(0);
        }
    }
    summary
}

/// Spawn rsync, stream progress to the callback. Returns the child handle so
/// the caller can store it for cancellation.
pub async fn spawn_sync(
    config: &RsyncConfig,
    progress: ProgressCallback,
) -> std::io::Result<(Child, Arc<Mutex<String>>)> {
    let args = build_rsync_args(config);
    let mut child = Command::new("rsync")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stderr_buffer = Arc::new(Mutex::new(String::new()));

    // stdout: progress lines
    let progress_clone = progress.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();
        // rsync uses \r to overwrite progress; treat both \r and \n as record separators.
        loop {
            buf.clear();
            let n = match read_until_either(&mut reader, &mut buf).await {
                Ok(n) => n,
                Err(_) => break,
            };
            if n == 0 { break; }
            let line = String::from_utf8_lossy(&buf).to_string();
            if let Some(p) = parse_progress_line(&line) {
                (progress_clone)(p);
            }
        }
    });

    // stderr: collect into buffer
    let stderr_buf_clone = stderr_buffer.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 { break; }
            let mut buf = stderr_buf_clone.lock().unwrap();
            buf.push_str(&line);
            // Cap at 16KB
            if buf.len() > 16 * 1024 {
                let from = buf.len() - 16 * 1024;
                *buf = buf[from..].to_string();
            }
        }
    });

    Ok((child, stderr_buffer))
}

async fn read_until_either<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    buf: &mut Vec<u8>,
) -> std::io::Result<usize> {
    use tokio::io::AsyncReadExt;
    let mut byte = [0u8; 1];
    let mut total = 0;
    loop {
        let n = reader.read(&mut byte).await?;
        if n == 0 { return Ok(total); }
        total += 1;
        buf.push(byte[0]);
        if byte[0] == b'\n' || byte[0] == b'\r' { return Ok(total); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dry_run_summary() {
        let sample = r#"
sending incremental file list
>f+++++++++ a.txt
>f.st...... b.txt
*deleting old.txt

Number of files: 3 (reg: 3)
Number of created files: 1 (reg: 1)
Number of deleted files: 1
Number of regular files transferred: 2
Total file size: 4,096 bytes
Total transferred file size: 4,096 bytes
"#;
        let s = parse_dry_run_output(sample);
        assert_eq!(s.files_to_copy, 1);
        assert_eq!(s.files_to_update, 1);
        assert_eq!(s.files_to_delete, 1);
        assert_eq!(s.total_bytes, 4096);
        assert_eq!(s.file_list.len(), 3);
    }
}
```

Note: `--info=progress2 --stats` does not produce itemized `>f+++++++++` lines by default. To make `parse_dry_run_output` work, we must add `--itemize-changes` to the rsync args specifically for dry-run. Update `build_rsync_args` to append `-i` when `dry_run` is true:

In `src-tauri/src/rsync.rs`, in `build_rsync_args`, just before the `dry_run` push:

```rust
    if c.dry_run {
        args.push("-i".into());
        args.push("--dry-run".into());
    }
```

And update the corresponding test in `rsync::tests::dry_run_adds_flag`:

```rust
    #[test]
    fn dry_run_adds_flag() {
        let cfg = RsyncConfig { dry_run: true, ..Default::default() };
        let a = build_rsync_args(&cfg);
        assert!(a.contains(&"--dry-run".into()));
        assert!(a.contains(&"-i".into()));
    }
```

Append to `src-tauri/src/lib.rs`:

```rust
pub mod sync;
```

- [ ] **Step 2: Run tests, verify pass**

```bash
cd src-tauri && cargo test --lib
```

Expected: all unit tests pass.

- [ ] **Step 3: Manual integration test (optional, requires real Tailscale + remote)**

Create two test directories on your two machines, then from a Rust binary or REPL invoke `run_dry_run` with a config like:

```rust
let cfg = RsyncConfig {
    source: "/Users/wuhoujin/Documents/test_src/".into(),
    destination: "wuhoujin@wuhoujins-mac-mini:/Users/wuhoujin/Documents/test_dst/".into(),
    timeout_seconds: 60,
    ..Default::default()
};
```

Defer this manual check to Task 26 (end-to-end).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/sync.rs src-tauri/src/rsync.rs src-tauri/src/lib.rs
git commit -m "feat(sync): dry-run summary + spawn_sync with progress streaming"
```

---

### Task 11: Sync cancellation + manager

**Files:**
- Modify: `src-tauri/src/sync.rs` (append SyncManager)

- [ ] **Step 1: Append**

Append to `src-tauri/src/sync.rs` (above its `#[cfg(test)]` block):

```rust
use std::collections::HashMap;
use tokio::sync::Mutex as AsyncMutex;

pub type SyncId = String;

pub struct SyncManager {
    inner: AsyncMutex<HashMap<SyncId, Child>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self { inner: AsyncMutex::new(HashMap::new()) }
    }

    pub async fn register(&self, id: SyncId, child: Child) {
        self.inner.lock().await.insert(id, child);
    }

    pub async fn cancel(&self, id: &str) -> bool {
        let mut map = self.inner.lock().await;
        if let Some(mut child) = map.remove(id) {
            #[cfg(unix)]
            {
                if let Some(pid) = child.id() {
                    unsafe {
                        libc::kill(pid as i32, libc::SIGTERM);
                    }
                }
            }
            let _ = child.wait().await;
            true
        } else {
            false
        }
    }

    pub async fn wait_and_remove(&self, id: &str) -> Option<std::process::ExitStatus> {
        let mut map = self.inner.lock().await;
        let mut child = map.remove(id)?;
        drop(map);
        child.wait().await.ok()
    }
}

impl Default for SyncManager {
    fn default() -> Self { Self::new() }
}
```

Add to `src-tauri/Cargo.toml` `[dependencies]`:

```toml
libc = "0.2"
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build
```

Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/sync.rs src-tauri/Cargo.toml
git commit -m "feat(sync): SyncManager for tracking and cancelling running rsyncs"
```

---

### Task 12: Remote helpers (ssh ls / mkdir)

**Files:**
- Create: `src-tauri/src/remote.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod remote;`)

- [ ] **Step 1: Implement**

In `src-tauri/src/remote.rs`:

```rust
use serde::Serialize;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
pub enum PathProbeResult {
    Exists,
    Missing,
    SshFailed(String),
}

pub async fn probe_remote_path(user: &str, host: &str, path: &str) -> PathProbeResult {
    // Use 'test -d' to check directory existence.
    let target = format!("{}@{}", user, host);
    let cmd = format!(
        "test -e {} && echo OK || echo MISSING",
        shell_escape(path)
    );
    let output = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5", &target, &cmd])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.contains("OK") {
                PathProbeResult::Exists
            } else {
                PathProbeResult::Missing
            }
        }
        Ok(o) => {
            PathProbeResult::SshFailed(String::from_utf8_lossy(&o.stderr).into_owned())
        }
        Err(e) => PathProbeResult::SshFailed(e.to_string()),
    }
}

pub async fn create_remote_dir(user: &str, host: &str, path: &str) -> Result<(), String> {
    let target = format!("{}@{}", user, host);
    let cmd = format!("mkdir -p {}", shell_escape(path));
    let output = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5", &target, &cmd])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// Single-quote escape for POSIX shells.
fn shell_escape(s: &str) -> String {
    let escaped = s.replace('\'', r"'\''");
    format!("'{}'", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_escape_handles_quotes() {
        assert_eq!(shell_escape("/a/b"), "'/a/b'");
        assert_eq!(shell_escape("/a's/b"), r"'/a'\''s/b'");
    }
}
```

Append to `src-tauri/src/lib.rs`:

```rust
pub mod remote;
```

- [ ] **Step 2: Run tests**

```bash
cd src-tauri && cargo test --lib remote::tests
```

Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/remote.rs src-tauri/src/lib.rs
git commit -m "feat(remote): probe path and create dir over ssh"
```

---

## Phase 3: Tauri Command Bridge

### Task 13: Errors module + AppState

**Files:**
- Create: `src-tauri/src/errors.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: Define error type**

In `src-tauri/src/errors.rs`:

```rust
use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error("io: {0}")]
    Io(String),
    #[error("tailscale: {0}")]
    Tailscale(String),
    #[error("ssh: {0}")]
    Ssh(String),
    #[error("rsync: {0}")]
    Rsync(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::Io(e.to_string()) }
}

impl From<crate::tailscale::TailscaleError> for AppError {
    fn from(e: crate::tailscale::TailscaleError) -> Self { AppError::Tailscale(e.to_string()) }
}

pub type AppResult<T> = Result<T, AppError>;
```

Append to `src-tauri/src/lib.rs`:

```rust
pub mod errors;
pub mod commands;
```

- [ ] **Step 2: Define AppState in main.rs**

Replace `src-tauri/src/main.rs` content with:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
use tailsync_lib::pairs::DirectoryPair;
use tailsync_lib::sync::SyncManager;

pub struct AppState {
    pub pairs: Mutex<Vec<DirectoryPair>>,
    pub pairs_path: PathBuf,
    pub sync_manager: SyncManager,
}

fn main() {
    let pairs_path = config_dir().join("pairs.json");
    let pairs = tailsync_lib::pairs::load_pairs(&pairs_path).unwrap_or_default();

    let state = AppState {
        pairs: Mutex::new(pairs),
        pairs_path,
        sync_manager: SyncManager::new(),
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
```

Set crate name. In `src-tauri/Cargo.toml` ensure:

```toml
[package]
name = "tailsync"
version = "0.1.0"
edition = "2021"

[lib]
name = "tailsync_lib"
path = "src/lib.rs"

[[bin]]
name = "tailsync"
path = "src/main.rs"

[dependencies]
# ... existing ...
dirs = "5"
```

- [ ] **Step 3: Verify build**

```bash
cd src-tauri && cargo build
```

Expected: build error because `commands.rs` is empty / undefined functions. That is fine; Task 14 fills it in.

- [ ] **Step 4: Stub commands.rs to make it compile**

Create `src-tauri/src/commands.rs` with placeholder stubs:

```rust
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
```

```bash
cd src-tauri && cargo build
```

Expected: clean build. (The `unused State` import will warn — fine for now.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/errors.rs src-tauri/src/main.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat: AppError + AppState scaffolding + stub commands"
```

---

### Task 14: Pair CRUD commands

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Replace stubs**

In `src-tauri/src/commands.rs`, replace the four pair-stub commands:

```rust
use crate::errors::{AppError, AppResult};
use crate::pairs::{save_pairs, DirectoryPair};
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
```

Keep the other stubs in place — they are still referenced by `main.rs`.

Note: `AppState` lives in `main.rs` but `commands.rs` references it via `crate::AppState`. Since `commands.rs` is in the lib crate but `AppState` is in the bin crate, this won't compile as written. Fix by moving `AppState` into the lib:

In `src-tauri/src/lib.rs` add:

```rust
use std::path::PathBuf;
use std::sync::Mutex;
use crate::pairs::DirectoryPair;
use crate::sync::SyncManager;

pub struct AppState {
    pub pairs: Mutex<Vec<DirectoryPair>>,
    pub pairs_path: PathBuf,
    pub sync_manager: SyncManager,
}
```

In `src-tauri/src/main.rs`, replace the local `pub struct AppState` with:

```rust
use tailsync_lib::AppState;
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build
```

Expected: clean build.

- [ ] **Step 3: Manual smoke**

```bash
npm run tauri dev
```

Open DevTools in the app window, in the console:

```js
const { invoke } = window.__TAURI__.core;
await invoke('list_pairs');                     // []
await invoke('add_pair', { pair: {
  id: '', name: 'test', local_path: '/tmp', remote_host: 'mac-mini',
  remote_user: 'wuhoujin', remote_path: '/tmp', excludes: [],
  bandwidth_limit_kbps: null, mirror_mode: false, last_sync: null
}});
await invoke('list_pairs');                     // [ {...} ]
```

Verify `~/Library/Application Support/tailsync/pairs.json` was written.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/main.rs
git commit -m "feat(commands): pair CRUD wired to pairs.json"
```

---

### Task 15: Tailscale, env, and remote-validation commands

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Implement**

In `src-tauri/src/commands.rs`, replace those four stub commands:

```rust
use crate::env_check::{check_environment, EnvCheckResult};
use crate::remote::{create_remote_dir as remote_mkdir, probe_remote_path as remote_probe, PathProbeResult};
use crate::tailscale::{fetch_status, TailnetDevice};

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
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build
```

Expected: clean.

- [ ] **Step 3: Manual smoke (in DevTools)**

```js
await invoke('env_check');
await invoke('list_tailnet_devices');
await invoke('probe_remote_path', { user: 'wuhoujin', host: 'wuhoujins-mac-mini', path: '/tmp' });
```

Each should return real data.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(commands): tailnet listing, env check, remote path helpers"
```

---

### Task 16: Sync commands (dry-run, start with events, cancel)

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Implement**

Replace the three sync stubs in `src-tauri/src/commands.rs`:

```rust
use crate::excludes::merged_excludes;
use crate::rsync::{ProgressUpdate, RsyncConfig};
use crate::sync::{run_dry_run, spawn_sync, DryRunSummary, SyncResult};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::Arc;
use tauri::Emitter;
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
    let manager_handle = state.sync_manager.clone_arc();
    let task_id_done = task_id.clone();
    tokio::spawn(async move {
        let exit = manager_handle.wait_and_remove(&task_id_done).await;
        let stderr = stderr_buf.lock().unwrap().clone();
        let result = SyncResult {
            exit_code: exit.and_then(|s| s.code()).unwrap_or(-1),
            message: if exit.map(|s| s.success()).unwrap_or(false) { "完成".into() } else { "失败".into() },
            stderr_tail: stderr,
        };
        let _ = app_for_done.emit(&format!("sync-done:{}", task_id_done), result);
        let _ = std::fs::remove_file(&tmp);
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn cancel_sync(state: State<'_, AppState>, task_id: String) -> AppResult<bool> {
    Ok(state.sync_manager.cancel(&task_id).await)
}
```

The `clone_arc` helper is needed because `SyncManager` lives behind `State`. Update `SyncManager` in `src-tauri/src/sync.rs` to be wrappable: change `AppState` to hold `Arc<SyncManager>`:

In `src-tauri/src/lib.rs`:

```rust
use std::sync::Arc;
// ...
pub struct AppState {
    pub pairs: Mutex<Vec<DirectoryPair>>,
    pub pairs_path: PathBuf,
    pub sync_manager: Arc<SyncManager>,
}
```

In `src-tauri/src/main.rs`:

```rust
let state = AppState {
    pairs: Mutex::new(pairs),
    pairs_path,
    sync_manager: Arc::new(SyncManager::new()),
};
```

In `src-tauri/src/sync.rs` add:

```rust
impl SyncManager {
    pub fn clone_arc(self: &std::sync::Arc<Self>) -> std::sync::Arc<Self> {
        std::sync::Arc::clone(self)
    }
}
```

And in `commands.rs`, replace the `state.sync_manager.clone_arc()` line with `state.sync_manager.clone()` since `Arc<SyncManager>` already implements `Clone`. (Remove the `clone_arc` helper if unused.)

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build
```

Expected: clean. Resolve any lifetime / borrow issues by adjusting lock guard scopes (drop `guard` before `.await`).

- [ ] **Step 3: Manual smoke**

In DevTools (with one pair pre-saved):

```js
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const pairs = await invoke('list_pairs');
const id = pairs[0].id;

const summary = await invoke('dry_run_sync', { req: { pair_id: id, direction: 'push' } });
console.log(summary);

const taskId = await invoke('start_sync', { req: { pair_id: id, direction: 'push' } });
const off1 = await listen(`sync-progress:${taskId}`, e => console.log('progress', e.payload));
const off2 = await listen(`sync-done:${taskId}`, e => console.log('done', e.payload));
```

Expected: `summary` shows file counts; progress events stream; a final done event arrives.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/sync.rs src-tauri/src/lib.rs src-tauri/src/main.rs
git commit -m "feat(commands): dry-run, start_sync with events, cancel_sync"
```

---

### Task 17: Open system settings (Full Disk Access)

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Implement**

Replace the `open_full_disk_access` stub:

```rust
#[tauri::command]
pub async fn open_full_disk_access() -> AppResult<()> {
    let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";
    std::process::Command::new("open")
        .arg(url)
        .status()
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}
```

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build
```

Expected: clean.

- [ ] **Step 3: Manual smoke**

```js
await invoke('open_full_disk_access');
```

Expected: System Settings opens to the Full Disk Access pane.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(commands): open Full Disk Access settings pane"
```

---

## Phase 4: Vue Frontend

### Task 18: Shared types and Tauri wrappers

**Files:**
- Create: `src/types.ts`
- Create: `src/lib/tauri.ts`
- Create: `src/lib/format.ts`

- [ ] **Step 1: Create types**

In `src/types.ts`:

```ts
export type SyncDirection = 'push' | 'pull';
export type SyncStatus = 'success' | 'failed' | 'interrupted';

export interface LastSync {
  direction: SyncDirection;
  timestamp: number;
  status: SyncStatus;
  message: string;
}

export interface DirectoryPair {
  id: string;
  name: string;
  local_path: string;
  remote_host: string;
  remote_user: string;
  remote_path: string;
  excludes: string[];
  bandwidth_limit_kbps: number | null;
  mirror_mode: boolean;
  last_sync: LastSync | null;
}

export interface TailnetDevice {
  hostname: string;
  tailscale_ip: string;
  user: string;
  os: string;
  online: boolean;
  is_self: boolean;
  ssh_enabled: boolean;
}

export interface EnvCheckResult {
  tailscale_installed: boolean;
  tailscale_logged_in: boolean;
  tailscale_ssh_enabled: boolean;
  self_hostname: string | null;
  error_detail: string | null;
}

export interface DryRunSummary {
  files_to_copy: number;
  files_to_delete: number;
  files_to_update: number;
  total_bytes: number;
  file_list: string[];
}

export interface ProgressUpdate {
  bytes_transferred: number;
  total_bytes: number | null;
  percent: number | null;
  rate_bps: number | null;
  eta_seconds: number | null;
  current_file: string | null;
}

export interface SyncResult {
  exit_code: number;
  message: string;
  stderr_tail: string;
}

export type PathProbeResult =
  | 'Exists'
  | 'Missing'
  | { SshFailed: string };
```

- [ ] **Step 2: Create Tauri wrapper**

In `src/lib/tauri.ts`:

```ts
import { invoke as rawInvoke } from '@tauri-apps/api/core';
import { listen as rawListen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  DirectoryPair, TailnetDevice, EnvCheckResult,
  DryRunSummary, ProgressUpdate, SyncResult, PathProbeResult,
  SyncDirection,
} from '../types';

export const api = {
  listPairs: () => rawInvoke<DirectoryPair[]>('list_pairs'),
  addPair: (pair: DirectoryPair) => rawInvoke<DirectoryPair>('add_pair', { pair }),
  updatePair: (pair: DirectoryPair) => rawInvoke<DirectoryPair>('update_pair', { pair }),
  deletePair: (id: string) => rawInvoke<void>('delete_pair', { id }),

  listTailnetDevices: () => rawInvoke<TailnetDevice[]>('list_tailnet_devices'),
  envCheck: () => rawInvoke<EnvCheckResult>('env_check'),

  probeRemotePath: (user: string, host: string, path: string) =>
    rawInvoke<PathProbeResult>('probe_remote_path', { user, host, path }),
  createRemoteDir: (user: string, host: string, path: string) =>
    rawInvoke<void>('create_remote_dir', { user, host, path }),

  dryRun: (pairId: string, direction: SyncDirection) =>
    rawInvoke<DryRunSummary>('dry_run_sync', { req: { pair_id: pairId, direction } }),
  startSync: (pairId: string, direction: SyncDirection) =>
    rawInvoke<string>('start_sync', { req: { pair_id: pairId, direction } }),
  cancelSync: (taskId: string) =>
    rawInvoke<boolean>('cancel_sync', { taskId }),

  openFullDiskAccess: () => rawInvoke<void>('open_full_disk_access'),
};

export async function onSyncProgress(taskId: string, cb: (p: ProgressUpdate) => void): Promise<UnlistenFn> {
  return rawListen<ProgressUpdate>(`sync-progress:${taskId}`, e => cb(e.payload));
}

export async function onSyncDone(taskId: string, cb: (r: SyncResult) => void): Promise<UnlistenFn> {
  return rawListen<SyncResult>(`sync-done:${taskId}`, e => cb(e.payload));
}
```

- [ ] **Step 3: Create formatters**

In `src/lib/format.ts`:

```ts
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function formatRate(bps: number): string {
  return `${formatBytes(bps)}/s`;
}

export function formatRelativeTime(unixSec: number): string {
  const diff = Date.now() / 1000 - unixSec;
  if (diff < 60) return '刚刚';
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

export function formatEta(sec: number | null): string {
  if (sec == null) return '--';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  return `${m}:${String(s).padStart(2, '0')}`;
}
```

- [ ] **Step 4: Add Vitest config**

Create `vitest.config.ts`:

```ts
import { defineConfig } from 'vitest/config';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'jsdom',
    globals: true,
  },
});
```

Add to `package.json` `scripts`:

```json
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 5: Add format tests**

Create `src/lib/format.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { formatBytes, formatRate, formatEta } from './format';

describe('formatters', () => {
  it('formats bytes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1024 * 1024 * 5.5)).toBe('5.5 MB');
  });

  it('formats rate', () => {
    expect(formatRate(2048)).toBe('2.0 KB/s');
  });

  it('formats eta', () => {
    expect(formatEta(null)).toBe('--');
    expect(formatEta(65)).toBe('1:05');
    expect(formatEta(3661)).toBe('1:01:01');
  });
});
```

```bash
npm test
```

Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add src/types.ts src/lib/ vitest.config.ts package.json
git commit -m "feat(frontend): types, tauri wrapper, formatters + vitest setup"
```

---

### Task 19: Pinia store for pairs

**Files:**
- Create: `src/stores/pairs.ts`

- [ ] **Step 1: Create store**

In `src/stores/pairs.ts`:

```ts
import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '../lib/tauri';
import type { DirectoryPair } from '../types';

export const usePairsStore = defineStore('pairs', () => {
  const pairs = ref<DirectoryPair[]>([]);
  const loading = ref(false);

  async function refresh() {
    loading.value = true;
    try {
      pairs.value = await api.listPairs();
    } finally {
      loading.value = false;
    }
  }

  async function add(pair: DirectoryPair) {
    const created = await api.addPair(pair);
    pairs.value.push(created);
    return created;
  }

  async function update(pair: DirectoryPair) {
    const updated = await api.updatePair(pair);
    const idx = pairs.value.findIndex(p => p.id === pair.id);
    if (idx >= 0) pairs.value[idx] = updated;
    return updated;
  }

  async function remove(id: string) {
    await api.deletePair(id);
    pairs.value = pairs.value.filter(p => p.id !== id);
  }

  return { pairs, loading, refresh, add, update, remove };
});
```

In `src/main.ts` ensure Pinia is registered:

```ts
import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';

createApp(App).use(createPinia()).mount('#app');
```

- [ ] **Step 2: Verify it compiles**

```bash
npm run dev
```

Expected: Vite dev server starts without errors. Stop with Ctrl-C.

- [ ] **Step 3: Commit**

```bash
git add src/stores/pairs.ts src/main.ts
git commit -m "feat(frontend): Pinia store for directory pairs"
```

---

### Task 20: EnvCheck guard component

**Files:**
- Create: `src/components/EnvCheck.vue`

- [ ] **Step 1: Implement**

In `src/components/EnvCheck.vue`:

```vue
<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { api } from '../lib/tauri';
import type { EnvCheckResult } from '../types';

const result = ref<EnvCheckResult | null>(null);
const checking = ref(false);

async function check() {
  checking.value = true;
  try {
    result.value = await api.envCheck();
  } finally {
    checking.value = false;
  }
}

onMounted(check);

const emit = defineEmits<{ ready: [hostname: string] }>();

function proceed() {
  if (result.value?.tailscale_installed && result.value.tailscale_logged_in && result.value.tailscale_ssh_enabled) {
    emit('ready', result.value.self_hostname || '');
  }
}

// Auto-emit when all green
import { watch } from 'vue';
watch(result, () => proceed(), { immediate: false });
</script>

<template>
  <div class="env-check">
    <h2>环境检查</h2>
    <div v-if="checking">检测中...</div>
    <div v-else-if="result">
      <ul>
        <li :class="{ ok: result.tailscale_installed }">
          Tailscale CLI 已安装：{{ result.tailscale_installed ? '是' : '否' }}
        </li>
        <li v-if="result.tailscale_installed" :class="{ ok: result.tailscale_logged_in }">
          已登录 tailnet：{{ result.tailscale_logged_in ? '是' : '否' }}
        </li>
        <li v-if="result.tailscale_logged_in" :class="{ ok: result.tailscale_ssh_enabled }">
          Tailscale SSH 已启用：{{ result.tailscale_ssh_enabled ? '是' : '否' }}
        </li>
      </ul>

      <div v-if="!result.tailscale_installed" class="hint">
        请先安装 Tailscale：<a href="https://tailscale.com/download" target="_blank">tailscale.com/download</a>
      </div>
      <div v-else-if="!result.tailscale_logged_in" class="hint">
        请在菜单栏 Tailscale 图标登录你的 tailnet。
      </div>
      <div v-else-if="!result.tailscale_ssh_enabled" class="hint">
        请在终端执行：<code>tailscale set --ssh</code>
      </div>

      <button @click="check">重新检测</button>
    </div>
  </div>
</template>

<style scoped>
.env-check { padding: 24px; max-width: 480px; margin: 0 auto; }
ul { list-style: none; padding: 0; }
li { padding: 6px 0; color: #c33; }
li.ok { color: #2a7; }
li.ok::before { content: '✓ '; }
li:not(.ok)::before { content: '✗ '; }
.hint { margin-top: 12px; padding: 12px; background: #fff8e1; border-radius: 4px; }
button { margin-top: 16px; padding: 6px 16px; }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/EnvCheck.vue
git commit -m "feat(ui): EnvCheck first-run guard"
```

---

### Task 21: StatusBar component

**Files:**
- Create: `src/components/StatusBar.vue`

- [ ] **Step 1: Implement**

In `src/components/StatusBar.vue`:

```vue
<script setup lang="ts">
defineProps<{ hostname: string; tailscaleConnected: boolean }>();
</script>

<template>
  <div class="status-bar">
    <div class="left">
      <span class="label">本机：</span>
      <span class="value">{{ hostname || '未知' }}</span>
    </div>
    <div class="right">
      <span class="label">Tailscale：</span>
      <span :class="['dot', tailscaleConnected ? 'ok' : 'bad']"></span>
      <span>{{ tailscaleConnected ? '已连接' : '未连接' }}</span>
    </div>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  justify-content: space-between;
  padding: 8px 16px;
  background: #f5f5f7;
  border-bottom: 1px solid #e0e0e2;
  font-size: 13px;
}
.label { color: #666; }
.value { font-weight: 500; }
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin: 0 6px; }
.dot.ok { background: #2a7; }
.dot.bad { background: #c33; }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/StatusBar.vue
git commit -m "feat(ui): StatusBar"
```

---

### Task 22: PairList component

**Files:**
- Create: `src/components/PairList.vue`

- [ ] **Step 1: Implement**

In `src/components/PairList.vue`:

```vue
<script setup lang="ts">
import { computed } from 'vue';
import { usePairsStore } from '../stores/pairs';
import { formatRelativeTime } from '../lib/format';
import type { DirectoryPair, SyncDirection } from '../types';

const store = usePairsStore();

const emit = defineEmits<{
  add: [];
  edit: [pair: DirectoryPair];
  sync: [pair: DirectoryPair, direction: SyncDirection];
}>();

function lastSyncText(p: DirectoryPair): string {
  if (!p.last_sync) return '尚未同步';
  const dir = p.last_sync.direction === 'push' ? '推过去' : '拉回来';
  const when = formatRelativeTime(p.last_sync.timestamp);
  const status = { success: '已完成', failed: '失败', interrupted: '已中断' }[p.last_sync.status];
  return `${dir} · ${when} ${status}`;
}

function statusClass(p: DirectoryPair): string {
  if (!p.last_sync) return '';
  return p.last_sync.status;
}
</script>

<template>
  <div class="pair-list">
    <div v-if="store.pairs.length === 0" class="empty">
      还没有目录对。点击下方"+ 新建目录对"开始。
    </div>
    <table v-else>
      <thead>
        <tr>
          <th>名称</th>
          <th>路径</th>
          <th>上次同步</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="p in store.pairs" :key="p.id">
          <td><strong>{{ p.name }}</strong></td>
          <td class="paths">
            <code>{{ p.local_path }}</code>
            <span class="arrow">↔</span>
            <code>{{ p.remote_host }}:{{ p.remote_path }}</code>
          </td>
          <td :class="['last-sync', statusClass(p)]">{{ lastSyncText(p) }}</td>
          <td class="actions">
            <button class="push" @click="emit('sync', p, 'push')">推过去</button>
            <button class="pull" @click="emit('sync', p, 'pull')">拉回来</button>
            <button class="edit" @click="emit('edit', p)">⋯</button>
          </td>
        </tr>
      </tbody>
    </table>
    <div class="footer">
      <button class="add" @click="emit('add')">+ 新建目录对</button>
    </div>
  </div>
</template>

<style scoped>
.pair-list { padding: 16px; }
.empty { padding: 48px; text-align: center; color: #888; }
table { width: 100%; border-collapse: collapse; }
th, td { padding: 12px 8px; text-align: left; border-bottom: 1px solid #eee; vertical-align: middle; font-size: 13px; }
.paths { font-family: ui-monospace, monospace; color: #555; }
.paths code { background: #f5f5f7; padding: 2px 6px; border-radius: 3px; }
.arrow { margin: 0 6px; color: #888; }
.last-sync.success { color: #2a7; }
.last-sync.failed { color: #c33; }
.last-sync.interrupted { color: #d80; }
.actions { white-space: nowrap; text-align: right; }
.actions button { margin-left: 6px; padding: 5px 12px; }
.actions .push { background: #2563eb; color: white; border: 0; border-radius: 4px; }
.actions .pull { background: #16a34a; color: white; border: 0; border-radius: 4px; }
.actions .edit { background: transparent; border: 0; color: #666; }
.footer { padding: 16px 0; }
.add { padding: 6px 16px; }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/PairList.vue
git commit -m "feat(ui): PairList table with split push/pull buttons"
```

---

### Task 23: PairForm component

**Files:**
- Create: `src/components/PairForm.vue`

- [ ] **Step 1: Implement**

In `src/components/PairForm.vue`:

```vue
<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/tauri';
import type { DirectoryPair, TailnetDevice, PathProbeResult } from '../types';

const props = defineProps<{ initial?: DirectoryPair | null }>();
const emit = defineEmits<{ save: [pair: DirectoryPair]; cancel: [] }>();

function blank(): DirectoryPair {
  return {
    id: '',
    name: '',
    local_path: '',
    remote_host: '',
    remote_user: '',
    remote_path: '',
    excludes: [],
    bandwidth_limit_kbps: null,
    mirror_mode: false,
    last_sync: null,
  };
}

const form = ref<DirectoryPair>({ ...(props.initial ?? blank()) });
const excludesText = ref(form.value.excludes.join('\n'));
const bwlimitText = ref(form.value.bandwidth_limit_kbps?.toString() ?? '');

const tailnetDevices = ref<TailnetDevice[]>([]);
const remoteProbe = ref<PathProbeResult | null>(null);

onMounted(async () => {
  try {
    tailnetDevices.value = (await api.listTailnetDevices()).filter(d => !d.is_self);
  } catch (_) { /* noop */ }
});

async function pickLocalPath() {
  const selected = await open({ directory: true, multiple: false });
  if (typeof selected === 'string') form.value.local_path = selected;
}

async function probePath() {
  if (!form.value.remote_user || !form.value.remote_host || !form.value.remote_path) {
    remoteProbe.value = null; return;
  }
  remoteProbe.value = await api.probeRemotePath(
    form.value.remote_user, form.value.remote_host, form.value.remote_path
  );
}

watch([
  () => form.value.remote_user,
  () => form.value.remote_host,
  () => form.value.remote_path,
], () => probePath());

function save() {
  form.value.excludes = excludesText.value.split('\n').map(s => s.trim()).filter(Boolean);
  const n = parseInt(bwlimitText.value, 10);
  form.value.bandwidth_limit_kbps = isNaN(n) || n <= 0 ? null : n;
  emit('save', { ...form.value });
}

function probeIcon(): string {
  if (!remoteProbe.value) return '';
  if (remoteProbe.value === 'Exists') return '✓ 路径存在';
  if (remoteProbe.value === 'Missing') return '✗ 路径不存在（可保存，首次同步时再创建）';
  return `⚠ SSH 失败：${(remoteProbe.value as { SshFailed: string }).SshFailed}`;
}

function probeClass(): string {
  if (!remoteProbe.value) return '';
  if (remoteProbe.value === 'Exists') return 'ok';
  return 'warn';
}
</script>

<template>
  <div class="pair-form">
    <h3>{{ initial ? '编辑目录对' : '新建目录对' }}</h3>

    <label>备注名
      <input v-model="form.name" placeholder="如：读书笔记" />
    </label>

    <label>本机路径
      <div class="row">
        <input v-model="form.local_path" placeholder="/Users/me/Documents/notes" />
        <button @click="pickLocalPath">选择…</button>
      </div>
    </label>

    <label>对端机器
      <select v-model="form.remote_host">
        <option value="" disabled>选择 tailnet 中的机器</option>
        <option v-for="d in tailnetDevices" :key="d.hostname" :value="d.hostname"
                :disabled="!d.online">
          {{ d.hostname }} {{ d.online ? '' : '(离线)' }}
        </option>
      </select>
    </label>

    <label>对端登录用户
      <input v-model="form.remote_user" placeholder="对端的 macOS 账号名" />
    </label>

    <label>对端路径
      <input v-model="form.remote_path" placeholder="/Users/peer/sync/notes" />
      <span :class="['probe', probeClass()]">{{ probeIcon() }}</span>
    </label>

    <label>排除规则（每行一个 glob，如 .DS_Store / node_modules/ / *.tmp）
      <textarea v-model="excludesText" rows="4"></textarea>
    </label>

    <label>限速（KB/s，0 或留空表示不限速）
      <input v-model="bwlimitText" type="number" min="0" />
    </label>

    <label class="checkbox">
      <input type="checkbox" v-model="form.mirror_mode" />
      镜像模式（开启后会从对端删除本机已删除的文件，危险操作）
    </label>

    <div class="actions">
      <button @click="emit('cancel')">取消</button>
      <button class="primary" @click="save">保存</button>
    </div>
  </div>
</template>

<style scoped>
.pair-form { padding: 24px; max-width: 560px; }
label { display: block; margin-bottom: 14px; font-size: 13px; color: #444; }
label > input, label > select, label > textarea {
  display: block; width: 100%; margin-top: 4px; padding: 6px 8px; box-sizing: border-box;
}
.row { display: flex; gap: 6px; margin-top: 4px; }
.row input { flex: 1; }
label.checkbox { display: flex; align-items: center; gap: 6px; }
label.checkbox > input { width: auto; margin: 0; }
.probe { display: block; margin-top: 4px; font-size: 12px; }
.probe.ok { color: #2a7; }
.probe.warn { color: #d80; }
.actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
.actions .primary { background: #2563eb; color: white; border: 0; padding: 6px 16px; border-radius: 4px; }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/PairForm.vue
git commit -m "feat(ui): PairForm with tailnet dropdown + remote path probe"
```

---

### Task 24: SyncDialog (preview + progress)

**Files:**
- Create: `src/components/SyncDialog.vue`

- [ ] **Step 1: Implement**

In `src/components/SyncDialog.vue`:

```vue
<script setup lang="ts">
import { ref, onUnmounted } from 'vue';
import { api, onSyncProgress, onSyncDone } from '../lib/tauri';
import { formatBytes, formatRate, formatEta } from '../lib/format';
import type { DirectoryPair, SyncDirection, DryRunSummary, ProgressUpdate, SyncResult } from '../types';

const props = defineProps<{ pair: DirectoryPair; direction: SyncDirection }>();
const emit = defineEmits<{ close: []; done: [result: SyncResult] }>();

type Phase = 'preview-loading' | 'preview-ready' | 'preview-failed' | 'syncing' | 'done' | 'failed';
const phase = ref<Phase>('preview-loading');
const summary = ref<DryRunSummary | null>(null);
const previewError = ref('');
const progress = ref<ProgressUpdate | null>(null);
const result = ref<SyncResult | null>(null);
const taskId = ref('');
let unlistenProgress: (() => void) | null = null;
let unlistenDone: (() => void) | null = null;

const dirText = props.direction === 'push'
  ? `${props.pair.local_path} → ${props.pair.remote_host}:${props.pair.remote_path}`
  : `${props.pair.remote_host}:${props.pair.remote_path} → ${props.pair.local_path}`;

async function loadPreview() {
  try {
    summary.value = await api.dryRun(props.pair.id, props.direction);
    phase.value = 'preview-ready';
  } catch (e: any) {
    previewError.value = typeof e === 'string' ? e : (e?.message || JSON.stringify(e));
    phase.value = 'preview-failed';
  }
}

async function confirm() {
  phase.value = 'syncing';
  try {
    taskId.value = await api.startSync(props.pair.id, props.direction);
    unlistenProgress = await onSyncProgress(taskId.value, p => { progress.value = p; });
    unlistenDone = await onSyncDone(taskId.value, r => {
      result.value = r;
      phase.value = r.exit_code === 0 ? 'done' : 'failed';
      emit('done', r);
    });
  } catch (e: any) {
    previewError.value = typeof e === 'string' ? e : JSON.stringify(e);
    phase.value = 'failed';
  }
}

async function cancel() {
  if (taskId.value) await api.cancelSync(taskId.value);
}

loadPreview();

onUnmounted(() => {
  unlistenProgress?.();
  unlistenDone?.();
});
</script>

<template>
  <div class="modal-backdrop">
    <div class="modal">
      <h3>{{ direction === 'push' ? '推送' : '拉取' }}：{{ pair.name }}</h3>
      <div class="path-line">{{ dirText }}</div>

      <div v-if="phase === 'preview-loading'">分析中…</div>

      <div v-else-if="phase === 'preview-ready' && summary">
        <div class="summary">
          <div>复制 <strong>{{ summary.files_to_copy }}</strong> 个文件</div>
          <div>修改 <strong>{{ summary.files_to_update }}</strong> 个文件</div>
          <div>删除 <strong>{{ summary.files_to_delete }}</strong> 个文件</div>
          <div>共 <strong>{{ formatBytes(summary.total_bytes) }}</strong></div>
        </div>
        <details v-if="summary.file_list.length">
          <summary>查看完整文件列表 ({{ summary.file_list.length }})</summary>
          <pre>{{ summary.file_list.join('\n') }}</pre>
        </details>
        <div class="actions">
          <button @click="emit('close')">取消</button>
          <button class="primary" @click="confirm">确认执行</button>
        </div>
      </div>

      <div v-else-if="phase === 'preview-failed'">
        <div class="error">预览失败</div>
        <details><summary>详细信息</summary><pre>{{ previewError }}</pre></details>
        <div class="actions"><button @click="emit('close')">关闭</button></div>
      </div>

      <div v-else-if="phase === 'syncing'">
        <div class="current-file">{{ progress?.current_file || '准备中…' }}</div>
        <div class="progress-bar">
          <div class="fill" :style="{ width: `${progress?.percent ?? 0}%` }"></div>
        </div>
        <div class="meta">
          <span>{{ progress?.percent?.toFixed(0) ?? 0 }}%</span>
          <span>{{ formatBytes(progress?.bytes_transferred ?? 0) }}</span>
          <span>{{ formatRate(progress?.rate_bps ?? 0) }}</span>
          <span>剩余 {{ formatEta(progress?.eta_seconds ?? null) }}</span>
        </div>
        <div class="actions"><button @click="cancel">取消</button></div>
      </div>

      <div v-else-if="phase === 'done'">
        <div class="ok">同步完成</div>
        <div class="actions"><button class="primary" @click="emit('close')">关闭</button></div>
      </div>

      <div v-else-if="phase === 'failed'">
        <div class="error">{{ result?.message || '同步失败' }}</div>
        <details><summary>错误详情</summary><pre>{{ result?.stderr_tail || previewError }}</pre></details>
        <div class="actions">
          <button @click="emit('close')">关闭</button>
          <button class="primary" @click="loadPreview">重试</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-backdrop {
  position: fixed; inset: 0; background: rgba(0,0,0,0.4);
  display: flex; align-items: center; justify-content: center; z-index: 100;
}
.modal { background: white; border-radius: 8px; padding: 24px; min-width: 480px; max-width: 720px; max-height: 80vh; overflow: auto; }
.path-line { font-family: ui-monospace, monospace; color: #666; font-size: 12px; padding: 8px 0 16px; word-break: break-all; }
.summary { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; padding: 12px; background: #f5f5f7; border-radius: 4px; margin-bottom: 12px; }
.actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
.actions .primary { background: #2563eb; color: white; border: 0; padding: 6px 16px; border-radius: 4px; }
.current-file { font-family: ui-monospace, monospace; font-size: 12px; color: #555; padding: 8px 0; word-break: break-all; }
.progress-bar { height: 8px; background: #eee; border-radius: 4px; overflow: hidden; }
.fill { height: 100%; background: #2563eb; transition: width 0.3s; }
.meta { display: flex; justify-content: space-between; padding-top: 6px; font-size: 12px; color: #666; }
.error { color: #c33; padding: 12px 0; }
.ok { color: #2a7; padding: 12px 0; }
pre { background: #f5f5f7; padding: 12px; border-radius: 4px; max-height: 240px; overflow: auto; font-size: 11px; }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/components/SyncDialog.vue
git commit -m "feat(ui): SyncDialog with preview + live progress + cancel"
```

---

### Task 25: App shell — wire everything together

**Files:**
- Modify: `src/App.vue`

- [ ] **Step 1: Replace App.vue content**

Replace `src/App.vue` with:

```vue
<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { usePairsStore } from './stores/pairs';
import EnvCheck from './components/EnvCheck.vue';
import StatusBar from './components/StatusBar.vue';
import PairList from './components/PairList.vue';
import PairForm from './components/PairForm.vue';
import SyncDialog from './components/SyncDialog.vue';
import type { DirectoryPair, SyncDirection } from './types';

const ready = ref(false);
const hostname = ref('');
const store = usePairsStore();

const showForm = ref(false);
const editingPair = ref<DirectoryPair | null>(null);

const showSync = ref(false);
const syncPair = ref<DirectoryPair | null>(null);
const syncDirection = ref<SyncDirection>('push');

function onEnvReady(host: string) {
  hostname.value = host;
  ready.value = true;
  store.refresh();
}

function openAdd() { editingPair.value = null; showForm.value = true; }
function openEdit(p: DirectoryPair) { editingPair.value = p; showForm.value = true; }
async function onSave(pair: DirectoryPair) {
  if (pair.id) await store.update(pair);
  else await store.add(pair);
  showForm.value = false;
}

function openSync(p: DirectoryPair, dir: SyncDirection) {
  syncPair.value = p;
  syncDirection.value = dir;
  showSync.value = true;
}

function onSyncDone() {
  // Refresh to pick up last_sync (the backend should update pair on completion).
  store.refresh();
}
</script>

<template>
  <div class="app">
    <EnvCheck v-if="!ready" @ready="onEnvReady" />
    <template v-else>
      <StatusBar :hostname="hostname" :tailscale-connected="true" />
      <PairList @add="openAdd" @edit="openEdit" @sync="openSync" />
    </template>

    <div v-if="showForm" class="modal-backdrop" @click.self="showForm = false">
      <div class="modal-shell">
        <PairForm :initial="editingPair" @save="onSave" @cancel="showForm = false" />
      </div>
    </div>

    <SyncDialog
      v-if="showSync && syncPair"
      :pair="syncPair"
      :direction="syncDirection"
      @close="showSync = false"
      @done="onSyncDone"
    />
  </div>
</template>

<style>
body { margin: 0; font-family: -apple-system, BlinkMacSystemFont, sans-serif; }
.app { min-height: 100vh; background: white; }
.modal-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: flex; align-items: center; justify-content: center; z-index: 50; }
.modal-shell { background: white; border-radius: 8px; max-height: 90vh; overflow: auto; }
</style>
```

Note: `last_sync` updates from backend require a future enhancement — for now, the next sync action will overwrite the pair via update_pair after success. To wire that, modify the `tokio::spawn` block in `commands.rs::start_sync` to call `state.pairs.lock()` and `save_pairs(...)` with the updated `last_sync` after the sync completes. Add this enhancement now:

In `src-tauri/src/commands.rs`, inside the `tokio::spawn` block of `start_sync`, before emitting the done event, add:

```rust
        // Update pair's last_sync.
        let last = LastSync {
            direction: match direction_for_record { Direction::Push => SyncDirection::Push, Direction::Pull => SyncDirection::Pull },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
            status: if exit.map(|s| s.success()).unwrap_or(false) { SyncStatus::Success }
                    else if exit.is_none() { SyncStatus::Interrupted }
                    else { SyncStatus::Failed },
            message: result.message.clone(),
        };
        if let Ok(state_arc) = app_for_done.try_state::<AppState>() {
            let mut guard = state_arc.pairs.lock().unwrap();
            if let Some(p) = guard.iter_mut().find(|p| p.id == pair_id_for_record) {
                p.last_sync = Some(last);
            }
            let _ = save_pairs(&state_arc.pairs_path, &guard);
        }
```

Add the necessary imports/captures: `direction_for_record` and `pair_id_for_record` are clones of `req.direction` and `pair.id` made before the spawn. Adjust the start_sync function:

```rust
let direction_for_record = req.direction.clone();
let pair_id_for_record = pair.id.clone();
```

Also derive `Clone` on `Direction` (`#[derive(Debug, Clone, Deserialize)]` already there). Import `LastSync, SyncStatus, SyncDirection` at top: `use crate::pairs::{DirectoryPair, LastSync, SyncStatus, SyncDirection, save_pairs};`. The `try_state` method returns `Option<State<T>>`; switch to `app_for_done.state::<AppState>()` if try_state isn't available in your Tauri version.

- [ ] **Step 2: Verify build**

```bash
cd src-tauri && cargo build
cd .. && npm run dev
```

Expected: both build clean, dev server starts.

- [ ] **Step 3: End-to-end smoke**

Run `npm run tauri dev`. Verify in app:
- env check passes (or shows what to fix)
- "+ 新建目录对" opens form
- tailnet dropdown lists peers
- saving creates `pairs.json`
- "推过去" opens preview dialog with file counts
- confirming runs rsync, progress streams
- list updates with last sync info after completion

- [ ] **Step 4: Commit**

```bash
git add src/App.vue src-tauri/src/commands.rs
git commit -m "feat: wire app shell, persist last_sync after completion"
```

---

## Phase 5: Polish & Distribution

### Task 26: End-to-end manual verification on two real Macs

**Files:** none (verification only)

- [ ] **Step 1: Install on both Macs**

Build a release binary on Mac A:

```bash
cd ~/Documents/projects/tailsync
npm run tauri build
```

Output is at `src-tauri/target/release/bundle/macos/tailsync.app` and a DMG under `bundle/dmg/`. Copy to Mac B (e.g. via airdrop or via tailsync once it works once).

Verify on both: app launches, env check passes.

- [ ] **Step 2: Push direction**

On Mac A, create a directory `~/tailsync-test/from-a/` with 5 files. Add a pair pointing to `~/tailsync-test/landing/` on Mac B. Push. Verify B has the files.

- [ ] **Step 3: Pull direction**

On Mac A, click "拉回来" on the same pair after creating new files on B's side. Verify A receives them.

- [ ] **Step 4: Interruption + resume**

On Mac A, configure a pair with a 1GB+ file. Start push. Halfway through, click cancel. Verify the partial file appears in `~/tailsync-test/landing/.rsync-partial/`. Click push again — verify it resumes (only the remaining bytes transfer).

- [ ] **Step 5: Slow network**

Set `bandwidth_limit_kbps: 256` on a pair, push a multi-MB file, verify rate stays around 256 KB/s in the progress UI.

- [ ] **Step 6: Mirror mode + delete**

Enable mirror mode on a pair. Delete a file on A. Push. Confirm the dry-run preview lists the deletion, and after confirm the file disappears on B.

- [ ] **Step 7: Error path**

Break SSH temporarily (e.g., turn Tailscale off on B). Try to push from A. Verify error dialog shows a useful translation plus collapsible stderr.

- [ ] **Step 8: Document any issues found, file as TODO commits**

```bash
git commit --allow-empty -m "verify: end-to-end on macbook-pro-2 + wuhoujins-mac-mini"
```

---

### Task 27: README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write**

In `README.md`:

```markdown
# tailsync

Mac 之间通过 Tailscale 内网做单向文件同步的桌面工具。两台机器各装一份，分别可以往对面推或者从对面拉，方向用界面上分开的两个按钮显式控制。底层是 rsync 走 Tailscale SSH。

## 前置条件

- 两台机器都安装并登录了同一个 Tailscale tailnet
- 两台机器都执行了 `tailscale set --ssh`，开启了 Tailscale SSH
- 两台机器都安装了 `rsync`（macOS 自带）

## 开发

```bash
npm install
npm run tauri dev      # 开发
npm run tauri build    # 打包 release
cd src-tauri && cargo test --lib       # Rust 单测
npm test                                # 前端单测
```

## 配置文件

`~/Library/Application Support/tailsync/pairs.json`

可以在两台机器之间手动复制（注意调整 local_path / remote_path）。

## 设计文档

`docs/specs/2026-05-15-tailscale-sync-tool-design.md`
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README"
```

---

## Self-Review

- **Spec coverage:**
  - Goals (sync, both sides, direction, resume, compression, timeout, bwlimit, dry-run, exclude rules) → covered by Tasks 5–6, 10–11, 16
  - Architecture (independent apps, rsync over Tailscale SSH) → covered by Tasks 8, 10, 12
  - Data model (DirectoryPair, LastSync, enums) → Task 2
  - JSON storage with atomic rename → Task 4
  - Tailnet dropdown → Tasks 7, 15, 23
  - Two physical buttons (push/pull) → Task 22
  - Dry-run preview → Tasks 10, 16, 24
  - rsync param baseline (`-az --partial --partial-dir --info=progress2 --stats --timeout=300`) → Task 5
  - Optional `--delete` per mirror_mode → Task 5
  - Optional `--bwlimit` → Task 5
  - Default exclude list (.DS_Store, etc.) + per-pair excludes → Task 3, plus Task 16 builds the temp file
  - Error handling with stderr collapse → Tasks 13, 24
  - Auto-create remote dir → Tasks 12, 15
  - Open Full Disk Access settings → Task 17
  - First-run env check (CLI / login / SSH) → Tasks 9, 15, 20
  - No history (only last_sync on the pair) → Tasks 2, 25
  - Testing strategy (cargo test for pure logic, vitest for UI helpers, manual e2e) → spread across phases, finalized in Task 26

- **Placeholder scan:** No "TBD"/"TODO"/"add appropriate handling". Each step contains executable commands or full code.

- **Type consistency:**
  - `DirectoryPair` fields used identically across pairs.rs (Task 2), commands.rs (Task 14, 16), types.ts (Task 18), components (Tasks 22–25)
  - `RsyncConfig` defined in Task 5, used in Tasks 10, 16
  - `ProgressUpdate` defined in Task 6, used in Tasks 10, 16, 18, 24
  - `DryRunSummary` defined in Task 10, used in Tasks 16, 18, 24
  - `Direction` enum (snake_case in Rust serde, lowercase in TS) — consistent
  - `SyncManager` Arc-wrapped from Task 16 — must reflect back into Task 11's interface

---

## Execution Handoff

Plan complete and saved to `docs/plans/2026-05-15-tailsync-implementation.md`.
