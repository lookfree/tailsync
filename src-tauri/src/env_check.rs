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
