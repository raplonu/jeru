use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use dialoguer::{Select, theme::ColorfulTheme};

use jeru::{Kind, Manifest};

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
        /// Arguments after `--` are forwarded to claude
        #[arg(last = true)]
        extra: Vec<String>,
    },
    /// Show or edit the project README
    Readme {
        /// Project name; defaults to the current project
        name: Option<String>,
        #[command(subcommand)]
        action: Option<ReadmeAction>,
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
    /// Show, edit, or manage the project roadmap
    Roadmap {
        /// Project name; defaults to the current project
        name: Option<String>,
        #[command(subcommand)]
        action: Option<RoadmapAction>,
    },
    /// Print a shell completion script to stdout
    Completions {
        /// Target shell
        shell: Shell,
    },
    /// Create a new project
    Create {
        /// Project name (new directory under the project tree)
        name: String,
        /// Set this project as the current one after creating it
        #[arg(long)]
        active: bool,
        /// Create the project even if the directory already exists and is non-empty
        #[arg(long)]
        force: bool,
    },
    /// Open the project manifest (project.yml) in $EDITOR
    Edit {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// Open the project directory in VSCode instead of the manifest in $EDITOR
        #[arg(long)]
        folder: bool,
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

#[derive(Subcommand)]
enum ReadmeAction {
    /// Open the README in $EDITOR
    Edit {
        /// Project name; defaults to the current project
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum RoadmapAction {
    /// Open the roadmap in $EDITOR
    Edit {
        /// Project name; defaults to the current project
        name: Option<String>,
    },
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

    let result = match cli.command {
        Command::Ls => run_ls(),
        Command::Use { name } => run_use(&name),
        Command::Work {
            name,
            remote,
            repos,
            no_claude,
            no_knowledge,
            no_resources,
            extra,
        } => run_work(
            name,
            remote,
            repos,
            no_claude,
            no_knowledge,
            no_resources,
            extra,
        ),
        Command::Info { name } => run_info(name),
        Command::Claude { action } => match action {
            ClaudeCommand::Project { name, extra } => run_claude_open(name, extra, Target::Project),
            ClaudeCommand::Repos { name, extra } => run_claude_open(name, extra, Target::Repos),
        },
        Command::Compile { name } => run_compile(name),

        Command::Completions { shell } => {
            generate(shell, &mut Cli::command(), "jeru", &mut std::io::stdout());
            return;
        }
        Command::Readme { name, action } => run_readme(name, action),
        Command::Roadmap { name, action } => run_roadmap(name, action),
        Command::Edit { name, folder } => run_edit(name, folder),
        Command::Add {
            path,
            kind,
            project,
        } => run_add(project, path, kind),
        Command::Remove {
            path,
            kind,
            project,
        } => run_remove(project, path, kind),
        Command::List { name, kind } => run_list(name, kind),
        Command::Create {
            name,
            active,
            force,
        } => run_create(&name, active, force),
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run_ls() -> jeru::Result<()> {
    let projects = jeru::list_projects()?;
    if projects.is_empty() {
        println!("No projects found.");
    } else {
        for project in projects {
            println!("{}", project.name);
        }
    }
    Ok(())
}

fn run_use(name: &str) -> jeru::Result<()> {
    jeru::use_project(name)?;
    println!("Current project: {name}");
    Ok(())
}

fn run_work(
    name: Option<String>,
    remote: Option<String>,
    repos: bool,
    no_claude: bool,
    no_knowledge: bool,
    no_resources: bool,
    extra: Vec<String>,
) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    match remote {
        None => run_work_local(&name, repos, &extra),
        Some(host) => run_work_remote(
            &name,
            &host,
            repos,
            no_claude,
            no_knowledge,
            no_resources,
            &extra,
        ),
    }
}

fn run_work_local(name: &str, repos: bool, extra: &[String]) -> jeru::Result<()> {
    // Generate CLAUDE.md once; skip silently if it already exists.
    match jeru::init_claude_md(name, false) {
        Ok(path) => println!("Wrote {}", path.display()),
        Err(jeru::Error::AlreadyExists(_)) => {}
        Err(e) => return Err(e),
    }

    // Generate workspace once and open it. Skip if it already exists or has no repos.
    let workspace = match jeru::workspace_path(name) {
        Ok(p) if p.exists() => Some(p),
        _ => match jeru::write_workspace(name) {
            Ok(p) => {
                println!("Wrote {}", p.display());
                Some(p)
            }
            Err(jeru::Error::NoRepos(_)) if !repos => None,
            Err(e) => return Err(e),
        },
    };
    if let Some(ws) = &workspace {
        jeru::code_command(ws, &[]).spawn()?;
    }

    // Claude Code: repos mode opens in the first repo, otherwise the project dir.
    let status = if repos {
        jeru::claude_for_repos(name, extra)?.status()?
    } else {
        jeru::claude_for_project(name, extra)?.status()?
    };
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_work_remote(
    name: &str,
    host: &str,
    repos: bool,
    no_claude: bool,
    no_knowledge: bool,
    no_resources: bool,
    extra: &[String],
) -> jeru::Result<()> {
    use jeru::remote::{
        SyncOptions, build_sync_pairs, claude_ssh_cmd, launch_tmux, mutagen_start, mutagen_stop,
        remote_add_dirs, remote_home, remote_mkdirs, remote_repos_dirs, tmux_session_name,
        vscode_open_remote, vscode_open_workspace_remote,
    };

    let manifest = jeru::load_manifest(name)?;
    let opts = SyncOptions {
        knowledge: !no_knowledge,
        resources: !no_resources,
        repos_only: repos,
    };

    // Fetch remote home once so all path mapping is consistent.
    eprint!("Connecting to {host} to resolve remote home… ");
    let rhome = remote_home(host)?;
    eprintln!("{rhome}");

    let local_home = dirs::home_dir().ok_or(jeru::Error::NoHomeDir)?;
    let pairs = build_sync_pairs(name, &manifest, host, &rhome, &opts)?;

    // Generate CLAUDE.md once; skip silently if it already exists.
    match jeru::init_claude_md(name, false) {
        Ok(path) => println!("Wrote {}", path.display()),
        Err(jeru::Error::AlreadyExists(_)) => {}
        Err(e) => return Err(e),
    }

    // Generate workspace once before mutagen starts so the initial sync carries
    // it to the remote. pairs[0] is always the project dir.
    let project_remote_path = &pairs[0].remote_path;
    let remote_workspace: Option<String> = match jeru::workspace_path(name) {
        Ok(p) if p.exists() => Some(format!("{project_remote_path}/{name}.code-workspace")),
        _ => match jeru::write_workspace(name) {
            Ok(_) => Some(format!("{project_remote_path}/{name}.code-workspace")),
            Err(jeru::Error::NoRepos(_)) => None,
            Err(e) => return Err(e),
        },
    };

    // Ensure remote directories exist before mutagen tries to sync into them.
    eprint!("Creating remote directories… ");
    remote_mkdirs(host, &pairs)?;
    eprintln!("done");

    // Start (or resume) mutagen sessions.
    println!("Starting {} mutagen session(s)…", pairs.len());
    mutagen_start(&pairs, name)?;

    // Determine the remote path that VSCode and Claude will open.
    let (remote_cwd, claude_add_dirs) = if repos {
        let (cwd, add_dirs) = remote_repos_dirs(&manifest, &rhome, &local_home)?;
        (cwd, add_dirs)
    } else {
        // Project directory is always the first pair in non-repos mode.
        let cwd = pairs[0].remote_path.clone();
        let add_dirs = remote_add_dirs(&manifest, &rhome, &local_home, &opts)?;
        (cwd, add_dirs)
    };

    // Open VSCode: workspace file when available, project folder otherwise.
    match &remote_workspace {
        Some(ws) => vscode_open_workspace_remote(host, ws)?,
        None => vscode_open_remote(host, &remote_cwd)?,
    }

    // Build the SSH Claude command (unless --no-claude).
    let claude_cmd = if no_claude {
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
    mutagen_stop(&pairs)?;
    Ok(())
}

fn run_info(name: Option<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    let manifest = jeru::load_manifest(&name)?;
    print_manifest(&manifest);
    Ok(())
}

fn run_compile(name: Option<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;

    let claude_path = jeru::init_claude_md(&name, true)?;
    println!("Wrote {}", claude_path.display());

    let settings_path = jeru::write_settings(&name)?;
    println!("Wrote {}", settings_path.display());

    match jeru::write_workspace(&name) {
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

fn run_claude_open(name: Option<String>, extra: Vec<String>, target: Target) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    let mut command = match target {
        Target::Project => jeru::claude_for_project(&name, &extra)?,
        Target::Repos => jeru::claude_for_repos(&name, &extra)?,
    };
    let status = command.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_readme(name: Option<String>, action: Option<ReadmeAction>) -> jeru::Result<()> {
    match action {
        None => {
            let name = jeru::resolve_project(name)?;
            jeru::readme::show(&name)
        }
        Some(ReadmeAction::Edit { name }) => {
            let name = jeru::resolve_project(name)?;
            jeru::readme::edit(&name)
        }
    }
}

fn run_roadmap(name: Option<String>, action: Option<RoadmapAction>) -> jeru::Result<()> {
    match action {
        None => {
            let name = jeru::resolve_project(name)?;
            jeru::roadmap::show(&name)
        }
        Some(RoadmapAction::Edit { name }) => {
            let name = jeru::resolve_project(name)?;
            jeru::roadmap::edit(&name)
        }
    }
}

fn run_add(project: Option<String>, path: String, kind: Option<KindArg>) -> jeru::Result<()> {
    let name = jeru::resolve_project(project)?;

    let kind: Kind = match kind {
        Some(k) => k.into(),
        None => {
            let detected = jeru::detect_kind(&path)?;
            confirm_kind(&path, detected)?
        }
    };

    jeru::add_to_project(&name, &path, kind)?;
    println!("Added {} '{}' to project {name}", kind.label(), path);
    Ok(())
}

fn run_remove(project: Option<String>, path: String, kind: Option<KindArg>) -> jeru::Result<()> {
    let name = jeru::resolve_project(project)?;

    let kind: Kind = match kind {
        Some(k) => k.into(),
        None => {
            let detected = jeru::detect_kind(&path)?;
            confirm_kind(&path, detected)?
        }
    };

    jeru::remove_from_project(&name, &path, kind)?;
    println!("Removed {} '{}' from project {name}", kind.label(), path);
    Ok(())
}

fn run_list(name: Option<String>, kind: Option<KindArg>) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    let kind = kind.map(Kind::from);
    let entries = jeru::list_entries(&name, kind)?;
    for (k, entry) in &entries {
        println!("{}\t{}", k.label(), entry);
    }
    Ok(())
}

fn run_edit(name: Option<String>, folder: bool) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    if folder {
        let dir = jeru::project_dir(&name)?;
        let status = jeru::code_folder(&dir).status()?;
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        Ok(())
    } else {
        jeru::edit_manifest(&name)
    }
}

fn run_create(name: &str, active: bool, force: bool) -> jeru::Result<()> {
    let dir = jeru::create_project(name, force)?;
    println!("Created project '{name}' at {}", dir.display());
    if active {
        jeru::use_project(name)?;
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
const RESET: &str = "\x1b[0m";

fn print_manifest(m: &Manifest) {
    println!("\n{BOLD}{}{RESET}", m.name);

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
