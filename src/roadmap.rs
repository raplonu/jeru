use std::path::PathBuf;
use std::process::Command;

use crate::constants::ROADMAP_FILE;
use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::project::{expand_tilde, load_manifest, project_dir};

// ── path resolution ───────────────────────────────────────────────────────────

/// Return the effective roadmap path for a project.
///
/// Priority: manifest `roadmap` field → `<project-dir>/ROADMAP.md`.
pub fn effective_path(name: &str) -> Result<PathBuf> {
    effective_path_from(&load_manifest(name)?, name)
}

/// Same as [`effective_path`] but reuses an already-loaded manifest.
pub fn effective_path_from(manifest: &Manifest, name: &str) -> Result<PathBuf> {
    match &manifest.roadmap {
        Some(custom) => Ok(PathBuf::from(expand_tilde(custom)?)),
        None => Ok(project_dir(name)?.join(ROADMAP_FILE)),
    }
}

/// Return the roadmap path to embed in `CLAUDE.md`, or `None` if no roadmap
/// should be referenced.
///
/// - If `manifest.roadmap` is set, always include it.
/// - Otherwise include the default path only if the file actually exists.
pub fn claude_md_path(manifest: &Manifest, name: &str) -> Result<Option<PathBuf>> {
    if manifest.roadmap.is_some() {
        return Ok(Some(effective_path_from(manifest, name)?));
    }
    let default = project_dir(name)?.join(ROADMAP_FILE);
    if default.exists() {
        Ok(Some(default))
    } else {
        Ok(None)
    }
}

// ── show ──────────────────────────────────────────────────────────────────────

struct Section {
    heading: String,
    done: usize,
    total: usize,
    open: Vec<String>,
}

/// Parse the roadmap and print a per-section summary of open/closed items.
pub fn show(name: &str) -> Result<()> {
    let path = effective_path(name)?;
    if !path.exists() {
        println!("No roadmap found at {}", path.display());
        println!("Run `jeru roadmap edit` to create one.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let sections = parse(&content);

    if sections.is_empty() {
        println!("Roadmap has no sections with checkbox items.");
        return Ok(());
    }

    for s in &sections {
        let bar = if s.total == 0 {
            String::new()
        } else {
            format!("  {}/{}", s.done, s.total)
        };
        println!("\n{}{bar}", s.heading);
        for item in &s.open {
            println!("  ○ {item}");
        }
        if s.open.is_empty() && s.total > 0 {
            println!("  ✓ all done");
        }
    }
    println!();
    Ok(())
}

fn parse(content: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();

    for line in content.lines() {
        if line.starts_with('#') {
            let heading = line.trim_start_matches('#').trim().to_string();
            sections.push(Section { heading, done: 0, total: 0, open: Vec::new() });
            continue;
        }

        let Some(current) = sections.last_mut() else { continue };

        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("- [x]").or_else(|| trimmed.strip_prefix("- [X]")) {
            current.done += 1;
            current.total += 1;
            let _ = rest; // done items are not listed
        } else if let Some(rest) = trimmed.strip_prefix("- [ ]") {
            current.total += 1;
            current.open.push(rest.trim().to_string());
        }
    }

    // Drop sections that have no checkbox items at all
    sections.retain(|s| s.total > 0);
    sections
}

// ── edit ──────────────────────────────────────────────────────────────────────

const STARTER: &str = "\
## Backlog
- [ ]

## In progress
- [ ]

## Done
";

/// Open the roadmap in `$EDITOR`, creating a starter file if it does not exist.
pub fn edit(name: &str) -> Result<()> {
    let path = effective_path(name)?;

    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, STARTER)?;
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    Command::new(&editor).arg(&path).status()?;
    Ok(())
}

// ── link / unlink ─────────────────────────────────────────────────────────────

/// Store a custom roadmap path in the project manifest.
pub fn link(name: &str, path: &str) -> Result<()> {
    let mut manifest = load_manifest(name)?;
    manifest.roadmap = Some(path.to_string());
    manifest.save_to_dir(&project_dir(name)?)
}

/// Remove the custom roadmap path from the project manifest, reverting to the
/// default `ROADMAP.md` location.
pub fn unlink(name: &str) -> Result<()> {
    let mut manifest = load_manifest(name)?;
    if manifest.roadmap.is_none() {
        return Err(Error::AlreadyExists(
            "no custom roadmap path is set".to_string(),
        ));
    }
    manifest.roadmap = None;
    manifest.save_to_dir(&project_dir(name)?)
}
