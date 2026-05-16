use std::path::PathBuf;
use std::process::Command;

/// Locations checked when the user lacks a modern rsync on PATH.
/// macOS ships openrsync at /usr/bin/rsync which lacks --info=progress2,
/// --partial-dir, --bwlimit and friends — we need rsync 3.x.
const RSYNC_FALLBACK_PATHS: &[&str] = &[
    "/opt/homebrew/bin/rsync",
    "/usr/local/bin/rsync",
];

/// Find a modern rsync (3.x). Returns None if only openrsync or rsync 2.x is present.
pub fn rsync_binary() -> Option<PathBuf> {
    for p in RSYNC_FALLBACK_PATHS {
        let pb = PathBuf::from(p);
        if pb.exists() && is_modern_rsync(&pb) {
            return Some(pb);
        }
    }
    let system = PathBuf::from("/usr/bin/rsync");
    if system.exists() && is_modern_rsync(&system) {
        return Some(system);
    }
    None
}

fn is_modern_rsync(path: &PathBuf) -> bool {
    let Ok(out) = Command::new(path).arg("--version").output() else { return false; };
    let v = String::from_utf8_lossy(&out.stdout);
    let first = v.lines().next().unwrap_or("");
    // Reject openrsync (Apple's port) and rsync 2.x; accept rsync 3+.
    !first.starts_with("openrsync") && !first.contains("version 2.")
}

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
        args.push("-i".into());
        args.push("--dry-run".into());
    }
    args.push(c.source.clone());
    args.push(c.destination.clone());
    args
}

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
        assert!(a.contains(&"-i".into()));
    }

    #[test]
    fn excludes_file_added_when_present() {
        let cfg = RsyncConfig { excludes_file: Some("/tmp/ex".into()), ..Default::default() };
        let a = build_rsync_args(&cfg);
        assert!(a.contains(&"--exclude-from=/tmp/ex".into()));
    }

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
}
