use clap::{Parser, Subcommand};

use jeru::Manifest;

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
    Workon {
        /// Project name (directory under the project tree)
        name: String,
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
        Command::Workon { name } => run_workon(&name),
        Command::Info { name } => run_info(name),
        Command::Claude { action } => match action {
            ClaudeCommand::Init { name, force } => run_claude_init(name, force),
            ClaudeCommand::Settings { name } => run_claude_settings(name),
            ClaudeCommand::Project { name, extra } => run_claude_open(name, extra, Target::Project),
            ClaudeCommand::Repos { name, extra } => run_claude_open(name, extra, Target::Repos),
        },
        Command::Code { name, extra } => run_code(name, extra),
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

fn run_workon(name: &str) -> jeru::Result<()> {
    jeru::workon(name)?;
    println!("Now working on {name}");
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
