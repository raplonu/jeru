use std::path::PathBuf;

use crate::cache;
use crate::config::Config;
use crate::constants::CLAUDE_MD_FILE;
use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::roadmap;
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
pub fn use_project(name: &str) -> Result<()> {
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
    let roadmap = roadmap::claude_md_path(name)?;
    let readme = crate::readme::claude_md_path(name)?;
    let rendered = template::render_claude_md(&manifest, roadmap.as_deref(), readme.as_deref())?;
    std::fs::write(&dest, rendered)?;
    Ok(dest)
}

/// Create a new project directory and write a minimal manifest.
///
/// - Directory absent: created normally.
/// - Directory present, already has a manifest: always errors.
/// - Directory present, empty: proceeds without `force`.
/// - Directory present, non-empty, no manifest: requires `force`.
pub fn create_project(name: &str, force: bool) -> Result<PathBuf> {
    let dir = project_dir(name)?;
    if dir.is_dir() {
        if Manifest::load_from_dir(&dir).is_ok() {
            return Err(Error::AlreadyExists(dir.to_string_lossy().into_owned()));
        }
        let non_empty = std::fs::read_dir(&dir)?.next().is_some();
        if non_empty && !force {
            return Err(Error::DirectoryNotEmpty(dir.to_string_lossy().into_owned()));
        }
    } else {
        std::fs::create_dir_all(&dir)?;
    }
    let manifest = Manifest {
        name: name.to_string(),
        primary_repo: None,
        knowledge_sets: Vec::new(),
        repos: Vec::new(),
        resources: Vec::new(),
    };
    manifest.save_to_dir(&dir)?;
    Ok(dir)
}

/// Open the project manifest in `$EDITOR`.
pub fn edit_manifest(name: &str) -> Result<()> {
    use std::process::Command;
    let dir = project_dir(name)?;
    if !dir.is_dir() {
        return Err(Error::ProjectNotFound(name.to_string()));
    }
    let path = Manifest::path_in_dir(&dir)?;
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    Command::new(&editor).arg(&path).status()?;
    Ok(())
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
        if entry.file_type()?.is_dir() && Manifest::load_from_dir(&entry.path()).is_ok() {
            projects.push(Project {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
            });
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}
