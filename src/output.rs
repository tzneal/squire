use crate::diff::{CommitInfo, HunkInfo};
use std::fmt::Write;

fn short_sha(sha: &str) -> &str {
    &sha[..8.min(sha.len())]
}

pub fn format_log_short(commits: &[CommitInfo]) -> String {
    let mut buf = String::new();
    if commits.is_empty() {
        writeln!(buf, "No commits.").unwrap();
        return buf;
    }
    for c in commits {
        let (add, del) = count_lines(&c.hunks);
        let refs = if c.refs.is_empty() {
            String::new()
        } else {
            format!(" ({})", c.refs.join(", "))
        };
        writeln!(
            buf,
            "{}{}  {}  {} hunk{}  +{}/-{}",
            short_sha(&c.sha),
            refs,
            c.message,
            c.hunks.len(),
            if c.hunks.len() == 1 { "" } else { "s" },
            add,
            del
        )
        .unwrap();
    }
    buf
}

pub fn format_log_plain(commits: &[CommitInfo]) -> String {
    let mut buf = String::new();
    if commits.is_empty() {
        writeln!(buf, "No commits.").unwrap();
        return buf;
    }
    for c in commits {
        let refs = if c.refs.is_empty() {
            String::new()
        } else {
            format!(" ({})", c.refs.join(", "))
        };
        writeln!(buf, "{}{}  {}", short_sha(&c.sha), refs, c.message).unwrap();
        writeln!(buf, "  {}", c.date).unwrap();
        for hunk in &c.hunks {
            let (add, del) = count_hunk_lines(hunk);
            writeln!(
                buf,
                "  {}  @@ -{} +{} @@  +{}/-{}{}",
                hunk.file,
                hunk.old_range,
                hunk.new_range,
                add,
                del,
                hunk.header
                    .as_ref()
                    .map(|h| format!("  {h}"))
                    .unwrap_or_default()
            )
            .unwrap();
        }
        writeln!(buf).unwrap();
    }
    buf
}

pub fn count_lines(hunks: &[HunkInfo]) -> (usize, usize) {
    hunks
        .iter()
        .map(count_hunk_lines)
        .fold((0, 0), |(a, d), (ha, hd)| (a + ha, d + hd))
}

pub fn count_hunk_lines(hunk: &HunkInfo) -> (usize, usize) {
    let mut add = 0;
    let mut del = 0;
    for line in hunk.content.lines() {
        match line.as_bytes().first() {
            Some(b'+') => add += 1,
            Some(b'-') => del += 1,
            _ => {}
        }
    }
    (add, del)
}

pub fn format_short(hunks: &[HunkInfo]) -> String {
    let mut buf = String::new();
    if hunks.is_empty() {
        writeln!(buf, "No changes.").unwrap();
        return buf;
    }
    for hunk in hunks {
        let (add, del) = count_hunk_lines(hunk);
        let first_change = hunk
            .content
            .lines()
            .find(|l| l.starts_with('+') || l.starts_with('-'))
            .map(|l| format!("{} {}", &l[..1], l[1..].trim_start()))
            .unwrap_or_default();
        writeln!(
            buf,
            "{}  {}  {}  +{}/-{}{}  {}",
            hunk.id,
            hunk.file,
            hunk.new_range,
            add,
            del,
            hunk.header
                .as_ref()
                .map(|h| format!("  {h}"))
                .unwrap_or_default(),
            first_change,
        )
        .unwrap();
    }
    buf
}

pub fn format_plain(hunks: &[HunkInfo]) -> String {
    let mut buf = String::new();
    if hunks.is_empty() {
        writeln!(buf, "No changes.").unwrap();
        return buf;
    }
    let mut current_file = "";
    for hunk in hunks {
        if hunk.file != current_file {
            current_file = &hunk.file;
            writeln!(buf, "--- {current_file} ---").unwrap();
        }
        writeln!(
            buf,
            "[{}] @@ -{} +{} @@{}",
            hunk.id,
            hunk.old_range,
            hunk.new_range,
            hunk.header
                .as_ref()
                .map(|h| format!(" {h}"))
                .unwrap_or_default()
        )
        .unwrap();
        for (line, hash) in hunk.content.lines().zip(&hunk.line_hashes) {
            writeln!(buf, "  {hash} {line}").unwrap();
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hunk() -> HunkInfo {
        HunkInfo {
            id: "abcd1234".to_string(),
            file: "src/main.rs".to_string(),
            old_file: "src/main.rs".to_string(),
            old_range: "1,3".to_string(),
            new_range: "1,4".to_string(),
            header: Some("fn main()".to_string()),
            content: "-old\n+new\n context\n".to_string(),
            line_hashes: vec!["aa".to_string(), "bb".to_string(), "cc".to_string()],
            no_newline: false,
        }
    }

    #[test]
    fn format_short_empty() {
        assert_eq!(format_short(&[]), "No changes.\n");
    }

    #[test]
    fn format_short_with_hunk() {
        let out = format_short(&[sample_hunk()]);
        assert!(out.contains("abcd1234"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("+1/-1"));
        assert!(out.contains("fn main()"));
        assert!(out.contains("- old"));
    }

    #[test]
    fn format_plain_empty() {
        assert_eq!(format_plain(&[]), "No changes.\n");
    }

    #[test]
    fn format_plain_with_hunk() {
        let out = format_plain(&[sample_hunk()]);
        assert!(out.contains("--- src/main.rs ---"));
        assert!(out.contains("[abcd1234]"));
        assert!(out.contains("fn main()"));
        assert!(out.contains("aa -old"));
    }

    #[test]
    fn format_short_no_header() {
        let mut h = sample_hunk();
        h.header = None;
        let out = format_short(&[h]);
        assert!(out.contains("+1/-1"));
        assert!(!out.contains("fn main()"));
    }
}
