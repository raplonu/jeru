use std::process::Command;

use crate::constants::CODE_BIN;
use crate::error::{Error, Result};

// ── VSCode remote ─────────────────────────────────────────────────────────────

/// Open a directory in VSCode via Remote SSH (non-blocking).
pub fn vscode_open_remote(host: &str, remote_path: &str) -> Result<()> {
    Command::new(CODE_BIN)
        .arg("--folder-uri")
        .arg(format!("vscode-remote://ssh-remote+{host}{remote_path}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// Open a `.code-workspace` file in VSCode via Remote SSH (non-blocking).
pub fn vscode_open_workspace_remote(host: &str, remote_file_path: &str) -> Result<()> {
    Command::new(CODE_BIN)
        .arg("--file-uri")
        .arg(format!(
            "vscode-remote://ssh-remote+{host}{remote_file_path}"
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

// ── tmux ──────────────────────────────────────────────────────────────────────

/// Sanitise an arbitrary string for use as a tmux session name.
pub fn tmux_session_name(project: &str, host: &str) -> String {
    let slug = host.replace(['@', '.', ':'], "-");
    format!("jeru-{project}-{slug}")
}

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

/// Build the `ssh -t host 'cd … && claude …'` command string for tmux.
///
/// When `mcp` is set, a reverse port-forward is added and the Obsidian token is
/// exported inline for the remote Claude process.
pub fn claude_ssh_cmd(
    host: &str,
    remote_project_path: &str,
    add_dirs: &[String],
    extra: &[String],
    mcp: Option<&McpTunnel>,
) -> String {
    let add = add_dirs
        .iter()
        .map(|d| format!("--add-dir {}", sq(d)))
        .collect::<Vec<_>>()
        .join(" ");
    let tail = extra.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ");

    let env = match mcp.and_then(|m| m.token.as_deref()) {
        Some(token) => format!("OBSIDIAN_API_KEY={} ", sq(token)),
        None => String::new(),
    };
    let inner = format!(
        "cd {rp} && {env}claude {tail} {add}",
        rp = sq(remote_project_path)
    );

    let forward = match mcp {
        Some(m) => format!("-R {p}:{h}:{p} ", p = m.port, h = m.host),
        None => String::new(),
    };
    format!("ssh -t {forward}{host} {}", sq(&inner))
}

/// Launch a tmux session with a `sync` window (mutagen monitor) and,
/// optionally, a `claude` window.  Blocks until the user closes the session,
/// then returns.
pub fn launch_tmux(session: &str, claude_cmd: Option<&str>, project: &str) -> Result<()> {
    // Use the label added at session-creation time so all pairs are covered by
    // a single monitor command (sync monitor accepts only one session specifier).
    let monitor_cmd = format!("mutagen sync monitor --label-selector jeru-project={project}");

    // Create session (detached). If it already exists we just re-attach below.
    let created = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            session,
            "-n",
            "sync",
            &monitor_cmd,
        ])
        .status()?
        .success();

    if created && let Some(cmd) = claude_cmd {
        Command::new("tmux")
            .args(["new-window", "-t", session, "-n", "claude", cmd])
            .status()?;
        // Start focused on the claude window.
        Command::new("tmux")
            .args(["select-window", "-t", &format!("{session}:claude")])
            .status()?;
    }

    // Inside an existing tmux session, `attach-session` is rejected
    // ("sessions should be nested with care"). Use `switch-client` instead,
    // which replaces the current client's view with the new session.
    let attach_cmd = if std::env::var("TMUX").is_ok() {
        "switch-client"
    } else {
        "attach-session"
    };
    let ok = Command::new("tmux")
        .args([attach_cmd, "-t", session])
        .status()?
        .success();
    if !ok {
        return Err(Error::Mutagen(format!("tmux {attach_cmd} failed")));
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
    fn claude_ssh_cmd_quotes_add_dirs_with_spaces() {
        let cmd = claude_ssh_cmd(
            "myhost",
            "/remote/proj",
            &["/path/with spaces/dir".to_string()],
            &[],
            None,
        );
        // The path with spaces must appear in the output. The whole inner
        // command is shell-quoted, so the literal quoting style varies, but
        // the path substring is always present.
        assert!(cmd.contains("/path/with spaces/dir"), "cmd: {cmd}");
        // The space must not split the path across two separate --add-dir tokens.
        assert!(!cmd.contains("spaces/dir --"), "unexpected split: {cmd}");
    }

    #[test]
    fn claude_ssh_cmd_quotes_extra_args() {
        let cmd = claude_ssh_cmd(
            "myhost",
            "/remote/proj",
            &[],
            &["--flag with space".to_string()],
            None,
        );
        assert!(cmd.contains("'--flag with space'"));
    }

    #[test]
    fn claude_ssh_cmd_extra_args_before_add_dirs() {
        let cmd = claude_ssh_cmd(
            "myhost",
            "/remote/proj",
            &["/some/dir".to_string()],
            &["remote-control".to_string()],
            None,
        );
        let extra_pos = cmd.find("remote-control").unwrap();
        let add_pos = cmd.find("--add-dir").unwrap();
        assert!(extra_pos < add_pos, "extra args must appear before --add-dir flags: {cmd}");
    }

    #[test]
    fn claude_ssh_cmd_adds_reverse_tunnel_and_token() {
        let tunnel = McpTunnel {
            host: "127.0.0.1".to_string(),
            port: 27123,
            token: Some("secret-token".to_string()),
        };
        let cmd = claude_ssh_cmd("myhost", "/remote/proj", &[], &[], Some(&tunnel));
        // Reverse forward present, before the host.
        assert!(cmd.contains("-R 27123:127.0.0.1:27123"), "cmd: {cmd}");
        assert!(
            cmd.find("-R ").unwrap() < cmd.find("myhost").unwrap(),
            "forward must precede host: {cmd}"
        );
        // Token exported before claude (the inner command is itself shell-quoted,
        // so assert presence + ordering rather than an exact quoted form).
        let env_pos = cmd.find("OBSIDIAN_API_KEY=").expect("token env present");
        let claude_pos = cmd.find("claude").unwrap();
        assert!(env_pos < claude_pos, "token must precede claude: {cmd}");
        assert!(cmd.contains("secret-token"), "cmd: {cmd}");
    }

    #[test]
    fn claude_ssh_cmd_tunnel_without_token_omits_env() {
        let tunnel = McpTunnel {
            host: "127.0.0.1".to_string(),
            port: 27123,
            token: None,
        };
        let cmd = claude_ssh_cmd("myhost", "/remote/proj", &[], &[], Some(&tunnel));
        assert!(cmd.contains("-R 27123:127.0.0.1:27123"), "cmd: {cmd}");
        assert!(!cmd.contains("OBSIDIAN_API_KEY"), "no token => no env: {cmd}");
    }

    #[test]
    fn claude_ssh_cmd_no_mcp_has_no_tunnel_or_token() {
        let cmd = claude_ssh_cmd("myhost", "/remote/proj", &[], &[], None);
        assert!(!cmd.contains("-R "), "cmd: {cmd}");
        assert!(!cmd.contains("OBSIDIAN_API_KEY"), "cmd: {cmd}");
    }

    #[test]
    fn tmux_session_name_sanitises_host() {
        assert_eq!(
            tmux_session_name("myproj", "user@host.example.com"),
            "jeru-myproj-user-host-example-com"
        );
    }
}
