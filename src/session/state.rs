use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::constants::SESSIONS_DIR;
use crate::error::{Error, Result};
use crate::remote::tmux_name;

/// Persisted record of one background session, stored as a JSON file under
/// `cache_dir/sessions/`. This is the source of truth for `ls`/`stop`/`inspect`:
/// local sessions have no mutagen labels, so they cannot be enumerated otherwise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Display id: `project` (local) or `project@host` (remote).
    pub id: String,
    pub project: String,
    /// SSH host for a remote session; `None` for local.
    pub remote: Option<String>,
    /// `claude remote-control --spawn` mode.
    pub spawn: String,
    /// Local tmux session name (sanitised id).
    pub tmux: String,
    /// Remote tmux session name (remote sessions only).
    pub remote_tmux: Option<String>,
    /// mutagen session names to terminate on stop.
    #[serde(default)]
    pub mutagen_sessions: Vec<String>,
    /// Remote directories to clean up on stop.
    ///
    /// Excludes code repos: those are left on the remote and reconciled by the
    /// conflict manager on the next `session up` rather than wiped on stop.
    #[serde(default)]
    pub remote_dirs: Vec<String>,
    /// Whether to skip remote cleanup on stop.
    #[serde(default)]
    pub no_cleanup: bool,
    /// A URL/path the user can open in VSCode.
    pub vscode_url: String,
    /// Captured claude pane output (the remote-control URL etc.).
    #[serde(default)]
    pub claude_output: Option<String>,
    /// Unix epoch seconds when the session started.
    pub started_at: u64,
}

/// Build the display id for a session.
pub fn session_id(project: &str, remote: Option<&str>) -> String {
    match remote {
        Some(host) => format!("{project}@{host}"),
        None => project.to_string(),
    }
}

/// Seconds since the Unix epoch (0 if the clock is before it).
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl SessionState {
    /// Directory holding all session state files.
    pub fn dir(config: &Config) -> PathBuf {
        config.cache_dir.join(SESSIONS_DIR)
    }

    /// Path of the state file for a given id (filename is the sanitised id).
    fn path(config: &Config, id: &str) -> PathBuf {
        Self::dir(config).join(format!("{}.json", tmux_name(id)))
    }

    /// Write this session's state to disk.
    pub fn save(&self, config: &Config) -> Result<()> {
        let dir = Self::dir(config);
        std::fs::create_dir_all(&dir)?;
        let mut content = serde_json::to_string_pretty(self)?;
        content.push('\n');
        std::fs::write(Self::path(config, &self.id), content)?;
        Ok(())
    }

    /// Load the session with the exact id, if present.
    pub fn load(config: &Config, id: &str) -> Result<Option<Self>> {
        let path = Self::path(config, id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&std::fs::read_to_string(path)?)?))
    }

    /// All persisted sessions, sorted by id.
    pub fn list(config: &Config) -> Result<Vec<Self>> {
        let dir = Self::dir(config);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let s: Self = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
                out.push(s);
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Remove the state file for an id (a missing file is not an error).
    pub fn remove(config: &Config, id: &str) -> Result<()> {
        let path = Self::path(config, id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Resolve a user-supplied `query` to a single session: an exact id match
    /// wins; otherwise match uniquely by project name. Errors when nothing
    /// matches or the project name is ambiguous across multiple sessions.
    pub fn find(config: &Config, query: &str) -> Result<Self> {
        if let Some(exact) = Self::load(config, query)? {
            return Ok(exact);
        }
        let mut matches: Vec<Self> = Self::list(config)?
            .into_iter()
            .filter(|s| s.project == query)
            .collect();
        match matches.len() {
            0 => Err(Error::SessionNotFound(query.to_string())),
            1 => Ok(matches.pop().unwrap()),
            _ => {
                let ids = matches
                    .iter()
                    .map(|s| s.id.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(Error::SessionAmbiguous(query.to_string(), ids))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(dir: &std::path::Path) -> Config {
        Config {
            projects_dir: dir.to_path_buf(),
            knowledge_dir: dir.to_path_buf(),
            cache_dir: dir.to_path_buf(),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: "http://127.0.0.1:27123/mcp/".to_string(),
            obsidian_api_key_env: "OBSIDIAN_API_KEY".to_string(),
            obsidian_autostart: false,
            obsidian_launch_cmd: "false".to_string(),
        }
    }

    fn sample(id: &str, project: &str, remote: Option<&str>) -> SessionState {
        SessionState {
            id: id.to_string(),
            project: project.to_string(),
            remote: remote.map(String::from),
            spawn: "same-dir".to_string(),
            tmux: tmux_name(id),
            remote_tmux: remote.map(|_| format!("jeru-{project}")),
            mutagen_sessions: vec!["jeru-x".to_string()],
            remote_dirs: vec!["/home/u/p".to_string()],
            no_cleanup: false,
            vscode_url: "vscode://file/home/u/p".to_string(),
            claude_output: Some("https://claude.ai/code/x".to_string()),
            started_at: now_epoch(),
        }
    }

    #[test]
    fn session_id_local_and_remote() {
        assert_eq!(session_id("proj", None), "proj");
        assert_eq!(session_id("proj", Some("user@host")), "proj@user@host");
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let s = sample("proj@host.com", "proj", Some("host.com"));
        s.save(&config).unwrap();
        let loaded = SessionState::load(&config, "proj@host.com").unwrap().unwrap();
        assert_eq!(loaded.id, "proj@host.com");
        assert_eq!(loaded.remote.as_deref(), Some("host.com"));
        assert_eq!(loaded.remote_dirs, vec!["/home/u/p".to_string()]);
    }

    #[test]
    fn list_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        sample("a", "a", None).save(&config).unwrap();
        sample("b@h", "b", Some("h")).save(&config).unwrap();
        let all = SessionState::list(&config).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "a");
        SessionState::remove(&config, "a").unwrap();
        assert_eq!(SessionState::list(&config).unwrap().len(), 1);
    }

    #[test]
    fn find_exact_and_by_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        sample("proj@host", "proj", Some("host")).save(&config).unwrap();
        // Exact id.
        assert_eq!(SessionState::find(&config, "proj@host").unwrap().id, "proj@host");
        // Unique project name.
        assert_eq!(SessionState::find(&config, "proj").unwrap().id, "proj@host");
        // Missing.
        assert!(matches!(
            SessionState::find(&config, "nope"),
            Err(Error::SessionNotFound(_))
        ));
    }

    #[test]
    fn find_ambiguous_project() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        sample("proj@h1", "proj", Some("h1")).save(&config).unwrap();
        sample("proj@h2", "proj", Some("h2")).save(&config).unwrap();
        assert!(matches!(
            SessionState::find(&config, "proj"),
            Err(Error::SessionAmbiguous(_, _))
        ));
    }
}
