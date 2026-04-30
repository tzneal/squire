mod helpers;
use helpers::TestRepo;

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
