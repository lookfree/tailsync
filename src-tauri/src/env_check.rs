use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EnvCheckResult {
    pub tailscale_installed: bool,
    pub tailscale_logged_in: bool,
    pub tailscale_ssh_enabled: bool,
    pub rsync_modern: bool,
    pub self_hostname: Option<String>,
    pub error_detail: Option<String>,
}

pub fn check_environment() -> EnvCheckResult {
    let rsync_modern = crate::rsync::rsync_binary().is_some();

    if !which_tailscale() {
        return EnvCheckResult {
            tailscale_installed: false,
            tailscale_logged_in: false,
            tailscale_ssh_enabled: false,
            rsync_modern,
            self_hostname: None,
            error_detail: Some("未找到 tailscale CLI（PATH 和常见安装位置都没有）".into()),
        };
    }

    match crate::tailscale::fetch_status() {
        Ok((me, _)) => EnvCheckResult {
            tailscale_installed: true,
            tailscale_logged_in: !me.hostname.is_empty() && !me.tailscale_ip.is_empty(),
            tailscale_ssh_enabled: me.ssh_enabled,
            rsync_modern,
            self_hostname: Some(me.hostname),
            error_detail: None,
        },
        Err(e) => EnvCheckResult {
            tailscale_installed: true,
            tailscale_logged_in: false,
            tailscale_ssh_enabled: false,
            rsync_modern,
            self_hostname: None,
            error_detail: Some(e.to_string()),
        },
    }
}

fn which_tailscale() -> bool {
    crate::tailscale::tailscale_binary().is_some()
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
