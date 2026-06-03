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
pub fn detect_kind(path: &str) -> Result<Kind> {
    let expanded = to_absolute_path(path)?;
    let p = Path::new(&expanded);
    let config = Config::load()?;

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
pub fn add_to_project(name: &str, path: &str, kind: Kind) -> Result<()> {
    let mut manifest = load_manifest(name)?;

    match kind {
        Kind::Repo => {
            let abs = to_absolute_path(path)?;
            if manifest.repos.iter().any(|r| r == &abs) {
                return Err(Error::AlreadyExists(format!("repo '{abs}'")));
            }
            manifest.repos.push(abs);
        }
        Kind::Knowledge => {
            let id = knowledge_id(path)?;
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

    manifest.save_to_dir(&project_dir(name)?)
}

/// Remove `path` with the given `kind` from the manifest of project `name`.
///
/// For knowledge sets the path is converted to an ID before matching.
/// Returns an error if the entry is not found.
pub fn remove_from_project(name: &str, path: &str, kind: Kind) -> Result<()> {
    let mut manifest = load_manifest(name)?;

    match kind {
        Kind::Repo => {
            let pos = manifest
                .repos
                .iter()
                .position(|r| r == path)
                .ok_or_else(|| Error::NotFound(format!("repo '{path}'")))?;
            manifest.repos.remove(pos);
        }
        Kind::Knowledge => {
            let id = knowledge_id(path)?;
            let pos = manifest
                .knowledge_sets
                .iter()
                .position(|k| k == &id)
                .ok_or_else(|| Error::NotFound(format!("knowledge set '{id}'")))?;
            manifest.knowledge_sets.remove(pos);
        }
        Kind::Resource => {
            let pos = manifest
                .resources
                .iter()
                .position(|r| r == path)
                .ok_or_else(|| Error::NotFound(format!("resource '{path}'")))?;
            manifest.resources.remove(pos);
        }
    }

    manifest.save_to_dir(&project_dir(name)?)
}

/// Return all entries in the project manifest, optionally filtered by kind.
///
/// Each element is `(kind, entry)` where `entry` is the stored string
/// (path for repos and resources, ID for knowledge sets).
pub fn list_entries(name: &str, kind: Option<Kind>) -> Result<Vec<(Kind, String)>> {
    let manifest = load_manifest(name)?;
    let mut out = Vec::new();

    if matches!(kind, None | Some(Kind::Repo)) {
        for r in &manifest.repos {
            out.push((Kind::Repo, r.clone()));
        }
    }
    if matches!(kind, None | Some(Kind::Knowledge)) {
        for k in &manifest.knowledge_sets {
            out.push((Kind::Knowledge, k.clone()));
        }
    }
    if matches!(kind, None | Some(Kind::Resource)) {
        for r in &manifest.resources {
            out.push((Kind::Resource, r.clone()));
        }
    }

    Ok(out)
}

/// Derive the knowledge set ID from a path.
///
/// If the path is under the knowledge base dir, the ID is the relative suffix
/// (e.g. `~/knowledge/rust/async` → `rust/async`).  Otherwise the last path
/// component is used as a best-effort fallback.
fn knowledge_id(path: &str) -> Result<String> {
    let expanded = to_absolute_path(path)?;
    let p = Path::new(&expanded);
    let config = Config::load()?;

    if let Ok(rel) = p.strip_prefix(&config.knowledge_dir) {
        return Ok(rel.to_string_lossy().into_owned());
    }

    Ok(p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string()))
}
