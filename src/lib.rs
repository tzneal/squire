pub mod cleanup;
pub mod cli;
pub mod diff;
pub mod git;
pub mod output;
pub mod rebase;
pub mod resolve;
pub mod response;

use cli::{Cli, Command};
use resolve::{
    apply_resolved, check_rebase_conflict, find_hunk, residual_hunks, resolve_hunks,
    resolve_selector, split_last_arg, stage_hunks, stage_hunks_or_cached,
};
use std::fmt::Write;
use std::path::Path;

/// Collected output from a squire invocation.
#[derive(Default)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn println(&mut self, s: &str) {
        writeln!(self.stdout, "{s}").unwrap();
    }

    pub fn eprintln(&mut self, s: &str) {
        writeln!(self.stderr, "{s}").unwrap();
    }
}

pub fn short_sha(sha: &str) -> &str {
    &sha[..8.min(sha.len())]
}

fn find_todo_line(lines: &[String], sha_prefix: &str) -> Result<usize, String> {
    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() >= 2
                && !parts[0].starts_with('#')
                && (parts[1].starts_with(sha_prefix) || sha_prefix.starts_with(parts[1]))
            {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    match matches.len() {
        0 => Err(format!("no todo line matches sha prefix: {sha_prefix}")),
        1 => Ok(matches[0]),
        _ => Err(format!(
            "ambiguous sha prefix {sha_prefix}: matches {} lines",
            matches.len()
        )),
    }
}

fn run_seqedit(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("seqedit requires at least one action and a todo file path".to_string());
    }
    let (actions, file) = args.split_at(args.len() - 1);
    let file = &file[0];

    let todo =
        std::fs::read_to_string(file).map_err(|e| format!("failed to read todo file: {e}"))?;
    let mut lines: Vec<String> = todo.lines().map(String::from).collect();

    for action_arg in actions {
        let (action, sha_part) = action_arg
            .split_once(':')
            .ok_or_else(|| format!("invalid action syntax: {action_arg} (expected action:sha)"))?;
        match action {
            "pick" | "reword" | "edit" | "squash" | "fixup" | "drop" => {}
            _ => {
                return Err(format!(
                    "unknown action: {action} (expected pick, reword, edit, squash, fixup, or drop)"
                ));
            }
        }
        // Support "action:sha>after_sha" to move the line after another.
        let (sha_prefix, move_after) = match sha_part.split_once('>') {
            Some((s, a)) => (s, Some(a)),
            None => (sha_part, None),
        };
        let idx = find_todo_line(&lines, sha_prefix)?;
        let old_action = lines[idx].split(' ').next().unwrap();
        lines[idx] = lines[idx].replacen(old_action, action, 1);
        if let Some(after_sha) = move_after {
            let line = lines.remove(idx);
            let after_idx = find_todo_line(&lines, after_sha)?;
            // Insert after the target and any fixup/squash lines already following it.
            let mut insert_at = after_idx + 1;
            while insert_at < lines.len() {
                let first_word = lines[insert_at].split(' ').next().unwrap_or("");
                if first_word == "fixup" || first_word == "squash" {
                    insert_at += 1;
                } else {
                    break;
                }
            }
            lines.insert(insert_at, line);
        }
    }

    let mut result = lines.join("\n");
    if todo.ends_with('\n') {
        result.push('\n');
    }
    std::fs::write(file, result).map_err(|e| format!("failed to write todo file: {e}"))?;
    Ok(())
}

fn run_stash(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    message: Option<&str>,
    hunk_ids: &[String],
) -> Result<(), String> {
    let (raw, _) = diff_with_untracked(dir, &[])?;
    let hunks = diff::parse_diff(&raw)?;
    let (selected, had_partial) = resolve_hunks(&hunks, hunk_ids, false)?;
    let selected_ids: std::collections::HashSet<&str> =
        selected.iter().map(|h| h.id.as_str()).collect();
    let affected_files: std::collections::HashSet<&str> =
        selected.iter().map(|h| h.file.as_str()).collect();
    let keep: Vec<&diff::HunkInfo> = hunks
        .iter()
        .filter(|h| !selected_ids.contains(h.id.as_str()))
        .collect();

    let keep_patch = if keep.is_empty() {
        None
    } else {
        Some(diff::reconstruct_patch(&keep))
    };

    if let Some(ref p) = keep_patch {
        git::apply_worktree(dir, p)?;
    }

    let stash_result = git::stash_push(dir, message);

    if let Some(ref p) = keep_patch {
        git::apply_worktree_forward(dir, p)?;
    }

    stash_result?;
    let new_hunks = residual_hunks(dir, had_partial, &affected_files, false)?;
    emit_result(
        out,
        cli.json,
        response::ActionCount::Stashed(selected.len()),
        &format!("Stashed {} hunk(s)", selected.len()),
        &new_hunks,
    );
    Ok(())
}

fn run_squash(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    message: Option<&str>,
    commits: &[String],
) -> Result<(), String> {
    if !git::is_clean(dir)? {
        return Err("squash requires a clean working tree".to_string());
    }
    let target = resolve_reachable_commit(dir, &commits[0])?;
    let sources: Vec<String> = commits[1..]
        .iter()
        .map(|c| resolve_reachable_commit(dir, c))
        .collect::<Result<_, _>>()?;
    git::rebase_squash(dir, &target, &sources, message)
        .map_err(|e| check_rebase_conflict(dir, e, cli.json))?;
    emit_result(
        out,
        cli.json,
        response::ActionCount::Squashed(sources.len()),
        &format!(
            "Squashed {} commit(s) into {}",
            sources.len(),
            short_sha(&target)
        ),
        &[],
    );
    Ok(())
}

fn run_log(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    n: usize,
    max_hunk_lines: usize,
) -> Result<(), String> {
    let raw = git::log(dir, n)?;
    let commits = diff::parse_log(&raw)?;
    if cli.json {
        // Only the JSON path emits raw hunk content; truncate it there to keep
        // `squire log --json -n 50` bounded for LLM context windows. Plain and
        // short already only emit summary lines, so they need no truncation.
        let mut commits = commits;
        if max_hunk_lines > 0 {
            for c in &mut commits {
                let sha = c.sha.clone();
                truncate_commit_hunks(&sha, &mut c.hunks, max_hunk_lines);
            }
        }
        let s = serde_json::to_string_pretty(&commits)
            .map_err(|e| format!("failed to serialize JSON: {e}"))?;
        out.println(&s);
    } else if cli.short {
        out.stdout.push_str(&output::format_log_short(&commits));
    } else {
        out.stdout.push_str(&output::format_log_plain(&commits));
    }
    Ok(())
}

/// Cap the total content lines across a commit's hunks. Once adding a hunk
/// would exceed the cap, that hunk (and all subsequent ones) keep their
/// summary fields (id, file, ranges, header) but their `content` is
/// replaced with a marker and `line_hashes` are cleared. Callers can
/// fetch the full body with `squire show <sha> <id>` — the commit SHA is
/// required because `squire show` without a ref only searches the working
/// tree, not commits.
fn truncate_commit_hunks(sha: &str, hunks: &mut [diff::HunkInfo], max_lines: usize) {
    let short = short_sha(sha);
    let mut used = 0usize;
    for h in hunks {
        let lines = h.content.lines().count();
        if used.saturating_add(lines) > max_lines {
            h.content = format!(
                "[content truncated: {lines} lines elided; run `squire show {short} {}` for full content]\n",
                h.id
            );
            h.line_hashes.clear();
        } else {
            used += lines;
        }
    }
}

fn run_split(out: &mut Output, dir: &Path, commit: &str) -> Result<(), String> {
    if !git::is_clean(dir)? {
        return Err("split requires a clean working tree".to_string());
    }
    let target = resolve_reachable_commit(dir, commit)?;
    let head = git::rev_parse(dir, "HEAD")?;
    if target == head {
        git::reset_mixed(dir, "HEAD~1")?;
    } else {
        git::rebase_edit_and_reset(dir, &target)?;
    }
    out.println("Ready to split. Use `squire diff` and `squire stage` to selectively commit.");
    Ok(())
}

fn run_status(cli: &Cli, out: &mut Output, dir: &Path) -> Result<(), String> {
    let branch = git::branch(dir)?;
    let rebasing = git::rebase_in_progress(dir)?;

    let conflicts = if rebasing {
        git::conflicting_files(dir).unwrap_or_default()
    } else {
        Vec::new()
    };

    let has_conflicts = !conflicts.is_empty();
    let cached_raw = git::diff(dir, &["--cached".to_string()])?;
    let (unstaged_raw, _) = diff_with_untracked(dir, &[])?;
    let parse = |raw: &str| -> Result<Vec<diff::HunkInfo>, String> {
        if has_conflicts {
            Ok(diff::parse_diff(raw).unwrap_or_default())
        } else {
            diff::parse_diff(raw)
        }
    };
    let staged = parse(&cached_raw)?;
    let unstaged = parse(&unstaged_raw)?;

    let (sa, sd) = output::count_lines(&staged);
    let (ua, ud) = output::count_lines(&unstaged);

    if cli.json {
        let status = response::StatusResult {
            branch: branch.clone(),
            rebase_in_progress: rebasing,
            staged,
            unstaged,
            staged_lines: response::LineCounts {
                added: sa,
                removed: sd,
            },
            unstaged_lines: response::LineCounts {
                added: ua,
                removed: ud,
            },
            conflicts: rebase::build_conflict_files(&conflicts),
        };
        out.println(&serde_json::to_string(&status).unwrap());
    } else {
        out.println(&format!("On branch {branch}"));
        if rebasing {
            out.println("Rebase in progress");
        }
        if !conflicts.is_empty() {
            out.println(&format!("Conflicts ({}):", conflicts.len()));
            for (f, s) in &conflicts {
                out.println(&format!("  {s}: {f}"));
            }
            out.println("Resolve conflicts, stage with `git add`, then run `GIT_EDITOR=true git rebase --continue`.");
            out.println("To cancel: `git rebase --abort`.");
        }
        let clean = staged.is_empty() && unstaged.is_empty();
        if clean && conflicts.is_empty() {
            out.println("Nothing to commit, working tree clean");
        } else {
            let printer = if cli.short {
                output::format_short
            } else {
                output::format_plain
            };
            if !staged.is_empty() {
                out.println(&format!("Staged ({}, +{}/-{}):", staged.len(), sa, sd));
                out.stdout.push_str(&printer(&staged));
            }
            if !unstaged.is_empty() {
                out.println(&format!("Unstaged ({}, +{}/-{}):", unstaged.len(), ua, ud));
                out.stdout.push_str(&printer(&unstaged));
            }
        }
    }
    Ok(())
}

fn run_drop(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    commit: &str,
    hunk_ids: &[String],
) -> Result<(), String> {
    let target = resolve_reachable_commit(dir, commit)?;
    let head = git::rev_parse(dir, "HEAD")?;
    let is_head = target == head;
    if !is_head {
        if !git::is_clean(dir)? {
            return Err("drop requires a clean working tree for non-HEAD commits".to_string());
        }
        git::rebase_edit(dir, &target).map_err(|e| check_rebase_conflict(dir, e, cli.json))?;
    }
    // After rebase_edit, the target is now HEAD
    let raw = git::diff(dir, &["HEAD~1".to_string(), "HEAD".to_string()])?;
    let hunks = diff::parse_diff(&raw)?;
    let (selected, _) = resolve_hunks(&hunks, hunk_ids, false)?;
    apply_resolved(dir, &selected, &["--cached", "--reverse"])?;
    git::commit_amend_allow_empty(dir)?;
    if !is_head {
        git::checkout_head(dir)?;
        git::rebase_continue(dir).map_err(|e| check_rebase_conflict(dir, e, cli.json))?;
    }
    emit_result(
        out,
        cli.json,
        response::ActionCount::Dropped(selected.len()),
        &format!(
            "Dropped {} hunk(s) from {}",
            selected.len(),
            short_sha(&target)
        ),
        &[],
    );
    Ok(())
}

fn run_commit(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    message: &str,
    hunk_ids: &[String],
) -> Result<(), String> {
    let total = stage_hunks_or_cached(dir, hunk_ids)?;
    git::commit(dir, message)?;
    emit_result(
        out,
        cli.json,
        response::ActionCount::Committed(total),
        &format!("Committed {total} hunk(s)"),
        &[],
    );
    Ok(())
}

fn run_amend(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    message: Option<&str>,
    commit: Option<&str>,
    hunk_ids: &[String],
) -> Result<(), String> {
    let total = stage_hunks_or_cached(dir, hunk_ids)?;
    let mut amended_target = String::from("HEAD");
    if let Some(rev) = commit {
        let head = git::rev_parse(dir, "HEAD")?;
        let target = resolve_reachable_commit(dir, rev)?;
        if target == head {
            git::commit_amend(dir, message)?;
        } else if message.is_some() {
            return Err("-m cannot be used with --commit for non-HEAD targets".to_string());
        } else {
            amended_target = short_sha(&target).to_string();
            // Snapshot HEAD before we create the fixup so we can detect
            // and roll it back if the rebase aborts.
            let head_before_fixup = head.clone();
            git::commit_fixup(dir, &target)?;
            let dirty = !git::is_clean(dir)?;
            if dirty {
                git::stash_push(dir, None)?;
            }
            if let Err(e) = git::rebase_autosquash(dir, &target) {
                // The rebase failed. Three things need to happen, in order:
                //   1. Capture any conflict info while the rebase is still
                //      in progress, because the abort in step 2 deletes the
                //      `.git/rebase-merge/*` state the formatter reads.
                //   2. If a rebase is still in progress (conflict), abort it
                //      so we leave the repo in a clean state.
                //   3. Roll back the fixup commit we created on HEAD before
                //      starting the rebase — otherwise the user is left with
                //      a dangling `fixup! ...` commit.
                //   4. Restore the stashed dirty tree if we stashed one.
                // This makes `amend --commit` atomic: either it succeeds or
                // the working history is unchanged.
                let formatted = amend_conflict_error(dir, e, cli.json);
                if let Ok(true) = git::rebase_in_progress(dir) {
                    let _ = git::rebase_abort(dir);
                }
                // Only roll back if HEAD currently points at the fixup commit
                // we created (HEAD moved forward by exactly one commit whose
                // parent is head_before_fixup). This guards against clobbering
                // state we didn't create.
                if let Ok(parent) = git::rev_parse(dir, "HEAD^")
                    && parent == head_before_fixup
                {
                    let _ = git::reset_hard(dir, &head_before_fixup);
                }
                if dirty {
                    let _ = git::stash_pop(dir);
                }
                return Err(formatted);
            }
            if dirty {
                git::stash_pop(dir)?;
            }
        }
    } else {
        git::commit_amend(dir, message)?;
    }
    emit_result(
        out,
        cli.json,
        response::ActionCount::Amended(total),
        &format!("Amended {total} hunk(s) into {amended_target}"),
        &[],
    );
    Ok(())
}

/// Build an error message for a commit SHA that resolves but isn't
/// reachable from HEAD. When possible, look up the rewritten equivalent
/// on the current branch (by matching the original commit's subject and
/// author date) and suggest retrying with it.
fn stale_sha_error(dir: &Path, input: &str, resolved_sha: &str) -> String {
    let subject = git::commit_subject(dir, resolved_sha).ok();
    let author_date = git::commit_author_date(dir, resolved_sha).ok();
    let suggestion = match subject.as_deref() {
        Some(subj) if !subj.is_empty() => {
            // Search a generous window so that squash/amend chains deep in
            // history are still found.
            git::find_reachable_by_subject(dir, subj, author_date.as_deref(), 256)
                .ok()
                .flatten()
        }
        _ => None,
    };
    let short_input = if input.len() > 12 {
        &input[..12]
    } else {
        input
    };
    match (suggestion, subject) {
        (Some(new_sha), Some(subj)) => format!(
            "commit {short_input} is not reachable from HEAD — it was rewritten to {} ({subj:?}); retry with {}",
            short_sha(&new_sha),
            short_sha(&new_sha),
        ),
        (Some(new_sha), None) => format!(
            "commit {short_input} is not reachable from HEAD — it was rewritten to {}; retry with {}",
            short_sha(&new_sha),
            short_sha(&new_sha),
        ),
        (None, _) => format!(
            "commit {short_input} is not reachable from HEAD; pass a commit on the current branch"
        ),
    }
}

/// Resolve `rev` to a full SHA and verify it's reachable from HEAD.
///
/// Rejects SHAs that `git rev-parse` can resolve via the object database or
/// reflog but which aren't part of the current branch's history. This
/// typically happens after a prior rewrite (amend/drop/squash/reword) — the
/// original SHA still exists in the object db, but using it as a rebase
/// base (e.g. `target~1`) would replay the rewritten branch on top of a
/// superseded ancestor, producing spurious conflicts.
///
/// Used by every command that takes a `<commit>` argument and then rebases
/// against it: `amend --commit`, `drop`, `reword`, `squash` (target and
/// each source), and `split`.
fn resolve_reachable_commit(dir: &Path, rev: &str) -> Result<String, String> {
    let sha = git::rev_parse(dir, rev)?;
    if !git::is_reachable_from_head(dir, &sha)? {
        return Err(stale_sha_error(dir, rev, &sha));
    }
    Ok(sha)
}

/// Format an amend-specific conflict error. Called while the rebase is
/// still in progress, so it can read `.git/rebase-merge/*` state for
/// conflicting files and the commit being replayed. The caller is
/// expected to abort the rebase and roll back the fixup immediately
/// after calling this.
///
/// Unlike `check_rebase_conflict`, the hint tells the user that the
/// amend was rolled back (because that's what the caller is about to
/// do), rather than suggesting `git rebase --continue`. This makes
/// `amend --commit` atomic from the user's perspective.
fn amend_conflict_error(dir: &Path, err: String, json: bool) -> String {
    let in_progress = matches!(git::rebase_in_progress(dir), Ok(true));
    let files = git::conflicting_files(dir).unwrap_or_default();
    if !in_progress || files.is_empty() {
        return err;
    }
    let current_commit = git::rebase_current_commit(dir);
    let onto = git::rebase_onto(dir);
    let hint = "amend could not fold cleanly; conflicting hunks left the amend \
         aborted and history unchanged. Try staging and committing the \
         conflicting changes separately, or resolve by hand with a manual \
         rebase."
        .to_string();
    if json {
        let result = response::ConflictError {
            conflict: true,
            conflicting_files: rebase::build_conflict_files(&files),
            hint,
            current_commit: current_commit
                .as_ref()
                .map(|(sha, msg)| response::CommitRef {
                    sha: sha.clone(),
                    message: msg.clone(),
                    upstream_match: None,
                }),
            ours_theirs: onto.as_ref().map(|o| response::OursTheirs {
                ours: format!("upstream ({o})"),
                theirs: "your commit being replayed".to_string(),
            }),
        };
        return serde_json::to_string(&result).unwrap();
    }
    let mut out = Output::default();
    if let Some((sha, subject)) = &current_commit {
        out.println(&format!("Replaying: {sha:.8} {subject}"));
    }
    out.println("Conflict during amend (rolled back, history unchanged):");
    rebase::format_conflict_files(&mut out, &files);
    if let Some(ref o) = onto {
        out.println(&format!(
            "Note: \"ours\" = upstream ({o}), \"theirs\" = your commit"
        ));
    }
    out.println(&hint);
    out.stdout.trim_end().to_string()
}

fn run_reword(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    commit: &str,
    message: &str,
) -> Result<(), String> {
    let target = resolve_reachable_commit(dir, commit)?;
    let head = git::rev_parse(dir, "HEAD")?;
    if target == head {
        git::commit_amend(dir, Some(message))?;
    } else if !git::is_clean(dir)? {
        return Err("reword requires a clean working tree for non-HEAD commits".to_string());
    } else {
        git::rebase_reword(dir, &target, message)
            .map_err(|e| check_rebase_conflict(dir, e, cli.json))?;
    }
    if cli.json {
        let result = response::RewordResult {
            reworded: true,
            message: message.to_string(),
        };
        out.println(&serde_json::to_string(&result).unwrap());
    } else {
        out.println(&format!("Reworded commit {}", short_sha(&target)));
    }
    Ok(())
}

fn run_revert(cli: &Cli, out: &mut Output, dir: &Path, hunk_ids: &[String]) -> Result<(), String> {
    let (unstaged_raw, _) = diff_with_untracked(dir, &[])?;
    let unstaged = diff::parse_diff(&unstaged_raw)?;
    let cached_raw = git::diff(dir, &["--cached".to_string()])?;
    let cached = diff::parse_diff(&cached_raw)?;

    let mut unstaged_args = Vec::new();
    let mut cached_args = Vec::new();
    for arg in hunk_ids {
        let id = arg.split_once(':').map_or(arg.as_str(), |(id, _)| id);
        if let Ok(h) = find_hunk(&unstaged, id) {
            if h.old_file == "/dev/null" {
                return Err(format!(
                    "cannot revert untracked file '{}'; use rm to delete it",
                    h.file
                ));
            }
            unstaged_args.push(arg.clone());
        } else {
            find_hunk(&cached, id).map_err(|_| format!("hunk {id} not found"))?;
            cached_args.push(arg.clone());
        }
    }
    let mut had_partial = false;
    let mut affected_files: std::collections::HashSet<String> = std::collections::HashSet::new();
    if !unstaged_args.is_empty() {
        let (resolved, partial) = resolve_hunks(&unstaged, &unstaged_args, true)?;
        had_partial |= partial;
        for h in &resolved {
            affected_files.insert(h.file.clone());
        }
        apply_resolved(dir, &resolved, &["--reverse"])?;
    }
    if !cached_args.is_empty() {
        let (resolved, partial) = resolve_hunks(&cached, &cached_args, true)?;
        had_partial |= partial;
        for h in &resolved {
            affected_files.insert(h.file.clone());
        }
        let refs: Vec<&diff::HunkInfo> = resolved.iter().collect();
        let patch = diff::reconstruct_patch(&refs);
        git::apply_cached(dir, &patch, true)?;
        git::apply_worktree(dir, &patch)?;
    }
    let file_refs: std::collections::HashSet<&str> =
        affected_files.iter().map(|s| s.as_str()).collect();
    let new_hunks = residual_hunks(dir, had_partial, &file_refs, false)?;
    let total = unstaged_args.len() + cached_args.len();
    emit_result(
        out,
        cli.json,
        response::ActionCount::Reverted(total),
        &format!("Reverted {total} hunk(s)"),
        &new_hunks,
    );
    Ok(())
}

fn run_stage(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    hunk_ids: &[String],
    unstage: bool,
) -> Result<(), String> {
    let raw = if unstage {
        git::diff(dir, &["--cached".to_string()])?
    } else {
        let (r, _) = diff_with_untracked(dir, &[])?;
        r
    };
    let (total, new_hunks) = stage_hunks(dir, &raw, hunk_ids, unstage)?;
    let (label, count) = if unstage {
        ("Unstaged", response::ActionCount::Unstaged(total))
    } else {
        ("Staged", response::ActionCount::Staged(total))
    };
    emit_result(
        out,
        cli.json,
        count,
        &format!("{label} {total} hunk(s)"),
        &new_hunks,
    );
    Ok(())
}

fn run_show(cli: &Cli, out: &mut Output, dir: &Path, args: &[String]) -> Result<(), String> {
    let (json, short, filtered) = extract_format_flags(cli, args);
    let (git_args, hunk_arg) = split_last_arg(&filtered, "show requires a hunk ID")?;
    let (hunk_id, selector) = match hunk_arg.split_once(':') {
        Some((id, sel)) => (id, Some(sel)),
        None => (hunk_arg, None),
    };
    let resolve_show = |hunk: &diff::HunkInfo| -> Result<diff::HunkInfo, String> {
        match selector {
            Some(sel) => {
                let line_hashes = resolve_selector(hunk, sel)?;
                let refs: Vec<&str> = line_hashes.iter().map(|s| s.as_str()).collect();
                diff::select_lines(hunk, &refs, false)
            }
            None => Ok(hunk.clone()),
        }
    };
    if !git_args.is_empty() {
        let mut show_args = vec!["--format=".to_string()];
        show_args.extend_from_slice(git_args);
        let raw = git::show(dir, &show_args)?;
        let hunks = diff::parse_diff(&raw)?;
        if let Ok(hunk) = find_hunk(&hunks, hunk_id) {
            let resolved = resolve_show(hunk)?;
            print_hunks(out, json, short, std::slice::from_ref(&resolved))?;
            return Ok(());
        }
    }
    let cached_raw = git::diff(dir, &["--cached".to_string()])?;
    let (unstaged_raw, _) = diff_with_untracked(dir, &[])?;
    let cached_hunks = diff::parse_diff(&cached_raw)?;
    let unstaged_hunks = diff::parse_diff(&unstaged_raw)?;
    let hunk =
        match find_hunk(&cached_hunks, hunk_id).or_else(|_| find_hunk(&unstaged_hunks, hunk_id)) {
            Ok(h) => h.clone(),
            Err(e) => {
                // If the arg resolves to a commit, show all hunks from that commit.
                if git_args.is_empty() && selector.is_none() && git::is_ref(dir, hunk_id) {
                    let raw = git::show(dir, &["--format=".to_string(), hunk_id.to_string()])?;
                    let hunks = diff::parse_diff(&raw)?;
                    print_hunks(out, json, short, &hunks)?;
                    return Ok(());
                }
                return Err(e);
            }
        };
    let resolved = resolve_show(&hunk)?;
    print_hunks(out, json, short, std::slice::from_ref(&resolved))?;
    Ok(())
}

fn run_diff(cli: &Cli, out: &mut Output, dir: &Path, args: &[String]) -> Result<(), String> {
    let (json, short, filtered) = extract_format_flags(cli, args);
    let (raw, binary_files) = diff_with_untracked(dir, &filtered)?;
    let hunks = diff::parse_diff(&raw)?;
    print_hunks(out, json, short, &hunks)?;
    for f in &binary_files {
        out.eprintln(&format!("warning: skipping binary file {f}"));
    }
    Ok(())
}

/// Run squire with a parsed CLI in the given directory.
pub fn run(cli: &Cli, command: &Command, dir: &Path) -> Result<Output, String> {
    let mut out = Output::default();
    match command {
        Command::Diff { args } => run_diff(cli, &mut out, dir, args)?,
        Command::Show { args } => run_show(cli, &mut out, dir, args)?,
        Command::Stage { hunk_ids } => run_stage(cli, &mut out, dir, hunk_ids, false)?,
        Command::Unstage { hunk_ids } => run_stage(cli, &mut out, dir, hunk_ids, true)?,
        Command::Revert { hunk_ids } => run_revert(cli, &mut out, dir, hunk_ids)?,
        Command::Commit { message, hunk_ids } => run_commit(cli, &mut out, dir, message, hunk_ids)?,
        Command::Amend {
            message,
            commit,
            hunk_ids,
        } => run_amend(
            cli,
            &mut out,
            dir,
            message.as_deref(),
            commit.as_deref(),
            hunk_ids,
        )?,
        Command::Reword { commit, message } => run_reword(cli, &mut out, dir, commit, message)?,
        Command::Drop { commit, hunk_ids } => run_drop(cli, &mut out, dir, commit, hunk_ids)?,
        Command::Status => run_status(cli, &mut out, dir)?,
        Command::Log { n, max_hunk_lines } => run_log(cli, &mut out, dir, *n, *max_hunk_lines)?,
        Command::Split { commit } => run_split(&mut out, dir, commit)?,
        Command::Cleanup { master } => cleanup::run_cleanup(cli, &mut out, dir, master.as_deref())?,
        Command::Squash { message, commits } => {
            run_squash(cli, &mut out, dir, message.as_deref(), commits)?
        }
        Command::Seqedit { args } => run_seqedit(args)?,
        Command::Stash { message, hunk_ids } => {
            run_stash(cli, &mut out, dir, message.as_deref(), hunk_ids)?
        }
        Command::Rebase { onto } => rebase::run_rebase(cli, &mut out, dir, onto.as_deref())?,
    }
    Ok(out)
}

/// Extract --json/--short flags from args, merging with CLI-level flags.
fn extract_format_flags(cli: &Cli, args: &[String]) -> (bool, bool, Vec<String>) {
    let json = cli.json || args.iter().any(|a| a == "--json");
    let short = cli.short || args.iter().any(|a| a == "--short");
    let filtered = args
        .iter()
        .filter(|a| *a != "--json" && *a != "--short")
        .cloned()
        .collect();
    (json, short, filtered)
}

/// Run `git diff` with the given args and append untracked file diffs.
fn diff_with_untracked(dir: &Path, args: &[String]) -> Result<(String, Vec<String>), String> {
    let mut raw = git::diff(dir, args)?;
    let mut binary_files = Vec::new();
    // Only include untracked files for plain working-tree diffs (no refs, no --cached).
    // Clap strips the `--` separator, so args after it appear as bare positional args.
    let has_refs_or_cached = args.iter().any(|a| {
        *a == "--cached" || *a == "--staged" || (!a.starts_with('-') && git::is_ref(dir, a))
    });
    if !has_refs_or_cached {
        let mut files = git::list_untracked(dir)?;
        // Non-flag args that aren't refs are path filters — apply them to untracked files.
        let path_filters: Vec<&str> = args
            .iter()
            .filter(|a| !a.starts_with('-') && !git::is_ref(dir, a))
            .map(|s| s.as_str())
            .collect();
        if !path_filters.is_empty() {
            files.retain(|f| path_filters.iter().any(|p| f.starts_with(p) || f == p));
        }
        if !files.is_empty() {
            let root = std::path::PathBuf::from(git::toplevel(dir)?);
            let (diff_text, binaries) = diff::generate_untracked_diff(&files, &root)?;
            raw.push_str(&diff_text);
            binary_files = binaries;
        }
    }
    Ok((raw, binary_files))
}

fn print_hunks(
    out: &mut Output,
    json: bool,
    short: bool,
    hunks: &[diff::HunkInfo],
) -> Result<(), String> {
    if json {
        let s = serde_json::to_string_pretty(hunks)
            .map_err(|e| format!("failed to serialize JSON: {e}"))?;
        out.println(&s);
    } else if short {
        out.stdout.push_str(&output::format_short(hunks));
    } else {
        out.stdout.push_str(&output::format_plain(hunks));
    }
    Ok(())
}

fn emit_result(
    out: &mut Output,
    json: bool,
    count: response::ActionCount,
    msg: &str,
    new_hunks: &[diff::HunkInfo],
) {
    if json {
        let result = response::ActionResult {
            count,
            message: msg.to_string(),
            new_hunks: new_hunks
                .iter()
                .map(response::NewHunkSummary::from)
                .collect(),
        };
        out.println(
            &serde_json::to_string(&result).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
        );
    } else {
        out.println(msg);
        for h in new_hunks {
            let (add, del) = output::count_hunk_lines(h);
            let header = h
                .header
                .as_ref()
                .map(|s| format!("  {s}"))
                .unwrap_or_default();
            let first_change = h
                .content
                .lines()
                .find(|l| l.starts_with('+') || l.starts_with('-'))
                .map(|l| format!("  {} {}", &l[..1], l[1..].trim_start()))
                .unwrap_or_default();
            out.println(&format!(
                "  new hunk: {}  {}  +{}/-{}{}{}",
                h.id, h.file, add, del, header, first_change
            ));
        }
    }
}
