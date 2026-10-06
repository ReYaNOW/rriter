use std::path::PathBuf;

use crate::editor::LineDiffHunk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlameCommit {
    pub oid: git2::Oid,
    pub short_oid: String,
    pub author: String,
    pub author_mail: String,
    pub author_time: i64,
    pub summary: String,
    pub uncommitted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GitBlame {
    pub commits: Vec<BlameCommit>,
    pub line_commit: Vec<u32>,
    pub age_ranks: Vec<f32>,
    pub column_labels: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GitBlameState {
    pub key: Option<(crate::platform::PathKey, PathBuf, git2::Oid)>,
    pub generation: u64,
    pub blame: Option<GitBlame>,
    pub pending: bool,
    pub failed_key: Option<(crate::platform::PathKey, PathBuf, git2::Oid)>,
    pub messages: Vec<(git2::Oid, String)>,
    pub column_open: bool,
    pub inline_text: String,
    pub inline_line: Option<usize>,
    pub inline_key: Option<(usize, usize, u64, u64)>,
    pub inline_since: Option<std::time::Instant>,
}

pub fn parse_porcelain(out: &str) -> Option<GitBlame> {
    let mut lines = out.lines().peekable();
    let mut commits = Vec::<BlameCommit>::new();
    let mut commit_indices = std::collections::HashMap::<git2::Oid, u32>::new();
    let mut line_commit = Vec::<Option<u32>>::new();
    let mut next_line = 1usize;
    let mut group_remaining = 0usize;
    let mut group_oid = None;

    while let Some(header) = lines.next() {
        let mut fields = header.split_whitespace();
        let oid_text = fields.next()?;
        if oid_text.len() != 40 || !oid_text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let oid = git2::Oid::from_str(oid_text).ok()?;
        let original_line = fields.next()?.parse::<usize>().ok()?;
        let final_line = fields.next()?.parse::<usize>().ok()?;
        if original_line == 0 || final_line == 0 || final_line < next_line {
            return None;
        }
        if group_remaining > 0 && group_oid != Some(oid) {
            return None;
        }
        if let Some(group_count) = fields.next() {
            if group_remaining > 0
                || group_count.parse::<usize>().ok().filter(|count| *count > 0).is_none()
            {
                return None;
            }
            group_remaining = group_count.parse().ok()?;
            group_oid = Some(oid);
        } else if group_remaining == 0 {
            group_remaining = 1;
            group_oid = Some(oid);
        }
        if fields.next().is_some() {
            return None;
        }

        let existing = commit_indices.get(&oid).copied();
        let mut author = None;
        let mut author_mail = None;
        let mut author_time = None;
        let mut summary = None;
        let mut content_found = false;
        for line in lines.by_ref() {
            if let Some(content) = line.strip_prefix('\t') {
                let index = if let Some(index) = existing {
                    index
                } else {
                    let author = author.unwrap_or_default();
                    let author_mail = author_mail.unwrap_or_default();
                    let author_time = author_time?;
                    let summary = summary.unwrap_or_default();
                    let index = u32::try_from(commits.len()).ok()?;
                    let full_oid = oid.to_string();
                    commits.push(BlameCommit {
                        oid,
                        short_oid: full_oid.chars().take(8).collect(),
                        author,
                        author_mail,
                        author_time,
                        summary,
                        uncommitted: oid.is_zero(),
                    });
                    commit_indices.insert(oid, index);
                    index
                };
                let _ = content;
                if final_line > line_commit.len() + 1 {
                    return None;
                }
                if final_line <= line_commit.len() {
                    return None;
                }
                line_commit.push(Some(index));
                next_line = final_line.saturating_add(1);
                group_remaining = group_remaining.checked_sub(1)?;
                if group_remaining == 0 {
                    group_oid = None;
                }
                content_found = true;
                break;
            }
            if line.is_empty() {
                return None;
            }
            if let Some((key, value)) = line.split_once(' ') {
                match key {
                    "author" if existing.is_none() => author = Some(value.to_owned()),
                    "author-mail" if existing.is_none() => {
                        author_mail = Some(value.trim_start_matches('<').trim_end_matches('>').to_owned())
                    }
                    "author-time" if existing.is_none() => author_time = value.parse::<i64>().ok(),
                    "summary" if existing.is_none() => summary = Some(value.to_owned()),
                    _ => {}
                }
            } else if line == "boundary" || line == "previous" {
                continue;
            } else {
                return None;
            }
        }
        if !content_found {
            return None;
        }
    }
    if group_remaining > 0 || line_commit.iter().any(Option::is_none) {
        return None;
    }
    let line_commit: Vec<u32> = line_commit.into_iter().collect::<Option<_>>()?;
    let age_ranks = age_rank(&commits.iter().map(|commit| commit.author_time).collect::<Vec<_>>());
    let column_labels = commits.iter().map(column_label).collect();
    Some(GitBlame { commits, line_commit, age_ranks, column_labels })
}

/// Maps a buffer line to HEAD; the result may be at or beyond HEAD's line count,
/// so callers must bound it before indexing blame data.
pub fn head_line_for(hunks: &[LineDiffHunk], line: usize) -> Option<usize> {
    let partition = hunks.partition_point(|hunk| hunk.after_start <= line);
    if partition > 0 {
        let hunk = hunks.get(partition - 1)?;
        if line < hunk.after_end {
            return None;
        }
        // Hunk bounds are absolute, so the nearest hunk above carries the offset of all earlier ones.
        return line.checked_sub(hunk.after_end)?.checked_add(hunk.before_end);
    }
    Some(line)
}

pub fn blame_blocks(line_commits: &[Option<u32>]) -> Vec<bool> {
    let mut starts = Vec::with_capacity(line_commits.len());
    let mut previous = None;
    for (index, current) in line_commits.iter().copied().enumerate() {
        let starts_block = index == 0 || current.is_none() || current != previous || previous.is_none();
        starts.push(starts_block);
        previous = current;
    }
    starts
}

pub fn age_rank(times: &[i64]) -> Vec<f32> {
    let mut unique_times = times.to_vec();
    unique_times.sort_unstable();
    unique_times.dedup();
    if unique_times.len() <= 1 {
        return vec![1.0; times.len()];
    }
    let newest_rank = unique_times.len() - 1;
    times
        .iter()
        .map(|time| {
            unique_times
                .binary_search(time)
                .map(|rank| rank as f32 / newest_rank as f32)
                .unwrap_or(1.0)
        })
        .collect()
}

pub fn truncate_to_width(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let keep = width.saturating_sub(1);
    let mut result: String = text.chars().take(keep).collect();
    result.push('…');
    result
}

fn column_label(commit: &BlameCommit) -> String {
    let days = commit.author_time.div_euclid(86_400);
    let (year, month, day) = unix_days_to_date(days);
    let date = format!("{day:02}.{month:02}.{:02} ", year.rem_euclid(100));
    let author = truncate_to_width(&commit.author, 24usize.saturating_sub(date.chars().count()));
    truncate_to_width(&format!("{date}{author}"), 24)
}

fn unix_days_to_date(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::LineDiffHunk;

    fn oid(ch: char) -> String {
        std::iter::repeat(ch).take(40).collect()
    }

    fn porcelain(entries: &[(&str, usize, usize, &str, &str, i64, &str)]) -> String {
        let mut out = String::new();
        for &(id, original, final_line, author, summary, time, content) in entries {
            out.push_str(&format!("{id} {original} {final_line} 1\n"));
            out.push_str(&format!("author {author}\nauthor-mail <{author}@example.com>\nauthor-time {time}\nauthor-tz +0000\nsummary {summary}\nfilename file.rs\n\t{content}\n"));
        }
        out
    }

    #[test]
    fn porcelain_deduplicates_commits_and_accepts_boundary_and_zero_oid() {
        let output = format!(
            "{} 1 1 2\nauthor Áda\nauthor-mail <ada@example.com>\nauthor-time 10\nauthor-tz +0000\nsummary first\nboundary\nfilename file.rs\n\tone\n{} 2 2\n\ttwo\n{} 3 3 1\nauthor Uncommitted\nauthor-mail <work@example.com>\nauthor-time 20\nauthor-tz +0000\nsummary work\nfilename file.rs\n\tthree\n",
            oid('a'), oid('a'), oid('0')
        );
        let blame = parse_porcelain(&output).expect("valid porcelain");
        assert_eq!(blame.commits.len(), 2);
        assert_eq!(blame.line_commit, vec![0, 0, 1]);
        assert_eq!(blame.commits[0].author, "Áda");
        assert!(blame.commits[1].uncommitted);
        assert_eq!(blame.commits[0].short_oid.len(), 8);
    }

    #[test]
    fn porcelain_rejects_malformed_headers_missing_content_and_gaps() {
        assert!(parse_porcelain("nope 1 1\n\ta\n").is_none());
        assert!(parse_porcelain(&format!("{} x 1\n\ta\n", oid('a'))).is_none());
        assert!(parse_porcelain(&format!("{} 1 1\n", oid('a'))).is_none());
        assert!(parse_porcelain(&format!("{} 1 2\n\ta\n", oid('a'))).is_none());
        assert!(parse_porcelain(&format!("{} 1 1 2\n\ta\n", oid('a'))).is_none());
        assert!(parse_porcelain("").is_some_and(|blame| blame.line_commit.is_empty()));
    }

    fn hunk(before_start: usize, before_end: usize, after_start: usize, after_end: usize) -> LineDiffHunk {
        LineDiffHunk { before_start, before_end, after_start, after_end }
    }

    #[test]
    fn head_line_mapping_handles_insert_delete_replace_and_hunks() {
        assert_eq!(head_line_for(&[hunk(2, 2, 2, 3)], 2), None);
        assert_eq!(head_line_for(&[hunk(2, 2, 2, 3)], 3), Some(2));
        assert_eq!(head_line_for(&[hunk(2, 4, 2, 2)], 2), Some(4));
        assert_eq!(head_line_for(&[hunk(0, 2, 0, 0)], 0), Some(2));
        assert_eq!(head_line_for(&[hunk(5, 7, 5, 5)], 5), Some(7));
        assert_eq!(head_line_for(&[hunk(2, 4, 2, 4)], 3), None);
        assert_eq!(head_line_for(&[hunk(1, 2, 1, 3), hunk(5, 7, 6, 6)], 7), Some(8));
        assert_eq!(head_line_for(&[hunk(0, 0, 0, 2)], 2), Some(0));
        assert_eq!(head_line_for(&[], 12), Some(12));
    }

    #[test]
    fn head_line_mapping_keeps_trailing_empty_line_outside_head_blame() {
        assert_eq!(head_line_for(&[], 2), Some(2));
        assert_eq!(head_line_for(&[hunk(0, 0, 0, 1)], 2), Some(1));
    }

    #[test]
    fn blocks_age_and_text_formatting_are_stable() {
        assert_eq!(blame_blocks(&[Some(1), Some(1), Some(2), None, Some(2)]), vec![true, false, true, true, true]);
        assert_eq!(age_rank(&[10, 20, 20]), vec![0.0, 1.0, 1.0]);
        assert_eq!(age_rank(&[10, 20, 1000]), vec![0.0, 0.5, 1.0]);
        assert_eq!(age_rank(&[10, 20, 20, 1000]), vec![0.0, 0.5, 0.5, 1.0]);
        assert_eq!(age_rank(&[5, 5]), vec![1.0, 1.0]);
        assert_eq!(truncate_to_width("абвг", 3), "аб…");
        assert_eq!(truncate_to_width("abcdef", 2), "a…");
        assert_eq!(truncate_to_width("abc", 3), "abc");
    }

    #[test]
    fn blame_builds_labels_with_a_24_character_cap() {
        let output = porcelain(&[(&oid('a'), 1, 1, "A very long author name that is not ASCII Ω", "subject", 1_700_000_000, "x")]);
        let blame = parse_porcelain(&output).expect("valid porcelain");
        assert_eq!(blame.column_labels.len(), 1);
        assert!(blame.column_labels[0].chars().count() <= 24);
        assert!(blame.column_labels[0].starts_with("14.11.23 "));
    }

    #[test]
    fn line_mapping_handles_thousands_of_hunks() {
        let hunks: Vec<_> = (0..5000)
            .map(|i| hunk(i, i, i * 2, i * 2 + 1))
            .collect();
        assert_eq!(head_line_for(&hunks, 1), Some(0));
        assert_eq!(head_line_for(&hunks, 10_000), Some(5_000));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHeadSnapshot {
    pub repo_root: PathBuf,
    pub repo_key: crate::platform::PathKey,
    pub rel_path: PathBuf,
    pub head_oid: Option<git2::Oid>,
    pub path_in_head: bool,
}
