mod helpers;
use helpers::TestRepo;

#[test]
fn rebase_no_upstream_returns_error() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    // Rename the only branch so there's no main/master to fall back to.
    repo.git(&["branch", "-m", "main", "dev"]);

    let err = repo.squire_err(&["rebase"]);
    assert!(
        err.contains("no upstream") || err.contains("cannot detect master"),
        "got: {err}"
    );
}

#[test]
fn rebase_dirty_working_tree_fails() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.write_file("f.txt", "dirty\n");

    let err = repo.squire_err(&["rebase"]);
    assert!(err.contains("clean working tree"), "got: {err}");
}

#[test]
fn rebase_up_to_date_json() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a fake upstream by making a second branch and pointing origin/main at it.
    repo.git(&["branch", "upstream"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "up_to_date");
    assert_eq!(val["branch"], "main");
    assert_eq!(val["commits_ahead"], 0);
}

#[test]
fn rebase_falls_back_to_master_branch() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Switch to a feature branch with no tracking ref.
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);

    // Advance main so feature is actually behind.
    repo.git(&["checkout", "main"]);
    repo.write_file("g.txt", "main-only\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main advance"]);
    repo.git(&["checkout", "feature"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["branch"], "feature");
    // Should have fallen back to local main since there's no remote.
    assert_eq!(val["upstream"], "main");
    assert_eq!(val["commits_ahead"], 1);
}

#[test]
fn rebase_ready_creates_tag_and_shows_steps() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin/main pointing at init.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main with a commit the local branch doesn't have.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local main back and add a different commit.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["commits_ahead"], 1);
    let tag = val["safety_tag"].as_str().unwrap();
    assert!(tag.starts_with("pre-rebase/main-"), "tag: {tag}");

    // Verify the tag actually exists.
    let tag_check = repo.git(&["rev-parse", "--verify", tag]);
    assert!(!tag_check.trim().is_empty());

    // Steps should mention the upstream.
    let steps = val["steps"].as_array().unwrap();
    assert!(steps[0].as_str().unwrap().contains("rebase"));
}

#[test]
fn rebase_ready_plain_text() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main independently.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local main back and add a different commit.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("Safety tag:"), "got: {out}");
    assert!(out.contains("pre-rebase/main-"), "got: {out}");
    assert!(out.contains("1 commit(s) ahead"), "got: {out}");
    assert!(out.contains("git rebase --empty=drop"), "got: {out}");
}

#[test]
fn rebase_during_conflict_shows_conflicts() {
    let repo = TestRepo::with_rebase_conflict();

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "rebasing");
    let conflicts = val["conflicts"].as_array().unwrap();
    assert!(!conflicts.is_empty());
    assert!(conflicts[0]["strategy"].is_string());
    assert!(conflicts[0]["command"].is_string());
    assert!(val["steps"].is_array());

    repo.git(&["rebase", "--abort"]);
}

#[test]
fn rebase_during_rebase_no_conflicts() {
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

    // Split middle commit to get into a rebase state without conflicts.
    let second = repo.git(&["rev-parse", "HEAD~1"]);
    repo.squire(&["split", second.trim()]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "rebasing");
    assert!(val.get("conflicts").is_none());
    let steps = val["steps"].as_array().unwrap();
    assert!(steps[0].as_str().unwrap().contains("rebase --continue"));

    repo.git(&["rebase", "--abort"]);
}

#[test]
fn rebase_ready_json_includes_commits_behind() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local and diverge.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(
        val["commits_behind"], 1,
        "ready state should include commits_behind"
    );
}

#[test]
fn rebase_up_to_date_json_includes_commits_behind() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["branch", "upstream"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "up_to_date");
    assert_eq!(
        val["commits_behind"], 0,
        "up_to_date state should include commits_behind"
    );
}

#[test]
fn rebase_ready_deduplicates_safety_tag() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Advance origin/main.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local and diverge.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out1 = repo.squire(&["--json", "rebase"]);
    let val1: serde_json::Value = serde_json::from_str(&out1).unwrap();
    let tag1 = val1["safety_tag"].as_str().unwrap().to_string();

    // Run again immediately — same epoch second likely, tag already exists.
    let out2 = repo.squire(&["--json", "rebase"]);
    let val2: serde_json::Value = serde_json::from_str(&out2).unwrap();
    let tag2 = val2["safety_tag"].as_str().unwrap().to_string();

    // Tags should differ since the first one already existed.
    assert_ne!(tag1, tag2, "second invocation should create a distinct tag");

    // Both tags should exist.
    repo.git(&["rev-parse", "--verify", &tag1]);
    repo.git(&["rev-parse", "--verify", &tag2]);
}

// ── Coverage: rebase.rs plain-text paths ──

#[test]
fn rebase_up_to_date_plain_text_with_unpushed_commits() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin/main at init.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Add a local commit so ahead > 0 but behind == 0.
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("up to date"), "got: {out}");
    assert!(out.contains("unpushed commit"), "got: {out}");
}

#[test]
fn rebase_up_to_date_plain_text_no_unpushed() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);
    repo.git(&["branch", "upstream"]);
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("up to date"), "got: {out}");
    assert!(!out.contains("unpushed"), "got: {out}");
}

#[test]
fn rebase_onto_overrides_upstream() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a target branch with a divergent commit.
    repo.git(&["checkout", "-b", "target"]);
    repo.write_file("g.txt", "target\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target commit"]);

    // Go back to main and add a commit.
    repo.git(&["checkout", "main"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main commit"]);

    let out = repo.squire(&["--json", "rebase", "--onto", "target"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["upstream"], "target");
}

#[test]
fn rebase_in_progress_no_conflicts_plain_text() {
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

    // Split middle commit to get into a rebase state without conflicts.
    let second = repo.git(&["rev-parse", "HEAD~1"]);
    repo.squire(&["split", second.trim()]);

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("no conflicts"), "got: {out}");
    assert!(out.contains("rebase --continue"), "got: {out}");

    repo.git(&["rebase", "--abort"]);
}

#[test]
fn rebase_in_progress_with_conflicts_plain_text() {
    let repo = TestRepo::with_rebase_conflict();

    let out = repo.squire(&["rebase"]);
    assert!(out.contains("conflict"), "got: {out}");
    assert!(out.contains("Resolve conflicts"), "got: {out}");
    assert!(out.contains("rebase --continue"), "got: {out}");

    repo.git(&["rebase", "--abort"]);
}

// ── Coverage: git.rs upstream_ref paths ──

#[test]
fn rebase_uses_configured_upstream() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin and a tracking branch.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);

    // Advance origin/main.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &origin_sha]);

    // Reset local and diverge.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["upstream"], "origin/main");
}

#[test]
fn rebase_falls_back_to_origin_branch() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);

    // Create a feature branch with no tracking, but origin/feature exists.
    repo.git(&["checkout", "-b", "feature"]);
    repo.git(&["push", "origin", "feature"]);

    // Advance origin/feature.
    repo.write_file("g.txt", "origin\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "origin-only"]);
    let origin_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/feature", &origin_sha]);

    // Reset local and diverge.
    repo.git(&["reset", "--hard", "HEAD~1"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "local"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["upstream"], "origin/feature");
}

// ── Coverage: git.rs detect_master_branch remote HEAD path ──

#[test]
fn rebase_detects_master_via_remote_head() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Set up origin with symbolic HEAD pointing to main.
    repo.git(&["remote", "add", "origin", repo.path().to_str().unwrap()]);
    repo.git(&["fetch", "origin"]);
    // Set refs/remotes/origin/HEAD -> refs/remotes/origin/main
    repo.git(&["remote", "set-head", "origin", "main"]);

    // Create a feature branch with no tracking.
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("f.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature commit"]);

    // Advance origin/main.
    repo.git(&["checkout", "main"]);
    repo.write_file("g.txt", "main-only\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "main advance"]);
    let sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["update-ref", "refs/remotes/origin/main", &sha]);
    repo.git(&["checkout", "feature"]);

    let out = repo.squire(&["--json", "rebase"]);
    let val: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(val["state"], "ready");
    assert_eq!(val["upstream"], "origin/main");
}
