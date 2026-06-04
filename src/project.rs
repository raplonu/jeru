use std::path::PathBuf;

use crate::cache;
use crate::config::Config;
use crate::constants::{CLAUDE_MD_FILE, README_FILE, ROADMAP_FILE};
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
pub fn projects_dir(config: &Config) -> PathBuf {
    config.projects_dir.clone()
}

/// Directory for a single named project under the project tree.
pub fn project_dir(config: &Config, name: &str) -> PathBuf {
    config.projects_dir.join(name)
}

/// Directory a knowledge set ID resolves to.
pub fn knowledge_dir(config: &Config, id: &str) -> PathBuf {
    config.knowledge_dir.join(id)
}

/// Expand a leading `~` in a manifest path to the user's home directory.
/// Other paths are returned unchanged.
pub fn expand_tilde(path: &str) -> Result<PathBuf> {
    let rest = match path.strip_prefix("~/") {
        Some(rest) => rest,
        None if path == "~" => "",
        None => return Ok(PathBuf::from(path)),
    };
    let home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    Ok(home.join(rest))
}

/// Resolve a path to an absolute path.
///
/// Expands `~` and, if the result is still relative, resolves it against the
/// current working directory.
pub fn to_absolute_path(path: &str) -> Result<String> {
    let expanded = expand_tilde(path)?;
    if expanded.is_absolute() {
        return Ok(expanded.to_string_lossy().into_owned());
    }
    let abs = std::env::current_dir()?.join(expanded);
    Ok(abs.to_string_lossy().into_owned())
}

/// Set a project as the current one for subsequent commands.
///
/// Fails if the project directory does not exist.
pub fn use_project(config: &Config, name: &str) -> Result<()> {
    if !project_dir(config, name).is_dir() {
        return Err(Error::ProjectNotFound(name.to_string()));
    }
    cache::set_current_project(config, name)
}

/// Load the manifest for a named project.
pub fn load_manifest(config: &Config, name: &str) -> Result<Manifest> {
    let dir = project_dir(config, name);
    if !dir.is_dir() {
        return Err(Error::ProjectNotFound(name.to_string()));
    }
    Manifest::load_from_dir(&dir)
}

/// Generate the project `CLAUDE.md` from its manifest.
///
/// Refuses to overwrite an existing file unless `force` is set, and returns the
/// path written.
pub fn init_claude_md(config: &Config, name: &str, force: bool) -> Result<PathBuf> {
    let manifest = load_manifest(config, name)?;
    let dir = project_dir(config, name);
    let dest = dir.join(CLAUDE_MD_FILE);
    if dest.exists() && !force {
        return Err(Error::AlreadyExists(dest.to_string_lossy().into_owned()));
    }
    let roadmap = dir.join(ROADMAP_FILE);
    let roadmap = roadmap.exists().then_some(roadmap);
    let readme = dir.join(README_FILE);
    let readme = readme.exists().then_some(readme);
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
pub fn create_project(config: &Config, name: &str, force: bool) -> Result<PathBuf> {
    let dir = project_dir(config, name);
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

/// List projects found under the project tree.
///
/// A missing project tree is not an error — it yields an empty list.
pub fn list_projects(config: &Config) -> Result<Vec<Project>> {
    let dir = projects_dir(config);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_path_is_unchanged() {
        let result = to_absolute_path("/usr/local/bin").unwrap();
        assert_eq!(result, "/usr/local/bin");
    }

    #[test]
    fn tilde_path_becomes_absolute() {
        let result = to_absolute_path("~/foo/bar").unwrap();
        let p = std::path::Path::new(&result);
        assert!(p.is_absolute(), "expected absolute path, got {result}");
        assert!(result.ends_with("foo/bar"));
        assert!(!result.contains('~'));
    }

    #[test]
    fn relative_path_resolved_against_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let result = to_absolute_path("foo/bar").unwrap();
        let expected = cwd.join("foo/bar").to_string_lossy().into_owned();
        assert_eq!(result, expected);
    }

    #[test]
    fn dot_slash_relative_path_is_absolute() {
        let result = to_absolute_path("./my-repo").unwrap();
        assert!(
            std::path::Path::new(&result).is_absolute(),
            "expected absolute path, got {result}"
        );
        assert!(!result.starts_with("./"));
    }
}
