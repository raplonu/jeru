use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::error::Result;

const DATA_JSON: &str = ".obsidian/plugins/journals/data.json";

/// Information about a project's journal, read from data.json.
pub struct JournalInfo {
    /// Absolute path to the journal folder on disk.
    pub path: PathBuf,
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

    let data_json_path = config.knowledge_dir.join(DATA_JSON);
    if !data_json_path.exists() {
        return Ok(JournalInfo {
            path: journal_dir,
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
        let folder = format!("project/{knowledge_location}/journal");
        if let Some(journals) = root["journals"].as_object_mut() {
            journals.insert(
                knowledge_location.to_string(),
                new_journal_entry(knowledge_location, &folder),
            );
        }
        let mut content = serde_json::to_string_pretty(&Value::Object(root.clone()))?;
        content.push('\n');
        std::fs::write(&data_json_path, content)?;
    }

    let (write_type, date_format) = root["journals"]
        .get(knowledge_location)
        .map(|e| {
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
            (wt, df)
        })
        .unwrap_or_else(|| ("day".to_string(), "YYYY-MM-DD".to_string()));

    Ok(JournalInfo {
        path: journal_dir,
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
