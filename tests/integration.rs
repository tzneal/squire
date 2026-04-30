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

// Regression test: `squire amend --commit <sha>` used to accept any SHA
// that `git rev-parse` could resolve (including commits that were rewritten
// by a prior amend and no longer appear in the current branch). The rebase
// base was then computed from the stale SHA, producing spurious conflicts.
// Now we reject stale SHAs with an error that points at the equivalent
// commit on the current branch.
#[test]
fn amend_commit_rejects_stale_sha_after_prior_amend() {
    let repo = TestRepo::new();
    repo.write_file("base.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Commit A — target of the first amend.
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit A"]);
    let a_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Commit B — target of the second amend (will be rewritten by the first).
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit B"]);
    let b_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // First amend: fold hunk for a.txt into commit A. This rewrites A, and
    // replays B on top so B gets a new SHA.
    repo.write_file("a-extra.txt", "extra for A\n");
    let hunks_a = repo.diff_json();
    let a_extra_id = hunks_a[0]["id"].as_str().unwrap();
    repo.squire(&["amend", "--commit", &a_before[..8], a_extra_id]);

    // b_before is now unreachable — it was replayed to a new SHA.
    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(b_before, head_after, "B should have been replayed");

    // Second amend with the stale pre-rewrite SHA of B should be rejected.
    repo.write_file("b-extra.txt", "extra for B\n");
    let hunks_b = repo.diff_json();
    let b_extra_id = hunks_b[0]["id"].as_str().unwrap();
    let err = repo.squire_err(&["amend", "--commit", &b_before[..8], b_extra_id]);

    // Error should call out that the SHA is unreachable and suggest the
    // rewritten equivalent on HEAD.
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    // The error should point at the equivalent commit (same subject as B)
    // so the user can retry with the right SHA.
    assert!(
        err.contains(&head_after[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );

    // Nothing should have been committed or rebased as a side effect.
    assert!(
        !repo.git(&["log", "--oneline", "-1"]).contains("fixup!"),
        "no dangling fixup commit should be left on HEAD"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress"
    );
}

// Build a repo with three commits (base, A, B) where a prior
// `amend --commit` has rewritten A. Returns (repo, b_before_sha) — the
// stale pre-rewrite SHA of B that other commands should now reject.
//
// Structure after the helper returns:
//   HEAD -> A' (rewritten) -> B' (rewritten) -> base
// `b_before_sha` resolves via rev-parse (it's still in the object db) but
// is not reachable from HEAD.
fn build_stale_sha_scenario() -> (TestRepo, String) {
    let repo = TestRepo::new();
    repo.write_file("base.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Commit A — target of the first amend.
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit A"]);
    let a_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Commit B — its SHA will be the stale one after A is rewritten.
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit B"]);
    let b_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Amend an unrelated file into A so B gets replayed onto a new SHA.
    repo.write_file("a-extra.txt", "extra for A\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["amend", "--commit", &a_before[..8], id]);

    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(b_before, head_after, "B should have been replayed");
    (repo, b_before)
}

// Regression test: `squire drop <sha>` used to accept SHAs that rev-parse
// could resolve but that weren't reachable from HEAD (e.g. commits
// rewritten by a prior amend). We now reject them with a pointer to the
// rewritten equivalent.
#[test]
fn drop_rejects_stale_sha_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();

    // Find a hunk ID from the current (rewritten) B so the drop args
    // are otherwise valid.
    let b_new = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let log = repo
        .squire(&["--json", "log", "-n", "1"])
        .trim()
        .to_string();
    let parsed: serde_json::Value = serde_json::from_str(&log).unwrap();
    let id = parsed[0]["hunks"][0]["id"].as_str().unwrap().to_string();

    let err = repo.squire_err(&["drop", &b_before[..8], &id]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    assert!(
        err.contains(&b_new[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );

    // No rebase should be in progress and HEAD should be unchanged.
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), b_new);
}

// Regression test: `squire reword <sha> -m <msg>` used to accept stale
// SHAs resolved via the object db, producing spurious conflicts when the
// rebase base (`<sha>~1`) pointed at a superseded ancestor.
#[test]
fn reword_rejects_stale_sha_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    let b_new = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let err = repo.squire_err(&["reword", &b_before[..8], "-m", "new message"]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    assert!(
        err.contains(&b_new[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), b_new);
}

// Regression test: `squire split <sha>` used to accept stale SHAs, which
// would start a rebase against a superseded ancestor.
#[test]
fn split_rejects_stale_sha_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    let b_new = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let err = repo.squire_err(&["split", &b_before[..8]]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    assert!(
        err.contains(&b_new[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), b_new);
}

// Regression test: `squire squash <target> <sources>...` used to accept
// stale SHAs for both the target and each source. We now reject either.
#[test]
fn squash_rejects_stale_target_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    // Create one more commit so we have a valid source to squash from.
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit C"]);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Stale target, valid source.
    let err = repo.squire_err(&["squash", &b_before[..8], &head[..8]]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error for target, got: {err}"
    );
}

// Regression test: a stale SHA passed as a *source* to squash is also
// rejected (not just the target).
#[test]
fn squash_rejects_stale_source_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    // Add a new commit so we have a valid target on HEAD.
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit C"]);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Valid target, stale source.
    let err = repo.squire_err(&["squash", &head[..8], &b_before[..8]]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error for source, got: {err}"
    );
}

// that fails with a conflict, the `fixup!` commit that squire created on
// HEAD before starting the rebase used to be left behind after the rebase
// aborted internally. Now we detect the abort and roll back the fixup so
// HEAD returns to its original state.
#[test]
fn amend_commit_cleans_up_fixup_on_rebase_conflict() {
    let repo = TestRepo::new();
    // Three commits where each touches the same file, so an autosquash
    // rebase of the oldest will need to replay the newer ones and can
    // conflict.
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);
    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Unstaged change to the same line — will fold into 'target' via a
    // fixup, and then the replay of 'later' on top will conflict.
    repo.write_file("shared.txt", "line1 v2 amended\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    // Expect an error (rebase conflict).
    let _err = repo.squire_err(&["amend", "--commit", &target[..8], id]);

    // HEAD must be back at head_before — no dangling fixup commit.
    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        head_after, head_before,
        "HEAD should be restored after failed amend; a dangling fixup was left behind"
    );
    let log = repo.git(&["log", "--oneline", "-5"]);
    assert!(
        !log.contains("fixup!"),
        "no fixup! commit should remain, got log:\n{log}"
    );

    // No rebase should be in progress.
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress after failed amend"
    );
}

// When amend rolls back on conflict, the JSON error must include
// `rolled_back: true` and the manual-recovery hint so LLM callers can
// distinguish this from the paused (--pause-on-conflict) case.
#[test]
fn amend_conflict_json_has_rolled_back_field() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    repo.write_file("shared.txt", "line1 v2 amended\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let err_output = repo.squire_json_err(&["--json", "amend", "--commit", &target[..8], id]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(
        parsed["rolled_back"].as_bool(),
        Some(true),
        "default amend must report rolled_back:true, got: {parsed}"
    );
    assert!(
        parsed["hint"]
            .as_str()
            .unwrap()
            .contains("--pause-on-conflict"),
        "hint must mention --pause-on-conflict, got: {parsed}"
    );
}

// With --pause-on-conflict, amend leaves the rebase paused instead of
// rolling back, so the LLM can resolve the conflict by hand.
#[test]
fn amend_pause_on_conflict_leaves_rebase_paused() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    repo.write_file("shared.txt", "line1 v2 amended\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    let err_output = repo.squire_json_err(&[
        "--json",
        "amend",
        "--commit",
        &target[..8],
        "--pause-on-conflict",
        id,
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(
        parsed["rolled_back"].as_bool(),
        Some(false),
        "--pause-on-conflict must report rolled_back:false"
    );
    assert!(
        parsed["hint"]
            .as_str()
            .unwrap()
            .contains("rebase --continue"),
        "paused hint must point at git rebase --continue"
    );
    // Rebase should still be in progress.
    assert!(
        repo.path().join(".git/rebase-merge").exists()
            || repo.path().join(".git/rebase-apply").exists(),
        "--pause-on-conflict should leave the rebase paused"
    );
    // Clean up so TestRepo drop doesn't leave stale rebase dir.
    let _ = std::process::Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(repo.path())
        .output();
}

// Build a three-commit history where dropping hunks from the middle
// commit will conflict when the later commit is replayed on top.
// Returns (repo, target_sha, hunk_id_to_drop, pre_command_head_sha).
fn build_drop_conflict_scenario() -> (TestRepo, String, String, String) {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Target commit: changes the single line. We'll try to drop this
    // change; the later commit depends on it.
    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Find the hunk ID for the target's change so we can pass it to
    // `squire drop`.
    let log = repo.squire(&["--json", "log", "-n", "1"]);
    let parsed: serde_json::Value = serde_json::from_str(&log).unwrap();
    let hunk_id = parsed[0]["hunks"][0]["id"].as_str().unwrap().to_string();

    // Later commit: further modifies the same line. Dropping the target's
    // change will break this commit's replay.
    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);
    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    (repo, target, hunk_id, head_before)
}

// By default (no --pause-on-conflict), `drop` on a non-HEAD commit whose
// changes conflict with later commits must roll back to the pre-command
// HEAD and leave no rebase in progress.
#[test]
fn drop_conflict_rolls_back_by_default() {
    let (repo, target, hunk_id, head_before) = build_drop_conflict_scenario();

    let err_output = repo.squire_json_err(&["--json", "drop", &target[..8], &hunk_id]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["conflict"].as_bool(), Some(true));
    assert_eq!(parsed["rolled_back"].as_bool(), Some(true));

    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(head_before, head_after, "drop should be atomic on conflict");
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress after rolled-back drop"
    );
}

// With --pause-on-conflict, `drop` leaves the rebase paused so the
// caller can resolve by hand.
#[test]
fn drop_pause_on_conflict_leaves_rebase_paused() {
    let (repo, target, hunk_id, _head_before) = build_drop_conflict_scenario();

    let err_output = repo.squire_json_err(&[
        "--json",
        "drop",
        &target[..8],
        "--pause-on-conflict",
        &hunk_id,
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["rolled_back"].as_bool(), Some(false));
    assert!(
        repo.path().join(".git/rebase-merge").exists()
            || repo.path().join(".git/rebase-apply").exists(),
        "--pause-on-conflict should leave the rebase paused"
    );
    // Clean up.
    let _ = std::process::Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(repo.path())
        .output();
}

// Build a history where squashing a non-adjacent commit into an earlier
// target will conflict: the source's diff was computed against an
// intermediate commit, so reapplying it directly on top of the target
// produces a merge conflict.
// Returns (repo, target_sha, source_sha, pre_command_head_sha).
fn build_squash_conflict_scenario() -> (TestRepo, String, String, String) {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Target: line becomes "a".
    repo.write_file("shared.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target (a)"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Intermediate commit: line becomes "b".
    repo.write_file("shared.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "middle (b)"]);

    // Source: line becomes "c". Source's diff is "b -> c". When folded
    // into target (which has "a"), the apply conflicts because there is
    // no "b" there.
    repo.write_file("shared.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "source (c)"]);
    let source = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    (repo, target, source, head_before)
}

#[test]
fn squash_conflict_rolls_back_by_default() {
    let (repo, target, source, head_before) = build_squash_conflict_scenario();

    let err_output = repo.squire_json_err(&["--json", "squash", &target[..8], &source[..8]]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["conflict"].as_bool(), Some(true));
    assert_eq!(parsed["rolled_back"].as_bool(), Some(true));

    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        head_before, head_after,
        "squash should be atomic on conflict"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress after rolled-back squash"
    );
}

#[test]
fn squash_pause_on_conflict_leaves_rebase_paused() {
    let (repo, target, source, _head_before) = build_squash_conflict_scenario();

    let err_output = repo.squire_json_err(&[
        "--json",
        "squash",
        "--pause-on-conflict",
        &target[..8],
        &source[..8],
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["rolled_back"].as_bool(), Some(false));
    assert!(
        repo.path().join(".git/rebase-merge").exists()
            || repo.path().join(".git/rebase-apply").exists(),
        "--pause-on-conflict should leave the rebase paused"
    );
    let _ = std::process::Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(repo.path())
        .output();
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
fn reword_older_commit_dirty_tree_succeeds_and_preserves_state() {
    let repo = TestRepo::new();
    repo.write_file("seed.txt", "s\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);
    repo.write_file("a.txt", "dirty\n");

    let pre_status = repo.git(&["status", "--porcelain"]);
    repo.squire(&["reword", "HEAD~1", "-m", "reworded"]);
    let post_status = repo.git(&["status", "--porcelain"]);
    assert_eq!(pre_status, post_status, "dirty state must survive reword");
    let msg = repo.git(&["log", "--format=%s", "-1", "HEAD~1"]);
    assert_eq!(msg.trim(), "reworded");
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
fn drop_older_commit_dirty_tree_preserves_state() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "a2\n");
    repo.write_file("b.txt", "b2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);
    repo.write_file("a.txt", "dirty\n");

    let pre_status = repo.git(&["status", "--porcelain"]);
    // Get a hunk ID from the target commit.
    let hunks_out = repo.squire(&["--json", "diff", "HEAD~2", "HEAD~1"]);
    let hunks: serde_json::Value = serde_json::from_str(&hunks_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap().to_string();

    repo.squire(&["drop", &target[..8], &id]);
    let post_status = repo.git(&["status", "--porcelain"]);
    assert_eq!(pre_status, post_status, "dirty state must survive drop");
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
fn squash_dirty_working_tree_preserves_state() {
    let repo = TestRepo::new();
    repo.write_file("seed.txt", "s\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("a.txt", "dirty\n");
    let pre_status = repo.git(&["status", "--porcelain"]);

    repo.squire(&["squash", "HEAD~1", "HEAD"]);

    let post_status = repo.git(&["status", "--porcelain"]);
    assert_eq!(pre_status, post_status, "dirty state must survive squash");
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
    // When amend-into-older triggers a rebase conflict, squire aborts the
    // rebase and rolls back the pre-rebase fixup commit, so the working
    // history is unchanged. The error message still points at the
    // conflicting file so the user knows why the amend could not land.
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

    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

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

    // Atomic amend: HEAD is back where it started, no dangling fixup,
    // no rebase in progress.
    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        head_before, head_after,
        "amend should be atomic on conflict"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should remain in progress"
    );
}

#[test]
fn status_shows_conflicts_during_rebase() {
    // Use a direct git rebase to get into a mid-rebase conflict state;
    // squire amend no longer leaves the rebase paused on conflict.
    let repo = TestRepo::with_rebase_conflict();

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
fn status_no_conflicts_field_when_clean() {
    let repo = TestRepo::with_committed_file("a.txt", "old\n", "new\n");

    let output = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert!(status.get("conflicts").is_none());
}

#[test]
fn status_plain_shows_conflicts() {
    let repo = TestRepo::with_rebase_conflict();

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
fn cleanup_best_match_shows_similarity() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: one commit with no exact message match in master
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add the feature"]);
    repo.git(&["checkout", "main"]);

    // Master: similar message, different patch
    repo.write_file("h.txt", "other\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add the feature flag"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    let commit = &feature["commits"][0];
    let best = &commit["best_match"];
    assert!(
        !best.is_null(),
        "expected best_match for unmerged commit with similar message"
    );
    assert!(best["message_similarity"].as_f64().unwrap() > 0.5);
    assert!(best["diff_similarity"].is_number());
    assert!(!best["sha"].as_str().unwrap().is_empty());
}

#[test]
fn cleanup_no_best_match_for_applied_commits() {
    // Commits that are patch-applied should not have best_match
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    repo.git(&["checkout", "main"]);

    // Squash-merge: same patch, same message
    repo.write_file("f.txt", "advanced\n");
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
    // Applied commits should not have best_match (it's omitted)
    assert!(feature["commits"][0].get("best_match").is_none());
}

#[test]
fn cleanup_duplicate_subject_not_false_merged_equivalent() {
    // Branch has 2 commits both titled "fix typo" touching different files.
    // Master has only 1 commit titled "fix typo" matching one of them.
    // The second branch commit is genuinely unmerged — should NOT be
    // merged_equivalent.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: two "fix typo" commits touching different files
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.write_file("b.txt", "typo fix b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.git(&["checkout", "main"]);

    // Master: advance, then cherry-pick only the first "fix typo"
    repo.write_file("f.txt", "advanced\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    // Replicate only the a.txt change with the same message
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    // The second "fix typo" (b.txt) is NOT in master — must not be merged_equivalent
    assert_ne!(
        feature["status"], "merged_equivalent",
        "branch has unmerged work but was classified as merged_equivalent"
    );
}

#[test]
fn cleanup_duplicate_subject_all_patches_applied_independently() {
    // Branch has 2 commits both titled "fix typo" touching different files.
    // Master independently has both patches applied (via separate commits
    // with different messages). git cherry says both are applied, and the
    // HashSet says both messages match (master also has a "fix typo").
    // This is actually correct — both patches ARE in master — but the
    // message match is coincidental. The commit-level detail should show
    // patch_applied=true for both.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: two "fix typo" commits
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.write_file("b.txt", "typo fix b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.git(&["checkout", "main"]);

    // Master: advance, then add both patches with different messages
    repo.write_file("f.txt", "advanced\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.write_file("b.txt", "typo fix b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo in b"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    // Both patches are genuinely in master, so merged_equivalent is correct
    assert_eq!(feature["status"], "merged_equivalent");
}

#[test]
fn cleanup_cherry_pick_with_reworded_message_detected() {
    // Branch commit is cherry-picked to master with a different message.
    // git cherry detects the patch is applied, but the current code misses
    // it because msg_match is false. Should be merged_equivalent.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: one commit
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    let branch_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["checkout", "main"]);

    // Master: advance, then cherry-pick with a different message
    repo.write_file("f.txt", "advanced\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    repo.git(&["cherry-pick", &branch_sha]);
    // Reword the cherry-picked commit
    repo.git(&["commit", "--amend", "-m", "feat: add feature (reworded)"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    // git cherry knows the patch is applied — should be merged_equivalent
    assert_eq!(
        feature["status"], "merged_equivalent",
        "cherry-picked commit with reworded message should be detected as merged"
    );
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
fn squash_with_message_on_non_head_target() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    repo.write_file("f.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "source"]);
    repo.write_file("g.txt", "x\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    // Squash source into target with -m; "later" stays on top
    repo.squire(&["--json", "squash", "-m", "replaced", "HEAD~2", "HEAD~1"]);

    // The message should land on the target commit (HEAD~1), not HEAD
    let target_msg = repo.git(&["log", "-1", "--format=%s", "HEAD~1"]);
    assert_eq!(target_msg.trim(), "replaced");

    // HEAD should keep its original message
    let head_msg = repo.git(&["log", "-1", "--format=%s"]);
    assert_eq!(head_msg.trim(), "later");
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
    assert!(!new_hunks[0]["line_hashes"].as_array().unwrap().is_empty());

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
    let repo = TestRepo::with_rebase_conflict();

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "rebasing");
    let conflicts = val["conflicts"].as_array().unwrap();
    assert!(!conflicts.is_empty());
    assert!(conflicts[0]["strategy"].is_string());
    assert!(conflicts[0]["command"].is_string());
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

    // amend is atomic — no rebase should be left in progress for the
    // caller to abort.
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should remain in progress after failed amend"
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

#[test]
fn stage_rename_only_hunk() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Perform rename in the working tree (delete + create)
    // then tell git about it so it detects the rename
    std::fs::rename(repo.path().join("old.txt"), repo.path().join("new.txt")).unwrap();

    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();
    // Should see a delete hunk and an untracked new file hunk
    assert!(!arr.is_empty(), "expected hunks, got: {hunks}");

    // Stage all hunks
    let ids: Vec<String> = arr
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_string())
        .collect();
    let args: Vec<&str> = std::iter::once("stage")
        .chain(ids.iter().map(|s| s.as_str()))
        .collect();
    repo.squire(&args);

    // After staging, git should see the rename
    let status = repo.git(&["status", "--porcelain"]);
    // Either "R  old.txt -> new.txt" or "D old.txt" + "A new.txt"
    assert!(
        status.contains("new.txt"),
        "new.txt should be staged, got: {status}"
    );
}

#[test]
fn drop_rename_only_hunk_from_head() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    repo.git(&["commit", "-m", "rename"]);

    // Get the rename hunk ID from the commit
    let out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // Drop the rename hunk — should undo the rename in the commit
    repo.squire(&["drop", "HEAD", id]);

    // The commit should no longer contain the rename
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(
        !show.contains("old.txt") && !show.contains("new.txt"),
        "rename should be dropped from commit, got: {show}"
    );
}

#[test]
fn amend_rename_into_head() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("old.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "modify"]);

    // Now rename in working tree
    std::fs::rename(repo.path().join("old.txt"), repo.path().join("new.txt")).unwrap();

    let hunks = repo.diff_json();
    let ids: Vec<String> = hunks
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_string())
        .collect();
    let mut args: Vec<&str> = vec!["amend"];
    for id in &ids {
        args.push(id);
    }
    repo.squire(&args);

    // After amend, HEAD should contain the rename
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.trim().is_empty(),
        "working tree should be clean: {status}"
    );
    assert!(repo.path().join("new.txt").exists());
    assert!(!repo.path().join("old.txt").exists());
}

#[test]
fn commit_staged_rename_only() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);

    // The rename is already staged; squire commit should handle it
    let hunks_out = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&hunks_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["commit", "-m", "rename file", id]);

    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("rename file"));
    assert!(repo.path().join("new.txt").exists());
    assert!(!repo.path().join("old.txt").exists());
}

#[test]
fn unstage_rename_only_hunk() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);

    // Rename is staged; get its hunk ID
    let out = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // Unstage the rename
    repo.squire(&["unstage", id]);

    // After unstage, the index should match HEAD (old.txt exists)
    let cached = repo.squire(&["--json", "diff", "--cached"]);
    let cached_hunks: serde_json::Value = serde_json::from_str(&cached).unwrap();
    assert!(
        cached_hunks.as_array().unwrap().is_empty(),
        "nothing should be staged after unstage"
    );
}

#[test]
fn revert_staged_rename_only_hunk() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);

    let out = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // Revert the rename — should restore old.txt in both index and worktree
    repo.squire(&["revert", id]);

    assert!(repo.path().join("old.txt").exists());
    let status = repo.git(&["status", "--porcelain"]);
    // new.txt may remain as untracked, but old.txt should be restored
    assert!(
        !status.contains("D  old.txt") && !status.contains("D old.txt"),
        "old.txt should not be deleted, got: {status}"
    );
}

#[test]
fn drop_rename_only_hunk_from_older_commit() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    repo.git(&["commit", "-m", "rename"]);
    repo.write_file("other.txt", "stuff\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    let out = repo.squire(&["--json", "diff", "HEAD~2", "HEAD~1"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    // Drop the rename from the older commit
    repo.squire(&["drop", "HEAD~1", id]);

    // The rename commit should now be empty (or gone)
    // old.txt should exist in the tree at HEAD
    let content = repo.git(&["show", "HEAD:old.txt"]);
    assert_eq!(content, "hello\n");
}

#[test]
fn amend_content_into_commit_with_rename() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.write_file("b.txt", "world\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Commit that renames + modifies another file
    repo.git(&["mv", "a.txt", "a_renamed.txt"]);
    repo.write_file("b.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "rename and modify"]);
    repo.write_file("c.txt", "extra\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    // Now amend a new change into HEAD
    repo.write_file("c.txt", "extra updated\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["amend", id]);

    // The rename from the earlier commit should still be intact
    assert!(repo.path().join("a_renamed.txt").exists());
    assert!(!repo.path().join("a.txt").exists());
}

#[test]
fn stash_file_deletion_hunk() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.write_file("b.txt", "world\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    std::fs::remove_file(repo.path().join("a.txt")).unwrap();

    let hunks = repo.diff_json();
    let del_hunk = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "/dev/null")
        .expect("should have a deletion hunk");
    let id = del_hunk["id"].as_str().unwrap();

    repo.squire(&["stash", id]);

    // After stash, a.txt should be restored in the working tree
    assert!(
        repo.path().join("a.txt").exists(),
        "a.txt should be restored after stashing the deletion"
    );

    // Pop the stash — a.txt should be deleted again
    repo.git(&["stash", "pop"]);
    assert!(!repo.path().join("a.txt").exists());
}

#[test]
fn stash_new_file_hunk() {
    let repo = TestRepo::new();
    repo.write_file("existing.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("new.txt", "brand new\n");

    let hunks = repo.diff_json();
    let new_hunk = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["old_file"].as_str().unwrap() == "/dev/null")
        .expect("should have a new-file hunk");
    let id = new_hunk["id"].as_str().unwrap();

    repo.squire(&["stash", id]);

    // After stash, new.txt should be gone from working tree
    assert!(
        !repo.path().join("new.txt").exists(),
        "new.txt should be removed after stashing"
    );

    // Pop the stash — new.txt should reappear
    repo.git(&["stash", "pop"]);
    assert!(repo.path().join("new.txt").exists());
}

#[test]
fn revert_file_deletion_restores_file() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    std::fs::remove_file(repo.path().join("a.txt")).unwrap();

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["revert", id]);

    assert!(repo.path().join("a.txt").exists());
    let content = std::fs::read_to_string(repo.path().join("a.txt")).unwrap();
    assert_eq!(content, "hello\n");
}

#[test]
fn commit_file_deletion() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.write_file("b.txt", "world\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    std::fs::remove_file(repo.path().join("a.txt")).unwrap();

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["commit", "-m", "delete a", id]);

    let log = repo.git(&["log", "--oneline", "-1"]);
    assert!(log.contains("delete a"));
    // Verify the file is actually deleted in the commit
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("a.txt"));
}

#[test]
fn drop_file_deletion_from_head() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.write_file("b.txt", "world\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["rm", "a.txt"]);
    repo.write_file("b.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "delete and modify"]);

    let out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let del_hunk = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "/dev/null")
        .expect("should have deletion hunk");
    let id = del_hunk["id"].as_str().unwrap();

    repo.squire(&["drop", "HEAD", id]);

    // The deletion should be removed from the commit
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(
        !show.contains("a.txt"),
        "a.txt deletion should be dropped from commit, got: {show}"
    );
    // b.txt modification should still be there
    assert!(show.contains("b.txt"));
    // a.txt should still exist in the commit tree
    let a_content = repo.git(&["show", "HEAD:a.txt"]);
    assert_eq!(a_content, "hello\n");
}

#[test]
fn drop_new_file_from_head() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("b.txt", "new file\n");
    repo.write_file("a.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add b and modify a"]);

    let out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let new_hunk = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["old_file"].as_str().unwrap() == "/dev/null")
        .expect("should have new-file hunk");
    let id = new_hunk["id"].as_str().unwrap();

    repo.squire(&["drop", "HEAD", id]);

    // b.txt should not be in the commit
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(!show.contains("b.txt"), "b.txt should be dropped: {show}");
    assert!(show.contains("a.txt"));
}

#[test]
fn squash_commits_with_rename() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "a.txt", "b.txt"]);
    repo.git(&["commit", "-m", "rename"]);
    repo.write_file("b.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "modify"]);

    // Squash the modify commit into the rename commit
    repo.squire(&["squash", "HEAD~1", "HEAD"]);

    // Should have 2 commits: init + squashed
    let log = repo.git(&["log", "--oneline"]);
    let lines: Vec<&str> = log.trim().lines().collect();
    assert_eq!(lines.len(), 2, "expected 2 commits, got: {log}");
    // b.txt should exist with changed content
    let content = std::fs::read_to_string(repo.path().join("b.txt")).unwrap();
    assert_eq!(content, "changed\n");
    assert!(!repo.path().join("a.txt").exists());
}

#[test]
fn squash_non_adjacent_commits_folds_into_target() {
    // Regression: when source is not directly after target in history,
    // seqedit must move the fixup line next to the target, not leave it
    // in place (where it would fixup into the wrong commit).
    let repo = TestRepo::new();
    repo.write_file("a.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("a.txt", "target\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.write_file("b.txt", "intermediate\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "intermediate"]);

    repo.write_file("c.txt", "source\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "source"]);
    let source = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.write_file("d.txt", "later\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    repo.squire(&["squash", &target[..8], &source[..8]]);

    // Target commit must have a NEW sha (its tree changed)
    let new_target_sha = repo.git(&["rev-parse", "HEAD~2"]).trim().to_string();
    assert_ne!(
        new_target_sha, target,
        "target SHA must change after squash"
    );

    // c.txt (from source) should be in the target commit
    let target_files = repo.git(&["show", "--stat", "HEAD~2"]);
    assert!(
        target_files.contains("c.txt"),
        "source changes should be in target commit, got: {target_files}"
    );

    // Should be 4 commits: base, target (with source folded), intermediate, later
    let log = repo.git(&["log", "--oneline"]);
    assert!(!log.contains("source"), "source commit should be gone");
    assert!(log.contains("target"));
    assert!(log.contains("intermediate"));
    assert!(log.contains("later"));
}

#[test]
fn split_commit_with_rename() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.write_file("b.txt", "world\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "a.txt", "a_renamed.txt"]);
    repo.write_file("b.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "rename and modify"]);

    repo.squire(&["split", "HEAD"]);

    // After split, changes should be unstaged
    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();
    // Should see the rename (as delete + add) and the b.txt modification
    assert!(
        arr.len() >= 2,
        "expected at least 2 hunks after split, got: {hunks}"
    );
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
fn status_shows_staged_rename() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);

    let out = repo.squire(&["--json", "status"]);
    let status: serde_json::Value = serde_json::from_str(&out).unwrap();
    let staged = status["staged"].as_array().unwrap();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0]["old_file"].as_str().unwrap(), "old.txt");
    assert_eq!(staged[0]["file"].as_str().unwrap(), "new.txt");
}

#[test]
fn drop_rename_with_content_change_from_head() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "line1\nline2\nline3\nline4\nline5\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    // Small edit preserving enough similarity for git to detect rename
    repo.write_file("new.txt", "line1\nline2\nchanged\nline4\nline5\n");
    repo.write_file("extra.txt", "keep\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "rename with edit"]);

    let out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = hunks.as_array().unwrap();

    // The rename+content hunk has old_file=old.txt, file=new.txt
    let rename_hunk = arr
        .iter()
        .find(|h| {
            h["old_file"].as_str().unwrap() == "old.txt" && h["file"].as_str().unwrap() == "new.txt"
        })
        .expect("should have rename+content hunk");
    let id = rename_hunk["id"].as_str().unwrap();

    repo.squire(&["drop", "HEAD", id]);

    // Both the content change AND the rename should be undone
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(
        !show.contains("old.txt") || !show.contains("new.txt"),
        "rename should be fully dropped from commit, got: {show}"
    );
    // old.txt should exist in the commit with original content
    let content = repo.git(&["show", "HEAD:old.txt"]);
    assert_eq!(content, "line1\nline2\nline3\nline4\nline5\n");
}

#[test]
fn amend_into_older_commit_with_rename_in_between() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.write_file("b.txt", "world\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "modify a"]);
    repo.git(&["mv", "b.txt", "b_renamed.txt"]);
    repo.git(&["commit", "-m", "rename b"]);

    // Amend a new change into the "modify a" commit (HEAD~1)
    repo.write_file("a.txt", "changed again\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["amend", "--commit", "HEAD~1", id]);

    // The rename from the later commit should still be intact
    assert!(repo.path().join("b_renamed.txt").exists());
    assert!(!repo.path().join("b.txt").exists());
    let a_content = std::fs::read_to_string(repo.path().join("a.txt")).unwrap();
    assert_eq!(a_content, "changed again\n");
}

#[test]
fn commit_file_addition_and_deletion_together() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.write_file("b.txt", "world\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Delete a.txt and add c.txt
    std::fs::remove_file(repo.path().join("a.txt")).unwrap();
    repo.write_file("c.txt", "new content\n");

    let hunks = repo.diff_json();
    let ids: Vec<String> = hunks
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_string())
        .collect();
    let mut args: Vec<&str> = vec!["commit", "-m", "delete a, add c"];
    for id in &ids {
        args.push(id);
    }
    repo.squire(&args);

    assert!(!repo.path().join("a.txt").exists());
    assert!(repo.path().join("c.txt").exists());
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("a.txt"));
    assert!(show.contains("c.txt"));
}

#[test]
fn revert_staged_file_deletion() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "hello\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["rm", "a.txt"]);

    // The deletion is staged
    let out = repo.squire(&["--json", "diff", "--cached"]);
    let hunks: serde_json::Value = serde_json::from_str(&out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap();

    repo.squire(&["revert", id]);

    // File should be restored in both index and working tree
    assert!(repo.path().join("a.txt").exists());
    let content = std::fs::read_to_string(repo.path().join("a.txt")).unwrap();
    assert_eq!(content, "hello\n");
    let status = repo.git(&["status", "--porcelain"]);
    assert!(status.trim().is_empty(), "should be clean: {status}");
}

#[test]
fn stash_rename_as_delete_and_add() {
    let repo = TestRepo::new();
    repo.write_file("old.txt", "hello\n");
    repo.write_file("keep.txt", "stay\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Rename via filesystem (shows as delete + untracked add)
    std::fs::rename(repo.path().join("old.txt"), repo.path().join("new.txt")).unwrap();
    repo.write_file("keep.txt", "modified\n");

    let hunks = repo.diff_json();
    let arr = hunks.as_array().unwrap();
    // Stash only the rename hunks (delete + add), keep the modification
    let rename_ids: Vec<String> = arr
        .iter()
        .filter(|h| {
            let f = h["file"].as_str().unwrap();
            let of = h["old_file"].as_str().unwrap();
            f != "keep.txt" && of != "keep.txt"
        })
        .map(|h| h["id"].as_str().unwrap().to_string())
        .collect();
    let mut args: Vec<&str> = vec!["stash"];
    for id in &rename_ids {
        args.push(id);
    }
    repo.squire(&args);

    // old.txt should be restored, new.txt gone
    assert!(repo.path().join("old.txt").exists());
    assert!(!repo.path().join("new.txt").exists());
    // keep.txt modification should still be in working tree
    let keep = std::fs::read_to_string(repo.path().join("keep.txt")).unwrap();
    assert_eq!(keep, "modified\n");
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

// ── Coverage: rebase.rs plain-text paths ──

#[test]
fn rebase_up_to_date_plain_text_with_unpushed_commits() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin/main at init.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Add a local commit so ahead > 0 but behind == 0.
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("up to date"), "got: {out}");
    assert!(out.contains("unpushed commit"), "got: {out}");
}

#[test]
fn rebase_up_to_date_plain_text_no_unpushed() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["branch", "upstream"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("up to date"), "got: {out}");
    assert!(!out.contains("unpushed"), "got: {out}");
}

#[test]
fn rebase_onto_overrides_upstream() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a target branch with a divergent commit.
    repo.git(&["checkout", "-b", "target"]);
    repo.write_file("g.txt", "target\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target commit"]);

    // Go back to main and add a commit.
    repo.git(&["checkout", "main"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main commit"]);

    let out = repo.squire(&["--json", "rebase", "--onto", "target"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["upstream"], "target");
}

#[test]
fn rebase_in_progress_no_conflicts_plain_text() {
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

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("no conflicts"), "got: {out}");
    assert!(out.contains("rebase --continue"), "got: {out}");

    repo.git(&["rebase", "--abort"]);
}

#[test]
fn rebase_in_progress_with_conflicts_plain_text() {
    let repo = TestRepo::with_rebase_conflict();

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("conflict"), "got: {out}");
    assert!(out.contains("Resolve conflicts"), "got: {out}");
    assert!(out.contains("rebase --continue"), "got: {out}");

    repo.git(&["rebase", "--abort"]);
}

// ── Coverage: git.rs stash with message ──

#[test]
fn stash_with_message() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("a.txt", "a-modified\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["stash", "-m", "my stash msg", id]);

    let stash_list = repo.git(&["stash", "list"]);
    assert!(stash_list.contains("my stash msg"), "got: {stash_list}");
}

// ── Coverage: git.rs upstream_ref paths ──

#[test]
fn rebase_uses_configured_upstream() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin and a tracking branch.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);

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
    assert_eq!(val["upstream"], "origin/main");
}

#[test]
fn rebase_falls_back_to_origin_branch() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Create a feature branch with no tracking, but origin/feature exists.
    repo.git(&["checkout", "-b", "feature"]);
    repo.git(&["push", "origin", "feature"]);

    // Advance origin/feature.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/feature", &origin_sha]);

    // Reset local and diverge.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["upstream"], "origin/feature");
}

// ── Coverage: git.rs detect_master_branch remote HEAD path ──

#[test]
fn rebase_detects_master_via_remote_head() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin with symbolic HEAD pointing to main.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);
    // Set refs/remotes/origin/HEAD -> refs/remotes/origin/main
    repo.git(&["remote", "set-head", "origin", "main"]);

    // Create a feature branch with no tracking.
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);

    // Advance origin/main.
    repo.git(&["checkout", "main"]);
    repo.write_file("g.txt", "main-only\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main advance"]);
    let sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &sha]);
    repo.git(&["checkout", "feature"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["upstream"], "origin/main");
}

// ─────────────────────────────────────────────────────────────────────
// No-data-loss invariants for mutating commands.
//
// Every command that can mutate the working tree, index, or HEAD must
// leave UNRELATED state byte-for-byte identical to pre-command state.
// "Unrelated" = anything the user did not explicitly pass as a hunk ID
// (or, for message-only commands like reword, everything).
//
// These tests assert the invariant across rollback paths and across the
// successful-path scoping. They are the regression suite for five bugs:
//   1. amend --commit <non-HEAD>: rollback lost staged-by-squire hunks
//   2. amend HEAD: folded pre-existing staged content into the amend
//   3. drop HEAD: folded pre-existing staged content into the amend
//   4. reword HEAD: folded pre-existing staged content into the reword
//   5. revert (staged hunks): two-step apply failed partially
// ─────────────────────────────────────────────────────────────────────

/// Snapshot the full state of a repo into a comparable structure so
/// tests can assert that unrelated state survives a command.
fn snapshot_state(repo: &TestRepo) -> (String, String, Vec<(String, String)>) {
    // (HEAD sha, porcelain status, list of (path, content) for every
    // present file including untracked).
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let status = repo.git(&["status", "--porcelain"]);
    let mut files: Vec<(String, String)> = Vec::new();
    fn walk(root: &std::path::Path, dir: &std::path::Path, files: &mut Vec<(String, String)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let rel = p.strip_prefix(root).unwrap();
                if rel.starts_with(".git") {
                    continue;
                }
                if p.is_dir() {
                    walk(root, &p, files);
                } else if p.is_file() {
                    let content = std::fs::read_to_string(&p).unwrap_or_default();
                    files.push((rel.to_string_lossy().to_string(), content));
                }
            }
        }
    }
    walk(repo.path(), repo.path(), &mut files);
    files.sort();
    (head, status, files)
}

/// Bug 1: `squire amend --commit <non-HEAD>` that hits a rebase conflict
/// MUST restore the working tree, index, and untracked files to the
/// exact pre-command state. Previously it would silently drop the hunks
/// that were staged by `stage_hunks_or_cached` before the atomic scope.
#[test]
fn amend_commit_rollback_preserves_staged_hunks() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "L1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "L1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "L1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    // User edits that should survive the rollback:
    //   - unstaged edit on shared.txt (targeted by the amend)
    //   - unstaged edit on side.txt (not targeted)
    //   - untracked fresh.txt (not targeted)
    repo.write_file("shared.txt", "L1 v2 amended\n");
    repo.write_file("side.txt", "side work\n");
    repo.git(&["add", "side.txt"]);
    repo.write_file("side.txt", "side work + more\n");
    repo.write_file("fresh.txt", "untracked\n");

    let pre = snapshot_state(&repo);

    let hunks = repo.diff_json();
    let id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "shared.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    // Expect an error (rebase conflict).
    let _err = repo.squire_err(&["amend", "--commit", &target[..8], id]);

    let post = snapshot_state(&repo);
    assert_eq!(pre.0, post.0, "HEAD changed after rollback");
    assert_eq!(
        pre.1, post.1,
        "porcelain status differs after rollback:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents differ after rollback");
    // No orphan stashes.
    let stash_list = repo.git(&["stash", "list"]);
    assert!(
        stash_list.is_empty(),
        "orphan stash left behind: {stash_list}"
    );
}

/// Bug 2: `squire amend HEAD <hunk>` must NOT silently fold pre-existing
/// staged content that the user did not name into the amended commit.
/// Unrelated staged content must stay staged; only the named hunks are
/// folded into HEAD.
#[test]
fn amend_head_preserves_unrelated_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a-v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "orig"]);

    // User stages `other.txt` (a new file) as separate work in progress.
    repo.write_file("other.txt", "other work\n");
    repo.git(&["add", "other.txt"]);
    // User also has an unstaged edit to a.txt that they want amended.
    repo.write_file("a.txt", "a-v2\n");

    let hunks = repo.diff_json();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    repo.squire(&["amend", a_id]);

    // HEAD should include only a.txt's change, not other.txt.
    let head_stat = repo.git(&["show", "HEAD", "--stat"]);
    assert!(
        head_stat.contains("a.txt"),
        "a.txt should be in the amended HEAD: {head_stat}"
    );
    assert!(
        !head_stat.contains("other.txt"),
        "other.txt must NOT be folded into amended HEAD: {head_stat}"
    );
    // other.txt must still be staged (not lost, not amended).
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  other.txt"),
        "other.txt should still be staged after amend HEAD: {status}"
    );
}

/// Bug 3: `squire drop HEAD <hunk>` must NOT silently fold pre-existing
/// staged content into the amended commit.
#[test]
fn drop_head_preserves_unrelated_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("a.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "change-to-drop"]);

    // User stages separate work `other.txt` as a new file.
    repo.write_file("other.txt", "other\n");
    repo.git(&["add", "other.txt"]);

    let hunks = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let parsed: serde_json::Value = serde_json::from_str(&hunks).unwrap();
    let id = parsed[0]["id"].as_str().unwrap().to_string();

    repo.squire(&["drop", "HEAD", &id]);

    let head_stat = repo.git(&["show", "HEAD", "--stat"]);
    assert!(
        !head_stat.contains("other.txt"),
        "other.txt must NOT be folded into dropped HEAD commit: {head_stat}"
    );
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  other.txt"),
        "other.txt should still be staged after drop HEAD: {status}"
    );
}

/// Bug 4: `squire reword HEAD` must NOT silently fold pre-existing
/// staged content into the reworded commit. reword is a message-only
/// operation.
#[test]
fn reword_head_preserves_unrelated_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "orig"]);

    // User stages separate work.
    repo.write_file("other.txt", "other\n");
    repo.git(&["add", "other.txt"]);

    repo.squire(&["reword", "HEAD", "-m", "new msg"]);

    let head_stat = repo.git(&["show", "HEAD", "--stat"]);
    assert!(
        !head_stat.contains("other.txt"),
        "other.txt must NOT be folded into reworded HEAD: {head_stat}"
    );
    let head_msg = repo.git(&["log", "-1", "--format=%s"]);
    assert_eq!(head_msg.trim(), "new msg");
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  other.txt"),
        "other.txt should still be staged after reword: {status}"
    );
}

/// Bug 5: `squire revert <staged-hunk>` must not leave the index in a
/// partial state if the two-step apply fails. When the underlying
/// `git apply --cached --reverse` succeeds but the working-tree apply
/// fails, previously the index was mutated and the user had to debug it.
/// Now the whole operation is rolled back.
#[test]
fn revert_staged_hunk_rolls_back_on_worktree_apply_failure() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "A\nB\nC\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Stage A -> A2. Then make the working tree also modify C -> C2,
    // so that reverse-applying the staged hunk against the working
    // tree context "A2,B,C" fails (the context line C is actually C2).
    repo.write_file("f.txt", "A2\nB\nC\n");
    repo.git(&["add", "f.txt"]);
    repo.write_file("f.txt", "A2\nB\nC2\n");

    let pre = snapshot_state(&repo);

    // Identify the staged hunk id via diff --cached.
    let staged_out = repo.squire(&["--json", "diff", "--cached"]);
    let staged: serde_json::Value = serde_json::from_str(&staged_out).unwrap();
    let id = staged[0]["id"].as_str().unwrap().to_string();

    // Expect failure.
    let _err = repo.squire_err(&["revert", &id]);

    let post = snapshot_state(&repo);
    assert_eq!(pre.0, post.0, "HEAD changed");
    assert_eq!(
        pre.1, post.1,
        "porcelain status differs:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents differ");
}

/// Workflow check: alternating "amend hunks into X; amend other hunks
/// into Y" must work without requiring a clean tree between steps. This
/// guards against the old `is_clean` check coming back as a reflex.
#[test]
fn amend_commit_alternating_workflow() {
    let repo = TestRepo::new();
    // base commit so that X-base has a parent for fixup's `~1` lookup.
    repo.write_file("seed.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("x.txt", "X-v1\n");
    repo.write_file("y.txt", "Y-v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "X-base"]);
    let x_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("z.txt", "Z\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Y-base"]);
    let _y_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("after.txt", "after\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after-both"]);

    // User has unstaged edits to x.txt (for x) and y.txt (for y).
    repo.write_file("x.txt", "X-v2\n");
    repo.write_file("y.txt", "Y-v2\n");

    // Find hunk IDs for each file.
    let hunks = repo.diff_json();
    let x_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "x.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Amend x.txt into X-base.
    repo.squire(&["amend", "--commit", &x_sha[..8], &x_id]);

    // Tree is NOT clean — y.txt is still unstaged. After the first
    // amend the original y_sha was rewritten; re-resolve via HEAD~1.
    let new_y_sha = repo.git(&["rev-parse", "HEAD~1"]).trim().to_string();
    assert_ne!(
        new_y_sha, _y_sha,
        "first amend should have rewritten Y-base"
    );
    let status_after_first = repo.git(&["status", "--porcelain"]);
    assert!(
        status_after_first.contains("y.txt"),
        "y.txt should still be dirty after first amend: {status_after_first}"
    );
    let hunks = repo.diff_json();
    let hunks_str = serde_json::to_string_pretty(&hunks).unwrap();
    let y_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "y.txt")
        .unwrap_or_else(|| panic!("y.txt not in diff after first amend.\nstatus: {status_after_first}\nhunks: {hunks_str}"))["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Amend y.txt into the rewritten Y-base. Must NOT reject on
    // "clean tree" grounds even though x.txt's rewrite just happened.
    repo.squire(&["amend", "--commit", &new_y_sha[..8], &y_id]);

    // Verify X-base now contains x.txt change, Y-base contains y.txt change.
    let x_show = repo.git(&["show", "HEAD~2"]);
    assert!(x_show.contains("X-v2"), "X amend should be in HEAD~2");
    let y_show = repo.git(&["show", "HEAD~1"]);
    assert!(y_show.contains("Y-v2"), "Y amend should be in HEAD~1");
}

/// reword of a non-HEAD commit must not require a clean tree; the user
/// workflow "reword an old commit while keeping other in-progress edits"
/// must work.
#[test]
fn reword_non_head_allows_dirty_tree() {
    let repo = TestRepo::new();
    // Seed commit so target has a parent for the rebase `~1` lookup.
    repo.write_file("seed.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "old msg"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("g.txt", "g\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    // Dirty state unrelated to target.
    repo.write_file("scratch.txt", "scratch\n");
    repo.git(&["add", "scratch.txt"]);
    repo.write_file("untracked.txt", "u\n");

    let pre_status = repo.git(&["status", "--porcelain"]);

    repo.squire(&["reword", &target[..8], "-m", "reworded"]);

    // target's message changed.
    let log = repo.git(&["log", "--format=%s", "-1", "HEAD~1"]);
    assert_eq!(log.trim(), "reworded");
    // Dirty state preserved.
    let post_status = repo.git(&["status", "--porcelain"]);
    assert_eq!(
        pre_status, post_status,
        "unrelated dirty state must survive non-HEAD reword"
    );
}

/// drop of a non-HEAD commit must not require a clean tree.
#[test]
fn drop_non_head_allows_dirty_tree() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "drop-me"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("g.txt", "g\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);

    // Dirty state.
    repo.write_file("scratch.txt", "scratch\n");
    repo.git(&["add", "scratch.txt"]);

    // Hunk id from the target commit.
    let parsed: serde_json::Value =
        serde_json::from_str(&repo.squire(&["--json", "diff", "HEAD~2", "HEAD~1"])).unwrap();
    let id = parsed[0]["id"].as_str().unwrap().to_string();

    repo.squire(&["drop", &target[..8], &id]);

    // scratch.txt should still be staged.
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  scratch.txt"),
        "scratch should still be staged: {status}"
    );
}
