mod helpers;
use helpers::TestRepo;

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
