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
        .args(["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=5", &target, &cmd])
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
        .args(["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=5", &target, &cmd])
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
