use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::constants::CLAUDE_BIN;
use crate::error::{Error, Result};
use crate::mcp::write_mcp_json_for_dir;
use crate::project::{expand_tilde, load_manifest, project_dir};
use crate::settings::{write_settings, write_settings_for_dir};

fn claude_command(cwd: PathBuf, extra: &[String]) -> Command {
    let mut cmd = Command::new(CLAUDE_BIN);
    cmd.current_dir(cwd);
    cmd.args(extra);
    cmd
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
    Ok(claude_command(cwd, extra))
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
    Ok(claude_command(cwd, extra))
}
