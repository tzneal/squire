//! Atomic rebase scope: wrap a multi-step rebase operation so that on
//! failure the repository is left exactly as it was before the operation
//! started. If the caller opts in to `PauseOnConflict` mode, conflicts
//! leave the rebase paused instead of rolling back so the user can
//! resolve by hand.
//!
//! This module centralizes HEAD snapshotting, stash management, rebase
//! abort, and the conflict-error formatting that used to live ad-hoc in
//! each rebase-based command (`amend --commit`, `drop`, `reword`,
//! `squash`, `split`).

use crate::Output;
use crate::git;
use crate::rebase;
use crate::response;
use std::cell::Cell;
use std::path::Path;

/// How a rebase failure should be handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicMode {
    /// On any error (including conflicts), abort the rebase and reset
    /// HEAD to its pre-scope state. Default.
    Atomic,
    /// On a rebase conflict, leave the rebase paused so the user can
    /// resolve by hand. Non-conflict errors still trigger a full
    /// rollback because the user did not opt in to dealing with those
    /// by hand.
    PauseOnConflict,
}

/// Context passed to the scope body. Exposes a helper for stashing
/// dirty unstaged state such that the scope knows to restore it on
/// success or failure. A future addition may expose the pre-scope HEAD
/// sha for callers that need it.
pub struct AtomicCtx<'a> {
    dir: &'a Path,
    /// Set by the body via `stash_unstaged_if_dirty` to inform the scope
    /// that a stash was pushed and must be popped on exit (success) or
    /// after rollback (failure).
    stash_pushed: Cell<bool>,
}

impl<'a> AtomicCtx<'a> {
    /// If the working tree currently has unstaged changes, `git stash
    /// push -u` them and record that fact. The scope will restore the
    /// stash on both success and rollback.
    ///
    /// Why not stash on entry? Because some commands (notably
    /// `amend --commit`) need to commit the already-staged index as a
    /// fixup *before* the remaining unstaged state is stashed — stashing
    /// first would wipe the index they need. Letting the body call this
    /// at the right point preserves that ordering.
    pub fn stash_unstaged_if_dirty(&self) -> Result<(), String> {
        if !git::is_clean(self.dir)? {
            git::stash_push(self.dir, None)?;
            self.stash_pushed.set(true);
        }
        Ok(())
    }
}

/// Run `body` under an atomic rebase scope.
///
/// Before calling `body`:
///   1. Snapshot HEAD.
///
/// After `body` returns:
///   - `Ok`: pop the stash if the body pushed one, and return Ok.
///   - `Err(e)` with a rebase conflict and `mode == PauseOnConflict`:
///     leave the rebase paused; return a formatted conflict error with
///     `rolled_back: false`. The stash (if any) stays pushed — popping
///     it into a paused rebase would be chaos. The user gets told.
///   - `Err(e)` otherwise: abort any in-progress rebase, reset HEAD to
///     the snapshot, pop the stash (best-effort). Return a formatted
///     error. If the original error was a rebase conflict, the error
///     has `rolled_back: true` and includes a hint for manual recovery.
///
/// `command_name` is used in error prose (e.g. "Conflict during amend").
pub fn run_atomic<F>(
    dir: &Path,
    mode: AtomicMode,
    command_name: &str,
    json: bool,
    body: F,
) -> Result<(), String>
where
    F: FnOnce(&AtomicCtx) -> Result<(), String>,
{
    let head_sha = git::rev_parse(dir, "HEAD")?;
    let ctx = AtomicCtx {
        dir,
        stash_pushed: Cell::new(false),
    };
    match body(&ctx) {
        Ok(()) => {
            if ctx.stash_pushed.get() {
                git::stash_pop(dir)?;
            }
            Ok(())
        }
        Err(e) => Err(handle_failure(
            dir,
            mode,
            command_name,
            json,
            &head_sha,
            ctx.stash_pushed.get(),
            e,
        )),
    }
}

/// Shared failure path. Decides between rollback and pause based on mode
/// and whether the error is a rebase conflict, performs the necessary
/// git operations, and returns a formatted error string.
fn handle_failure(
    dir: &Path,
    mode: AtomicMode,
    command_name: &str,
    json: bool,
    head_snapshot: &str,
    stash_pushed: bool,
    raw_err: String,
) -> String {
    // Capture conflict state *before* we touch the rebase. `rebase_abort`
    // deletes `.git/rebase-merge/*`, which is the state we read to build
    // the conflict error.
    let in_progress = matches!(git::rebase_in_progress(dir), Ok(true));
    let files = git::conflicting_files(dir).unwrap_or_default();
    let is_conflict = in_progress && !files.is_empty();
    let current_commit = if is_conflict {
        git::rebase_current_commit(dir)
    } else {
        None
    };
    let onto = if is_conflict {
        git::rebase_onto(dir)
    } else {
        None
    };

    // Decide whether we're rolling back or leaving paused.
    let rollback = !matches!((mode, is_conflict), (AtomicMode::PauseOnConflict, true));

    if rollback {
        // Order matters: abort any in-progress rebase first (otherwise
        // reset --hard refuses while in a rebase), then reset HEAD to the
        // snapshot, then pop the stash.
        if in_progress {
            let _ = git::rebase_abort(dir);
        }
        // Reset HEAD unconditionally: the scope owns every mutation that
        // happened between `begin` and `fail`, so the snapshot is the
        // source of truth. This kills any fixup commit, partial amend, or
        // intermediate state the body may have created.
        let _ = git::reset_hard(dir, head_snapshot);
        if stash_pushed {
            let _ = git::stash_pop(dir);
        }
    }
    // In the PauseOnConflict-conflict case we do nothing: the rebase
    // stays paused, the stash stays pushed (we'll tell the user about it
    // in the hint), and the user takes over from here.

    if !is_conflict {
        // Not a rebase conflict — no structured error to build, just
        // return the raw error (already rolled back above).
        return raw_err;
    }

    format_conflict_error(
        command_name,
        json,
        rollback,
        &files,
        current_commit.as_ref(),
        onto.as_deref(),
        stash_pushed,
    )
}

/// Build the structured or prose conflict error that commands return to
/// the user. Covers both the rolled-back and paused cases.
fn format_conflict_error(
    command_name: &str,
    json: bool,
    rolled_back: bool,
    files: &[(String, String)],
    current_commit: Option<&(String, String)>,
    onto: Option<&str>,
    stash_pushed: bool,
) -> String {
    let hint = conflict_hint(command_name, rolled_back, stash_pushed);

    if json {
        let result = response::ConflictError {
            conflict: true,
            rolled_back,
            conflicting_files: rebase::build_conflict_files(files),
            hint,
            current_commit: current_commit.map(|(sha, msg)| response::CommitRef {
                sha: sha.clone(),
                message: msg.clone(),
                upstream_match: None,
            }),
            ours_theirs: onto.map(|o| response::OursTheirs {
                ours: format!("upstream ({o})"),
                theirs: "your commit being replayed".to_string(),
            }),
        };
        return serde_json::to_string(&result).unwrap();
    }

    let mut out = Output::default();
    if let Some((sha, subject)) = current_commit {
        out.println(&format!("Replaying: {sha:.8} {subject}"));
    }
    let header = if rolled_back {
        format!("Conflict during {command_name} (rolled back, history unchanged):")
    } else {
        format!("Conflict during {command_name} (rebase paused, resolve by hand):")
    };
    out.println(&header);
    rebase::format_conflict_files(&mut out, files);
    if let Some(o) = onto {
        out.println(&format!(
            "Note: \"ours\" = upstream ({o}), \"theirs\" = your commit"
        ));
    }
    out.println(&hint);
    out.stdout.trim_end().to_string()
}

/// Produce the per-mode recovery hint. For rolled-back conflicts we tell
/// the LLM how to either retry with --pause-on-conflict or do a manual
/// rebase. For paused conflicts we give the standard continue/abort
/// commands.
fn conflict_hint(command_name: &str, rolled_back: bool, stash_pushed: bool) -> String {
    if rolled_back {
        format!(
            "{command_name} could not complete cleanly and was rolled back; \
             the working tree and history are unchanged. To retry and \
             resolve by hand, re-run with `--pause-on-conflict` to leave \
             the rebase paused on the conflicting commit. Alternative: \
             perform a manual rebase (`git rebase -i <target>~1`, mark \
             the target as `edit`, apply your changes, resolve conflicts, \
             `git add`, `GIT_EDITOR=true git rebase --continue`)."
        )
    } else {
        let mut hint = String::from(
            "Resolve conflicts, stage with `git add`, then run \
             `GIT_EDITOR=true git rebase --continue`. To cancel and \
             restore the pre-command state: `git rebase --abort` \
             followed by `git reset --hard <pre-command HEAD>`.",
        );
        if stash_pushed {
            hint.push_str(
                " Note: your dirty working tree was stashed before the \
                 rebase — after the rebase completes or is aborted, run \
                 `git stash pop` to restore it.",
            );
        }
        hint
    }
}
