use std::path::PathBuf;
use std::process::Command;

use crate::constants::ROADMAP_FILE;
use crate::error::Result;
use crate::project::project_dir;

// ── path resolution ───────────────────────────────────────────────────────────

/// Return the roadmap path for a project (`<project-dir>/ROADMAP.md`).
pub fn effective_path(name: &str) -> Result<PathBuf> {
    Ok(project_dir(name)?.join(ROADMAP_FILE))
}

/// Return the roadmap path to embed in `CLAUDE.md`, or `None` if the file does
/// not exist.
pub fn claude_md_path(name: &str) -> Result<Option<PathBuf>> {
    let path = project_dir(name)?.join(ROADMAP_FILE);
    if path.exists() {
        Ok(Some(path))
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
            sections.push(Section {
                heading,
                done: 0,
                total: 0,
                open: Vec::new(),
            });
            continue;
        }

        let Some(current) = sections.last_mut() else {
            continue;
        };

        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("- [x]")
            .or_else(|| trimmed.strip_prefix("- [X]"))
        {
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
