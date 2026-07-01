use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::config::Config;
use crate::constants::{CODE_BIN, WORKSPACE_EXT};
use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::project::{expand_tilde, load_manifest, project_dir};

/// Path of the project's generated `.code-workspace` file.
pub fn workspace_path(config: &Config, name: &str) -> PathBuf {
    project_dir(config, name).join(format!("{name}{WORKSPACE_EXT}"))
}

/// Build the `.code-workspace` JSON for `manifest`'s repos, resolving each
/// repo's path to a folder entry via `resolve`.
fn build_workspace(manifest: &Manifest, resolve: impl Fn(&str) -> Result<String>) -> Result<Value> {
    let folders = manifest
        .repos
        .iter()
        .map(|repo| {
            let path_str = resolve(repo)?;
            Ok(json!({ "name": folder_name(&path_str), "path": path_str }))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({ "folders": folders, "settings": {} }))
}

/// Generate the VSCode workspace for a project, listing its repos as folders.
/// Returns the path written.
pub fn write_workspace(config: &Config, name: &str) -> Result<PathBuf> {
    let manifest = load_manifest(config, name)?;
    if manifest.repos.is_empty() {
        return Err(Error::NoRepos(name.to_string()));
    }

    let workspace = build_workspace(&manifest, |repo| {
        Ok(expand_tilde(repo)?.to_string_lossy().into_owned())
    })?;
    let path = workspace_path(config, name);
    let mut content = serde_json::to_string_pretty(&workspace)?;
    content.push('\n');
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Render the `.code-workspace` content for `name`'s repos with folder paths
/// mapped to their locations on the remote host under `remote_home`.
///
/// Returns the rendered JSON content; writes nothing.
pub fn remote_workspace_content(
    config: &Config,
    name: &str,
    local_home: &Path,
    remote_home: &str,
) -> Result<String> {
    let manifest = load_manifest(config, name)?;
    if manifest.repos.is_empty() {
        return Err(Error::NoRepos(name.to_string()));
    }

    let workspace = build_workspace(&manifest, |repo| {
        crate::remote::to_remote(&expand_tilde(repo)?, local_home, remote_home)
    })?;
    let mut content = serde_json::to_string_pretty(&workspace)?;
    content.push('\n');
    Ok(content)
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

/// Build a `code` invocation that opens a directory as a folder.
pub fn code_folder(dir: &Path) -> Command {
    let mut cmd = Command::new(CODE_BIN);
    cmd.arg(dir);
    cmd
}

/// Open a VSCode URI non-interactively (fire-and-forget).
///
/// Works for both `vscode://file/…` (local) and `vscode://vscode-remote/…`
/// (remote SSH) URIs. Errors are silently ignored — VSCode opening is best-effort.
pub fn open_url(url: &str) {
    let _ = Command::new(CODE_BIN)
        .args(["--folder-uri", url])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Wrap `url` in an OSC 8 terminal hyperlink escape sequence.
///
/// Custom URI schemes like `vscode://` or `vscode-remote://` are often not
/// recognised by a terminal's own URL-detection heuristics, leaving them
/// unclickable (or only partially clickable). OSC 8 explicitly marks the link
/// boundaries, so terminals that support it (most modern ones) make the whole
/// string clickable regardless of scheme.
pub fn osc8_link(url: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\")
}

fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_workspace_maps_repo_paths() {
        let manifest = Manifest {
            name: "proj".to_string(),
            knowledge_location: "proj".to_string(),
            primary_repo: None,
            knowledge_sets: Vec::new(),
            repos: vec!["~/code/r1".to_string(), "~/code/r2".to_string()],
            resources: Vec::new(),
        };
        let workspace =
            build_workspace(&manifest, |repo| Ok(format!("/remote/{}", repo.trim_start_matches("~/")))).unwrap();
        let folders = workspace["folders"].as_array().unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0]["path"], "/remote/code/r1");
        assert_eq!(folders[0]["name"], "r1");
        assert_eq!(folders[1]["path"], "/remote/code/r2");
    }
}

