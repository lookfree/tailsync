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
