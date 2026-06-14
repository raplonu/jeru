use std::path::PathBuf;

use figment::Figment;
use figment::providers::{Env, Serialized};
use serde::{Deserialize, Serialize};

use crate::constants::{
    CACHE_DIR_NAME, ENV_PREFIX, KNOWLEDGE_DIR, OBSIDIAN_API_KEY_ENV, OBSIDIAN_MCP_URL, PROJECTS_DIR,
};
use crate::error::{Error, Result};

/// Runtime configuration for jeru, resolved once per call.
///
/// Defaults come from standard OS directories; every field can be overridden
/// by a `JERU_<FIELD>` environment variable (e.g. `JERU_PROJECTS_DIR`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub projects_dir: PathBuf,
    pub knowledge_dir: PathBuf,
    pub cache_dir: PathBuf,
    /// Whether to generate `.mcp.json` wiring Claude Code to the Obsidian MCP server.
    pub obsidian_mcp_enabled: bool,
    /// URL of the Obsidian Local REST API MCP endpoint.
    pub obsidian_mcp_url: String,
    /// Name of the environment variable the generated `.mcp.json` references for
    /// the Obsidian API token (kept out of the file so it is never synced).
    pub obsidian_api_key_env: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let defaults = Self::defaults()?;
        Ok(Figment::new()
            .merge(Serialized::defaults(&defaults))
            .merge(Env::prefixed(ENV_PREFIX))
            .extract()?)
    }

    fn defaults() -> Result<Self> {
        let home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
        let cache = dirs::cache_dir().ok_or(Error::NoCacheDir)?;
        Ok(Self {
            projects_dir: home.join(PROJECTS_DIR),
            knowledge_dir: home.join(KNOWLEDGE_DIR),
            cache_dir: cache.join(CACHE_DIR_NAME),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: OBSIDIAN_MCP_URL.to_string(),
            obsidian_api_key_env: OBSIDIAN_API_KEY_ENV.to_string(),
        })
    }
}
