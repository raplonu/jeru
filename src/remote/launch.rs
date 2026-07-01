use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::constants::CLAUDE_BIN;
use crate::error::{Error, Result};

/// Named tmux socket used for all remote sessions.  `ssh -t` (PTY, login
/// shell) and `ssh host bash` (no PTY) can inherit different `$TMPDIR` values,
/// which makes tmux pick different socket directories.  All remote tmux
/// commands pin `TMUX_TMPDIR=/tmp` and use `-L jeru` so they always hit the
/// same server regardless of environment.
const REMOTE_TMUX_SOCKET: &str = "jeru";

/// Path of the remote file that the claude pane's output is logged to via
/// `tmux pipe-pane`.  We read claude's startup output (and detect the
/// workspace-trust error) from this file rather than `tmux capture-pane`,
/// because some tmux builds (e.g. dgx's `next-3.4`) have a broken
/// `capture-pane` that returns garbage and crashes the server.
fn remote_log_path(remote_tmux: &str) -> String {
    format!("/tmp/{remote_tmux}.log")
}

// ── VSCode remote ─────────────────────────────────────────────────────────────

/// The `vscode://vscode-remote/...` URI that opens `remote_path` on `host` over Remote SSH.
///
/// Printed for the user to open (e.g. `code --folder-uri <uri>`) rather than
/// launching VSCode directly. The `windowId=_blank` query tells VSCode to open
/// the folder in a new window instead of reusing an existing one.
pub fn vscode_remote_uri(host: &str, remote_path: &str) -> String {
    format!("vscode://vscode-remote/ssh-remote+{host}{remote_path}?windowId=_blank")
}

// ── tmux session naming ────────────────────────────────────────────────────────

/// Sanitise a session id into a tmux-safe session name.
///
/// tmux uses `.` and `:` as target separators, so they are not allowed in
/// session names; `/` would also confuse window targets. Everything else
/// (including `@`) is preserved so `project@host` stays readable.
pub fn tmux_name(id: &str) -> String {
    id.replace(['.', ':', '/'], "-")
}

/// The tmux session name used on the *remote* host for a project's claude.
pub fn remote_tmux_name(project: &str) -> String {
    tmux_name(&format!("jeru-{project}"))
}

// ── MCP tunnel ─────────────────────────────────────────────────────────────────

/// A reverse SSH tunnel exposing the local Obsidian MCP server to the remote.
///
/// `-R {port}:{host}:{port}` makes the remote's `host:port` forward back to the
/// local machine, so the synced `.mcp.json` (which points at the same host:port)
/// reaches local Obsidian. The token, if known, is injected into the remote
/// Claude's environment so `${OBSIDIAN_API_KEY}` in `.mcp.json` resolves.
pub struct McpTunnel {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

// ── claude command builders ────────────────────────────────────────────────────

/// The shell command that launches `claude` for a *local* session,
/// to be used as a tmux window command.
///
/// Runs claude in a restart loop so it stays up as long as the tmux session
/// lives. Without `--fresh`, tries `--continue` first; if claude exits non-zero
/// (e.g. "no prior session to continue"), falls back to a plain launch.
pub fn claude_local_cmd(cwd: &str, token: Option<&str>, name: &str, fresh: bool) -> String {
    let env = match token {
        Some(t) => format!("OBSIDIAN_API_KEY={} ", sq(t)),
        None => String::new(),
    };
    let run = format!("{env}{CLAUDE_BIN} --name {}", sq(name));
    let cd = format!("cd {}", sq(cwd));
    if fresh {
        format!("{cd} && while true; do {run}; sleep 1; done")
    } else {
        format!("{cd} && while true; do if ! {run} --continue; then {run}; fi; sleep 1; done")
    }
}

/// The shell script for a *remote* session's tmux window: a self-reconnecting
/// ssh that runs `claude` inside a tmux session **on the remote host**, so
/// claude survives ssh disconnects.
///
/// The inner command creates the remote session **detached** (running claude),
/// turns on `pipe-pane` logging so jeru can read claude's output from a file
/// (see [`remote_log_path`] — `capture-pane` is unusable on some tmux builds),
/// then attaches.  On reconnect the session already exists, so `new-session`
/// is a harmless no-op and we simply re-enable logging and re-attach to the
/// still-running claude.  The `-R` reverse tunnel (when `mcp` is set) is
/// re-established each reconnect, keeping Obsidian MCP reachable.
pub fn remote_loop_script(
    host: &str,
    remote_tmux: &str,
    remote_project_path: &str,
    mcp: Option<&McpTunnel>,
    name: &str,
    fresh: bool,
) -> String {
    let env = match mcp.and_then(|m| m.token.as_deref()) {
        Some(token) => format!("OBSIDIAN_API_KEY={} ", sq(token)),
        None => String::new(),
    };
    // Restart loop keeps claude running inside the remote tmux session.
    // Without `fresh`, tries --continue first; falls back to a plain launch if
    // claude exits non-zero (e.g. "no prior session to continue").
    let run = format!("{env}{CLAUDE_BIN} --name {}", sq(name));
    let rp = sq(remote_project_path);
    let claude = if fresh {
        format!("cd {rp} && while true; do {run}; sleep 1; done")
    } else {
        format!("cd {rp} && while true; do if ! {run} --continue; then {run}; fi; sleep 1; done")
    };
    let log = remote_log_path(remote_tmux);
    // Three tmux commands sequenced with `;` (no shell control flow, so this
    // parses identically under sh and the remote login shell, which may be
    // fish). `env TMUX_TMPDIR=/tmp` pins the socket dir (fish has no inline
    // `VAR=val cmd`). `new-session -d` fails harmlessly if the session already
    // exists (reconnect); `2>/dev/null` swallows that. `pipe-pane` (re-run
    // without `-o`) tees the pane to the log file. `attach` joins the session.
    let tmux = format!("env TMUX_TMPDIR=/tmp tmux -L {}", REMOTE_TMUX_SOCKET);
    let inner = format!(
        "{tmux} new-session -d -s {sess} {cmd} 2>/dev/null; \
         {tmux} pipe-pane -t {sess} {pipe}; \
         {tmux} attach -t {sess}",
        sess = sq(remote_tmux),
        cmd = sq(&claude),
        pipe = sq(&format!("cat >> {log}")),
    );

    let forward = match mcp {
        Some(m) => format!("-R {p}:{h}:{p} ", p = m.port, h = m.host),
        None => String::new(),
    };
    let ssh = format!("ssh -t {forward}{host} {}", sq(&inner));
    // Reconnect loop: if ssh drops (laptop sleep, network move), wait briefly and
    // re-attach to the remote tmux (claude is still running there).
    format!(
        "#!/bin/sh\nwhile true; do {ssh}; echo '[jeru] ssh disconnected; reconnecting…'; sleep 2; done\n"
    )
}

/// Write `script` to `path` and make it executable.
///
/// tmux runs window commands via `$SHELL -c <command>`, which on this machine is
/// fish. Fish's quoting rules diverge from POSIX `sh` for deeply-nested escaped
/// strings (the reconnect loop nests ssh and remote-tmux quoting several levels
/// deep), so passing the loop as an inline `sh -c '...'` string can fail to parse
/// under fish and silently kill the window. Writing it to a script file sidesteps
/// re-parsing entirely: the tmux window command only needs to name the file.
pub fn write_remote_loop_script(path: &Path, script: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, script)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// The tmux window command that runs the reconnect-loop script at `path`.
pub fn remote_loop_tmux_cmd(path: &Path) -> String {
    format!("sh {}", sq(&path.to_string_lossy()))
}

// ── tmux control ───────────────────────────────────────────────────────────────

/// Width/height for detached sessions. A detached session has no client to size
/// it, so it defaults to 80 columns and claude's output wraps at that width —
/// splitting long remote-control URLs across rows. A wide pane keeps the URL on
/// one line. The remote claude inherits this size (the loop's `ssh -t` PTY is
/// this pane), so [`render_screen`] replays its output at the same dimensions.
/// On `inspect`, attaching resizes the session to the client's terminal, so
/// this only affects the detached period.
pub(crate) const DETACHED_WIDTH: u16 = 240;
pub(crate) const DETACHED_HEIGHT: u16 = 50;

/// Create a detached tmux session with a first window running `cmd`.
pub fn tmux_new_detached(session: &str, window: &str, cmd: &str) -> Result<()> {
    tmux_status(&[
        "new-session", "-d", "-x", &DETACHED_WIDTH.to_string(), "-y", &DETACHED_HEIGHT.to_string(),
        "-s", session, "-n", window, cmd,
    ])
}

/// Add a window running `cmd` to an existing session.
pub fn tmux_new_window(session: &str, window: &str, cmd: &str) -> Result<()> {
    tmux_status(&["new-window", "-t", session, "-n", window, cmd])
}

/// Whether a tmux session with exactly the given name exists.
///
/// The `=` prefix forces an exact match; without it, tmux treats `-t` as a
/// prefix pattern and `has-session -t jeru` would report success for an
/// unrelated session like `jeru-menhix-tonix`.
pub fn tmux_has_session(session: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", &format!("={session}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Kill a tmux session. A missing session is treated as success.
pub fn tmux_kill_session(session: &str) -> Result<()> {
    if !tmux_has_session(session) {
        return Ok(());
    }
    tmux_status(&["kill-session", "-t", session])
}

/// Capture the visible contents of a tmux pane (e.g. `session:window`).
///
/// `-J` joins wrapped lines so a URL that spans rows is returned intact.
pub fn tmux_capture_pane(target: &str) -> Result<String> {
    let out = Command::new("tmux")
        .args(["capture-pane", "-p", "-J", "-t", target])
        .output()?;
    if !out.status.success() {
        return Err(Error::Tmux(format!("capture-pane {target} failed")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Attach to (or, when already inside tmux, switch to) a session. Blocks until
/// the user detaches — used by `jeru session inspect`.
pub fn tmux_attach(session: &str) -> Result<()> {
    // Inside an existing tmux session, `attach-session` is rejected
    // ("sessions should be nested with care"). Use `switch-client` instead.
    let attach_cmd = if std::env::var("TMUX").is_ok() {
        "switch-client"
    } else {
        "attach-session"
    };
    tmux_status(&[attach_cmd, "-t", session])
}

/// Kill `target`'s current pane and (re)run `cmd` in it, resetting the screen.
///
/// Used to relaunch claude after the user accepts workspace trust: the window is
/// kept alive by a fallback shell (see the `; exec sh` wrapper in `start_local`),
/// and respawning gives a clean pane so stale error output isn't recaptured.
pub fn tmux_respawn_window(target: &str, cmd: &str) -> Result<()> {
    tmux_status(&["respawn-window", "-k", "-t", target, cmd])
}

fn tmux_status(args: &[&str]) -> Result<()> {
    let ok = Command::new("tmux").args(args).status()?.success();
    if !ok {
        return Err(Error::Tmux(format!("tmux {} failed", args.join(" "))));
    }
    Ok(())
}

// ── remote tmux control (over ssh) ─────────────────────────────────────────────

/// Kill the remote claude tmux session over ssh (gracefully ending claude) and
/// remove its pipe-pane log so the next session starts with a clean capture. A
/// missing session is treated as success.
pub fn remote_kill_tmux(host: &str, remote_tmux: &str) -> Result<()> {
    let tmux = format!("TMUX_TMPDIR=/tmp tmux -L {}", REMOTE_TMUX_SOCKET);
    let cmd = format!(
        "{tmux} has-session -t {t} 2>/dev/null && {tmux} kill-session -t {t}; rm -f {log}; true",
        t = sq(remote_tmux),
        log = sq(&remote_log_path(remote_tmux)),
    );
    let ok = ssh_bash_ok(host, &cmd)?;
    if !ok {
        return Err(Error::RemoteSsh(host.to_string()));
    }
    Ok(())
}

/// Poll the remote claude output over a single SSH connection.
///
/// Reads the `pipe-pane` log file (see [`remote_log_path`]) rather than
/// `tmux capture-pane`, which is broken on some tmux builds.  The polling loop
/// runs *on the remote* so only one SSH round-trip is paid.  Returns the raw
/// (terminal-control-laden) log once it is non-empty, or empty after
/// `timeout_secs` elapse.  Callers should clean it with [`render_screen`].
pub fn remote_poll_capture(host: &str, remote_tmux: &str, timeout_secs: u32) -> Result<String> {
    let log = remote_log_path(remote_tmux);
    let script = format!(
        concat!(
            "log={log}\n",
            "end=$((SECONDS + {timeout}))\n",
            "while [ $SECONDS -lt $end ]; do\n",
            "  if [ -s \"$log\" ]; then\n",
            "    cat \"$log\"\n",
            "    exit 0\n",
            "  fi\n",
            "  sleep 0.5\n",
            "done\n",
            "exit 1\n",
        ),
        log = sq(&log),
        timeout = timeout_secs,
    );
    let out = ssh_bash_output(host, &script)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Render the final screen from raw `pipe-pane` output.
///
/// The log is a raw terminal *stream*: claude redraws its status in place with
/// cursor-up + erase sequences, so every frame accumulates in the file. Feeding
/// it through a vt100 emulator replays those redraws and leaves only the final
/// frame — exactly what a terminal would show — and resolves OSC 8 hyperlinks
/// to their visible labels. Rendered at the detached pane size claude drew at.
/// Trailing blank lines and per-line trailing spaces are trimmed.
pub fn render_screen(raw: &str) -> String {
    let mut parser = vt100::Parser::new(DETACHED_HEIGHT, DETACHED_WIDTH, 0);
    parser.process(raw.as_bytes());
    let contents = parser.screen().contents();
    let lines: Vec<&str> = contents.lines().map(|l| l.trim_end()).collect();
    // Drop fully-blank rows at the top and bottom of the screen.
    let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = lines.iter().rposition(|l| !l.is_empty()).map_or(0, |i| i + 1);
    lines[start..end.max(start)].join("\n")
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Single-quote a shell argument, escaping any single quotes inside.
pub(crate) fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Run a script on `host` under bash via SSH, piped through stdin so the
/// remote login shell is irrelevant.
fn ssh_bash_output(host: &str, script: &str) -> std::io::Result<std::process::Output> {
    let mut child = Command::new("ssh")
        .arg(host)
        .arg("bash")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(script.as_bytes())?;
    child.wait_with_output()
}

fn ssh_bash_ok(host: &str, script: &str) -> std::io::Result<bool> {
    Ok(ssh_bash_output(host, script)?.status.success())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_name_sanitises_separators() {
        assert_eq!(tmux_name("proj@user@host.example.com"), "proj@user@host-example-com");
        assert_eq!(tmux_name("a/b:c.d"), "a-b-c-d");
        assert_eq!(tmux_name("plain"), "plain");
    }

    #[test]
    fn remote_tmux_name_prefixes_and_sanitises() {
        assert_eq!(remote_tmux_name("my.proj"), "jeru-my-proj");
    }

    #[test]
    fn vscode_remote_uri_format() {
        assert_eq!(
            vscode_remote_uri("user@host", "/home/u/p"),
            "vscode://vscode-remote/ssh-remote+user@host/home/u/p?windowId=_blank"
        );
    }

    #[test]
    fn local_cmd_with_token() {
        let cmd = claude_local_cmd("/home/u/proj", Some("secret"), "proj", false);
        assert!(cmd.contains("cd '/home/u/proj'"), "cmd: {cmd}");
        assert!(cmd.contains("OBSIDIAN_API_KEY='secret'"), "cmd: {cmd}");
        assert!(cmd.contains("while true"), "cmd: {cmd}");
        assert!(cmd.contains("--continue"), "cmd: {cmd}");
        assert!(cmd.contains("claude --name 'proj'"), "cmd: {cmd}");
    }

    #[test]
    fn local_cmd_without_token_omits_env() {
        let cmd = claude_local_cmd("/home/u/proj", None, "proj", false);
        assert!(!cmd.contains("OBSIDIAN_API_KEY"), "cmd: {cmd}");
        assert!(cmd.contains("while true"), "cmd: {cmd}");
        assert!(cmd.contains("--continue"), "cmd: {cmd}");
    }

    #[test]
    fn local_cmd_fresh_omits_continue() {
        let cmd = claude_local_cmd("/home/u/proj", None, "proj", true);
        assert!(cmd.contains("while true"), "cmd: {cmd}");
        assert!(cmd.contains("claude --name 'proj'"), "cmd: {cmd}");
        assert!(!cmd.contains("--continue"), "cmd: {cmd}");
    }

    #[test]
    fn remote_loop_wraps_remote_tmux_and_reconnects() {
        let script = remote_loop_script("myhost", "jeru-proj", "/remote/proj", None, "proj", false);
        // Script is meant to be run via `sh`.
        assert!(script.starts_with("#!/bin/sh\n"), "script: {script}");
        // Creates the remote session detached, on a pinned socket so all remote
        // tmux commands hit the same server. Uses `env` because the SSH command
        // goes through the login shell (which may be fish — no `VAR=val cmd`).
        assert!(script.contains("env TMUX_TMPDIR=/tmp tmux -L jeru new-session -d -s"), "script: {script}");
        // Logs the pane to a file via pipe-pane (capture-pane is unusable on
        // some tmux builds) and then attaches.
        assert!(script.contains("pipe-pane -t"), "script: {script}");
        assert!(script.contains("cat >> /tmp/jeru-proj.log"), "script: {script}");
        assert!(script.contains("attach -t"), "script: {script}");
        assert!(script.contains("jeru-proj"), "script: {script}");
        assert!(script.contains("claude --name"), "script: {script}");
        assert!(script.contains("--continue"), "script: {script}");
        // Restart loop keeps claude running inside the remote tmux session.
        assert!(script.contains("while true"), "script: {script}");
        // Self-reconnecting loop.
        assert!(script.contains("while true"), "script: {script}");
        // No tunnel when mcp is None.
        assert!(!script.contains("-R "), "script: {script}");
        assert!(!script.contains("OBSIDIAN_API_KEY"), "script: {script}");
    }

    #[test]
    fn render_screen_strips_escapes_and_preserves_utf8() {
        let raw = "\u{1b}[?2004h\u{1b}[1m·✔︎· Connected · toto\u{1b}[0m\r\n";
        assert_eq!(render_screen(raw), "·✔︎· Connected · toto");
    }

    #[test]
    fn render_screen_collapses_in_place_redraws() {
        // claude redraws its status with cursor-up + erase-display between
        // frames. Only the final frame should survive.
        let raw = "\u{1b}[33mConnecting\u{1b}[39m\r\n\
                   \u{1b}[1A\u{1b}[JConnected v1\r\n\
                   \u{1b}[1A\u{1b}[JConnected v2\r\n";
        assert_eq!(render_screen(raw), "Connected v2");
    }

    #[test]
    fn render_screen_resolves_osc8_hyperlink_to_label() {
        // OSC 8 hyperlink: ESC ]8;;URL BEL <label> ESC ]8;; BEL → keep label.
        let raw = "\u{1b}]8;;https://example.com/x\u{07}mavis@dgx\u{1b}]8;;\u{07}\r\n";
        assert_eq!(render_screen(raw), "mavis@dgx");
    }

    #[test]
    fn write_remote_loop_script_creates_executable_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("remote-loop.sh");
        let script = remote_loop_script("myhost", "jeru-proj", "/remote/proj", None, "proj", false);
        write_remote_loop_script(&path, &script).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, script);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "script should be executable");
    }

    #[test]
    fn remote_loop_tmux_cmd_quotes_path() {
        let cmd = remote_loop_tmux_cmd(Path::new("/home/u/it's a dir/script.sh"));
        assert!(cmd.starts_with("sh "), "cmd: {cmd}");
        // A single level of quoting parses fine under both sh and fish, unlike
        // the deeply-nested `sh -c '...'` string this replaced.
        assert!(sh_parses(&cmd), "not valid shell: {cmd}");
    }

    /// Whether `cmd` is syntactically valid POSIX shell (parsed, not executed).
    fn sh_parses(cmd: &str) -> bool {
        Command::new("sh")
            .args(["-n", "-c", cmd])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn local_cmd_is_valid_shell() {
        // Paths/tokens with awkward characters must survive quoting.
        let cmd = claude_local_cmd("/home/u/it's a dir", Some("to'ken"), "proj", false);
        assert!(sh_parses(&cmd), "not valid shell: {cmd}");
    }

    #[test]
    fn remote_loop_is_valid_shell() {
        // The remote loop nests three quoting levels (loop → ssh → remote tmux);
        // make sure the whole thing still parses as shell.
        let tunnel = McpTunnel {
            host: "127.0.0.1".to_string(),
            port: 27123,
            token: Some("to'ken".to_string()),
        };
        let script = remote_loop_script(
            "user@host",
            "jeru-proj",
            "/home/u/it's a dir",
            Some(&tunnel),
            "proj@user@host",
            false,
        );
        assert!(sh_parses(&script), "not valid shell: {script}");
    }

    #[test]
    fn remote_loop_adds_reverse_tunnel_and_token() {
        let tunnel = McpTunnel {
            host: "127.0.0.1".to_string(),
            port: 27123,
            token: Some("secret-token".to_string()),
        };
        let script =
            remote_loop_script("myhost", "jeru-proj", "/remote/proj", Some(&tunnel), "proj@myhost", false);
        assert!(script.contains("-R 27123:127.0.0.1:27123"), "script: {script}");
        assert!(
            script.find("-R ").unwrap() < script.find("myhost").unwrap(),
            "forward must precede host: {script}"
        );
        let env_pos = script.find("OBSIDIAN_API_KEY=").expect("token env present");
        let claude_pos = script.find("claude --name").unwrap();
        assert!(env_pos < claude_pos, "token must precede claude: {script}");
        assert!(script.contains("secret-token"), "script: {script}");
    }
}
