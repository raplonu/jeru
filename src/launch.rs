use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::constants::CLAUDE_BIN;
use crate::error::{Error, Result};
use crate::project::{expand_tilde, load_manifest, project_dir};
use crate::settings::additional_directories;

fn claude_command(cwd: PathBuf, add_dirs: &[String], extra: &[String]) -> Command {
    let mut cmd = Command::new(CLAUDE_BIN);
    cmd.current_dir(cwd);
    for dir in add_dirs {
        cmd.arg("--add-dir").arg(dir);
    }
    cmd.args(extra);
    cmd
}

/// Build a `claude` invocation rooted at the project directory, with every
/// linked folder (repos, knowledge sets, resources) available.
///
/// `extra` is forwarded verbatim to `claude`.
pub fn claude_for_project(config: &Config, name: &str, extra: &[String]) -> Result<Command> {
    let manifest = load_manifest(config, name)?;
    let cwd = project_dir(config, name);
    let add_dirs = additional_directories(config, &manifest)?;
    Ok(claude_command(cwd, &add_dirs, extra))
}

/// Build a `claude` invocation rooted at the project's first repo, with the
/// remaining repos available as additional directories.
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
    Ok(claude_command(cwd, &add_dirs, extra))
}
