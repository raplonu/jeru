use std::path::PathBuf;

use crate::cache;
use crate::config::Config;
use crate::constants::CLAUDE_MD_FILE;
use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::template;

/// A project: a directory living under the project tree.
#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
}

/// Root of the project tree.
pub fn projects_dir() -> Result<PathBuf> {
    Ok(Config::load()?.projects_dir)
}

/// Directory for a single named project under the project tree.
pub fn project_dir(name: &str) -> Result<PathBuf> {
    Ok(projects_dir()?.join(name))
}

/// Expand a leading `~` in a manifest path to the user's home directory.
/// Other paths are returned unchanged.
pub fn expand_tilde(path: &str) -> Result<String> {
    let rest = match path.strip_prefix("~/") {
        Some(rest) => rest,
        None if path == "~" => "",
        None => return Ok(path.to_string()),
    };
    let home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    Ok(home.join(rest).to_string_lossy().into_owned())
}

/// Directory a knowledge set ID resolves to.
pub fn knowledge_dir(id: &str) -> Result<PathBuf> {
    Ok(Config::load()?.knowledge_dir.join(id))
}

/// Set a project as the current one for subsequent commands.
///
/// Fails if the project directory does not exist.
pub fn workon(name: &str) -> Result<()> {
    if !project_dir(name)?.is_dir() {
        return Err(Error::ProjectNotFound(name.to_string()));
    }
    cache::set_current_project(name)
}

/// Load the manifest for a named project.
pub fn load_manifest(name: &str) -> Result<Manifest> {
    let dir = project_dir(name)?;
    if !dir.is_dir() {
        return Err(Error::ProjectNotFound(name.to_string()));
    }
    Manifest::load_from_dir(&dir)
}

/// Generate the project `CLAUDE.md` from its manifest.
///
/// Refuses to overwrite an existing file unless `force` is set, and returns the
/// path written.
pub fn init_claude_md(name: &str, force: bool) -> Result<PathBuf> {
    let manifest = load_manifest(name)?;
    let dest = project_dir(name)?.join(CLAUDE_MD_FILE);
    if dest.exists() && !force {
        return Err(Error::AlreadyExists(dest.to_string_lossy().into_owned()));
    }
    let rendered = template::render_claude_md(&manifest)?;
    std::fs::write(&dest, rendered)?;
    Ok(dest)
}

/// List projects found under the project tree.
///
/// A missing project tree is not an error — it yields an empty list.
pub fn list_projects() -> Result<Vec<Project>> {
    let dir = projects_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            projects.push(Project {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
            });
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}
