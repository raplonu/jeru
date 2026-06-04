use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
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

/// The full set of sync pairs for a remote work session.
///
/// The project directory pair is always present and accessible by name,
/// removing the implicit `pairs[0]` assumption from callers.
pub struct SyncPairs {
    inner: Vec<SyncPair>,
}

impl SyncPairs {
    /// The sync pair for the project directory itself.
    pub fn project(&self) -> &SyncPair {
        &self.inner[0]
    }

    /// All sync pairs as a slice (project pair is first).
    pub fn all(&self) -> &[SyncPair] {
        &self.inner
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        false // project pair is always present
    }
}

// ── remote home ───────────────────────────────────────────────────────────────

/// Fetch the remote user's home directory via SSH.
pub fn remote_home(host: &str) -> Result<String> {
    let out = Command::new("ssh").args([host, "echo $HOME"]).output()?;
    if !out.status.success() {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ── path mapping ──────────────────────────────────────────────────────────────

/// Map a local absolute path to a remote absolute path, keeping the same
/// relative suffix under `~/`.
pub(super) fn to_remote(local: &Path, local_home: &Path, remote_home: &str) -> Result<String> {
    let rel = local
        .strip_prefix(local_home)
        .map_err(|_| Error::PathNotUnderHome(local.to_string_lossy().into_owned()))?;
    Ok(format!("{remote_home}/{}", rel.to_string_lossy()))
}

// ── session naming ────────────────────────────────────────────────────────────

fn session_name(project: &str, local: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let rel = local.strip_prefix(&home).unwrap_or(local);
    let slug = rel.to_string_lossy().replace('/', "-");
    format!("jeru-{project}-{slug}")
}

// ── sync pairs ────────────────────────────────────────────────────────────────

/// Build the full set of sync pairs for a project.
///
/// The project directory is always the first (and named) pair; callers access
/// it via [`SyncPairs::project`] rather than indexing into a plain slice.
pub fn build_sync_pairs(
    config: &Config,
    project_name: &str,
    manifest: &Manifest,
    host: &str,
    remote_home: &str,
    opts: &SyncOptions,
) -> Result<SyncPairs> {
    let local_home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    let mut inner = Vec::new();

    let mut push = |local: PathBuf| -> Result<()> {
        let rpath = to_remote(&local, &local_home, remote_home)?;
        inner.push(SyncPair {
            session: session_name(project_name, &local),
            remote: format!("{host}:{rpath}"),
            remote_path: rpath,
            local,
        });
        Ok(())
    };

    // Project directory — always first
    push(project_dir(config, project_name))?;

    // primary_repo + repos, deduplicated by path
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut push_repo = |raw: &str| -> Result<()> {
        let p = expand_tilde(raw)?;
        if seen_paths.insert(p.clone()) {
            push(p)?;
        }
        Ok(())
    };
    if let Some(primary) = &manifest.primary_repo {
        push_repo(primary.as_str())?;
    }
    for repo in &manifest.repos {
        push_repo(repo.as_str())?;
    }

    if !opts.repos_only {
        if opts.knowledge {
            for id in &manifest.knowledge_sets {
                push(knowledge_dir(config, id))?;
            }
        }
        if opts.resources {
            for res in &manifest.resources {
                push(expand_tilde(res)?)?;
            }
        }
    }

    Ok(SyncPairs { inner })
}

// ── mutagen ───────────────────────────────────────────────────────────────────

/// Ensure all remote endpoint directories exist via a single SSH call.
///
/// Must be called before `mutagen_start`: mutagen cannot create parent
/// directories itself and will report "Transition problems" if they are absent.
pub fn remote_mkdirs(host: &str, pairs: &[SyncPair]) -> Result<()> {
    let args = pairs
        .iter()
        .map(|p| sq(&p.remote_path))
        .collect::<Vec<_>>()
        .join(" ");
    let cmd = format!("mkdir -p {args}");
    let ok = Command::new("ssh").args([host, &cmd]).status()?.success();
    if !ok {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(())
}

/// Read ignore patterns from a `.gitignore` file in `dir`, if present.
///
/// Returns only actionable lines: empty lines, comments (`#`), and negations
/// (`!`) are skipped — mutagen does not support negation patterns.
fn gitignore_patterns(dir: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(dir.join(".gitignore")) else {
        return Vec::new();
    };
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .map(String::from)
        .collect()
}

/// Start (or resume) a mutagen session for every sync pair.
///
/// All sessions are tagged with `jeru-project=<project>` so they can be
/// selected together (e.g. by `sync monitor --label-selector`).
pub fn mutagen_start(pairs: &[SyncPair], project: &str) -> Result<()> {
    let label = format!("jeru-project={project}");
    for p in pairs {
        let patterns = gitignore_patterns(&p.local);
        let mut cmd = Command::new("mutagen");
        cmd.args([
            "sync",
            "create",
            "--name",
            &p.session,
            "--label",
            &label,
            "--sync-mode",
            "two-way-resolved",
        ]);
        for pat in &patterns {
            cmd.args(["--ignore", pat]);
        }
        cmd.arg(p.local.to_str().unwrap_or_default());
        cmd.arg(&p.remote);
        let ok = cmd.status()?.success();

        if !ok {
            // Session likely already exists — try to resume it.
            let resumed = Command::new("mutagen")
                .args(["sync", "resume", &p.session])
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
            .args(["sync", "terminate", &p.session])
            .status();
    }
    Ok(())
}

// ── remote path helpers ───────────────────────────────────────────────────────

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
        .ok_or_else(|| Error::NoRepos(String::new()))?;

    let cwd = to_remote(&expand_tilde(first)?, local_home, remote_home)?;
    let add_dirs = rest
        .iter()
        .map(|r| to_remote(&expand_tilde(r)?, local_home, remote_home))
        .collect::<Result<Vec<_>>>()?;
    Ok((cwd, add_dirs))
}

/// Build the list of remote absolute paths for Claude's `--add-dir` flags,
/// mirroring the local `additional_directories` logic.
pub fn remote_add_dirs(
    config: &Config,
    manifest: &Manifest,
    remote_home: &str,
    local_home: &Path,
    opts: &SyncOptions,
) -> Result<Vec<String>> {
    let mut dirs = Vec::new();

    if let Some(primary) = &manifest.primary_repo {
        dirs.push(to_remote(&expand_tilde(primary)?, local_home, remote_home)?);
    }
    for repo in &manifest.repos {
        dirs.push(to_remote(&expand_tilde(repo)?, local_home, remote_home)?);
    }
    if opts.knowledge {
        for id in &manifest.knowledge_sets {
            dirs.push(to_remote(&knowledge_dir(config, id), local_home, remote_home)?);
        }
    }
    if opts.resources {
        for res in &manifest.resources {
            dirs.push(to_remote(&expand_tilde(res)?, local_home, remote_home)?);
        }
    }

    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    Ok(dirs)
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Single-quote a shell argument, escaping any single quotes inside.
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sq_plain_string() {
        assert_eq!(sq("hello world"), "'hello world'");
    }

    #[test]
    fn sq_with_single_quote() {
        assert_eq!(sq("it's"), "'it'\\''s'");
    }

    #[test]
    fn sq_empty() {
        assert_eq!(sq(""), "''");
    }

    #[test]
    fn session_name_is_unique_for_different_paths() {
        let a = session_name("proj", std::path::Path::new("/home/user/code/repo-a"));
        let b = session_name("proj", std::path::Path::new("/home/user/code/repo-b"));
        assert_ne!(a, b);
    }

    #[test]
    fn to_remote_maps_relative_suffix() {
        let local = std::path::Path::new("/home/alice/code/myrepo");
        let home = std::path::Path::new("/home/alice");
        assert_eq!(
            to_remote(local, home, "/home/bob").unwrap(),
            "/home/bob/code/myrepo"
        );
    }

    #[test]
    fn to_remote_rejects_path_outside_home() {
        let local = std::path::Path::new("/tmp/something");
        let home = std::path::Path::new("/home/alice");
        assert!(to_remote(local, home, "/home/bob").is_err());
    }
}
