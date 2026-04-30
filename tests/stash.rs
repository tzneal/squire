mod helpers;
use helpers::TestRepo;

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
