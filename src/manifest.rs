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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_repo: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge_sets: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
}

impl Manifest {
    /// Write the manifest back to its directory, overwriting the existing file.
    ///
    /// Uses the first candidate filename that already exists, or falls back to
    /// `project.yml` for new projects.
    pub fn save_to_dir(&self, dir: &Path) -> Result<()> {
        let path = MANIFEST_FILES
            .iter()
            .map(|file| dir.join(file))
            .find(|p| p.is_file())
            .unwrap_or_else(|| dir.join(MANIFEST_FILES[0]));
        let mut content = serde_yaml_ng::to_string(self)?;
        content.push('\n');
        std::fs::write(path, content)?;
        Ok(())
    }

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
