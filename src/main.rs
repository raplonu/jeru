use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use dialoguer::{Select, theme::ColorfulTheme};

use jeru::{Config, Kind, Manifest};

#[derive(Parser)]
#[command(name = "jeru", about = "Personal project tree manager", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage projects (list, select, create, compile, edit, entries…)
    #[command(visible_alias = "p")]
    Project {
        #[command(subcommand)]
        action: ProjectCommand,
    },
    /// Manage background work sessions (detached tmux running claude remote-control)
    #[command(visible_alias = "s")]
    Session {
        #[command(subcommand)]
        action: SessionCommand,
    },
    /// Claude Code integration
    Claude {
        #[command(subcommand)]
        action: ClaudeCommand,
    },
    /// Print a shell completion script to stdout
    Completions {
        /// Target shell
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// List projects under the project tree
    Ls,
    /// Set the current project for subsequent commands
    #[command(visible_alias = "u")]
    Use {
        /// Project name (directory under the project tree)
        name: String,
    },
    /// Show a project's manifest (optionally only one kind of entry)
    #[command(visible_alias = "i")]
    Info {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// Show only entries of this kind (repos, knowledge sets, or resources)
        #[arg(short, long, value_enum)]
        kind: Option<KindArg>,
    },
    /// Create a new project
    #[command(visible_alias = "new")]
    Create {
        /// Project name (new directory under the project tree)
        name: String,
        /// Subfolder under knowledge/project/ (defaults to project name)
        #[arg(long)]
        knowledge_location: Option<String>,
        /// Set this project as the current one after creating it
        #[arg(long)]
        active: bool,
        /// Create the project even if the directory already exists and is non-empty
        #[arg(long)]
        force: bool,
    },
    /// Generate CLAUDE.md, .claude/settings.json, and the VSCode workspace
    #[command(visible_alias = "c")]
    Compile {
        /// Project name; defaults to the current project
        name: Option<String>,
    },
    /// Validate project manifests for common issues
    #[command(visible_alias = "check")]
    Validate {
        /// Project name to validate; defaults to the current project
        name: Option<String>,
        /// Validate all projects
        #[arg(long)]
        all: bool,
    },
    /// Open a project file in $EDITOR, or the project folder in VSCode
    #[command(visible_alias = "e")]
    Edit {
        /// Project name; defaults to the current project
        project: Option<String>,
        /// File to open (relative to the project directory); omit to open VSCode
        #[arg(short = 'f', long)]
        filename: Option<String>,
        /// List accepted filename aliases
        #[arg(long)]
        list_alias: bool,
    },
    /// Add a repo, knowledge set, or resource to a project
    Add {
        /// Path to add
        path: String,
        /// Kind of entry; deduced from path if omitted (interactive confirmation)
        #[arg(short, long, value_enum)]
        kind: Option<KindArg>,
        /// Project name; defaults to the current project
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Remove a repo, knowledge set, or resource from a project
    #[command(visible_alias = "rm")]
    Remove {
        /// Path or knowledge set ID to remove
        path: String,
        /// Kind of entry; deduced from path if omitted (interactive confirmation)
        #[arg(short, long, value_enum)]
        kind: Option<KindArg>,
        /// Project name; defaults to the current project
        #[arg(short, long)]
        project: Option<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum KindArg {
    Repo,
    Knowledge,
    Resource,
}

impl From<KindArg> for Kind {
    fn from(k: KindArg) -> Self {
        match k {
            KindArg::Repo => Kind::Repo,
            KindArg::Knowledge => Kind::Knowledge,
            KindArg::Resource => Kind::Resource,
        }
    }
}

#[derive(Subcommand)]
enum SessionCommand {
    /// Bring up a background session for a project (locally or on a remote host)
    Up {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// SSH host to run the session on remotely (e.g. user@hostname)
        #[arg(long)]
        remote: Option<String>,
        /// claude remote-control spawn mode
        #[arg(long, value_enum, default_value_t = SpawnArg::SameDir)]
        spawn: SpawnArg,
        /// Work only on repos: claude opens in the first repo, only repos synced remotely
        #[arg(long)]
        repos: bool,
        /// Do not sync resources (remote only)
        #[arg(long, requires = "remote")]
        no_resources: bool,
        /// Skip removing remote directories when the session is stopped (remote only)
        #[arg(long, requires = "remote")]
        no_cleanup: bool,
        /// Skip comparing remote directory contents and wipe ALL non-empty
        /// remote sync directories without prompting (remote only). Without
        /// this flag, non-empty remote directories are compared against
        /// local and conflicts are resolved interactively.
        #[arg(long, requires = "remote")]
        override_remote: bool,
    },
    /// List active sessions
    Ls,
    /// Bring down a session and clean up
    Down {
        /// Session id (project or project@host); omit to pick from a list
        id: Option<String>,
    },
    /// Attach to a session's tmux to watch claude
    #[command(alias = "inspect")]
    Attach {
        /// Session id (project or project@host); defaults to the current project
        id: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum SpawnArg {
    SameDir,
    Worktree,
    Session,
}

impl SpawnArg {
    fn as_str(self) -> &'static str {
        match self {
            SpawnArg::SameDir => "same-dir",
            SpawnArg::Worktree => "worktree",
            SpawnArg::Session => "session",
        }
    }
}

#[derive(Subcommand)]
enum ClaudeCommand {
    /// Open Claude Code in the project directory, with all linked folders
    Project {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// Arguments after `--` are forwarded to claude
        #[arg(last = true)]
        extra: Vec<String>,
    },
    /// Open Claude Code in the project's first repo, with the rest added
    Repos {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// Arguments after `--` are forwarded to claude
        #[arg(last = true)]
        extra: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Completions doesn't need Config.
    if let Command::Completions { shell } = cli.command {
        generate(shell, &mut Cli::command(), "jeru", &mut std::io::stdout());
        return;
    }

    let result = run(cli);
    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> jeru::Result<()> {
    let config = Config::load()?;

    match cli.command {
        Command::Project { action } => run_project(&config, action),
        Command::Session { action } => run_session(&config, action),
        Command::Claude { action } => match action {
            ClaudeCommand::Project { name, extra } => {
                run_claude_open(&config, name, extra, Target::Project)
            }
            ClaudeCommand::Repos { name, extra } => {
                run_claude_open(&config, name, extra, Target::Repos)
            }
        },
        Command::Completions { .. } => unreachable!("handled before Config::load"),
    }
}

fn run_project(config: &Config, action: ProjectCommand) -> jeru::Result<()> {
    match action {
        ProjectCommand::Ls => run_ls(config),
        ProjectCommand::Use { name } => run_use(config, &name),
        ProjectCommand::Info { name, kind } => run_info(config, name, kind),
        ProjectCommand::Compile { name } => run_compile(config, name),
        ProjectCommand::Edit {
            project,
            filename,
            list_alias,
        } => run_edit(config, filename, project, list_alias),
        ProjectCommand::Add {
            path,
            kind,
            project,
        } => run_add(config, project, path, kind),
        ProjectCommand::Remove {
            path,
            kind,
            project,
        } => run_remove(config, project, path, kind),
        ProjectCommand::Validate { name, all } => run_validate(config, name, all),
        ProjectCommand::Create {
            name,
            knowledge_location,
            active,
            force,
        } => run_create(config, &name, knowledge_location, active, force),
    }
}

fn run_ls(config: &Config) -> jeru::Result<()> {
    let projects = jeru::list_projects(config)?;
    if projects.is_empty() {
        println!("No projects found.");
    } else {
        for project in projects {
            println!("{}", project.name);
        }
    }
    Ok(())
}

fn run_use(config: &Config, name: &str) -> jeru::Result<()> {
    jeru::use_project(config, name)?;
    println!("Current project: {name}");
    Ok(())
}

fn run_session(config: &Config, action: SessionCommand) -> jeru::Result<()> {
    use jeru::session;

    match action {
        SessionCommand::Up {
            name,
            remote,
            spawn,
            repos,
            no_resources,
            no_cleanup,
            override_remote,
        } => {
            let project = jeru::resolve_project(config, name)?;
            let opts = jeru::SessionStartOptions {
                spawn: spawn.as_str().to_string(),
                repos,
                no_resources,
                no_cleanup,
                override_remote,
            };
            session::start(config, &project, remote.as_deref(), &opts)
        }
        SessionCommand::Ls => session::list(config),
        SessionCommand::Down { id } => match id {
            Some(id) => session::stop(config, &id),
            None => match select_session_id(config)? {
                Some(id) => session::stop(config, &id),
                None => {
                    println!("No active sessions.");
                    Ok(())
                }
            },
        },
        SessionCommand::Attach { id } => {
            let id = resolve_session_id(config, id)?;
            session::inspect(config, &id)
        }
    }
}

/// A session id given explicitly, or the current project's name as a fallback
/// (matched against active sessions by `SessionState::find`).
fn resolve_session_id(config: &Config, id: Option<String>) -> jeru::Result<String> {
    match id {
        Some(id) => Ok(id),
        None => jeru::resolve_project(config, None),
    }
}

/// Prompt the user to pick an active session, returning its id (or `None` if
/// there are no active sessions).
fn select_session_id(config: &Config) -> jeru::Result<Option<String>> {
    let sessions = jeru::SessionState::list(config)?;
    if sessions.is_empty() {
        return Ok(None);
    }

    let labels: Vec<String> = sessions
        .iter()
        .map(|s| {
            let scope = match &s.remote {
                Some(host) => format!("remote {host}"),
                None => "local".to_string(),
            };
            format!("{}  [{scope}]", s.id)
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Session to bring down")
        .items(&labels)
        .default(0)
        .interact()
        .map_err(std::io::Error::other)?;

    Ok(Some(sessions[selection].id.clone()))
}

fn run_info(config: &Config, name: Option<String>, kind: Option<KindArg>) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, name)?;
    let manifest = jeru::load_manifest(config, &name)?;
    match kind.map(Kind::from) {
        None => print_manifest(&manifest),
        Some(k) => {
            let (title, items) = match k {
                Kind::Repo => ("repos", &manifest.repos),
                Kind::Knowledge => ("knowledge sets", &manifest.knowledge_sets),
                Kind::Resource => ("resources", &manifest.resources),
            };
            print_section(title, items);
            println!();
        }
    }
    Ok(())
}

fn run_compile(config: &Config, name: Option<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, name)?;

    let claude_path = jeru::init_claude_md(config, &name, true)?;
    println!("Wrote {}", claude_path.display());

    let settings_path = jeru::write_settings(config, &name)?;
    println!("Wrote {}", settings_path.display());

    if let Some(mcp_path) = jeru::write_mcp_json(config, &name)? {
        println!("Wrote {}", mcp_path.display());
        // The token is referenced via an env var, not written to the file. If it
        // is not set, surface the value from the Obsidian plugin so the user can
        // export it before launching Claude.
        if std::env::var(&config.obsidian_api_key_env).is_err() {
            match jeru::read_obsidian_api_key(config) {
                Some(key) => println!(
                    "  note: set the Obsidian token before `jeru session start`:\n    export {}={key}",
                    config.obsidian_api_key_env
                ),
                None => println!(
                    "  note: set ${} to your Obsidian Local REST API token before `jeru session start`",
                    config.obsidian_api_key_env
                ),
            }
        }
    }

    match jeru::write_workspace(config, &name) {
        Ok(ws) => println!("Wrote {}", ws.display()),
        Err(jeru::Error::NoRepos(_)) => {}
        Err(e) => return Err(e),
    }

    Ok(())
}

enum Target {
    Project,
    Repos,
}

fn run_claude_open(
    config: &Config,
    name: Option<String>,
    extra: Vec<String>,
    target: Target,
) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, name)?;
    let mut command = match target {
        Target::Project => jeru::claude_for_project(config, &name, &extra)?,
        Target::Repos => jeru::claude_for_repos(config, &name, &extra)?,
    };
    let status = command.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_add(
    config: &Config,
    project: Option<String>,
    path: String,
    kind: Option<KindArg>,
) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, project)?;

    let kind: Kind = match kind {
        Some(k) => k.into(),
        None => {
            let detected = jeru::detect_kind(config, &path)?;
            confirm_kind(&path, detected)?
        }
    };

    jeru::add_to_project(config, &name, &path, kind)?;
    println!("Added {} '{}' to project {name}", kind.label(), path);
    Ok(())
}

fn run_remove(
    config: &Config,
    project: Option<String>,
    path: String,
    kind: Option<KindArg>,
) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, project)?;

    let kind: Kind = match kind {
        Some(k) => k.into(),
        None => {
            let detected = jeru::detect_kind(config, &path)?;
            confirm_kind(&path, detected)?
        }
    };

    jeru::remove_from_project(config, &name, &path, kind)?;
    println!("Removed {} '{}' from project {name}", kind.label(), path);
    Ok(())
}

const EDIT_ALIASES: &[(&str, &str)] = &[
    ("@project", jeru::constants::MANIFEST_FILE),
    ("@roadmap", jeru::constants::ROADMAP_FILE),
    ("@readme", jeru::constants::README_FILE),
];

fn run_edit(
    config: &Config,
    filename: Option<String>,
    project: Option<String>,
    list_alias: bool,
) -> jeru::Result<()> {
    if list_alias {
        for (alias, target) in EDIT_ALIASES {
            println!("{alias:<12}{target}");
        }
        return Ok(());
    }
    let name = jeru::resolve_project(config, project)?;
    match filename {
        None => {
            let dir = jeru::project_dir(config, &name);
            let status = jeru::code_folder(&dir).status()?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
            Ok(())
        }
        Some(file) => {
            use std::process::Command;
            let dir = jeru::project_dir(config, &name);
            let path = resolve_edit_path(&dir, &file)?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = Command::new(&editor).arg(&path).status()?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
            Ok(())
        }
    }
}

fn run_create(
    config: &Config,
    name: &str,
    knowledge_location: Option<String>,
    active: bool,
    force: bool,
) -> jeru::Result<()> {
    let loc = knowledge_location.as_deref().unwrap_or(name);
    let dir = jeru::create_project(config, name, loc, force)?;
    println!("Created project '{name}' at {}", dir.display());
    if active {
        jeru::use_project(config, name)?;
        println!("Current project: {name}");
    }
    Ok(())
}

fn confirm_kind(path: &str, detected: Kind) -> jeru::Result<Kind> {
    const KINDS: [Kind; 3] = [Kind::Repo, Kind::Knowledge, Kind::Resource];
    const LABELS: [&str; 3] = ["repo", "knowledge", "resource"];

    let default = KINDS.iter().position(|k| *k == detected).unwrap_or(0);

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Kind for '{path}'"))
        .items(&LABELS)
        .default(default)
        .interact()
        .map_err(std::io::Error::other)?;

    Ok(KINDS[selection])
}

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

fn run_validate(config: &Config, name: Option<String>, all: bool) -> jeru::Result<()> {
    let names: Vec<String> = if all {
        jeru::list_projects(config)?.into_iter().map(|p| p.name).collect()
    } else {
        vec![jeru::resolve_project(config, name)?]
    };

    let mut any_issues = false;

    for name in &names {
        let issues = jeru::validate_project(config, name)?;
        if issues.is_empty() {
            println!("{BOLD}{name}{RESET}  {GREEN}ok{RESET}");
        } else {
            any_issues = true;
            let n = issues.len();
            let label = if n == 1 { "issue" } else { "issues" };
            println!("{BOLD}{name}{RESET}  {RED}{n} {label}{RESET}");
            for issue in &issues {
                println!("  [{DIM}{}{RESET}] {}", issue.kind.tag(), issue.message);
            }
        }
    }

    if any_issues {
        std::process::exit(1);
    }
    Ok(())
}

fn print_manifest(m: &Manifest) {
    println!("\n{BOLD}{}{RESET}", m.name);

    println!("  knowledge location: {}", m.knowledge_location);

    match &m.primary_repo {
        Some(repo) => println!("  primary repo: {repo}"),
        None => println!("  primary repo: {DIM}(none){RESET}"),
    }

    print_section("knowledge sets", &m.knowledge_sets);
    print_section("repos", &m.repos);
    print_section("resources", &m.resources);
    println!();
}

fn print_section(title: &str, items: &[String]) {
    println!("\n  {BOLD}{title}{RESET}");
    if items.is_empty() {
        println!("    {DIM}(none){RESET}");
    } else {
        for item in items {
            println!("    - {item}");
        }
    }
}

fn resolve_edit_path(dir: &std::path::Path, file: &str) -> jeru::Result<std::path::PathBuf> {
    if let Some((_, filename)) = EDIT_ALIASES.iter().find(|(alias, _)| *alias == file) {
        return Ok(dir.join(filename));
    }
    if file.starts_with('@') {
        return Err(jeru::Error::UnknownAlias(file.to_string()));
    }
    Ok(dir.join(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_aliases_resolve_to_their_filenames() {
        let dir = std::path::Path::new("/proj");
        assert_eq!(
            resolve_edit_path(dir, "@project").unwrap(),
            dir.join(jeru::constants::MANIFEST_FILE)
        );
        assert_eq!(
            resolve_edit_path(dir, "@roadmap").unwrap(),
            dir.join(jeru::constants::ROADMAP_FILE)
        );
        assert_eq!(
            resolve_edit_path(dir, "@readme").unwrap(),
            dir.join(jeru::constants::README_FILE)
        );
    }

    #[test]
    fn unknown_alias_returns_error() {
        let dir = std::path::Path::new("/proj");
        let err = resolve_edit_path(dir, "@unknown").unwrap_err();
        assert!(matches!(err, jeru::Error::UnknownAlias(s) if s == "@unknown"));
    }

    #[test]
    fn plain_filename_resolves_under_dir() {
        let dir = std::path::Path::new("/proj");
        assert_eq!(
            resolve_edit_path(dir, "notes.md").unwrap(),
            dir.join("notes.md")
        );
    }
}
