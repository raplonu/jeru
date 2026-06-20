use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::constants::CLAUDE_BIN;
use crate::error::{Error, Result};
use crate::mcp::{read_obsidian_api_key, write_mcp_json_for_dir};
use crate::project::{expand_tilde, load_manifest, project_dir};
use crate::settings::{write_settings, write_settings_for_dir};

/// What a local session needs to launch `claude` in a detached tmux window:
/// the working directory and the resolved Obsidian token (if any).
pub struct LocalLaunch {
    pub cwd: PathBuf,
    pub token: Option<String>,
}

/// Resolve the Obsidian API token: an already-set env var wins, otherwise read
/// it from the vault's Local REST API config. Returns `None` if neither exists.
pub fn resolve_obsidian_token(config: &Config) -> Option<String> {
    std::env::var(&config.obsidian_api_key_env)
        .ok()
        .or_else(|| read_obsidian_api_key(config))
}

/// Prepare a local session: write `.claude/settings.json` and `.mcp.json` for
/// the launch directory and resolve the Obsidian token, returning where to run
/// `claude` and the token to export. Mirrors [`claude_for_project`] /
/// [`claude_for_repos`] but yields data for a detached tmux launch instead of a
/// foreground [`Command`].
pub fn prepare_local_session(config: &Config, name: &str, repos: bool) -> Result<LocalLaunch> {
    let cwd = if repos {
        let manifest = load_manifest(config, name)?;
        let (first, rest) = manifest
            .repos
            .split_first()
            .ok_or_else(|| Error::NoRepos(name.to_string()))?;
        let cwd = expand_tilde(first)?;
        let add_dirs = rest
            .iter()
            .map(|repo| expand_tilde(repo).map(|p| p.to_string_lossy().into_owned()))
            .collect::<Result<Vec<_>>>()?;
        write_settings_for_dir(&cwd, &add_dirs)?;
        cwd
    } else {
        write_settings(config, name)?;
        project_dir(config, name)
    };
    write_mcp_json_for_dir(&cwd, config)?;
    let token = if config.obsidian_mcp_enabled {
        resolve_obsidian_token(config)
    } else {
        None
    };
    Ok(LocalLaunch { cwd, token })
}

fn claude_command(cwd: PathBuf, name: &str, extra: &[String]) -> Command {
    let mut cmd = Command::new(CLAUDE_BIN);
    cmd.current_dir(cwd);
    cmd.args(["--name", name]);
    cmd.args(extra);
    cmd
}

/// Inject the Obsidian API token into the spawned `claude` so the generated
/// `.mcp.json` (whose auth header references `${OBSIDIAN_API_KEY}`) can
/// authenticate without the user having to export the variable themselves.
///
/// Resolution mirrors the remote path: an already-set env var wins, otherwise
/// the token is read from the vault's Local REST API config. A missing token is
/// non-fatal — Claude just can't reach Obsidian, matching prior behaviour.
fn inject_obsidian_token(cmd: &mut Command, config: &Config) {
    if !config.obsidian_mcp_enabled {
        return;
    }
    let token = resolve_obsidian_token(config);
    match token {
        Some(token) => {
            cmd.env(&config.obsidian_api_key_env, token);
        }
        None => eprintln!(
            "warning: no Obsidian token (${} unset and none found in vault); \
             Claude won't be able to reach Obsidian",
            config.obsidian_api_key_env
        ),
    }
}

/// Build a `claude` invocation rooted at the project directory.  The project's
/// linked directories are written into `.claude/settings.json` so that any
/// `extra` subcommand or flags are forwarded to `claude` without interference.
///
/// `extra` is forwarded verbatim to `claude`.
pub fn claude_for_project(config: &Config, name: &str, extra: &[String]) -> Result<Command> {
    let cwd = project_dir(config, name);
    write_settings(config, name)?;
    write_mcp_json_for_dir(&cwd, config)?;
    let mut cmd = claude_command(cwd, name, extra);
    inject_obsidian_token(&mut cmd, config);
    Ok(cmd)
}

/// Build a `claude` invocation rooted at the project's first repo.  The
/// remaining repos are written into `.claude/settings.json` under that repo so
/// that any `extra` subcommand or flags are forwarded without interference.
///
/// `extra` is forwarded verbatim to `claude`.
pub fn claude_for_repos(config: &Config, name: &str, extra: &[String]) -> Result<Command> {
    let manifest = load_manifest(config, name)?;
    let (first, rest) = manifest
        .repos
        .split_first()
        .ok_or_else(|| Error::NoRepos(name.to_string()))?;

    let cwd = expand_tilde(first)?;
    let add_dirs = rest
        .iter()
        .map(|repo| expand_tilde(repo).map(|p| p.to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>>>()?;
    write_settings_for_dir(&cwd, &add_dirs)?;
    write_mcp_json_for_dir(&cwd, config)?;
    let mut cmd = claude_command(cwd, name, extra);
    inject_obsidian_token(&mut cmd, config);
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    /// A config whose token env var is a name guaranteed unset, so the lookup
    /// falls through to the vault (avoids racing on a shared process env var).
    fn config_with_vault(dir: &Path) -> Config {
        Config {
            projects_dir: dir.to_path_buf(),
            knowledge_dir: dir.to_path_buf(),
            cache_dir: dir.to_path_buf(),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: "http://127.0.0.1:27123/mcp/".to_string(),
            obsidian_api_key_env: "JERU_TEST_OBSIDIAN_TOKEN_DEFINITELY_UNSET".to_string(),
            obsidian_autostart: false,
            obsidian_launch_cmd: "false".to_string(),
        }
    }

    fn write_vault_token(dir: &Path, token: &str) {
        let plugin = dir.join(".obsidian/plugins/obsidian-local-rest-api");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("data.json"), format!(r#"{{"apiKey":"{token}"}}"#)).unwrap();
    }

    fn env_value(cmd: &Command, key: &str) -> Option<String> {
        cmd.get_envs().find_map(|(k, v)| {
            (k == OsStr::new(key)).then(|| v.map(|v| v.to_string_lossy().into_owned()))
        })?
    }

    #[test]
    fn injects_token_from_vault() {
        let dir = tempfile::tempdir().unwrap();
        write_vault_token(dir.path(), "vault-secret");
        let config = config_with_vault(dir.path());
        let mut cmd = Command::new("claude");
        inject_obsidian_token(&mut cmd, &config);
        assert_eq!(
            env_value(&cmd, &config.obsidian_api_key_env).as_deref(),
            Some("vault-secret")
        );
    }

    #[test]
    fn no_token_set_when_mcp_disabled() {
        let dir = tempfile::tempdir().unwrap();
        write_vault_token(dir.path(), "vault-secret");
        let mut config = config_with_vault(dir.path());
        config.obsidian_mcp_enabled = false;
        let mut cmd = Command::new("claude");
        inject_obsidian_token(&mut cmd, &config);
        assert!(env_value(&cmd, &config.obsidian_api_key_env).is_none());
    }

    #[test]
    fn no_token_set_when_vault_has_none() {
        let dir = tempfile::tempdir().unwrap(); // no data.json written
        let config = config_with_vault(dir.path());
        let mut cmd = Command::new("claude");
        inject_obsidian_token(&mut cmd, &config);
        assert!(env_value(&cmd, &config.obsidian_api_key_env).is_none());
    }
}
