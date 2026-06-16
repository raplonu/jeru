use std::time::{Duration, Instant};

use crate::config::Config;
use crate::constants::WORKSPACE_EXT;
use crate::error::{Error, Result};
use crate::launch::{prepare_local_session, resolve_obsidian_token};
use crate::obsidian::{ensure_running, port_reachable};
use crate::mcp::mcp_host_port;
use crate::project::{init_claude_md, load_manifest};
use crate::remote::{
    DirDiff, McpTunnel, SyncOptions, SyncPairs, build_sync_pairs, claude_local_cmd,
    remote_add_dirs, remote_capture_pane, remote_check_empty, remote_cleanup, remote_compare,
    remote_home, remote_kill_tmux, remote_loop_script, remote_loop_tmux_cmd, remote_mkdirs,
    remote_repos_dirs,
    remote_rm_dirs, remote_tmux_name, remote_write_file, remote_write_settings, sq,
    tmux_capture_pane, tmux_has_session, tmux_name, tmux_new_detached, tmux_new_window,
    tmux_respawn_window, vscode_remote_uri, mutagen_start, write_remote_loop_script,
};

use super::conflicts::{self, Resolution};
use super::state::{SessionState, now_epoch, session_id};

/// Options controlling how a session is started.
pub struct StartOptions {
    /// `claude remote-control --spawn` mode (e.g. "same-dir").
    pub spawn: String,
    /// Work only on repos (claude opens in the first repo; only repos synced).
    pub repos: bool,
    /// Do not sync resources (remote only).
    pub no_resources: bool,
    /// Skip removing remote directories on stop (remote only).
    pub no_cleanup: bool,
    /// Delete non-empty remote directories at startup instead of aborting.
    pub override_remote: bool,
}

/// Start a session for `project`, locally or on `remote`.
pub fn start(
    config: &Config,
    project: &str,
    remote: Option<&str>,
    opts: &StartOptions,
) -> Result<()> {
    let id = session_id(project, remote);
    let tmux = tmux_name(&id);
    if tmux_has_session(&tmux) || SessionState::load(config, &id)?.is_some() {
        return Err(Error::AlreadyExists(format!("session '{id}'")));
    }
    match remote {
        None => start_local(config, project, &id, &tmux, opts),
        Some(host) => start_remote(config, project, host, &id, &tmux, opts),
    }
}

fn start_local(
    config: &Config,
    project: &str,
    id: &str,
    tmux: &str,
    opts: &StartOptions,
) -> Result<()> {
    // Launch Obsidian normally if its MCP server isn't up (fire-and-forget).
    ensure_running(config);

    init_claude(config, project)?;
    let launch = prepare_local_session(config, project, opts.repos)?;
    let cwd = launch.cwd.to_string_lossy().into_owned();

    let cmd = claude_local_cmd(&cwd, &opts.spawn, launch.token.as_deref());
    println!("Launching session '{id}' in tmux…");
    // If claude exits immediately (e.g. the workspace-trust error), `exec sh`
    // keeps the pane alive so its output stays readable for trust detection
    // instead of the lone window — and the whole session — dying instantly.
    tmux_new_detached(tmux, "claude", &format!("{cmd}; exec sh"))?;

    // Prefer opening the generated `.code-workspace` (lists all repos) over
    // the bare project folder; fall back when the project has no repos.
    let target = match crate::vscode::write_workspace(config, project) {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(Error::NoRepos(_)) => cwd.clone(),
        Err(e) => return Err(e),
    };
    // `windowId=_blank` tells VSCode to open the folder in a new window
    // instead of reusing an existing one.
    let vscode_url = format!("vscode://file{target}?windowId=_blank");
    let raw = poll_capture(|| tmux_capture_pane(&format!("{tmux}:claude")));
    let output = if raw.as_deref().is_some_and(is_trust_error) {
        handle_local_trust(&cwd, tmux, &cmd)?
    } else {
        raw
    };

    let state = SessionState {
        id: id.to_string(),
        project: project.to_string(),
        remote: None,
        spawn: opts.spawn.clone(),
        tmux: tmux.to_string(),
        remote_tmux: None,
        mutagen_sessions: Vec::new(),
        remote_dirs: Vec::new(),
        no_cleanup: false,
        vscode_url,
        claude_output: output.clone(),
        started_at: now_epoch(),
    };
    state.save(config)?;
    report(&state);
    Ok(())
}

fn start_remote(
    config: &Config,
    project: &str,
    host: &str,
    id: &str,
    tmux: &str,
    opts: &StartOptions,
) -> Result<()> {
    let manifest = load_manifest(config, project)?;

    // Launch Obsidian normally if needed, before the reverse tunnel is built so
    // its MCP port is already listening (fire-and-forget).
    ensure_running(config);

    let sync_opts = SyncOptions {
        // Knowledge is served live over the forwarded Obsidian MCP port, so it is
        // only mutagen-synced when MCP is disabled.
        knowledge: !config.obsidian_mcp_enabled,
        resources: !opts.no_resources,
        repos_only: opts.repos,
    };

    eprint!("Connecting to {host} to resolve remote home… ");
    let rhome = remote_home(host)?;
    eprintln!("{rhome}");

    let local_home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    let pairs = build_sync_pairs(config, project, &manifest, host, &rhome, &sync_opts)?;

    init_claude(config, project)?;

    // Ensure .mcp.json is in the project dir so the initial sync carries it.
    if let Some(path) = crate::mcp::write_mcp_json(config, project)? {
        println!("Wrote {}", path.display());
    }

    // Check whether any remote directory is already non-empty: stale files
    // could be reconciled back into the local tree by mutagen's two-way
    // sync, so non-empty dirs are compared against local and, if they
    // conflict, resolved interactively before proceeding.
    eprint!("Checking remote directories… ");
    let nonempty = remote_check_empty(host, pairs.all())?;
    if nonempty.is_empty() {
        eprintln!("clean");
    } else if opts.override_remote {
        eprintln!("non-empty, overriding");
        eprint!("Deleting remote directories… ");
        remote_cleanup(host, pairs.all())?;
        eprintln!("done");
    } else {
        eprintln!("non-empty, comparing…");
        if !resolve_remote_conflicts(host, &pairs, &nonempty)? {
            println!("Aborted.");
            return Ok(());
        }
    }

    eprint!("Creating remote directories… ");
    remote_mkdirs(host, pairs.all())?;
    eprintln!("done");

    // Write a `.code-workspace` file on the remote with folder paths
    // translated to the remote tree, mirroring `write_workspace` for local
    // sessions. Excluded from the project dir's mutagen sync (see
    // `build_sync_pairs`) so it doesn't clash with the local copy.
    let remote_workspace = match crate::vscode::remote_workspace_content(config, project, &local_home, &rhome) {
        Ok(content) => {
            let path = format!("{}/{project}{WORKSPACE_EXT}", pairs.project().remote_path);
            remote_write_file(host, &path, &content)?;
            Some(path)
        }
        Err(Error::NoRepos(_)) => None,
        Err(e) => return Err(e),
    };

    println!("Starting {} mutagen session(s)…", pairs.len());
    mutagen_start(pairs.all(), project)?;

    // Remote path that claude opens, and the dirs written into its settings.
    let (remote_cwd, claude_add_dirs) = if opts.repos {
        remote_repos_dirs(&manifest, &rhome, &local_home)?
    } else {
        let cwd = pairs.project().remote_path.clone();
        let add_dirs = remote_add_dirs(config, &manifest, &rhome, &local_home, &sync_opts)?;
        (cwd, add_dirs)
    };
    remote_write_settings(host, &remote_cwd, &claude_add_dirs)?;

    let tunnel = build_mcp_tunnel(config);
    let remote_tmux = remote_tmux_name(project);
    let script = remote_loop_script(host, &remote_tmux, &remote_cwd, &opts.spawn, tunnel.as_ref());
    let script_path = SessionState::dir(config).join(format!("{tmux}-remote-loop.sh"));
    write_remote_loop_script(&script_path, &script)?;
    let claude_cmd = remote_loop_tmux_cmd(&script_path);

    // Detached local tmux: a `sync` window monitoring mutagen and a `claude`
    // window holding the self-reconnecting ssh into the remote tmux.
    let monitor_cmd = format!("mutagen sync monitor --label-selector jeru-project={project}");
    println!("Launching session '{id}' in tmux…");
    tmux_new_detached(tmux, "sync", &monitor_cmd)?;
    tmux_new_window(tmux, "claude", &claude_cmd)?;

    let vscode_target = remote_workspace.unwrap_or_else(|| remote_cwd.clone());
    let vscode_url = vscode_remote_uri(host, &vscode_target);
    // claude runs in the remote tmux; capture its pane over ssh once it boots.
    // The `; exec sh` fallback in the loop script keeps that session alive after a
    // trust error, so its output stays readable here instead of being erased by
    // the reconnect loop.
    let raw = poll_capture(|| remote_capture_pane(host, &remote_tmux));
    let output = if raw.as_deref().is_some_and(is_trust_error) {
        handle_remote_trust(host, &remote_cwd)?;
        // Kill the now-idle remote session so the reconnect loop relaunches
        // claude — this time in the trusted directory.
        remote_kill_tmux(host, &remote_tmux)?;
        poll_capture(|| remote_capture_pane(host, &remote_tmux))
    } else {
        raw
    };

    let state = SessionState {
        id: id.to_string(),
        project: project.to_string(),
        remote: Some(host.to_string()),
        spawn: opts.spawn.clone(),
        tmux: tmux.to_string(),
        remote_tmux: Some(remote_tmux),
        mutagen_sessions: pairs.all().iter().map(|p| p.session.clone()).collect(),
        // Repos are intentionally omitted: they're left on the remote and
        // reconciled by the conflict manager on the next `session up` rather
        // than wiped on `session down` (see `SyncPair::is_repo`).
        remote_dirs: pairs
            .all()
            .iter()
            .filter(|p| !p.is_repo)
            .map(|p| p.remote_path.clone())
            .collect(),
        no_cleanup: opts.no_cleanup,
        vscode_url,
        claude_output: output.clone(),
        started_at: now_epoch(),
    };
    state.save(config)?;
    report(&state);
    Ok(())
}

/// For each sync pair flagged non-empty by `remote_check_empty`, compare local
/// vs remote contents. Pairs that are "safe" ([`DirDiff::is_safe`]) proceed
/// untouched. Pairs with conflicts go through the interactive resolver.
///
/// Returns `Ok(false)` if the user aborts (the caller should print "Aborted."
/// and return early without starting mutagen); `Ok(true)` to proceed. Pairs
/// resolved as "override" have their remote directory wiped via
/// `remote_rm_dirs`; "continue" pairs are left as-is for mutagen to reconcile.
fn resolve_remote_conflicts(host: &str, pairs: &SyncPairs, nonempty: &[String]) -> Result<bool> {
    let mut conflicts: Vec<(String, std::path::PathBuf, DirDiff)> = Vec::new();
    for pair in pairs.all() {
        if !nonempty.contains(&pair.remote_path) {
            continue;
        }
        let diff = remote_compare(host, pair)?;
        if !diff.is_safe() {
            conflicts.push((pair.remote_path.clone(), pair.local.clone(), diff));
        }
    }

    if conflicts.is_empty() {
        eprintln!("no conflicts, proceeding");
        return Ok(true);
    }

    match conflicts::resolve(host, &conflicts)? {
        None => Ok(false),
        Some(resolutions) => {
            for ((remote_path, _, _), resolution) in conflicts.iter().zip(resolutions) {
                if resolution == Resolution::Override {
                    remote_rm_dirs(host, std::slice::from_ref(remote_path))?;
                }
            }
            Ok(true)
        }
    }
}

/// Generate CLAUDE.md once; skip silently if it already exists.
fn init_claude(config: &Config, project: &str) -> Result<()> {
    match init_claude_md(config, project, false) {
        Ok(path) => {
            println!("Wrote {}", path.display());
            Ok(())
        }
        Err(Error::AlreadyExists(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

fn report(state: &SessionState) {
    println!();
    super::control::print_session_info(state);
}

fn is_trust_error(output: &str) -> bool {
    output.contains(crate::constants::WORKSPACE_TRUST_ERROR)
}

/// Spawn `claude` interactively in `cwd` so the user can accept workspace
/// trust, then relaunch the original `remote-control` command in the pane.
fn handle_local_trust(cwd: &str, tmux: &str, original_cmd: &str) -> Result<Option<String>> {
    eprintln!("\nWorkspace not trusted. Launching `claude` for trust acceptance (exit when done)…");
    std::process::Command::new(crate::constants::CLAUDE_BIN)
        .current_dir(cwd)
        .status()?;
    // Respawn the (still-alive, shell-backed) pane with a fresh screen so the
    // stale trust error isn't recaptured before claude redraws.
    let target = format!("{tmux}:claude");
    tmux_respawn_window(&target, &format!("{original_cmd}; exec sh"))?;
    Ok(poll_capture(|| tmux_capture_pane(&target)))
}

/// SSH into `host` and run `claude` interactively in `remote_cwd` so the user
/// can accept workspace trust. Blocks until the user exits Claude.
fn handle_remote_trust(host: &str, remote_cwd: &str) -> Result<()> {
    eprintln!("\nWorkspace not trusted on remote. Connecting to {host} for trust acceptance (exit when done)…");
    let cmd = format!("cd {} && {}", sq(remote_cwd), crate::constants::CLAUDE_BIN);
    std::process::Command::new("ssh")
        .args(["-t", host, &cmd])
        .status()?;
    Ok(())
}

/// Poll a capture closure until it yields non-empty output or ~30s elapse.
fn poll_capture(mut capture: impl FnMut() -> Result<String>) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(text) = capture()
            && !text.trim().is_empty()
        {
            return Some(text);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Build the reverse-tunnel description for remote Obsidian MCP access.
///
/// Returns `None` when MCP is disabled. Emits non-fatal warnings if the local
/// Obsidian server is unreachable or no token is available.
pub fn build_mcp_tunnel(config: &Config) -> Option<McpTunnel> {
    if !config.obsidian_mcp_enabled {
        return None;
    }
    let (host, port) = match mcp_host_port(&config.obsidian_mcp_url) {
        Some(hp) => hp,
        None => {
            eprintln!(
                "warning: could not parse Obsidian MCP url '{}'; remote MCP access disabled",
                config.obsidian_mcp_url
            );
            return None;
        }
    };
    if !port_reachable(&host, port) {
        eprintln!(
            "warning: Obsidian MCP server not reachable at {host}:{port} — \
             start Obsidian or remote MCP access won't work"
        );
    }
    let token = resolve_obsidian_token(config);
    if token.is_none() {
        eprintln!(
            "warning: no Obsidian token (${} unset and none found in vault); \
             remote MCP will not authenticate",
            config.obsidian_api_key_env
        );
    }
    Some(McpTunnel { host, port, token })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_real_trust_error() {
        // The exact message `claude remote-control` prints in an untrusted dir.
        let output = "Error: Workspace not trusted. Please run `claude` in \
                      /home/u/project/toto first to review and accept the \
                      workspace trust dialog.";
        assert!(is_trust_error(output));
    }

    #[test]
    fn ignores_normal_startup_output() {
        let output = "·✔︎· Connected · toto · HEAD\n    Capacity: 1/32";
        assert!(!is_trust_error(output));
    }
}
