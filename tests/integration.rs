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
fn stage_single_hunk() {
    let repo = TestRepo::with_committed_file("f.txt", "line1\nline2\n", "changed1\nchanged2\n");

    // Get hunk ID and stage it
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["stage", id]);

    // Verify staged diff contains the change
    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("+changed1"));
    assert!(staged.contains("+changed2"));

    // Verify working tree diff is now empty
    let unstaged = repo.git(&["diff"]);
    assert!(unstaged.trim().is_empty());
}

#[test]
fn stage_subset_of_hunks() {
    let repo =
        TestRepo::with_two_committed_files("a.txt", "aaa\n", "AAA\n", "b.txt", "bbb\n", "BBB\n");

    // Get hunk IDs
    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();

    // Find the a.txt hunk
    let a_hunk = arr
        .iter()
        .find(|h| h["file"].as_str().unwrap().contains("a.txt"))
        .unwrap();
    let a_id = a_hunk["id"].as_str().unwrap();

    // Stage only a.txt
    repo.squire(&["stage", a_id]);

    // Verify only a.txt is staged
    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("+AAA"));
    assert!(!staged.contains("+BBB"));

    // Verify b.txt is still unstaged
    let unstaged = repo.git(&["diff"]);
    assert!(unstaged.contains("+BBB"));
    assert!(!unstaged.contains("+AAA"));
}

#[test]
fn stage_unknown_id_fails() {
    let repo = TestRepo::with_committed_file("f.txt", "a\n", "b\n");

    let stderr = repo.squire_err(&["stage", "deadbeef"]);
    assert!(!stderr.is_empty());
}

#[test]
fn unstage_single_hunk() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    // Stage it first
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["stage", id]);

    // Now get the staged hunk ID (may differ since diff source changed)
    let cached_out = repo.git(&["diff", "--cached"]);
    assert!(cached_out.contains("+new"));

    // Unstage using the cached diff's hunk ID
    // The hunk ID is content-based, so same content = same ID
    repo.squire(&["unstage", id]);

    // Verify nothing is staged anymore
    let after = repo.git(&["diff", "--cached"]);
    assert!(after.trim().is_empty());

    // Verify change is back in working tree
    let unstaged = repo.git(&["diff"]);
    assert!(unstaged.contains("+new"));
}

#[test]
fn unstage_subset_of_hunks() {
    let repo =
        TestRepo::with_two_committed_files("a.txt", "aaa\n", "AAA\n", "b.txt", "bbb\n", "BBB\n");

    // Stage both
    let hunks = repo.diff_json();
    let ids: Vec<&str> = hunks
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap())
        .collect();
    repo.squire(&["stage", ids[0], ids[1]]);

    // Find the a.txt hunk ID from cached diff (same content = same ID)
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap().contains("a.txt"))
        .unwrap()["id"]
        .as_str()
        .unwrap();

    // Unstage only a.txt
    repo.squire(&["unstage", a_id]);

    // b.txt should still be staged
    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("+BBB"));
    assert!(!staged.contains("+AAA"));
}

#[test]
fn stage_line_range_stages_partial_hunk() {
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

    // line_hashes[1] = -old1, line_hashes[2] = +new1
    // Stage only the first change using comma-separated hashes
    let selector = format!("{},{}", line_hashes[1], line_hashes[2]);
    repo.squire(&["stage", &format!("{id}:{selector}")]);

    // Verify first change is staged
    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("+new1"));
    assert!(!staged.contains("+new2"));

    // Verify second change is still unstaged
    let unstaged = repo.git(&["diff"]);
    assert!(unstaged.contains("+new2"));
    assert!(!unstaged.contains("+new1"));
}

#[test]
fn stage_line_hash_range_stages_partial_hunk() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "ctx1\nold1\nctx2\nold2\nctx3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "ctx1\nnew1\nctx2\nnew2\nctx3\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let line_hashes: Vec<&str> = hunks[0]["line_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Stage lines 1-2 (the -old1 and +new1) using range syntax
    let range = format!("{}-{}", line_hashes[1], line_hashes[2]);
    repo.squire(&["stage", &format!("{id}:{range}")]);

    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("+new1"));
    assert!(!staged.contains("+new2"));
}

#[test]
fn split_head_resets_commit() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "original\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "to-split"]);

    let head_commit = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.squire(&["split", &head_commit]);

    // Changes should now be unstaged
    let diff = repo.git(&["diff"]);
    assert!(diff.contains("+changed"));
    // HEAD should be back to the init commit
    let log = repo.git(&["log", "--oneline"]);
    assert!(!log.contains("to-split"));
}

#[test]
fn split_older_commit_rebases_and_resets() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    repo.write_file("a.txt", "a-changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);

    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    repo.squire(&["split", &target]);

    // Should be mid-rebase with target commit's changes unstaged
    let diff = repo.git(&["diff"]);
    assert!(diff.contains("+a-changed"));
    // The "later" commit should not be in the log yet (rebase paused)
    let log = repo.git(&["log", "--oneline"]);
    assert!(!log.contains("later"));
}

#[test]
fn split_dirty_working_tree_fails() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "original\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "dirty\n");

    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let stderr = repo.squire_err(&["split", &head]);
    assert!(stderr.contains("clean working tree"));
}

#[test]
fn status_shows_staged_and_unstaged() {
    let repo =
        TestRepo::with_two_committed_files("a.txt", "aaa\n", "AAA\n", "b.txt", "bbb\n", "BBB\n");

    // Stage only a.txt
    let hunks = repo.diff_json();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap().contains("a.txt"))
        .unwrap()["id"]
        .as_str()
        .unwrap();
    repo.squire(&["stage", a_id]);

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    let staged = status["staged"].as_array().unwrap();
    let unstaged = status["unstaged"].as_array().unwrap();
    assert_eq!(staged.len(), 1);
    assert_eq!(unstaged.len(), 1);
    assert!(staged[0]["file"].as_str().unwrap().contains("a.txt"));
    assert!(unstaged[0]["file"].as_str().unwrap().contains("b.txt"));
}

#[test]
fn status_includes_untracked_files() {
    let repo = TestRepo::new();
    repo.write_file("tracked.txt", "content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("untracked.txt", "new file\n");

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    let unstaged = status["unstaged"].as_array().unwrap();
    assert_eq!(unstaged.len(), 1);
    assert!(
        unstaged[0]["file"]
            .as_str()
            .unwrap()
            .contains("untracked.txt")
    );
}

#[test]
fn status_works_on_repo_with_no_commits() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "hello\n");

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(status["branch"], "main");
    assert_eq!(status["unstaged"].as_array().unwrap().len(), 1);
}

#[test]
fn status_empty_repo_shows_no_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(status["staged"].as_array().unwrap().is_empty());
    assert!(status["unstaged"].as_array().unwrap().is_empty());
}

#[test]
fn status_shows_branch_name() {
    let repo = TestRepo::with_committed_file("a.txt", "old\n", "new\n");
    repo.git(&["checkout", "-b", "feature-xyz"]);

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(status["branch"].as_str().unwrap(), "feature-xyz");
}

#[test]
fn status_shows_line_counts() {
    let repo =
        TestRepo::with_two_committed_files("a.txt", "aaa\n", "AAA\n", "b.txt", "bbb\n", "BBB\n");

    // Stage only a.txt
    let hunks = repo.diff_json();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap().contains("a.txt"))
        .unwrap()["id"]
        .as_str()
        .unwrap();
    repo.squire(&["stage", a_id]);

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    let staged_add = status["staged_lines"]["added"].as_u64().unwrap();
    let staged_del = status["staged_lines"]["removed"].as_u64().unwrap();
    let unstaged_add = status["unstaged_lines"]["added"].as_u64().unwrap();
    let unstaged_del = status["unstaged_lines"]["removed"].as_u64().unwrap();
    assert_eq!(staged_add, 1);
    assert_eq!(staged_del, 1);
    assert_eq!(unstaged_add, 1);
    assert_eq!(unstaged_del, 1);
}

#[test]
fn status_shows_rebase_in_progress() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "line2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("f.txt", "line3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    // Start a split on the middle commit to trigger a rebase
    let second = repo.git(&["rev-parse", "HEAD~1"]);
    repo.squire(&["split", second.trim()]);

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(status["rebase_in_progress"].as_bool().unwrap(), true);
}

#[test]
fn status_no_rebase_when_clean() {
    let repo = TestRepo::with_committed_file("a.txt", "old\n", "new\n");

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(status["rebase_in_progress"].as_bool().unwrap(), false);
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
fn stage_untracked_file() {
    let repo = TestRepo::new();
    repo.write_file("tracked.txt", "content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("new.txt", "hello\n");

    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();
    let untracked = arr
        .iter()
        .find(|h| h["file"].as_str().unwrap().contains("new.txt"))
        .unwrap();
    let id = untracked["id"].as_str().unwrap();

    repo.squire(&["stage", id]);

    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("+hello"));
}

#[test]
fn status_plain_shows_branch_and_summary() {
    let repo = TestRepo::with_committed_file("a.txt", "old\n", "new\n");

    let output = repo.squire(&["status"]);

    assert!(
        output.contains("On branch main"),
        "expected branch line, got: {output}"
    );
    assert!(
        output.contains("+1/-1"),
        "expected line counts, got: {output}"
    );
}

#[test]
fn stage_all_hunks_with_identical_content_across_files() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "old\n");
    repo.write_file("b.txt", "old\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "new\n");
    repo.write_file("b.txt", "new\n");

    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // IDs must be unique despite identical content
    let id0 = arr[0]["id"].as_str().unwrap();
    let id1 = arr[1]["id"].as_str().unwrap();
    assert_ne!(id0, id1);

    // Staging both must succeed
    repo.squire(&["stage", id0, id1]);

    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("a.txt"));
    assert!(staged.contains("b.txt"));
}

// --- commit command ---

#[test]
fn commit_stages_and_commits_in_one_step() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["commit", "-m", "feat: update", id]);

    // Working tree should be clean
    let diff = repo.git(&["diff"]);
    assert!(diff.trim().is_empty());
    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("feat: update"));
}

#[test]
fn commit_json_output() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let out = repo.squire(&["--json", "commit", "-m", "msg", id]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["committed"].as_u64().unwrap(), 1);
}

#[test]
fn commit_with_line_selector() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "ctx1\nold1\nctx2\nold2\nctx3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "ctx1\nnew1\nctx2\nnew2\nctx3\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let lh: Vec<&str> = hunks[0]["line_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    let selector = format!("{},{}", lh[1], lh[2]);
    repo.squire(&["commit", "-m", "partial", &format!("{id}:{selector}")]);

    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.trim().is_empty());
    let unstaged = repo.git(&["diff"]);
    assert!(unstaged.contains("+new2"));
    assert!(!unstaged.contains("+new1"));
}

// --- amend command ---

#[test]
fn amend_stages_and_amends() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "original msg"]);
    repo.write_file("g.txt", "extra\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["amend", id]);

    // g.txt should now be in HEAD
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("g.txt"));
    // Message should be preserved
    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("original msg"));
}

#[test]
fn amend_with_new_message() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "old msg"]);
    repo.write_file("g.txt", "extra\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["amend", "-m", "new msg", id]);

    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("new msg"));
    assert!(!log.contains("old msg"));
}

#[test]
fn amend_json_output() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "msg"]);
    repo.write_file("g.txt", "extra\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let out = repo.squire(&["--json", "amend", id]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["amended"].as_u64().unwrap(), 1);
}

#[test]
fn amend_into_older_commit() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    // Create an unstaged change to amend into the target commit
    repo.write_file("d.txt", "d\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["amend", "--commit", &target[..8], id]);

    // d.txt should be in the target commit (HEAD~1), not HEAD
    let target_show = repo.git(&["show", "--stat", "HEAD~1"]);
    assert!(target_show.contains("d.txt"));
    let head_show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(!head_show.contains("d.txt"));
    // All commit messages should be preserved
    let log = repo.git(&["log", "--oneline", "-3"]);
    assert!(log.contains("target"));
    assert!(log.contains("third"));
}

#[test]
fn amend_commit_head_same_as_default() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    repo.write_file("g.txt", "extra\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["amend", "--commit", "HEAD", id]);

    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("g.txt"));
    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("target"));
}

#[test]
fn amend_commit_rejects_message_for_non_head() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("c.txt", "c\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let err = repo.squire_err(&["amend", "--commit", "HEAD~1", "-m", "nope", id]);
    assert!(err.contains("cannot be used"));
}

// --- hunk ID prefix matching ---

#[test]
fn stage_with_prefix_match() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let hunks = repo.diff_json();
    let full_id = hunks[0]["id"].as_str().unwrap();
    let prefix = &full_id[..4];

    repo.squire(&["stage", prefix]);

    let staged = repo.git(&["diff", "--cached"]);
    assert!(staged.contains("+new"));
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

#[test]
fn diff_short_shows_compact_output() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let out = repo.squire(&["--short", "diff"]);
    // Short output: id, file, range, +/-counts on one line
    assert!(out.contains("f.txt"), "expected file name, got: {out}");
    assert!(out.contains("+1/-1"), "expected line counts, got: {out}");
}

#[test]
fn status_short_shows_compact_output() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let out = repo.squire(&["--short", "status"]);
    assert!(
        out.contains("Unstaged"),
        "expected Unstaged section, got: {out}"
    );
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
fn stage_untracked_file_no_trailing_newline() {
    let repo = TestRepo::new();
    repo.write_file("tracked.txt", "content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // File without trailing newline
    repo.write_file("no_newline.txt", "hello");

    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();
    let hunk = arr
        .iter()
        .find(|h| h["file"].as_str().unwrap().contains("no_newline.txt"))
        .expect("untracked file should appear in diff");
    let id = hunk["id"].as_str().unwrap();

    repo.squire(&["stage", id]);

    // The staged blob must match the file exactly (no spurious trailing newline)
    let staged_bytes = repo.git(&["show", ":no_newline.txt"]);
    assert_eq!(
        staged_bytes, "hello",
        "staged content should not have trailing newline"
    );
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
fn cleanup_merged_branch_detected() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create and merge a branch
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature work"]);
    repo.git(&["checkout", "main"]);
    repo.git(&["merge", "feature"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = result["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    assert_eq!(feature["status"].as_str().unwrap(), "merged");
}
