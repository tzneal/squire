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
#[command(version)]
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

  squire revert <hunk-id>[:<line-selector>]...
    Revert hunks by ID from the working tree (discards changes).
    Works on both unstaged and staged hunks. Staged hunks are
    unstaged and reverse-applied in one step.
    Same line selector syntax as stage. For line selectors, select
    both the - and + lines of a change pair to revert it.
      squire revert abc12345
      squire revert abc12345:f3,a1
      squire revert abc12345 def67890

  squire commit -m <message> <hunk-id>[:<line-selector>]...
    Stage hunks and commit in one step. Equivalent to
    `squire stage <ids> && git commit -m <message>`.
      squire commit -m \"feat: parser\" abc12345
      squire commit -m \"fix: typo\" abc12345:f3,a1 def67890

  squire amend [--commit <ref>] [-m <message>] <hunk-id>[:<line-selector>]...
    Stage hunks and amend into a commit. Defaults to HEAD.
    Use --commit to target an older commit (creates a fixup commit
    and autosquash rebases). -m replaces the message (HEAD only).
      squire amend abc12345              # amend HEAD, keep message
      squire amend -m \"new msg\" abc12345 # amend HEAD with new message
      squire amend --commit HEAD~2 abc12345  # amend older commit
      squire amend --commit HEAD~2 abc12345   # amend older commit

  squire reword <commit> -m <message>
    Change a commit message without staging hunks.
    For HEAD: delegates to `git commit --amend -m`.
    For older commits: uses seqedit reword + custom GIT_EDITOR.
      squire reword HEAD -m \"new message\"
      squire reword HEAD~2 -m \"fix: corrected typo\"

  squire drop <commit> <hunk-id>...
    Remove specific hunks from an existing commit (inverse of amend).
    Find hunk IDs with `squire diff <commit>~1 <commit>` or `squire log --json`.
    For HEAD: reverse-applies and amends.
    For older commits: rebase + reverse-apply + amend + continue.
      squire drop HEAD abc12345
      squire drop HEAD~2 abc12345 def67890

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

  squire seqedit <action:sha-prefix>... <todo-file>
    Edit a git rebase todo file. Designed to be used as
    GIT_SEQUENCE_EDITOR, replacing sed/awk one-liners for
    non-interactive rebase operations.

    Actions use the syntax action:sha-prefix where action is one of:
    pick, reword, edit, squash, fixup, drop. SHA prefixes match against the
    abbreviated commit hashes in the todo file (matching works in
    both directions for full and abbreviated SHAs).

      GIT_SEQUENCE_EDITOR=\"squire seqedit edit:abc1234\" git rebase -i HEAD~3
      GIT_SEQUENCE_EDITOR=\"squire seqedit fixup:abc1 drop:def5\" git rebase -i HEAD~5

  squire squash [-m <message>] <target> <source>...
    Fold source commits into the target commit. The target's message
    is kept; use -m to replace it. Requires a clean working tree.
    Uses seqedit + non-interactive rebase under the hood.
      squire squash HEAD~2 HEAD~1 HEAD       # fold last 2 into HEAD~2
      squire squash abc1234 def5678          # fold def5678 into abc1234
      squire squash -m \"combined\" abc1 def5  # squash with new message

  squire stash [-m <message>] <hunk-id>[:<line-selector>]...
    Stash specific hunks into git stash. Removes the selected hunks
    from the working tree and saves them as a regular git stash entry.
    Other unstaged changes are preserved. Use `git stash pop` to restore.
      squire stash abc12345              # stash one hunk
      squire stash -m \"wip\" abc12345    # stash with a message
      squire stash abc12345:f3,a1       # stash specific lines
      squire stash abc12345 def67890    # stash multiple hunks

  squire rebase [--onto <ref>]
    Print a contextualized rebase playbook. Inspects the repo state
    and emits step-by-step instructions adapted to where you are:
    - Not rebasing: creates a safety tag, shows the rebase command
    - Mid-rebase with conflicts: lists conflicts and resolution rules
    - Mid-rebase without conflicts: shows the continue command
    - Up to date: reports no rebase needed
    The safety tag is created automatically on the first run.
    Use --onto to override the upstream ref (e.g. rebase onto a
    different branch than the configured upstream).
      squire rebase                      # plain text playbook
      squire rebase --onto origin/main   # override upstream
      squire rebase --json               # structured output

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
  If the target was not HEAD, run `GIT_EDITOR=true git rebase --continue`
  to replay the remaining commits non-interactively.

BRANCH CLEANUP
  To identify and clean up stale local branches:
  1. squire cleanup --json               # analyze all local branches
  2. Delete MERGED and MERGED_EQUIVALENT branches:
     git branch -d <branch>
  3. Review NEEDS_EVALUATION branches — check the commits listed
     to decide if remaining changes are needed
  4. Leave UNMERGED branches alone (or delete with -D if abandoned)

REBASING
  To rebase the current branch onto its upstream (or a specific branch):
  1. squire rebase --json                # creates safety tag, shows steps
     squire rebase --onto main --json    # rebase onto a specific branch
  2. Run the commands from the steps array directly — do not ask the user
  3. squire rebase --json                # check state after rebase
  4. If conflicts: resolve per the conflict_rules in the output, `git add`,
     run tests, then `GIT_EDITOR=true git rebase --continue`
  5. Repeat steps 3-4 until rebase completes
  Recovery: `git reset --hard <safety-tag>` (tag name in step 1 output)

IMPORTANT NOTES
  - Hunk IDs change when file content changes. Re-run `squire diff` after
    any edit or staging operation to get current IDs. (For partial line
    operations, the new_hunks field in the response provides updated IDs
    without needing to re-run diff.)
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
      \"refs\": [\"HEAD -> main\", \"origin/main\"],
      \"hunks\": [ { \"id\": \"d3f1a2b0\", ... } ] }
    The refs array contains branch/tag decorations (omitted when empty).
    Plain text shows refs in parentheses after the short SHA.

  diff/show return an array of hunk objects:
    { \"id\": \"abc12345\", \"file\": \"src/main.rs\",
      \"old_file\": \"src/main.rs\",
      \"old_range\": \"10,5\", \"new_range\": \"10,7\",
      \"header\": \"fn some_function()\",
      \"content\": \"...\", \"line_hashes\": [\"f3\", \"a1\", \"7b\", ...] }
  commit returns: { \"committed\": N, \"message\": \"...\" }
  amend returns: { \"amended\": N, \"message\": \"...\" }
  reword returns: { \"reworded\": true, \"message\": \"...\" }
  drop returns: { \"dropped\": N, \"message\": \"...\" }
  squash returns: { \"squashed\": N, \"message\": \"...\" }
  stash returns: { \"stashed\": N, \"message\": \"...\" }
  stage/unstage/revert/stash return: { \"staged\": N, \"message\": \"...\" }
    When line selectors are used, a new_hunks array is included with
    the residual hunks (id, file, old_range, new_range, header,
    line_hashes). Omitted when empty. This avoids needing to re-run
    squire diff after partial line operations.
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

  rebase returns one of three states:
    { \"state\": \"ready\", \"branch\": \"feat\", \"upstream\": \"origin/main\",
      \"commits_ahead\": 3, \"safety_tag\": \"pre-rebase/feat-1743562800\",
      \"steps\": [\"GIT_EDITOR=true git rebase --empty=drop origin/main\", ...] }
    { \"state\": \"rebasing\", \"branch\": \"feat\",
      \"conflicts\": [{\"file\": \"src/main.rs\", \"status\": \"both_modified\"}],
      \"conflict_rules\": {...}, \"steps\": [...] }
    { \"state\": \"up_to_date\", \"branch\": \"feat\", \"upstream\": \"origin/main\" }

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

    /// Revert specific hunks from the working tree
    ///
    /// Finds the specified hunks from the unstaged or staged diff
    /// and reverse-applies them, discarding those changes. Staged
    /// hunks are unstaged and reverse-applied in one step. Supports
    /// the same line-selector syntax as `squire stage`.
    ///
    /// Examples:
    ///   squire revert abc12345             # revert one hunk
    ///   squire revert abc12345:f3,a1       # revert specific lines
    ///   squire revert abc12345 def67890    # revert multiple hunks
    #[command(verbatim_doc_comment)]
    Revert {
        /// One or more hunk IDs to revert
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

    /// Stage hunks and amend into an existing commit
    ///
    /// Stages the specified hunks, then amends the target commit.
    /// Defaults to HEAD. Use --commit to target an older commit
    /// (creates a fixup commit and autosquash rebases).
    /// If -m is given, replaces the commit message; otherwise keeps it.
    ///
    /// Examples:
    ///   squire amend abc12345                      # amend HEAD
    ///   squire amend -m "new msg" abc12345          # amend HEAD, new message
    ///   squire amend --commit HEAD~2 abc12345       # amend older commit
    ///   squire amend --commit HEAD~2 abc12345       # amend older commit
    #[command(verbatim_doc_comment)]
    Amend {
        /// Optional replacement commit message
        #[arg(short, long)]
        message: Option<String>,
        /// Target commit to amend into (default: HEAD)
        #[arg(short, long)]
        commit: Option<String>,
        /// One or more hunk IDs to stage and amend into HEAD
        #[arg(required = true)]
        hunk_ids: Vec<String>,
    },

    /// Change a commit message without staging hunks
    ///
    /// For HEAD: delegates to `git commit --amend -m`.
    /// For older commits: uses a non-interactive rebase with reword.
    /// Requires a clean working tree (for non-HEAD targets).
    ///
    /// Examples:
    ///   squire reword HEAD -m "new message"
    ///   squire reword HEAD~2 -m "fix: corrected typo"
    #[command(verbatim_doc_comment)]
    Reword {
        /// The commit to reword
        #[arg(required = true)]
        commit: String,
        /// New commit message
        #[arg(short, long, required = true)]
        message: String,
    },

    /// Remove specific hunks from an existing commit
    ///
    /// Inverse of `amend`: finds hunks in the target commit and
    /// reverse-applies them. Use `squire diff <commit>~1 <commit>`
    /// or `squire log --json` to find hunk IDs.
    ///
    /// For HEAD: reverse-applies and amends.
    /// For older commits: uses rebase to pause, reverse-apply, amend,
    /// and continue. Requires a clean working tree.
    ///
    /// Examples:
    ///   squire drop HEAD abc12345
    ///   squire drop HEAD~2 abc12345 def67890
    #[command(verbatim_doc_comment)]
    Drop {
        /// The commit to drop hunks from
        #[arg(required = true)]
        commit: String,
        /// One or more hunk IDs to remove from the commit
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
    /// After splitting, use `GIT_EDITOR=true git rebase --continue` to replay
    /// remaining commits non-interactively (if the target was not HEAD).
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

    /// Edit a git rebase todo file (used as GIT_SEQUENCE_EDITOR)
    ///
    /// Reads a rebase todo file, applies the specified actions, and
    /// writes it back. Designed to replace sed/awk one-liners in
    /// non-interactive rebase operations.
    ///
    /// Actions use the syntax `action:sha-prefix` where action is
    /// one of: pick, reword, edit, squash, fixup, drop.
    ///
    /// Examples:
    ///   GIT_SEQUENCE_EDITOR="squire seqedit edit:abc1234" git rebase -i HEAD~3
    ///   GIT_SEQUENCE_EDITOR="squire seqedit fixup:abc1 drop:def5" git rebase -i HEAD~5
    #[command(verbatim_doc_comment)]
    Seqedit {
        /// Actions (action:sha-prefix) followed by the todo file path.
        /// Git passes the file as the last argument.
        #[arg(trailing_var_arg = true, required = true)]
        args: Vec<String>,
    },

    /// Squash commits into a target commit
    ///
    /// Folds one or more source commits into a target commit using a
    /// non-interactive rebase. The target commit's message is kept;
    /// source commits are discarded. Use -m to replace the message.
    /// Requires a clean working tree.
    ///
    /// Examples:
    ///   squire squash HEAD~2 HEAD~1 HEAD    # squash last 2 into HEAD~2
    ///   squire squash -m "new msg" abc1234 def5678
    #[command(verbatim_doc_comment)]
    Squash {
        /// Optional replacement commit message for the target
        #[arg(short, long)]
        message: Option<String>,
        /// Target commit (first) followed by source commits to fold in
        #[arg(required = true, num_args = 2..)]
        commits: Vec<String>,
    },

    /// Stash specific hunks into git stash
    ///
    /// Temporarily removes the selected hunks from the working tree and
    /// saves them as a regular git stash entry. Other unstaged changes
    /// are preserved. Use `git stash pop` to restore.
    ///
    /// Examples:
    ///   squire stash abc12345              # stash one hunk
    ///   squire stash -m "wip" abc12345    # stash with a message
    ///   squire stash abc12345:f3,a1       # stash specific lines
    ///   squire stash abc12345 def67890    # stash multiple hunks
    #[command(verbatim_doc_comment)]
    Stash {
        /// Optional stash message
        #[arg(short, long)]
        message: Option<String>,
        /// One or more hunk IDs to stash
        #[arg(required = true)]
        hunk_ids: Vec<String>,
    },

    /// Print a contextualized rebase playbook
    ///
    /// Inspects the repo state and prints step-by-step instructions
    /// for rebasing onto the upstream branch. Creates a safety tag
    /// before the first rebase. Output adapts to the current state:
    /// pre-rebase, mid-rebase with conflicts, or up-to-date.
    ///
    /// Examples:
    ///   squire rebase                      # plain text playbook
    ///   squire rebase --onto origin/main   # override upstream
    ///   squire rebase --json               # structured output
    #[command(verbatim_doc_comment)]
    Rebase {
        /// Override the upstream ref to rebase onto
        #[arg(long)]
        onto: Option<String>,
    },
}
