use std::process::Command;

use crate::constants::CLAUDE_BIN;
use crate::error::{Error, Result};

// ── VSCode remote ─────────────────────────────────────────────────────────────

/// The `vscode-remote://` URI that opens `remote_path` on `host` over Remote SSH.
///
/// Printed for the user to open (e.g. `code --folder-uri <uri>`) rather than
/// launching VSCode directly.
pub fn vscode_remote_uri(host: &str, remote_path: &str) -> String {
    format!("vscode-remote://ssh-remote+{host}{remote_path}")
}

// ── tmux session naming ────────────────────────────────────────────────────────

/// Sanitise a session id into a tmux-safe session name.
///
/// tmux uses `.` and `:` as target separators, so they are not allowed in
/// session names; `/` would also confuse window targets. Everything else
/// (including `@`) is preserved so `project@host` stays readable.
pub fn tmux_name(id: &str) -> String {
    id.replace(['.', ':', '/'], "-")
}

/// The tmux session name used on the *remote* host for a project's claude.
pub fn remote_tmux_name(project: &str) -> String {
    tmux_name(&format!("jeru-{project}"))
}

// ── MCP tunnel ─────────────────────────────────────────────────────────────────

/// A reverse SSH tunnel exposing the local Obsidian MCP server to the remote.
///
/// `-R {port}:{host}:{port}` makes the remote's `host:port` forward back to the
/// local machine, so the synced `.mcp.json` (which points at the same host:port)
/// reaches local Obsidian. The token, if known, is injected into the remote
/// Claude's environment so `${OBSIDIAN_API_KEY}` in `.mcp.json` resolves.
pub struct McpTunnel {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

// ── claude command builders ────────────────────────────────────────────────────

/// The shell command that runs `claude remote-control` for a *local* session,
/// to be used as a tmux window command.
pub fn claude_local_cmd(cwd: &str, spawn: &str, token: Option<&str>) -> String {
    let env = match token {
        Some(t) => format!("OBSIDIAN_API_KEY={} ", sq(t)),
        None => String::new(),
    };
    format!(
        "cd {cwd} && {env}{CLAUDE_BIN} remote-control --spawn {spawn}",
        cwd = sq(cwd)
    )
}

/// The shell command for a *remote* session's tmux window: a self-reconnecting
/// ssh that runs `claude remote-control` inside a tmux session **on the remote
/// host**, so claude survives ssh disconnects.
///
/// `tmux new-session -A` creates the remote session (running claude) on first
/// connect and re-attaches to the still-running claude on every reconnect. The
/// `-R` reverse tunnel (when `mcp` is set) is re-established each reconnect,
/// keeping Obsidian MCP reachable.
pub fn claude_remote_loop_cmd(
    host: &str,
    remote_tmux: &str,
    remote_project_path: &str,
    spawn: &str,
    mcp: Option<&McpTunnel>,
) -> String {
    let env = match mcp.and_then(|m| m.token.as_deref()) {
        Some(token) => format!("OBSIDIAN_API_KEY={} ", sq(token)),
        None => String::new(),
    };
    let claude = format!(
        "cd {rp} && {env}{CLAUDE_BIN} remote-control --spawn {spawn}",
        rp = sq(remote_project_path)
    );
    let inner = format!("tmux new-session -A -s {} {}", sq(remote_tmux), sq(&claude));

    let forward = match mcp {
        Some(m) => format!("-R {p}:{h}:{p} ", p = m.port, h = m.host),
        None => String::new(),
    };
    let ssh = format!("ssh -t {forward}{host} {}", sq(&inner));
    // Reconnect loop: if ssh drops (laptop sleep, network move), wait briefly and
    // re-attach to the remote tmux (claude is still running there).
    format!("while true; do {ssh}; echo '[jeru] ssh disconnected; reconnecting…'; sleep 2; done")
}

// ── tmux control ───────────────────────────────────────────────────────────────

/// Width/height for detached sessions. A detached session has no client to size
/// it, so it defaults to 80 columns and `capture-pane` returns claude's output
/// wrapped at that width — splitting long remote-control URLs across rows. A
/// wide pane keeps the URL on one line. On `inspect`, attaching resizes the
/// session to the client's terminal, so this only affects the detached period.
const DETACHED_WIDTH: &str = "240";
const DETACHED_HEIGHT: &str = "50";

/// Create a detached tmux session with a first window running `cmd`.
pub fn tmux_new_detached(session: &str, window: &str, cmd: &str) -> Result<()> {
    tmux_status(&[
        "new-session", "-d", "-x", DETACHED_WIDTH, "-y", DETACHED_HEIGHT, "-s", session, "-n",
        window, cmd,
    ])
}

/// Add a window running `cmd` to an existing session.
pub fn tmux_new_window(session: &str, window: &str, cmd: &str) -> Result<()> {
    tmux_status(&["new-window", "-t", session, "-n", window, cmd])
}

/// Whether a tmux session with the given name exists.
pub fn tmux_has_session(session: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Kill a tmux session. A missing session is treated as success.
pub fn tmux_kill_session(session: &str) -> Result<()> {
    if !tmux_has_session(session) {
        return Ok(());
    }
    tmux_status(&["kill-session", "-t", session])
}

/// Capture the visible contents of a tmux pane (e.g. `session:window`).
///
/// `-J` joins wrapped lines so a URL that spans rows is returned intact.
pub fn tmux_capture_pane(target: &str) -> Result<String> {
    let out = Command::new("tmux")
        .args(["capture-pane", "-p", "-J", "-t", target])
        .output()?;
    if !out.status.success() {
        return Err(Error::Tmux(format!("capture-pane {target} failed")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Attach to (or, when already inside tmux, switch to) a session. Blocks until
/// the user detaches — used by `jeru session inspect`.
pub fn tmux_attach(session: &str) -> Result<()> {
    // Inside an existing tmux session, `attach-session` is rejected
    // ("sessions should be nested with care"). Use `switch-client` instead.
    let attach_cmd = if std::env::var("TMUX").is_ok() {
        "switch-client"
    } else {
        "attach-session"
    };
    tmux_status(&[attach_cmd, "-t", session])
}

fn tmux_status(args: &[&str]) -> Result<()> {
    let ok = Command::new("tmux").args(args).status()?.success();
    if !ok {
        return Err(Error::Tmux(format!("tmux {} failed", args.join(" "))));
    }
    Ok(())
}

// ── remote tmux control (over ssh) ─────────────────────────────────────────────

/// Capture the remote claude tmux pane over ssh (`-J` joins wrapped lines).
pub fn remote_capture_pane(host: &str, remote_tmux: &str) -> Result<String> {
    let cmd = format!("tmux capture-pane -p -J -t {}", sq(remote_tmux));
    let out = Command::new("ssh").args([host, &cmd]).output()?;
    if !out.status.success() {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Kill the remote claude tmux session over ssh (gracefully ending claude). A
/// missing session is treated as success.
pub fn remote_kill_tmux(host: &str, remote_tmux: &str) -> Result<()> {
    let cmd = format!(
        "tmux has-session -t {t} 2>/dev/null && tmux kill-session -t {t} || true",
        t = sq(remote_tmux)
    );
    let ok = Command::new("ssh").args([host, &cmd]).status()?.success();
    if !ok {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Single-quote a shell argument, escaping any single quotes inside.
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_name_sanitises_separators() {
        assert_eq!(tmux_name("proj@user@host.example.com"), "proj@user@host-example-com");
        assert_eq!(tmux_name("a/b:c.d"), "a-b-c-d");
        assert_eq!(tmux_name("plain"), "plain");
    }

    #[test]
    fn remote_tmux_name_prefixes_and_sanitises() {
        assert_eq!(remote_tmux_name("my.proj"), "jeru-my-proj");
    }

    #[test]
    fn vscode_remote_uri_format() {
        assert_eq!(
            vscode_remote_uri("user@host", "/home/u/p"),
            "vscode-remote://ssh-remote+user@host/home/u/p"
        );
    }

    #[test]
    fn local_cmd_with_token_and_spawn() {
        let cmd = claude_local_cmd("/home/u/proj", "worktree", Some("secret"));
        assert!(cmd.contains("cd '/home/u/proj'"), "cmd: {cmd}");
        assert!(cmd.contains("OBSIDIAN_API_KEY='secret'"), "cmd: {cmd}");
        assert!(cmd.contains("claude remote-control --spawn worktree"), "cmd: {cmd}");
    }

    #[test]
    fn local_cmd_without_token_omits_env() {
        let cmd = claude_local_cmd("/home/u/proj", "same-dir", None);
        assert!(!cmd.contains("OBSIDIAN_API_KEY"), "cmd: {cmd}");
        assert!(cmd.contains("claude remote-control --spawn same-dir"), "cmd: {cmd}");
    }

    #[test]
    fn remote_loop_wraps_remote_tmux_and_reconnects() {
        let cmd = claude_remote_loop_cmd("myhost", "jeru-proj", "/remote/proj", "session", None);
        // Runs claude inside a remote tmux session via new-session -A.
        assert!(cmd.contains("tmux new-session -A -s"), "cmd: {cmd}");
        assert!(cmd.contains("jeru-proj"), "cmd: {cmd}");
        assert!(cmd.contains("remote-control --spawn session"), "cmd: {cmd}");
        // Self-reconnecting loop.
        assert!(cmd.contains("while true"), "cmd: {cmd}");
        // No tunnel when mcp is None.
        assert!(!cmd.contains("-R "), "cmd: {cmd}");
        assert!(!cmd.contains("OBSIDIAN_API_KEY"), "cmd: {cmd}");
    }

    /// Whether `cmd` is syntactically valid POSIX shell (parsed, not executed).
    fn sh_parses(cmd: &str) -> bool {
        Command::new("sh")
            .args(["-n", "-c", cmd])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn local_cmd_is_valid_shell() {
        // Paths/tokens with awkward characters must survive quoting.
        let cmd = claude_local_cmd("/home/u/it's a dir", "same-dir", Some("to'ken"));
        assert!(sh_parses(&cmd), "not valid shell: {cmd}");
    }

    #[test]
    fn remote_loop_is_valid_shell() {
        // The remote loop nests three quoting levels (loop → ssh → remote tmux);
        // make sure the whole thing still parses as shell.
        let tunnel = McpTunnel {
            host: "127.0.0.1".to_string(),
            port: 27123,
            token: Some("to'ken".to_string()),
        };
        let cmd = claude_remote_loop_cmd(
            "user@host",
            "jeru-proj",
            "/home/u/it's a dir",
            "worktree",
            Some(&tunnel),
        );
        assert!(sh_parses(&cmd), "not valid shell: {cmd}");
    }

    #[test]
    fn remote_loop_adds_reverse_tunnel_and_token() {
        let tunnel = McpTunnel {
            host: "127.0.0.1".to_string(),
            port: 27123,
            token: Some("secret-token".to_string()),
        };
        let cmd =
            claude_remote_loop_cmd("myhost", "jeru-proj", "/remote/proj", "same-dir", Some(&tunnel));
        assert!(cmd.contains("-R 27123:127.0.0.1:27123"), "cmd: {cmd}");
        assert!(
            cmd.find("-R ").unwrap() < cmd.find("myhost").unwrap(),
            "forward must precede host: {cmd}"
        );
        let env_pos = cmd.find("OBSIDIAN_API_KEY=").expect("token env present");
        let claude_pos = cmd.find("remote-control").unwrap();
        assert!(env_pos < claude_pos, "token must precede claude: {cmd}");
        assert!(cmd.contains("secret-token"), "cmd: {cmd}");
    }
}
