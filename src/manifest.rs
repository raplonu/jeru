use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Candidate manifest filenames, in lookup order.
const MANIFEST_FILES: [&str; 2] = ["project.yml", "project.yaml"];

/// The `project.yml` manifest: the single source of truth for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,

    /// Optional default working repo for sustained code work.
    #[serde(default)]
    pub primary_repo: Option<String>,

    #[serde(default)]
    pub knowledge_sets: Vec<String>,

    #[serde(default)]
    pub repos: Vec<String>,

    #[serde(default)]
    pub resources: Vec<String>,
}

impl Manifest {
    /// Load the manifest from a project directory, trying each candidate
    /// filename in turn.
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let path = MANIFEST_FILES
            .iter()
            .map(|file| dir.join(file))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                Error::ManifestNotFound(dir.to_string_lossy().into_owned())
            })?;

        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml_ng::from_str(&content)?)
    }
}
