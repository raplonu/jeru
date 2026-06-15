// Project tree layout
pub const PROJECTS_DIR: &str = "project";
pub const KNOWLEDGE_DIR: &str = "knowledge";

// Project files
pub const CLAUDE_MD_FILE: &str = "CLAUDE.md";
pub const CLAUDE_DIR: &str = ".claude";
pub const SETTINGS_FILE: &str = "settings.json";

// settings.json keys
pub const ADDITIONAL_DIRS_KEY: &str = "additionalDirectories";

// .mcp.json (project-scoped MCP server config consumed by Claude Code)
pub const MCP_FILE: &str = ".mcp.json";
pub const MCP_SERVERS_KEY: &str = "mcpServers";
pub const OBSIDIAN_SERVER_NAME: &str = "obsidian";

// Obsidian MCP integration defaults (overridable via JERU_OBSIDIAN_* env vars)
pub const OBSIDIAN_MCP_URL: &str = "http://127.0.0.1:27123/mcp/";
pub const OBSIDIAN_API_KEY_ENV: &str = "OBSIDIAN_API_KEY";

// Command used to launch Obsidian (normally, with its GUI) when it is not
// already running, executed via `sh -c`. jeru spawns this fire-and-forget and
// never stops it — Obsidian is the user's vault editor and stays up across
// sessions so its MCP server remains reachable.
pub const OBSIDIAN_LAUNCH_CMD: &str = "obsidian";

// VSCode workspace file extension (includes leading dot)
pub const WORKSPACE_EXT: &str = ".code-workspace";

// jeru cache
pub const CACHE_DIR_NAME: &str = "jeru";
pub const CURRENT_PROJECT_FILE: &str = "current_project";

// Subdirectory of the cache holding one JSON state file per active session.
pub const SESSIONS_DIR: &str = "sessions";

// Default manifest filename inside a project directory
pub const MANIFEST_FILE: &str = "project.yml";

// Default roadmap filename inside a project directory
pub const ROADMAP_FILE: &str = "ROADMAP.md";

// Default readme filename inside a project directory
pub const README_FILE: &str = "README.md";

// External binaries
pub const CLAUDE_BIN: &str = "claude";
pub const CODE_BIN: &str = "code";

// Prefix for JERU_* env var overrides (e.g. JERU_PROJECTS_DIR)
pub const ENV_PREFIX: &str = "JERU_";
