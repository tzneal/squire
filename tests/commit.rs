mod helpers;
use helpers::TestRepo;

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
