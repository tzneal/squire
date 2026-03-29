use patch::{Line, Patch};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// A parsed hunk with its content-hash ID.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct HunkInfo {
    /// Short content-hash ID (first 8 hex chars of SHA-256)
    pub id: String,
    /// File path (new side)
    pub file: String,
    /// Old file path
    pub old_file: String,
    /// Old file line range as "start,count"
    pub old_range: String,
    /// New file line range as "start,count"
    pub new_range: String,
    /// The raw hunk content (with +/- prefixes)
    pub content: String,
    /// Section header from the @@ line (e.g. function name), if present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// Short unique hash for each content line
    pub line_hashes: Vec<String>,
    /// True if this hunk ends without a trailing newline in the file
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_newline: bool,
}

/// Parse a unified diff string into a list of HunkInfos with content-hash IDs.
pub fn parse_diff(diff_text: &str) -> Result<Vec<HunkInfo>, String> {
    if diff_text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Strip `\ No newline at end of file` markers before parsing — the
    // `patch` crate panics when this marker appears mid-hunk (e.g. between
    // a `-` and `+` line when a file gains a trailing newline).  We detect
    // when the marker follows a `+` line (new side lacks trailing newline)
    // so we can restore the `no_newline` flag per file.
    let mut cleaned = String::with_capacity(diff_text.len());
    // NOTE: tracking is per-file; git produces one patch block per file so
    // this is safe for all standard `git diff` / `git log -p` output.
    let mut no_nl_new_side = std::collections::HashSet::new();
    let mut current_file = String::new();
    let mut prev_was_add = false;
    for line in diff_text.lines() {
        if line.starts_with("+++ ") {
            current_file = strip_diff_prefix(line.trim_start_matches("+++ "));
        }
        if line.starts_with("\\ No newline at end of file") {
            if prev_was_add {
                no_nl_new_side.insert(current_file.clone());
            }
            continue;
        }
        prev_was_add = line.starts_with('+') && !line.starts_with("+++ ");
        cleaned.push_str(line);
        cleaned.push('\n');
    }

    let patches =
        Patch::from_multiple(&cleaned).map_err(|e| format!("failed to parse diff: {e}"))?;
    let mut hunks = Vec::new();
    for patch in &patches {
        let file = strip_diff_prefix(&patch.new.path);
        let old_file = strip_diff_prefix(&patch.old.path);
        let hunk_count = patch.hunks.len();
        let file_no_nl = no_nl_new_side.contains(&file);
        for (hi, hunk) in patch.hunks.iter().enumerate() {
            let content = hunk_content_string(&hunk.lines);
            let old_range = format!("{},{}", hunk.old_range.start, hunk.old_range.count);
            let id = hunk_id(&file, &old_range, &content);
            let lines: Vec<&str> = content.lines().collect();
            let line_hashes = compute_line_hashes(&lines);
            hunks.push(HunkInfo {
                id,
                file: file.clone(),
                old_file: old_file.clone(),
                old_range,
                new_range: format!("{},{}", hunk.new_range.start, hunk.new_range.count),
                content,
                header: hunk.hint().map(String::from),
                line_hashes,
                no_newline: file_no_nl && hi == hunk_count - 1,
            });
        }
    }
    Ok(hunks)
}

/// Strip the `a/` or `b/` prefix that git adds to diff paths.
/// Leaves `/dev/null` and other paths unchanged.
fn strip_diff_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("a/").or_else(|| path.strip_prefix("b/")) {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// Reconstruct a valid unified diff patch from selected hunks.
/// Hunks are sorted by file and position to produce a valid patch
/// regardless of the order IDs were provided.
pub fn reconstruct_patch(hunks: &[&HunkInfo]) -> String {
    let mut sorted: Vec<&HunkInfo> = hunks.to_vec();
    sorted.sort_by(|a, b| {
        (&a.old_file, &a.file)
            .cmp(&(&b.old_file, &b.file))
            .then_with(|| {
                let a_start = a
                    .old_range
                    .split(',')
                    .next()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                let b_start = b
                    .old_range
                    .split(',')
                    .next()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                a_start.cmp(&b_start)
            })
    });
    let mut patch = String::new();
    let mut current_key: (&str, &str) = ("", "");
    for hunk in &sorted {
        let key = (hunk.old_file.as_str(), hunk.file.as_str());
        if key != current_key {
            current_key = key;
            let old_path = if hunk.old_file == "/dev/null" {
                "/dev/null".to_string()
            } else {
                format!("a/{}", hunk.old_file)
            };
            let new_path = if hunk.file == "/dev/null" {
                "/dev/null".to_string()
            } else {
                format!("b/{}", hunk.file)
            };
            patch.push_str(&format!("--- {old_path}\n+++ {new_path}\n"));
        }
        patch.push_str(&format!(
            "@@ -{} +{} @@{}\n",
            hunk.old_range,
            hunk.new_range,
            hunk.header
                .as_ref()
                .map(|h| format!(" {h}"))
                .unwrap_or_default()
        ));
        patch.push_str(&hunk.content);
        if hunk.no_newline {
            patch.push_str("\\ No newline at end of file\n");
        }
    }
    patch
}

fn hunk_content_string(lines: &[Line<'_>]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for line in lines {
        let (prefix, text) = match line {
            Line::Add(t) => ('+', *t),
            Line::Remove(t) => ('-', *t),
            Line::Context(t) => (' ', *t),
        };
        writeln!(s, "{prefix}{text}").unwrap();
    }
    s
}

/// Compute a short content-hash ID: first 8 hex chars of SHA-256 of the content.
fn hunk_id(file: &str, old_range: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file.as_bytes());
    hasher.update(b"\0");
    hasher.update(old_range.as_bytes());
    hasher.update(b"\0");
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    hex::encode(&hash[..4])
}

/// Compute full hex hash for a line, mixing in an occurrence index to distinguish duplicates.
fn line_hash_full(line: &str, occurrence: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(line.as_bytes());
    hasher.update(occurrence.to_le_bytes());
    hex::encode(hasher.finalize())
}

/// Compute shortest-unique-prefix hashes (min 2 chars) for a set of lines.
pub fn compute_line_hashes(lines: &[&str]) -> Vec<String> {
    // Track per-content occurrence count so duplicate lines get distinct hashes
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    let full: Vec<String> = lines
        .iter()
        .map(|l| {
            let occ = counts.entry(l).or_insert(0);
            let h = line_hash_full(l, *occ);
            *occ += 1;
            h
        })
        .collect();
    // Find minimum prefix length where all hashes are unique
    let mut len = 2;
    loop {
        let mut seen = std::collections::HashSet::new();
        let all_unique = full.iter().all(|h| seen.insert(&h[..len.min(h.len())]));
        if all_unique || len >= 8 {
            break;
        }
        len += 1;
    }
    full.iter()
        .map(|h| h[..len.min(h.len())].to_string())
        .collect()
}

/// Generate synthetic unified diff text for untracked files so they appear as new-file hunks.
/// Binary files are skipped and their paths are returned separately.
pub fn generate_untracked_diff(
    files: &[String],
    repo_root: &std::path::Path,
) -> Result<(String, Vec<String>), String> {
    use std::fmt::Write;
    let mut out = String::new();
    let mut binary = Vec::new();
    for file in files {
        let abs = repo_root.join(file);
        let bytes = std::fs::read(&abs).map_err(|e| format!("failed to read {file}: {e}"))?;
        if is_binary(&bytes) {
            binary.push(file.clone());
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let count = lines.len();
        writeln!(out, "--- /dev/null").unwrap();
        writeln!(out, "+++ b/{file}").unwrap();
        if count == 0 {
            writeln!(out, "@@ -0,0 +0,0 @@").unwrap();
        } else {
            writeln!(out, "@@ -0,0 +1,{count} @@").unwrap();
            for (i, line) in lines.iter().enumerate() {
                if i == count - 1 && !text.ends_with('\n') {
                    // Last line without trailing newline — omit the writeln newline
                    // and append the no-newline marker
                    write!(out, "+{line}\n\\ No newline at end of file\n").unwrap();
                } else {
                    writeln!(out, "+{line}").unwrap();
                }
            }
        }
    }
    Ok((out, binary))
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8000)].contains(&0)
}

/// Build a sub-hunk containing only the selected lines (by line hash).
/// Unselected `-` lines become context, unselected `+` lines are dropped.
pub fn select_lines(
    hunk: &HunkInfo,
    selected_hashes: &[&str],
    reverse: bool,
) -> Result<HunkInfo, String> {
    let lines: Vec<&str> = hunk.content.lines().collect();
    if lines.is_empty() {
        return Err("hunk has no content lines".into());
    }

    // Resolve each selected hash to a line index
    let mut selected_indices = std::collections::HashSet::new();
    for sel in selected_hashes {
        let matches: Vec<usize> = hunk
            .line_hashes
            .iter()
            .enumerate()
            .filter(|(_, h)| h.starts_with(*sel))
            .map(|(i, _)| i)
            .collect();
        match matches.len() {
            0 => return Err(format!("line hash '{sel}' not found in hunk {}", hunk.id)),
            1 => {
                selected_indices.insert(matches[0]);
            }
            _ => {
                return Err(format!(
                    "line hash '{sel}' is ambiguous in hunk {}",
                    hunk.id
                ));
            }
        }
    }

    let old_start: u64 = hunk
        .old_range
        .split(',')
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or("invalid old_range")?;
    let new_start: u64 = hunk
        .new_range
        .split(',')
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or("invalid new_range")?;

    // Build new content: selected lines keep their prefix,
    // unselected `-` becomes context (keep the line in old, so it appears in new too),
    // unselected `+` is dropped entirely.
    let mut new_lines = Vec::new();
    let mut old_count = 0u64;
    let mut new_count = 0u64;
    for (i, &line) in lines.iter().enumerate() {
        let prefix = line.as_bytes().first().copied();
        if selected_indices.contains(&i) {
            // Keep as-is
            new_lines.push(line.to_string());
            match prefix {
                Some(b'-') => old_count += 1,
                Some(b'+') => new_count += 1,
                _ => {
                    old_count += 1;
                    new_count += 1;
                }
            }
        } else {
            match prefix {
                Some(b'-') if !reverse => {
                    new_lines.push(format!(" {}", &line[1..]));
                    old_count += 1;
                    new_count += 1;
                }
                Some(b'+') if reverse => {
                    new_lines.push(format!(" {}", &line[1..]));
                    old_count += 1;
                    new_count += 1;
                }
                Some(b'-') | Some(b'+') => {} // drop
                _ => {
                    new_lines.push(line.to_string());
                    old_count += 1;
                    new_count += 1;
                }
            }
        }
    }

    let mut content = String::new();
    for line in &new_lines {
        content.push_str(line);
        content.push('\n');
    }

    let content_lines: Vec<&str> = content.lines().collect();
    let line_hashes = compute_line_hashes(&content_lines);

    let old_range = format!("{},{}", old_start, old_count);

    Ok(HunkInfo {
        id: hunk_id(&hunk.file, &old_range, &content),
        file: hunk.file.clone(),
        old_file: hunk.old_file.clone(),
        old_range,
        new_range: format!("{},{}", new_start, new_count),
        content,
        header: hunk.header.clone(),
        line_hashes,
        no_newline: hunk.no_newline && {
            // Only propagate if the original final + line is still in the sub-hunk.
            let last_add = lines.iter().rposition(|l| l.starts_with('+'));
            last_add.is_some_and(|i| selected_indices.contains(&i))
        },
    })
}

/// A commit with its metadata and parsed hunks.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub hunks: Vec<HunkInfo>,
}

/// Parse `git log --format=%H%x00%an%x00%aI%x00%s -p` output into commits with hunks.
pub fn parse_log(raw: &str) -> Result<Vec<CommitInfo>, String> {
    let mut commits = Vec::new();
    let mut current: Option<CommitInfo> = None;
    let mut diff_buf = String::new();

    for line in raw.lines() {
        if line.contains('\0') {
            // Flush previous commit
            if let Some(mut ci) = current.take() {
                if !diff_buf.trim().is_empty() {
                    ci.hunks = parse_diff(&diff_buf)?;
                }
                commits.push(ci);
                diff_buf.clear();
            }
            let parts: Vec<&str> = line.splitn(4, '\0').collect();
            if parts.len() < 4 {
                return Err(format!("unexpected log line: {line}"));
            }
            current = Some(CommitInfo {
                sha: parts[0].to_string(),
                author: parts[1].to_string(),
                date: parts[2].to_string(),
                message: parts[3].to_string(),
                hunks: Vec::new(),
            });
        } else if current.is_some() {
            diff_buf.push_str(line);
            diff_buf.push('\n');
        }
    }
    // Flush last commit
    if let Some(mut ci) = current {
        if !diff_buf.trim().is_empty() {
            ci.hunks = parse_diff(&diff_buf)?;
        }
        commits.push(ci);
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DIFF: &str = "\
--- a/hello.txt
+++ b/hello.txt
@@ -1,3 +1,3 @@
 line1
-old line
+new line
 line3
";

    #[test]
    fn parse_produces_correct_hunk_count() {
        let hunks = parse_diff(SAMPLE_DIFF).unwrap();
        assert_eq!(hunks.len(), 1);
    }

    #[test]
    fn hunk_id_is_deterministic() {
        let hunks1 = parse_diff(SAMPLE_DIFF).unwrap();
        let hunks2 = parse_diff(SAMPLE_DIFF).unwrap();
        assert_eq!(hunks1[0].id, hunks2[0].id);
    }

    #[test]
    fn hunk_id_is_8_hex_chars() {
        let hunks = parse_diff(SAMPLE_DIFF).unwrap();
        assert_eq!(hunks[0].id.len(), 8);
        assert!(hunks[0].id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hunk_has_correct_file() {
        let hunks = parse_diff(SAMPLE_DIFF).unwrap();
        assert_eq!(hunks[0].file, "hello.txt");
    }

    #[test]
    fn hunk_has_correct_ranges() {
        let hunks = parse_diff(SAMPLE_DIFF).unwrap();
        assert_eq!(hunks[0].old_range, "1,3");
        assert_eq!(hunks[0].new_range, "1,3");
    }

    #[test]
    fn hunk_content_has_diff_markers() {
        let hunks = parse_diff(SAMPLE_DIFF).unwrap();
        assert!(hunks[0].content.contains("-old line"));
        assert!(hunks[0].content.contains("+new line"));
        assert!(hunks[0].content.contains(" line1"));
    }

    #[test]
    fn different_content_produces_different_ids() {
        let diff2 = "\
--- a/hello.txt
+++ b/hello.txt
@@ -1,3 +1,3 @@
 line1
-something else
+another thing
 line3
";
        let hunks1 = parse_diff(SAMPLE_DIFF).unwrap();
        let hunks2 = parse_diff(diff2).unwrap();
        assert_ne!(hunks1[0].id, hunks2[0].id);
    }

    #[test]
    fn empty_diff_returns_empty_vec() {
        let hunks = parse_diff("").unwrap();
        assert!(hunks.is_empty());
    }

    #[test]
    fn multi_file_diff_parses_all_hunks() {
        let diff = "\
--- a/file1.txt
+++ b/file1.txt
@@ -1,2 +1,2 @@
-old1
+new1
 ctx
--- a/file2.txt
+++ b/file2.txt
@@ -1,2 +1,2 @@
-old2
+new2
 ctx
";
        let hunks = parse_diff(diff).unwrap();
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].file, "file1.txt");
        assert_eq!(hunks[1].file, "file2.txt");
    }

    const SELECT_DIFF: &str = "\
--- a/f.txt
+++ b/f.txt
@@ -1,6 +1,6 @@
 ctx1
-old1
+new1
 ctx2
-old2
+new2
 ctx3
";

    #[test]
    fn line_hashes_are_unique_and_min_2_chars() {
        let hunks = parse_diff(SELECT_DIFF).unwrap();
        let hashes = &hunks[0].line_hashes;
        assert_eq!(hashes.len(), 7);
        assert!(hashes.iter().all(|h| h.len() >= 2));
        // All unique
        let set: std::collections::HashSet<&str> = hashes.iter().map(|h| h.as_str()).collect();
        assert_eq!(set.len(), hashes.len());
    }

    #[test]
    fn select_lines_stages_first_change() {
        let hunks = parse_diff(SELECT_DIFF).unwrap();
        let h = &hunks[0];
        // Select the -old1 and +new1 lines (indices 1 and 2)
        let sel = vec![h.line_hashes[1].as_str(), h.line_hashes[2].as_str()];
        let sub = select_lines(h, &sel, false).unwrap();
        assert!(sub.content.contains("-old1"));
        assert!(sub.content.contains("+new1"));
        // old2/new2 should not be changed — -old2 becomes context, +new2 dropped
        assert!(!sub.content.contains("-old2"));
        assert!(!sub.content.contains("+new2"));
        assert!(sub.content.contains(" old2"));
    }

    #[test]
    fn select_lines_unknown_hash_fails() {
        let hunks = parse_diff(SELECT_DIFF).unwrap();
        let result = select_lines(&hunks[0], &["zzzzzz"], false);
        assert!(result.is_err());
    }

    const HEADER_DIFF: &str = "\
--- a/lib.rs
+++ b/lib.rs
@@ -10,3 +10,3 @@ fn example()
 ctx
-old
+new
 ctx
";

    #[test]
    fn parse_populates_header_when_present() {
        let hunks = parse_diff(HEADER_DIFF).unwrap();
        assert_eq!(hunks[0].header.as_deref(), Some("fn example()"));
    }

    #[test]
    fn parse_leaves_header_none_when_absent() {
        let hunks = parse_diff(SAMPLE_DIFF).unwrap();
        assert!(hunks[0].header.is_none());
    }

    #[test]
    fn select_lines_preserves_header() {
        let hunks = parse_diff(HEADER_DIFF).unwrap();
        let h = &hunks[0];
        let sel = vec![h.line_hashes[1].as_str(), h.line_hashes[2].as_str()];
        let sub = select_lines(h, &sel, false).unwrap();
        assert_eq!(sub.header.as_deref(), Some("fn example()"));
    }

    #[test]
    fn header_omitted_from_json_when_none() {
        let hunks = parse_diff(SAMPLE_DIFF).unwrap();
        let json = serde_json::to_string(&hunks[0]).unwrap();
        assert!(!json.contains("header"));
    }

    #[test]
    fn header_included_in_json_when_present() {
        let hunks = parse_diff(HEADER_DIFF).unwrap();
        let json = serde_json::to_string(&hunks[0]).unwrap();
        assert!(json.contains("\"header\":\"fn example()\""));
    }

    #[test]
    fn parse_log_with_leading_garbage_lines() {
        // Simulate git outputting unexpected lines before the first commit header
        let raw = "some warning line\n\
                   abc123\x00Author\x002026-01-01T00:00:00Z\x00first commit\n\
                   --- a/f.txt\n\
                   +++ b/f.txt\n\
                   @@ -1,1 +1,1 @@\n\
                   -old\n\
                   +new\n";
        let commits = parse_log(raw).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "first commit");
        assert_eq!(commits[0].hunks.len(), 1);
    }

    #[test]
    fn no_newline_mid_diff_does_not_panic() {
        // Old side had no trailing newline, new side adds one. The
        // `\ No newline at end of file` marker sits between the `-` and `+`
        // lines of the same hunk.
        let diff = "\
diff --git a/a.json b/a.json
index aaa..bbb 100644
--- a/a.json
+++ b/a.json
@@ -1,2 +1,2 @@
 [
-}
\\ No newline at end of file
+}
";
        let hunks = parse_diff(diff).unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file, "a.json");
    }

    #[test]
    fn no_newline_new_side_sets_flag() {
        let diff = "\
diff --git a/a.txt b/a.txt
index aaa..bbb 100644
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-old
+new
\\ No newline at end of file
";
        let hunks = parse_diff(diff).unwrap();
        assert!(hunks[0].no_newline);
    }

    #[test]
    fn reconstruct_patch_new_file_uses_dev_null() {
        let h = HunkInfo {
            id: "aaaa1111".to_string(),
            file: "new.txt".to_string(),
            old_file: "/dev/null".to_string(),
            old_range: "0,0".to_string(),
            new_range: "1,1".to_string(),
            content: "+hello\n".to_string(),
            header: None,
            line_hashes: vec!["aa".to_string()],
            no_newline: false,
        };
        let patch = reconstruct_patch(&[&h]);
        assert!(
            patch.contains("--- /dev/null"),
            "old side should be /dev/null"
        );
        assert!(patch.contains("+++ b/new.txt"));
    }

    #[test]
    fn reconstruct_patch_deleted_file_uses_dev_null() {
        let h = HunkInfo {
            id: "bbbb2222".to_string(),
            file: "/dev/null".to_string(),
            old_file: "gone.txt".to_string(),
            old_range: "1,1".to_string(),
            new_range: "0,0".to_string(),
            content: "-bye\n".to_string(),
            header: None,
            line_hashes: vec!["bb".to_string()],
            no_newline: false,
        };
        let patch = reconstruct_patch(&[&h]);
        assert!(patch.contains("--- a/gone.txt"));
        assert!(
            patch.contains("+++ /dev/null"),
            "new side should be /dev/null"
        );
    }

    #[test]
    fn reconstruct_patch_appends_no_newline_marker() {
        let h = HunkInfo {
            id: "cccc3333".to_string(),
            file: "f.txt".to_string(),
            old_file: "f.txt".to_string(),
            old_range: "1,1".to_string(),
            new_range: "1,1".to_string(),
            content: "-old\n+new\n".to_string(),
            header: None,
            line_hashes: vec!["cc".to_string(), "dd".to_string()],
            no_newline: true,
        };
        let patch = reconstruct_patch(&[&h]);
        assert!(patch.contains("\\ No newline at end of file"));
    }

    #[test]
    fn generate_untracked_diff_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("empty.txt"), "").unwrap();
        let (diff, binaries) =
            generate_untracked_diff(&["empty.txt".to_string()], dir.path()).unwrap();
        assert!(binaries.is_empty());
        assert!(diff.contains("@@ -0,0 +0,0 @@"));
    }

    #[test]
    fn generate_untracked_diff_binary_file_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bin.dat"), b"\x00\x01\x02").unwrap();
        let (diff, binaries) =
            generate_untracked_diff(&["bin.dat".to_string()], dir.path()).unwrap();
        assert!(diff.is_empty());
        assert_eq!(binaries, vec!["bin.dat"]);
    }

    #[test]
    fn select_lines_empty_content_fails() {
        let h = HunkInfo {
            id: "deadbeef".to_string(),
            file: "f.txt".to_string(),
            old_file: "f.txt".to_string(),
            old_range: "1,0".to_string(),
            new_range: "1,0".to_string(),
            content: String::new(),
            header: None,
            line_hashes: vec![],
            no_newline: false,
        };
        assert!(select_lines(&h, &["aa"], false).is_err());
    }

    #[test]
    fn select_lines_ambiguous_hash_fails() {
        let hunks = parse_diff(SELECT_DIFF).unwrap();
        assert!(!hunks.is_empty()); // sanity check
        // Construct a hunk with duplicate hash prefixes manually
        let h = HunkInfo {
            id: "deadbeef".to_string(),
            file: "f.txt".to_string(),
            old_file: "f.txt".to_string(),
            old_range: "1,2".to_string(),
            new_range: "1,2".to_string(),
            content: " ctx1\n ctx2\n".to_string(),
            header: None,
            line_hashes: vec!["aabb".to_string(), "aacc".to_string()],
            no_newline: false,
        };
        let result = select_lines(&h, &["aa"], false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ambiguous"));
    }

    #[test]
    fn select_lines_selected_context_counts_both_sides() {
        // Select a context line — it should count toward both old and new
        let diff = "\
--- a/f.txt
+++ b/f.txt
@@ -1,3 +1,3 @@
 ctx1
-old
+new
 ctx2
";
        let hunks = parse_diff(diff).unwrap();
        let h = &hunks[0];
        // Select only the context line ctx1 (index 0)
        let sel = vec![h.line_hashes[0].as_str()];
        let sub = select_lines(h, &sel, false).unwrap();
        // Should contain ctx1 as a selected context line
        assert!(sub.content.contains(" ctx1"));
        // ctx1 selected→context(1,1), -old unselected→context(1,1), +new dropped, ctx2 context(1,1)
        assert_eq!(sub.old_range, "1,3");
        assert_eq!(sub.new_range, "1,3");
    }

    #[test]
    fn parse_log_malformed_line_errors() {
        // A line with \0 but fewer than 4 parts
        let raw = "abc123\0Author\0date_only\n";
        let result = parse_log(raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unexpected log line"));
    }

    #[test]
    fn reconstruct_patch_different_old_files_same_new_file() {
        // Two hunks with different old_file but same file (e.g. two renames to same target)
        let h1 = HunkInfo {
            id: "aaaa1111".to_string(),
            file: "target.txt".to_string(),
            old_file: "old_a.txt".to_string(),
            old_range: "1,1".to_string(),
            new_range: "1,1".to_string(),
            content: "-a\n+b\n".to_string(),
            header: None,
            line_hashes: vec!["aa".to_string(), "bb".to_string()],
            no_newline: false,
        };
        let h2 = HunkInfo {
            id: "bbbb2222".to_string(),
            file: "target.txt".to_string(),
            old_file: "old_b.txt".to_string(),
            old_range: "1,1".to_string(),
            new_range: "1,1".to_string(),
            content: "-c\n+d\n".to_string(),
            header: None,
            line_hashes: vec!["cc".to_string(), "dd".to_string()],
            no_newline: false,
        };
        let refs = vec![&h1, &h2];
        let patch = reconstruct_patch(&refs);
        // Both old_file paths must appear in the patch
        assert!(
            patch.contains("--- a/old_a.txt"),
            "missing old_a.txt in:\n{patch}"
        );
        assert!(
            patch.contains("--- a/old_b.txt"),
            "missing old_b.txt in:\n{patch}"
        );
    }

    #[test]
    fn select_lines_no_newline_not_propagated_when_final_add_dropped() {
        // File has no trailing newline. The last hunk line is `+new2` (no newline).
        // If we select only the first change pair and drop `+new2`,
        // the sub-hunk should NOT have no_newline set.
        let diff = "\
--- a/f.txt
+++ b/f.txt
@@ -1,4 +1,4 @@
 ctx
-old1
+new1
-old2
+new2
\\ No newline at end of file
";
        let hunks = parse_diff(diff).unwrap();
        assert!(hunks[0].no_newline, "source hunk should have no_newline");

        // Select only -old1 and +new1 (indices 1 and 2), dropping -old2 and +new2
        let h = &hunks[0];
        let sel = vec![h.line_hashes[1].as_str(), h.line_hashes[2].as_str()];
        let sub = select_lines(h, &sel, false).unwrap();

        assert!(
            !sub.no_newline,
            "sub-hunk should not have no_newline when the final + line was dropped"
        );
    }
}
