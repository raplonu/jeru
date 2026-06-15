//! Obsidian launch helper.
//!
//! The Obsidian MCP server (used by Claude for vault access, both locally and —
//! via a reverse tunnel — remotely) only runs while the Obsidian app is open.
//! `jeru session start` launches Obsidian normally if its MCP server is not
//! already reachable, then leaves it running: Obsidian is the user's vault
//! editor and stays up across sessions. jeru never stops it.

use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::mcp::mcp_host_port;

/// How long to wait for the MCP port to come up after launching Obsidian.
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// How often to poll the MCP port while waiting for readiness.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Per-attempt TCP connect timeout for the reachability probe.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

/// Ensure the Obsidian MCP server is reachable, launching Obsidian if not.
///
/// Fire-and-forget: if Obsidian must be started, jeru spawns it detached and
/// never stops it. No-op (and never spawns) when MCP is disabled, autostart is
/// off, the MCP URL is unparseable, or Obsidian is already listening. All
/// failure modes are non-fatal: a warning is printed and the caller proceeds.
pub fn ensure_running(config: &Config) {
    if !config.obsidian_mcp_enabled || !config.obsidian_autostart {
        return;
    }
    let Some((host, port)) = mcp_host_port(&config.obsidian_mcp_url) else {
        eprintln!(
            "warning: could not parse Obsidian MCP url '{}'; not autostarting Obsidian",
            config.obsidian_mcp_url
        );
        return;
    };

    // Already running — leave it alone.
    if port_reachable(&host, port) {
        return;
    }

    eprint!("Obsidian not running; starting… ");
    // `Child` is intentionally dropped without waiting/killing: std's Drop does
    // not reap or signal the child, so Obsidian keeps running after we return.
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(&config.obsidian_launch_cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to spawn: {e}");
            return;
        }
    };

    // Wait for the MCP port to accept connections (Obsidian + the REST API
    // plugin take a few seconds to come up).
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if port_reachable(&host, port) {
            eprintln!("ready");
            return;
        }
        // If the launcher already exited, the port will never open.
        if matches!(child.try_wait(), Ok(Some(_))) {
            eprintln!("failed: Obsidian exited before the MCP port came up");
            return;
        }
        if Instant::now() >= deadline {
            eprintln!("timed out waiting for MCP port; continuing without it");
            return;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Whether a TCP connection to `host:port` succeeds within [`CONNECT_TIMEOUT`].
pub fn port_reachable(host: &str, port: u16) -> bool {
    (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| {
            addrs.find_map(|a| TcpStream::connect_timeout(&a, CONNECT_TIMEOUT).ok())
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn base_config() -> Config {
        Config {
            projects_dir: "/tmp".into(),
            knowledge_dir: "/tmp".into(),
            cache_dir: "/tmp".into(),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: "http://127.0.0.1:27123/mcp/".to_string(),
            obsidian_api_key_env: "OBSIDIAN_API_KEY".to_string(),
            obsidian_autostart: true,
            // A command that would fail loudly if ever run — these tests must
            // never actually spawn it.
            obsidian_launch_cmd: "false".to_string(),
        }
    }

    #[test]
    fn noop_when_mcp_disabled() {
        let mut config = base_config();
        config.obsidian_mcp_enabled = false;
        // Must return without spawning the (failing) launch command.
        ensure_running(&config);
    }

    #[test]
    fn noop_when_autostart_disabled() {
        let mut config = base_config();
        config.obsidian_autostart = false;
        ensure_running(&config);
    }

    #[test]
    fn noop_when_already_running() {
        // Bind a real listener and point the MCP url at it: ensure_running must
        // see the port as reachable and not spawn anything.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = base_config();
        config.obsidian_mcp_url = format!("http://127.0.0.1:{port}/mcp/");
        ensure_running(&config);
    }

    #[test]
    fn port_reachable_detects_open_and_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_reachable("127.0.0.1", port));
        drop(listener);
        assert!(!port_reachable("127.0.0.1", port));
    }
}
