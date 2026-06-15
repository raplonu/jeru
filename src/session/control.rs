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

/// List all sessions with their status and URLs.
pub fn list(config: &Config) -> Result<()> {
    let sessions = SessionState::list(config)?;
    if sessions.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }
    for s in sessions {
        let status = if tmux_has_session(&s.tmux) {
            "running"
        } else {
            "dead"
        };
        let scope = match &s.remote {
            Some(host) => format!("remote {host}"),
            None => "local".to_string(),
        };
        println!(
            "{id}  [{status}]  {scope}  spawn={spawn}  up {age}",
            id = s.id,
            spawn = s.spawn,
            age = human_age(s.started_at),
        );
        println!("    VSCode: {}", s.vscode_url);
    }
    Ok(())
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
