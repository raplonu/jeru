use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::constants::{ADDITIONAL_DIRS_KEY, WORKSPACE_EXT};
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
    /// Extra mutagen `--ignore` patterns for this pair, on top of any
    /// `.gitignore`-derived ones.
    pub ignore: Vec<String>,
    /// Whether this pair is a code repo (primary_repo or repos).
    ///
    /// Repos are deliberately left on the remote when a session ends: their
    /// divergence is reconciled by the conflict manager (`remote_check_empty`
    /// → `remote_compare` → resolve) on the next `session up`, rather than
    /// being wiped — that avoids destroying remote-only work between sessions.
    pub is_repo: bool,
}

/// Result of comparing a local sync-pair directory against its remote
/// counterpart, by relative file path and size.
#[derive(Debug, Default, PartialEq)]
pub struct DirDiff {
    /// Files present locally but not on the remote.
    pub local_only: Vec<String>,
    /// Files present on the remote but not locally.
    pub remote_only: Vec<String>,
    /// Files present on both sides but with different sizes.
    pub differing: Vec<String>,
}

impl DirDiff {
    /// True if mutagen's two-way sync can proceed without risk of the remote
    /// clobbering local state: the remote has nothing extra and nothing
    /// differs (local-only additions are fine — they'll just sync up).
    pub fn is_safe(&self) -> bool {
        self.remote_only.is_empty() && self.differing.is_empty()
    }
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
pub(crate) fn to_remote(local: &Path, local_home: &Path, remote_home: &str) -> Result<String> {
    let rel = local
        .strip_prefix(local_home)
        .map_err(|_| Error::PathNotUnderHome(local.to_string_lossy().into_owned()))?;
    Ok(format!("{remote_home}/{}", rel.to_string_lossy()))
}

// ── session naming ────────────────────────────────────────────────────────────

/// Build a mutagen session name from `project` and `local`.
///
/// Mutagen session names only allow alphanumerics and `-`, so any other
/// character (path separators, underscores, dots…) is replaced with `-`.
fn session_name(project: &str, local: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let rel = local.strip_prefix(&home).unwrap_or(local);
    let raw = format!("jeru-{project}-{}", rel.to_string_lossy());
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
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

    let mut push = |local: PathBuf, ignore: Vec<String>, is_repo: bool| -> Result<()> {
        let rpath = to_remote(&local, &local_home, remote_home)?;
        inner.push(SyncPair {
            session: session_name(project_name, &local),
            remote: format!("{host}:{rpath}"),
            remote_path: rpath,
            local,
            ignore,
            is_repo,
        });
        Ok(())
    };

    // Project directory — always first. The generated `.code-workspace` file
    // is excluded from sync: local and remote copies list different folder
    // paths, so each side maintains its own.
    push(
        project_dir(config, project_name),
        vec![format!("{project_name}{WORKSPACE_EXT}")],
        false,
    )?;

    // primary_repo + repos, deduplicated by path
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut push_repo = |raw: &str| -> Result<()> {
        let p = expand_tilde(raw)?;
        if seen_paths.insert(p.clone()) {
            push(p, Vec::new(), true)?;
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
                push(knowledge_dir(config, id), Vec::new(), false)?;
            }
        }
        if opts.resources {
            for res in &manifest.resources {
                push(expand_tilde(res)?, Vec::new(), false)?;
            }
        }
    }

    Ok(SyncPairs { inner })
}

// ── mutagen ───────────────────────────────────────────────────────────────────

/// Return the remote paths of any directories that already exist and are non-empty.
///
/// Call this before `remote_mkdirs` / `mutagen_start` to enforce a clean-slate
/// invariant: stale files from a previous session (e.g. files deleted locally
/// but still present on the remote) would be propagated back to local by
/// mutagen's two-way reconciliation, corrupting the working tree.
pub fn remote_check_empty(host: &str, pairs: &[SyncPair]) -> Result<Vec<String>> {
    // For each path: print it if the directory exists AND is non-empty.
    let checks = pairs
        .iter()
        .map(|p| {
            let q = sq(&p.remote_path);
            format!("[ ! -d {q} ] || [ -z \"$(ls -A {q} 2>/dev/null)\" ] || echo {q}")
        })
        .collect::<Vec<_>>()
        .join("; ");

    let out = Command::new("ssh").args([host, &checks]).output()?;
    if !out.status.success() {
        return Err(Error::RemoteSsh(host.to_string()));
    }

    let nonempty: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();

    Ok(nonempty)
}

/// Compare `pair.local` against its remote counterpart by listing all files
/// (path relative to the sync root, plus size in bytes) on both sides and
/// diffing them.
///
/// This is a *heuristic*: path+size equality does not guarantee identical
/// content, but it's enough to surface obvious, surprising divergence before
/// launching mutagen — true content conflicts that slip through are still
/// caught by mutagen's `two-way-resolved` mode at sync time.
///
/// `.git` directories are excluded from both listings (large, and not
/// actionable via this prompt). Symlinks are excluded too (`find -type f`
/// only matches regular files) — a known, accepted gap. Requires GNU `find`
/// (`-printf`), i.e. a Linux remote (and local) host.
///
/// If `pair.local` does not exist, it's treated as "local has nothing": any
/// remote files become `remote_only`, correctly triggering a conflict prompt
/// rather than a silent override.
pub fn remote_compare(host: &str, pair: &SyncPair) -> Result<DirDiff> {
    let local = list_files_local(&pair.local)?;
    let remote = list_files_remote(host, &pair.remote_path)?;
    Ok(diff_listings(&local, &remote))
}

/// `find <dir> -type f -not -path '*/.git/*' -printf '%P\t%s\n' | sort`,
/// parsed into `(relative_path, size)` pairs. Returns an empty list if `dir`
/// does not exist.
fn list_files_local(dir: &Path) -> Result<Vec<(String, u64)>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let out = Command::new("find")
        .args([
            dir.to_str().unwrap_or_default(),
            "-type",
            "f",
            "-not",
            "-path",
            "*/.git/*",
            "-printf",
            "%P\t%s\n",
        ])
        .output()?;
    Ok(parse_file_listing(&out.stdout))
}

/// Same listing as [`list_files_local`], run on `host` over SSH against
/// `remote_path`.
fn list_files_remote(host: &str, remote_path: &str) -> Result<Vec<(String, u64)>> {
    let cmd = format!(
        "find {} -type f -not -path '*/.git/*' -printf '%P\\t%s\\n' | sort",
        sq(remote_path)
    );
    let out = Command::new("ssh").args([host, &cmd]).output()?;
    if !out.status.success() {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(parse_file_listing(&out.stdout))
}

/// Parse `path\tsize\n` lines (as produced by `find -printf '%P\t%s\n'`) into
/// `(path, size)` pairs, sorted by path.
fn parse_file_listing(output: &[u8]) -> Vec<(String, u64)> {
    let mut listing: Vec<(String, u64)> = String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| {
            let (path, size) = line.rsplit_once('\t')?;
            Some((path.to_string(), size.parse().ok()?))
        })
        .collect();
    listing.sort_by(|a, b| a.0.cmp(&b.0));
    listing
}

/// Diff two `(relative_path, size)` listings, sorted by path with unique
/// paths within each listing.
///
/// Paths present in both with matching sizes are dropped; mismatched sizes
/// go to `differing`; paths present on only one side go to `local_only` /
/// `remote_only`.
fn diff_listings(local: &[(String, u64)], remote: &[(String, u64)]) -> DirDiff {
    let mut diff = DirDiff::default();
    let (mut i, mut j) = (0, 0);
    while i < local.len() && j < remote.len() {
        let (lpath, lsize) = &local[i];
        let (rpath, rsize) = &remote[j];
        match lpath.cmp(rpath) {
            std::cmp::Ordering::Less => {
                diff.local_only.push(lpath.clone());
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                diff.remote_only.push(rpath.clone());
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                if lsize != rsize {
                    diff.differing.push(lpath.clone());
                }
                i += 1;
                j += 1;
            }
        }
    }
    diff.local_only.extend(local[i..].iter().map(|(p, _)| p.clone()));
    diff.remote_only.extend(remote[j..].iter().map(|(p, _)| p.clone()));
    diff
}

/// Build the `(src, dst)` argument pair for an rsync between a local directory
/// and its remote counterpart. The trailing slashes make rsync compare the
/// directories' *contents* rather than nesting one inside the other.
fn rsync_endpoints(local: &Path, host: &str, remote_path: &str) -> (String, String) {
    (
        format!("{}/", local.to_string_lossy()),
        format!("{host}:{remote_path}/"),
    )
}

/// Show how `local` differs from its remote counterpart via an `rsync` dry-run.
///
/// This is the interactive "view" for a conflict flagged by [`remote_compare`]:
/// a checksum-based, itemized dry run (`rsync -rni --checksum --delete`) that
/// lists the diverging files from the local side's perspective —
/// `<`/`>`/`c` entries are local additions or content changes, `*deleting`
/// entries exist only on the remote. Nothing is written (`-n`); `.git` is
/// excluded to mirror the detection listing. Output streams straight to the
/// terminal.
pub fn remote_rsync_preview(host: &str, local: &Path, remote_path: &str) -> Result<()> {
    if !local.is_dir() {
        println!("  Local directory does not exist — every remote file is remote-only.");
        return Ok(());
    }
    let (src, dst) = rsync_endpoints(local, host, remote_path);
    let status = Command::new("rsync")
        .args(["-rni", "--checksum", "--delete", "--exclude=.git", &src, &dst])
        .status()?;
    if !status.success() {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(())
}

/// Remove all synced remote directories after a session ends.
///
/// This keeps the remote clean so the next `remote_check_empty` passes.
pub fn remote_cleanup(host: &str, pairs: &[SyncPair]) -> Result<()> {
    let dirs: Vec<String> = pairs.iter().map(|p| p.remote_path.clone()).collect();
    remote_rm_dirs(host, &dirs)
}

/// Remove the given remote directories via a single `rm -rf` over SSH.
///
/// Path-based variant of [`remote_cleanup`] for callers (e.g. `session stop`)
/// that only have the stored remote paths, not full [`SyncPair`]s.
pub fn remote_rm_dirs(host: &str, dirs: &[String]) -> Result<()> {
    if dirs.is_empty() {
        return Ok(());
    }
    let paths = dirs.iter().map(|d| sq(d)).collect::<Vec<_>>().join(" ");
    let cmd = format!("rm -rf {paths}");
    let ok = Command::new("ssh").args([host, &cmd]).status()?.success();
    if !ok {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(())
}

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

/// Write (or update) `.claude/settings.json` inside `remote_dir` on `host` with
/// `dirs` as `additionalDirectories`, mirroring `write_settings_for_dir` for the
/// remote side.
///
/// Linked directories are written into the settings file rather than passed as
/// `--add-dir` flags on the `claude` command line: those flags are rejected once
/// `extra` contains a subcommand (e.g. `claude remote-control --add-dir ...`),
/// causing claude to exit immediately.
pub fn remote_write_settings(host: &str, remote_dir: &str, dirs: &[String]) -> Result<()> {
    let claude_dir = format!("{remote_dir}/.claude");
    let settings_path = format!("{claude_dir}/settings.json");

    let cat_cmd = format!("cat {} 2>/dev/null", sq(&settings_path));
    let out = Command::new("ssh").args([host, &cat_cmd]).output()?;

    let mut root = match serde_json::from_slice::<Value>(&out.stdout) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };

    let permissions = root
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    if !permissions.is_object() {
        return Err(Error::InvalidSettings(settings_path));
    }
    permissions[ADDITIONAL_DIRS_KEY] = json!(dirs);

    let mut content = serde_json::to_string_pretty(&Value::Object(root))?;
    content.push('\n');

    let write_cmd = format!("mkdir -p {} && cat > {}", sq(&claude_dir), sq(&settings_path));
    let mut child = Command::new("ssh")
        .args([host, &write_cmd])
        .stdin(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(content.as_bytes())?;
    let ok = child.wait()?.success();
    if !ok {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(())
}

/// Write `content` to `path` on `host` over SSH, creating parent directories
/// as needed.
pub fn remote_write_file(host: &str, path: &str, content: &str) -> Result<()> {
    let dir = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cmd = format!("mkdir -p {} && cat > {}", sq(&dir), sq(path));
    let mut child = Command::new("ssh")
        .args([host, &cmd])
        .stdin(Stdio::piped())
        .spawn()?;
    child.stdin.take().expect("piped stdin").write_all(content.as_bytes())?;
    let ok = child.wait()?.success();
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
        let mut patterns = gitignore_patterns(&p.local);
        patterns.extend(p.ignore.iter().cloned());
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
    let names: Vec<String> = pairs.iter().map(|p| p.session.clone()).collect();
    mutagen_terminate(&names);
    Ok(())
}

/// Terminate mutagen sessions by name.
///
/// Name-based variant of [`mutagen_stop`] for callers (e.g. `session stop`) that
/// only have the stored session names. Errors are ignored (a session may already
/// be gone).
pub fn mutagen_terminate(sessions: &[String]) {
    for s in sessions {
        let _ = Command::new("mutagen")
            .args(["sync", "terminate", s])
            .status();
    }
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
    fn rsync_endpoints_have_trailing_slashes() {
        let (src, dst) =
            rsync_endpoints(std::path::Path::new("/home/u/repo"), "host", "/home/r/repo");
        assert_eq!(src, "/home/u/repo/");
        assert_eq!(dst, "host:/home/r/repo/");
    }

    #[test]
    fn session_name_is_unique_for_different_paths() {
        let a = session_name("proj", std::path::Path::new("/home/user/code/repo-a"));
        let b = session_name("proj", std::path::Path::new("/home/user/code/repo-b"));
        assert_ne!(a, b);
    }

    #[test]
    fn session_name_sanitises_invalid_characters() {
        let name = session_name(
            "mavis",
            std::path::Path::new("/home/user/rtctk-doc/sphinx_doc/_build/markdown"),
        );
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "unexpected character in {name}"
        );
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

    fn test_config() -> Config {
        let home = dirs::home_dir().unwrap();
        Config {
            projects_dir: home.join("proj"),
            knowledge_dir: home.join("knowledge"),
            cache_dir: home.join(".cache/jeru"),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: "http://127.0.0.1:27123/mcp/".to_string(),
            obsidian_api_key_env: "OBSIDIAN_API_KEY".to_string(),
            obsidian_autostart: false,
            obsidian_launch_cmd: "false".to_string(),
        }
    }

    fn test_manifest() -> Manifest {
        Manifest {
            name: "proj".to_string(),
            knowledge_location: "proj".to_string(),
            primary_repo: None,
            knowledge_sets: vec!["docs".to_string()],
            repos: vec!["~/code/r1".to_string()],
            resources: vec!["~/refs/x".to_string()],
        }
    }

    #[test]
    fn build_sync_pairs_excludes_knowledge_when_disabled() {
        let home = dirs::home_dir().unwrap();
        let opts = SyncOptions { knowledge: false, resources: true, repos_only: false };
        let pairs =
            build_sync_pairs(&test_config(), "proj", &test_manifest(), "host", "/home/remote", &opts)
                .unwrap();
        let locals: Vec<_> = pairs.all().iter().map(|p| p.local.clone()).collect();
        assert!(locals.contains(&home.join("proj/proj")), "project dir present");
        assert!(locals.contains(&home.join("code/r1")), "repo present");
        assert!(locals.contains(&home.join("refs/x")), "resource present");
        assert!(
            !locals.contains(&home.join("knowledge/docs")),
            "knowledge must be excluded: {locals:?}"
        );
    }

    #[test]
    fn build_sync_pairs_flags_only_repos() {
        let home = dirs::home_dir().unwrap();
        let opts = SyncOptions { knowledge: true, resources: true, repos_only: false };
        let pairs =
            build_sync_pairs(&test_config(), "proj", &test_manifest(), "host", "/home/remote", &opts)
                .unwrap();
        let repos: Vec<_> = pairs.all().iter().filter(|p| p.is_repo).map(|p| p.local.clone()).collect();
        // Only the repo dir is flagged; project, knowledge, resources are not.
        assert_eq!(repos, vec![home.join("code/r1")], "only the repo should be flagged: {repos:?}");
        assert!(!pairs.project().is_repo, "project dir must not be a repo");
    }

    #[test]
    fn diff_listings_identical() {
        let listing = vec![("a.txt".to_string(), 10), ("b.txt".to_string(), 20)];
        let diff = diff_listings(&listing, &listing);
        assert_eq!(diff, DirDiff::default());
        assert!(diff.is_safe());
    }

    #[test]
    fn diff_listings_local_only_is_safe() {
        let local = vec![("a.txt".to_string(), 10), ("b.txt".to_string(), 20)];
        let remote = vec![("a.txt".to_string(), 10)];
        let diff = diff_listings(&local, &remote);
        assert_eq!(diff.local_only, vec!["b.txt".to_string()]);
        assert!(diff.remote_only.is_empty());
        assert!(diff.differing.is_empty());
        assert!(diff.is_safe());
    }

    #[test]
    fn diff_listings_remote_only_is_unsafe() {
        let local = vec![("a.txt".to_string(), 10)];
        let remote = vec![("a.txt".to_string(), 10), ("b.txt".to_string(), 20)];
        let diff = diff_listings(&local, &remote);
        assert_eq!(diff.remote_only, vec!["b.txt".to_string()]);
        assert!(!diff.is_safe());
    }

    #[test]
    fn diff_listings_size_mismatch_is_unsafe() {
        let local = vec![("a.txt".to_string(), 10)];
        let remote = vec![("a.txt".to_string(), 99)];
        let diff = diff_listings(&local, &remote);
        assert_eq!(diff.differing, vec!["a.txt".to_string()]);
        assert!(!diff.is_safe());
    }

    #[test]
    fn diff_listings_empty_both() {
        let diff = diff_listings(&[], &[]);
        assert_eq!(diff, DirDiff::default());
        assert!(diff.is_safe());
    }

    #[test]
    fn diff_listings_mixed_categories() {
        let local = vec![
            ("a.txt".to_string(), 1),
            ("b.txt".to_string(), 2),
            ("d.txt".to_string(), 4),
        ];
        let remote = vec![
            ("a.txt".to_string(), 1),
            ("c.txt".to_string(), 3),
            ("d.txt".to_string(), 99),
        ];
        let diff = diff_listings(&local, &remote);
        assert_eq!(diff.local_only, vec!["b.txt".to_string()]);
        assert_eq!(diff.remote_only, vec!["c.txt".to_string()]);
        assert_eq!(diff.differing, vec!["d.txt".to_string()]);
        assert!(!diff.is_safe());
    }

    #[test]
    fn build_sync_pairs_includes_knowledge_when_enabled() {
        let home = dirs::home_dir().unwrap();
        let opts = SyncOptions { knowledge: true, resources: true, repos_only: false };
        let pairs =
            build_sync_pairs(&test_config(), "proj", &test_manifest(), "host", "/home/remote", &opts)
                .unwrap();
        let locals: Vec<_> = pairs.all().iter().map(|p| p.local.clone()).collect();
        assert!(locals.contains(&home.join("knowledge/docs")), "knowledge present: {locals:?}");
    }
}
