//! Interactive resolution of remote-sync directory conflicts.
//!
//! Used by [`super::start`] when a remote sync directory is non-empty and
//! [`crate::remote::DirDiff::is_safe`] is false: the user is shown what
//! differs and chooses, per folder, whether to override (wipe the remote
//! copy), continue (let mutagen's two-way-resolved sync reconcile it), or
//! abort the whole `session up`.

use dialoguer::{Select, theme::ColorfulTheme};

use crate::error::Result;
use crate::remote::DirDiff;

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
/// `conflicts` is `(remote_path, DirDiff)` for each folder with issues.
/// Returns `Some(resolutions)` (one per input, same order) once every folder
/// has been resolved to [`Resolution::Override`] or [`Resolution::Continue`],
/// or `None` if the user chooses to abort.
pub(crate) fn resolve(conflicts: &[(String, DirDiff)]) -> Result<Option<Vec<Resolution>>> {
    let mut pending: Vec<Option<Resolution>> = vec![None; conflicts.len()];

    println!("\nThe following remote directories have unresolved differences:");
    for (path, diff) in conflicts {
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
            .map(|((path, _), res)| {
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
            match resolve_one(&conflicts[selection].0, &conflicts[selection].1)? {
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
fn resolve_one(remote_path: &str, diff: &DirDiff) -> Result<SubmenuOutcome> {
    loop {
        let items = [
            "View conflicting files",
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
            0 => print_diff(diff),
            1 => return Ok(SubmenuOutcome::Resolved(Resolution::Override)),
            2 => return Ok(SubmenuOutcome::Resolved(Resolution::Continue)),
            3 => return Ok(SubmenuOutcome::Back),
            _ => return Ok(SubmenuOutcome::Abort),
        }
    }
}

/// Print the files responsible for a folder's conflict status.
///
/// `local_only` is intentionally not shown: folders whose only difference is
/// local-only files are "safe" and never reach this menu.
fn print_diff(diff: &DirDiff) {
    const MAX_SHOWN: usize = 20;

    if !diff.remote_only.is_empty() {
        println!("  Only on remote ({}):", diff.remote_only.len());
        for f in diff.remote_only.iter().take(MAX_SHOWN) {
            println!("    {f}");
        }
        if diff.remote_only.len() > MAX_SHOWN {
            println!("    ... and {} more", diff.remote_only.len() - MAX_SHOWN);
        }
    }
    if !diff.differing.is_empty() {
        println!("  Different size locally vs remote ({}):", diff.differing.len());
        for f in diff.differing.iter().take(MAX_SHOWN) {
            println!("    {f}");
        }
        if diff.differing.len() > MAX_SHOWN {
            println!("    ... and {} more", diff.differing.len() - MAX_SHOWN);
        }
    }
    println!();
}
