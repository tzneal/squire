use crate::cli::Cli;
use crate::resolve::conflict_strategy;
use crate::response;
use crate::{Output, git};
use std::path::Path;

/// Build typed conflict file list with strategy/command on every entry.
pub fn build_conflict_files(files: &[(String, String)]) -> Vec<response::ConflictFile> {
    files
        .iter()
        .map(|(f, s)| match conflict_strategy(f) {
            Some((strategy, command)) => {
                response::ConflictFile::with_strategy(f, s, strategy, command)
            }
            None => response::ConflictFile::with_strategy(
                f,
                s,
                "non_trivial",
                "show the diff and ask for guidance",
            ),
        })
        .collect()
}

/// Format conflict file list as plain text lines.
pub fn format_conflict_files(out: &mut Output, files: &[(String, String)]) {
    for (f, s) in files {
        if let Some((_, command)) = conflict_strategy(f) {
            out.println(&format!("  {s}: {f}  → {command}"));
        } else {
            out.println(&format!("  {s}: {f}  → show the diff and ask for guidance"));
        }
    }
}

pub fn run_rebase(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    onto: Option<&str>,
) -> Result<(), String> {
    let branch = git::branch(dir)?;
    let rebasing = git::rebase_in_progress(dir)?;

    if rebasing {
        return run_rebase_in_progress(cli, out, dir, &branch);
    }

    // Not rebasing — check upstream and print pre-rebase playbook.
    if !git::is_clean(dir)? {
        return Err(
            "rebase requires a clean working tree; commit or stash changes first".to_string(),
        );
    }
    let upstream = match onto {
        Some(o) => o.to_string(),
        None => git::upstream_ref(dir)?,
    };
    let ahead = git::commits_ahead(dir, &upstream)?;
    let behind = git::commits_behind(dir, &upstream)?;

    if behind == 0 {
        return run_rebase_up_to_date(cli, out, &branch, &upstream, ahead);
    }

    // Create safety tag.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut tag_name = format!("pre-rebase/{branch}-{secs}");
    if !git::create_tag(dir, &tag_name)? {
        // Tag already exists — append a suffix to make it unique.
        for i in 2.. {
            tag_name = format!("pre-rebase/{branch}-{secs}-{i}");
            if git::create_tag(dir, &tag_name)? {
                break;
            }
        }
    }

    if cli.json {
        let result = response::RebaseResult::Ready {
            branch: branch.clone(),
            upstream: upstream.clone(),
            commits_ahead: ahead,
            commits_behind: behind,
            safety_tag: tag_name.clone(),
            steps: vec![
                format!("GIT_EDITOR=true git rebase --empty=drop {upstream}"),
                "squire rebase   # follow conflict instructions if any".to_string(),
            ],
        };
        out.println(
            &serde_json::to_string_pretty(&result)
                .map_err(|e| format!("failed to serialize JSON: {e}"))?,
        );
    } else {
        out.println(&format!(
            "Branch {branch} ({ahead} commit(s) ahead of {upstream})"
        ));
        out.println("");
        out.println(&format!("Safety tag: {tag_name}"));
        out.println(&format!("  Recovery: git reset --hard {tag_name}"));
        out.println("");
        out.println("Next steps:");
        out.println(&format!(
            "  1. GIT_EDITOR=true git rebase --empty=drop {upstream}"
        ));
        out.println("  2. squire rebase   # follow conflict instructions if any");
    }
    Ok(())
}

fn run_rebase_up_to_date(
    cli: &Cli,
    out: &mut Output,
    branch: &str,
    upstream: &str,
    ahead: usize,
) -> Result<(), String> {
    if cli.json {
        let result = response::RebaseResult::UpToDate {
            branch: branch.to_string(),
            upstream: upstream.to_string(),
            commits_ahead: ahead,
            commits_behind: 0,
            verify: "if a rebase was just performed, run project tests and linter to confirm it did not break anything".to_string(),
        };
        out.println(
            &serde_json::to_string_pretty(&result)
                .map_err(|e| format!("failed to serialize JSON: {e}"))?,
        );
    } else if ahead > 0 {
        out.println(&format!(
            "Branch {branch} is up to date with {upstream} ({ahead} unpushed commit(s))."
        ));
        out.println("If a rebase was just performed, run project tests and linter to confirm it did not break anything.");
    } else {
        out.println(&format!("Branch {branch} is up to date with {upstream}."));
        out.println("If a rebase was just performed, run project tests and linter to confirm it did not break anything.");
    }
    Ok(())
}

fn run_rebase_in_progress(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    branch: &str,
) -> Result<(), String> {
    let conflicts = git::conflicting_files(dir).unwrap_or_default();
    let current_commit = git::rebase_current_commit(dir);
    let onto = git::rebase_onto(dir);
    let progress = git::rebase_progress(dir);

    if cli.json {
        let (conflict_files, steps) = if conflicts.is_empty() {
            (
                vec![],
                vec![
                    "GIT_EDITOR=true git rebase --continue".to_string(),
                    "squire rebase".to_string(),
                ],
            )
        } else {
            (
                build_conflict_files(&conflicts),
                vec![
                    "resolve conflicts per each file's recommendation".to_string(),
                    "git add <resolved files>".to_string(),
                    "run tests and fix any failures".to_string(),
                    "GIT_EDITOR=true git rebase --continue".to_string(),
                    "squire rebase".to_string(),
                ],
            )
        };
        let result = response::RebaseResult::Rebasing(response::RebaseInProgress {
            branch: branch.to_string(),
            step: progress.map(|(cur, _)| cur),
            total_steps: progress.map(|(_, end)| end),
            current_commit: current_commit
                .as_ref()
                .map(|(sha, msg)| response::CommitRef {
                    sha: sha.clone(),
                    message: msg.clone(),
                }),
            ours_theirs: onto.as_ref().map(|o| response::OursTheirs {
                ours: format!("upstream ({o}) — the base you are rebasing onto"),
                theirs: format!("your commit being replayed from {branch}"),
            }),
            conflicts: conflict_files,
            steps,
        });
        out.println(
            &serde_json::to_string_pretty(&result)
                .map_err(|e| format!("failed to serialize JSON: {e}"))?,
        );
    } else if conflicts.is_empty() {
        out.println("Rebase in progress, no conflicts.");
        out.println("");
        out.println("Next steps:");
        out.println("  GIT_EDITOR=true git rebase --continue");
        out.println("  squire rebase   # check for more conflicts");
    } else {
        if let Some((cur, end)) = progress {
            out.println(&format!("Step {cur} of {end}"));
        }
        if let Some((sha, msg)) = &current_commit {
            out.println(&format!("Replaying: {sha:.8} {msg}"));
        }
        if let Some(ref o) = onto {
            out.println(&format!(
                "Note: during rebase, \"ours\" = upstream ({o}), \"theirs\" = your commit from {branch}"
            ));
        }
        out.println(&format!(
            "Rebase in progress — {} conflict(s):",
            conflicts.len()
        ));
        format_conflict_files(out, &conflicts);
        out.println("");
        out.println("Next steps:");
        out.println("  1. Resolve conflicts per each file's recommendation");
        out.println("  2. git add <resolved files>");
        out.println("  3. Run tests and fix any failures");
        out.println("  4. GIT_EDITOR=true git rebase --continue");
        out.println("  5. squire rebase   # check for more conflicts");
    }
    Ok(())
}
