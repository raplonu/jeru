use std::time::{Duration, Instant};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::launch::{prepare_local_session, resolve_obsidian_token};
use crate::obsidian::{ensure_running, port_reachable};
use crate::mcp::mcp_host_port;
use crate::project::{init_claude_md, load_manifest};
use crate::remote::{
    McpTunnel, SyncOptions, build_sync_pairs, claude_local_cmd, remote_add_dirs,
    remote_capture_pane, remote_check_empty, remote_cleanup, remote_home, remote_loop_script,
    remote_loop_tmux_cmd, remote_mkdirs, remote_repos_dirs, remote_tmux_name,
    remote_write_settings, tmux_capture_pane, tmux_has_session, tmux_name, tmux_new_detached,
    tmux_new_window, vscode_remote_uri, mutagen_start, write_remote_loop_script,
};

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
    tmux_new_detached(tmux, "claude", &cmd)?;

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
    let output = poll_capture(|| tmux_capture_pane(&format!("{tmux}:claude")));

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
    report(&state, output.as_deref());
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

    // Abort if any remote directory is already non-empty (stale files would be
    // reconciled back into the local tree by mutagen's two-way sync).
    eprint!("Checking remote directories… ");
    let nonempty = remote_check_empty(host, pairs.all())?;
    if !nonempty.is_empty() {
        if opts.override_remote {
            eprintln!("non-empty, overriding");
            eprint!("Deleting remote directories… ");
            remote_cleanup(host, pairs.all())?;
            eprintln!("done");
        } else {
            eprintln!();
            return Err(Error::RemoteNotEmpty(host.to_string(), nonempty.join(" ")));
        }
    } else {
        eprintln!("clean");
    }

    eprint!("Creating remote directories… ");
    remote_mkdirs(host, pairs.all())?;
    eprintln!("done");

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

    let vscode_url = vscode_remote_uri(host, &remote_cwd);
    // claude runs in the remote tmux; capture its pane over ssh once it boots.
    let output = poll_capture(|| remote_capture_pane(host, &remote_tmux));

    let state = SessionState {
        id: id.to_string(),
        project: project.to_string(),
        remote: Some(host.to_string()),
        spawn: opts.spawn.clone(),
        tmux: tmux.to_string(),
        remote_tmux: Some(remote_tmux),
        mutagen_sessions: pairs.all().iter().map(|p| p.session.clone()).collect(),
        remote_dirs: pairs.all().iter().map(|p| p.remote_path.clone()).collect(),
        no_cleanup: opts.no_cleanup,
        vscode_url,
        claude_output: output.clone(),
        started_at: now_epoch(),
    };
    state.save(config)?;
    report(&state, output.as_deref());
    Ok(())
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

/// Print the session URLs and captured claude output.
fn report(state: &SessionState, output: Option<&str>) {
    println!("\nSession '{}' started.", state.id);
    println!("  VSCode:  {}", crate::vscode::osc8_link(&state.vscode_url));
    match output {
        Some(text) if !text.trim().is_empty() => {
            println!("  Claude:\n{}", indent(text.trim_end()));
        }
        _ => println!(
            "  Claude:  (no output captured yet — `jeru session inspect {}`)",
            state.id
        ),
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
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
