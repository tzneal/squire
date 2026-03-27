pub mod cli;
pub mod diff;
pub mod git;
pub mod output;

use cli::{Cli, Command};
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

/// Run squire with a parsed CLI in the given directory.
pub fn run(cli: &Cli, command: &Command, dir: &Path) -> Result<Output, String> {
    let mut out = Output::default();
    match command {
        Command::Diff { args } => {
            let (json, short, filtered) = extract_format_flags(cli, args);
            let (raw, binary_files) = diff_with_untracked(dir, &filtered)?;
            let hunks = diff::parse_diff(&raw)?;
            print_hunks(&mut out, json, short, &hunks)?;
            for f in &binary_files {
                out.eprintln(&format!("warning: skipping binary file {f}"));
            }
        }
        Command::Show { args } => {
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
                    print_hunks(&mut out, json, short, std::slice::from_ref(&resolved))?;
                    return Ok(out);
                }
            }
            let cached_raw = git::diff(dir, &["--cached".to_string()])?;
            let (unstaged_raw, _) = diff_with_untracked(dir, &[])?;
            let cached_hunks = diff::parse_diff(&cached_raw)?;
            let unstaged_hunks = diff::parse_diff(&unstaged_raw)?;
            let hunk = find_hunk(&cached_hunks, hunk_id)
                .or_else(|_| find_hunk(&unstaged_hunks, hunk_id))?;
            let resolved = resolve_show(hunk)?;
            print_hunks(&mut out, json, short, std::slice::from_ref(&resolved))?;
        }
        Command::Stage { hunk_ids } | Command::Unstage { hunk_ids } => {
            let unstage = matches!(command, Command::Unstage { .. });
            let raw = if unstage {
                git::diff(dir, &["--cached".to_string()])?
            } else {
                let (r, _) = diff_with_untracked(dir, &[])?;
                r
            };
            let total = stage_hunks(dir, &raw, hunk_ids, unstage)?;
            let (label, key) = if unstage {
                ("Unstaged", "unstaged")
            } else {
                ("Staged", "staged")
            };
            emit_result(
                &mut out,
                cli.json,
                key,
                total,
                &format!("{label} {total} hunk(s)"),
            );
        }
        Command::Revert { hunk_ids } => {
            let (raw, _) = diff_with_untracked(dir, &[])?;
            let hunks = diff::parse_diff(&raw)?;
            let selected = resolve_hunks(&hunks, hunk_ids, true)?;
            let refs: Vec<&diff::HunkInfo> = selected.iter().collect();
            let patch = diff::reconstruct_patch(&refs);
            git::apply_worktree(dir, &patch)?;
            emit_result(
                &mut out,
                cli.json,
                "reverted",
                selected.len(),
                &format!("Reverted {} hunk(s)", selected.len()),
            );
        }
        Command::Commit { message, hunk_ids } => {
            let total = stage_hunks_or_cached(dir, hunk_ids)?;
            git::commit(dir, message)?;
            emit_result(
                &mut out,
                cli.json,
                "committed",
                total,
                &format!("Committed {total} hunk(s)"),
            );
        }
        Command::Amend {
            message,
            commit,
            hunk_ids,
        } => {
            let total = stage_hunks_or_cached(dir, hunk_ids)?;
            match commit {
                Some(rev) => {
                    let target = git::rev_parse(dir, rev)?;
                    let head = git::rev_parse(dir, "HEAD")?;
                    if target == head {
                        git::commit_amend(dir, message.as_deref())?;
                    } else {
                        if message.is_some() {
                            return Err(
                                "-m cannot be used with --commit for non-HEAD targets".to_string()
                            );
                        }
                        git::rebase_autosquash(dir, &target)?;
                    }
                }
                None => {
                    git::commit_amend(dir, message.as_deref())?;
                }
            }
            emit_result(
                &mut out,
                cli.json,
                "amended",
                total,
                &format!("Amended {total} hunk(s) into HEAD"),
            );
        }
        Command::Status => {
            let branch = git::branch(dir)?;
            let rebasing = git::rebase_in_progress(dir)?;

            let cached_raw = git::diff(dir, &["--cached".to_string()])?;
            let staged = diff::parse_diff(&cached_raw)?;

            let (unstaged_raw, _) = diff_with_untracked(dir, &[])?;
            let unstaged = diff::parse_diff(&unstaged_raw)?;

            let (sa, sd) = output::count_lines(&staged);
            let (ua, ud) = output::count_lines(&unstaged);

            if cli.json {
                out.println(
                    &serde_json::json!({
                        "branch": branch,
                        "rebase_in_progress": rebasing,
                        "staged": staged,
                        "unstaged": unstaged,
                        "staged_lines": { "added": sa, "removed": sd },
                        "unstaged_lines": { "added": ua, "removed": ud },
                    })
                    .to_string(),
                );
            } else {
                out.println(&format!("On branch {branch}"));
                if rebasing {
                    out.println("Rebase in progress");
                }
                let clean = staged.is_empty() && unstaged.is_empty();
                if clean {
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
        }
        Command::Log { n } => {
            let raw = git::log(dir, *n)?;
            let commits = diff::parse_log(&raw)?;
            if cli.json {
                let s = serde_json::to_string_pretty(&commits)
                    .map_err(|e| format!("failed to serialize JSON: {e}"))?;
                out.println(&s);
            } else if cli.short {
                out.stdout.push_str(&output::format_log_short(&commits));
            } else {
                out.stdout.push_str(&output::format_log_plain(&commits));
            }
        }
        Command::Split { commit } => {
            if !git::is_clean(dir)? {
                return Err("split requires a clean working tree".to_string());
            }
            let target = git::rev_parse(dir, commit)?;
            let head = git::rev_parse(dir, "HEAD")?;
            if target == head {
                git::reset_mixed(dir, "HEAD~1")?;
            } else {
                git::rebase_edit_and_reset(dir, &target)?;
            }
            out.println(
                "Ready to split. Use `squire diff` and `squire stage` to selectively commit.",
            );
        }
        Command::Cleanup { master } => {
            let has_remote = git::fetch(dir)?;
            if !has_remote {
                out.eprintln("warning: no remote 'origin' found, comparing against local branches");
            }

            let master_branch = match master {
                Some(m) => m.clone(),
                None => git::detect_master_branch(dir, has_remote)?,
            };
            let compare_ref = if has_remote {
                format!("origin/{master_branch}")
            } else {
                master_branch.clone()
            };

            let all_branches = git::list_branches(dir)?;
            let merged_set = git::merged_branches(dir, &compare_ref)?;
            let current = git::branch(dir)?;

            // Collect master commit messages for matching
            let master_msgs = git::commit_messages(dir, &compare_ref, 500)?;
            let master_msg_set: std::collections::HashSet<&str> =
                master_msgs.iter().map(|s| s.as_str()).collect();

            #[derive(serde::Serialize)]
            struct BranchInfo {
                name: String,
                status: String,
                last_commit_date: String,
                commit_count: usize,
                commits: Vec<CommitSummary>,
                #[serde(skip_serializing_if = "Option::is_none")]
                note: Option<String>,
            }
            #[derive(serde::Serialize)]
            struct CommitSummary {
                sha: String,
                message: String,
                #[serde(skip_serializing_if = "Option::is_none")]
                message_in_master: Option<bool>,
                #[serde(skip_serializing_if = "Option::is_none")]
                patch_applied: Option<bool>,
            }

            let mut branches = Vec::new();

            for branch in &all_branches {
                if branch == &master_branch || branch == &current {
                    continue;
                }

                let last_date = git::branch_last_commit_date(dir, branch).unwrap_or_default();

                if merged_set.contains(branch) {
                    branches.push(BranchInfo {
                        name: branch.clone(),
                        status: "merged".to_string(),
                        last_commit_date: last_date,
                        commit_count: 0,
                        commits: vec![],
                        note: None,
                    });
                    continue;
                }

                let commits = git::commits_not_in(dir, branch, &compare_ref)?;
                if commits.is_empty() {
                    branches.push(BranchInfo {
                        name: branch.clone(),
                        status: "merged".to_string(),
                        last_commit_date: last_date,
                        commit_count: 0,
                        commits: vec![],
                        note: None,
                    });
                    continue;
                }

                let commit_count = commits.len();
                let mut summaries = Vec::new();
                let mut all_messages_match = true;
                let mut any_message_match = false;
                let mut all_patches_applied = true;
                let mut needs_eval = false;

                let applied_shas =
                    git::cherry_applied(dir, &compare_ref, branch).unwrap_or_default();

                for (sha, msg) in &commits {
                    let msg_match = master_msg_set.contains(msg.as_str());
                    if msg_match {
                        any_message_match = true;
                    } else {
                        all_messages_match = false;
                    }

                    let patch_applied = if msg_match {
                        let applied = applied_shas.contains(sha.as_str());
                        if !applied {
                            all_patches_applied = false;
                            needs_eval = true;
                        }
                        Some(applied)
                    } else {
                        all_patches_applied = false;
                        None
                    };

                    summaries.push(CommitSummary {
                        sha: sha[..8.min(sha.len())].to_string(),
                        message: msg.clone(),
                        message_in_master: if msg_match { Some(true) } else { None },
                        patch_applied,
                    });
                }

                let (status, note) = if all_messages_match && all_patches_applied {
                    ("merged_equivalent".to_string(), Some("All commits have matching messages and patches in master (squash/cherry-pick merged).".to_string()))
                } else if needs_eval {
                    ("needs_evaluation".to_string(), Some("Some commits have matching messages in master but patches differ. An LLM should evaluate these commits to determine if the branch is fully merged.".to_string()))
                } else if any_message_match {
                    ("needs_evaluation".to_string(), Some("Some commit messages match master but others do not. Evaluate whether remaining changes are needed.".to_string()))
                } else {
                    ("unmerged".to_string(), None)
                };

                branches.push(BranchInfo {
                    name: branch.clone(),
                    status,
                    last_commit_date: last_date,
                    commit_count,
                    commits: summaries,
                    note,
                });
            }

            if cli.json {
                let s = serde_json::to_string_pretty(&serde_json::json!({
                    "master_branch": master_branch,
                    "current_branch": current,
                    "has_remote": has_remote,
                    "branches": branches,
                }))
                .map_err(|e| format!("failed to serialize JSON: {e}"))?;
                out.println(&s);
            } else {
                out.println(&format!("Master branch: {master_branch}"));
                out.println(&format!("Current branch: {current}"));
                out.println("");

                if branches.is_empty() {
                    out.println("No other branches found.");
                }

                for b in &branches {
                    let count_str = if b.commit_count > 0 {
                        format!(
                            " ({} commit{})",
                            b.commit_count,
                            if b.commit_count == 1 { "" } else { "s" }
                        )
                    } else {
                        String::new()
                    };
                    out.println(&format!(
                        "[{}] {}{}  (last: {})",
                        b.status.to_uppercase(),
                        b.name,
                        count_str,
                        b.last_commit_date
                    ));
                    if let Some(note) = &b.note {
                        out.println(&format!("  {note}"));
                    }
                    for c in &b.commits {
                        let mut extras = String::new();
                        if c.message_in_master == Some(true) {
                            extras.push_str(" (message in master");
                            match c.patch_applied {
                                Some(true) => extras.push_str(", patch applied)"),
                                Some(false) => extras.push_str(", patch differs)"),
                                None => extras.push(')'),
                            }
                        }
                        out.println(&format!("  {} {}{}", c.sha, c.message, extras));
                    }
                    out.println("");
                }
            }
        }
        Command::Squash { message, commits } => {
            if !git::is_clean(dir)? {
                return Err("squash requires a clean working tree".to_string());
            }
            let target = git::rev_parse(dir, &commits[0])?;
            let sources: Vec<String> = commits[1..]
                .iter()
                .map(|c| git::rev_parse(dir, c))
                .collect::<Result<_, _>>()?;
            git::rebase_squash(dir, &target, &sources)?;
            if let Some(msg) = message {
                git::commit_amend(dir, Some(msg))?;
            }
            emit_result(
                &mut out,
                cli.json,
                "squashed",
                sources.len(),
                &format!(
                    "Squashed {} commit(s) into {}",
                    sources.len(),
                    &target[..8.min(target.len())]
                ),
            );
        }
        Command::Seqedit { args } => {
            if args.len() < 2 {
                return Err("seqedit requires at least one action and a todo file path".to_string());
            }
            let (actions, file) = args.split_at(args.len() - 1);
            let file = &file[0];

            let todo = std::fs::read_to_string(file)
                .map_err(|e| format!("failed to read todo file: {e}"))?;
            let mut lines: Vec<String> = todo.lines().map(String::from).collect();

            for action_arg in actions {
                let (action, sha_prefix) = action_arg.split_once(':').ok_or_else(|| {
                    format!("invalid action syntax: {action_arg} (expected action:sha)")
                })?;
                match action {
                    "pick" | "edit" | "squash" | "fixup" | "drop" => {}
                    _ => {
                        return Err(format!(
                            "unknown action: {action} (expected pick, edit, squash, fixup, or drop)"
                        ));
                    }
                }
                let matches: Vec<usize> = lines
                    .iter()
                    .enumerate()
                    .filter_map(|(i, line)| {
                        let parts: Vec<&str> = line.splitn(3, ' ').collect();
                        if parts.len() >= 2
                            && !parts[0].starts_with('#')
                            && (parts[1].starts_with(sha_prefix)
                                || sha_prefix.starts_with(parts[1]))
                        {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .collect();
                match matches.len() {
                    0 => return Err(format!("no todo line matches sha prefix: {sha_prefix}")),
                    1 => {
                        let line = &mut lines[matches[0]];
                        let old_action = line.split(' ').next().unwrap();
                        *line = line.replacen(old_action, action, 1);
                    }
                    _ => {
                        return Err(format!(
                            "ambiguous sha prefix {sha_prefix}: matches {} lines",
                            matches.len()
                        ));
                    }
                }
            }

            let mut result = lines.join("\n");
            if todo.ends_with('\n') {
                result.push('\n');
            }
            std::fs::write(file, result).map_err(|e| format!("failed to write todo file: {e}"))?;
        }
        Command::Stash { message, hunk_ids } => {
            let (raw, _) = diff_with_untracked(dir, &[])?;
            let hunks = diff::parse_diff(&raw)?;
            let selected = resolve_hunks(&hunks, hunk_ids, false)?;
            let selected_ids: std::collections::HashSet<&str> =
                selected.iter().map(|h| h.id.as_str()).collect();
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

            let stash_result = git::stash_push(dir, message.as_deref());

            if let Some(ref p) = keep_patch {
                git::apply_worktree_forward(dir, p)?;
            }

            stash_result?;
            emit_result(
                &mut out,
                cli.json,
                "stashed",
                selected.len(),
                &format!("Stashed {} hunk(s)", selected.len()),
            );
        }
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
    let args_before_sep: Vec<&String> = args.iter().take_while(|a| *a != "--").collect();
    let has_refs_or_cached = args_before_sep.iter().any(|a| {
        *a == "--cached" || *a == "--staged" || (!a.starts_with('-') && git::is_ref(dir, a))
    });
    if !has_refs_or_cached {
        let files = git::list_untracked(dir)?;
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

/// Parse diff, resolve hunk IDs, build patch, and apply to index. Returns hunk count.
fn stage_hunks(dir: &Path, raw: &str, hunk_ids: &[String], reverse: bool) -> Result<usize, String> {
    let hunks = diff::parse_diff(raw)?;
    let selected = resolve_hunks(&hunks, hunk_ids, false)?;
    let refs: Vec<&diff::HunkInfo> = selected.iter().collect();
    let patch = diff::reconstruct_patch(&refs);
    git::apply_cached(dir, &patch, reverse)?;
    Ok(selected.len())
}

/// Like stage_hunks, but accepts hunks that are already staged.
/// Unstaged hunks get staged; already-staged hunks are counted but not re-applied.
fn stage_hunks_or_cached(dir: &Path, hunk_ids: &[String]) -> Result<usize, String> {
    let (unstaged_raw, _) = diff_with_untracked(dir, &[])?;
    let unstaged = diff::parse_diff(&unstaged_raw)?;
    let cached_raw = git::diff(dir, &["--cached".to_string()])?;
    let cached = diff::parse_diff(&cached_raw)?;

    let mut to_stage = Vec::new();
    for arg in hunk_ids {
        let id = arg.split_once(':').map_or(arg.as_str(), |(id, _)| id);
        if find_hunk(&unstaged, id).is_ok() {
            to_stage.push(arg.clone());
        } else {
            find_hunk(&cached, id).map_err(|_| format!("hunk {id} not found"))?;
        }
    }
    if !to_stage.is_empty() {
        let hunks_to_apply = resolve_hunks(&unstaged, &to_stage, false)?;
        let refs: Vec<&diff::HunkInfo> = hunks_to_apply.iter().collect();
        let patch = diff::reconstruct_patch(&refs);
        git::apply_cached(dir, &patch, false)?;
    }
    Ok(hunk_ids.len())
}

fn emit_result(out: &mut Output, json: bool, key: &str, count: usize, msg: &str) {
    if json {
        out.println(&serde_json::json!({ key: count, "message": msg }).to_string());
    } else {
        out.println(msg);
    }
}

/// Split args into (git_args, hunk_id). The hunk ID is the last arg,
/// validated as a hex string (optionally with a `:selector` suffix).
fn split_last_arg<'a>(
    args: &'a [String],
    err_msg: &str,
) -> Result<(&'a [String], &'a str), String> {
    if args.is_empty() {
        return Err(err_msg.to_string());
    }
    let (git_args, last) = args.split_at(args.len() - 1);
    let id = &last[0];
    let hex_part = id.split(':').next().unwrap_or(id);
    if hex_part.is_empty() || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "'{id}' is not a valid hunk ID (expected hex string)"
        ));
    }
    Ok((git_args, id))
}

fn find_hunk<'a>(hunks: &'a [diff::HunkInfo], id: &str) -> Result<&'a diff::HunkInfo, String> {
    if !id.chars().all(|c| c.is_ascii_hexdigit()) || id.is_empty() {
        return Err(format!(
            "'{id}' is not a valid hunk ID (expected hex string)"
        ));
    }
    let matches: Vec<&diff::HunkInfo> = hunks.iter().filter(|h| h.id.starts_with(id)).collect();
    match matches.len() {
        0 => Err(format!("hunk {id} not found")),
        1 => Ok(matches[0]),
        _ => Err(format!(
            "hunk ID prefix '{id}' is ambiguous ({} matches)",
            matches.len()
        )),
    }
}

/// Resolve hunk ID args (with optional line selectors) into concrete HunkInfos.
fn resolve_hunks(
    hunks: &[diff::HunkInfo],
    hunk_ids: &[String],
    reverse: bool,
) -> Result<Vec<diff::HunkInfo>, String> {
    let mut selected = Vec::new();
    for arg in hunk_ids {
        if let Some((id, selector)) = arg.split_once(':') {
            let hunk = find_hunk(hunks, id)?;
            let line_hashes = resolve_selector(hunk, selector)?;
            let refs: Vec<&str> = line_hashes.iter().map(|s| s.as_str()).collect();
            selected.push(diff::select_lines(hunk, &refs, reverse)?);
        } else {
            let hunk = find_hunk(hunks, arg)?;
            selected.push(hunk.clone());
        }
    }
    Ok(selected)
}

/// Resolve a selector string into a list of line hashes.
/// Supports comma-separated hashes (f3,a1,7b) and ranges (f3-7b).
fn resolve_selector(hunk: &diff::HunkInfo, selector: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    for part in selector.split(',') {
        if let Some((start_hash, end_hash)) = try_split_range(part) {
            let start_idx = find_line_index(&hunk.line_hashes, start_hash)?;
            let end_idx = find_line_index(&hunk.line_hashes, end_hash)?;
            if start_idx > end_idx {
                return Err(format!(
                    "invalid range: {start_hash} comes after {end_hash}"
                ));
            }
            for hash in &hunk.line_hashes[start_idx..=end_idx] {
                result.push(hash.clone());
            }
        } else {
            result.push(part.to_string());
        }
    }
    Ok(result)
}

/// Try to split a part as a hash range (e.g. "f3-7b").
/// Returns None if it doesn't look like a range.
fn try_split_range(part: &str) -> Option<(&str, &str)> {
    for (i, _) in part.char_indices().filter(|(_, c)| *c == '-') {
        if i >= 2 {
            let left = &part[..i];
            let right = &part[i + 1..];
            if right.len() >= 2
                && left.chars().all(|c| c.is_ascii_hexdigit())
                && right.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Some((left, right));
            }
        }
    }
    None
}

fn find_line_index(line_hashes: &[String], prefix: &str) -> Result<usize, String> {
    let matches: Vec<usize> = line_hashes
        .iter()
        .enumerate()
        .filter(|(_, h)| h.starts_with(prefix))
        .map(|(i, _)| i)
        .collect();
    match matches.len() {
        0 => Err(format!("line hash '{prefix}' not found")),
        1 => Ok(matches[0]),
        _ => Err(format!("line hash '{prefix}' is ambiguous")),
    }
}
