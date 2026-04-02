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
fn unstage_with_staged_binary_file_does_not_panic() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    // Stage the text change and a binary file together
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["stage", id]);
    let bin_path = repo.path().join("image.bin");
    std::fs::write(&bin_path, b"\x00\x01\x02\xff").unwrap();
    repo.git(&["add", "image.bin"]);

    // Unstage the text hunk — git diff --cached now contains a binary
    // diff block which must not cause a panic.
    repo.squire(&["unstage", id]);

    // Text change is back in working tree
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
fn revert_single_hunk() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["revert", id]);

    // Change should be gone from working tree
    let diff = repo.git(&["diff"]);
    assert!(diff.trim().is_empty());

    // File should have original content
    let content = std::fs::read_to_string(repo.path().join("f.txt")).unwrap();
    assert_eq!(content, "old\n");
}

#[test]
fn revert_subset_of_hunks() {
    let repo =
        TestRepo::with_two_committed_files("a.txt", "aaa\n", "AAA\n", "b.txt", "bbb\n", "BBB\n");

    let hunks = repo.diff_json();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap().contains("a.txt"))
        .unwrap()["id"]
        .as_str()
        .unwrap();

    repo.squire(&["revert", a_id]);

    // a.txt reverted, b.txt still changed
    let diff = repo.git(&["diff"]);
    assert!(!diff.contains("+AAA"));
    assert!(diff.contains("+BBB"));
}

#[test]
fn revert_with_line_selector() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "ctx1\nold1\nctx2\nold2\nctx3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "ctx1\nnew1\nctx2\nnew2\nctx3\n");

    let hunks = repo.diff_json();
    let hunk = &hunks[0];
    let line_hashes = hunk["line_hashes"].as_array().unwrap();
    let content_str = hunk["content"].as_str().unwrap();
    let lines: Vec<&str> = content_str.lines().collect();

    // Find hashes for -old1 and +new1 (the first change pair)
    let mut first_change_hashes = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if *line == "-old1" || *line == "+new1" {
            first_change_hashes.push(line_hashes[i].as_str().unwrap());
        }
    }
    assert_eq!(first_change_hashes.len(), 2);

    let id = hunk["id"].as_str().unwrap();
    let selector = format!("{id}:{},{}", first_change_hashes[0], first_change_hashes[1]);
    repo.squire(&["revert", &selector]);

    // Only the first change should be reverted
    let content = std::fs::read_to_string(repo.path().join("f.txt")).unwrap();
    assert!(content.contains("old1"), "first change should be reverted");
    assert!(content.contains("new2"), "second change should remain");
}

#[test]
fn revert_json_output() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let output = repo.squire(&["--json", "revert", id]);
    let result: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(result["reverted"], 1);
}

#[test]
fn revert_staged_hunk() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");
    repo.git(&["add", "f.txt"]);

    // Hunk is now staged, not unstaged
    let cached = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&cached).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["revert", id]);

    // File should be back to original content
    let content = std::fs::read_to_string(repo.path().join("f.txt")).unwrap();
    assert_eq!(content, "old\n");
    // Nothing should be staged or unstaged
    let status = repo.squire(&["--json", "status"]);
    let s: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert!(s["staged"].as_array().unwrap().is_empty());
    assert!(s["unstaged"].as_array().unwrap().is_empty());
}

#[test]
fn revert_mixed_staged_and_unstaged() {
    let repo =
        TestRepo::with_two_committed_files("a.txt", "aaa\n", "AAA\n", "b.txt", "bbb\n", "BBB\n");
    // Stage a.txt, leave b.txt unstaged
    repo.git(&["add", "a.txt"]);

    let cached = repo.squire(&["--json", "diff", "--cached"]);
    let cached_hunks: serde_json::Value = serde_json::from_str(&cached).unwrap();
    let a_id = cached_hunks[0]["id"].as_str().unwrap().to_string();

    let unstaged = repo.squire(&["--json", "diff"]);
    let unstaged_hunks: serde_json::Value = serde_json::from_str(&unstaged).unwrap();
    let b_id = unstaged_hunks[0]["id"].as_str().unwrap().to_string();

    let output = repo.squire(&["--json", "revert", &a_id, &b_id]);
    let result: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(result["reverted"], 2);

    let a = std::fs::read_to_string(repo.path().join("a.txt")).unwrap();
    let b = std::fs::read_to_string(repo.path().join("b.txt")).unwrap();
    assert_eq!(a, "aaa\n");
    assert_eq!(b, "bbb\n");
}

#[test]
fn revert_untracked_file_refuses() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("new.txt", "untracked\n");

    let hunks = repo.diff_json();
    let id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "new.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    let err = repo.squire_err(&["revert", id]);
    assert!(err.contains("cannot revert untracked file"), "{err}");
    assert!(std::fs::read_to_string(repo.path().join("new.txt")).is_ok());
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

    assert!(status["rebase_in_progress"].as_bool().unwrap());
}

#[test]
fn status_no_rebase_when_clean() {
    let repo = TestRepo::with_committed_file("a.txt", "old\n", "new\n");

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(!status["rebase_in_progress"].as_bool().unwrap());
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
fn amend_already_staged_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "original msg"]);

    // Stage a change via git add, then amend using the staged hunk ID
    repo.write_file("g.txt", "extra\n");
    repo.git(&["add", "g.txt"]);

    let cached = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&cached).unwrap();
    let id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "g.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    repo.squire(&["amend", id]);

    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("g.txt"));
    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("original msg"));
}

#[test]
fn commit_already_staged_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    repo.write_file("g.txt", "new\n");
    repo.git(&["add", "g.txt"]);

    let cached = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&cached).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["commit", "-m", "from staged", id]);

    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("from staged"));
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("g.txt"));
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
fn amend_into_older_commit_with_dirty_tree() {
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

    // Two unstaged changes: one to amend, one to leave dirty
    repo.write_file("d.txt", "d\n");
    repo.write_file("e.txt", "e\n");

    let hunks = repo.diff_json();
    let d_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "d.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    // Amend only d.txt into the older commit — e.txt stays dirty
    repo.squire(&["amend", "--commit", &target[..8], d_id]);

    // d.txt should be in the target commit
    let target_show = repo.git(&["show", "--stat", "HEAD~1"]);
    assert!(target_show.contains("d.txt"));

    // e.txt should still be an unstaged change in the working tree
    let status = repo.git(&["status", "--porcelain"]);
    assert!(status.contains("e.txt"));
    assert!(std::fs::read_to_string(repo.path().join("e.txt")).unwrap() == "e\n");
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

// --- reword ---

#[test]
fn reword_head() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "old msg"]);

    repo.squire(&["reword", "HEAD", "-m", "new msg"]);

    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("new msg"));
    assert!(!log.contains("old msg"));
}

#[test]
fn reword_older_commit() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    repo.squire(&["reword", "HEAD~1", "-m", "second reworded"]);

    let log = repo.git(&["log", "--oneline", "-3"]);
    assert!(log.contains("second reworded"));
    assert!(log.contains("first"));
    assert!(log.contains("third"));
    assert!(!log.contains("\nsecond\n"));
}

#[test]
fn reword_json_output() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "old"]);

    let out = repo.squire(&["--json", "reword", "HEAD", "-m", "new"]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["reworded"], true);
    assert_eq!(result["message"], "new");
}

#[test]
fn reword_older_commit_dirty_tree_fails() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("a.txt", "dirty\n");

    let err = repo.squire_err(&["reword", "HEAD~1", "-m", "nope"]);
    assert!(err.contains("clean working tree"));
}

// --- drop ---

#[test]
fn drop_hunk_from_head() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "a2\n");
    repo.write_file("b.txt", "b2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "change both"]);

    // Find the a.txt hunk
    let out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"] == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    repo.squire(&["drop", "HEAD", &a_id]);

    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("b.txt"));
    assert!(!show.contains("a.txt"));
    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("change both"));
}

#[test]
fn drop_hunk_from_older_commit() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "a2\n");
    repo.write_file("b.txt", "b2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    let out = repo.squire(&["--json", "diff", "HEAD~2", "HEAD~1"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"] == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    repo.squire(&["drop", "HEAD~1", &a_id]);

    // a.txt hunk should be gone from target commit (now HEAD~1)
    let show = repo.git(&["show", "--stat", "HEAD~1"]);
    assert!(show.contains("b.txt"));
    assert!(!show.contains("a.txt"));
    // third commit should still be there
    let log = repo.git(&["log", "--oneline", "-3"]);
    assert!(log.contains("target"));
    assert!(log.contains("third"));
}

#[test]
fn drop_json_output() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "a2\n");
    repo.write_file("b.txt", "b2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "change"]);

    let out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"] == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = repo.squire(&["--json", "drop", "HEAD", &a_id]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["dropped"].as_u64().unwrap(), 1);
}

#[test]
fn drop_older_commit_dirty_tree_fails() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "a2\n");
    repo.write_file("b.txt", "b2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);
    repo.write_file("a.txt", "dirty\n");

    let err = repo.squire_err(&["drop", "HEAD~1", "244cca06"]);
    assert!(err.contains("clean working tree"));
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

#[test]
fn seqedit_changes_action_in_todo_file() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(
        &todo,
        "pick abc1234 first commit\npick def5678 second commit\n",
    )
    .unwrap();

    let repo = TestRepo::new();
    repo.squire(&["seqedit", "edit:abc1", todo.to_str().unwrap()]);

    let result = std::fs::read_to_string(&todo).unwrap();
    assert!(result.starts_with("edit abc1234 first commit\n"));
    assert!(result.contains("pick def5678 second commit\n"));
}

#[test]
fn seqedit_multiple_actions() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(
        &todo,
        "pick abc1234 first\npick def5678 second\npick 99900ab third\n",
    )
    .unwrap();

    let repo = TestRepo::new();
    repo.squire(&["seqedit", "fixup:def5", "drop:9990", todo.to_str().unwrap()]);

    let result = std::fs::read_to_string(&todo).unwrap();
    assert!(result.starts_with("pick abc1234 first\n"));
    assert!(result.contains("fixup def5678 second\n"));
    assert!(result.contains("drop 99900ab third\n"));
}

#[test]
fn seqedit_unknown_sha_fails() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(&todo, "pick abc1234 first\n").unwrap();

    let repo = TestRepo::new();
    let err = repo.squire_err(&["seqedit", "edit:zzz999", todo.to_str().unwrap()]);
    assert!(err.contains("no todo line matches sha prefix"));
}

#[test]
fn seqedit_invalid_action_fails() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(&todo, "pick abc1234 first\n").unwrap();

    let repo = TestRepo::new();
    let err = repo.squire_err(&["seqedit", "bogus:abc1", todo.to_str().unwrap()]);
    assert!(err.contains("unknown action"));
}

#[test]
fn seqedit_ambiguous_sha_prefix_fails() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(&todo, "pick abc1234 first\npick abc1999 second\n").unwrap();

    let repo = TestRepo::new();
    let err = repo.squire_err(&["seqedit", "edit:abc1", todo.to_str().unwrap()]);
    assert!(
        err.contains("ambiguous"),
        "expected ambiguity error, got: {err}"
    );
}

#[test]
fn squash_folds_commit_into_target() {
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
    repo.git(&["commit", "-m", "to-squash"]);

    repo.write_file("d.txt", "d\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    repo.squire(&["squash", &target[..8], "HEAD~1"]);

    // Should be 3 commits: base, target (with c.txt folded in), later
    let log = repo.git(&["log", "--oneline"]);
    assert!(!log.contains("to-squash"), "squashed commit should be gone");
    assert!(log.contains("target"));
    assert!(log.contains("later"));

    // c.txt should be in the target commit
    let target_show = repo.git(&["show", "--stat", "HEAD~1"]);
    assert!(target_show.contains("c.txt"));
    assert!(target_show.contains("b.txt"));
}

#[test]
fn squash_multiple_sources() {
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
    repo.git(&["commit", "-m", "squash-me-1"]);
    let src1 = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.write_file("d.txt", "d\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "squash-me-2"]);
    let src2 = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.squire(&["squash", &target[..8], &src1[..8], &src2[..8]]);

    let log = repo.git(&["log", "--oneline"]);
    assert_eq!(log.lines().count(), 2); // base + target
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("b.txt"));
    assert!(show.contains("c.txt"));
    assert!(show.contains("d.txt"));
}

#[test]
fn squash_dirty_working_tree_fails() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("a.txt", "dirty\n");

    let err = repo.squire_err(&["squash", "HEAD~1", "HEAD"]);
    assert!(err.contains("clean working tree"));
}

#[test]
fn stash_selected_hunks() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    repo.write_file("a.txt", "a-modified\n");
    repo.write_file("b.txt", "b-new\n");

    let hunks = repo.diff_json();
    assert_eq!(hunks.as_array().unwrap().len(), 2);

    // Stash only the a.txt hunk
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    repo.squire(&["stash", a_id]);

    // b.txt should still be in the working tree
    let remaining = repo.diff_json();
    assert_eq!(remaining.as_array().unwrap().len(), 1);
    assert_eq!(remaining[0]["file"].as_str().unwrap(), "b.txt");

    // git stash list should show one entry
    let stash_list = repo.git(&["stash", "list"]);
    assert!(!stash_list.is_empty());

    // Pop and verify a.txt is back
    repo.git(&["stash", "pop"]);
    let after_pop = repo.diff_json();
    assert_eq!(after_pop.as_array().unwrap().len(), 2);
}

#[test]
fn stash_all_hunks() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    repo.write_file("a.txt", "a-modified\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["stash", id]);

    // Working tree should be clean
    let remaining = repo.diff_json();
    assert!(remaining.as_array().unwrap().is_empty());

    // Stash should have the change
    let stash_list = repo.git(&["stash", "list"]);
    assert!(!stash_list.is_empty());
}

#[test]
fn amend_conflict_returns_structured_error() {
    // Create a repo where amending an older commit will conflict.
    // We need 4 commits so HEAD~2 has a parent for the rebase.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("f.txt", "first\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);

    repo.write_file("f.txt", "second\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("f.txt", "third\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    // Create a conflicting change and try to amend it into "first" (HEAD~2)
    repo.write_file("f.txt", "conflict\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    let err_output = repo.squire_json_err(&["--json", "amend", "--commit", "HEAD~2", id]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();

    assert!(parsed["conflict"].as_bool().unwrap(), "parsed: {parsed}");
    let files = parsed["conflicting_files"].as_array().unwrap();
    assert!(!files.is_empty());
    assert_eq!(files[0]["file"].as_str().unwrap(), "f.txt");
    assert!(
        parsed["hint"]
            .as_str()
            .unwrap()
            .contains("GIT_EDITOR=true git rebase --continue")
    );

    // Clean up the paused rebase
    repo.git(&["rebase", "--abort"]);
}

#[test]
fn status_shows_conflicts_during_rebase() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("f.txt", "first\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);

    repo.write_file("f.txt", "second\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("f.txt", "third\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    // Try to amend into "first" (HEAD~2) to cause a conflict
    repo.write_file("f.txt", "conflict\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let _ = repo.run_squire(&["--json", "amend", "--commit", "HEAD~2", id]);

    // Now check status — should show conflict info
    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(status["rebase_in_progress"].as_bool().unwrap());
    let conflicts = status["conflicts"].as_array().unwrap();
    assert!(!conflicts.is_empty());
    assert_eq!(conflicts[0]["file"].as_str().unwrap(), "f.txt");

    // Clean up
    repo.git(&["rebase", "--abort"]);
}

#[test]
fn status_no_conflicts_field_when_clean() {
    let repo = TestRepo::with_committed_file("a.txt", "old\n", "new\n");

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(status.get("conflicts").is_none());
}

#[test]
fn status_plain_shows_conflicts() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("f.txt", "first\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);

    repo.write_file("f.txt", "second\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("f.txt", "third\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    repo.write_file("f.txt", "conflict\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let _ = repo.run_squire(&["--json", "amend", "--commit", "HEAD~2", id]);

    let output = repo.squire(&["status"]);
    assert!(output.contains("Conflicts"));
    assert!(output.contains("f.txt"));
    assert!(output.contains("GIT_EDITOR=true git rebase --continue"));

    repo.git(&["rebase", "--abort"]);
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
fn status_plain_clean_working_tree() {
    let repo = TestRepo::with_committed_file("f.txt", "hello\n", "hello\n");
    let out = repo.squire(&["status"]);
    assert!(out.contains("Nothing to commit, working tree clean"));
}

#[test]
fn status_plain_shows_staged_and_unstaged_sections() {
    let repo = TestRepo::with_committed_file("f.txt", "a\n", "c\n");
    // Stage a different file so we have both staged and unstaged
    repo.write_file("g.txt", "new\n");
    repo.git(&["add", "g.txt"]);
    let out = repo.squire(&["status"]);
    assert!(out.contains("Staged ("));
    assert!(out.contains("Unstaged ("));
}

#[test]
fn cleanup_plain_text_output() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a merged branch
    repo.git(&["checkout", "-b", "merged-branch"]);
    repo.write_file("f.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "change"]);
    repo.git(&["checkout", "main"]);
    repo.git(&["merge", "merged-branch"]);

    // Create an unmerged branch
    repo.git(&["checkout", "-b", "unmerged-branch"]);
    repo.write_file("f.txt", "unmerged\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "unmerged work"]);
    repo.git(&["checkout", "main"]);

    let out = repo.squire(&["cleanup", "--master", "main"]);
    assert!(out.contains("Master branch: main"));
    assert!(out.contains("[MERGED]"));
    assert!(out.contains("merged-branch"));
    assert!(out.contains("[UNMERGED]"));
    assert!(out.contains("unmerged-branch"));
    assert!(out.contains("unmerged work"));
}

#[test]
fn cleanup_squash_merged_branch_detected() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a branch with a commit
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    repo.git(&["checkout", "main"]);

    // Advance main so the branch is not ancestry-merged, then
    // replicate the branch's change with the same message (squash merge).
    repo.write_file("f.txt", "updated\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    repo.write_file("g.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    assert_eq!(feature["status"], "merged_equivalent");
}

#[test]
fn cleanup_needs_evaluation_branch() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a branch with a commit
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "branch version\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    repo.git(&["checkout", "main"]);

    // Same message but different patch on main
    repo.write_file("g.txt", "main version\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    assert_eq!(feature["status"], "needs_evaluation");
    assert!(feature["note"].as_str().unwrap().contains("patches differ"));
}

#[test]
fn cleanup_partial_message_match() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch with two commits, only one message matches main
    repo.git(&["checkout", "-b", "partial"]);
    repo.write_file("g.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "shared msg"]);
    repo.write_file("h.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "unique to branch"]);
    repo.git(&["checkout", "main"]);

    // Only one matching message on main
    repo.write_file("x.txt", "x\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "shared msg"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let partial = branches.iter().find(|b| b["name"] == "partial").unwrap();
    assert_eq!(partial["status"], "needs_evaluation");
}

#[test]
fn squash_with_message_replacement() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("f.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("f.txt", "d\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    let out = repo.squire(&[
        "--json", "squash", "-m", "combined", "HEAD~2", "HEAD~1", "HEAD",
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["squashed"], 2);

    let log = repo.git(&["log", "-1", "--format=%s"]);
    assert_eq!(log.trim(), "combined");
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
fn show_short_mode() {
    let repo = TestRepo::with_committed_file("f.txt", "old\n", "new\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let out = repo.squire(&["--short", "show", id]);
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

#[test]
fn revert_line_selector_invalid_range_order() {
    let repo = TestRepo::with_committed_file("f.txt", "a\nb\nc\nd\ne\n", "a\nB\nC\nD\ne\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let hashes = hunks[0]["line_hashes"].as_array().unwrap();
    // Find the last and first change line hashes and reverse them
    let last = hashes.last().unwrap().as_str().unwrap();
    let first = hashes[0].as_str().unwrap();
    // Use reversed range: last-first
    let sel = format!("{id}:{last}-{first}");
    let err = repo.squire_err(&["stage", &sel]);
    assert!(err.contains("comes after") || err.contains("not found"));
}

#[test]
fn stage_partial_json_reports_new_hunks() {
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

    // Stage only the first change pair (-old1, +new1)
    let selector = format!("{},{}", line_hashes[1], line_hashes[2]);
    let out = repo.squire(&["--json", "stage", &format!("{id}:{selector}")]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(result["staged"], 1);
    let new_hunks = result["new_hunks"].as_array().unwrap();
    assert_eq!(new_hunks.len(), 1);
    assert_eq!(new_hunks[0]["file"], "f.txt");
    assert!(new_hunks[0]["id"].as_str().unwrap().len() == 8);
    assert!(new_hunks[0]["line_hashes"].as_array().unwrap().len() > 0);

    // The new hunk ID should match what squire diff now reports
    let remaining = repo.diff_json();
    assert_eq!(remaining[0]["id"], new_hunks[0]["id"]);
}

#[test]
fn stage_full_hunk_no_new_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "old\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "new\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    let out = repo.squire(&["--json", "stage", id]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(result["staged"], 1);
    assert!(result.get("new_hunks").is_none());
}

#[test]
fn stage_partial_plain_reports_new_hunks() {
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

    let selector = format!("{},{}", line_hashes[1], line_hashes[2]);
    let out = repo.squire(&["stage", &format!("{id}:{selector}")]);

    assert!(out.contains("new hunk:"));
    assert!(out.contains("f.txt"));
}

#[test]
fn unstage_partial_json_reports_new_hunks() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "ctx1\nold1\nctx2\nold2\nctx3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "ctx1\nnew1\nctx2\nnew2\nctx3\n");

    // Stage everything first
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["stage", id]);

    // Now get staged hunks and unstage partially
    let staged_out = repo.squire(&["--json", "diff", "--cached"]);
    let staged: Vec<serde_json::Value> = serde_json::from_str(&staged_out).unwrap();
    let staged_id = staged[0]["id"].as_str().unwrap();
    let line_hashes: Vec<&str> = staged[0]["line_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    let selector = format!("{},{}", line_hashes[1], line_hashes[2]);
    let out = repo.squire(&["--json", "unstage", &format!("{staged_id}:{selector}")]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(result["unstaged"], 1);
    let new_hunks = result["new_hunks"].as_array().unwrap();
    assert_eq!(new_hunks.len(), 1);
}

#[test]
fn rebase_no_upstream_returns_error() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Rename the only branch so there's no main/master to fall back to.
    repo.git(&["branch", "-m", "main", "dev"]);

    let err = repo.squire_err(&["rebase"]);
    assert!(
        err.contains("no upstream") || err.contains("cannot detect master"),
        "got: {err}"
    );
}

#[test]
fn rebase_dirty_working_tree_fails() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "dirty\n");

    let err = repo.squire_err(&["rebase"]);
    assert!(err.contains("clean working tree"), "got: {err}");
}

#[test]
fn rebase_up_to_date_json() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a fake upstream by making a second branch and pointing origin/main at it.
    repo.git(&["branch", "upstream"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "up_to_date");
    assert_eq!(val["branch"], "main");
    assert_eq!(val["commits_ahead"], 0);
}

#[test]
fn rebase_falls_back_to_master_branch() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Switch to a feature branch with no tracking ref.
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);

    // Advance main so feature is actually behind.
    repo.git(&["checkout", "main"]);
    repo.write_file("g.txt", "main-only\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main advance"]);
    repo.git(&["checkout", "feature"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["branch"], "feature");
    // Should have fallen back to local main since there's no remote.
    assert_eq!(val["upstream"], "main");
    assert_eq!(val["commits_ahead"], 1);
}

#[test]
fn rebase_ready_creates_tag_and_shows_steps() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin/main pointing at init.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main with a commit the local branch doesn't have.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local main back and add a different commit.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["commits_ahead"], 1);
    let tag = val["safety_tag"].as_str().unwrap();
    assert!(tag.starts_with("pre-rebase/main-"), "tag: {tag}");

    // Verify the tag actually exists.
    let tag_check = repo.git(&["rev-parse", "--verify", tag]);
    assert!(!tag_check.trim().is_empty());

    // Steps should mention the upstream.
    let steps = val["steps"].as_array().unwrap();
    assert!(steps[0].as_str().unwrap().contains("rebase"));
}

#[test]
fn rebase_ready_plain_text() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main independently.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local main back and add a different commit.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("Safety tag:"), "got: {out}");
    assert!(out.contains("pre-rebase/main-"), "got: {out}");
    assert!(out.contains("1 commit(s) ahead"), "got: {out}");
    assert!(out.contains("git rebase --empty=drop"), "got: {out}");
}

#[test]
fn rebase_during_conflict_shows_conflicts() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("f.txt", "first\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);

    repo.write_file("f.txt", "second\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    // Trigger a conflict via amend into older commit.
    repo.write_file("f.txt", "conflict\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let _ = repo.run_squire(&["--json", "amend", "--commit", "HEAD~1", id]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "rebasing");
    let conflicts = val["conflicts"].as_array().unwrap();
    assert!(!conflicts.is_empty());
    assert!(val["conflict_rules"].is_object());
    assert!(val["steps"].is_array());

    repo.git(&["rebase", "--abort"]);
}

#[test]
fn rebase_during_rebase_no_conflicts() {
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

    // Split middle commit to get into a rebase state without conflicts.
    let second = repo.git(&["rev-parse", "HEAD~1"]);
    repo.squire(&["split", second.trim()]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "rebasing");
    assert!(val.get("conflicts").is_none());
    let steps = val["steps"].as_array().unwrap();
    assert!(steps[0].as_str().unwrap().contains("rebase --continue"));

    repo.git(&["rebase", "--abort"]);
}

#[test]
fn rebase_ready_json_includes_commits_behind() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local and diverge.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(
        val["commits_behind"], 1,
        "ready state should include commits_behind"
    );
}

#[test]
fn rebase_up_to_date_json_includes_commits_behind() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["branch", "upstream"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "up_to_date");
    assert_eq!(
        val["commits_behind"], 0,
        "up_to_date state should include commits_behind"
    );
}

#[test]
fn rebase_ready_deduplicates_safety_tag() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local and diverge.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out1 = repo.squire(&["--json", "rebase"]);
    let val1: serde_json::Value = serde_json::from_str(&out1).unwrap();
    let tag1 = val1["safety_tag"].as_str().unwrap().to_string();

    // Run again immediately — same epoch second likely, tag already exists.
    let out2 = repo.squire(&["--json", "rebase"]);
    let val2: serde_json::Value = serde_json::from_str(&out2).unwrap();
    let tag2 = val2["safety_tag"].as_str().unwrap().to_string();

    // Tags should differ since the first one already existed.
    assert_ne!(tag1, tag2, "second invocation should create a distinct tag");

    // Both tags should exist.
    repo.git(&["rev-parse", "--verify", &tag1]);
    repo.git(&["rev-parse", "--verify", &tag2]);
}

#[test]
fn amend_conflict_plain_text_is_not_json() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("f.txt", "first\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);

    repo.write_file("f.txt", "second\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("f.txt", "third\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    // Create a conflicting change and amend without --json.
    repo.write_file("f.txt", "conflict\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    let err = repo.squire_err(&["amend", "--commit", "HEAD~2", id]);
    // The error should NOT be raw JSON when --json wasn't passed.
    assert!(
        !err.starts_with('{'),
        "plain-text error should not be JSON, got: {err}"
    );
    assert!(
        err.contains("f.txt"),
        "error should mention the conflicting file, got: {err}"
    );

    repo.git(&["rebase", "--abort"]);
}
