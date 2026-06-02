use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::project::{expand_tilde, knowledge_dir, project_dir};

// ── types ─────────────────────────────────────────────────────────────────────

pub struct SyncOptions {
    pub knowledge: bool,
    pub resources: bool,
    /// Sync only repos; skip the project directory, knowledge sets, and resources.
    pub repos_only: bool,
}

/// One local ↔ remote directory pair managed by a mutagen session.
pub struct SyncPair {
    /// Stable mutagen session name for this pair.
    pub session: String,
    pub local: PathBuf,
    /// `host:absolute/remote/path`
    pub remote: String,
    /// Absolute remote path (without `host:` prefix).
    pub remote_path: String,
}

// ── remote home ───────────────────────────────────────────────────────────────

/// Fetch the remote user's home directory via SSH.
pub fn remote_home(host: &str) -> Result<String> {
    let out = Command::new("ssh")
        .args([host, "echo $HOME"])
        .output()?;
    if !out.status.success() {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ── path mapping ──────────────────────────────────────────────────────────────

/// Map a local absolute path to a remote absolute path, keeping the same
/// relative suffix under `~/`.
fn to_remote(local: &Path, local_home: &Path, remote_home: &str) -> Result<String> {
    let rel = local.strip_prefix(local_home).map_err(|_| {
        Error::PathNotUnderHome(local.to_string_lossy().into_owned())
    })?;
    Ok(format!("{remote_home}/{}", rel.to_string_lossy()))
}

// ── session naming ────────────────────────────────────────────────────────────

fn session_name(project: &str, local: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let rel = local.strip_prefix(&home).unwrap_or(local);
    let slug = rel.to_string_lossy().replace('/', "-");
    // Truncate so names don't become unwieldy
    let slug = if slug.len() > 40 { &slug[..40] } else { &slug };
    format!("jeru-{project}-{slug}")
}

// ── sync pairs ────────────────────────────────────────────────────────────────

/// Build the full list of sync pairs for a project.
pub fn build_sync_pairs(
    project_name: &str,
    manifest: &Manifest,
    host: &str,
    remote_home: &str,
    opts: &SyncOptions,
) -> Result<Vec<SyncPair>> {
    let local_home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    let mut pairs = Vec::new();

    let mut push = |local: PathBuf| -> Result<()> {
        let rpath = to_remote(&local, &local_home, remote_home)?;
        pairs.push(SyncPair {
            session: session_name(project_name, &local),
            remote: format!("{host}:{rpath}"),
            remote_path: rpath,
            local,
        });
        Ok(())
    };

    if !opts.repos_only {
        // Project directory
        push(project_dir(project_name)?)?;
    }

    // Repos — always included
    for repo in &manifest.repos {
        push(PathBuf::from(expand_tilde(repo)?))?;
    }

    if !opts.repos_only {
        // Knowledge sets
        if opts.knowledge {
            for id in &manifest.knowledge_sets {
                push(knowledge_dir(id)?)?;
            }
        }

        // Resources
        if opts.resources {
            for res in &manifest.resources {
                push(PathBuf::from(expand_tilde(res)?))?;
            }
        }
    }

    Ok(pairs)
}

// ── mutagen ───────────────────────────────────────────────────────────────────

/// Start (or resume) a mutagen session for every sync pair.
pub fn mutagen_start(pairs: &[SyncPair]) -> Result<()> {
    for p in pairs {
        let ok = Command::new("mutagen")
            .args([
                "sync",
                "create",
                "--name",
                &p.session,
                "--ignore-vcs",
                "--sync-mode",
                "two-way-resolved",
                p.local.to_str().unwrap_or_default(),
                &p.remote,
            ])
            .status()?
            .success();

        if !ok {
            // Session likely already exists — try to resume it.
            let resumed = Command::new("mutagen")
                .args(["sync", "resume", "--name", &p.session])
                .status()?
                .success();
            if !resumed {
                return Err(Error::Mutagen(format!(
                    "could not start or resume session '{}'",
                    p.session
                )));
            }
        }
    }
    Ok(())
}

/// Terminate all mutagen sessions created for this run.
pub fn mutagen_stop(pairs: &[SyncPair]) -> Result<()> {
    for p in pairs {
        // Ignore errors on termination (session may already be gone).
        let _ = Command::new("mutagen")
            .args(["sync", "terminate", "--name", &p.session])
            .status();
    }
    Ok(())
}

// ── VSCode remote ─────────────────────────────────────────────────────────────

/// Open the project directory in VSCode via Remote SSH (non-blocking).
pub fn vscode_open_remote(host: &str, remote_path: &str) -> Result<()> {
    Command::new("code")
        .arg("--folder-uri")
        .arg(format!("vscode-remote://ssh-remote+{host}{remote_path}"))
        .spawn()?;
    Ok(())
}

// ── tmux ──────────────────────────────────────────────────────────────────────

/// Sanitise an arbitrary string for use as a tmux session name.
pub fn tmux_session_name(project: &str, host: &str) -> String {
    let slug = host.replace(['@', '.', ':'], "-");
    format!("jeru-{project}-{slug}")
}

/// Build the `ssh -t host 'cd … && claude …'` command string for tmux.
pub fn claude_ssh_cmd(
    host: &str,
    remote_project_path: &str,
    add_dirs: &[String],
    extra: &[String],
) -> String {
    let add = add_dirs
        .iter()
        .map(|d| format!("--add-dir {}", sq(d)))
        .collect::<Vec<_>>()
        .join(" ");
    let tail = extra.join(" ");
    let inner = format!("cd {rp} && claude {add} {tail}", rp = sq(remote_project_path));
    format!("ssh -t {host} {}", sq(&inner))
}

/// Launch a tmux session with a `sync` window (mutagen monitor) and,
/// optionally, a `claude` window.  Blocks until the user closes the session,
/// then returns.
pub fn launch_tmux(
    session: &str,
    pairs: &[SyncPair],
    claude_cmd: Option<&str>,
) -> Result<()> {
    let monitor_cmd = format!(
        "mutagen sync monitor {}",
        pairs.iter().map(|p| p.session.as_str()).collect::<Vec<_>>().join(" ")
    );

    // Create session (detached). If it already exists we just re-attach below.
    let created = Command::new("tmux")
        .args(["new-session", "-d", "-s", session, "-n", "sync", &monitor_cmd])
        .status()?
        .success();

    if created {
        if let Some(cmd) = claude_cmd {
            Command::new("tmux")
                .args(["new-window", "-t", session, "-n", "claude", cmd])
                .status()?;
            // Start focused on the claude window.
            Command::new("tmux")
                .args(["select-window", "-t", &format!("{session}:claude")])
                .status()?;
        }
    }

    // Attach — blocks until the user closes all windows.
    Command::new("tmux")
        .args(["attach-session", "-t", session])
        .status()?;

    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Single-quote a shell argument, escaping any single quotes inside.
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Return `(cwd, add_dirs)` for a repos-only remote Claude invocation.
///
/// `cwd` is the remote path of the first repo; `add_dirs` are the remaining
/// repos' remote paths.
pub fn remote_repos_dirs(
    manifest: &Manifest,
    remote_home: &str,
    local_home: &Path,
) -> Result<(String, Vec<String>)> {
    let (first, rest) = manifest
        .repos
        .split_first()
        .ok_or_else(|| crate::error::Error::NoRepos(String::new()))?;

    let cwd = to_remote(&PathBuf::from(expand_tilde(first)?), local_home, remote_home)?;
    let add_dirs = rest
        .iter()
        .map(|r| to_remote(&PathBuf::from(expand_tilde(r)?), local_home, remote_home))
        .collect::<Result<Vec<_>>>()?;
    Ok((cwd, add_dirs))
}

/// Build the list of remote absolute paths for Claude's `--add-dir` flags,
/// mirroring the local `additional_directories` logic.
pub fn remote_add_dirs(
    manifest: &Manifest,
    remote_home: &str,
    local_home: &Path,
    opts: &SyncOptions,
) -> Result<Vec<String>> {
    let mut dirs = Vec::new();

    if let Some(primary) = &manifest.primary_repo {
        dirs.push(to_remote(&PathBuf::from(expand_tilde(primary)?), local_home, remote_home)?);
    }
    for repo in &manifest.repos {
        dirs.push(to_remote(&PathBuf::from(expand_tilde(repo)?), local_home, remote_home)?);
    }
    if opts.knowledge {
        for id in &manifest.knowledge_sets {
            dirs.push(to_remote(&knowledge_dir(id)?, local_home, remote_home)?);
        }
    }
    if opts.resources {
        for res in &manifest.resources {
            dirs.push(to_remote(&PathBuf::from(expand_tilde(res)?), local_home, remote_home)?);
        }
    }

    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    Ok(dirs)
}
