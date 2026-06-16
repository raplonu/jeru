//! Interactive resolution of remote-sync directory conflicts.
//!
//! Used by [`super::start`] when a remote sync directory is non-empty and
//! [`crate::remote::DirDiff::is_safe`] is false: the user is shown what
//! differs and chooses, per folder, whether to override (wipe the remote
//! copy), continue (let mutagen's two-way-resolved sync reconcile it), or
//! abort the whole `session up`.

use std::path::{Path, PathBuf};

use dialoguer::{Select, theme::ColorfulTheme};

use crate::error::Result;
use crate::remote::{DirDiff, remote_rsync_preview};

/// How to handle one conflicting remote directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resolution {
    /// Wipe the remote directory; local content wins.
    Override,
    /// Leave the remote directory as-is; let mutagen's `two-way-resolved`
    /// sync mode reconcile the differences.
    Continue,
}

/// Outcome of the per-folder submenu.
enum SubmenuOutcome {
    Resolved(Resolution),
    Back,
    Abort,
}

/// Interactively resolve a set of conflicting sync-pair directories.
///
/// `conflicts` is `(remote_path, local_path, DirDiff)` for each folder with
/// issues; `host` is the SSH host used to render diffs. Returns
/// `Some(resolutions)` (one per input, same order) once every folder has been
/// resolved to [`Resolution::Override`] or [`Resolution::Continue`], or `None`
/// if the user chooses to abort.
pub(crate) fn resolve(
    host: &str,
    conflicts: &[(String, PathBuf, DirDiff)],
) -> Result<Option<Vec<Resolution>>> {
    let mut pending: Vec<Option<Resolution>> = vec![None; conflicts.len()];

    println!("\nThe following remote directories have unresolved differences:");
    for (path, _, diff) in conflicts {
        println!(
            "  - {path}  ({} only on remote, {} differing)",
            diff.remote_only.len(),
            diff.differing.len()
        );
    }
    println!();

    loop {
        if pending.iter().all(Option::is_some) {
            return Ok(Some(pending.into_iter().map(Option::unwrap).collect()));
        }

        let mut items: Vec<String> = conflicts
            .iter()
            .zip(&pending)
            .map(|((path, _, _), res)| {
                let tag = match res {
                    Some(Resolution::Override) => "[override]",
                    Some(Resolution::Continue) => "[continue]",
                    None => "[pending]",
                };
                format!("{tag:<11} {path}")
            })
            .collect();
        items.push("Override all remaining".to_string());
        items.push("Continue all remaining".to_string());
        items.push("Abort".to_string());

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Remote sync conflicts — pick a folder to inspect, or an action")
            .items(&items)
            .default(0)
            .interact()
            .map_err(std::io::Error::other)?;

        let n = conflicts.len();
        if selection < n {
            let (remote_path, local, _) = &conflicts[selection];
            match resolve_one(host, local, remote_path)? {
                SubmenuOutcome::Resolved(r) => pending[selection] = Some(r),
                SubmenuOutcome::Back => {}
                SubmenuOutcome::Abort => return Ok(None),
            }
        } else if selection == n {
            for p in pending.iter_mut().filter(|p| p.is_none()) {
                *p = Some(Resolution::Override);
            }
        } else if selection == n + 1 {
            for p in pending.iter_mut().filter(|p| p.is_none()) {
                *p = Some(Resolution::Continue);
            }
        } else {
            return Ok(None);
        }
    }
}

/// Submenu for a single conflicting folder. Returns `SubmenuOutcome::Back` to
/// return to the top-level menu without resolving it.
fn resolve_one(host: &str, local: &Path, remote_path: &str) -> Result<SubmenuOutcome> {
    loop {
        let items = [
            "View diff (rsync dry-run)",
            "Override (wipe remote, local wins)",
            "Continue (let mutagen resolve)",
            "Back",
            "Abort",
        ];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Folder: {remote_path}"))
            .items(&items)
            .default(0)
            .interact()
            .map_err(std::io::Error::other)?;

        match selection {
            0 => view_diff(host, local, remote_path),
            1 => return Ok(SubmenuOutcome::Resolved(Resolution::Override)),
            2 => return Ok(SubmenuOutcome::Resolved(Resolution::Continue)),
            3 => return Ok(SubmenuOutcome::Back),
            _ => return Ok(SubmenuOutcome::Abort),
        }
    }
}

/// Show the diverging files for a folder via an `rsync` dry-run (local vs.
/// remote). A failure here is non-fatal — it just means no preview is shown, so
/// the user can still choose Override/Continue/Abort.
fn view_diff(host: &str, local: &Path, remote_path: &str) {
    println!("\n  rsync dry-run ('>'/'c' = local-only or changed, '*deleting' = remote-only):");
    if let Err(e) = remote_rsync_preview(host, local, remote_path) {
        eprintln!("  rsync preview failed: {e}");
    }
    println!();
}
