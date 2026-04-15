use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Resolve the path to the squire binary for use as GIT_SEQUENCE_EDITOR.
/// Checks next to current_exe and one directory up (for cargo test, where
/// the test binary is in target/debug/deps/ but squire is in target/debug/).
fn squire_exe() -> Result<PathBuf, String> {
    let current =
        std::env::current_exe().map_err(|e| format!("failed to resolve current exe: {e}"))?;
    for dir in current.ancestors().skip(1).take(2) {
        let candidate = dir.join("squire");
        if candidate.exists() && candidate != current {
            return Ok(candidate);
        }
    }
    Ok(current)
}

/// Run a git subcommand with args in the given directory and return stdout.
fn git_cmd(dir: &Path, subcmd: &str, args: &[String]) -> Result<String, String> {
    let output = Command::new("git")
        .arg(subcmd)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git failed: {stderr}"));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("invalid utf-8 from git: {e}"))
}

/// Check if a string resolves to a git ref.
pub fn is_ref(dir: &Path, s: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", s])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|st| st.success())
}

pub fn diff(dir: &Path, args: &[String]) -> Result<String, String> {
    git_cmd(dir, "diff", args)
}

pub fn show(dir: &Path, args: &[String]) -> Result<String, String> {
    git_cmd(dir, "show", args)
}

/// List untracked files (not ignored, not staged).
pub fn list_untracked(dir: &Path) -> Result<Vec<String>, String> {
    let raw = git_cmd(
        dir,
        "ls-files",
        &["--others".to_string(), "--exclude-standard".to_string()],
    )?;
    Ok(raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Pipe a patch to `git apply` with the given extra flags.
pub fn apply(dir: &Path, patch: &str, extra_args: &[&str]) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new("git")
        .arg("apply")
        .args(["--unidiff-zero", "--whitespace=nowarn"])
        .args(extra_args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run git apply: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("failed to open stdin for git apply")?
        .write_all(patch.as_bytes())
        .map_err(|e| format!("failed to write patch: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("git apply failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git apply failed: {stderr}"));
    }
    Ok(())
}

/// Pipe a patch to `git apply --cached`, optionally reversed.
pub fn apply_cached(dir: &Path, patch: &str, reverse: bool) -> Result<(), String> {
    let mut args = vec!["--cached"];
    if reverse {
        args.push("--reverse");
    }
    apply(dir, patch, &args)
}

/// Pipe a patch to `git apply --reverse` against the working tree.
pub fn apply_worktree(dir: &Path, patch: &str) -> Result<(), String> {
    apply(dir, patch, &["--reverse"])
}

/// True if the working tree and index are clean.
pub fn is_clean(dir: &Path) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("failed to run git status: {e}"))?;
    Ok(output.stdout.is_empty())
}

/// Return the current branch name, or "HEAD" if detached.
pub fn branch(dir: &Path) -> Result<String, String> {
    git_cmd(
        dir,
        "rev-parse",
        &["--abbrev-ref".to_string(), "HEAD".to_string()],
    )
    .or_else(|_| {
        git_cmd(
            dir,
            "symbolic-ref",
            &["--short".to_string(), "HEAD".to_string()],
        )
    })
    .map(|s| s.trim().to_string())
}

/// Resolve the .git directory path.
fn git_dir(dir: &Path) -> Result<std::path::PathBuf, String> {
    let raw =
        git_cmd(dir, "rev-parse", &["--git-dir".to_string()]).map(|s| s.trim().to_string())?;
    Ok(if Path::new(&raw).is_absolute() {
        std::path::PathBuf::from(raw)
    } else {
        dir.join(raw)
    })
}

/// True if an interactive rebase is in progress.
pub fn rebase_in_progress(dir: &Path) -> Result<bool, String> {
    let gd = git_dir(dir)?;
    Ok(gd.join("rebase-merge").exists() || gd.join("rebase-apply").exists())
}

/// Return the SHA and subject of the commit currently being replayed during a rebase.
/// Returns None if not mid-rebase or the info isn't available.
pub fn rebase_current_commit(dir: &Path) -> Option<(String, String)> {
    let gd = git_dir(dir).ok()?;
    // rebase-merge/stopped-sha is written when the rebase pauses (conflict or edit).
    let sha_file = gd.join("rebase-merge/stopped-sha");
    let sha = std::fs::read_to_string(sha_file).ok()?.trim().to_string();
    if sha.is_empty() {
        return None;
    }
    let msg = git_cmd(
        dir,
        "log",
        &["--format=%s".to_string(), "-1".to_string(), sha.clone()],
    )
    .ok()?
    .trim()
    .to_string();
    Some((sha, msg))
}

/// Return (current_step, total_steps) for the in-progress rebase.
pub fn rebase_progress(dir: &Path) -> Option<(usize, usize)> {
    let gd = git_dir(dir).ok()?;
    let cur: usize = std::fs::read_to_string(gd.join("rebase-merge/msgnum"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let end: usize = std::fs::read_to_string(gd.join("rebase-merge/end"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some((cur, end))
}

/// Return the onto ref for the current rebase, if available.
pub fn rebase_onto(dir: &Path) -> Option<String> {
    let gd = git_dir(dir).ok()?;
    let sha = std::fs::read_to_string(gd.join("rebase-merge/onto"))
        .ok()?
        .trim()
        .to_string();
    if sha.is_empty() {
        return None;
    }
    // Try to resolve to a friendly name.
    git_cmd(dir, "name-rev", &["--name-only".to_string(), sha.clone()])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "undefined")
        .or(Some(sha))
}

/// Return the absolute path to the repository root.
pub fn toplevel(dir: &Path) -> Result<String, String> {
    git_cmd(dir, "rev-parse", &["--show-toplevel".to_string()]).map(|s| s.trim().to_string())
}

/// Return `git log` output with patch diffs.
pub fn log(dir: &Path, n: usize) -> Result<String, String> {
    let output = Command::new("git")
        .args([
            "log",
            "--format=%H%x00%an%x00%aI%x00%s%x00%D",
            "-p",
            &format!("-{n}"),
        ])
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("failed to run git log: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git log failed: {stderr}"));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("invalid utf-8 from git log: {e}"))
}

/// Resolve a commit-ish to a full SHA.
pub fn rev_parse(dir: &Path, rev: &str) -> Result<String, String> {
    git_cmd(dir, "rev-parse", &[rev.to_string()]).map(|s| s.trim().to_string())
}

/// Mixed reset to the given ref.
pub fn reset_mixed(dir: &Path, rev: &str) -> Result<(), String> {
    git_cmd(dir, "reset", &[rev.to_string()])?;
    Ok(())
}

/// Reset working tree to match HEAD (checkout all files).
pub fn checkout_head(dir: &Path) -> Result<(), String> {
    git_cmd(
        dir,
        "checkout",
        &["HEAD".to_string(), "--".to_string(), ".".to_string()],
    )?;
    Ok(())
}

/// Create a commit with the given message.
pub fn commit(dir: &Path, message: &str) -> Result<(), String> {
    git_cmd(dir, "commit", &["-m".to_string(), message.to_string()])?;
    Ok(())
}

/// Amend the current commit. Replaces message if provided, otherwise keeps it.
pub fn commit_amend(dir: &Path, message: Option<&str>) -> Result<(), String> {
    commit_amend_opts(dir, message, false)
}

pub fn commit_amend_allow_empty(dir: &Path) -> Result<(), String> {
    commit_amend_opts(dir, None, true)
}

fn commit_amend_opts(dir: &Path, message: Option<&str>, allow_empty: bool) -> Result<(), String> {
    let mut args = vec!["--amend".to_string()];
    if allow_empty {
        args.push("--allow-empty".to_string());
    }
    match message {
        Some(msg) => args.extend(["-m".to_string(), msg.to_string()]),
        None => args.push("--no-edit".to_string()),
    }
    git_cmd(dir, "commit", &args)?;
    Ok(())
}

/// Create a fixup commit targeting `target_sha` and autosquash-rebase it in.
pub fn commit_fixup(dir: &Path, target_sha: &str) -> Result<(), String> {
    git_cmd(
        dir,
        "commit",
        &["--fixup".to_string(), target_sha.to_string()],
    )
    .map(|_| ())
}

pub fn rebase_autosquash(dir: &Path, target_sha: &str) -> Result<(), String> {
    let parent = format!("{target_sha}~1");
    let output = Command::new("git")
        .args(["rebase", "-i", "--autosquash", &parent])
        .current_dir(dir)
        .env("GIT_SEQUENCE_EDITOR", "true")
        .output()
        .map_err(|e| format!("failed to run git rebase: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git rebase --autosquash failed: {stderr}"));
    }
    Ok(())
}

/// Non-interactive rebase with seqedit actions.
fn rebase_seqedit(dir: &Path, parent: &str, actions: &[String]) -> Result<(), String> {
    let exe = squire_exe()?;
    let mut editor_args = vec![exe.display().to_string(), "seqedit".to_string()];
    editor_args.extend_from_slice(actions);
    let editor_script = editor_args.join(" ");
    let output = Command::new("git")
        .args(["rebase", "-i", parent])
        .current_dir(dir)
        .env("GIT_SEQUENCE_EDITOR", &editor_script)
        .output()
        .map_err(|e| format!("failed to run git rebase: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git rebase failed: {stderr}"));
    }
    Ok(())
}

/// Non-interactive rebase that marks source commits as fixup onto the target.
/// When `message` is provided, the target commit is also reworded in the same
/// rebase pass so the message lands on the target rather than HEAD.
pub fn rebase_squash(
    dir: &Path,
    target: &str,
    sources: &[String],
    message: Option<&str>,
) -> Result<(), String> {
    let mut actions: Vec<String> = sources.iter().map(|s| format!("fixup:{s}")).collect();
    if message.is_some() {
        actions.push(format!("reword:{target}"));
    }
    let parent = format!("{target}~1");
    if let Some(msg) = message {
        let exe = squire_exe()?;
        let mut editor_args = vec![exe.display().to_string(), "seqedit".to_string()];
        editor_args.extend(actions.iter().cloned());
        let seq_editor = editor_args.join(" ");
        let msg_file = tempfile::Builder::new()
            .prefix("squire-squash-")
            .tempfile()
            .map_err(|e| format!("failed to create temp file: {e}"))?;
        std::fs::write(msg_file.path(), msg)
            .map_err(|e| format!("failed to write squash message: {e}"))?;
        let git_editor = format!("cp {}", msg_file.path().display());
        let output = Command::new("git")
            .args(["rebase", "-i", &parent])
            .current_dir(dir)
            .env("GIT_SEQUENCE_EDITOR", &seq_editor)
            .env("GIT_EDITOR", &git_editor)
            .output()
            .map_err(|e| format!("failed to run git rebase: {e}"))?;
        drop(msg_file);
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git rebase failed: {stderr}"));
        }
        Ok(())
    } else {
        rebase_seqedit(dir, &parent, &actions)
    }
}

/// Non-interactive rebase that marks `commit` as "edit" and stops there,
/// then does a mixed reset so changes are unstaged.
pub fn rebase_edit_and_reset(dir: &Path, commit: &str) -> Result<(), String> {
    rebase_edit(dir, commit)?;
    reset_mixed(dir, "HEAD~1")
}

/// Non-interactive rebase that marks `commit` as "edit" and stops there.
pub fn rebase_edit(dir: &Path, commit: &str) -> Result<(), String> {
    let parent = format!("{commit}~1");
    rebase_seqedit(dir, &parent, &[format!("edit:{commit}")])
}

/// Continue an in-progress rebase.
pub fn rebase_continue(dir: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["rebase", "--continue"])
        .current_dir(dir)
        .env("GIT_EDITOR", "true")
        .output()
        .map_err(|e| format!("failed to run git rebase: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git rebase --continue failed: {stderr}"));
    }
    Ok(())
}

/// Non-interactive rebase that rewords a commit's message.
/// Uses seqedit to mark the commit as "reword" and a temp file + `cp` as
/// GIT_EDITOR to avoid shell-injection risks from arbitrary message content.
pub fn rebase_reword(dir: &Path, commit: &str, message: &str) -> Result<(), String> {
    let exe = squire_exe()?;
    let seq_editor = format!("{} seqedit reword:{commit}", exe.display());
    let parent = format!("{commit}~1");
    // Write message to a NamedTempFile; use `cp <path>` as GIT_EDITOR so
    // git invokes `cp <path> <editor-file>` — no shell escaping needed.
    let msg_file = tempfile::Builder::new()
        .prefix("squire-reword-")
        .tempfile()
        .map_err(|e| format!("failed to create temp file: {e}"))?;
    std::fs::write(msg_file.path(), message)
        .map_err(|e| format!("failed to write reword message: {e}"))?;
    let git_editor = format!("cp {}", msg_file.path().display());
    let output = Command::new("git")
        .args(["rebase", "-i", &parent])
        .current_dir(dir)
        .env("GIT_SEQUENCE_EDITOR", &seq_editor)
        .env("GIT_EDITOR", &git_editor)
        .output()
        .map_err(|e| format!("failed to run git rebase: {e}"))?;
    drop(msg_file);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git rebase failed: {stderr}"));
    }
    Ok(())
}

/// Stash all working tree changes.
pub fn stash_push(dir: &Path, message: Option<&str>) -> Result<(), String> {
    let mut args = vec!["stash", "push", "-u"];
    if let Some(m) = message {
        args.extend(["-m", m]);
    }
    let output = Command::new("git")
        .args(&args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run git stash: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git stash failed: {stderr}"));
    }
    Ok(())
}

pub fn stash_pop(dir: &Path) -> Result<(), String> {
    git_cmd(dir, "stash", &["pop".to_string()]).map(|_| ())
}

/// Apply a patch forward (not reversed) to the working tree.
pub fn apply_worktree_forward(dir: &Path, patch: &str) -> Result<(), String> {
    apply(dir, patch, &[])
}

/// Fetch from origin. Returns true if fetch succeeded, false if no remote.
pub fn fetch(dir: &Path) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run git fetch: {e}"))?;
    Ok(output.status.success())
}

/// Detect the master branch name. Checks for "main", "master", or the
/// remote HEAD default branch. When `has_remote` is true, checks origin/ refs;
/// otherwise checks local branches.
pub fn detect_master_branch(dir: &Path, has_remote: bool) -> Result<String, String> {
    if has_remote {
        // Try remote HEAD first (most reliable after fetch)
        let output = Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(dir)
            .output()
            .map_err(|e| format!("failed to detect master branch: {e}"))?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Some(name) = s.trim().strip_prefix("refs/remotes/origin/") {
                return Ok(name.to_string());
            }
        }
    }
    for name in &["main", "master", "mainline"] {
        let ref_to_check = if has_remote {
            format!("origin/{name}")
        } else {
            name.to_string()
        };
        let result = Command::new("git")
            .args(["rev-parse", "--verify", &ref_to_check])
            .current_dir(dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if let Ok(s) = result
            && s.success()
        {
            return Ok(name.to_string());
        }
    }
    Err("cannot detect master branch; use --master to specify".to_string())
}

/// List local branches that are fully merged into the given branch.
pub fn merged_branches(dir: &Path, into: &str) -> Result<Vec<String>, String> {
    let raw = git_cmd(
        dir,
        "branch",
        &[
            "--format=%(refname:short)".to_string(),
            "--merged".to_string(),
            into.to_string(),
        ],
    )?;
    Ok(raw
        .lines()
        .filter(|l| !l.is_empty() && *l != into)
        .map(String::from)
        .collect())
}

/// List all local branch names.
pub fn list_branches(dir: &Path) -> Result<Vec<String>, String> {
    let raw = git_cmd(dir, "branch", &["--format=%(refname:short)".to_string()])?;
    Ok(raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Get commits on `branch` not reachable from `base`, returning (sha, first_line_message).
pub fn commits_not_in(
    dir: &Path,
    branch: &str,
    base: &str,
) -> Result<Vec<(String, String)>, String> {
    let range = format!("{base}..{branch}");
    let raw = git_cmd(dir, "log", &["--format=%H %s".to_string(), range])?;
    Ok(raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (sha, msg) = l.split_once(' ').unwrap_or((l, ""));
            (sha.to_string(), msg.to_string())
        })
        .collect())
}

/// Get commit messages on `branch` reachable from its tip, limited to `n`.
pub fn commit_messages(dir: &Path, branch: &str, n: usize) -> Result<Vec<String>, String> {
    let raw = git_cmd(
        dir,
        "log",
        &[
            "--format=%s".to_string(),
            format!("-{n}"),
            branch.to_string(),
        ],
    )?;
    Ok(raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Return the set of SHAs on `branch` whose patches are already applied in `upstream`.
/// Uses a single `git cherry` call for all commits at once.
pub fn cherry_applied(
    dir: &Path,
    upstream: &str,
    branch: &str,
) -> Result<std::collections::HashSet<String>, String> {
    let raw = git_cmd(dir, "cherry", &[upstream.to_string(), branch.to_string()])?;
    // '-' prefix means equivalent commit exists upstream
    Ok(raw
        .lines()
        .filter(|l| l.starts_with('-'))
        .filter_map(|l| l.get(2..).map(String::from))
        .collect())
}

/// Return files with unmerged conflicts (during a paused rebase/merge).
/// Each entry is (file, status) where status is e.g. "both_modified",
/// "deleted_by_us", "deleted_by_them", "both_added".
pub fn conflicting_files(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let raw = git_cmd(
        dir,
        "status",
        &["--porcelain".to_string(), "-z".to_string()],
    )?;
    let mut result = Vec::new();
    for entry in raw.split('\0') {
        if entry.len() < 4 {
            continue;
        }
        let (xy, file) = entry.split_at(3);
        let x = xy.as_bytes()[0];
        let y = xy.as_bytes()[1];
        let status = match (x, y) {
            (b'U', b'U') => "both_modified",
            (b'A', b'A') => "both_added",
            (b'D', b'U') => "deleted_by_us",
            (b'U', b'D') => "deleted_by_them",
            (b'A', b'U') | (b'U', b'A') => "both_modified",
            _ => continue,
        };
        result.push((file.to_string(), status.to_string()));
    }
    Ok(result)
}

/// Return the ISO date of the last commit on a branch.
pub fn branch_last_commit_date(dir: &Path, branch: &str) -> Result<String, String> {
    git_cmd(
        dir,
        "log",
        &[
            "-1".to_string(),
            "--format=%aI".to_string(),
            branch.to_string(),
        ],
    )
    .map(|s| s.trim().to_string())
}

/// Count commits on the current branch ahead of the given upstream ref.
pub fn commits_ahead(dir: &Path, upstream: &str) -> Result<usize, String> {
    let raw = git_cmd(
        dir,
        "rev-list",
        &["--count".to_string(), format!("{upstream}..HEAD")],
    )?;
    raw.trim()
        .parse()
        .map_err(|e| format!("failed to parse commit count: {e}"))
}

/// Count commits on the upstream not reachable from HEAD.
pub fn commits_behind(dir: &Path, upstream: &str) -> Result<usize, String> {
    let raw = git_cmd(
        dir,
        "rev-list",
        &["--count".to_string(), format!("HEAD..{upstream}")],
    )?;
    raw.trim()
        .parse()
        .map_err(|e| format!("failed to parse commit count: {e}"))
}

/// Create a lightweight tag. Returns Ok(false) if the tag already exists.
pub fn create_tag(dir: &Path, name: &str) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["tag", name])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run git tag: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already exists") {
            return Ok(false);
        }
        return Err(format!("git tag failed: {stderr}"));
    }
    Ok(true)
}

/// Resolve the upstream tracking ref for the current branch.
/// Returns e.g. "origin/main". Falls back to origin/{branch} if no
/// tracking ref is configured but the remote ref exists, then to
/// the detected master branch (origin/main or origin/master).
pub fn upstream_ref(dir: &Path) -> Result<String, String> {
    // Try the configured upstream first.
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to resolve upstream: {e}"))?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return Ok(s);
        }
    }
    // Fall back to origin/{branch} if that ref exists.
    let current = branch(dir)?;
    let candidate = format!("origin/{current}");
    if is_ref(dir, &candidate) {
        return Ok(candidate);
    }
    // Fall back to the master branch.
    let has_remote = fetch(dir)?;
    let master = detect_master_branch(dir, has_remote)?;
    let master_ref = if has_remote {
        format!("origin/{master}")
    } else {
        master
    };
    if is_ref(dir, &master_ref) {
        return Ok(master_ref);
    }
    Err("no upstream ref found (set one with `git branch --set-upstream-to`)".to_string())
}
