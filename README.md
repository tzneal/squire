# squire

Non-interactive, hunk-addressable git staging CLI for LLMs.

![Splitting a commit with squire](doc/demo-split.gif)

`git add -p` is an interactive TUI that LLMs can't drive. squire
exposes the same hunk-level staging through single commands with
structured arguments, so an LLM (or script) can selectively stage,
unstage, revert, and show hunks without any interactive prompts. It also
provides branch cleanup analysis to identify merged, squash-merged,
and stale branches.

Every hunk gets a short, stable, content-based ID (first 8 hex chars
of the hunk content's SHA-256 hash). Each line within a hunk also gets
a short content hash (shortest unique prefix, min 2 hex chars). Use
these IDs and line hashes to reference hunks and individual lines
across commands.

## Install

```bash
cargo install --path .
```

Requires Rust 1.85+ and `git` on your PATH.

## Usage

Default output is human-readable plain text. Global flags:

- `--json` — structured JSON output
- `--short` — compact one-line-per-hunk summary (ID, file, range,
  +/- counts)
- `--llm-help` — print a comprehensive reference for LLM
  consumption, then exit

### View hunks

```bash
squire diff                          # unstaged working tree changes (includes untracked)
squire diff --cached                 # staged changes
squire diff HEAD~1                   # working tree vs ref
squire diff HEAD~1 HEAD~2            # ref vs ref
squire diff -- src/main.rs           # filter by path
squire diff --json                   # output as JSON
```

### Inspect a hunk

```bash
squire show abc12345                 # hunk from working tree or staged
squire show HEAD abc12345            # hunk from last commit (falls back to diff)
squire show HEAD~2 abc12345          # hunk from two commits ago
```

### Stage, unstage, and revert hunks

```bash
squire stage abc12345 def67890       # stage specific hunks
squire stage abc12345:f3,a1          # stage specific lines by hash
squire stage abc12345:f3-7b          # stage a range of lines
squire unstage abc12345              # unstage specific hunks
squire revert abc12345               # discard changes from working tree
squire revert abc12345:f3,a1         # revert specific lines
```

### Commit and amend

```bash
squire commit -m "feat: parser" abc12345       # stage + commit in one step
squire amend abc12345                          # amend into HEAD
squire amend -m "new msg" abc12345             # amend HEAD with new message
squire amend --commit HEAD~2 abc12345          # amend into an older commit
```

When `--commit` targets a non-HEAD commit, squire creates a fixup
commit and runs an autosquash rebase to fold it in. The `-m` flag
is only supported when amending HEAD.

### Check status

```bash
squire status                        # plain text summary
squire status --json                 # structured output
```

## Stage untracked files selectively

Untracked files always appear as new-file hunks in `squire diff`, so you
can stage them with the same workflow as modified files.

```bash
# See all changes including new files
$ squire diff
--- b/src/new_module.rs ---
[a1b2c3d4] @@ -0,0 +1,40 @@
+... entire new file as a single hunk ...

# Stage the whole new file by hunk ID
$ squire stage a1b2c3d4
$ git commit -m "feat: add new_module"
```

## Branch cleanup

```bash
squire cleanup                       # auto-detect master branch
squire cleanup --master main         # specify master branch
squire cleanup --json                # structured output for LLM
```

Analyzes local branches and classifies each as:

- **MERGED** — fully merged via git ancestry
- **MERGED_EQUIVALENT** — commit messages and patches match master
  (squash/cherry-pick merge)
- **NEEDS_EVALUATION** — some commit messages match master but patches
  differ; an LLM should review these commits to determine if the
  branch is fully merged
- **UNMERGED** — no matching commits found in master

## Split a commit

```bash
squire split <commit>                # prepare to split a commit
```

Requires a clean working tree. Resets the target commit so its
changes are unstaged, ready for selective re-staging with `squire stage`.

For HEAD, this is a simple mixed reset. For older commits, squire runs a
non-interactive rebase that pauses at the target commit and resets it.

```bash
# Split the most recent commit into two
$ squire split abc1234
$ squire diff --json                 # see the unstaged changes
$ squire stage <id1> <id2>          # stage hunks for first commit
$ git commit -m "feat: part one"
$ squire stage <id3>                # stage remaining hunks
$ git commit -m "feat: part two"

# Split an older commit (rebase pauses at the commit)
$ squire split def5678
$ squire diff --json
$ squire stage <id> && git commit -m "first half"
$ squire stage <id> && git commit -m "second half"
$ git rebase --continue           # replay remaining commits
```

## Edit rebase todo (sequence editor)

```bash
GIT_SEQUENCE_EDITOR="squire seqedit edit:abc1234" git rebase -i HEAD~3
GIT_SEQUENCE_EDITOR="squire seqedit fixup:abc1 drop:def5" git rebase -i HEAD~5
```

`squire seqedit` rewrites a git rebase todo file, replacing sed/awk
one-liners. It accepts one or more `action:sha-prefix` arguments
followed by the todo file path (passed automatically by git).

Supported actions: `pick`, `edit`, `squash`, `fixup`, `drop`.

## How it works

squire wraps standard git primitives:

- `git diff` and `git diff --cached` for working tree and index diffs
- `git show <sha>` for commit diffs
- `git apply --cached` to stage patches
- `git apply --cached --reverse` to unstage patches
- `git apply --reverse` to revert working tree changes

`squire` parses unified diff output, assigns content-hash IDs to each hunk,
and reconstructs patches from selected hunks when staging or
unstaging.

## JSON output

Pass `--json` to get structured output from any command. `diff` and
`show` return an array of hunk objects:

```json
[
  {
    "id": "abc12345",
    "file": "src/main.rs",
    "old_file": "src/main.rs",
    "old_range": "10,5",
    "new_range": "10,7",
    "content": " ctx\n-old\n+new\n ...",
    "header": "fn main()",
    "line_hashes": ["e0", "90", "ca", "..."]
  }
]
```

The `header` field is present only when the `@@` line includes a
section header (for example, a function name).

`stage`, `unstage`, and `revert` return a result object:

```json
{ "staged": 2, "message": "Staged 2 hunk(s)" }
```

`status` returns branch info, rebase state, hunks, and line counts.
The `staged` and `unstaged` arrays contain the same hunk objects as
`diff`:

```json
{
  "branch": "main",
  "rebase_in_progress": false,
  "staged": [{ "id": "...", "file": "...", "..." : "..." }],
  "unstaged": [{ "id": "...", "file": "...", "..." : "..." }],
  "staged_lines": { "added": 3, "removed": 1 },
  "unstaged_lines": { "added": 5, "removed": 2 }
}
```

## Development

```bash
cargo build
cargo test
```

## License

MIT — see [LICENSE](LICENSE).
