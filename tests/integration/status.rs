use crate::helpers::TestRepo;

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
