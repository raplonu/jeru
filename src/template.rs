use minijinja::Environment;

use crate::error::Result;
use crate::manifest::Manifest;

/// Default project `CLAUDE.md` template, embedded in the binary.
const CLAUDE_MD_TEMPLATE: &str = include_str!("../templates/CLAUDE.md.j2");

/// Render the project `CLAUDE.md` from the manifest.
pub fn render_claude_md(manifest: &Manifest) -> Result<String> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.add_template("claude_md", CLAUDE_MD_TEMPLATE)?;
    let template = env.get_template("claude_md")?;
    Ok(template.render(manifest)?)
}
