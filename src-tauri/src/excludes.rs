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
