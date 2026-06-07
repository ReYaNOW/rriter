use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink, SinkMatch};
use memchr::memmem::Finder;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::{
    PROJECT_SEARCH_FILE_CAP_BYTES, PROJECT_SEARCH_FILE_RESULT_CAP, PROJECT_SEARCH_MATCH_CAP,
    ProjectSearchFile, ProjectSearchMatch, SearchCaps, SearchPatternPlan, SearchProfile,
    elapsed_ms_u64, is_definitely_binary_project_search_file,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn search_project_file_grep(
    path: &Path,
    plan: &SearchPatternPlan,
    needle: &[u8],
    matcher: &RegexMatcher,
    searcher: &mut Searcher,
    case_sensitive: bool,
    caps: &Mutex<SearchCaps>,
    profile: &SearchProfile,
    capped_flag: &AtomicBool,
) -> Option<ProjectSearchFile> {
    if capped_flag.load(Ordering::Relaxed) {
        return None;
    }
    profile.files_seen.fetch_add(1, Ordering::Relaxed);
    if is_definitely_binary_project_search_file(path) {
        return None;
    }
    let file_len = path.metadata().ok()?.len();
    if file_len > PROJECT_SEARCH_FILE_CAP_BYTES {
        return None;
    }
    profile.files_read.fetch_add(1, Ordering::Relaxed);
    profile.bytes_read.fetch_add(file_len, Ordering::Relaxed);

    let scan_started = Instant::now();
    let mut sink = ProjectSearchGrepSink {
        needle,
        case_sensitive,
        caps,
        profile,
        capped_flag,
        matches: Vec::new(),
    };
    let search_ok = searcher.search_path(matcher, path, &mut sink).is_ok();
    profile
        .scan_ms
        .fetch_add(elapsed_ms_u64(scan_started), Ordering::Relaxed);
    if !search_ok || sink.matches.is_empty() {
        return None;
    }

    let relative_path = plan.relative_display(path);
    let icon_key = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(crate::app::file_icons::file_icon_key_for_name)
        .unwrap_or("default_file");
    Some(ProjectSearchFile {
        path: path.to_path_buf(),
        relative_path,
        icon_key,
        matches: sink.matches,
    })
}

struct ProjectSearchGrepSink<'a> {
    needle: &'a [u8],
    case_sensitive: bool,
    caps: &'a Mutex<SearchCaps>,
    profile: &'a SearchProfile,
    capped_flag: &'a AtomicBool,
    matches: Vec<ProjectSearchMatch>,
}

impl Sink for ProjectSearchGrepSink<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        if self.capped_flag.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let Some(line_number) = mat.line_number() else {
            return Ok(true);
        };
        let prep_started = Instant::now();
        let keep_going = push_project_search_line_matches(
            mat.bytes(),
            mat.absolute_byte_offset() as usize,
            line_number.saturating_sub(1).min(u32::MAX as u64) as u32,
            self.needle,
            self.case_sensitive,
            self.caps,
            self.capped_flag,
            &mut self.matches,
            self.profile,
        );
        self.profile
            .prep_ms
            .fetch_add(elapsed_ms_u64(prep_started), Ordering::Relaxed);
        Ok(keep_going)
    }
}

#[allow(clippy::too_many_arguments)]
fn push_project_search_line_matches(
    line_bytes_with_term: &[u8],
    line_start_abs: usize,
    line_number: u32,
    needle: &[u8],
    case_sensitive: bool,
    caps: &Mutex<SearchCaps>,
    capped_flag: &AtomicBool,
    matches: &mut Vec<ProjectSearchMatch>,
    profile: &SearchProfile,
) -> bool {
    if needle.is_empty() {
        return true;
    }
    let line_bytes = trim_project_search_line_term(line_bytes_with_term);
    let Ok(line) = std::str::from_utf8(line_bytes) else {
        return true;
    };
    let mut keep_going = true;
    collect_line_match_ranges(line.as_bytes(), needle, case_sensitive, |start, end| {
        if !reserve_project_search_match(caps, capped_flag, matches.is_empty()) {
            keep_going = false;
            return false;
        }
        profile.matches.fetch_add(1, Ordering::Relaxed);
        push_line_match(line, line_start_abs, line_number, start, end, matches);
        !capped_flag.load(Ordering::Relaxed)
    });
    keep_going && !capped_flag.load(Ordering::Relaxed)
}

fn trim_project_search_line_term(mut bytes: &[u8]) -> &[u8] {
    if bytes.last() == Some(&b'\n') {
        bytes = &bytes[..bytes.len() - 1];
    }
    if bytes.last() == Some(&b'\r') {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn collect_line_match_ranges(
    haystack: &[u8],
    needle: &[u8],
    case_sensitive: bool,
    mut emit: impl FnMut(usize, usize) -> bool,
) {
    if needle.is_empty() || haystack.len() < needle.len() {
        return;
    }
    if case_sensitive {
        let finder = Finder::new(needle);
        for start in finder.find_iter(haystack) {
            if !emit(start, start + needle.len()) {
                break;
            }
        }
        return;
    }
    let first = needle[0];
    let lower = first.to_ascii_lowercase();
    let upper = first.to_ascii_uppercase();
    if lower == upper {
        for start in memchr::memchr_iter(first, haystack) {
            if ascii_slice_matches(haystack, needle, start) && !emit(start, start + needle.len()) {
                break;
            }
        }
    } else {
        for start in memchr::memchr2_iter(lower, upper, haystack) {
            if ascii_slice_matches(haystack, needle, start) && !emit(start, start + needle.len()) {
                break;
            }
        }
    }
}

fn ascii_slice_matches(haystack: &[u8], needle: &[u8], start: usize) -> bool {
    let end = start.saturating_add(needle.len());
    end <= haystack.len() && haystack[start..end].eq_ignore_ascii_case(needle)
}

fn reserve_project_search_match(
    caps: &Mutex<SearchCaps>,
    capped_flag: &AtomicBool,
    first_match_in_file: bool,
) -> bool {
    let mut caps = match caps.lock() {
        Ok(caps) => caps,
        Err(_) => {
            capped_flag.store(true, Ordering::Relaxed);
            return false;
        }
    };
    if caps.capped
        || caps.matches >= PROJECT_SEARCH_MATCH_CAP
        || (first_match_in_file && caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP)
    {
        caps.capped = true;
        capped_flag.store(true, Ordering::Relaxed);
        return false;
    }
    if first_match_in_file {
        caps.files += 1;
    }
    caps.matches += 1;
    if caps.matches >= PROJECT_SEARCH_MATCH_CAP || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP {
        caps.capped = true;
        capped_flag.store(true, Ordering::Relaxed);
    }
    true
}

fn push_line_match(
    line: &str,
    line_start_abs: usize,
    line_number: u32,
    start: usize,
    end: usize,
    matches: &mut Vec<ProjectSearchMatch>,
) {
    let start = start.min(line.len());
    let end = end.min(line.len()).max(start);
    matches.push(ProjectSearchMatch {
        byte_start: line_start_abs.saturating_add(start),
        byte_end: line_start_abs.saturating_add(end),
        line_byte_start: line_start_abs,
        start_line: line_number,
        start_col: 0,
        end_line: line_number,
        end_col: 0,
        preview: String::new(),
        preview_match_start: 0,
        preview_match_end: 0,
        preview_ready: false,
        extra_lines: 0,
    });
}
