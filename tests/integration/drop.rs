use crate::helpers::TestRepo;

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
