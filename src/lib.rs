pub mod cleanup;
pub mod cli;
pub mod diff;
pub mod git;
pub mod output;
pub mod rebase;

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

fn short_sha(sha: &str) -> &str {
    &sha[..8.min(sha.len())]
}

/// For files whose contents are fully derived (lockfiles, manifests, generated code),
/// returns a (strategy, command) pair so the resolver can skip manual conflict resolution.
pub fn conflict_strategy(filename: &str) -> Option<(&'static str, &'static str)> {
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    let lower = basename.to_ascii_lowercase();

    // Lockfiles — accept incoming, regenerate.
    match lower.as_str() {
        "cargo.lock" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs Cargo.lock && cargo generate-lockfile",
            ));
        }
        "go.sum" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs go.sum && go mod tidy",
            ));
        }
        "package-lock.json" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs package-lock.json && npm install",
            ));
        }
        "yarn.lock" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs yarn.lock && yarn install",
            ));
        }
        "pnpm-lock.yaml" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs pnpm-lock.yaml && pnpm install",
            ));
        }
        "poetry.lock" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs poetry.lock && poetry lock",
            ));
        }
        "gemfile.lock" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs Gemfile.lock && bundle install",
            ));
        }
        "composer.lock" => {
            return Some((
                "accept_incoming_and_relock",
                "git checkout --theirs composer.lock && composer install",
            ));
        }
        _ => {}
    }

    // Dependency manifests — keep both sides, relock.
    match lower.as_str() {
        "cargo.toml" => {
            return Some((
                "keep_both_and_relock",
                "keep both dependency entries, then cargo generate-lockfile",
            ));
        }
        "go.mod" => {
            return Some((
                "keep_both_and_relock",
                "keep both require/replace entries, then go mod tidy",
            ));
        }
        "package.json" => {
            return Some((
                "keep_both_and_relock",
                "keep both dependency entries, then npm install",
            ));
        }
        "pyproject.toml" => {
            return Some((
                "keep_both_and_relock",
                "keep both dependency entries, then re-run lock command",
            ));
        }
        "gemfile" => {
            return Some((
                "keep_both_and_relock",
                "keep both gem entries, then bundle install",
            ));
        }
        _ => {}
    }

    // Generated files — accept incoming, regenerate.
    let lower_full = filename.to_ascii_lowercase();
    if lower_full.ends_with(".pb.go")
        || lower_full.ends_with(".pb.rs")
        || lower_full.ends_with("_generated.go")
        || lower_full.ends_with("_generated.rs")
        || lower_full.ends_with(".generated.ts")
        || lower_full.ends_with(".min.js")
        || lower_full.ends_with(".min.css")
    {
        return Some((
            "accept_incoming_and_regenerate",
            "git checkout --theirs <file>, then regenerate",
        ));
    }

    None
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
        let (action, sha_prefix) = action_arg
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
    let new_hunks = if had_partial {
        let (post_raw, _) = diff_with_untracked(dir, &[])?;
        let all = diff::parse_diff(&post_raw)?;
        all.into_iter()
            .filter(|h| affected_files.contains(h.file.as_str()))
            .collect()
    } else {
        Vec::new()
    };
    emit_result(
        out,
        cli.json,
        "stashed",
        selected.len(),
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
    let target = git::rev_parse(dir, &commits[0])?;
    let sources: Vec<String> = commits[1..]
        .iter()
        .map(|c| git::rev_parse(dir, c))
        .collect::<Result<_, _>>()?;
    git::rebase_squash(dir, &target, &sources)
        .map_err(|e| check_rebase_conflict(dir, e, cli.json))?;
    if let Some(msg) = message {
        git::commit_amend(dir, Some(msg))?;
    }
    emit_result(
        out,
        cli.json,
        "squashed",
        sources.len(),
        &format!(
            "Squashed {} commit(s) into {}",
            sources.len(),
            short_sha(&target)
        ),
        &[],
    );
    Ok(())
}

fn run_log(cli: &Cli, out: &mut Output, dir: &Path, n: usize) -> Result<(), String> {
    let raw = git::log(dir, n)?;
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
    Ok(())
}

fn run_split(out: &mut Output, dir: &Path, commit: &str) -> Result<(), String> {
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
        let mut status = serde_json::json!({
            "branch": branch,
            "rebase_in_progress": rebasing,
            "staged": staged,
            "unstaged": unstaged,
            "staged_lines": { "added": sa, "removed": sd },
            "unstaged_lines": { "added": ua, "removed": ud },
        });
        if !conflicts.is_empty() {
            let conflict_list: Vec<serde_json::Value> = conflicts
                .iter()
                .map(|(f, s)| serde_json::json!({"file": f, "status": s}))
                .collect();
            status["conflicts"] = serde_json::json!(conflict_list);
        }
        out.println(&status.to_string());
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
    let target = git::rev_parse(dir, commit)?;
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
    let refs: Vec<&diff::HunkInfo> = selected.iter().collect();
    let patch = diff::reconstruct_patch(&refs);
    git::apply_cached(dir, &patch, true)?;
    git::commit_amend_allow_empty(dir)?;
    if !is_head {
        git::checkout_head(dir)?;
        git::rebase_continue(dir).map_err(|e| check_rebase_conflict(dir, e, cli.json))?;
    }
    emit_result(
        out,
        cli.json,
        "dropped",
        selected.len(),
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
        "committed",
        total,
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
        let target = git::rev_parse(dir, rev)?;
        let head = git::rev_parse(dir, "HEAD")?;
        if target == head {
            git::commit_amend(dir, message)?;
        } else if message.is_some() {
            return Err("-m cannot be used with --commit for non-HEAD targets".to_string());
        } else {
            amended_target = short_sha(&target).to_string();
            git::commit_fixup(dir, &target)?;
            let dirty = !git::is_clean(dir)?;
            if dirty {
                git::stash_push(dir, None)?;
            }
            if let Err(e) = git::rebase_autosquash(dir, &target) {
                if dirty {
                    let _ = git::stash_pop(dir);
                }
                return Err(check_rebase_conflict(dir, e, cli.json));
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
        "amended",
        total,
        &format!("Amended {total} hunk(s) into {amended_target}"),
        &[],
    );
    Ok(())
}

fn run_reword(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    commit: &str,
    message: &str,
) -> Result<(), String> {
    let target = git::rev_parse(dir, commit)?;
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
        out.println(&serde_json::json!({ "reworded": true, "message": message }).to_string());
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
    let mut affected_files = std::collections::HashSet::new();
    if !unstaged_args.is_empty() {
        let (resolved, partial) = resolve_hunks(&unstaged, &unstaged_args, true)?;
        had_partial |= partial;
        for h in &resolved {
            affected_files.insert(h.file.clone());
        }
        let refs: Vec<&diff::HunkInfo> = resolved.iter().collect();
        let patch = diff::reconstruct_patch(&refs);
        git::apply_worktree(dir, &patch)?;
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
    let new_hunks = if had_partial {
        let (raw, _) = diff_with_untracked(dir, &[])?;
        let all = diff::parse_diff(&raw)?;
        all.into_iter()
            .filter(|h| affected_files.contains(&h.file))
            .collect()
    } else {
        Vec::new()
    };
    let total = unstaged_args.len() + cached_args.len();
    emit_result(
        out,
        cli.json,
        "reverted",
        total,
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
    let (label, key) = if unstage {
        ("Unstaged", "unstaged")
    } else {
        ("Staged", "staged")
    };
    emit_result(
        out,
        cli.json,
        key,
        total,
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
        find_hunk(&cached_hunks, hunk_id).or_else(|_| find_hunk(&unstaged_hunks, hunk_id))?;
    let resolved = resolve_show(hunk)?;
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
        Command::Log { n } => run_log(cli, &mut out, dir, *n)?,
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

/// Check if a rebase error is actually a conflict, and if so, return a
/// structured error message that includes the conflicting files.
fn check_rebase_conflict(dir: &Path, err: String, json: bool) -> String {
    if let Ok(true) = git::rebase_in_progress(dir)
        && let Ok(files) = git::conflicting_files(dir)
        && !files.is_empty()
    {
        let current_commit = git::rebase_current_commit(dir);
        let onto = git::rebase_onto(dir);
        if json {
            let file_list: Vec<serde_json::Value> = files
                .iter()
                .map(|(f, s)| serde_json::json!({"file": f, "status": s}))
                .collect();
            let mut val = serde_json::json!({
                "conflict": true,
                "conflicting_files": file_list,
                "hint": "Resolve conflicts, stage with `git add`, then run `GIT_EDITOR=true git rebase --continue`. To cancel: `git rebase --abort`."
            });
            if let Some((sha, msg)) = &current_commit {
                val["current_commit"] = serde_json::json!({"sha": sha, "message": msg});
            }
            if let Some(ref o) = onto {
                val["ours_theirs"] = serde_json::json!({
                    "ours": format!("upstream ({o})"),
                    "theirs": "your commit being replayed",
                });
            }
            return val.to_string();
        }
        let mut msg = String::new();
        if let Some((sha, subject)) = &current_commit {
            msg.push_str(&format!("Replaying: {sha:.8} {subject}\n"));
        }
        msg.push_str("Conflict during rebase:\n");
        for (f, s) in &files {
            if let Some((_, command)) = conflict_strategy(f) {
                msg.push_str(&format!("  {s}: {f}  → {command}\n"));
            } else {
                msg.push_str(&format!("  {s}: {f}\n"));
            }
        }
        if let Some(ref o) = onto {
            msg.push_str(&format!(
                "Note: \"ours\" = upstream ({o}), \"theirs\" = your commit\n"
            ));
        }
        msg.push_str("Resolve conflicts, stage with `git add`, then run `GIT_EDITOR=true git rebase --continue`. To cancel: `git rebase --abort`.");
        return msg;
    }
    err
}

/// Parse diff, resolve hunk IDs, build patch, and apply to index. Returns hunk count.
fn stage_hunks(
    dir: &Path,
    raw: &str,
    hunk_ids: &[String],
    reverse: bool,
) -> Result<(usize, Vec<diff::HunkInfo>), String> {
    let hunks = diff::parse_diff(raw)?;
    let (selected, had_line_selectors) = resolve_hunks(&hunks, hunk_ids, reverse)?;
    let refs: Vec<&diff::HunkInfo> = selected.iter().collect();
    let patch = diff::reconstruct_patch(&refs);
    git::apply_cached(dir, &patch, reverse)?;

    let new_hunks = if had_line_selectors {
        // Re-diff affected files to get accurate post-apply hunk IDs.
        let files: std::collections::HashSet<&str> =
            selected.iter().map(|h| h.file.as_str()).collect();
        let source_raw = if reverse {
            git::diff(dir, &["--cached".to_string()])?
        } else {
            let (r, _) = diff_with_untracked(dir, &[])?;
            r
        };
        let all = diff::parse_diff(&source_raw)?;
        all.into_iter()
            .filter(|h| files.contains(h.file.as_str()))
            .collect()
    } else {
        Vec::new()
    };

    Ok((selected.len(), new_hunks))
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
        let (hunks_to_apply, _) = resolve_hunks(&unstaged, &to_stage, false)?;
        let refs: Vec<&diff::HunkInfo> = hunks_to_apply.iter().collect();
        let patch = diff::reconstruct_patch(&refs);
        git::apply_cached(dir, &patch, false)?;
    }
    Ok(hunk_ids.len())
}

fn emit_result(
    out: &mut Output,
    json: bool,
    key: &str,
    count: usize,
    msg: &str,
    new_hunks: &[diff::HunkInfo],
) {
    if json {
        let mut val = serde_json::json!({ key: count, "message": msg });
        if !new_hunks.is_empty() {
            val["new_hunks"] = serde_json::json!(
                new_hunks
                    .iter()
                    .map(|h| {
                        let mut obj = serde_json::json!({
                            "id": h.id,
                            "file": h.file,
                            "old_range": h.old_range,
                            "new_range": h.new_range,
                            "line_hashes": h.line_hashes,
                        });
                        if let Some(ref header) = h.header {
                            obj["header"] = serde_json::json!(header);
                        }
                        obj
                    })
                    .collect::<Vec<_>>()
            );
        }
        out.println(&val.to_string());
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
) -> Result<(Vec<diff::HunkInfo>, bool), String> {
    let mut selected = Vec::new();
    let mut had_line_selectors = false;
    for arg in hunk_ids {
        if let Some((id, selector)) = arg.split_once(':') {
            let hunk = find_hunk(hunks, id)?;
            let line_hashes = resolve_selector(hunk, selector)?;
            let refs: Vec<&str> = line_hashes.iter().map(|s| s.as_str()).collect();
            let sub = diff::select_lines(hunk, &refs, reverse)?;
            if sub.id != hunk.id {
                had_line_selectors = true;
            }
            selected.push(sub);
        } else {
            let hunk = find_hunk(hunks, arg)?;
            selected.push(hunk.clone());
        }
    }
    Ok((selected, had_line_selectors))
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
