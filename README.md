# jeru - A fully **vibe coded** project manager coded in rust

Personal project tree manager. Each project bundles its code repos, knowledge sets, and resources into a single manifest so they can be opened together in Claude Code and VSCode with one command — locally or on a remote machine.

## Concepts

**Project** — a directory under `~/project/<name>/` containing a `project.yml` manifest. Projects are the unit of work; everything else is referenced from there.

**Knowledge set** — a body of reference material stored centrally at `~/knowledge/<id>/`. Projects reference sets by ID; the content is never duplicated.

**Resource** — any other folder a project needs (specs, design files, docs, etc.).

## Manifest

`~/project/<name>/project.yml` is the single source of truth for a project.

```yaml
name: service-a
primary_repo: ~/code/service-a   # optional: default repo for sustained code work
knowledge_sets:
  - rust-async
  - nix-flakes
repos:
  - ~/code/service-a
  - ~/code/service-a-proto
resources:
  - ~/docs/service-a-specs
```

Paths are stored as written (`~/…` or absolute). Knowledge set IDs are resolved to `~/knowledge/<id>`.

## Commands

The CLI is organised into three groups — `jeru project` (alias `p`) for managing
projects, `jeru session` (alias `s`) for running work sessions, and `jeru claude`
for opening Claude Code directly — plus `jeru completions`.

### Managing projects — `jeru project` / `jeru p`

```
jeru project ls                         List all projects
jeru project use <name>                 Set the current project (persisted)
jeru project info [name] [--kind repo|knowledge|resource]   Show the manifest
jeru project create <name> [--active] [--force]
jeru project compile [name]             Regenerate derived files
jeru project validate [name] [--all]    Check manifests for issues
jeru project edit [<name>] [-f <filename>] [--list-alias]
jeru project add <path> [--kind repo|knowledge|resource] [-p <name>]
jeru project remove <path> [--kind repo|knowledge|resource] [-p <name>]
```

Most subcommands accept an optional project name; when omitted they fall back to
the current project set by `jeru project use`. Subcommands have short aliases:
`use`→`u`, `info`→`i`, `create`→`new`, `compile`→`c`, `validate`→`check`,
`edit`→`e`, `remove`→`rm` (e.g. `jeru p i` for `jeru project info`).

- **`info`** prints the full manifest; pass `--kind`/`-k` to show only repos,
  knowledge sets, or resources.
- **`create`** makes a new project directory under `~/project/<name>/` with a
  starter `project.yml`. `--active` sets it current; `--force` allows a non-empty
  existing directory.
- **`edit`** without a filename opens the project folder in VSCode; with a
  filename, opens it in `$EDITOR`. Filenames can be plain (relative to the project
  dir) or a built-in alias: `@project` → `project.yml`, `@readme` → `README.md`,
  `@roadmap` → `ROADMAP.md`. `jeru project edit --list-alias` prints the table.
- **`add`** deduces the kind from the path when `--kind` is omitted (under
  `~/knowledge/` → knowledge set; existing directory → repo; file/extension →
  resource), with an interactive confirmation prompt. **`remove`** works the same
  way.

### Sessions

A **session** is a background activity: `claude remote-control` running in a
detached tmux session, controlled from claude.ai/code or the Claude mobile app.
`jeru session up` returns immediately (it does not take over your terminal),
prints the claude output (including the remote-control URL) and a VSCode URL, and
the session keeps running until you `jeru session down` it.

```
jeru session up [name] [--spawn same-dir|worktree|session] [--repos]
jeru session ls
jeru session down   [session-id]
jeru session attach [session-id]
```

- A session's **id** is the project name (`myproj`) for a local session, or
  `project@host` for a remote one. `down`/`attach` take that id (defaulting to
  the current project), and `ls` prints it.
- `--spawn` is forwarded to `claude remote-control --spawn` (default `same-dir`).
- `--repos` opens claude in the first repo, with the other repos added.
- `jeru session attach` attaches to the session's tmux so you can watch claude;
  detaching leaves the session running.
- Obsidian: if MCP is enabled and its server isn't already up, jeru launches
  Obsidian normally and leaves it running (it is never stopped by jeru).

#### Remote sessions

```
jeru session up [name] --remote <host> [options]
```

Runs the session on an SSH target (`user@hostname` or a `~/.ssh/config` alias).

**What it does:**

1. Fetches the remote `$HOME` via SSH (paths are mirrored: `~/code/…` locally
   becomes `~/code/…` remotely).
2. Starts a **mutagen** continuous two-way sync session per directory (respecting
   `.gitignore`). Knowledge sets are served live over the Obsidian MCP server, so
   they are not synced.
3. Launches a detached **tmux session** with two windows:
   - `sync` — `mutagen sync monitor` for all active sessions
   - `claude` — a self-reconnecting `ssh` that runs `claude remote-control` inside
     a tmux session **on the remote host**, so claude survives ssh disconnects
     (laptop sleep, network changes). When MCP is enabled, an `ssh -R` reverse
     tunnel exposes the local Obsidian server to the remote.

`jeru session down <id>` gracefully ends the remote claude, tears down both tmux
sessions, terminates mutagen, and removes the remote directories.

If a remote sync directory is already non-empty, its contents are compared
against the local directory. If the remote has nothing the local side doesn't
(or only differs by local-only additions), the session proceeds normally. If
the remote has extra or differing files, you're shown an interactive menu per
folder to either override (wipe the remote copy, local wins), continue (let
mutagen's two-way-resolved sync reconcile it), or abort — with shortcuts to
apply a choice to all remaining folders at once.

**Options (only valid with `--remote`):**

| Flag | Effect |
|---|---|
| `--repos` | Sync repos only; claude opens in the first repo |
| `--no-resources` | Do not sync resources |
| `--no-cleanup` | Keep the remote directories when the session is stopped |
| `--override-remote` | Skip the comparison and wipe ALL non-empty remote directories without prompting |

### Compiling a project

`jeru project compile [name]` regenerates all derived files from the manifest in
one step:

- `CLAUDE.md` — project briefing for Claude Code, listing all linked repos, knowledge sets, and resources
- `.claude/settings.json` — sets `additionalDirectories` so Claude Code can read every linked folder
- `<name>.code-workspace` — VSCode workspace with all repos as folders (skipped if the project has no repos)

Run it whenever you change `project.yml`.

### Claude Code integration

```
jeru claude project [name] [-- args]   Open Claude in the project directory
jeru claude repos [name] [-- args]     Open Claude in the first repo
```

## Directory layout

```
~/
├── project/
│   ├── service-a/
│   │   ├── project.yml
│   │   ├── CLAUDE.md                  (generated by jeru project compile)
│   │   ├── ROADMAP.md                 (default roadmap location)
│   │   ├── service-a.code-workspace   (generated by jeru project compile)
│   │   └── .claude/
│   │       └── settings.json          (generated by jeru project compile)
│   └── …
└── knowledge/
    ├── rust-async/
    └── nix-flakes/
```

## Configuration

The three base directories can be overridden via environment variables (useful for testing or non-standard layouts):

| Variable | Default |
|---|---|
| `JERU_PROJECTS_DIR` | `~/project` |
| `JERU_KNOWLEDGE_DIR` | `~/knowledge` |
| `JERU_CACHE_DIR` | `<os-cache>/jeru` |

## Prerequisites

| Tool | Required for |
|---|---|
| [`claude`](https://claude.ai/code) | `jeru claude` and `jeru session` commands |
| [`code`](https://code.visualstudio.com) | `jeru project compile` (and opening VSCode URLs) |
| [`tmux`](https://github.com/tmux/tmux) | `jeru session` (local and remote) |
| [`mutagen`](https://mutagen.io) | `jeru session … --remote` |
| [VSCode Remote SSH extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-ssh) | opening the VSCode URL of a remote session |

## Shell completions

Generate and install a completion script with:

```fish
# Fish
jeru completions fish > ~/.config/fish/completions/jeru.fish
```

```bash
# Bash
jeru completions bash > ~/.local/share/bash-completion/completions/jeru
```

```zsh
# Zsh
jeru completions zsh > "${fpath[1]}/_jeru"
```

`powershell` and `elvish` are also supported.

## Building

```
cargo build --release
```
