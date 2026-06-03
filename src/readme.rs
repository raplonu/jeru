use std::path::PathBuf;
use std::process::Command;

use crate::constants::README_FILE;
use crate::error::Result;
use crate::project::project_dir;

/// Return the readme path for a project (`<project-dir>/README.md`).
pub fn effective_path(name: &str) -> Result<PathBuf> {
    Ok(project_dir(name)?.join(README_FILE))
}

/// Return the readme path to embed in `CLAUDE.md`, or `None` if the file does
/// not exist.
pub fn claude_md_path(name: &str) -> Result<Option<PathBuf>> {
    let path = project_dir(name)?.join(README_FILE);
    if path.exists() {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

/// Print the readme contents to stdout.
pub fn show(name: &str) -> Result<()> {
    let path = effective_path(name)?;
    if !path.exists() {
        println!("No README found at {}", path.display());
        println!("Run `jeru readme edit` to create one.");
        return Ok(());
    }
    print!("{}", std::fs::read_to_string(&path)?);
    Ok(())
}

/// Open the readme in `$EDITOR`, creating an empty file if it does not exist.
pub fn edit(name: &str) -> Result<()> {
    let path = effective_path(name)?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, "")?;
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    Command::new(&editor).arg(&path).status()?;
    Ok(())
}
