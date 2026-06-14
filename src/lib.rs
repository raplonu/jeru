pub mod add;
pub mod cache;
pub mod config;
pub mod constants;
pub mod error;
pub mod journal;
pub mod launch;
pub mod manifest;
pub mod project;
pub mod remote;
pub mod settings;
pub mod template;
pub mod validate;
pub mod vscode;

pub use add::{Kind, add_to_project, detect_kind, list_entries, remove_from_project};
pub use cache::{current_project, resolve_project, set_current_project};
pub use config::Config;
pub use error::{Error, Result};
pub use launch::{claude_for_project, claude_for_repos};
pub use manifest::Manifest;
pub use journal::JournalInfo;
pub use project::{
    Project, create_project, expand_tilde, init_claude_md, knowledge_dir,
    list_projects, load_manifest, project_dir, projects_dir, to_absolute_path, use_project,
};
pub use settings::{additional_directories, write_settings, write_settings_for_dir};
pub use validate::{Issue as ValidationIssue, IssueKind as ValidationIssueKind, validate_project};
pub use vscode::{code_command, code_folder, workspace_path, write_workspace};
