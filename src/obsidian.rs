//! Headless Obsidian lifecycle management.
//!
//! The Obsidian MCP server (used by Claude for vault access, both locally and —
//! via a reverse tunnel — remotely) only runs while the Obsidian app is open.
//! To avoid forcing a GUI, `jeru work` can launch Obsidian headlessly under Xvfb
//! and stop it again when the session ends. We only ever stop an instance that
//! *we* started: if Obsidian is already running, it is left untouched.

use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::mcp::mcp_host_port;

/// How long to wait for the MCP port to come up after launching Obsidian.
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// How often to poll the MCP port while waiting for readiness.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Per-attempt TCP connect timeout for the reachability probe.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// Grace period between SIGTERM and SIGKILL when stopping Obsidian.
const TERM_GRACE: Duration = Duration::from_secs(5);

/// Owns a headless Obsidian process started by jeru.
///
/// Dropping the handle (or calling [`ObsidianHandle::shutdown`]) stops that
/// process and its whole process group. A handle that owns nothing — because MCP
/// is disabled, autostart is off, or Obsidian was already running — is a no-op.
pub struct ObsidianHandle {
    /// The spawned `sh -c '<launch cmd>'`, group leader of the Obsidian tree.
    /// `None` once stopped or when jeru did not start Obsidian.
    child: Option<Child>,
}

impl ObsidianHandle {
    /// A handle that owns nothing and does nothing on shutdown.
    fn noop() -> Self {
        Self { child: None }
    }

    /// Whether jeru started Obsidian (and is therefore responsible for stopping it).
    pub fn started(&self) -> bool {
        self.child.is_some()
    }

    /// Stop the Obsidian process group, if jeru started it. Idempotent.
    ///
    /// `process_group(0)` made the spawned `sh` a group leader (pgid == pid), so
    /// signalling `-pid` reaches the whole `xvfb-run` → Obsidian subtree.
    pub fn shutdown(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let pid = child.id() as libc::pid_t;
        eprint!("Stopping headless Obsidian… ");
        // SIGTERM the group, give it a moment, then SIGKILL anything left.
        unsafe { libc::kill(-pid, libc::SIGTERM) };
        let deadline = Instant::now() + TERM_GRACE;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                _ => {
                    unsafe { libc::kill(-pid, libc::SIGKILL) };
                    let _ = child.wait();
                    break;
                }
            }
        }
        eprintln!("done");
    }
}

impl Drop for ObsidianHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Ensure the Obsidian MCP server is reachable, launching it headlessly if not.
///
/// Returns a handle that stops Obsidian on drop **only if jeru started it**.
/// No-op (and never spawns) when MCP is disabled, autostart is off, the MCP URL
/// is unparseable, or Obsidian is already listening. All failure modes are
/// non-fatal: a warning is printed and the caller proceeds without MCP.
pub fn ensure_running(config: &Config) -> ObsidianHandle {
    if !config.obsidian_mcp_enabled || !config.obsidian_autostart {
        return ObsidianHandle::noop();
    }
    let Some((host, port)) = mcp_host_port(&config.obsidian_mcp_url) else {
        eprintln!(
            "warning: could not parse Obsidian MCP url '{}'; not autostarting Obsidian",
            config.obsidian_mcp_url
        );
        return ObsidianHandle::noop();
    };

    // Already running — someone else's session, leave it alone.
    if port_reachable(&host, port) {
        return ObsidianHandle::noop();
    }

    eprint!("Obsidian not running; starting headless… ");
    let child = match Command::new("sh")
        .arg("-c")
        .arg(&config.obsidian_launch_cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to spawn: {e}");
            return ObsidianHandle::noop();
        }
    };
    let mut handle = ObsidianHandle { child: Some(child) };

    // Wait for the MCP port to accept connections (Obsidian + the REST API
    // plugin take a few seconds to come up under Xvfb).
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if port_reachable(&host, port) {
            eprintln!("ready");
            return handle;
        }
        // If the launcher already exited, the port will never open.
        if let Some(child) = handle.child.as_mut()
            && matches!(child.try_wait(), Ok(Some(_)))
        {
            eprintln!("failed: Obsidian exited before the MCP port came up");
            handle.child = None; // already dead; nothing to stop
            return handle;
        }
        if Instant::now() >= deadline {
            eprintln!("timed out waiting for MCP port; continuing without it");
            return handle;
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
        assert!(!ensure_running(&config).started());
    }

    #[test]
    fn noop_when_autostart_disabled() {
        let mut config = base_config();
        config.obsidian_autostart = false;
        assert!(!ensure_running(&config).started());
    }

    #[test]
    fn noop_when_already_running() {
        // Bind a real listener and point the MCP url at it: ensure_running must
        // see the port as reachable and not spawn anything.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = base_config();
        config.obsidian_mcp_url = format!("http://127.0.0.1:{port}/mcp/");
        assert!(!ensure_running(&config).started());
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
