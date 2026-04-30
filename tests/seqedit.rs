mod helpers;
use helpers::TestRepo;

#[test]
fn seqedit_changes_action_in_todo_file() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(
        &todo,
        "pick abc1234 first commit\npick def5678 second commit\n",
    )
    .unwrap();

    let repo = TestRepo::new();
    repo.squire(&["seqedit", "edit:abc1", todo.to_str().unwrap()]);

    let result = std::fs::read_to_string(&todo).unwrap();
    assert!(result.starts_with("edit abc1234 first commit\n"));
    assert!(result.contains("pick def5678 second commit\n"));
}

#[test]
fn seqedit_multiple_actions() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(
        &todo,
        "pick abc1234 first\npick def5678 second\npick 99900ab third\n",
    )
    .unwrap();

    let repo = TestRepo::new();
    repo.squire(&["seqedit", "fixup:def5", "drop:9990", todo.to_str().unwrap()]);

    let result = std::fs::read_to_string(&todo).unwrap();
    assert!(result.starts_with("pick abc1234 first\n"));
    assert!(result.contains("fixup def5678 second\n"));
    assert!(result.contains("drop 99900ab third\n"));
}

#[test]
fn seqedit_unknown_sha_fails() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(&todo, "pick abc1234 first\n").unwrap();

    let repo = TestRepo::new();
    let err = repo.squire_err(&["seqedit", "edit:zzz999", todo.to_str().unwrap()]);
    assert!(err.contains("no todo line matches sha prefix"));
}

#[test]
fn seqedit_invalid_action_fails() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(&todo, "pick abc1234 first\n").unwrap();

    let repo = TestRepo::new();
    let err = repo.squire_err(&["seqedit", "bogus:abc1", todo.to_str().unwrap()]);
    assert!(err.contains("unknown action"));
}

#[test]
fn seqedit_ambiguous_sha_prefix_fails() {
    let dir = tempfile::tempdir().unwrap();
    let todo = dir.path().join("git-rebase-todo");
    std::fs::write(&todo, "pick abc1234 first\npick abc1999 second\n").unwrap();

    let repo = TestRepo::new();
    let err = repo.squire_err(&["seqedit", "edit:abc1", todo.to_str().unwrap()]);
    assert!(
        err.contains("ambiguous"),
        "expected ambiguity error, got: {err}"
    );
}
