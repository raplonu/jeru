use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Candidate manifest filenames, in lookup order.
const MANIFEST_FILES: [&str; 2] = [crate::constants::MANIFEST_FILE, "project.yaml"];

/// Return the first manifest file that already exists in `dir`, or the default
/// filename if none do. Used by both `path_in_dir` and `save_to_dir`.
fn find_manifest_path(dir: &Path) -> std::path::PathBuf {
    MANIFEST_FILES
        .iter()
        .map(|file| dir.join(file))
        .find(|p| p.is_file())
        .unwrap_or_else(|| dir.join(MANIFEST_FILES[0]))
}

/// The `project.yml` manifest: the single source of truth for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,

    /// Subfolder under `knowledge/project/` shared by related projects.
    /// Defaults to `name` when absent in older manifests.
    #[serde(default)]
    pub knowledge_location: String,

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
    /// Return the path of the manifest file in `dir` (existing or default).
    pub fn path_in_dir(dir: &Path) -> Result<std::path::PathBuf> {
        Ok(find_manifest_path(dir))
    }

    /// Write the manifest back to its directory, overwriting the existing file.
    ///
    /// Uses the first candidate filename that already exists, or falls back to
    /// `project.yml` for new projects.
    pub fn save_to_dir(&self, dir: &Path) -> Result<()> {
        let mut content = serde_yaml_ng::to_string(self)?;
        content.push('\n');
        std::fs::write(find_manifest_path(dir), content)?;
        Ok(())
    }

    /// Load the manifest from a project directory, trying each candidate
    /// filename in turn.
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let path = MANIFEST_FILES
            .iter()
            .map(|file| dir.join(file))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| Error::ManifestNotFound(dir.to_string_lossy().into_owned()))?;

        let content = std::fs::read_to_string(path)?;
        let mut m: Self = serde_yaml_ng::from_str(&content)?;
        if m.knowledge_location.is_empty() {
            m.knowledge_location = m.name.clone();
        }
        Ok(m)
    }
}
