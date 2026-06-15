use std::path::Path;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::project::{load_manifest, project_dir, to_absolute_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Repo,
    Knowledge,
    Resource,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Repo => "repo",
            Kind::Knowledge => "knowledge",
            Kind::Resource => "resource",
        }
    }
}

/// Deduce the kind of `path` from its location.
///
/// - Under the knowledge base dir → [`Kind::Knowledge`]
/// - Has `.git` or is an existing directory → [`Kind::Repo`]
/// - Otherwise → [`Kind::Resource`]
pub fn detect_kind(config: &Config, path: &str) -> Result<Kind> {
    let expanded = to_absolute_path(path)?;
    let p = Path::new(&expanded);

    if p.starts_with(&config.knowledge_dir) {
        return Ok(Kind::Knowledge);
    }

    if p.join(".git").exists() || p.is_dir() {
        return Ok(Kind::Repo);
    }

    if p.is_file() || p.extension().is_some() {
        return Ok(Kind::Resource);
    }

    // Path doesn't exist and has no extension: assume repo
    Ok(Kind::Repo)
}

/// Add `path` with the given `kind` to the manifest of project `name`.
///
/// For knowledge sets the path is converted to an ID relative to the
/// knowledge base directory.  Returns an error if the entry is already
/// present.
pub fn add_to_project(config: &Config, name: &str, path: &str, kind: Kind) -> Result<()> {
    let mut manifest = load_manifest(config, name)?;

    match kind {
        Kind::Repo => {
            let abs = to_absolute_path(path)?;
            if manifest.repos.iter().any(|r| r == &abs) {
                return Err(Error::AlreadyExists(format!("repo '{abs}'")));
            }
            manifest.repos.push(abs);
        }
        Kind::Knowledge => {
            let id = knowledge_id(config, path)?;
            if manifest.knowledge_sets.iter().any(|k| k == &id) {
                return Err(Error::AlreadyExists(format!("knowledge set '{id}'")));
            }
            manifest.knowledge_sets.push(id);
        }
        Kind::Resource => {
            let abs = to_absolute_path(path)?;
            if manifest.resources.iter().any(|r| r == &abs) {
                return Err(Error::AlreadyExists(format!("resource '{abs}'")));
            }
            manifest.resources.push(abs);
        }
    }

    manifest.save_to_dir(&project_dir(config, name))
}

/// Remove `path` with the given `kind` from the manifest of project `name`.
///
/// For knowledge sets the path is converted to an ID before matching.
/// For repos and resources the path is normalised to an absolute path (matching
/// how `add_to_project` stores entries) before comparing.
/// Returns an error if the entry is not found.
pub fn remove_from_project(config: &Config, name: &str, path: &str, kind: Kind) -> Result<()> {
    let mut manifest = load_manifest(config, name)?;

    match kind {
        Kind::Repo => {
            let abs = to_absolute_path(path)?;
            let pos = manifest
                .repos
                .iter()
                .position(|r| r == &abs)
                .ok_or_else(|| Error::NotFound(format!("repo '{abs}'")))?;
            manifest.repos.remove(pos);
        }
        Kind::Knowledge => {
            let id = knowledge_id(config, path)?;
            let pos = manifest
                .knowledge_sets
                .iter()
                .position(|k| k == &id)
                .ok_or_else(|| Error::NotFound(format!("knowledge set '{id}'")))?;
            manifest.knowledge_sets.remove(pos);
        }
        Kind::Resource => {
            let abs = to_absolute_path(path)?;
            let pos = manifest
                .resources
                .iter()
                .position(|r| r == &abs)
                .ok_or_else(|| Error::NotFound(format!("resource '{abs}'")))?;
            manifest.resources.remove(pos);
        }
    }

    manifest.save_to_dir(&project_dir(config, name))
}

/// Derive the knowledge set ID from a path.
///
/// The path must be under the knowledge base directory; the ID is the relative
/// suffix (e.g. `~/knowledge/rust/async` → `rust/async`).  Returns an error
/// for paths outside the knowledge directory to avoid silent ID collisions.
fn knowledge_id(config: &Config, path: &str) -> Result<String> {
    let expanded = to_absolute_path(path)?;
    let p = Path::new(&expanded);

    p.strip_prefix(&config.knowledge_dir)
        .map(|rel| rel.to_string_lossy().into_owned())
        .map_err(|_| {
            Error::NotFound(format!(
                "path '{}' is not under the knowledge directory '{}'",
                expanded,
                config.knowledge_dir.display()
            ))
        })
}
