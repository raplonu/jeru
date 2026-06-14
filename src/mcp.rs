use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::constants::{MCP_FILE, MCP_SERVERS_KEY, OBSIDIAN_SERVER_NAME};
use crate::error::{Error, Result};
use crate::project::project_dir;

/// The `obsidian` MCP server entry: a streamable-HTTP server pointing at the
/// Obsidian Local REST API MCP endpoint, authenticated via an environment
/// variable reference so the token never lands in the (Syncthing-synced) file.
fn obsidian_server(config: &Config) -> Value {
    json!({
        "type": "http",
        "url": config.obsidian_mcp_url,
        "headers": {
            "Authorization": format!("Bearer ${{{}}}", config.obsidian_api_key_env),
        },
    })
}

/// Write (or update) `.mcp.json` inside `dir`, upserting the `obsidian` server
/// under `mcpServers` while preserving any other servers already configured.
///
/// Returns `Ok(None)` when Obsidian MCP integration is disabled; otherwise the
/// path written.
pub fn write_mcp_json_for_dir(dir: &Path, config: &Config) -> Result<Option<PathBuf>> {
    if !config.obsidian_mcp_enabled {
        return Ok(None);
    }
    let path = dir.join(MCP_FILE);

    let mut root = if path.exists() {
        match serde_json::from_str(&std::fs::read_to_string(&path)?)? {
            Value::Object(map) => map,
            _ => return Err(Error::InvalidMcpConfig(path.to_string_lossy().into_owned())),
        }
    } else {
        Map::new()
    };

    let servers = root
        .entry(MCP_SERVERS_KEY)
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers.is_object() {
        return Err(Error::InvalidMcpConfig(path.to_string_lossy().into_owned()));
    }
    servers[OBSIDIAN_SERVER_NAME] = obsidian_server(config);

    let mut content = serde_json::to_string_pretty(&Value::Object(root))?;
    content.push('\n');
    std::fs::write(&path, content)?;
    Ok(Some(path))
}

/// Generate (or update) `.mcp.json` for a named project in its project dir.
pub fn write_mcp_json(config: &Config, name: &str) -> Result<Option<PathBuf>> {
    write_mcp_json_for_dir(&project_dir(config, name), config)
}

/// Path to the Obsidian Local REST API plugin config within the vault.
const REST_API_DATA_JSON: &str = ".obsidian/plugins/obsidian-local-rest-api/data.json";

/// Read the Obsidian Local REST API token from the plugin's `data.json`, if
/// available. Used only to surface a setup hint; the token is never written to
/// generated files.
pub fn read_obsidian_api_key(config: &Config) -> Option<String> {
    let path = config.knowledge_dir.join(REST_API_DATA_JSON);
    let value: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    value.get("apiKey")?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(dir: &Path) -> Config {
        Config {
            projects_dir: dir.to_path_buf(),
            knowledge_dir: dir.to_path_buf(),
            cache_dir: dir.to_path_buf(),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: "http://127.0.0.1:27123/mcp/".to_string(),
            obsidian_api_key_env: "OBSIDIAN_API_KEY".to_string(),
        }
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn writes_obsidian_server_with_env_var_reference() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());

        let path = write_mcp_json_for_dir(dir.path(), &config)
            .unwrap()
            .expect("path written");
        let json = read_json(&path);

        let server = &json["mcpServers"]["obsidian"];
        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "http://127.0.0.1:27123/mcp/");
        assert_eq!(
            server["headers"]["Authorization"],
            "Bearer ${OBSIDIAN_API_KEY}"
        );
        // The literal token must never be written to the file.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("Bearer ey"), "token must not be inlined");
    }

    #[test]
    fn preserves_other_servers_on_update() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let path = dir.path().join(MCP_FILE);

        std::fs::write(
            &path,
            r#"{"mcpServers":{"other":{"type":"stdio","command":"foo"}}}"#,
        )
        .unwrap();

        write_mcp_json_for_dir(dir.path(), &config).unwrap();
        let json = read_json(&path);

        assert_eq!(json["mcpServers"]["other"]["command"], "foo");
        assert_eq!(json["mcpServers"]["obsidian"]["type"], "http");
    }

    #[test]
    fn disabled_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = test_config(dir.path());
        config.obsidian_mcp_enabled = false;

        let result = write_mcp_json_for_dir(dir.path(), &config).unwrap();
        assert!(result.is_none());
        assert!(!dir.path().join(MCP_FILE).exists());
    }

    #[test]
    fn honors_custom_url_and_env_var() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = test_config(dir.path());
        config.obsidian_mcp_url = "http://localhost:9999/mcp/".to_string();
        config.obsidian_api_key_env = "MY_TOKEN".to_string();

        let path = write_mcp_json_for_dir(dir.path(), &config)
            .unwrap()
            .unwrap();
        let json = read_json(&path);

        assert_eq!(json["mcpServers"]["obsidian"]["url"], "http://localhost:9999/mcp/");
        assert_eq!(
            json["mcpServers"]["obsidian"]["headers"]["Authorization"],
            "Bearer ${MY_TOKEN}"
        );
    }
}
