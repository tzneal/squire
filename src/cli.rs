use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "squire",
    about = "Non-interactive, hunk-addressable git staging CLI",
    long_about = "squire lets you view, address, and stage individual diff hunks by ID.\n\n\
        Every hunk gets a short, stable, content-based ID (first 8 hex chars of a \
        SHA-256 of the hunk content). Use these IDs to selectively stage, unstage, \
        and show hunks. Every operation is a single non-interactive command with \
        structured I/O.\n\n\
        Default output is human-readable plain text. Pass --json for structured output.\n\n\
        Examples:\n  \
        squire diff                    # list hunks with IDs\n  \
        squire diff --json             # same, as JSON\n  \
        squire diff -- src/main.rs     # only hunks in src/main.rs\n  \
        squire show abc12345           # show one hunk in detail\n  \
        squire stage abc12345 def67890 # stage two hunks",
    after_long_help = "NOTE: If you are an LLM or agent, run `squire --llm-help` for a complete reference."
)]
pub struct Cli {
    /// Output as JSON instead of plain text
    #[arg(long, global = true)]
    pub json: bool,

    /// Output a compact one-line-per-hunk summary (id, file, range, +/-counts)
    #[arg(long, global = true)]
    pub short: bool,

    /// Print a single comprehensive reference for LLM consumption, then exit
    #[arg(long)]
    pub llm_help: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

pub const LLM_HELP: &str = "\
squire — non-interactive, hunk-addressable git staging CLI

WHEN TO USE
  Use squire instead of `git add -p` when you need to selectively stage
  hunks without an interactive TUI. squire is designed for LLMs and scripts:
  every operation is a single non-interactive command with structured I/O.

  Use squire when you want to:
  - Stage only some hunks from a file (partial staging)
  - Stage and commit/amend in one step
  - Inspect individual hunks by stable ID
  - Script or automate selective staging
  - Analyze local branches for cleanup (merged, squash-merged, stale)

  You do NOT need squire to stage entire files — use `git add <file>` for that.

HOW IT WORKS
  squire wraps git primitives (git diff, git show, git apply --cached).
  It parses unified diff output and assigns each hunk a stable 8-char
  hex ID (SHA-256 of content). Each content line within a hunk also
  gets a short hex hash (shortest unique prefix, min 2 chars). These
  line hashes are stable: the same line content always produces the
  same hash. Use --json on any command for structured output suitable
  for programmatic consumption. Use --short for a compact one-line-
  per-hunk summary (id, file, range, +/-counts) without content.

HUNK ID PREFIX MATCHING
  Hunk IDs are 8-char hex strings, but you can use any unambiguous
  prefix. For example, if hunk \"abc12345\" is the only hunk starting
  with \"abc1\", then `squire stage abc1` works. If the prefix matches
  multiple hunks, squire returns an error listing the ambiguity.

COMMANDS
  squire diff [<git-diff-args>...]
    List hunks with IDs. Args pass through to `git diff`.
    Untracked files are always included as new-file diffs.
      squire diff                        # unstaged changes + untracked
      squire diff --cached               # staged changes
      squire diff --short                # compact summary: id, file, range, +/-
      squire diff HEAD~1                 # vs a ref
      squire diff HEAD~1 HEAD~2           # ref vs ref
      squire diff -- src/lib.rs          # filter by path

  squire show [<git-show-args>...] <hunk-id>
    Show a single hunk by ID. When a ref is given, searches `git show`
    output first. If the hunk is not found (or no ref is given), falls
    back to staged and unstaged working tree diffs.
      squire show abc12345               # from working tree or staged
      squire show HEAD abc12345          # from commit (falls back to diff)
      squire show HEAD~2 abc12345

  squire stage <hunk-id>[:<line-selector>]...
    Stage hunks by ID (from unstaged diff). Optionally select specific
    lines within a hunk using line hashes from the diff output.
      squire stage abc12345              # stage entire hunk
      squire stage abc1                  # prefix match (if unambiguous)
      squire stage abc12345:f3,a1       # stage only lines f3 and a1
      squire stage abc12345:f3-7b       # stage line range f3 through 7b
      squire stage abc12345 def67890    # stage multiple hunks

  squire unstage <hunk-id>[:<line-selector>]...
    Unstage hunks by ID (reverses staged diff). Same line selector
    syntax as stage.
      squire unstage abc12345
      squire unstage abc12345:f3,a1

  squire commit -m <message> <hunk-id>[:<line-selector>]...
    Stage hunks and commit in one step. Equivalent to
    `squire stage <ids> && git commit -m <message>`.
      squire commit -m \"feat: parser\" abc12345
      squire commit -m \"fix: typo\" abc12345:f3,a1 def67890

  squire amend [-m <message>] <hunk-id>[:<line-selector>]...
    Stage hunks and amend the current commit. If -m is given,
    replaces the commit message; otherwise keeps it.
      squire amend abc12345              # amend, keep message
      squire amend -m \"new msg\" abc12345 # amend with new message

  squire split <commit>
    Prepare to split a commit. Requires a clean working tree.
    Resets the target commit so its changes are unstaged, ready
    for selective re-staging with `squire stage` or `squire commit`.
    For HEAD: mixed reset. For older commits: non-interactive
    rebase that pauses at the commit, then resets it.
      squire split abc1234             # split any commit

  squire log [-n <count>]
    Show recent commits with hunk IDs. Hunk IDs match what
    `squire diff <sha>~1 <sha>` would produce, so you can go
    straight from `squire log` to `squire split` + `squire stage`.
    Default: last 10 commits.
      squire log                       # last 10 commits
      squire log -n 5                  # last 5
      squire log --json                # structured output with hunks
      squire log --short               # one line per commit

  squire status
    Show staged and unstaged hunks in one view, including untracked
    files. Gives a complete picture of what will be in the next
    commit and what's left. Also shows the current branch name,
    whether a rebase is in progress, and total added/removed line
    counts for staged and unstaged changes.
      squire status                    # plain text
      squire status --json             # structured output
      squire status --short            # compact summary

  squire cleanup [--master <branch>]
    Analyze local branches to identify which can be cleaned up.
    Auto-detects the master branch (main/master) or accepts --master.
    Reports each branch as one of:
      MERGED          — fully merged via git ancestry
      MERGED_EQUIVALENT — commit messages and patches match master
                        (squash/cherry-pick merge)
      NEEDS_EVALUATION — some commit messages match master but patches
                        differ; an LLM should review these commits
      UNMERGED        — no matching commits found in master
    Examples:
      squire cleanup                     # auto-detect master
      squire cleanup --master main       # specify master branch
      squire cleanup --json              # structured output for LLM

TYPICAL WORKFLOW
  1. squire diff --json                  # discover hunks, line hashes
  2. squire commit -m \"feat: ...\" <id>  # stage and commit in one step
  3. squire commit -m \"fix: ...\" <id>:<hash>,<hash>  # partial hunk commit

  Or stage and commit separately:
  1. squire diff --json                  # discover hunks
  2. squire stage <id>                   # stage a whole hunk
  3. git commit -m \"feat: ...\"       # commit staged hunks

  Untracked files:
  Untracked files always appear as new-file hunks in `squire diff`.
  Stage them the same way as modified files.
  1. squire diff --json                   # new files appear as hunks
  2. squire commit -m \"feat: ...\" <id>  # stage and commit a new file

SPLITTING A COMMIT
  To split an existing commit into multiple smaller commits:
  1. squire log --json -n 5              # review recent history + hunk IDs
  2. squire split <commit>               # unstage the commit's changes
  3. squire diff --json                   # see all unstaged hunks
  4. squire commit -m \"feat: ...\" <id1> <id2>  # commit first group
  5. squire diff --json                   # re-check what's left
  6. squire commit -m \"fix: ...\" <id3>  # commit second group
  Repeat steps 4-5 until all changes are committed.
  If the target was not HEAD, run `git rebase --continue` to replay
  the remaining commits.

BRANCH CLEANUP
  To identify and clean up stale local branches:
  1. squire cleanup --json               # analyze all local branches
  2. Delete MERGED and MERGED_EQUIVALENT branches:
     git branch -d <branch>
  3. Review NEEDS_EVALUATION branches — check the commits listed
     to decide if remaining changes are needed
  4. Leave UNMERGED branches alone (or delete with -D if abandoned)

IMPORTANT NOTES
  - Hunk IDs change when file content changes. Re-run `squire diff` after
    any edit or staging operation to get current IDs.
  - Hunk IDs support prefix matching: use any unambiguous prefix.
  - Line hashes are content-based: the same line always gets the same
    hash. Hashes are the shortest unique prefix (min 2 hex chars)
    within each hunk.
  - After staging, the hunk disappears from `squire diff` and appears
    in `squire diff --cached`.

JSON OUTPUT
  --json returns structured output depending on the command:

  log returns an array of commit objects:
    { \"sha\": \"abc1234f...\", \"author\": \"name\",
      \"date\": \"2026-03-15T14:30:00Z\", \"message\": \"feat: ...\",
      \"hunks\": [ { \"id\": \"d3f1a2b0\", ... } ] }

  diff/show return an array of hunk objects:
    { \"id\": \"abc12345\", \"file\": \"src/main.rs\",
      \"old_file\": \"src/main.rs\",
      \"old_range\": \"10,5\", \"new_range\": \"10,7\",
      \"header\": \"fn some_function()\",
      \"content\": \"...\", \"line_hashes\": [\"f3\", \"a1\", \"7b\", ...] }
  commit returns: { \"committed\": N, \"message\": \"...\" }
  amend returns: { \"amended\": N, \"message\": \"...\" }
  stage/unstage return: { \"staged\": N, \"message\": \"...\" }
  status returns:
    { \"branch\": \"main\", \"rebase_in_progress\": false,
      \"staged\": [...], \"unstaged\": [...],
      \"staged_lines\": { \"added\": 3, \"removed\": 1 },
      \"unstaged_lines\": { \"added\": 5, \"removed\": 2 } }
  cleanup returns:
    { \"master_branch\": \"main\", \"current_branch\": \"feature-x\",
      \"branches\": [
        { \"name\": \"old-branch\", \"status\": \"merged\", \"commits\": [] },
        { \"name\": \"squashed\", \"status\": \"merged_equivalent\",
          \"commits\": [{ \"sha\": \"abc12345\", \"message\": \"feat: ...\",
            \"message_in_master\": true, \"patch_applied\": true }],
          \"note\": \"All commits have matching messages and patches...\" },
        { \"name\": \"maybe\", \"status\": \"needs_evaluation\",
          \"commits\": [{ \"sha\": \"def67890\", \"message\": \"fix: ...\",
            \"message_in_master\": true, \"patch_applied\": false }],
          \"note\": \"...patches differ. An LLM should evaluate...\" },
        { \"name\": \"wip\", \"status\": \"unmerged\",
          \"commits\": [{ \"sha\": \"11223344\", \"message\": \"wip\" }] }
      ] }

JSON ERRORS
  When --json is set, errors are also returned as JSON on stdout:
    { \"error\": \"hunk deadbeef not found\" }
  The process still exits with a non-zero status code.
";

#[derive(Subcommand)]
pub enum Command {
    /// Show diff with hunk IDs (passes arguments through to `git diff`)
    ///
    /// Untracked files are always included as new-file diffs.
    ///
    /// Examples:
    ///   squire diff                        # unstaged working tree changes
    ///   squire diff --cached               # staged changes
    ///   squire diff HEAD~1                 # working tree vs ref
    ///   squire diff HEAD~1 HEAD~2          # ref vs ref
    ///   squire diff -- src/lib.rs          # filter by path
    ///   squire diff --json                 # output as JSON array
    #[command(verbatim_doc_comment)]
    Diff {
        /// Arguments passed through to `git diff`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Show a single hunk by ID
    ///
    /// When a ref is given, searches `git show` output first. If the
    /// hunk is not found there (or no ref is given), falls back to
    /// staged and unstaged working tree diffs.
    ///
    /// Examples:
    ///   squire show abc12345               # hunk from working tree or staged
    ///   squire show HEAD abc12345          # hunk from last commit (or fallback)
    ///   squire show HEAD~2 abc12345        # hunk from two commits ago
    ///   squire show --json HEAD abc12345   # output as JSON
    #[command(verbatim_doc_comment)]
    Show {
        /// Optional git-show args followed by the hunk ID (last arg)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        args: Vec<String>,
    },

    /// Stage specific hunks by ID into the git index
    ///
    /// Finds the specified hunks from the unstaged working tree diff,
    /// reconstructs a patch containing only those hunks, and applies
    /// it to the index via `git apply --cached`.
    ///
    /// Append `:line-hashes` to a hunk ID to stage specific lines.
    /// Use comma-separated hashes or a range (start-end).
    ///
    /// Examples:
    ///   squire stage abc12345              # stage one hunk
    ///   squire stage abc12345 def67890     # stage multiple hunks
    ///   squire stage abc12345:f3,a1        # stage specific lines
    ///   squire stage abc12345:f3-7b        # stage a range of lines
    ///   squire stage --json abc12345       # report result as JSON
    #[command(verbatim_doc_comment)]
    Stage {
        /// One or more hunk IDs to stage
        #[arg(required = true)]
        hunk_ids: Vec<String>,
    },

    /// Unstage specific hunks by ID from the git index
    ///
    /// Finds the specified hunks from the staged (cached) diff,
    /// reconstructs a patch, and reverse-applies it via
    /// `git apply --cached --reverse`. Supports line selectors.
    ///
    /// Examples:
    ///   squire unstage abc12345            # unstage one hunk
    ///   squire unstage abc12345:f3,a1      # unstage specific lines
    ///   squire unstage abc12345 def67890   # unstage multiple hunks
    #[command(verbatim_doc_comment)]
    Unstage {
        /// One or more hunk IDs to unstage
        #[arg(required = true)]
        hunk_ids: Vec<String>,
    },

    /// Show staged and unstaged hunks in one view
    ///
    /// Reports both staged and unstaged hunks (including untracked
    /// files) so you can see what will be in the next commit and
    /// what's left, without separate diff calls.
    ///
    /// Examples:
    ///   squire status                      # plain text summary
    ///   squire status --json               # structured output
    #[command(verbatim_doc_comment)]
    Status,

    /// Stage hunks and commit in one step
    ///
    /// Stages the specified hunks, then commits with the given message.
    /// Supports the same line-selector syntax as `squire stage`.
    ///
    /// Examples:
    ///   squire commit -m "feat: add parser" abc12345
    ///   squire commit -m "fix: typo" abc12345:f3,a1 def67890
    #[command(verbatim_doc_comment)]
    Commit {
        /// Commit message
        #[arg(short, long, required = true)]
        message: String,
        /// One or more hunk IDs to stage and commit
        #[arg(required = true)]
        hunk_ids: Vec<String>,
    },

    /// Stage hunks and amend the current commit
    ///
    /// Stages the specified hunks, then amends HEAD.
    /// If -m is given, replaces the commit message; otherwise keeps it.
    ///
    /// Examples:
    ///   squire amend abc12345              # amend, keep message
    ///   squire amend -m "new msg" abc12345 # amend with new message
    #[command(verbatim_doc_comment)]
    Amend {
        /// Optional replacement commit message
        #[arg(short, long)]
        message: Option<String>,
        /// One or more hunk IDs to stage and amend into HEAD
        #[arg(required = true)]
        hunk_ids: Vec<String>,
    },

    /// Prepare to split a commit into multiple commits
    ///
    /// Resets the target commit so its changes are unstaged, ready
    /// for selective re-staging with `squire stage`. Requires a clean
    /// working tree.
    ///
    /// For HEAD: performs a mixed reset.
    /// For older commits: runs a non-interactive rebase to pause at
    /// the commit, then resets it.
    ///
    /// After splitting, use `git rebase --continue` to replay
    /// remaining commits (if the target was not HEAD).
    ///
    /// Examples:
    ///   squire split abc1234               # split a commit
    #[command(verbatim_doc_comment)]
    Split {
        /// The commit to split
        #[arg(required = true)]
        commit: String,
    },

    /// Analyze local branches for cleanup
    ///
    /// Identifies branches that are fully merged, likely merged (matching
    /// commit messages found in master), or unmerged. Designed to assist
    /// an LLM in deciding which branches can be safely deleted.
    ///
    /// Examples:
    ///   squire cleanup                     # auto-detect master branch
    ///   squire cleanup --master main       # specify master branch
    ///   squire cleanup --json              # structured output
    #[command(verbatim_doc_comment)]
    Cleanup {
        /// Name of the master/main branch (auto-detected if omitted)
        #[arg(long)]
        master: Option<String>,
    },

    /// Show recent commit history with hunk IDs
    ///
    /// Lists recent commits with their hunks, so you can plan
    /// splits, squashes, and reorders from a single command.
    /// Hunk IDs match what `squire diff <sha>~1 <sha>` would return.
    ///
    /// Examples:
    ///   squire log                          # last 10 commits
    ///   squire log -n 5                     # last 5 commits
    ///   squire log --json                   # structured output
    ///   squire log --short                  # one line per commit
    #[command(verbatim_doc_comment)]
    Log {
        /// Number of commits to show
        #[arg(short, long, default_value = "10")]
        n: usize,
    },
}
