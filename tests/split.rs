mod helpers;
use helpers::TestRepo;

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
