use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
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

/// Pipe a patch to `git apply --cached`, optionally reversed.
pub fn apply_cached(dir: &Path, patch: &str, reverse: bool) -> Result<(), String> {
    use std::io::Write;
    let mut args = vec!["apply", "--cached", "--unidiff-zero", "--whitespace=nowarn"];
    if reverse {
        args.push("--reverse");
    }
    let mut child = Command::new("git")
        .args(&args)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
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
        let label = if reverse {
            "git apply --reverse"
        } else {
            "git apply"
        };
        return Err(format!("{label} failed: {stderr}"));
    }
    Ok(())
}

/// Pipe a patch to `git apply --reverse` against the working tree.
pub fn apply_worktree(dir: &Path, patch: &str) -> Result<(), String> {
    use std::io::Write;
    let args = [
        "apply",
        "--reverse",
        "--unidiff-zero",
        "--whitespace=nowarn",
    ];
    let mut child = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
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
        return Err(format!("git apply --reverse failed: {stderr}"));
    }
    Ok(())
}

/// True if the working tree and index are clean.
pub fn is_clean(dir: &Path) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
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

/// True if an interactive rebase is in progress.
pub fn rebase_in_progress(dir: &Path) -> Result<bool, String> {
    let git_dir =
        git_cmd(dir, "rev-parse", &["--git-dir".to_string()]).map(|s| s.trim().to_string())?;
    let git_dir = if Path::new(&git_dir).is_absolute() {
        std::path::PathBuf::from(git_dir)
    } else {
        dir.join(git_dir)
    };
    Ok(git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists())
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
            "--format=%H%x00%an%x00%aI%x00%s",
            "-p",
            &format!("-{n}"),
        ])
        .current_dir(dir)
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

/// Create a commit with the given message.
pub fn commit(dir: &Path, message: &str) -> Result<(), String> {
    git_cmd(dir, "commit", &["-m".to_string(), message.to_string()])?;
    Ok(())
}

/// Amend the current commit. Replaces message if provided, otherwise keeps it.
pub fn commit_amend(dir: &Path, message: Option<&str>) -> Result<(), String> {
    let mut args = vec!["--amend".to_string()];
    match message {
        Some(msg) => args.extend(["-m".to_string(), msg.to_string()]),
        None => args.push("--no-edit".to_string()),
    }
    git_cmd(dir, "commit", &args)?;
    Ok(())
}

/// Create a fixup commit targeting `target_sha` and autosquash-rebase it in.
pub fn rebase_autosquash(dir: &Path, target_sha: &str) -> Result<(), String> {
    let parent = format!("{target_sha}~1");
    git_cmd(
        dir,
        "commit",
        &["--fixup".to_string(), target_sha.to_string()],
    )?;
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

/// Non-interactive rebase that marks source commits as fixup onto the target.
pub fn rebase_squash(dir: &Path, target: &str, sources: &[String]) -> Result<(), String> {
    let exe = squire_exe()?;
    let actions: Vec<String> = sources.iter().map(|s| format!("fixup:{s}")).collect();
    let mut editor_args = vec![exe.display().to_string(), "seqedit".to_string()];
    editor_args.extend(actions);
    let editor_script = editor_args.join(" ");
    let parent = format!("{target}~1");
    let output = Command::new("git")
        .args(["rebase", "-i", &parent])
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

/// Non-interactive rebase that marks `commit` as "edit" and stops there,
/// then does a mixed reset so changes are unstaged.
pub fn rebase_edit_and_reset(dir: &Path, commit: &str) -> Result<(), String> {
    // rebase onto the parent of the target commit
    let parent = format!("{commit}~1");
    let exe = squire_exe()?;
    let editor_script = format!("{} seqedit edit:{commit}", exe.display());
    let output = Command::new("git")
        .args(["rebase", "-i", &parent])
        .current_dir(dir)
        .env("GIT_SEQUENCE_EDITOR", &editor_script)
        .output()
        .map_err(|e| format!("failed to run git rebase: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git rebase failed: {stderr}"));
    }
    reset_mixed(dir, "HEAD~1")
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
        for name in &["main", "master"] {
            let result = Command::new("git")
                .args(["rev-parse", "--verify", &format!("origin/{name}")])
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
    } else {
        for name in &["main", "master"] {
            let result = Command::new("git")
                .args(["rev-parse", "--verify", name])
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
