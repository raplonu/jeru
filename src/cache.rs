use std::path::PathBuf;

use crate::error::{Error, Result};

/// Cache directory for jeru state, under the user's cache dir.
fn cache_dir() -> Result<PathBuf> {
    let dir = dirs::cache_dir().ok_or(Error::NoCacheDir)?;
    Ok(dir.join("jeru"))
}

fn current_project_file() -> Result<PathBuf> {
    Ok(cache_dir()?.join("current_project"))
}

/// The project currently being worked on, if any.
pub fn current_project() -> Result<Option<String>> {
    let path = current_project_file()?;
    if !path.exists() {
        return Ok(None);
    }
    let name = std::fs::read_to_string(path)?.trim().to_string();
    Ok((!name.is_empty()).then_some(name))
}

/// Persist the current project name.
pub fn set_current_project(name: &str) -> Result<()> {
    let path = current_project_file()?;
    std::fs::create_dir_all(path.parent().expect("cache file has a parent"))?;
    std::fs::write(path, name)?;
    Ok(())
}

/// Resolve a project name from an optional argument, falling back to the
/// current project set via `jeru workon`.
pub fn resolve_project(name: Option<String>) -> Result<String> {
    match name {
        Some(name) => Ok(name),
        None => current_project()?.ok_or(Error::NoCurrentProject),
    }
}
