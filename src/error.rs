use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not determine home directory")]
    NoHomeDir,

    #[error("could not determine cache directory")]
    NoCacheDir,

    #[error("no project given and no current project set (use `jeru use <name>`)")]
    NoCurrentProject,

    #[error("project '{0}' not found")]
    ProjectNotFound(String),

    #[error("project '{0}' has no repos")]
    NoRepos(String),

    #[error("no manifest (project.yml or project.yaml) found in {0}")]
    ManifestNotFound(String),

    #[error("{0} already exists (refusing to overwrite)")]
    AlreadyExists(String),

    #[error("{0} not found in project manifest")]
    NotFound(String),

    #[error("directory '{0}' is not empty; use --force to create the project anyway")]
    DirectoryNotEmpty(String),

    #[error("unknown alias '{0}'; run `jeru edit --list-alias` to see valid aliases")]
    UnknownAlias(String),

    #[error("{0}: existing settings.json is not a JSON object")]
    InvalidSettings(String),

    #[error("{0}: existing .mcp.json is not a JSON object")]
    InvalidMcpConfig(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("failed to parse manifest: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

    #[error("failed to handle settings JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("failed to render template: {0}")]
    Template(#[from] minijinja::Error),

    #[error("invalid configuration: {0}")]
    Config(Box<figment::Error>),

    #[error("SSH command failed for '{0}'")]
    RemoteSsh(String),

    #[error("mutagen error: {0}")]
    Mutagen(String),

    #[error("tmux error: {0}")]
    Tmux(String),

    #[error("no session '{0}' found (run `jeru session ls`)")]
    SessionNotFound(String),

    #[error("ambiguous session '{0}'; matches: {1} (pass the full id)")]
    SessionAmbiguous(String, String),

    #[error("path '{0}' is not under the home directory and cannot be mapped to a remote path")]
    PathNotUnderHome(String),
}

impl From<figment::Error> for Error {
    fn from(e: figment::Error) -> Self {
        Error::Config(Box::new(e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
