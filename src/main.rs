use clap::{Parser, Subcommand, ValueEnum};
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
    /// Generate the VSCode workspace and open it in VSCode
    Code {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// Arguments after `--` are forwarded to code
        #[arg(last = true)]
        extra: Vec<String>,
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
    /// Generate the project CLAUDE.md from its manifest
    Init {
        /// Project name; defaults to the current project
        name: Option<String>,
        /// Overwrite an existing CLAUDE.md
        #[arg(short, long)]
        force: bool,
    },
    /// Generate .claude/settings.json so Claude Code can read linked folders
    Settings {
        /// Project name; defaults to the current project
        name: Option<String>,
    },
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
        Command::Work { name, extra } => run_work(name, extra),
        Command::Info { name } => run_info(name),
        Command::Claude { action } => match action {
            ClaudeCommand::Init { name, force } => run_claude_init(name, force),
            ClaudeCommand::Settings { name } => run_claude_settings(name),
            ClaudeCommand::Project { name, extra } => run_claude_open(name, extra, Target::Project),
            ClaudeCommand::Repos { name, extra } => run_claude_open(name, extra, Target::Repos),
        },
        Command::Code { name, extra } => run_code(name, extra),
        Command::Add { path, kind, project } => run_add(project, path, kind),
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

fn run_work(name: Option<String>, extra: Vec<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;

    // VSCode: generate workspace and spawn (non-blocking — it's a GUI app).
    // If the project has no repos we skip VSCode silently.
    match jeru::write_workspace(&name) {
        Ok(workspace) => {
            println!("Wrote {}", workspace.display());
            jeru::code_command(&workspace, &[]).spawn()?;
        }
        Err(jeru::Error::NoRepos(_)) => {}
        Err(e) => return Err(e),
    }

    // Claude Code: run in the foreground (takes over the terminal).
    let status = jeru::claude_for_project(&name, &extra)?.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_info(name: Option<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    let manifest = jeru::load_manifest(&name)?;
    print_manifest(&manifest);
    Ok(())
}

fn run_claude_init(name: Option<String>, force: bool) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    let path = jeru::init_claude_md(&name, force)?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn run_claude_settings(name: Option<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    let path = jeru::write_settings(&name)?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn run_code(name: Option<String>, extra: Vec<String>) -> jeru::Result<()> {
    let name = jeru::resolve_project(name)?;
    let workspace = jeru::write_workspace(&name)?;
    println!("Wrote {}", workspace.display());
    let status = jeru::code_command(&workspace, &extra).status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
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

fn confirm_kind(path: &str, detected: Kind) -> jeru::Result<Kind> {
    const KINDS: [Kind; 3] = [Kind::Repo, Kind::Knowledge, Kind::Resource];
    const LABELS: [&str; 3] = ["repo", "knowledge", "resource"];

    let default = KINDS.iter().position(|k| *k == detected).unwrap_or(0);

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Kind for '{path}'"))
        .items(&LABELS)
        .default(default)
        .interact()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

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
