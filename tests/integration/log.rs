use crate::helpers::TestRepo;

// --- log command ---

#[test]
fn log_json_returns_commits_with_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let out = repo.squire(&["--json", "log", "-n", "2"]);
    let commits: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = commits.as_array().unwrap();

    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["message"].as_str().unwrap(), "second");
    assert_eq!(arr[1]["message"].as_str().unwrap(), "first");
    assert!(!arr[0]["sha"].as_str().unwrap().is_empty());
    assert!(!arr[0]["author"].as_str().unwrap().is_empty());
    assert!(!arr[0]["date"].as_str().unwrap().is_empty());

    // Second commit should have hunks
    let hunks = arr[0]["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert!(hunks[0]["content"].as_str().unwrap().contains("+v2"));
}

#[test]
fn log_hunk_ids_match_diff() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "old\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "new\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    // Get hunk ID from log
    let log_out = repo.squire(&["--json", "log", "-n", "1"]);
    let log_commits: serde_json::Value = serde_json::from_str(&log_out).unwrap();
    let log_hunk_id = log_commits[0]["hunks"][0]["id"].as_str().unwrap();

    // Get hunk ID from diff HEAD~1 HEAD
    let diff_out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let diff_hunks: serde_json::Value = serde_json::from_str(&diff_out).unwrap();
    let diff_hunk_id = diff_hunks[0]["id"].as_str().unwrap();

    assert_eq!(log_hunk_id, diff_hunk_id);
}

#[test]
fn log_short_shows_compact_output() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "initial commit"]);

    let out = repo.squire(&["--short", "log", "-n", "1"]);
    assert!(out.contains("initial commit"), "got: {out}");
    assert!(out.contains("hunk"), "got: {out}");
}

#[test]
fn log_plain_shows_date_and_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "test commit"]);

    let out = repo.squire(&["log", "-n", "1"]);
    assert!(out.contains("test commit"), "got: {out}");
    assert!(out.contains("f.txt"), "got: {out}");
}

#[test]
fn log_n_limits_output() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("f.txt", "v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    let out = repo.squire(&["--json", "log", "-n", "2"]);
    let commits: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(commits.as_array().unwrap().len(), 2);
    assert_eq!(commits[0]["message"].as_str().unwrap(), "third");
    assert_eq!(commits[1]["message"].as_str().unwrap(), "second");
}

#[test]
fn log_json_truncates_bulky_hunk_content_by_default() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Create a commit with >100 added lines so the default cap (100) trips.
    let mut big = String::from("seed\n");
    for i in 0..300 {
        big.push_str(&format!("line {i}\n"));
    }
    repo.write_file("f.txt", &big);
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "big"]);

    let out = repo.squire(&["--json", "log", "-n", "1"]);
    let commits: serde_json::Value = serde_json::from_str(&out).unwrap();
    let hunks = commits[0]["hunks"].as_array().unwrap();
    assert!(!hunks.is_empty());
    let h = &hunks[0];
    // Summary fields preserved.
    assert!(!h["id"].as_str().unwrap().is_empty());
    assert_eq!(h["file"].as_str().unwrap(), "f.txt");
    assert!(!h["new_range"].as_str().unwrap().is_empty());
    // Content replaced by the truncation marker, which references `squire show`.
    let content = h["content"].as_str().unwrap();
    assert!(
        content.contains("content truncated") && content.contains("squire show"),
        "expected truncation marker, got: {content}"
    );
    // line_hashes cleared when truncated.
    assert!(h["line_hashes"].as_array().unwrap().is_empty());
}

// Regression test: the truncation marker printed by `squire log --json` must
// reference a command that actually works. Previously it said
// `squire show <id>`, which only searches the working tree and fails for
// hunks that live in a commit. The marker now includes the commit SHA so
// users can copy it and run it verbatim.
#[test]
fn log_json_truncation_marker_command_is_runnable() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    let mut big = String::from("seed\n");
    for i in 0..300 {
        big.push_str(&format!("line {i}\n"));
    }
    repo.write_file("f.txt", &big);
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "big"]);

    let out = repo.squire(&["--json", "log", "-n", "1"]);
    let commits: serde_json::Value = serde_json::from_str(&out).unwrap();
    let full_sha = commits[0]["sha"].as_str().unwrap().to_string();
    // The marker uses the short SHA (first 8 chars) to match squire log's
    // plain-text output. Git resolves short SHAs like any other ref.
    let short_sha = &full_sha[..8];
    let h = &commits[0]["hunks"][0];
    let hunk_id = h["id"].as_str().unwrap().to_string();
    let content = h["content"].as_str().unwrap();

    // The marker must include the commit SHA so the suggested command can
    // actually locate the hunk.
    assert!(
        content.contains(short_sha),
        "marker should reference commit SHA {short_sha}, got: {content}"
    );

    // The suggested command must be `squire show <sha> <id>` (the ref is
    // required — without it, `squire show` only searches the working tree).
    let expected_cmd = format!("squire show {short_sha} {hunk_id}");
    assert!(
        content.contains(&expected_cmd),
        "expected marker to suggest `{expected_cmd}`, got: {content}"
    );

    // And running that exact command must succeed and return the full hunk.
    let show_out = repo.squire(&["show", short_sha, &hunk_id]);
    assert!(
        show_out.contains("+line 0") && show_out.contains("+line 299"),
        "squire show {short_sha} {hunk_id} should return full hunk, got: {show_out}"
    );
}

#[test]
fn log_json_opt_out_with_max_hunk_lines_zero() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    let mut big = String::from("seed\n");
    for i in 0..300 {
        big.push_str(&format!("line {i}\n"));
    }
    repo.write_file("f.txt", &big);
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "big"]);

    let out = repo.squire(&["--json", "log", "-n", "1", "--max-hunk-lines", "0"]);
    let commits: serde_json::Value = serde_json::from_str(&out).unwrap();
    let h = &commits[0]["hunks"][0];
    let content = h["content"].as_str().unwrap();
    assert!(!content.contains("content truncated"), "got: {content}");
    // Full content preserved — should contain some of the added lines.
    assert!(content.contains("+line 0"), "got: {content}");
    assert!(content.contains("+line 299"), "got: {content}");
    assert!(!h["line_hashes"].as_array().unwrap().is_empty());
}

#[test]
fn log_json_small_commit_is_not_truncated() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let out = repo.squire(&["--json", "log", "-n", "1"]);
    let commits: serde_json::Value = serde_json::from_str(&out).unwrap();
    let content = commits[0]["hunks"][0]["content"].as_str().unwrap();
    assert!(content.contains("+v2"), "got: {content}");
    assert!(!content.contains("content truncated"), "got: {content}");
}

#[test]
fn log_rename_only_commit_shows_hunk() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    repo.git(&["commit", "-m", "rename"]);

    let out = repo.squire(&["--json", "log", "-n", "1"]);
    let commits: serde_json::Value = serde_json::from_str(&out).unwrap();
    let hunks = commits[0]["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0]["old_file"].as_str().unwrap(), "old.txt");
    assert_eq!(hunks[0]["file"].as_str().unwrap(), "new.txt");
}
