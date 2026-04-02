use crate::cli::Cli;
use crate::{Output, conflict_strategy, git};
use std::path::Path;

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
        out.println(
            &serde_json::to_string_pretty(&serde_json::json!({
                "state": "ready",
                "branch": branch,
                "upstream": upstream,
                "commits_ahead": ahead,
                "commits_behind": behind,
                "safety_tag": tag_name,
                "steps": [
                    format!("GIT_EDITOR=true git rebase --empty=drop {upstream}"),
                    "squire rebase   # follow conflict instructions if any",
                ],
            }))
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
        out.println(
            &serde_json::to_string_pretty(&serde_json::json!({
                "state": "up_to_date",
                "branch": branch,
                "upstream": upstream,
                "commits_ahead": ahead,
                "commits_behind": 0,
            }))
            .map_err(|e| format!("failed to serialize JSON: {e}"))?,
        );
    } else if ahead > 0 {
        out.println(&format!(
            "Branch {branch} is up to date with {upstream} ({ahead} unpushed commit(s))."
        ));
    } else {
        out.println(&format!("Branch {branch} is up to date with {upstream}."));
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
        let mut val = serde_json::json!({
            "state": "rebasing",
            "branch": branch,
        });
        if let Some((cur, end)) = progress {
            val["step"] = serde_json::json!(cur);
            val["total_steps"] = serde_json::json!(end);
        }
        if let Some((sha, msg)) = &current_commit {
            val["current_commit"] = serde_json::json!({"sha": sha, "message": msg});
        }
        if let Some(ref o) = onto {
            val["ours_theirs"] = serde_json::json!({
                "ours": format!("upstream ({o}) — the base you are rebasing onto"),
                "theirs": format!("your commit being replayed from {branch}"),
            });
        }
        if conflicts.is_empty() {
            val["steps"] =
                serde_json::json!(["GIT_EDITOR=true git rebase --continue", "squire rebase",]);
        } else {
            val["conflicts"] = serde_json::json!(
                conflicts
                    .iter()
                    .map(|(f, s)| {
                        let mut c = serde_json::json!({"file": f, "status": s});
                        if let Some((strategy, command)) = conflict_strategy(f) {
                            c["strategy"] = serde_json::json!(strategy);
                            c["command"] = serde_json::json!(command);
                        }
                        c
                    })
                    .collect::<Vec<_>>()
            );
            val["conflict_rules"] = serde_json::json!({
                "imports_includes": "keep both sides, then run a formatter or linter to clean up",
                "lockfiles": "take incoming, then re-run the lock command",
                "generated_files": "accept incoming, then regenerate",
                "non_trivial": "show the diff and ask for guidance",
            });
            val["steps"] = serde_json::json!([
                "resolve conflicts using rules above",
                "git add <resolved files>",
                "run tests and fix any failures",
                "GIT_EDITOR=true git rebase --continue",
                "squire rebase",
            ]);
        }
        out.println(
            &serde_json::to_string_pretty(&val)
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
        for (f, s) in &conflicts {
            if let Some((_, command)) = conflict_strategy(f) {
                out.println(&format!("  {s}: {f}  → {command}"));
            } else {
                out.println(&format!("  {s}: {f}"));
            }
        }
        out.println("");
        out.println("Conflict resolution rules:");
        out.println(
            "  - Imports/includes: keep both sides, then run a formatter or linter to clean up",
        );
        out.println("  - Lockfiles: take incoming, then re-run the lock command");
        out.println("  - Generated files: accept incoming, then regenerate");
        out.println("  - Non-trivial: show the diff and ask for guidance");
        out.println("");
        out.println("Next steps:");
        out.println("  1. Resolve conflicts using rules above");
        out.println("  2. git add <resolved files>");
        out.println("  3. Run tests and fix any failures");
        out.println("  4. GIT_EDITOR=true git rebase --continue");
        out.println("  5. squire rebase   # check for more conflicts");
    }
    Ok(())
}
