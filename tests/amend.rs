mod helpers;
use helpers::TestRepo;

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
