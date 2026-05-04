use crate::helpers::TestRepo;

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
