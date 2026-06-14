use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use dialoguer::{Select, theme::ColorfulTheme};

use jeru::{Config, Kind, Manifest};

#[derive(Parser)]
#[command(name = "jeru", about = "Personal project tree manager")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List projects under the project tree
    Ls,
    /// Set the current project for subsequent commands
    Use {
        /// Project name (directory under the project tree)
        name: String,
    },
    /// Open Claude Code and VSCode for a project
    Work {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// SSH host to work on remotely (e.g. user@hostname)
        #[arg(long)]
        remote: Option<String>,
        /// Work only on repos: Claude opens in the first repo, only repos are synced remotely
        #[arg(long)]
        repos: bool,
        /// Skip Claude (remote only)
        #[arg(long, requires = "remote")]
        no_claude: bool,
        /// Do not sync knowledge sets (remote only)
        #[arg(long, requires = "remote")]
        no_knowledge: bool,
        /// Do not sync resources (remote only)
        #[arg(long, requires = "remote")]
        no_resources: bool,
        /// Skip removing remote directories after the session ends (remote only)
        #[arg(long, requires = "remote")]
        no_cleanup: bool,
        /// Delete non-empty remote directories at startup instead of aborting (remote only)
        #[arg(long, requires = "remote")]
        override_remote: bool,
        /// Arguments after `--` are forwarded to claude
        #[arg(last = true)]
        extra: Vec<String>,
    },
    /// Show the manifest for a project
    Info {
        /// Project name; defaults to the current project
        name: Option<String>,
    },
    /// Claude Code integration
    Claude {
        #[command(subcommand)]
        action: ClaudeCommand,
    },
    /// Generate CLAUDE.md, .claude/settings.json, and the VSCode workspace
    Compile {
        /// Project name; defaults to the current project
        name: Option<String>,
    },
    /// Print a shell completion script to stdout
    Completions {
        /// Target shell
        shell: Shell,
    },
    /// Validate project manifests for common issues
    Validate {
        /// Project name to validate; defaults to the current project
        name: Option<String>,
        /// Validate all projects
        #[arg(long)]
        all: bool,
    },
    /// Create a new project
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
    /// Open a project file in $EDITOR, or the project folder in VSCode
    Edit {
        /// File to open (relative to the project directory); omit to open VSCode
        filename: Option<String>,
        /// Project name; defaults to the current project
        #[arg(short = 'p', long)]
        project: Option<String>,
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
    /// List repos, knowledge sets, and resources in a project
    List {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// Show only entries of this kind
        #[arg(short, long, value_enum)]
        kind: Option<KindArg>,
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
        Command::Ls => run_ls(&config),
        Command::Use { name } => run_use(&config, &name),
        Command::Work {
            name,
            remote,
            repos,
            no_claude,
            no_knowledge,
            no_resources,
            no_cleanup,
            override_remote,
            extra,
        } => run_work(
            &config,
            name,
            remote,
            WorkFlags { repos, no_claude, no_knowledge, no_resources, no_cleanup, override_remote },
            extra,
        ),
        Command::Info { name } => run_info(&config, name),
        Command::Claude { action } => match action {
            ClaudeCommand::Project { name, extra } => {
                run_claude_open(&config, name, extra, Target::Project)
            }
            ClaudeCommand::Repos { name, extra } => {
                run_claude_open(&config, name, extra, Target::Repos)
            }
        },
        Command::Compile { name } => run_compile(&config, name),
        Command::Edit {
            filename,
            project,
            list_alias,
        } => run_edit(&config, filename, project, list_alias),
        Command::Add {
            path,
            kind,
            project,
        } => run_add(&config, project, path, kind),
        Command::Remove {
            path,
            kind,
            project,
        } => run_remove(&config, project, path, kind),
        Command::List { name, kind } => run_list(&config, name, kind),
        Command::Validate { name, all } => run_validate(&config, name, all),
        Command::Create {
            name,
            knowledge_location,
            active,
            force,
        } => run_create(&config, &name, knowledge_location, active, force),
        Command::Completions { .. } => unreachable!("handled before Config::load"),
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

fn run_work(
    config: &Config,
    name: Option<String>,
    remote: Option<String>,
    flags: WorkFlags,
    extra: Vec<String>,
) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, name)?;
    match remote {
        None => run_work_local(config, &name, flags.repos, &extra),
        Some(host) => run_work_remote(config, &name, &host, flags, &extra),
    }
}

fn run_work_local(config: &Config, name: &str, repos: bool, extra: &[String]) -> jeru::Result<()> {
    // Generate CLAUDE.md once; skip silently if it already exists.
    match jeru::init_claude_md(config, name, false) {
        Ok(path) => println!("Wrote {}", path.display()),
        Err(jeru::Error::AlreadyExists(_)) => {}
        Err(e) => return Err(e),
    }

    // Generate workspace once and open it. Skip if it already exists or has no repos.
    let ws_path = jeru::workspace_path(config, name);
    let workspace = if ws_path.exists() {
        Some(ws_path)
    } else {
        match jeru::write_workspace(config, name) {
            Ok(p) => {
                println!("Wrote {}", p.display());
                Some(p)
            }
            Err(jeru::Error::NoRepos(_)) if !repos => None,
            Err(e) => return Err(e),
        }
    };
    if let Some(ws) = &workspace {
        jeru::code_command(ws, &[])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }

    // Claude Code: repos mode opens in the first repo, otherwise the project dir.
    let status = if repos {
        jeru::claude_for_repos(config, name, extra)?.status()?
    } else {
        jeru::claude_for_project(config, name, extra)?.status()?
    };
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_work_remote(
    config: &Config,
    name: &str,
    host: &str,
    flags: WorkFlags,
    extra: &[String],
) -> jeru::Result<()> {
    use jeru::remote::{
        SyncOptions, build_sync_pairs, claude_ssh_cmd, launch_tmux, mutagen_start, mutagen_stop,
        remote_add_dirs, remote_check_empty, remote_cleanup, remote_home, remote_mkdirs,
        remote_repos_dirs, tmux_session_name, vscode_open_remote, vscode_open_workspace_remote,
    };

    let manifest = jeru::load_manifest(config, name)?;
    let opts = SyncOptions {
        knowledge: !flags.no_knowledge,
        resources: !flags.no_resources,
        repos_only: flags.repos,
    };

    // Fetch remote home once so all path mapping is consistent.
    eprint!("Connecting to {host} to resolve remote home… ");
    let rhome = remote_home(host)?;
    eprintln!("{rhome}");

    let local_home = dirs::home_dir().ok_or(jeru::Error::NoHomeDir)?;
    let pairs = build_sync_pairs(config, name, &manifest, host, &rhome, &opts)?;

    // Generate CLAUDE.md once; skip silently if it already exists.
    match jeru::init_claude_md(config, name, false) {
        Ok(path) => println!("Wrote {}", path.display()),
        Err(jeru::Error::AlreadyExists(_)) => {}
        Err(e) => return Err(e),
    }

    // Generate workspace once before mutagen starts so the initial sync carries
    // it to the remote.
    let project_remote_path = &pairs.project().remote_path;
    let remote_workspace: Option<String> = {
        let ws_path = jeru::workspace_path(config, name);
        if ws_path.exists() {
            Some(format!("{project_remote_path}/{name}.code-workspace"))
        } else {
            match jeru::write_workspace(config, name) {
                Ok(_) => Some(format!("{project_remote_path}/{name}.code-workspace")),
                Err(jeru::Error::NoRepos(_)) => None,
                Err(e) => return Err(e),
            }
        }
    };

    // Abort if any remote directory is already non-empty.  Stale files from a
    // prior session would be reconciled back into the local tree by mutagen's
    // two-way sync (e.g. a file deleted locally would reappear).
    eprint!("Checking remote directories… ");
    let nonempty = remote_check_empty(host, pairs.all())?;
    if !nonempty.is_empty() {
        if flags.override_remote {
            eprintln!("non-empty, overriding");
            eprint!("Deleting remote directories… ");
            remote_cleanup(host, pairs.all())?;
            eprintln!("done");
        } else {
            eprintln!();
            return Err(jeru::Error::RemoteNotEmpty(
                host.to_string(),
                nonempty.join(" "),
            ));
        }
    } else {
        eprintln!("clean");
    }

    // Ensure remote directories exist before mutagen tries to sync into them.
    eprint!("Creating remote directories… ");
    remote_mkdirs(host, pairs.all())?;
    eprintln!("done");

    // Start (or resume) mutagen sessions.
    println!("Starting {} mutagen session(s)…", pairs.len());
    mutagen_start(pairs.all(), name)?;

    // Determine the remote path that VSCode and Claude will open.
    let (remote_cwd, claude_add_dirs) = if flags.repos {
        let (cwd, add_dirs) = remote_repos_dirs(&manifest, &rhome, &local_home)?;
        (cwd, add_dirs)
    } else {
        let cwd = pairs.project().remote_path.clone();
        let add_dirs = remote_add_dirs(config, &manifest, &rhome, &local_home, &opts)?;
        (cwd, add_dirs)
    };

    // Open VSCode: workspace file when available, project folder otherwise.
    match &remote_workspace {
        Some(ws) => vscode_open_workspace_remote(host, ws)?,
        None => vscode_open_remote(host, &remote_cwd)?,
    }

    // Build the SSH Claude command (unless --no-claude).
    let claude_cmd = if flags.no_claude {
        None
    } else {
        Some(claude_ssh_cmd(host, &remote_cwd, &claude_add_dirs, extra))
    };

    // Launch tmux: blocks until the user closes all windows.
    let session = tmux_session_name(name, host);
    println!("Launching tmux session '{session}'…");
    launch_tmux(&session, claude_cmd.as_deref(), name)?;

    // Clean up mutagen when the user exits.
    println!("Stopping mutagen sessions…");
    mutagen_stop(pairs.all())?;

    // Remove remote directories so the next session starts from a clean slate.
    // Skip with --no-cleanup when re-uploading large repos would be too costly.
    if !flags.no_cleanup {
        eprint!("Cleaning up remote directories… ");
        remote_cleanup(host, pairs.all())?;
        eprintln!("done");
    }
    Ok(())
}

fn run_info(config: &Config, name: Option<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, name)?;
    let manifest = jeru::load_manifest(config, &name)?;
    print_manifest(&manifest);
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
                    "  note: set the Obsidian token before `jeru work`:\n    export {}={key}",
                    config.obsidian_api_key_env
                ),
                None => println!(
                    "  note: set ${} to your Obsidian Local REST API token before `jeru work`",
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

struct WorkFlags {
    repos: bool,
    no_claude: bool,
    no_knowledge: bool,
    no_resources: bool,
    no_cleanup: bool,
    override_remote: bool,
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

fn run_list(config: &Config, name: Option<String>, kind: Option<KindArg>) -> jeru::Result<()> {
    let name = jeru::resolve_project(config, name)?;
    let kind = kind.map(Kind::from);
    let entries = jeru::list_entries(config, &name, kind)?;
    for (k, entry) in &entries {
        println!("{}\t{}", k.label(), entry);
    }
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
