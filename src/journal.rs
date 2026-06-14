use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::error::Result;

const DATA_JSON: &str = ".obsidian/plugins/journals/data.json";

/// Information about a project's journal, read from data.json.
pub struct JournalInfo {
    /// Absolute path to the journal folder on disk.
    pub path: PathBuf,
    /// Vault-relative folder for the journal, e.g. "project/jeru/journal".
    pub folder: String,
    /// Obsidian journals write type: "day", "week", "month", etc.
    pub write_type: String,
    /// Filename date format, e.g. "YYYY-MM-DD".
    pub date_format: String,
}

/// Ensure the project knowledge folder and its journal subdirectory exist, and
/// that the group is registered in Obsidian's journals plugin `data.json`.
///
/// If the journal entry is absent it is inserted with defaults (`day` /
/// `YYYY-MM-DD`). The existing entry is never modified. Returns the journal
/// configuration so the CLAUDE.md template can reflect the actual format.
pub fn ensure_journal(config: &Config, knowledge_location: &str) -> Result<JournalInfo> {
    let knowledge_dir = config.knowledge_dir.join("project").join(knowledge_location);
    let journal_dir = knowledge_dir.join("journal");
    std::fs::create_dir_all(&journal_dir)?;

    // Default vault-relative folder; overridden below by the configured value if present.
    let default_folder = format!("project/{knowledge_location}/journal");

    let data_json_path = config.knowledge_dir.join(DATA_JSON);
    if !data_json_path.exists() {
        return Ok(JournalInfo {
            path: journal_dir,
            folder: default_folder,
            write_type: "day".to_string(),
            date_format: "YYYY-MM-DD".to_string(),
        });
    }

    let mut root: Map<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(&data_json_path)?)?;

    if !root.contains_key("journals") {
        root.insert("journals".to_string(), Value::Object(Map::new()));
    }

    let absent = root["journals"]
        .as_object()
        .map(|j| !j.contains_key(knowledge_location))
        .unwrap_or(false);

    if absent {
        if let Some(journals) = root["journals"].as_object_mut() {
            journals.insert(
                knowledge_location.to_string(),
                new_journal_entry(knowledge_location, &default_folder),
            );
        }
        let mut content = serde_json::to_string_pretty(&Value::Object(root.clone()))?;
        content.push('\n');
        std::fs::write(&data_json_path, content)?;
    }

    let (folder, write_type, date_format) = root["journals"]
        .get(knowledge_location)
        .map(|e| {
            let folder = e
                .get("folder")
                .and_then(|f| f.as_str())
                .unwrap_or(&default_folder)
                .to_string();
            let wt = e
                .get("write")
                .and_then(|w| w.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("day")
                .to_string();
            let df = e
                .get("dateFormat")
                .and_then(|f| f.as_str())
                .unwrap_or("YYYY-MM-DD")
                .to_string();
            (folder, wt, df)
        })
        .unwrap_or_else(|| (default_folder.clone(), "day".to_string(), "YYYY-MM-DD".to_string()));

    Ok(JournalInfo {
        path: journal_dir,
        folder,
        write_type,
        date_format,
    })
}

fn new_journal_entry(name: &str, folder: &str) -> Value {
    json!({
        "name": name,
        "shelves": [],
        "write": { "type": "day" },
        "confirmCreation": false,
        "nameTemplate": "{{date}}",
        "dateFormat": "YYYY-MM-DD",
        "folder": folder,
        "templates": [],
        "start": "",
        "end": { "type": "never" },
        "index": {
            "enabled": false,
            "anchorDate": "",
            "anchorIndex": 1,
            "allowBefore": false,
            "type": "increment",
            "resetAfter": 2
        },
        "autoCreate": false,
        "commands": [],
        "decorations": [
            {
                "mode": "and",
                "conditions": [{ "type": "has-note" }],
                "styles": [{
                    "type": "shape",
                    "size": 0.4,
                    "shape": "circle",
                    "color": { "type": "theme", "name": "interactive-accent" },
                    "placement_x": "center",
                    "placement_y": "bottom"
                }]
            }
        ],
        "navBlock": {
            "type": "create",
            "decorateWholeBlock": false,
            "rows": [
                {
                    "template": "{{date:ddd}}",
                    "fontSize": 1, "bold": false, "italic": false, "link": "none",
                    "journal": "",
                    "color": { "type": "theme", "name": "text-normal" },
                    "background": { "type": "transparent" },
                    "addDecorations": false
                },
                {
                    "template": "{{date:D}}",
                    "fontSize": 3, "bold": true, "italic": false, "link": "self",
                    "journal": "",
                    "color": { "type": "theme", "name": "text-normal" },
                    "background": { "type": "transparent" },
                    "addDecorations": true
                },
                {
                    "template": "{{relative_date}}",
                    "fontSize": 0.7, "bold": false, "italic": false, "link": "none",
                    "journal": "",
                    "color": { "type": "theme", "name": "text-normal" },
                    "background": { "type": "transparent" },
                    "addDecorations": false
                },
                {
                    "template": "{{date:[W]w}}",
                    "fontSize": 1, "bold": false, "italic": false, "link": "week",
                    "journal": "",
                    "color": { "type": "theme", "name": "text-normal" },
                    "background": { "type": "transparent" },
                    "addDecorations": false
                },
                {
                    "template": "{{date:MMMM}}",
                    "fontSize": 1, "bold": false, "italic": false, "link": "month",
                    "journal": "",
                    "color": { "type": "theme", "name": "text-normal" },
                    "background": { "type": "transparent" },
                    "addDecorations": false
                },
                {
                    "template": "{{date:YYYY}}",
                    "fontSize": 1, "bold": false, "italic": false, "link": "year",
                    "journal": "",
                    "color": { "type": "theme", "name": "text-normal" },
                    "background": { "type": "transparent" },
                    "addDecorations": false
                }
            ]
        },
        "calendarViewBlock": { "rows": [], "decorateWholeBlock": false },
        "frontmatter": {
            "dateField": "",
            "addStartDate": false,
            "startDateField": "",
            "addEndDate": false,
            "endDateField": "",
            "indexField": ""
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_config(dir: &Path) -> Config {
        Config {
            projects_dir: dir.to_path_buf(),
            knowledge_dir: dir.to_path_buf(),
            cache_dir: dir.to_path_buf(),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: "http://127.0.0.1:27123/mcp/".to_string(),
            obsidian_api_key_env: "OBSIDIAN_API_KEY".to_string(),
            obsidian_autostart: false,
            obsidian_launch_cmd: "false".to_string(),
        }
    }

    /// Create `.obsidian/plugins/journals/data.json` with the given content.
    fn write_data_json(dir: &Path, content: &str) {
        let plugin = dir.join(".obsidian/plugins/journals");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("data.json"), content).unwrap();
    }

    fn read_data_json(dir: &Path) -> Value {
        let path = dir.join(".obsidian/plugins/journals/data.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn defaults_when_no_data_json() {
        let dir = tempfile::tempdir().unwrap();
        let info = ensure_journal(&test_config(dir.path()), "proj").unwrap();
        assert_eq!(info.folder, "project/proj/journal");
        assert_eq!(info.write_type, "day");
        assert_eq!(info.date_format, "YYYY-MM-DD");
        assert!(dir.path().join("project/proj/journal").is_dir());
    }

    #[test]
    fn inserts_missing_entry_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        write_data_json(dir.path(), r#"{"journals":{}}"#);
        let info = ensure_journal(&test_config(dir.path()), "proj").unwrap();
        assert_eq!(info.folder, "project/proj/journal");
        let written = read_data_json(dir.path());
        assert_eq!(written["journals"]["proj"]["folder"], "project/proj/journal");
        assert_eq!(written["journals"]["proj"]["write"]["type"], "day");
    }

    #[test]
    fn reads_custom_folder_and_format() {
        let dir = tempfile::tempdir().unwrap();
        write_data_json(
            dir.path(),
            r#"{"journals":{"proj":{"folder":"custom/notes","dateFormat":"YYYY/MM/DD","write":{"type":"week"}}}}"#,
        );
        let info = ensure_journal(&test_config(dir.path()), "proj").unwrap();
        assert_eq!(info.folder, "custom/notes");
        assert_eq!(info.date_format, "YYYY/MM/DD");
        assert_eq!(info.write_type, "week");
    }

    #[test]
    fn preserves_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        write_data_json(
            dir.path(),
            r#"{"journals":{"proj":{"folder":"custom/notes","dateFormat":"YYYY-MM-DD","write":{"type":"day"},"marker":"keep-me"}}}"#,
        );
        ensure_journal(&test_config(dir.path()), "proj").unwrap();
        let written = read_data_json(dir.path());
        assert_eq!(written["journals"]["proj"]["marker"], "keep-me");
        assert_eq!(written["journals"]["proj"]["folder"], "custom/notes");
    }
}
