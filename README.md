# jeru - A fully **vibe coded** rust project manager

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

### Navigation

```
jeru ls                    List all projects
jeru use <name>            Set the current project (persisted across commands)
jeru info [name]           Show the manifest for a project
```

Most commands accept an optional project name; when omitted they fall back to the current project set by `jeru use`.

### Creating a project

```
jeru create <name> [--active] [--force]
```

Creates a new project directory under `~/project/<name>/` with a starter `project.yml`. Pass `--active` to immediately set it as the current project, and `--force` to proceed even if the directory already exists and is non-empty.

### Editing files

```
jeru edit [<filename>] [-p <name>] [--list-alias]
```

Without a filename, opens the project folder in VSCode. With a filename, opens that file in `$EDITOR`. Filenames can be plain (relative to the project directory) or one of the built-in aliases:

| Alias | File |
|---|---|
| `@project` | `project.yml` |
| `@readme` | `README.md` |
| `@roadmap` | `ROADMAP.md` |

Run `jeru edit --list-alias` to print the full alias table.

### Adding entries

```
jeru add <path> [--kind repo|knowledge|resource] [--project <name>]
```

Adds a repo, knowledge set, or resource to the project manifest. When `--kind` is omitted the kind is deduced from the path:

- Under `~/knowledge/` → knowledge set (ID extracted automatically)
- Existing directory → repo
- File or path with extension → resource

If the kind is deduced, jeru shows an interactive prompt so you can confirm or change it.

### Removing entries

```
jeru remove <path> [--kind repo|knowledge|resource] [--project <name>]
```

Removes a repo, knowledge set, or resource from the manifest. Kind detection and the interactive confirmation prompt work the same way as `jeru add`.

### Listing entries

```
jeru list [<name>] [--kind repo|knowledge|resource]
```

Lists all repos, knowledge sets, and resources in a project. Pass `--kind` to filter by type.

### Sessions

A **session** is a background activity: `claude remote-control` running in a
detached tmux session, controlled from claude.ai/code or the Claude mobile app.
`jeru session start` returns immediately (it does not take over your terminal),
prints the claude output (including the remote-control URL) and a VSCode URL, and
the session keeps running until you `jeru session stop` it.

```
jeru session start [name] [--spawn same-dir|worktree|session] [--repos]
jeru session ls
jeru session stop   [session-id]
jeru session inspect [session-id]
```

- A session's **id** is the project name (`myproj`) for a local session, or
  `project@host` for a remote one. `stop`/`inspect` take that id (defaulting to
  the current project), and `ls` prints it.
- `--spawn` is forwarded to `claude remote-control --spawn` (default `same-dir`).
- `--repos` opens claude in the first repo, with the other repos added.
- `jeru session inspect` attaches to the session's tmux so you can watch claude;
  detaching leaves the session running.
- Obsidian: if MCP is enabled and its server isn't already up, jeru launches
  Obsidian normally and leaves it running (it is never stopped by jeru).

#### Remote sessions

```
jeru session start [name] --remote <host> [options]
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

`jeru session stop <id>` gracefully ends the remote claude, tears down both tmux
sessions, terminates mutagen, and removes the remote directories.

**Options (only valid with `--remote`):**

| Flag | Effect |
|---|---|
| `--repos` | Sync repos only; claude opens in the first repo |
| `--no-resources` | Do not sync resources |
| `--no-cleanup` | Keep the remote directories when the session is stopped |
| `--override-remote` | Delete pre-existing non-empty remote directories at startup |

### Compiling a project

```
jeru compile [name]
```

Regenerates all derived files from the manifest in one step:

- `CLAUDE.md` — project briefing for Claude Code, listing all linked repos, knowledge sets, and resources
- `.claude/settings.json` — sets `additionalDirectories` so Claude Code can read every linked folder
- `<name>.code-workspace` — VSCode workspace with all repos as folders (skipped if the project has no repos)

Run `jeru compile` whenever you change `project.yml`.

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
│   │   ├── CLAUDE.md                  (generated by jeru compile)
│   │   ├── ROADMAP.md                 (default roadmap location)
│   │   ├── service-a.code-workspace   (generated by jeru compile)
│   │   └── .claude/
│   │       └── settings.json          (generated by jeru compile)
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
| [`claude`](https://claude.ai/code) | All `claude` and `work` commands |
| [`code`](https://code.visualstudio.com) | `compile` and `work` commands |
| [VSCode Remote SSH extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-ssh) | `work --remote` |
| [`mutagen`](https://mutagen.io) | `work --remote` |
| [`tmux`](https://github.com/tmux/tmux) | `work --remote` |

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
