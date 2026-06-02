use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::constants::{ADDITIONAL_DIRS_KEY, CLAUDE_DIR, SETTINGS_FILE};
use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::project::{expand_tilde, knowledge_dir, load_manifest, project_dir};

/// All directories a project links — primary repo, repos, resolved knowledge
/// sets, and resources — as absolute paths, deduplicated and order-preserving.
pub fn additional_directories(manifest: &Manifest) -> Result<Vec<String>> {
    let mut dirs = Vec::new();

    if let Some(primary) = &manifest.primary_repo {
        dirs.push(expand_tilde(primary)?);
    }
    for repo in &manifest.repos {
        dirs.push(expand_tilde(repo)?);
    }
    for id in &manifest.knowledge_sets {
        dirs.push(knowledge_dir(id)?.to_string_lossy().into_owned());
    }
    for resource in &manifest.resources {
        dirs.push(expand_tilde(resource)?);
    }

    let mut seen = HashSet::new();
    dirs.retain(|dir| seen.insert(dir.clone()));
    Ok(dirs)
}

/// Generate (or update) `.claude/settings.json` for a project so Claude Code
/// can read every folder the project links.
///
/// Existing settings are preserved: only `permissions.additionalDirectories`
/// is rewritten. Returns the path written.
pub fn write_settings(name: &str) -> Result<PathBuf> {
    let manifest = load_manifest(name)?;
    let dirs = additional_directories(&manifest)?;

    let claude_dir = project_dir(name)?.join(CLAUDE_DIR);
    std::fs::create_dir_all(&claude_dir)?;
    let path = claude_dir.join(SETTINGS_FILE);

    let mut root = if path.exists() {
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        match value {
            Value::Object(map) => map,
            _ => return Err(Error::InvalidSettings(path.to_string_lossy().into_owned())),
        }
    } else {
        Map::new()
    };

    let permissions = root
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    if !permissions.is_object() {
        return Err(Error::InvalidSettings(path.to_string_lossy().into_owned()));
    }
    permissions[ADDITIONAL_DIRS_KEY] = json!(dirs);

    let mut content = serde_json::to_string_pretty(&Value::Object(root))?;
    content.push('\n');
    std::fs::write(&path, content)?;
    Ok(path)
}
