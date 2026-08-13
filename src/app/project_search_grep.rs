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
    commit_project_search_file_matches, elapsed_ms_u64, is_definitely_binary_project_search_file,
    utf16_units_between,
};

pub(super) enum ProjectSearchGrepResult {
    Complete(Option<ProjectSearchFile>),
    NeedsDecodedFallback,
}

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
) -> ProjectSearchGrepResult {
    if capped_flag.load(Ordering::Relaxed) {
        return ProjectSearchGrepResult::Complete(None);
    }
    profile.files_seen.fetch_add(1, Ordering::Relaxed);
    if is_definitely_binary_project_search_file(path) {
        return ProjectSearchGrepResult::Complete(None);
    }
    let Some(file_len) = path.metadata().ok().map(|meta| meta.len()) else {
        return ProjectSearchGrepResult::Complete(None);
    };
    if file_len > PROJECT_SEARCH_FILE_CAP_BYTES {
        return ProjectSearchGrepResult::Complete(None);
    }
    let match_limit = {
        let caps = crate::platform::lock_recover(caps);
        if caps.capped
            || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP
            || caps.matches >= PROJECT_SEARCH_MATCH_CAP
        {
            capped_flag.store(true, Ordering::Relaxed);
            return ProjectSearchGrepResult::Complete(None);
        }
        PROJECT_SEARCH_MATCH_CAP - caps.matches
    };
    profile.files_read.fetch_add(1, Ordering::Relaxed);
    profile.bytes_read.fetch_add(file_len, Ordering::Relaxed);

    let scan_started = Instant::now();
    let mut sink = ProjectSearchGrepSink {
        needle,
        case_sensitive,
        profile,
        capped_flag,
        match_limit,
        needs_decoded_fallback: false,
        matches: Vec::new(),
    };
    let search_ok = searcher.search_path(matcher, path, &mut sink).is_ok();
    profile
        .scan_ms
        .fetch_add(elapsed_ms_u64(scan_started), Ordering::Relaxed);
    if sink.needs_decoded_fallback {
        return ProjectSearchGrepResult::NeedsDecodedFallback;
    }
    if !search_ok || sink.matches.is_empty() {
        return ProjectSearchGrepResult::Complete(None);
    }
    if !commit_project_search_file_matches(&mut sink.matches, caps, capped_flag) {
        return ProjectSearchGrepResult::Complete(None);
    }
    profile
        .matches
        .fetch_add(sink.matches.len() as u64, Ordering::Relaxed);

    let relative_path = plan.relative_display(path);
    let icon_key = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(crate::app::file_icons::file_icon_key_for_name)
        .unwrap_or("default_file");
    ProjectSearchGrepResult::Complete(Some(ProjectSearchFile {
        path: path.to_path_buf(),
        relative_path,
        icon_key,
        matches: sink.matches,
    }))
}

struct ProjectSearchGrepSink<'a> {
    needle: &'a [u8],
    case_sensitive: bool,
    profile: &'a SearchProfile,
    capped_flag: &'a AtomicBool,
    match_limit: usize,
    needs_decoded_fallback: bool,
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
        let line_result = push_project_search_line_matches(
            mat.bytes(),
            mat.absolute_byte_offset() as usize,
            line_number.saturating_sub(1).min(u32::MAX as u64) as u32,
            self.needle,
            self.case_sensitive,
            self.match_limit,
            &mut self.matches,
        );
        self.profile
            .prep_ms
            .fetch_add(elapsed_ms_u64(prep_started), Ordering::Relaxed);
        match line_result {
            ProjectSearchGrepLineResult::Continue => Ok(true),
            ProjectSearchGrepLineResult::Stop => Ok(false),
            ProjectSearchGrepLineResult::NeedsDecodedFallback => {
                self.needs_decoded_fallback = true;
                Ok(false)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectSearchGrepLineResult {
    Continue,
    Stop,
    NeedsDecodedFallback,
}

fn push_project_search_line_matches(
    line_bytes_with_term: &[u8],
    line_start_abs: usize,
    line_number: u32,
    needle: &[u8],
    case_sensitive: bool,
    match_limit: usize,
    matches: &mut Vec<ProjectSearchMatch>,
) -> ProjectSearchGrepLineResult {
    if needle.is_empty() {
        return ProjectSearchGrepLineResult::Continue;
    }
    let line_bytes = trim_project_search_line_term(line_bytes_with_term);
    let Ok(line) = std::str::from_utf8(line_bytes) else {
        return ProjectSearchGrepLineResult::NeedsDecodedFallback;
    };
    let mut keep_going = true;
    collect_line_match_ranges(line.as_bytes(), needle, case_sensitive, |start, end| {
        if matches.len() >= match_limit {
            keep_going = false;
            return false;
        }
        push_line_match(line, line_start_abs, line_number, start, end, matches);
        matches.len() < match_limit
    });
    if keep_going && matches.len() < match_limit {
        ProjectSearchGrepLineResult::Continue
    } else {
        ProjectSearchGrepLineResult::Stop
    }
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
        start_col: utf16_units_between(line, 0, start),
        end_line: line_number,
        end_col: utf16_units_between(line, 0, end),
        preview: String::new(),
        preview_match_start: 0,
        preview_match_end: 0,
        preview_ready: false,
        extra_lines: 0,
    });
}

#[cfg(test)]
mod tests {
    use super::super::{
        ProjectSearchBackend, ProjectSearchRequest, ProjectSearchWorkerResult,
        project_search_backend, project_search_text, run_project_search,
    };
    use super::*;
    use std::borrow::Cow;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rriter_project_search_grep_{name}_{nanos}"))
    }

    fn encode_legacy_source(text: &str, encoding: crate::platform::LegacyEncoding) -> Vec<u8> {
        crate::platform::encode_text(
            text,
            crate::platform::TextFileFormat {
                encoding: crate::platform::TextEncoding::Legacy(encoding),
                line_ending: crate::platform::LineEnding::Lf,
            },
        )
        .unwrap()
    }

    fn single_match(result: &ProjectSearchWorkerResult) -> &ProjectSearchMatch {
        assert_eq!(result.error, None);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.total_matches, 1);
        result.files[0].matches.first().unwrap()
    }

    #[test]
    fn project_search_text_falls_back_to_legacy_for_lf_only_invalid_utf8() {
        let source = "# Привет, мир\ndef main():\n    строка = \"тест\"\n";
        let bytes = encode_legacy_source(source, crate::platform::LegacyEncoding::Windows1251);
        assert!(std::str::from_utf8(&bytes).is_err());
        assert!(!bytes.contains(&b'\r'));

        let text = project_search_text(&bytes).unwrap();
        assert!(matches!(text, Cow::Owned(_)));
        assert_eq!(text.as_ref(), source);
    }

    #[test]
    fn project_search_text_keeps_plain_utf8_lf_zero_copy() {
        let source = b"let needle = 1;\n";
        let text = project_search_text(source).unwrap();
        assert!(matches!(text, Cow::Borrowed(_)));
        assert_eq!(text.as_ref(), std::str::from_utf8(source).unwrap());
    }

    #[test]
    fn project_search_backend_keeps_grep_only_for_ascii_single_line_queries() {
        assert_eq!(project_search_backend("needle"), ProjectSearchBackend::Grep);
        assert_eq!(project_search_backend("Привет"), ProjectSearchBackend::Decoded);
        assert_eq!(project_search_backend("привет"), ProjectSearchBackend::Decoded);
        assert_eq!(
            project_search_backend("needle\nsecond"),
            ProjectSearchBackend::Decoded
        );
    }

    #[test]
    fn project_search_finds_case_sensitive_cyrillic_in_windows1251() {
        let root = temp_workspace("cp1251_case_sensitive");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("main.py"),
            encode_legacy_source(
                "# Привет\nvalue = 1\n",
                crate::platform::LegacyEncoding::Windows1251,
            ),
        )
        .unwrap();

        let result = run_project_search(ProjectSearchRequest {
            generation: 21,
            query: "Привет".to_string(),
            include: ".".to_string(),
            exclude: String::new(),
            case_sensitive: true,
            workspaces: vec![root.clone()],
            ignore_patterns: Vec::new(),
        });

        let mat = single_match(&result);
        assert_eq!((mat.start_line, mat.start_col, mat.end_col), (0, 2, 8));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn project_search_finds_case_insensitive_cyrillic_in_windows1251() {
        let root = temp_workspace("cp1251_case_insensitive");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("main.py"),
            encode_legacy_source(
                "# Привет, мир\ndef main():\n    строка = \"тест\"\n",
                crate::platform::LegacyEncoding::Windows1251,
            ),
        )
        .unwrap();

        let result = run_project_search(ProjectSearchRequest {
            generation: 22,
            query: "привет".to_string(),
            include: ".".to_string(),
            exclude: String::new(),
            case_sensitive: false,
            workspaces: vec![root.clone()],
            ignore_patterns: Vec::new(),
        });

        let mat = single_match(&result);
        assert_eq!((mat.start_line, mat.start_col, mat.end_col), (0, 2, 8));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn project_search_ascii_query_falls_back_for_mixed_windows1251_line() {
        let root = temp_workspace("cp1251_mixed_line");
        std::fs::create_dir_all(&root).unwrap();
        let source = "# Привет, мир\nlet привет = \"needle\";\n";
        std::fs::write(
            root.join("main.rs"),
            encode_legacy_source(source, crate::platform::LegacyEncoding::Windows1251),
        )
        .unwrap();

        let result = run_project_search(ProjectSearchRequest {
            generation: 23,
            query: "needle".to_string(),
            include: ".".to_string(),
            exclude: String::new(),
            case_sensitive: true,
            workspaces: vec![root.clone()],
            ignore_patterns: Vec::new(),
        });

        let mat = single_match(&result);
        let decoded_start = source.find("needle").unwrap();
        assert_eq!(mat.byte_start, decoded_start);
        assert_eq!(mat.byte_end, decoded_start + "needle".len());
        assert_eq!((mat.start_line, mat.start_col, mat.end_col), (1, 14, 20));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn project_search_ascii_query_falls_back_for_mixed_iso8859_5_line() {
        let root = temp_workspace("iso8859_5_mixed_line");
        std::fs::create_dir_all(&root).unwrap();
        let source = "// привет needle\n";
        std::fs::write(
            root.join("main.rs"),
            encode_legacy_source(source, crate::platform::LegacyEncoding::Iso8859_5),
        )
        .unwrap();

        let result = run_project_search(ProjectSearchRequest {
            generation: 24,
            query: "needle".to_string(),
            include: ".".to_string(),
            exclude: String::new(),
            case_sensitive: true,
            workspaces: vec![root.clone()],
            ignore_patterns: Vec::new(),
        });

        let mat = single_match(&result);
        let decoded_start = source.find("needle").unwrap();
        assert_eq!(mat.byte_start, decoded_start);
        assert_eq!((mat.start_line, mat.start_col, mat.end_col), (0, 10, 16));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn grep_legacy_fallback_does_not_commit_file_local_matches_to_global_caps() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rriter_project_search_grep_{nanos}"));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("main.rs");
        let bytes = crate::platform::encode_text(
            "needle\nlet привет = \"needle\";\n",
            crate::platform::TextFileFormat {
                encoding: crate::platform::TextEncoding::Legacy(
                    crate::platform::LegacyEncoding::Windows1251,
                ),
                line_ending: crate::platform::LineEnding::Lf,
            },
        )
        .unwrap();
        std::fs::write(&path, bytes).unwrap();
        let plan = SearchPatternPlan::new(&[root.clone()], ".", "").unwrap();
        let matcher = grep_regex::RegexMatcherBuilder::new()
            .build("needle")
            .unwrap();
        let mut searcher = grep_searcher::SearcherBuilder::new()
            .binary_detection(grep_searcher::BinaryDetection::quit(b'\0'))
            .line_number(true)
            .build();
        let caps = Mutex::new(SearchCaps::default());
        let profile = SearchProfile::default();
        let capped = AtomicBool::new(false);

        let result = search_project_file_grep(
            &path,
            &plan,
            b"needle",
            &matcher,
            &mut searcher,
            true,
            &caps,
            &profile,
            &capped,
        );

        assert!(matches!(result, ProjectSearchGrepResult::NeedsDecodedFallback));
        let caps = crate::platform::lock_recover(&caps);
        assert_eq!(caps.matches, 0);
        assert_eq!(caps.files, 0);
        assert!(!caps.capped);
        assert!(!capped.load(Ordering::Relaxed));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn grep_match_tracks_utf16_columns_and_crlf_disk_offsets() {
        let mut matches = Vec::new();
        push_line_match("a😀needle", 9, 4, 5, 11, &mut matches);
        let mat = &matches[0];
        assert_eq!(mat.byte_start, 14);
        assert_eq!(mat.byte_end, 20);
        assert_eq!(mat.start_line, 4);
        assert_eq!(mat.start_col, 3);
        assert_eq!(mat.end_col, 9);
    }

    #[test]
    fn trim_line_term_handles_lf_crlf_and_unterminated_lines() {
        assert_eq!(trim_project_search_line_term(b"abc\n"), b"abc");
        assert_eq!(trim_project_search_line_term(b"abc\r\n"), b"abc");
        assert_eq!(trim_project_search_line_term(b"abc"), b"abc");
    }
}
