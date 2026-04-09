//! Strongly-typed JSON response types for squire commands.

use crate::diff::HunkInfo;
use serde::Serialize;

// ── emit_result responses (stage/unstage/revert/commit/amend/drop/squash/stash) ──

/// Summary of a residual hunk after a partial line operation.
#[derive(Debug, Serialize)]
pub struct NewHunkSummary {
    pub id: String,
    pub file: String,
    pub old_range: String,
    pub new_range: String,
    pub line_hashes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
}

impl From<&HunkInfo> for NewHunkSummary {
    fn from(h: &HunkInfo) -> Self {
        Self {
            id: h.id.clone(),
            file: h.file.clone(),
            old_range: h.old_range.clone(),
            new_range: h.new_range.clone(),
            line_hashes: h.line_hashes.clone(),
            header: h.header.clone(),
        }
    }
}

/// Response for hunk-count operations (stage, unstage, commit, amend, etc.).
/// The `count_key` field is flattened so the JSON key varies by operation.
#[derive(Debug, Serialize)]
pub struct ActionResult {
    #[serde(flatten)]
    pub count: ActionCount,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub new_hunks: Vec<NewHunkSummary>,
}

/// The count field with a dynamic key name.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCount {
    Staged(usize),
    Unstaged(usize),
    Reverted(usize),
    Committed(usize),
    Amended(usize),
    Dropped(usize),
    Squashed(usize),
    Stashed(usize),
}

// ── reword ──

#[derive(Debug, Serialize)]
pub struct RewordResult {
    pub reworded: bool,
    pub message: String,
}

// ── status ──

#[derive(Debug, Serialize)]
pub struct LineCounts {
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Serialize)]
pub struct StatusResult {
    pub branch: String,
    pub rebase_in_progress: bool,
    pub staged: Vec<HunkInfo>,
    pub unstaged: Vec<HunkInfo>,
    pub staged_lines: LineCounts,
    pub unstaged_lines: LineCounts,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<ConflictFile>,
}

// ── conflict types (shared by status, rebase, check_rebase_conflict) ──

#[derive(Debug, Serialize)]
pub struct ConflictFile {
    pub file: String,
    pub status: String,
    pub strategy: String,
    pub command: String,
}

impl ConflictFile {
    pub fn with_strategy(file: &str, status: &str, strategy: &str, command: &str) -> Self {
        Self {
            file: file.to_string(),
            status: status.to_string(),
            strategy: strategy.to_string(),
            command: command.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommitRef {
    pub sha: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct OursTheirs {
    pub ours: String,
    pub theirs: String,
}

// ── check_rebase_conflict error ──

#[derive(Debug, Serialize)]
pub struct ConflictError {
    pub conflict: bool,
    pub conflicting_files: Vec<ConflictFile>,
    pub hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_commit: Option<CommitRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ours_theirs: Option<OursTheirs>,
}

// ── rebase ──

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RebaseResult {
    Ready {
        branch: String,
        upstream: String,
        commits_ahead: usize,
        commits_behind: usize,
        safety_tag: String,
        steps: Vec<String>,
    },
    UpToDate {
        branch: String,
        upstream: String,
        commits_ahead: usize,
        commits_behind: usize,
        verify: String,
    },
    Rebasing(RebaseInProgress),
}

#[derive(Debug, Serialize)]
pub struct RebaseInProgress {
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_steps: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_commit: Option<CommitRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ours_theirs: Option<OursTheirs>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<ConflictFile>,
    pub steps: Vec<String>,
}

// ── cleanup ──

#[derive(Debug, Serialize)]
pub struct CleanupResult {
    pub master_branch: String,
    pub current_branch: String,
    pub has_remote: bool,
    pub branches: Vec<super::cleanup::BranchInfo>,
}

// ── error ──

#[derive(Debug, Serialize)]
pub struct ErrorResult {
    pub error: String,
}
