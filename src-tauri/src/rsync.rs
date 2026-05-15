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
