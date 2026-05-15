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
