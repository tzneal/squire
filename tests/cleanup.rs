mod helpers;
use helpers::TestRepo;

#[test]
fn cleanup_merged_branch_detected() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "v1\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create and merge a branch
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("f.txt", "v2\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "feature work"]);
    repo.git(&["checkout", "main"]);
    repo.git(&["merge", "feature"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let result: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = result["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    assert_eq!(feature["status"].as_str().unwrap(), "merged");
}

#[test]
fn cleanup_plain_text_output() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a merged branch
    repo.git(&["checkout", "-b", "merged-branch"]);
    repo.write_file("f.txt", "changed\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "change"]);
    repo.git(&["checkout", "main"]);
    repo.git(&["merge", "merged-branch"]);

    // Create an unmerged branch
    repo.git(&["checkout", "-b", "unmerged-branch"]);
    repo.write_file("f.txt", "unmerged\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "unmerged work"]);
    repo.git(&["checkout", "main"]);

    let out = repo.squire(&["cleanup", "--master", "main"]);
    assert!(out.contains("Master branch: main"));
    assert!(out.contains("[MERGED]"));
    assert!(out.contains("merged-branch"));
    assert!(out.contains("[UNMERGED]"));
    assert!(out.contains("unmerged-branch"));
    assert!(out.contains("unmerged work"));
}

#[test]
fn cleanup_squash_merged_branch_detected() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a branch with a commit
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    repo.git(&["checkout", "main"]);

    // Advance main so the branch is not ancestry-merged, then
    // replicate the branch's change with the same message (squash merge).
    repo.write_file("f.txt", "updated\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    repo.write_file("g.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    assert_eq!(feature["status"], "merged_equivalent");
}

#[test]
fn cleanup_needs_evaluation_branch() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Create a branch with a commit
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "branch version\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    repo.git(&["checkout", "main"]);

    // Same message but different patch on main
    repo.write_file("g.txt", "main version\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    assert_eq!(feature["status"], "needs_evaluation");
    assert!(feature["note"].as_str().unwrap().contains("patches differ"));
}

#[test]
fn cleanup_partial_message_match() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch with two commits, only one message matches main
    repo.git(&["checkout", "-b", "partial"]);
    repo.write_file("g.txt", "a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "shared msg"]);
    repo.write_file("h.txt", "b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "unique to branch"]);
    repo.git(&["checkout", "main"]);

    // Only one matching message on main
    repo.write_file("x.txt", "x\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "shared msg"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let partial = branches.iter().find(|b| b["name"] == "partial").unwrap();
    assert_eq!(partial["status"], "needs_evaluation");
}

#[test]
fn cleanup_best_match_shows_similarity() {
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: one commit with no exact message match in master
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add the feature"]);
    repo.git(&["checkout", "main"]);

    // Master: similar message, different patch
    repo.write_file("h.txt", "other\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add the feature flag"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    let commit = &feature["commits"][0];
    let best = &commit["best_match"];
    assert!(
        !best.is_null(),
        "expected best_match for unmerged commit with similar message"
    );
    assert!(best["message_similarity"].as_f64().unwrap() > 0.5);
    assert!(best["diff_similarity"].is_number());
    assert!(!best["sha"].as_str().unwrap().is_empty());
}

#[test]
fn cleanup_no_best_match_for_applied_commits() {
    // Commits that are patch-applied should not have best_match
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    repo.git(&["checkout", "main"]);

    // Squash-merge: same patch, same message
    repo.write_file("f.txt", "advanced\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    repo.write_file("g.txt", "feature\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    assert_eq!(feature["status"], "merged_equivalent");
    // Applied commits should not have best_match (it's omitted)
    assert!(feature["commits"][0].get("best_match").is_none());
}

#[test]
fn cleanup_duplicate_subject_not_false_merged_equivalent() {
    // Branch has 2 commits both titled "fix typo" touching different files.
    // Master has only 1 commit titled "fix typo" matching one of them.
    // The second branch commit is genuinely unmerged — should NOT be
    // merged_equivalent.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: two "fix typo" commits touching different files
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.write_file("b.txt", "typo fix b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.git(&["checkout", "main"]);

    // Master: advance, then cherry-pick only the first "fix typo"
    repo.write_file("f.txt", "advanced\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    // Replicate only the a.txt change with the same message
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    // The second "fix typo" (b.txt) is NOT in master — must not be merged_equivalent
    assert_ne!(
        feature["status"], "merged_equivalent",
        "branch has unmerged work but was classified as merged_equivalent"
    );
}

#[test]
fn cleanup_duplicate_subject_all_patches_applied_independently() {
    // Branch has 2 commits both titled "fix typo" touching different files.
    // Master independently has both patches applied (via separate commits
    // with different messages). git cherry says both are applied, and the
    // HashSet says both messages match (master also has a "fix typo").
    // This is actually correct — both patches ARE in master — but the
    // message match is coincidental. The commit-level detail should show
    // patch_applied=true for both.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: two "fix typo" commits
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.write_file("b.txt", "typo fix b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.git(&["checkout", "main"]);

    // Master: advance, then add both patches with different messages
    repo.write_file("f.txt", "advanced\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    repo.write_file("a.txt", "typo fix a\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo"]);
    repo.write_file("b.txt", "typo fix b\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "fix typo in b"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    // Both patches are genuinely in master, so merged_equivalent is correct
    assert_eq!(feature["status"], "merged_equivalent");
}

#[test]
fn cleanup_cherry_pick_with_reworded_message_detected() {
    // Branch commit is cherry-picked to master with a different message.
    // git cherry detects the patch is applied, but the current code misses
    // it because msg_match is false. Should be merged_equivalent.
    let repo = TestRepo::new();
    repo.write_file("f.txt", "init\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "init"]);

    // Branch: one commit
    repo.git(&["checkout", "-b", "feature"]);
    repo.write_file("g.txt", "feature content\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "add feature"]);
    let branch_sha = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.git(&["checkout", "main"]);

    // Master: advance, then cherry-pick with a different message
    repo.write_file("f.txt", "advanced\n");
    repo.git(&["add", "."]);
    repo.git(&["commit", "-m", "advance main"]);
    repo.git(&["cherry-pick", &branch_sha]);
    // Reword the cherry-picked commit
    repo.git(&["commit", "--amend", "-m", "feat: add feature (reworded)"]);

    let out = repo.squire(&["--json", "cleanup", "--master", "main"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let branches = parsed["branches"].as_array().unwrap();
    let feature = branches.iter().find(|b| b["name"] == "feature").unwrap();
    // git cherry knows the patch is applied — should be merged_equivalent
    assert_eq!(
        feature["status"], "merged_equivalent",
        "cherry-picked commit with reworded message should be detected as merged"
    );
}
