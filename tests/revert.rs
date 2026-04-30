mod helpers;
use helpers::TestRepo;

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
