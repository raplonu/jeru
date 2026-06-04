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

/// Build the `ssh -t host 'cd … && claude …'` command string for tmux.
pub fn claude_ssh_cmd(
    host: &str,
    remote_project_path: &str,
    add_dirs: &[String],
    extra: &[String],
) -> String {
    let add = add_dirs
        .iter()
        .map(|d| format!("--add-dir {}", sq(d)))
        .collect::<Vec<_>>()
        .join(" ");
    let tail = extra.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ");
    let inner = format!(
        "cd {rp} && claude {add} {tail}",
        rp = sq(remote_project_path)
    );
    format!("ssh -t {host} {}", sq(&inner))
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
        );
        assert!(cmd.contains("'--flag with space'"));
    }

    #[test]
    fn tmux_session_name_sanitises_host() {
        assert_eq!(
            tmux_session_name("myproj", "user@host.example.com"),
            "jeru-myproj-user-host-example-com"
        );
    }
}
