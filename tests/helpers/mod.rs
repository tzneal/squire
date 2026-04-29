use std::process::Command;

/// A temporary git repo for integration tests.
pub struct TestRepo {
    dir: tempfile::TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = Self { dir };
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.email", "test@test.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    /// Run squire in-process via the library. Returns (stdout, Result).
    pub fn run_squire(&self, args: &[&str]) -> (String, Result<squire::Output, String>) {
        use clap::Parser;

        let full_args: Vec<&str> = std::iter::once("squire")
            .chain(args.iter().copied())
            .collect();
        let cli = squire::cli::Cli::parse_from(&full_args);

        let result = match cli.command {
            Some(ref command) => squire::run(&cli, command, self.dir.path()),
            None => Err("no subcommand".to_string()),
        };

        // For JSON error cases, format the error the same way main.rs does.
        match result {
            Ok(out) => (out.stdout.clone(), Ok(out)),
            Err(ref e) if cli.json => {
                let json_err = if e.starts_with('{') {
                    format!("{e}\n")
                } else {
                    format!("{}\n", serde_json::json!({ "error": e }))
                };
                (json_err, result)
            }
            _ => (String::new(), result),
        }
    }

    /// Run squire expecting success, return stdout.
    pub fn squire(&self, args: &[&str]) -> String {
        let (stdout, result) = self.run_squire(args);
        match result {
            Ok(_) => stdout,
            Err(e) => panic!("squire {:?} failed: {}", args, e),
        }
    }

    /// Run squire expecting failure, return the error message.
    // #[rustllmlint::allow(dead_public)]
    pub fn squire_err(&self, args: &[&str]) -> String {
        let (_, result) = self.run_squire(args);
        match result {
            Err(e) => e,
            Ok(_) => panic!("squire {:?} should have failed", args),
        }
    }

    /// Run squire expecting failure with --json, return stdout (JSON error).
    // #[rustllmlint::allow(dead_public)]
    pub fn squire_json_err(&self, args: &[&str]) -> String {
        let (stdout, result) = self.run_squire(args);
        assert!(result.is_err(), "squire {:?} should have failed", args);
        stdout
    }

    // #[rustllmlint::allow(dead_public)]
    pub fn write_file(&self, name: &str, content: &str) {
        let path = self.dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// Run `squire --json diff`, parse the JSON, and return it.
    // #[rustllmlint::allow(dead_public)]
    pub fn diff_json(&self) -> serde_json::Value {
        let out = self.squire(&["--json", "diff"]);
        serde_json::from_str(&out).unwrap()
    }

    /// Create a repo with a single committed file, then overwrite it with new content.
    // #[rustllmlint::allow(dead_public)]
    pub fn with_committed_file(name: &str, old: &str, new: &str) -> Self {
        let repo = Self::new();
        repo.write_file(name, old);
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "init"]);
        repo.write_file(name, new);
        repo
    }

    /// Create a repo with two committed files, then overwrite both with new content.
    // #[rustllmlint::allow(dead_public)]
    pub fn with_two_committed_files(
        a_name: &str,
        a_old: &str,
        a_new: &str,
        b_name: &str,
        b_old: &str,
        b_new: &str,
    ) -> Self {
        let repo = Self::new();
        repo.write_file(a_name, a_old);
        repo.write_file(b_name, b_old);
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "init"]);
        repo.write_file(a_name, a_new);
        repo.write_file(b_name, b_new);
        repo
    }

    /// Leave the repo in a mid-rebase state with a conflict on `f.txt`.
    /// Useful for tests of `squire status` / `squire diff` / `squire rebase`
    /// that want to exercise the rebase-in-progress code paths without
    /// depending on `squire amend`'s conflict behavior.
    ///
    /// The conflict is created by rebasing a branch with conflicting edits
    /// on top of another branch. After this returns, `.git/rebase-merge/`
    /// exists and `f.txt` has conflict markers. Callers should run
    /// `git rebase --abort` to clean up (or rely on TestRepo's tempdir drop).
    // #[rustllmlint::allow(dead_public)]
    pub fn with_rebase_conflict() -> Self {
        let repo = Self::new();
        repo.write_file("f.txt", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "base"]);

        // Branch A: change f.txt to "A".
        repo.git(&["checkout", "-b", "branch-a"]);
        repo.write_file("f.txt", "A\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "on branch A"]);

        // Branch B from base: change f.txt to "B".
        repo.git(&["checkout", "-b", "branch-b", "main"]);
        repo.write_file("f.txt", "B\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-m", "on branch B"]);

        // Rebase B onto A — guaranteed conflict on f.txt. We do not
        // assert success because git exits non-zero on conflicts.
        let _ = std::process::Command::new("git")
            .args(["rebase", "branch-a"])
            .current_dir(repo.dir.path())
            .output();

        repo
    }
}
