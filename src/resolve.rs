use crate::{Output, diff, git};
use std::path::Path;

/// For files whose contents are fully derived (lockfiles, manifests, generated code),
/// returns a (strategy, command) pair so the resolver can skip manual conflict resolution.
pub fn conflict_strategy(filename: &str) -> Option<(&'static str, &'static str)> {
    let basename = filename.rsplit('/').next().unwrap_or(filename);
    let lower = basename.to_ascii_lowercase();

    const LOCKFILES: &[(&str, &str, &str)] = &[
        (
            "cargo.lock",
            "accept_incoming_and_relock",
            "git checkout --theirs Cargo.lock && cargo generate-lockfile",
        ),
        (
            "go.sum",
            "accept_incoming_and_relock",
            "git checkout --theirs go.sum && go mod tidy",
        ),
        (
            "package-lock.json",
            "accept_incoming_and_relock",
            "git checkout --theirs package-lock.json && npm install",
        ),
        (
            "yarn.lock",
            "accept_incoming_and_relock",
            "git checkout --theirs yarn.lock && yarn install",
        ),
        (
            "pnpm-lock.yaml",
            "accept_incoming_and_relock",
            "git checkout --theirs pnpm-lock.yaml && pnpm install",
        ),
        (
            "poetry.lock",
            "accept_incoming_and_relock",
            "git checkout --theirs poetry.lock && poetry lock",
        ),
        (
            "gemfile.lock",
            "accept_incoming_and_relock",
            "git checkout --theirs Gemfile.lock && bundle install",
        ),
        (
            "composer.lock",
            "accept_incoming_and_relock",
            "git checkout --theirs composer.lock && composer install",
        ),
    ];

    const MANIFESTS: &[(&str, &str, &str)] = &[
        (
            "cargo.toml",
            "keep_both_and_relock",
            "keep both dependency entries, then cargo generate-lockfile",
        ),
        (
            "go.mod",
            "keep_both_and_relock",
            "keep both require/replace entries, then go mod tidy",
        ),
        (
            "package.json",
            "keep_both_and_relock",
            "keep both dependency entries, then npm install",
        ),
        (
            "pyproject.toml",
            "keep_both_and_relock",
            "keep both dependency entries, then re-run lock command",
        ),
        (
            "gemfile",
            "keep_both_and_relock",
            "keep both gem entries, then bundle install",
        ),
    ];

    const GENERATED_SUFFIXES: &[&str] = &[
        ".pb.go",
        ".pb.rs",
        "_generated.go",
        "_generated.rs",
        ".generated.ts",
        ".min.js",
        ".min.css",
    ];

    for &(name, strategy, command) in LOCKFILES.iter().chain(MANIFESTS) {
        if lower == name {
            return Some((strategy, command));
        }
    }

    let lower_full = filename.to_ascii_lowercase();
    if GENERATED_SUFFIXES.iter().any(|s| lower_full.ends_with(s)) {
        return Some((
            "accept_incoming_and_regenerate",
            "git checkout --theirs <file>, then regenerate",
        ));
    }

    None
}

/// Find a hunk by exact or prefix ID match.
pub fn find_hunk<'a>(hunks: &'a [diff::HunkInfo], id: &str) -> Result<&'a diff::HunkInfo, String> {
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
pub fn resolve_hunks(
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
pub fn resolve_selector(hunk: &diff::HunkInfo, selector: &str) -> Result<Vec<String>, String> {
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

/// Build a patch from resolved hunks and apply it.
pub fn apply_resolved(dir: &Path, hunks: &[diff::HunkInfo], args: &[&str]) -> Result<(), String> {
    let refs: Vec<&diff::HunkInfo> = hunks.iter().collect();
    let patch = diff::reconstruct_patch(&refs);
    git::apply(dir, &patch, args)
}

/// Re-diff after a partial operation and return residual hunks for affected files.
/// When `cached` is true, diffs the index; otherwise diffs the working tree (including untracked).
pub fn residual_hunks(
    dir: &Path,
    had_partial: bool,
    files: &std::collections::HashSet<&str>,
    cached: bool,
) -> Result<Vec<diff::HunkInfo>, String> {
    if !had_partial {
        return Ok(Vec::new());
    }
    let raw = if cached {
        git::diff(dir, &["--cached".to_string()])?
    } else {
        let (r, _) = super::diff_with_untracked(dir, &[])?;
        r
    };
    let all = diff::parse_diff(&raw)?;
    Ok(all
        .into_iter()
        .filter(|h| files.contains(h.file.as_str()))
        .collect())
}

/// Parse diff, resolve hunk IDs, build patch, and apply to index. Returns hunk count.
pub fn stage_hunks(
    dir: &Path,
    raw: &str,
    hunk_ids: &[String],
    reverse: bool,
) -> Result<(usize, Vec<diff::HunkInfo>), String> {
    let hunks = diff::parse_diff(raw)?;
    let (selected, had_line_selectors) = resolve_hunks(&hunks, hunk_ids, reverse)?;
    let mut args = vec!["--cached"];
    if reverse {
        args.push("--reverse");
    }
    apply_resolved(dir, &selected, &args)?;

    let files: std::collections::HashSet<&str> = selected.iter().map(|h| h.file.as_str()).collect();
    let new_hunks = residual_hunks(dir, had_line_selectors, &files, reverse)?;

    Ok((selected.len(), new_hunks))
}

/// Like stage_hunks, but accepts hunks that are already staged.
/// Unstaged hunks get staged; already-staged hunks are counted but not re-applied.
pub fn stage_hunks_or_cached(dir: &Path, hunk_ids: &[String]) -> Result<usize, String> {
    let (unstaged_raw, _) = super::diff_with_untracked(dir, &[])?;
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
        apply_resolved(dir, &hunks_to_apply, &["--cached"])?;
    }
    Ok(hunk_ids.len())
}

/// Split args into (git_args, hunk_id). The hunk ID is the last arg,
/// validated as a hex string (optionally with a `:selector` suffix).
pub fn split_last_arg<'a>(
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

/// Check if a rebase error is actually a conflict, and if so, return a
/// structured error message that includes the conflicting files.
pub fn check_rebase_conflict(dir: &Path, err: String, json: bool) -> String {
    if let Ok(true) = git::rebase_in_progress(dir)
        && let Ok(files) = git::conflicting_files(dir)
        && !files.is_empty()
    {
        let current_commit = git::rebase_current_commit(dir);
        let onto = git::rebase_onto(dir);
        if json {
            let mut val = serde_json::json!({
                "conflict": true,
                "conflicting_files": crate::rebase::conflict_files_json(&files),
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
        let mut out = Output::default();
        if let Some((sha, subject)) = &current_commit {
            out.println(&format!("Replaying: {sha:.8} {subject}"));
        }
        out.println("Conflict during rebase:");
        crate::rebase::format_conflict_files(&mut out, &files);
        if let Some(ref o) = onto {
            out.println(&format!(
                "Note: \"ours\" = upstream ({o}), \"theirs\" = your commit"
            ));
        }
        out.println("Resolve conflicts, stage with `git add`, then run `GIT_EDITOR=true git rebase --continue`. To cancel: `git rebase --abort`.");
        return out.stdout.trim_end().to_string();
    }
    err
}

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
