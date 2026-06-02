use std::path::Path;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::project::{expand_tilde, load_manifest, project_dir};

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
    let expanded = expand_tilde(path)?;
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
            if manifest.repos.iter().any(|r| r == path) {
                return Err(Error::AlreadyExists(format!("repo '{path}'")));
            }
            manifest.repos.push(path.to_string());
        }
        Kind::Knowledge => {
            let id = knowledge_id(path)?;
            if manifest.knowledge_sets.iter().any(|k| k == &id) {
                return Err(Error::AlreadyExists(format!("knowledge set '{id}'")));
            }
            manifest.knowledge_sets.push(id);
        }
        Kind::Resource => {
            if manifest.resources.iter().any(|r| r == path) {
                return Err(Error::AlreadyExists(format!("resource '{path}'")));
            }
            manifest.resources.push(path.to_string());
        }
    }

    manifest.save_to_dir(&project_dir(name)?)
}

/// Derive the knowledge set ID from a path.
///
/// If the path is under the knowledge base dir, the ID is the relative suffix
/// (e.g. `~/knowledge/rust/async` → `rust/async`).  Otherwise the last path
/// component is used as a best-effort fallback.
fn knowledge_id(path: &str) -> Result<String> {
    let expanded = expand_tilde(path)?;
    let p = Path::new(&expanded);
    let config = Config::load()?;

    if let Ok(rel) = p.strip_prefix(&config.knowledge_dir) {
        return Ok(rel.to_string_lossy().into_owned());
    }

    Ok(p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string()))
}
