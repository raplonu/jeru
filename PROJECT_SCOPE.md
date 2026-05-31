# Project Scope: `jeru` — A Rust TUI Project Manager

## Purpose

A terminal UI tool (Rust) for managing a personal **project tree**. Each project bundles together the resources it needs — knowledge sets, code repos, and other folders — so they can be opened together in Claude Code and VSCode with minimal friction. The tool generates editor/AI launch configs from a single source of truth per project.

## Core Concepts

### Project
A logical unit defined by a manifest (`project.yml`) living in its own directory under a project tree (e.g. `projects/<name>/`). A project may be a coding project (has repos) or not (knowledge/docs or ressources/ only).

### Knowledge Set
A reusable, self-contained body of reference material (Markdown + YAML frontmatter), stored centrally in one place (`knowledge/<id>/`, with an `index.md` entry point). Knowledge is the **single source of truth** — projects only *reference* sets by ID, never copy them. The relationship is **N:N**: many projects can link the same set, one project can link many sets. Knowledge is synced across machines via **Syncthing** (so the tool does not handle sync, but should support checking/reverting changes — git underneath).

### Resources
Any other folder a project needs (specs, docs, design files, etc.).

## Project Manifest (`project.yml`)

Lives committed inside each project directory. Enumerates everything the project links:

```yaml
name: service-a
primary_repo: ~/code/service-a   # optional; default working repo for sustained code work
knowledge_sets:
  - nix-flakes
  - rust-async
repos:
  - ~/code/service-a
  - ~/code/service-a-proto
resources:
  - ~/docs/service-a-specs
```

Paths kept as-written (absolute or `~`). Knowledge base path configurable (resolves IDs to `~/knowledge/<id>`).

## What the Tool Generates

All generated files live **committed inside each project dir** (decided).

1. **VSCode workspace** (`.code-workspace`) — JSON listing all folders (repos + knowledge sets + resources) with friendly names/emoji prefixes.
2. **Claude Code config** (`.claude/settings.json`) — populates `additionalDirectories` with all linked folders so Claude Code has access on launch.
3. **Project `CLAUDE.md`** — a thin briefing doc (see below).
4. **Repo `CLAUDE.md` starter template** — optional skeleton (build/test/standards sections) for consistency across new repos.

## Working Directory Behavior (decided)

For launching Claude Code, **cwd = the project directory itself** (not a repo). Rationale:
- The generated `.claude/settings.json` and the hand-written project `CLAUDE.md` live in a neutral, committed location without polluting any code repo.
- All repos and knowledge sets are equal `additionalDirectories`.
- `primary_repo` in the manifest tells Claude where to `cd` for sustained code work.

## CLAUDE.md Strategy (decided)

- **Project `CLAUDE.md`** (thin): lists linked repos with a one-line summary each (the "index"/router), lists knowledge sets, and contains an explicit **lazy-load directive**: "Do NOT read repo CLAUDE.md files up front. When about to read/edit/run against files in a repo, FIRST read that repo's CLAUDE.md; load only for repos you actually touch."
- **Repo `CLAUDE.md`** (substantive): build commands, test commands, coding standards, architecture — relevant only when working inside that repo. Kept self-contained so late loading is cheap.
- Rationale: keep Claude's working context lean; most sessions touch only one repo. The one-line summaries let Claude route to the right repo doc without loading any of them. (Note: this is instruction-following, reinforced by Claude Code's natural nested-`CLAUDE.md` discovery as it works inside a repo tree — reliable but not a hard hook.)

## CLI / TUI Surface (intended)

Command verbs sketched so far (names provisional):
- `proj code <name>` — generate + open the `.code-workspace` in VSCode.
- `proj claude <name>` — cd to project dir, launch `claude` with all linked folders as `additionalDirectories` (via settings and/or `--add-dir`).
- `proj sync <name>` — regenerate `.claude/settings.json` + workspace so editor and Claude Code stay aligned with the manifest.
- (TUI) browse the project tree, inspect a project's linked resources, trigger the above actions interactively.

## Tech / Environment Notes

- **Language:** Rust. TUI (candidate crates: `ratatui` + `crossterm`; `clap` for the CLI verbs; `serde` + `serde_yaml` for manifests; `serde_json` for generated configs).
- **User environment:** Nix/NixOS multi-node setup (laptop, NixOS server, macOS node), Home Manager flakes, Fish shell, Ghostty terminal. Cross-platform path handling matters (avoid symlink-based linking — fragile across macOS/NixOS).
- Knowledge sync handled externally by Syncthing; tool should still help check/revert changes (git).

## Out of Scope (for the initial simple version)

- Sync itself (Syncthing handles it).
- Git submodule/subtree versioning of knowledge (possible later; manifest could pin a `ref:` per knowledge set).
- Independent per-knowledge-set repos (monorepo + reference approach chosen for now).

## Design Principles

- Single source of truth: the manifest drives all generated artifacts; knowledge lives in one place and is referenced, never duplicated.
- Human-readable **and** AI-friendly: plain Markdown/YAML, diffable, with predictable entry points (`index.md`, `CLAUDE.md`).
- Lean context for Claude: load only what the current task needs.