use crate::cli::Cli;
use crate::{Output, git, short_sha};
use std::path::Path;

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

pub fn run_cleanup(
    cli: &Cli,
    out: &mut Output,
    dir: &Path,
    master: Option<&str>,
) -> Result<(), String> {
    let has_remote = git::fetch(dir)?;
    if !has_remote {
        out.eprintln("warning: no remote 'origin' found, comparing against local branches");
    }

    let master_branch = match master {
        Some(m) => m.to_string(),
        None => git::detect_master_branch(dir, has_remote)?,
    };
    let compare_ref = if has_remote {
        format!("origin/{master_branch}")
    } else {
        master_branch.clone()
    };

    let current = git::branch(dir)?;
    let branches = analyze_branches(dir, &master_branch, &compare_ref, &current)?;

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
        format_plain(out, &master_branch, &current, &branches);
    }
    Ok(())
}

fn analyze_branches(
    dir: &Path,
    master_branch: &str,
    compare_ref: &str,
    current: &str,
) -> Result<Vec<BranchInfo>, String> {
    let all_branches = git::list_branches(dir)?;
    let merged_set = git::merged_branches(dir, compare_ref)?;

    let master_msgs = git::commit_messages(dir, compare_ref, 500)?;
    let master_msg_set: std::collections::HashSet<&str> =
        master_msgs.iter().map(|s| s.as_str()).collect();

    let mut branches = Vec::new();

    for branch in &all_branches {
        if branch == master_branch || branch == current {
            continue;
        }

        let last_date = git::branch_last_commit_date(dir, branch).unwrap_or_default();

        let commits = if merged_set.contains(branch) {
            vec![]
        } else {
            git::commits_not_in(dir, branch, compare_ref)?
        };

        if merged_set.contains(branch) || commits.is_empty() {
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

        branches.push(classify_branch(
            dir,
            branch,
            last_date,
            &commits,
            compare_ref,
            &master_msg_set,
        )?);
    }
    Ok(branches)
}

fn classify_branch(
    dir: &Path,
    branch: &str,
    last_date: String,
    commits: &[(String, String)],
    compare_ref: &str,
    master_msg_set: &std::collections::HashSet<&str>,
) -> Result<BranchInfo, String> {
    let commit_count = commits.len();
    let mut summaries = Vec::new();
    let mut all_messages_match = true;
    let mut any_message_match = false;
    let mut all_patches_applied = true;
    let mut needs_eval = false;

    let applied_shas = git::cherry_applied(dir, compare_ref, branch).unwrap_or_default();

    for (sha, msg) in commits {
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
            sha: short_sha(sha).to_string(),
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

    Ok(BranchInfo {
        name: branch.to_string(),
        status,
        last_commit_date: last_date,
        commit_count,
        commits: summaries,
        note,
    })
}

fn format_plain(out: &mut Output, master_branch: &str, current: &str, branches: &[BranchInfo]) {
    out.println(&format!("Master branch: {master_branch}"));
    out.println(&format!("Current branch: {current}"));
    out.println("");

    if branches.is_empty() {
        out.println("No other branches found.");
    }

    for b in branches {
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
