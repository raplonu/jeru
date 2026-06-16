use crate::config::Config;
use crate::error::Result;
use crate::remote::{
    mutagen_terminate, remote_kill_tmux, remote_rm_dirs, tmux_attach, tmux_has_session,
    tmux_kill_session,
};

use super::state::{SessionState, now_epoch};

/// Stop a session: gracefully end claude, tear down tmux + mutagen, and clean up
/// remote directories (unless the session was started with `--no-cleanup`).
pub fn stop(config: &Config, query: &str) -> Result<()> {
    let state = SessionState::find(config, query)?;
    println!("Stopping session '{}'…", state.id);

    if let Some(host) = &state.remote {
        // End claude gracefully by killing the remote tmux session it runs in.
        if let Some(remote_tmux) = &state.remote_tmux {
            eprint!("Stopping remote claude… ");
            remote_kill_tmux(host, remote_tmux)?;
            eprintln!("done");
        }
        // Kill the local tmux session (stops the reconnect loop + sync monitor).
        tmux_kill_session(&state.tmux)?;
        // Remove the reconnect-loop script written at session start.
        let script_path = SessionState::dir(config).join(format!("{}-remote-loop.sh", state.tmux));
        let _ = std::fs::remove_file(script_path);
        // Terminate mutagen sessions.
        if !state.mutagen_sessions.is_empty() {
            eprint!("Stopping mutagen sessions… ");
            mutagen_terminate(&state.mutagen_sessions);
            eprintln!("done");
        }
        // Clean up the remote tree unless asked not to.
        if !state.no_cleanup && !state.remote_dirs.is_empty() {
            eprint!("Cleaning up remote directories… ");
            remote_rm_dirs(host, &state.remote_dirs)?;
            eprintln!("done");
        }
    } else {
        tmux_kill_session(&state.tmux)?;
    }

    SessionState::remove(config, &state.id)?;
    println!("Session '{}' stopped.", state.id);
    Ok(())
}

/// Stop all active sessions.
pub fn stop_all(config: &Config) -> Result<()> {
    let sessions = SessionState::list(config)?;
    if sessions.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }
    for s in sessions {
        stop(config, &s.id)?;
    }
    Ok(())
}

/// List active session IDs (one per line).
pub fn list(config: &Config) -> Result<()> {
    let sessions = SessionState::list(config)?;
    if sessions.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }
    for s in sessions {
        println!("{}", s.id);
    }
    Ok(())
}

/// Show detailed info for one session.
pub fn info(config: &Config, query: &str) -> Result<()> {
    let state = SessionState::find(config, query)?;
    print_session_info(&state);
    Ok(())
}

/// Show detailed info for all active sessions.
pub fn info_all(config: &Config) -> Result<()> {
    let sessions = SessionState::list(config)?;
    if sessions.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }
    for (i, s) in sessions.iter().enumerate() {
        if i > 0 {
            println!();
        }
        print_session_info(s);
    }
    Ok(())
}

/// Print the canonical session detail block (shared with session startup output).
pub fn print_session_info(state: &SessionState) {
    let status = if tmux_has_session(&state.tmux) { "running" } else { "dead" };
    let scope = match &state.remote {
        Some(host) => format!("remote {host}"),
        None => "local".to_string(),
    };
    println!("Session '{}' [{status}]", state.id);
    println!("  Scope:   {scope}");
    println!("  Spawn:   {}", state.spawn);
    println!("  Age:     {}", human_age(state.started_at));
    println!("  VSCode:  {}", crate::vscode::osc8_link(&state.vscode_url));
    match &state.claude_output {
        Some(text) if !text.trim().is_empty() => {
            println!("  Claude:");
            for line in text.trim_end().lines() {
                println!("    {line}");
            }
        }
        _ => println!(
            "  Claude:  (no output captured yet — `jeru session attach {}`)",
            state.id
        ),
    }
}

/// Attach to (or switch to) a session's local tmux. Blocks until detach.
pub fn inspect(config: &Config, query: &str) -> Result<()> {
    let state = SessionState::find(config, query)?;
    tmux_attach(&state.tmux)
}

/// Render an elapsed-since-start string like `3m`, `2h`, `1d`.
fn human_age(started_at: u64) -> String {
    let secs = now_epoch().saturating_sub(started_at);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_age_units() {
        let now = now_epoch();
        assert_eq!(human_age(now), "0s");
        assert_eq!(human_age(now.saturating_sub(90)), "1m");
        assert_eq!(human_age(now.saturating_sub(7200)), "2h");
        assert_eq!(human_age(now.saturating_sub(172800)), "2d");
    }
}
