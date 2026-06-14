use std::path::Path;

use minijinja::Environment;
use serde::Serialize;

use crate::error::Result;
use crate::journal::JournalInfo;
use crate::manifest::Manifest;

/// Default project `CLAUDE.md` template, embedded in the binary.
const CLAUDE_MD_TEMPLATE: &str = include_str!("../templates/CLAUDE.md.j2");

/// Context passed to the CLAUDE.md template.
#[derive(Serialize)]
struct ClaudeContext<'a> {
    #[serde(flatten)]
    manifest: &'a Manifest,
    roadmap_path: Option<String>,
    readme_path: Option<String>,
    journal_path: String,
    journal_write_type: String,
    journal_date_format: String,
}

/// Render the project `CLAUDE.md` from the manifest.
pub fn render_claude_md(
    manifest: &Manifest,
    roadmap: Option<&Path>,
    readme: Option<&Path>,
    journal: &JournalInfo,
) -> Result<String> {
    let roadmap_path = roadmap.map(|p| p.to_string_lossy().into_owned());
    let readme_path = readme.map(|p| p.to_string_lossy().into_owned());

    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.add_template("claude_md", CLAUDE_MD_TEMPLATE)?;
    let template = env.get_template("claude_md")?;
    Ok(template.render(ClaudeContext {
        manifest,
        roadmap_path,
        readme_path,
        journal_path: journal.path.to_string_lossy().into_owned(),
        journal_write_type: journal.write_type.clone(),
        journal_date_format: journal.date_format.clone(),
    })?)
}
