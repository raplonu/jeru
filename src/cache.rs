use std::path::PathBuf;

use crate::config::Config;
use crate::constants::CURRENT_PROJECT_FILE;
use crate::error::{Error, Result};

fn current_project_file(config: &Config) -> PathBuf {
    config.cache_dir.join(CURRENT_PROJECT_FILE)
}

/// The project currently being worked on, if any.
pub fn current_project(config: &Config) -> Result<Option<String>> {
    let path = current_project_file(config);
    if !path.exists() {
        return Ok(None);
    }
    let name = std::fs::read_to_string(path)?.trim().to_string();
    Ok((!name.is_empty()).then_some(name))
}

/// Persist the current project name.
pub fn set_current_project(config: &Config, name: &str) -> Result<()> {
    let path = current_project_file(config);
    std::fs::create_dir_all(path.parent().expect("cache file has a parent"))?;
    std::fs::write(path, name)?;
    Ok(())
}

/// Resolve a project name from an optional argument, falling back to the
/// current project set via `jeru use`.
pub fn resolve_project(config: &Config, name: Option<String>) -> Result<String> {
    match name {
        Some(name) => Ok(name),
        None => current_project(config)?.ok_or(Error::NoCurrentProject),
    }
}
