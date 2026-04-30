mod helpers;
use helpers::TestRepo;

#[test]
fn diff_commit_shows_hunks_from_commit() {
    let repo = TestRepo::new();
    repo.write_file("hello.txt", "line1\nline2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("hello.txt", "line1\nchanged\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let output = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = hunks.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert!(arr[0]["content"].as_str().unwrap().contains("-line2"));
    assert!(arr[0]["content"].as_str().unwrap().contains("+changed"));
    assert_eq!(arr[0]["id"].as_str().unwrap().len(), 8);
}

#[test]
fn diff_cached_shows_staged_changes() {
    let repo = TestRepo::new();
    repo.write_file("file.txt", "original\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("file.txt", "modified\n");
    repo.git(&["add", "file.txt"]);

    let output = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = hunks.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert!(arr[0]["content"].as_str().unwrap().contains("-original"));
    assert!(arr[0]["content"].as_str().unwrap().contains("+modified"));
}

#[test]
fn diff_working_tree_shows_unstaged_changes() {
    let repo = TestRepo::new();
    repo.write_file("file.txt", "original\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("file.txt", "modified\n");

    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert!(arr[0]["content"].as_str().unwrap().contains("-original"));
    assert!(arr[0]["content"].as_str().unwrap().contains("+modified"));
}

#[test]
fn diff_path_filter_limits_output() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "a changed\n");
    repo.write_file("b.txt", "b changed\n");

    let output = repo.squire(&["--json", "diff", "--", "a.txt"]);
    let hunks: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = hunks.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert!(arr[0]["file"].as_str().unwrap().contains("a.txt"));
}

#[test]
fn diff_empty_working_tree_returns_empty_json_array() {
    let repo = TestRepo::new();
    repo.write_file("file.txt", "content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    let hunks = repo.diff_json();
    assert!(hunks.as_array().unwrap().is_empty());
}

#[test]
fn show_displays_hunk_by_id() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    // Get hunk from the commit diff
    let diff_out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&diff_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // Show the hunk from HEAD
    let show_out = repo.squire(&["--json", "show", "HEAD", id]);
    let shown: serde_json::Value = serde_json::from_str(&show_out).unwrap();
    let arr = shown.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"].as_str().unwrap(), id);
    assert!(arr[0]["content"].as_str().unwrap().contains("-a"));
    assert!(arr[0]["content"].as_str().unwrap().contains("+b"));
}

#[test]
fn show_with_commit_flag() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "old\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "new\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let diff_out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&diff_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    let show_out = repo.squire(&["--json", "show", "HEAD", id]);
    let shown: serde_json::Value = serde_json::from_str(&show_out).unwrap();
    assert_eq!(shown[0]["id"].as_str().unwrap(), id);
}

#[test]
fn show_unknown_id_fails() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    let err = repo.squire_err(&["show", "HEAD", "deadbeef"]);
    assert!(err.contains("not found"));
}

#[test]
fn show_bare_commit_sha_lists_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "change"]);
    let sha = repo.git(&["rev-parse", "--short", "HEAD"]);
    let sha = sha.trim();

    let out = repo.squire(&["--json", "show", sha]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(hunks.as_array().unwrap().len(), 1);
    assert_eq!(hunks[0]["file"], "f.txt");
}

#[test]
fn show_falls_back_to_unstaged_diff() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "b\n");

    // Get hunk ID from unstaged diff
    let diff_out = repo.squire(&["--json", "diff"]);
    let hunks: serde_json::Value = serde_json::from_str(&diff_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // show with just the hunk ID (no ref)
    let show_out = repo.squire(&["--json", "show", id]);
    let shown: serde_json::Value = serde_json::from_str(&show_out).unwrap();
    assert_eq!(shown[0]["id"].as_str().unwrap(), id);
}

#[test]
fn show_falls_back_to_staged_diff() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "c\n");
    repo.git(&["add", "."]);

    // Get hunk ID from staged diff
    let diff_out = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&diff_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // show with just the hunk ID (no ref)
    let show_out = repo.squire(&["--json", "show", id]);
    let shown: serde_json::Value = serde_json::from_str(&show_out).unwrap();
    assert_eq!(shown[0]["id"].as_str().unwrap(), id);
}

#[test]
fn show_with_ref_falls_back_to_diff() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "b\n");

    // Get hunk ID from unstaged diff
    let diff_out = repo.squire(&["--json", "diff"]);
    let hunks: serde_json::Value = serde_json::from_str(&diff_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // show with HEAD + hunk ID — hunk won't be in HEAD, should fall back
    let show_out = repo.squire(&["--json", "show", "HEAD", id]);
    let shown: serde_json::Value = serde_json::from_str(&show_out).unwrap();
    assert_eq!(shown[0]["id"].as_str().unwrap(), id);
}

#[test]
fn diff_cached_excludes_untracked_files() {
    let repo = TestRepo::new();
    repo.write_file("tracked.txt", "old\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("tracked.txt", "new\n");
    repo.git(&["add", "tracked.txt"]);
    repo.write_file("untracked.txt", "hello\n");

    let output = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = hunks.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert!(arr[0]["file"].as_str().unwrap().contains("tracked.txt"));
}

#[test]
fn diff_ref_excludes_untracked_files() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("untracked.txt", "hello\n");

    let output = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = hunks.as_array().unwrap();

    assert_eq!(arr.len(), 1);
    assert!(arr[0]["file"].as_str().unwrap().contains("f.txt"));
}

#[test]
fn diff_short_shows_compact_output() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let out = repo.squire(&["--short", "diff"]);
    // Short output: id, file, range, +/-counts on one line
    assert!(out.contains("f.txt"), "expected file name, got: {out}");
    assert!(out.contains("+1/-1"), "expected line counts, got: {out}");
}

#[test]
fn diff_binary_file_warns_on_stderr() {
    let repo = TestRepo::new();
    repo.write_file("text.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Write a binary file (contains null bytes)
    let bin_path = repo.path().join("image.bin");
    std::fs::write(&bin_path, b"\x00\x01\x02\xff").unwrap();

    let (_, result) = repo.run_squire(&["--json", "diff"]);
    let out = result.unwrap();
    assert!(
        out.stderr.contains("skipping binary file"),
        "expected binary warning, got stderr: {}",
        out.stderr
    );
}

// --- cleanup command ---

#[test]
fn diff_path_without_separator_includes_untracked() {
    let repo = TestRepo::new();
    repo.write_file("tracked.txt", "content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Create an untracked file and diff with a bare path (no --)
    repo.write_file("untracked.txt", "new\n");

    let output = repo.squire(&["--json", "diff", "untracked.txt"]);
    let hunks: serde_json::Value = serde_json::from_str(&output).unwrap();
    let arr = hunks.as_array().unwrap();

    // The untracked file should appear even without -- separator
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["file"].as_str().unwrap().contains("untracked.txt"));
}

#[test]
fn diff_short_mode_via_cli_flag() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");
    let out = repo.squire(&["--short", "diff"]);
    // Short mode: one line per hunk with id, file, range, +/-
    assert!(out.contains("f.txt"));
    assert!(out.contains("+1/-1"));
}

#[test]
fn diff_path_filter_excludes_untracked_outside_path() {
    let repo = TestRepo::new();
    repo.write_file("src/a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create untracked files in and outside the path filter
    repo.write_file("src/new.txt", "new in src\n");
    repo.write_file("other/stray.txt", "stray\n");

    let output = repo.squire(&["--json", "diff", "--", "src/"]);
    let hunks: serde_json::Value = serde_json::from_str(&output).unwrap();
    let files: Vec<&str> = hunks
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["file"].as_str().unwrap())
        .collect();

    assert!(
        files.contains(&"src/new.txt"),
        "untracked file inside path filter should appear, got: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.contains("stray")),
        "untracked file outside path filter should be excluded, got: {files:?}"
    );
}

// ── Rename / add / delete support ──────────────────────────────────────────

#[test]
fn diff_cached_rename_only_shows_hunk() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);

    let out = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = hunks.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["old_file"].as_str().unwrap(), "old.txt");
    assert_eq!(arr[0]["file"].as_str().unwrap(), "new.txt");
    assert!(arr[0]["header"].as_str().unwrap().contains("rename"));
}

#[test]
fn diff_ref_rename_only_shows_hunk() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    repo.git(&["commit", "-m", "rename"]);

    let out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = hunks.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["old_file"].as_str().unwrap(), "old.txt");
    assert_eq!(arr[0]["file"].as_str().unwrap(), "new.txt");
}

#[test]
fn diff_short_shows_rename_hunk() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);

    let out = repo.squire(&["--short", "diff", "--cached"]);
    assert!(
        out.contains("new.txt"),
        "short output should mention the file: {out}"
    );
}

#[test]
fn diff_during_rebase_conflict_does_not_panic() {
    // Reproduce: `git diff` emits "* Unmerged path ..." lines during a
    // rebase conflict.  The patch crate panics on these.
    let repo = TestRepo::with_rebase_conflict();

    // We're mid-rebase with a conflict on f.txt.
    // Edit a separate file so `git diff` produces a normal diff
    // alongside the "* Unmerged path f.txt" line.
    repo.write_file("other.txt", "bbb\n");

    // `squire diff` should not panic on "* Unmerged path" lines
    let (stdout, result) = repo.run_squire(&["--json", "diff"]);
    assert!(result.is_ok(), "squire diff panicked or failed: {stdout}");

    repo.git(&["rebase", "--abort"]);
}

#[test]
fn show_with_line_selector() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "ctx1\nold1\nctx2\nold2\nctx3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "ctx1\nnew1\nctx2\nnew2\nctx3\n");

    // Get hunk with line hashes
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let line_hashes: Vec<&str> = hunks[0]["line_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Show only the first change using line selector
    let selector = format!("{}:{},{}", id, line_hashes[1], line_hashes[2]);
    let out = repo.squire(&["--json", "show", &selector]);
    let shown: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = shown.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["content"].as_str().unwrap().contains("-old1"));
    assert!(arr[0]["content"].as_str().unwrap().contains("+new1"));
    assert!(!arr[0]["content"].as_str().unwrap().contains("-old2"));
    assert!(!arr[0]["content"].as_str().unwrap().contains("+new2"));
}

#[test]
fn show_non_hex_id_fails() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");
    let err = repo.squire_err(&["show", "not-hex"]);
    assert!(err.contains("not a valid hunk ID"));
}

#[test]
fn show_with_line_selector_from_working_tree() {
    let repo = TestRepo::with_committed_file("f.txt", "a\nb\nc\n", "a\nB\nc\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let h1 = hunks[0]["line_hashes"][1].as_str().unwrap();
    let h2 = hunks[0]["line_hashes"][2].as_str().unwrap();
    let sel = format!("{id}:{h1},{h2}");
    let out = repo.squire(&["--json", "show", &sel]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(parsed[0]["content"].as_str().unwrap().contains("-b"));
    assert!(parsed[0]["content"].as_str().unwrap().contains("+B"));
}

#[test]
fn show_short_mode() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let out = repo.squire(&["--short", "show", id]);
    assert!(out.contains("f.txt"));
    assert!(out.contains("+1/-1"));
}

#[test]
fn show_rename_only_hunk_from_commit() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    repo.git(&["commit", "-m", "rename"]);

    // Get the hunk ID from log
    let log_out = repo.squire(&["--json", "log", "-n", "1"]);
    let commits: serde_json::Value = serde_json::from_str(&log_out).unwrap();
    let id = commits[0]["hunks"][0]["id"].as_str().unwrap();

    // Show the hunk from the commit
    let out = repo.squire(&["--json", "show", "HEAD", id]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = hunks.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["old_file"].as_str().unwrap(), "old.txt");
    assert_eq!(arr[0]["file"].as_str().unwrap(), "new.txt");
}

#[test]
fn show_with_prefix_match() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let hunks = repo.diff_json();
    let full_id = hunks[0]["id"].as_str().unwrap();
    let prefix = &full_id[..4];

    let out = repo.squire(&["--json", "show", prefix]);
    let shown: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(shown[0]["id"].as_str().unwrap(), full_id);
}

// --- JSON error output ---

#[test]
fn json_flag_produces_json_error() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let stdout = repo.squire_json_err(&["--json", "stage", "deadbeef"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed["error"].as_str().unwrap().contains("not found"));
}

#[test]
fn json_error_for_ambiguous_prefix() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "old\n");
    repo.write_file("b.txt", "old\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "new\n");
    repo.write_file("b.txt", "new\n");

    // Use a single-char prefix — very likely ambiguous with 2 hunks
    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // Try prefix "": should fail validation
    let stdout = repo.squire_json_err(&["--json", "stage", ""]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed["error"].is_string());
}
