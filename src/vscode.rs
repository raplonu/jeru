use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

use crate::constants::{CODE_BIN, WORKSPACE_EXT};
use crate::error::{Error, Result};
use crate::project::{expand_tilde, load_manifest, project_dir};

/// Path of the project's generated `.code-workspace` file.
pub fn workspace_path(name: &str) -> Result<PathBuf> {
    Ok(project_dir(name)?.join(format!("{name}{WORKSPACE_EXT}")))
}

/// Generate the VSCode workspace for a project, listing its repos as folders.
/// Returns the path written.
pub fn write_workspace(name: &str) -> Result<PathBuf> {
    let manifest = load_manifest(name)?;
    if manifest.repos.is_empty() {
        return Err(Error::NoRepos(name.to_string()));
    }

    let folders = manifest
        .repos
        .iter()
        .map(|repo| {
            let path = expand_tilde(repo)?;
            Ok(json!({ "name": folder_name(&path), "path": path }))
        })
        .collect::<Result<Vec<_>>>()?;

    let workspace = json!({ "folders": folders, "settings": {} });
    let path = workspace_path(name)?;
    let mut content = serde_json::to_string_pretty(&workspace)?;
    content.push('\n');
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Build a `code` invocation that opens a workspace file.
///
/// `extra` is forwarded verbatim to `code`.
pub fn code_command(workspace: &Path, extra: &[String]) -> Command {
    let mut cmd = Command::new(CODE_BIN);
    cmd.arg(workspace);
    cmd.args(extra);
    cmd
}

fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}
