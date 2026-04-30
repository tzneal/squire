//! Atomic scope: wrap a multi-step operation so that on failure the
//! repository is left EXACTLY as it was before the operation started —
//! including HEAD, the index, the working tree, and untracked files.
//!
//! The central guarantee is the no-data-loss invariant:
//!
//!   > Anything the user did not explicitly ask squire to mutate must
//!   > be byte-for-byte identical to pre-command state.
//!
//! To uphold that guarantee, every mutating command runs under
//! [`run_atomic`], which captures a full pre-command snapshot before
//! the body runs and restores it on any error path. If the caller opts
//! in to `PauseOnConflict` mode, rebase conflicts leave the rebase
//! paused instead of rolling back so the user can resolve by hand.
//!
//! This module centralizes HEAD/index/worktree/untracked snapshotting,
//! rebase abort, and the conflict-error formatting that used to live
//! ad-hoc in each rebase-based command (`amend --commit`, `drop`,
//! `reword`, `squash`, `split`).

use crate::Output;
use crate::git;
use crate::rebase;
use crate::response;
use std::path::Path;

/// How a rebase failure should be handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicMode {
    /// On any error (including conflicts), abort the rebase and restore
    /// the pre-scope snapshot. Default.
    Atomic,
    /// On a rebase conflict, leave the rebase paused so the user can
    /// resolve by hand. Non-conflict errors still trigger a full
    /// rollback because the user did not opt in to dealing with those
    /// by hand.
    PauseOnConflict,
}

/// Full pre-command state snapshot. Captures:
///   - HEAD sha
///   - Cached portion as a patch (diff --cached)
///   - Unstaged portion as a patch (diff, not including untracked)
///   - Untracked files (path + contents)
///   - A stash commit sha holding the whole tracked state, for
///     failure-path rollback via `reset --hard HEAD && stash apply`
///
/// The patch-based fields power the success path (reset-to-new-HEAD
/// then explicitly re-apply cached/unstaged/untracked, skipping
/// content now in HEAD). The stash sha powers the failure rollback
/// path (restore exact pre-command state onto pre-command HEAD).
#[derive(Debug)]
pub struct StateSnapshot {
    head: String,
    stash_sha: Option<String>,
    cached_patch: String,
    unstaged_patch: String,
    untracked: Vec<(String, Vec<u8>)>,
}

impl StateSnapshot {
    /// Capture the current repo state.
    pub fn capture(dir: &Path) -> Result<Self, String> {
        let head = git::rev_parse(dir, "HEAD")?;
        let stash_sha = git::capture_snapshot(dir)?;
        let cached_patch = git::diff(dir, &["--cached".to_string()])?;
        let unstaged_patch = git::diff(dir, &[])?;
        let untracked = git::list_untracked_with_contents(dir)?;
        Ok(Self {
            head,
            stash_sha,
            cached_patch,
            unstaged_patch,
            untracked,
        })
    }

    /// Restore the captured state onto the original HEAD. Used on
    /// rollback.
    pub fn restore(&self, dir: &Path) -> Result<(), String> {
        git::restore_snapshot(dir, &self.head, self.stash_sha.as_deref(), &self.untracked)
    }

    /// Apply the user's non-targeted state on top of the current HEAD
    /// (assumed to be fresh from the command body's new commit). Parts
    /// of the snapshot that match what the body put into HEAD are
    /// skipped:
    ///   - Patch hunks referencing files absent from the new HEAD are
    ///     dropped (because the body's mutation has already consumed
    ///     them, typically via delete or rename).
    ///   - Patch hunks already present in the new HEAD are dropped via
    ///     `git apply --3way`.
    ///   - Untracked files now tracked in HEAD at the same path are
    ///     skipped by `restore_untracked`.
    ///
    /// Precondition: working tree matches current HEAD exactly.
    pub fn restore_onto_new_head(&self, dir: &Path) -> Result<(), String> {
        // Filter each patch to drop hunks whose old-file or target
        // file is no longer present in the new HEAD. Those represent
        // changes the body has already absorbed (e.g. a rename that
        // deleted old.txt and added new.txt as a tracked file).
        let cached = filter_patch_for_existing_files(dir, &self.cached_patch)?;
        let unstaged = filter_patch_for_existing_files(dir, &self.unstaged_patch)?;
        // Apply unstaged patches first via --3way (needed because
        // context is relative to old HEAD). --3way stages the result,
        // so we reset the index afterward to keep them unstaged.
        if !unstaged.trim().is_empty() {
            git::apply_patch_tolerant(dir, &unstaged, &[])?;
            git::reset_mixed_head(dir)?;
        }
        // Now apply cached patches to both index and worktree.
        if !cached.trim().is_empty() {
            git::apply_patch_tolerant(dir, &cached, &["--index"])?;
        }
        git::restore_untracked(dir, &self.untracked)?;
        Ok(())
    }
}

/// Context passed to the scope body. Previously held stash state; now
/// a marker type kept for API compatibility. Future additions can expose
/// snapshot accessors for bodies that need them.
pub struct AtomicCtx;

/// Run `body` under an atomic scope.
///
/// Before calling `body`: capture a full [`StateSnapshot`].
///
/// After `body` returns:
///   - `Ok`: return Ok (snapshot is discarded; the body's changes are
///     committed).
///   - `Err(e)` with a rebase conflict and `mode == PauseOnConflict`:
///     leave the rebase paused; return a formatted conflict error with
///     `rolled_back: false`. The snapshot is not restored because the
///     user is taking over.
///   - `Err(e)` otherwise: abort any in-progress rebase, restore the
///     snapshot (HEAD + index + worktree + untracked), and return a
///     formatted error. If the original error was a rebase conflict,
///     the error has `rolled_back: true`.
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
    let snapshot = StateSnapshot::capture(dir)?;
    let ctx = AtomicCtx;
    match body(&ctx) {
        Ok(()) => Ok(()),
        Err(e) => Err(handle_failure(dir, mode, command_name, json, &snapshot, e)),
    }
}

/// Run a state-isolating operation: a command whose effect should be
/// scoped to exactly the mutations performed by `body`, preserving all
/// other user state (staged content, unstaged edits, untracked files).
///
/// Use this for:
///   - HEAD-amend commands (`amend HEAD`, `drop HEAD`, `reword HEAD`)
///     so that pre-existing staged content is not silently folded into
///     the amended commit.
///   - Non-HEAD rebase commands (`amend --commit`, `drop`, `reword`,
///     `squash`, `split`) so that dirty user state is preserved across
///     the rebase without requiring a clean-tree precondition.
///
/// Algorithm:
///   1. Capture pre-command snapshot S (for both rollback and success
///      restoration).
///   2. Reset HEAD (hard) + clean -fd. Working tree now matches HEAD
///      exactly; user state is temporarily held in S.
///   3. Call `body`. The body is expected to produce a new HEAD (via
///      `commit --amend`, fixup + autosquash, rebase, etc.).
///   4. `git reset --hard HEAD` + `git clean -fd`. Working tree now
///      matches the NEW HEAD. This clears any untracked files the body
///      may have left behind.
///   5. Apply S on top of new HEAD. This performs a three-way merge
///      that re-creates the user's pre-command state on top of the new
///      history. Mutations that went into HEAD become no-ops in the
///      resulting diff.
///   6. On any failure in 2–5: restore S to the original HEAD.
///
/// The body MUST NOT rely on pre-existing index or working tree state,
/// because step 2 has cleared it. Bodies that need to apply specific
/// hunks must capture them (via [`collect_named_hunks`]) BEFORE calling
/// this function, then apply them inside the body.
pub fn run_isolated<F>(
    dir: &Path,
    mode: AtomicMode,
    command_name: &str,
    json: bool,
    body: F,
) -> Result<(), String>
where
    F: FnOnce(&AtomicCtx) -> Result<(), String>,
{
    let snapshot = StateSnapshot::capture(dir)?;
    let ctx = AtomicCtx;

    // Wrap the whole operation so any failure triggers rollback.
    let result = (|| -> Result<(), String> {
        // Step 2: reset clean to HEAD.
        git::reset_hard(dir, &snapshot.head)?;
        git::clean_fd(dir)?;
        // Step 3: body applies named hunks and produces new HEAD.
        body(&ctx)?;
        // Step 4: reset clean to new HEAD. This clears any untracked
        // files the body may have left behind.
        git::reset_hard(dir, "HEAD")?;
        git::clean_fd(dir)?;
        // Step 5: restore user's non-targeted state on top of new HEAD,
        // using patch-based application that tolerates hunks already
        // present (three-way merge). This preserves staged content,
        // unstaged edits, and untracked files that weren't consumed
        // by the body.
        snapshot.restore_onto_new_head(dir)?;
        Ok(())
    })();

    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(handle_failure(dir, mode, command_name, json, &snapshot, e)),
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
    snapshot: &StateSnapshot,
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
        // reset --hard refuses while in a rebase), then restore the
        // full pre-command snapshot.
        if in_progress {
            let _ = git::rebase_abort(dir);
        }
        // Restore unconditionally: the scope owns every mutation that
        // happened between capture and failure, so the snapshot is the
        // source of truth. This restores HEAD + index + worktree +
        // untracked files exactly.
        let _ = snapshot.restore(dir);
    }
    // In the PauseOnConflict-conflict case we do nothing: the rebase
    // stays paused, and the user takes over from here.

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
) -> String {
    let hint = conflict_hint(command_name, rolled_back);

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
fn conflict_hint(command_name: &str, rolled_back: bool) -> String {
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
        String::from(
            "Resolve conflicts, stage with `git add`, then run \
             `GIT_EDITOR=true git rebase --continue`. To cancel and \
             restore the pre-command state: `git rebase --abort` \
             followed by `git reset --hard <pre-command HEAD>`.",
        )
    }
}

/// Filter a unified-diff patch to drop entire file-pair sections where
/// the old file (the `a/...` side) is not present in the current index.
/// This handles the case where the body's mutation consumed a file
/// (e.g. a rename that deleted the old path) — the snapshot's patch
/// still references the old path, but applying it would fail because
/// the file no longer exists.
///
/// Works on raw diff text (preserving `index` lines needed by --3way)
/// by splitting on `diff --git` boundaries and checking each section's
/// `--- a/<path>` line.
fn filter_patch_for_existing_files(dir: &std::path::Path, patch: &str) -> Result<String, String> {
    if patch.trim().is_empty() {
        return Ok(String::new());
    }
    let mut result = String::new();
    // Split on "diff --git " boundaries. Each section starts with
    // "diff --git a/... b/..." and includes all subsequent lines until
    // the next "diff --git" or end of string.
    let sections: Vec<&str> = {
        let mut starts = Vec::new();
        for (i, _) in patch.match_indices("\ndiff --git ") {
            starts.push(i + 1); // skip the leading \n
        }
        if patch.starts_with("diff --git ") {
            starts.insert(0, 0);
        }
        let mut secs = Vec::new();
        for (i, &start) in starts.iter().enumerate() {
            let end = starts.get(i + 1).copied().unwrap_or(patch.len());
            secs.push(&patch[start..end]);
        }
        secs
    };
    for section in sections {
        // Find the "--- a/<path>" line to determine the old file.
        let old_file = section
            .lines()
            .find(|l| l.starts_with("--- "))
            .and_then(|l| {
                l.strip_prefix("--- a/").or_else(|| {
                    if l == "--- /dev/null" {
                        Some("/dev/null")
                    } else {
                        None
                    }
                })
            });
        match old_file {
            Some("/dev/null") => {
                // Pure addition — always keep.
                result.push_str(section);
            }
            Some(path) => {
                if git::file_in_index(dir, path) {
                    result.push_str(section);
                }
                // else: file consumed by body, drop section
            }
            None => {
                // Can't determine old file — keep to be safe.
                result.push_str(section);
            }
        }
    }
    Ok(result)
}
