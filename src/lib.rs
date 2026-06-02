pub mod add;
pub mod cache;
pub mod config;
pub mod constants;
pub mod remote;
pub mod roadmap;
pub mod error;
pub mod launch;
pub mod manifest;
pub mod project;
pub mod settings;
pub mod template;
pub mod vscode;

pub use add::{Kind, add_to_project, detect_kind};
pub use cache::{current_project, resolve_project, set_current_project};
pub use config::Config;
pub use error::{Error, Result};
pub use launch::{claude_for_project, claude_for_repos};
pub use manifest::Manifest;
pub use project::{
    Project, expand_tilde, init_claude_md, knowledge_dir, list_projects, load_manifest,
    project_dir, projects_dir, use_project,
};
pub use settings::{additional_directories, write_settings};
pub use vscode::{code_command, workspace_path, write_workspace};
