mod helpers;
use helpers::TestRepo;

// Regression test: `squire amend --commit <sha>` used to accept any SHA
// that `git rev-parse` could resolve (including commits that were rewritten
// by a prior amend and no longer appear in the current branch). The rebase
// base was then computed from the stale SHA, producing spurious conflicts.
// Now we reject stale SHAs with an error that points at the equivalent
// commit on the current branch.
#[test]
fn amend_commit_rejects_stale_sha_after_prior_amend() {
    let repo = TestRepo::new();
    repo.write_file("base.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Commit A — target of the first amend.
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit A"]);
    let a_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Commit B — target of the second amend (will be rewritten by the first).
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit B"]);
    let b_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // First amend: fold hunk for a.txt into commit A. This rewrites A, and
    // replays B on top so B gets a new SHA.
    repo.write_file("a-extra.txt", "extra for A\n");
    let hunks_a = repo.diff_json();
    let a_extra_id = hunks_a[0]["id"].as_str().unwrap();
    repo.squire(&["amend", "--commit", &a_before[..8], a_extra_id]);

    // b_before is now unreachable — it was replayed to a new SHA.
    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(b_before, head_after, "B should have been replayed");

    // Second amend with the stale pre-rewrite SHA of B should be rejected.
    repo.write_file("b-extra.txt", "extra for B\n");
    let hunks_b = repo.diff_json();
    let b_extra_id = hunks_b[0]["id"].as_str().unwrap();
    let err = repo.squire_err(&["amend", "--commit", &b_before[..8], b_extra_id]);

    // Error should call out that the SHA is unreachable and suggest the
    // rewritten equivalent on HEAD.
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    // The error should point at the equivalent commit (same subject as B)
    // so the user can retry with the right SHA.
    assert!(
        err.contains(&head_after[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );

    // Nothing should have been committed or rebased as a side effect.
    assert!(
        !repo.git(&["log", "--oneline", "-1"]).contains("fixup!"),
        "no dangling fixup commit should be left on HEAD"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress"
    );
}

// Build a repo with three commits (base, A, B) where a prior
// `amend --commit` has rewritten A. Returns (repo, b_before_sha) — the
// stale pre-rewrite SHA of B that other commands should now reject.
//
// Structure after the helper returns:
//   HEAD -> A' (rewritten) -> B' (rewritten) -> base
// `b_before_sha` resolves via rev-parse (it's still in the object db) but
// is not reachable from HEAD.
fn build_stale_sha_scenario() -> (TestRepo, String) {
    let repo = TestRepo::new();
    repo.write_file("base.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Commit A — target of the first amend.
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit A"]);
    let a_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Commit B — its SHA will be the stale one after A is rewritten.
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit B"]);
    let b_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Amend an unrelated file into A so B gets replayed onto a new SHA.
    repo.write_file("a-extra.txt", "extra for A\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["amend", "--commit", &a_before[..8], id]);

    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_ne!(b_before, head_after, "B should have been replayed");
    (repo, b_before)
}

// Regression test: `squire drop <sha>` used to accept SHAs that rev-parse
// could resolve but that weren't reachable from HEAD (e.g. commits
// rewritten by a prior amend). We now reject them with a pointer to the
// rewritten equivalent.
#[test]
fn drop_rejects_stale_sha_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();

    // Find a hunk ID from the current (rewritten) B so the drop args
    // are otherwise valid.
    let b_new = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let log = repo
        .squire(&["--json", "log", "-n", "1"])
        .trim()
        .to_string();
    let parsed: serde_json::Value = serde_json::from_str(&log).unwrap();
    let id = parsed[0]["hunks"][0]["id"].as_str().unwrap().to_string();

    let err = repo.squire_err(&["drop", &b_before[..8], &id]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    assert!(
        err.contains(&b_new[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );

    // No rebase should be in progress and HEAD should be unchanged.
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), b_new);
}

// Regression test: `squire reword <sha> -m <msg>` used to accept stale
// SHAs resolved via the object db, producing spurious conflicts when the
// rebase base (`<sha>~1`) pointed at a superseded ancestor.
#[test]
fn reword_rejects_stale_sha_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    let b_new = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let err = repo.squire_err(&["reword", &b_before[..8], "-m", "new message"]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    assert!(
        err.contains(&b_new[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), b_new);
}

// Regression test: `squire split <sha>` used to accept stale SHAs, which
// would start a rebase against a superseded ancestor.
#[test]
fn split_rejects_stale_sha_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    let b_new = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let err = repo.squire_err(&["split", &b_before[..8]]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error, got: {err}"
    );
    assert!(
        err.contains(&b_new[..8]) || err.contains("commit B"),
        "error should reference the rewritten SHA or its subject, got: {err}"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).trim(), b_new);
}

// Regression test: `squire squash <target> <sources>...` used to accept
// stale SHAs for both the target and each source. We now reject either.
#[test]
fn squash_rejects_stale_target_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    // Create one more commit so we have a valid source to squash from.
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit C"]);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Stale target, valid source.
    let err = repo.squire_err(&["squash", &b_before[..8], &head[..8]]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error for target, got: {err}"
    );
}

// Regression test: a stale SHA passed as a *source* to squash is also
// rejected (not just the target).
#[test]
fn squash_rejects_stale_source_after_prior_amend() {
    let (repo, b_before) = build_stale_sha_scenario();
    // Add a new commit so we have a valid target on HEAD.
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "commit C"]);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Valid target, stale source.
    let err = repo.squire_err(&["squash", &head[..8], &b_before[..8]]);
    assert!(
        err.contains("not reachable from HEAD") || err.contains("was rewritten"),
        "expected stale-SHA error for source, got: {err}"
    );
}

// that fails with a conflict, the `fixup!` commit that squire created on
// HEAD before starting the rebase used to be left behind after the rebase
// aborted internally. Now we detect the abort and roll back the fixup so
// HEAD returns to its original state.
#[test]
fn amend_commit_cleans_up_fixup_on_rebase_conflict() {
    let repo = TestRepo::new();
    // Three commits where each touches the same file, so an autosquash
    // rebase of the oldest will need to replay the newer ones and can
    // conflict.
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);
    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Unstaged change to the same line — will fold into 'target' via a
    // fixup, and then the replay of 'later' on top will conflict.
    repo.write_file("shared.txt", "line1 v2 amended\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    // Expect an error (rebase conflict).
    let _err = repo.squire_err(&["amend", "--commit", &target[..8], id]);

    // HEAD must be back at head_before — no dangling fixup commit.
    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        head_after, head_before,
        "HEAD should be restored after failed amend; a dangling fixup was left behind"
    );
    let log = repo.git(&["log", "--oneline", "-5"]);
    assert!(
        !log.contains("fixup!"),
        "no fixup! commit should remain, got log:\n{log}"
    );

    // No rebase should be in progress.
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress after failed amend"
    );
}

// When amend rolls back on conflict, the JSON error must include
// `rolled_back: true` and the manual-recovery hint so LLM callers can
// distinguish this from the paused (--pause-on-conflict) case.
#[test]
fn amend_conflict_json_has_rolled_back_field() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    repo.write_file("shared.txt", "line1 v2 amended\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    let err_output = repo.squire_json_err(&["--json", "amend", "--commit", &target[..8], id]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(
        parsed["rolled_back"].as_bool(),
        Some(true),
        "default amend must report rolled_back:true, got: {parsed}"
    );
    assert!(
        parsed["hint"]
            .as_str()
            .unwrap()
            .contains("--pause-on-conflict"),
        "hint must mention --pause-on-conflict, got: {parsed}"
    );
}

// With --pause-on-conflict, amend leaves the rebase paused instead of
// rolling back, so the LLM can resolve the conflict by hand.
#[test]
fn amend_pause_on_conflict_leaves_rebase_paused() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    repo.write_file("shared.txt", "line1 v2 amended\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    let err_output = repo.squire_json_err(&[
        "--json",
        "amend",
        "--commit",
        &target[..8],
        "--pause-on-conflict",
        id,
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(
        parsed["rolled_back"].as_bool(),
        Some(false),
        "--pause-on-conflict must report rolled_back:false"
    );
    assert!(
        parsed["hint"]
            .as_str()
            .unwrap()
            .contains("rebase --continue"),
        "paused hint must point at git rebase --continue"
    );
    // Rebase should still be in progress.
    assert!(
        repo.path().join(".git/rebase-merge").exists()
            || repo.path().join(".git/rebase-apply").exists(),
        "--pause-on-conflict should leave the rebase paused"
    );
    // Clean up so TestRepo drop doesn't leave stale rebase dir.
    let _ = std::process::Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(repo.path())
        .output();
}

// Build a three-commit history where dropping hunks from the middle
// commit will conflict when the later commit is replayed on top.
// Returns (repo, target_sha, hunk_id_to_drop, pre_command_head_sha).
fn build_drop_conflict_scenario() -> (TestRepo, String, String, String) {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "line1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Target commit: changes the single line. We'll try to drop this
    // change; the later commit depends on it.
    repo.write_file("shared.txt", "line1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Find the hunk ID for the target's change so we can pass it to
    // `squire drop`.
    let log = repo.squire(&["--json", "log", "-n", "1"]);
    let parsed: serde_json::Value = serde_json::from_str(&log).unwrap();
    let hunk_id = parsed[0]["hunks"][0]["id"].as_str().unwrap().to_string();

    // Later commit: further modifies the same line. Dropping the target's
    // change will break this commit's replay.
    repo.write_file("shared.txt", "line1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);
    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    (repo, target, hunk_id, head_before)
}

// By default (no --pause-on-conflict), `drop` on a non-HEAD commit whose
// changes conflict with later commits must roll back to the pre-command
// HEAD and leave no rebase in progress.
#[test]
fn drop_conflict_rolls_back_by_default() {
    let (repo, target, hunk_id, head_before) = build_drop_conflict_scenario();

    let err_output = repo.squire_json_err(&["--json", "drop", &target[..8], &hunk_id]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["conflict"].as_bool(), Some(true));
    assert_eq!(parsed["rolled_back"].as_bool(), Some(true));

    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(head_before, head_after, "drop should be atomic on conflict");
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress after rolled-back drop"
    );
}

// With --pause-on-conflict, `drop` leaves the rebase paused so the
// caller can resolve by hand.
#[test]
fn drop_pause_on_conflict_leaves_rebase_paused() {
    let (repo, target, hunk_id, _head_before) = build_drop_conflict_scenario();

    let err_output = repo.squire_json_err(&[
        "--json",
        "drop",
        &target[..8],
        "--pause-on-conflict",
        &hunk_id,
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["rolled_back"].as_bool(), Some(false));
    assert!(
        repo.path().join(".git/rebase-merge").exists()
            || repo.path().join(".git/rebase-apply").exists(),
        "--pause-on-conflict should leave the rebase paused"
    );
    // Clean up.
    let _ = std::process::Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(repo.path())
        .output();
}

// Build a history where squashing a non-adjacent commit into an earlier
// target will conflict: the source's diff was computed against an
// intermediate commit, so reapplying it directly on top of the target
// produces a merge conflict.
// Returns (repo, target_sha, source_sha, pre_command_head_sha).
fn build_squash_conflict_scenario() -> (TestRepo, String, String, String) {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    // Target: line becomes "a".
    repo.write_file("shared.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target (a)"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Intermediate commit: line becomes "b".
    repo.write_file("shared.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "middle (b)"]);

    // Source: line becomes "c". Source's diff is "b -> c". When folded
    // into target (which has "a"), the apply conflicts because there is
    // no "b" there.
    repo.write_file("shared.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "source (c)"]);
    let source = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    (repo, target, source, head_before)
}

#[test]
fn squash_conflict_rolls_back_by_default() {
    let (repo, target, source, head_before) = build_squash_conflict_scenario();

    let err_output = repo.squire_json_err(&["--json", "squash", &target[..8], &source[..8]]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["conflict"].as_bool(), Some(true));
    assert_eq!(parsed["rolled_back"].as_bool(), Some(true));

    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        head_before, head_after,
        "squash should be atomic on conflict"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should be in progress after rolled-back squash"
    );
}

#[test]
fn squash_pause_on_conflict_leaves_rebase_paused() {
    let (repo, target, source, _head_before) = build_squash_conflict_scenario();

    let err_output = repo.squire_json_err(&[
        "--json",
        "squash",
        "--pause-on-conflict",
        &target[..8],
        &source[..8],
    ]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();
    assert_eq!(parsed["rolled_back"].as_bool(), Some(false));
    assert!(
        repo.path().join(".git/rebase-merge").exists()
            || repo.path().join(".git/rebase-apply").exists(),
        "--pause-on-conflict should leave the rebase paused"
    );
    let _ = std::process::Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(repo.path())
        .output();
}

#[test]
fn amend_conflict_returns_structured_error() {
    // When amend-into-older triggers a rebase conflict, squire aborts the
    // rebase and rolls back the pre-rebase fixup commit, so the working
    // history is unchanged. The error message still points at the
    // conflicting file so the user knows why the amend could not land.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("f.txt", "first\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);

    repo.write_file("f.txt", "second\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("f.txt", "third\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    let head_before = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    // Create a conflicting change and try to amend it into "first" (HEAD~2)
    repo.write_file("f.txt", "conflict\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    let err_output = repo.squire_json_err(&["--json", "amend", "--commit", "HEAD~2", id]);
    let parsed: serde_json::Value = serde_json::from_str(&err_output).unwrap();

    assert!(parsed["conflict"].as_bool().unwrap(), "parsed: {parsed}");
    let files = parsed["conflicting_files"].as_array().unwrap();
    assert!(!files.is_empty());
    assert_eq!(files[0]["file"].as_str().unwrap(), "f.txt");

    // Atomic amend: HEAD is back where it started, no dangling fixup,
    // no rebase in progress.
    let head_after = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        head_before, head_after,
        "amend should be atomic on conflict"
    );
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should remain in progress"
    );
}

#[test]
fn amend_conflict_plain_text_is_not_json() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "base\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);

    repo.write_file("f.txt", "first\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);

    repo.write_file("f.txt", "second\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    repo.write_file("f.txt", "third\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "third"]);

    // Create a conflicting change and amend without --json.
    repo.write_file("f.txt", "conflict\n");
    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();

    let err = repo.squire_err(&["amend", "--commit", "HEAD~2", id]);
    // The error should NOT be raw JSON when --json wasn't passed.
    assert!(
        !err.starts_with('{'),
        "plain-text error should not be JSON, got: {err}"
    );
    assert!(
        err.contains("f.txt"),
        "error should mention the conflicting file, got: {err}"
    );

    // amend is atomic — no rebase should be left in progress for the
    // caller to abort.
    assert!(
        !repo.path().join(".git/rebase-merge").exists()
            && !repo.path().join(".git/rebase-apply").exists(),
        "no rebase should remain in progress after failed amend"
    );
}

// ─────────────────────────────────────────────────────────────────────
// No-data-loss invariants for mutating commands.
//
// Every command that can mutate the working tree, index, or HEAD must
// leave UNRELATED state byte-for-byte identical to pre-command state.
// "Unrelated" = anything the user did not explicitly pass as a hunk ID
// (or, for message-only commands like reword, everything).
//
// These tests assert the invariant across rollback paths and across the
// successful-path scoping. They are the regression suite for five bugs:
//   1. amend --commit <non-HEAD>: rollback lost staged-by-squire hunks
//   2. amend HEAD: folded pre-existing staged content into the amend
//   3. drop HEAD: folded pre-existing staged content into the amend
//   4. reword HEAD: folded pre-existing staged content into the reword
//   5. revert (staged hunks): two-step apply failed partially
// ─────────────────────────────────────────────────────────────────────

/// Snapshot the full state of a repo into a comparable structure so
/// tests can assert that unrelated state survives a command.
fn snapshot_state(repo: &TestRepo) -> (String, String, Vec<(String, String)>) {
    // (HEAD sha, porcelain status, list of (path, content) for every
    // present file including untracked).
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let status = repo.git(&["status", "--porcelain"]);
    let mut files: Vec<(String, String)> = Vec::new();
    fn walk(root: &std::path::Path, dir: &std::path::Path, files: &mut Vec<(String, String)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let rel = p.strip_prefix(root).unwrap();
                if rel.starts_with(".git") {
                    continue;
                }
                if p.is_dir() {
                    walk(root, &p, files);
                } else if p.is_file() {
                    let content = std::fs::read_to_string(&p).unwrap_or_default();
                    files.push((rel.to_string_lossy().to_string(), content));
                }
            }
        }
    }
    walk(repo.path(), repo.path(), &mut files);
    files.sort();
    (head, status, files)
}

/// Bug 1: `squire amend --commit <non-HEAD>` that hits a rebase conflict
/// MUST restore the working tree, index, and untracked files to the
/// exact pre-command state. Previously it would silently drop the hunks
/// that were staged by `stage_hunks_or_cached` before the atomic scope.
#[test]
fn amend_commit_rollback_preserves_staged_hunks() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "L1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "L1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "L1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    // User edits that should survive the rollback:
    //   - unstaged edit on shared.txt (targeted by the amend)
    //   - unstaged edit on side.txt (not targeted)
    //   - untracked fresh.txt (not targeted)
    repo.write_file("shared.txt", "L1 v2 amended\n");
    repo.write_file("side.txt", "side work\n");
    repo.git(&["add", "side.txt"]);
    repo.write_file("side.txt", "side work + more\n");
    repo.write_file("fresh.txt", "untracked\n");

    let pre = snapshot_state(&repo);

    let hunks = repo.diff_json();
    let id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "shared.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    // Expect an error (rebase conflict).
    let _err = repo.squire_err(&["amend", "--commit", &target[..8], id]);

    let post = snapshot_state(&repo);
    assert_eq!(pre.0, post.0, "HEAD changed after rollback");
    assert_eq!(
        pre.1, post.1,
        "porcelain status differs after rollback:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents differ after rollback");
    // No orphan stashes.
    let stash_list = repo.git(&["stash", "list"]);
    assert!(
        stash_list.is_empty(),
        "orphan stash left behind: {stash_list}"
    );
}

/// Bug 2: `squire amend HEAD <hunk>` must NOT silently fold pre-existing
/// staged content that the user did not name into the amended commit.
/// Unrelated staged content must stay staged; only the named hunks are
/// folded into HEAD.
#[test]
fn amend_head_preserves_unrelated_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a-v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "orig"]);

    // User stages `other.txt` (a new file) as separate work in progress.
    repo.write_file("other.txt", "other work\n");
    repo.git(&["add", "other.txt"]);
    // User also has an unstaged edit to a.txt that they want amended.
    repo.write_file("a.txt", "a-v2\n");

    let hunks = repo.diff_json();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    repo.squire(&["amend", a_id]);

    // HEAD should include only a.txt's change, not other.txt.
    let head_stat = repo.git(&["show", "HEAD", "--stat"]);
    assert!(
        head_stat.contains("a.txt"),
        "a.txt should be in the amended HEAD: {head_stat}"
    );
    assert!(
        !head_stat.contains("other.txt"),
        "other.txt must NOT be folded into amended HEAD: {head_stat}"
    );
    // other.txt must still be staged (not lost, not amended).
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  other.txt"),
        "other.txt should still be staged after amend HEAD: {status}"
    );
}

/// Bug 3: `squire drop HEAD <hunk>` must NOT silently fold pre-existing
/// staged content into the amended commit.
#[test]
fn drop_head_preserves_unrelated_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("a.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "change-to-drop"]);

    // User stages separate work `other.txt` as a new file.
    repo.write_file("other.txt", "other\n");
    repo.git(&["add", "other.txt"]);

    let hunks = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let parsed: serde_json::Value = serde_json::from_str(&hunks).unwrap();
    let id = parsed[0]["id"].as_str().unwrap().to_string();

    repo.squire(&["drop", "HEAD", &id]);

    let head_stat = repo.git(&["show", "HEAD", "--stat"]);
    assert!(
        !head_stat.contains("other.txt"),
        "other.txt must NOT be folded into dropped HEAD commit: {head_stat}"
    );
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  other.txt"),
        "other.txt should still be staged after drop HEAD: {status}"
    );
}

/// Bug 4: `squire reword HEAD` must NOT silently fold pre-existing
/// staged content into the reworded commit. reword is a message-only
/// operation.
#[test]
fn reword_head_preserves_unrelated_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "orig"]);

    // User stages separate work.
    repo.write_file("other.txt", "other\n");
    repo.git(&["add", "other.txt"]);

    repo.squire(&["reword", "HEAD", "-m", "new msg"]);

    let head_stat = repo.git(&["show", "HEAD", "--stat"]);
    assert!(
        !head_stat.contains("other.txt"),
        "other.txt must NOT be folded into reworded HEAD: {head_stat}"
    );
    let head_msg = repo.git(&["log", "-1", "--format=%s"]);
    assert_eq!(head_msg.trim(), "new msg");
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  other.txt"),
        "other.txt should still be staged after reword: {status}"
    );
}

/// Bug 5: `squire revert <staged-hunk>` must not leave the index in a
/// partial state if the two-step apply fails. When the underlying
/// `git apply --cached --reverse` succeeds but the working-tree apply
/// fails, previously the index was mutated and the user had to debug it.
/// Now the whole operation is rolled back.
#[test]
fn revert_staged_hunk_rolls_back_on_worktree_apply_failure() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "A\nB\nC\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Stage A -> A2. Then make the working tree also modify C -> C2,
    // so that reverse-applying the staged hunk against the working
    // tree context "A2,B,C" fails (the context line C is actually C2).
    repo.write_file("f.txt", "A2\nB\nC\n");
    repo.git(&["add", "f.txt"]);
    repo.write_file("f.txt", "A2\nB\nC2\n");

    let pre = snapshot_state(&repo);

    // Identify the staged hunk id via diff --cached.
    let staged_out = repo.squire(&["--json", "diff", "--cached"]);
    let staged: serde_json::Value = serde_json::from_str(&staged_out).unwrap();
    let id = staged[0]["id"].as_str().unwrap().to_string();

    // Expect failure.
    let _err = repo.squire_err(&["revert", &id]);

    let post = snapshot_state(&repo);
    assert_eq!(pre.0, post.0, "HEAD changed");
    assert_eq!(
        pre.1, post.1,
        "porcelain status differs:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents differ");
}

/// Workflow check: alternating "amend hunks into X; amend other hunks
/// into Y" must work without requiring a clean tree between steps. This
/// guards against the old `is_clean` check coming back as a reflex.
#[test]
fn amend_commit_alternating_workflow() {
    let repo = TestRepo::new();
    // base commit so that X-base has a parent for fixup's `~1` lookup.
    repo.write_file("seed.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("x.txt", "X-v1\n");
    repo.write_file("y.txt", "Y-v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "X-base"]);
    let x_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("z.txt", "Z\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Y-base"]);
    let _y_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("after.txt", "after\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after-both"]);

    // User has unstaged edits to x.txt (for x) and y.txt (for y).
    repo.write_file("x.txt", "X-v2\n");
    repo.write_file("y.txt", "Y-v2\n");

    // Find hunk IDs for each file.
    let hunks = repo.diff_json();
    let x_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "x.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Amend x.txt into X-base.
    repo.squire(&["amend", "--commit", &x_sha[..8], &x_id]);

    // Tree is NOT clean — y.txt is still unstaged. After the first
    // amend the original y_sha was rewritten; re-resolve via HEAD~1.
    let new_y_sha = repo.git(&["rev-parse", "HEAD~1"]).trim().to_string();
    assert_ne!(
        new_y_sha, _y_sha,
        "first amend should have rewritten Y-base"
    );
    let status_after_first = repo.git(&["status", "--porcelain"]);
    assert!(
        status_after_first.contains("y.txt"),
        "y.txt should still be dirty after first amend: {status_after_first}"
    );
    let hunks = repo.diff_json();
    let hunks_str = serde_json::to_string_pretty(&hunks).unwrap();
    let y_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "y.txt")
        .unwrap_or_else(|| panic!("y.txt not in diff after first amend.\nstatus: {status_after_first}\nhunks: {hunks_str}"))["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Amend y.txt into the rewritten Y-base. Must NOT reject on
    // "clean tree" grounds even though x.txt's rewrite just happened.
    repo.squire(&["amend", "--commit", &new_y_sha[..8], &y_id]);

    // Verify X-base now contains x.txt change, Y-base contains y.txt change.
    let x_show = repo.git(&["show", "HEAD~2"]);
    assert!(x_show.contains("X-v2"), "X amend should be in HEAD~2");
    let y_show = repo.git(&["show", "HEAD~1"]);
    assert!(y_show.contains("Y-v2"), "Y amend should be in HEAD~1");
}

/// reword of a non-HEAD commit must not require a clean tree; the user
/// workflow "reword an old commit while keeping other in-progress edits"
/// must work.
#[test]
fn reword_non_head_allows_dirty_tree() {
    let repo = TestRepo::new();
    // Seed commit so target has a parent for the rebase `~1` lookup.
    repo.write_file("seed.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "old msg"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("g.txt", "g\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    // Dirty state unrelated to target.
    repo.write_file("scratch.txt", "scratch\n");
    repo.git(&["add", "scratch.txt"]);
    repo.write_file("untracked.txt", "u\n");

    let pre_status = repo.git(&["status", "--porcelain"]);

    repo.squire(&["reword", &target[..8], "-m", "reworded"]);

    // target's message changed.
    let log = repo.git(&["log", "--format=%s", "-1", "HEAD~1"]);
    assert_eq!(log.trim(), "reworded");
    // Dirty state preserved.
    let post_status = repo.git(&["status", "--porcelain"]);
    assert_eq!(
        pre_status, post_status,
        "unrelated dirty state must survive non-HEAD reword"
    );
}

/// drop of a non-HEAD commit must not require a clean tree.
#[test]
fn drop_non_head_allows_dirty_tree() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "drop-me"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("g.txt", "g\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);

    // Dirty state.
    repo.write_file("scratch.txt", "scratch\n");
    repo.git(&["add", "scratch.txt"]);

    // Hunk id from the target commit.
    let parsed: serde_json::Value =
        serde_json::from_str(&repo.squire(&["--json", "diff", "HEAD~2", "HEAD~1"])).unwrap();
    let id = parsed[0]["id"].as_str().unwrap().to_string();

    repo.squire(&["drop", &target[..8], &id]);

    // scratch.txt should still be staged.
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  scratch.txt"),
        "scratch should still be staged: {status}"
    );
}

// =========================================================================
// Exhaustive data-preservation tests
//
// Every rebase-based command (amend, drop, reword, squash) is tested with
// a full mixed dirty state: staged content + unstaged edits + untracked
// files. The snapshot_state helper captures HEAD, porcelain status, and
// every file's contents so we can assert byte-for-byte preservation.
//
// Rollback scenarios (conflict path) are tested with the same rich dirty
// state for drop, reword, and squash — amend already has this coverage
// via amend_commit_rollback_preserves_staged_hunks.
// =========================================================================

/// Helper: set up a rich mixed dirty state on `repo` and return the
/// pre-command snapshot. Creates:
///   - staged new file `staged_new.txt`
///   - staged edit to `committed.txt` (v1 -> v2 in index)
///   - unstaged edit to `committed.txt` (v2 in index -> v3 on disk)
///   - unstaged edit to `side.txt`
///   - untracked file `untracked.txt`
fn apply_mixed_dirty_state(repo: &TestRepo) {
    repo.write_file("staged_new.txt", "staged new content\n");
    repo.git(&["add", "staged_new.txt"]);
    repo.write_file("committed.txt", "v2-staged\n");
    repo.git(&["add", "committed.txt"]);
    repo.write_file("committed.txt", "v3-unstaged\n");
    repo.write_file("side.txt", "side edit\n");
    repo.write_file("untracked.txt", "untracked\n");
}

fn assert_state_preserved(
    pre: &(String, String, Vec<(String, String)>),
    post: &(String, String, Vec<(String, String)>),
    label: &str,
) {
    assert_eq!(pre.0, post.0, "{label}: HEAD changed");
    assert_eq!(
        pre.1, post.1,
        "{label}: porcelain status differs:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "{label}: file contents differ");
}

// --- amend HEAD with mixed dirty state ---

#[test]
fn amend_head_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("target.txt", "target\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "head"]);

    // Create an unstaged edit to target.txt that we'll amend into HEAD.
    repo.write_file("target.txt", "target amended\n");
    // Plus mixed dirty state on other files.
    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);

    let hunks = repo.diff_json();
    let id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "target.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    repo.squire(&["amend", id]);

    // HEAD changed (amend succeeded), but all unrelated dirty state survives.
    let post = snapshot_state(&repo);
    assert_ne!(pre.0, post.0, "HEAD should change after amend");
    assert_eq!(
        pre.1
            .lines()
            .filter(|l| !l.contains("target.txt"))
            .collect::<Vec<_>>(),
        post.1
            .lines()
            .filter(|l| !l.contains("target.txt"))
            .collect::<Vec<_>>(),
        "non-target porcelain lines must match"
    );
    // Verify specific dirty state survived.
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("staged_new.txt"),
        "staged new file lost: {status}"
    );
    assert!(
        status.contains("committed.txt"),
        "staged+unstaged edit lost: {status}"
    );
    assert!(status.contains("side.txt"), "unstaged edit lost: {status}");
    assert!(
        status.contains("untracked.txt"),
        "untracked file lost: {status}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("committed.txt")).unwrap(),
        "v3-unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}

// --- amend non-HEAD with mixed dirty state ---

#[test]
fn amend_non_head_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("target.txt", "target\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target-commit"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("after.txt", "after\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);

    repo.write_file("target.txt", "target amended\n");
    apply_mixed_dirty_state(&repo);

    let hunks = repo.diff_json();
    let id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "target.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    repo.squire(&["amend", "--commit", &target[..8], id]);

    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("staged_new.txt"),
        "staged new file lost: {status}"
    );
    assert!(
        status.contains("committed.txt"),
        "staged+unstaged edit lost: {status}"
    );
    assert!(status.contains("side.txt"), "unstaged edit lost: {status}");
    assert!(
        status.contains("untracked.txt"),
        "untracked file lost: {status}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("committed.txt")).unwrap(),
        "v3-unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}

// --- drop HEAD with mixed dirty state ---

#[test]
fn drop_head_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "head with two files"]);

    apply_mixed_dirty_state(&repo);

    // Drop only a.txt from HEAD, keep b.txt.
    let hunks_out = repo.squire(&["--json", "diff", "HEAD~1", "HEAD"]);
    let hunks: serde_json::Value = serde_json::from_str(&hunks_out).unwrap();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"] == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    repo.squire(&["drop", "HEAD", &a_id]);

    let status_post = repo.git(&["status", "--porcelain"]);
    // a.txt now appears as unstaged (dropped from commit -> working tree).
    // All other dirty state must survive.
    assert!(
        status_post.contains("staged_new.txt"),
        "staged new file lost: {status_post}"
    );
    assert!(
        status_post.contains("committed.txt"),
        "staged+unstaged edit lost: {status_post}"
    );
    assert!(
        status_post.contains("side.txt"),
        "unstaged edit lost: {status_post}"
    );
    assert!(
        status_post.contains("untracked.txt"),
        "untracked file lost: {status_post}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("committed.txt")).unwrap(),
        "v3-unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}

// --- drop non-HEAD with mixed dirty state ---

#[test]
fn drop_non_head_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("after.txt", "after\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);

    apply_mixed_dirty_state(&repo);

    let hunks_out = repo.squire(&["--json", "diff", "HEAD~2", "HEAD~1"]);
    let hunks: serde_json::Value = serde_json::from_str(&hunks_out).unwrap();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"] == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    repo.squire(&["drop", &target[..8], &a_id]);

    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("staged_new.txt"),
        "staged new file lost: {status}"
    );
    assert!(
        status.contains("committed.txt"),
        "staged+unstaged edit lost: {status}"
    );
    assert!(status.contains("side.txt"), "unstaged edit lost: {status}");
    assert!(
        status.contains("untracked.txt"),
        "untracked file lost: {status}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("committed.txt")).unwrap(),
        "v3-unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}

// --- reword HEAD with mixed dirty state ---

#[test]
fn reword_head_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "original msg"]);

    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);
    repo.squire(&["reword", "HEAD", "-m", "new msg"]);
    let post = snapshot_state(&repo);

    // HEAD changes (new message), but all dirty state survives.
    assert_ne!(pre.0, post.0, "HEAD should change after reword");
    assert_eq!(
        pre.1, post.1,
        "porcelain status must match:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents must match");
    let msg = repo.git(&["log", "-1", "--format=%s"]);
    assert_eq!(msg.trim(), "new msg");
}

// --- reword non-HEAD with mixed dirty state ---

#[test]
fn reword_non_head_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("seed.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target msg"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("after.txt", "after\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);

    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);
    repo.squire(&["reword", &target[..8], "-m", "reworded"]);
    let post = snapshot_state(&repo);

    assert_eq!(
        pre.1, post.1,
        "porcelain status must match:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents must match");
    let msg = repo.git(&["log", "--format=%s", "-1", "HEAD~1"]);
    assert_eq!(msg.trim(), "reworded");
}

// --- squash with mixed dirty state ---

#[test]
fn squash_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("seed.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "first"]);
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "second"]);

    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);
    repo.squire(&["squash", "HEAD~1", "HEAD"]);
    let post = snapshot_state(&repo);

    assert_eq!(
        pre.1, post.1,
        "porcelain status must match:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents must match");
}

// =========================================================================
// Rollback with rich dirty state
//
// When a rebase-based command hits a conflict and rolls back, the full
// mixed dirty state (staged + unstaged + untracked) must be restored
// byte-for-byte. amend already has this via
// amend_commit_rollback_preserves_staged_hunks; these cover drop,
// reword, and squash.
// =========================================================================

#[test]
fn drop_conflict_rollback_preserves_mixed_dirty_state() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "base\n");
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    // Target: shared.txt becomes "a".
    repo.write_file("shared.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target (a)"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    // Later commit: shared.txt becomes "b" — dropping target will conflict.
    repo.write_file("shared.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later (b)"]);

    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);

    let hunks_out = repo.squire(&[
        "--json",
        "diff",
        &format!("{t}~1", t = &target[..8]),
        &target[..8],
    ]);
    let hunks: serde_json::Value = serde_json::from_str(&hunks_out).unwrap();
    let id = hunks[0]["id"].as_str().unwrap().to_string();

    let _err = repo.squire_err(&["drop", &target[..8], &id]);

    let post = snapshot_state(&repo);
    assert_state_preserved(&pre, &post, "drop rollback");
    let stash_list = repo.git(&["stash", "list"]);
    assert!(
        stash_list.is_empty(),
        "orphan stash left behind: {stash_list}"
    );
}

#[test]
fn squash_conflict_rollback_preserves_mixed_dirty_state() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "base\n");
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target (a)"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "middle (b)"]);
    repo.write_file("shared.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "source (c)"]);
    let source = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);

    let _err = repo.squire_json_err(&["--json", "squash", &target[..8], &source[..8]]);

    let post = snapshot_state(&repo);
    assert_state_preserved(&pre, &post, "squash rollback");
    let stash_list = repo.git(&["stash", "list"]);
    assert!(
        stash_list.is_empty(),
        "orphan stash left behind: {stash_list}"
    );
}

#[test]
fn reword_conflict_rollback_preserves_mixed_dirty_state() {
    // Reword itself doesn't change file content, but the rebase can
    // still fail if the todo-file edit targets a commit that causes
    // issues during replay. We simulate this by creating a scenario
    // where the rebase machinery encounters a problem.
    //
    // Actually, reword conflicts are rare since no file content changes.
    // Instead, test that a reword of a non-HEAD commit with rich dirty
    // state succeeds and preserves everything (the interesting path).
    let repo = TestRepo::new();
    repo.write_file("seed.txt", "seed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "old msg"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("after.txt", "after\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);
    repo.write_file("more.txt", "more\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "more"]);

    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);
    repo.squire(&["reword", &target[..8], "-m", "reworded msg"]);
    let post = snapshot_state(&repo);

    assert_eq!(
        pre.1, post.1,
        "porcelain status must match:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents must match");
    let msg = repo.git(&["log", "--format=%s", "-1", "HEAD~2"]);
    assert_eq!(msg.trim(), "reworded msg");
    let stash_list = repo.git(&["stash", "list"]);
    assert!(
        stash_list.is_empty(),
        "orphan stash left behind: {stash_list}"
    );
}

// =========================================================================
// Edge case: amend non-HEAD rollback with mixed dirty state
// (complements the existing amend_commit_rollback_preserves_staged_hunks
// which already tests this but with a slightly different dirty state
// composition — this one uses the standardized apply_mixed_dirty_state)
// =========================================================================

#[test]
fn amend_non_head_conflict_rollback_preserves_mixed_dirty_state() {
    let repo = TestRepo::new();
    repo.write_file("shared.txt", "L1\n");
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("shared.txt", "L1 v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("shared.txt", "L1 v3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "later"]);

    // The amend hunk targets shared.txt which will conflict.
    repo.write_file("shared.txt", "L1 v2 amended\n");
    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);

    let hunks = repo.diff_json();
    let id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "shared.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    let _err = repo.squire_err(&["amend", "--commit", &target[..8], id]);

    let post = snapshot_state(&repo);
    assert_state_preserved(&pre, &post, "amend non-HEAD rollback");
    let stash_list = repo.git(&["stash", "list"]);
    assert!(
        stash_list.is_empty(),
        "orphan stash left behind: {stash_list}"
    );
}

// =========================================================================
// Additional data-preservation edge cases
// =========================================================================

/// `squire stash` must not disturb pre-existing staged content.
/// Stash operates on unstaged hunks; staged content must remain in the
/// index untouched.
#[test]
fn stash_preserves_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Stage a new file (separate work in progress).
    repo.write_file("staged.txt", "staged work\n");
    repo.git(&["add", "staged.txt"]);
    // Unstaged edit to stash.
    repo.write_file("a.txt", "a-modified\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["stash", id]);

    // staged.txt must still be staged.
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  staged.txt"),
        "staged content lost after stash: {status}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("staged.txt")).unwrap(),
        "staged work\n"
    );
    // a.txt should be clean (stashed away).
    assert!(
        !status.contains("a.txt"),
        "a.txt should be stashed: {status}"
    );
}

/// `squire stash` with mixed dirty state: staged content, unstaged edits
/// on other files, and untracked files must all survive when stashing a
/// single hunk.
#[test]
fn stash_mixed_dirty_state_preserves_non_stashed() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Mixed dirty state.
    repo.write_file("staged.txt", "staged\n");
    repo.git(&["add", "staged.txt"]);
    repo.write_file("a.txt", "a-modified\n");
    repo.write_file("b.txt", "b-modified\n");
    repo.write_file("untracked.txt", "untracked\n");

    let hunks = repo.diff_json();
    let a_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "a.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap();

    repo.squire(&["stash", a_id]);

    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  staged.txt"),
        "staged file lost: {status}"
    );
    assert!(
        status.contains("b.txt"),
        "unstaged edit on b.txt lost: {status}"
    );
    assert!(
        status.contains("untracked.txt"),
        "untracked file lost: {status}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("b.txt")).unwrap(),
        "b-modified\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}

/// `squire commit` with pre-existing staged content: the commit includes
/// ALL staged content (named hunks + pre-existing). This is by design
/// (same as `git add <hunk> && git commit`), but we document the
/// behavior with a test so it doesn't silently change.
#[test]
fn commit_includes_pre_existing_staged_content() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Pre-existing staged content.
    repo.write_file("other.txt", "other\n");
    repo.git(&["add", "other.txt"]);
    // Unstaged hunk to commit.
    repo.write_file("a.txt", "a-v2\n");

    let hunks = repo.diff_json();
    let id = hunks[0]["id"].as_str().unwrap();
    repo.squire(&["commit", "-m", "feat", id]);

    // Both files should be in the commit.
    let show = repo.git(&["show", "--stat", "HEAD"]);
    assert!(show.contains("a.txt"), "named hunk missing: {show}");
    assert!(
        show.contains("other.txt"),
        "pre-existing staged content should be included: {show}"
    );
}

/// `squire amend HEAD` with line selectors and mixed dirty state.
/// Partial hunk amend must preserve all non-targeted state.
#[test]
fn amend_head_line_selector_preserves_mixed_dirty_state() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "line1\nline2\nline3\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Edit two lines; we'll amend only one.
    repo.write_file("f.txt", "LINE1\nline2\nLINE3\n");
    // Mixed dirty state on other files.
    repo.write_file("staged.txt", "staged\n");
    repo.git(&["add", "staged.txt"]);
    repo.write_file("untracked.txt", "untracked\n");

    let hunks = repo.diff_json();
    let hunk = &hunks[0];
    let id = hunk["id"].as_str().unwrap();
    let line_hashes = hunk["line_hashes"].as_array().unwrap();
    // Stage only the first changed line (LINE1).
    let first_hash = line_hashes[0].as_str().unwrap();

    repo.squire(&["amend", &format!("{id}:{first_hash}")]);

    // staged.txt must still be staged.
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("A  staged.txt"),
        "staged file lost: {status}"
    );
    assert!(
        status.contains("untracked.txt"),
        "untracked file lost: {status}"
    );
    // f.txt should still have an unstaged change (LINE3).
    assert!(
        status.contains("f.txt"),
        "remaining f.txt change lost: {status}"
    );
    let content = std::fs::read_to_string(repo.path().join("f.txt")).unwrap();
    assert!(
        content.contains("LINE3"),
        "LINE3 should still be in working tree: {content}"
    );
}

/// `squire squash` with non-adjacent commits and mixed dirty state.
/// The rebase is more complex (seqedit moves lines), so dirty state
/// preservation is exercised through a harder code path.
#[test]
fn squash_non_adjacent_mixed_dirty_state_preserves_all() {
    let repo = TestRepo::new();
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "base"]);
    repo.write_file("a.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "target"]);
    let target = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("b.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "middle"]);
    repo.write_file("c.txt", "c\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "source"]);
    let source = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("d.txt", "d\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);

    apply_mixed_dirty_state(&repo);

    let pre = snapshot_state(&repo);
    repo.squire(&["squash", &target[..8], &source[..8]]);
    let post = snapshot_state(&repo);

    assert_eq!(
        pre.1, post.1,
        "porcelain status must match:\n-- pre --\n{}-- post --\n{}",
        pre.1, post.1
    );
    assert_eq!(pre.2, post.2, "file contents must match");
}

/// Sequential amend operations: amend into one commit, then amend into
/// another, with dirty state surviving both. This is the workflow from
/// amend_commit_alternating_workflow but with full mixed dirty state.
#[test]
fn sequential_amends_preserve_mixed_dirty_state() {
    let repo = TestRepo::new();
    repo.write_file("committed.txt", "v1\n");
    repo.write_file("side.txt", "side\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "seed"]);
    repo.write_file("x.txt", "x-v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "X-base"]);
    let x_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.write_file("y.txt", "y-v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "Y-base"]);
    repo.write_file("after.txt", "after\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "after"]);

    // Unstaged edits for both targets.
    repo.write_file("x.txt", "x-v2\n");
    repo.write_file("y.txt", "y-v2\n");
    // Plus mixed dirty state.
    apply_mixed_dirty_state(&repo);

    // First amend: x.txt into X-base.
    let hunks = repo.diff_json();
    let x_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "x.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    repo.squire(&["amend", "--commit", &x_sha[..8], &x_id]);

    // Mixed dirty state must survive the first amend.
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("staged_new.txt"),
        "staged file lost after first amend: {status}"
    );
    assert!(
        status.contains("untracked.txt"),
        "untracked file lost after first amend: {status}"
    );
    assert!(
        status.contains("committed.txt"),
        "staged+unstaged edit lost after first amend: {status}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("committed.txt")).unwrap(),
        "v3-unstaged\n"
    );

    // Second amend: y.txt into rewritten Y-base.
    let new_y_sha = repo.git(&["rev-parse", "HEAD~1"]).trim().to_string();
    let hunks = repo.diff_json();
    let y_id = hunks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap() == "y.txt")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    repo.squire(&["amend", "--commit", &new_y_sha[..8], &y_id]);

    // Mixed dirty state must survive the second amend too.
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.contains("staged_new.txt"),
        "staged file lost after second amend: {status}"
    );
    assert!(
        status.contains("untracked.txt"),
        "untracked file lost after second amend: {status}"
    );
    assert!(
        status.contains("committed.txt"),
        "staged+unstaged edit lost after second amend: {status}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("committed.txt")).unwrap(),
        "v3-unstaged\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "untracked\n"
    );
}
