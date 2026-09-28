pub fn start_project_search_worker(
    request: ProjectSearchRequest,
) -> Receiver<ProjectSearchWorkerMessage> {
    start_project_search_worker_cancellable(request).0
}

pub fn start_project_search_worker_cancellable(
    request: ProjectSearchRequest,
) -> (Receiver<ProjectSearchWorkerMessage>, Arc<AtomicBool>) {
    let (tx, rx) = channel();
    let generation = request.generation;
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let worker_tx = tx.clone();
    if let Err(err) = crate::platform::spawn_named("rriter-project-search", move || {
        stream_project_search(request, worker_tx, worker_cancel);
    }) {
        let _ = tx.send(ProjectSearchWorkerMessage::Done {
            generation,
            elapsed_ms: 0,
            capped: false,
            error: Some(format!("не удалось запустить поиск по проекту: {err}")),
        });
    }
    (rx, cancel)
}

#[cfg(test)]
pub fn run_project_search(request: ProjectSearchRequest) -> ProjectSearchWorkerResult {
    let (tx, rx) = channel();
    stream_project_search(request, tx, Arc::new(AtomicBool::new(false)));
    let mut result = ProjectSearchWorkerResult {
        files: Vec::new(),
        total_matches: 0,
        elapsed_ms: 0,
        capped: false,
        error: None,
    };
    while let Ok(message) = rx.recv() {
        match message {
            ProjectSearchWorkerMessage::File {
                file, elapsed_ms, ..
            } => {
                result.total_matches = result.total_matches.saturating_add(file.matches.len());
                result.files.push(file);
                result.elapsed_ms = elapsed_ms;
            }
            ProjectSearchWorkerMessage::Done {
                elapsed_ms,
                capped,
                error,
                ..
            } => {
                result.elapsed_ms = elapsed_ms;
                result.capped = capped;
                result.error = error;
                break;
            }
        }
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectSearchBackend {
    Grep,
    Decoded,
}

fn project_search_backend(query: &str) -> ProjectSearchBackend {
    if query.is_ascii() && !query.as_bytes().contains(&b'\n') && !query.as_bytes().contains(&b'\r')
    {
        ProjectSearchBackend::Grep
    } else {
        ProjectSearchBackend::Decoded
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
        fn temp_workspace(name: &str) -> PathBuf {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            std::env::temp_dir().join(format!("rriter_project_search_{name}_{nanos}"))
        }

        #[test]
        fn project_search_literal_case_glob_exclude_and_multiline_preview() {
            let root = temp_workspace("run");
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::create_dir_all(root.join("ignored")).unwrap();
            std::fs::write(root.join("src/main.rs"), "Hello\nWorld\nhello\n").unwrap();
            std::fs::write(root.join("ignored/main.rs"), "hello\n").unwrap();

            let result = run_project_search(ProjectSearchRequest {
                generation: 1,
                query: "hello".to_string(),
                include: "src/**/*.rs, ignored/**/*.rs".to_string(),
                exclude: "ignored".to_string(),
                case_sensitive: false,
                workspaces: vec![root.clone()],
                ignore_patterns: Vec::new(),
            });

            assert_eq!(result.error, None);
            assert_eq!(result.files.len(), 1);
            assert_eq!(result.total_matches, 2);
            let workspace_name = root.file_name().and_then(|name| name.to_str()).unwrap();
            assert_eq!(
                result.files[0].relative_path,
                format!("{workspace_name}/src/main.rs")
            );

            let multi = run_project_search(ProjectSearchRequest {
                generation: 2,
                query: "Hello\nWorld".to_string(),
                include: "./src".to_string(),
                exclude: String::new(),
                case_sensitive: true,
                workspaces: vec![root.clone()],
                ignore_patterns: Vec::new(),
            });
            assert_eq!(multi.total_matches, 1);
            assert_eq!(multi.files[0].matches[0].extra_lines, 1);
            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn project_search_preserves_positions_for_utf_and_legacy_text_files() {
            let root = temp_workspace("text_formats");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("crlf.txt"), "zero\r\na😀needle\r\n").unwrap();
            std::fs::write(
                root.join("utf8_bom.txt"),
                crate::platform::encode_text(
                    "zero\nneedle",
                    crate::platform::TextFileFormat {
                        encoding: crate::platform::TextEncoding::Utf8Bom,
                        line_ending: crate::platform::LineEnding::Lf,
                    },
                )
                .unwrap(),
            )
            .unwrap();
            std::fs::write(
                root.join("utf16.txt"),
                crate::platform::encode_text(
                    "zero\n😀needle",
                    crate::platform::TextFileFormat {
                        encoding: crate::platform::TextEncoding::Utf16Le,
                        line_ending: crate::platform::LineEnding::CrLf,
                    },
                )
                .unwrap(),
            )
            .unwrap();
            std::fs::write(
                root.join("windows1251.txt"),
                crate::platform::encode_text(
                    "Привет\nneedle",
                    crate::platform::TextFileFormat {
                        encoding: crate::platform::TextEncoding::Legacy(
                            crate::platform::LegacyEncoding::Windows1251,
                        ),
                        line_ending: crate::platform::LineEnding::Lf,
                    },
                )
                .unwrap(),
            )
            .unwrap();

            let result = run_project_search(ProjectSearchRequest {
                generation: 7,
                query: "needle".to_string(),
                include: ".".to_string(),
                exclude: String::new(),
                case_sensitive: true,
                workspaces: vec![root.clone()],
                ignore_patterns: Vec::new(),
            });

            assert_eq!(result.error, None);
            assert_eq!(result.total_matches, 4);
            let by_name = |name: &str| {
                result
                    .files
                    .iter()
                    .find(|file| file.path.file_name().and_then(|part| part.to_str()) == Some(name))
                    .and_then(|file| file.matches.first())
                    .unwrap()
            };
            let crlf = by_name("crlf.txt");
            assert_eq!((crlf.start_line, crlf.start_col, crlf.end_col), (1, 3, 9));
            let utf8_bom = by_name("utf8_bom.txt");
            assert_eq!(
                (utf8_bom.start_line, utf8_bom.start_col, utf8_bom.end_col),
                (1, 0, 6)
            );
            let utf16 = by_name("utf16.txt");
            assert_eq!(
                (utf16.start_line, utf16.start_col, utf16.end_col),
                (1, 2, 8)
            );
            let windows1251 = by_name("windows1251.txt");
            assert_eq!(
                (
                    windows1251.start_line,
                    windows1251.start_col,
                    windows1251.end_col
                ),
                (1, 0, 6)
            );
            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn project_search_respects_gitignore_and_settings_ignore() {
            let root = temp_workspace("ignore");
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::create_dir_all(root.join("ignored_git")).unwrap();
            std::fs::create_dir_all(root.join("ignored_ignore")).unwrap();
            std::fs::create_dir_all(root.join("settings_ignored")).unwrap();
            std::fs::create_dir_all(root.join(".git")).unwrap();
            std::fs::write(root.join("src/main.rs"), "needle\n").unwrap();
            std::fs::write(root.join("src/debug.log"), "needle\n").unwrap();
            std::fs::write(root.join("ignored_git/a.rs"), "needle\n").unwrap();
            std::fs::write(root.join("ignored_ignore/a.rs"), "needle\n").unwrap();
            std::fs::write(root.join("settings_ignored/a.rs"), "needle\n").unwrap();
            std::fs::write(root.join(".git/config"), "needle\n").unwrap();
            std::fs::write(root.join(".gitignore"), "ignored_git\n").unwrap();
            std::fs::write(root.join(".ignore"), "ignored_ignore\n").unwrap();

            let result = run_project_search(ProjectSearchRequest {
                generation: 3,
                query: "needle".to_string(),
                include: ".".to_string(),
                exclude: String::new(),
                case_sensitive: true,
                workspaces: vec![root.clone()],
                ignore_patterns: vec!["settings_ignored".to_string(), "*.log".to_string()],
            });

            assert_eq!(result.error, None);
            assert_eq!(result.total_matches, 1);
            let workspace_name = root.file_name().and_then(|name| name.to_str()).unwrap();
            assert_eq!(
                result.files[0].relative_path,
                format!("{workspace_name}/src/main.rs")
            );
            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn project_search_thread_count_never_oversubscribes_small_cpus() {
            assert_eq!(project_search_threads_for_available(1), 1);
            assert_eq!(project_search_threads_for_available(2), 2);
            assert_eq!(project_search_threads_for_available(4), 4);
            assert_eq!(
                project_search_threads_for_available(16),
                PROJECT_SEARCH_MAX_THREADS
            );
        }
}

#[derive(Default)]
struct SearchCaps {
    matches: usize,
    files: usize,
    capped: bool,
}

#[derive(Default)]
struct SearchProfile {
    files_seen: AtomicU64,
    files_read: AtomicU64,
    bytes_read: AtomicU64,
    matches: AtomicU64,
    read_ms: AtomicU64,
    scan_ms: AtomicU64,
    prep_ms: AtomicU64,
}

impl SearchProfile {
    fn log(&self, query: &str, backend: &str, elapsed_ms: u128, capped: bool) {
        #[cfg(test)]
        {
            let _ = (self, query, backend, elapsed_ms, capped);
            return;
        }
        #[cfg(not(test))]
        {
            let read_ms = self.read_ms.load(Ordering::Relaxed);
            let scan_ms = self.scan_ms.load(Ordering::Relaxed);
            let prep_ms = self.prep_ms.load(Ordering::Relaxed);
            eprintln!(
                "[PROJECT SEARCH] query={:?} backend={} threads={} total={}ms files={}/{} matches={} bytes={}KiB read_thread={}ms scan_thread={}ms prep_thread={}ms capped={}",
                query,
                backend,
                project_search_thread_count(),
                elapsed_ms,
                self.files_read.load(Ordering::Relaxed),
                self.files_seen.load(Ordering::Relaxed),
                self.matches.load(Ordering::Relaxed),
                self.bytes_read.load(Ordering::Relaxed) / 1024,
                read_ms,
                scan_ms,
                prep_ms,
                capped
            );
        }
    }
}

fn elapsed_ms_u64(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn push_project_search_ranges(
    text: &str,
    ranges: &[(usize, usize)],
    matches: &mut Vec<ProjectSearchMatch>,
    profile: &SearchProfile,
) {
    let prep_started = Instant::now();
    profile
        .matches
        .fetch_add(ranges.len() as u64, Ordering::Relaxed);
    let mut cursor = ProjectSearchLineCursor::default();
    for &(start, end) in ranges {
        push_match(text, start, end, &mut cursor, matches);
    }
    profile
        .prep_ms
        .fetch_add(elapsed_ms_u64(prep_started), Ordering::Relaxed);
}

fn stream_project_search(
    request: ProjectSearchRequest,
    tx: std::sync::mpsc::Sender<ProjectSearchWorkerMessage>,
    cancel: Arc<AtomicBool>,
) {
    let started = Instant::now();
    let generation = request.generation;
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    if request.query.is_empty() {
        let _ = tx.send(ProjectSearchWorkerMessage::Done {
            generation,
            elapsed_ms: started.elapsed().as_millis(),
            capped: false,
            error: None,
        });
        return;
    }

    let plan = match SearchPatternPlan::new(&request.workspaces, &request.include, &request.exclude)
    {
        Ok(plan) => plan,
        Err(error) => {
            let _ = tx.send(ProjectSearchWorkerMessage::Done {
                generation,
                elapsed_ms: started.elapsed().as_millis(),
                capped: false,
                error: Some(error),
            });
            return;
        }
    };
    if plan.workspaces.is_empty() {
        let _ = tx.send(ProjectSearchWorkerMessage::Done {
            generation,
            elapsed_ms: started.elapsed().as_millis(),
            capped: false,
            error: Some("Нет workspace".to_string()),
        });
        return;
    }

    let query = request.query;
    let needle = Arc::new(query.as_bytes().to_vec());
    let unicode_case_fallback = !request.case_sensitive && !needle.is_ascii();
    let search_backend = project_search_backend(&query);
    let grep_pattern = (search_backend == ProjectSearchBackend::Grep)
        .then(|| Arc::<str>::from(regex::escape(&query)));
    let backend = if grep_pattern.is_some() {
        "grep"
    } else if unicode_case_fallback {
        "unicode"
    } else {
        "buffer"
    };
    let settings_ignore = Arc::new(SearchIgnoreMatcher::new(request.ignore_patterns));
    let plan = Arc::new(plan);
    let caps = Arc::new(Mutex::new(SearchCaps::default()));
    let profile = Arc::new(SearchProfile::default());
    let capped_flag = Arc::new(AtomicBool::new(false));
    let roots = plan
        .walk_roots()
        .into_iter()
        .filter(|root| root.is_dir())
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    run_project_search_roots(
        roots,
        generation,
        started,
        Arc::clone(&plan),
        Arc::clone(&settings_ignore),
        Arc::clone(&caps),
        Arc::clone(&profile),
        Arc::clone(&capped_flag),
        tx.clone(),
        Arc::clone(&needle),
        grep_pattern,
        request.case_sensitive,
        unicode_case_fallback,
        Arc::clone(&cancel),
    );
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let capped = caps.lock().map(|caps| caps.capped).unwrap_or(true);
    let elapsed_ms = started.elapsed().as_millis();
    profile.log(&query, backend, elapsed_ms, capped);
    let _ = tx.send(ProjectSearchWorkerMessage::Done {
        generation,
        elapsed_ms,
        capped,
        error: None,
    });
}

fn run_project_search_roots(
    roots: Vec<PathBuf>,
    generation: u64,
    started: Instant,
    plan: Arc<SearchPatternPlan>,
    settings_ignore: Arc<SearchIgnoreMatcher>,
    caps: Arc<Mutex<SearchCaps>>,
    profile: Arc<SearchProfile>,
    capped_flag: Arc<AtomicBool>,
    tx: Sender<ProjectSearchWorkerMessage>,
    needle: Arc<Vec<u8>>,
    grep_pattern: Option<Arc<str>>,
    case_sensitive: bool,
    unicode_case_fallback: bool,
    cancel: Arc<AtomicBool>,
) {
    let mut roots = roots.into_iter();
    let Some(first_root) = roots.next() else {
        return;
    };
    let settings_ignore_for_walk = Arc::clone(&settings_ignore);
    let settings_workspaces = plan.workspaces.clone();
    let mut builder = ignore::WalkBuilder::new(first_root);
    for root in roots {
        builder.add(root);
    }
    builder
        .hidden(false)
        .ignore(true)
        .parents(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .threads(project_search_thread_count())
        .filter_entry(move |entry| {
            !settings_ignore_for_walk.matches_path(entry.path(), &settings_workspaces)
        });
    let visitor = move || {
        let plan = Arc::clone(&plan);
        let caps = Arc::clone(&caps);
        let profile = Arc::clone(&profile);
        let capped_flag = Arc::clone(&capped_flag);
        let cancel = Arc::clone(&cancel);
        let tx = tx.clone();
        let needle = Arc::clone(&needle);
        let grep_pattern = grep_pattern.as_ref().map(Arc::clone);
        let case_finder = case_sensitive.then(|| Finder::new(needle.as_slice()).into_owned());
        let mut file_buf = Vec::new();
        let grep_matcher = grep_pattern.as_ref().and_then(|pattern| {
            RegexMatcherBuilder::new()
                .case_insensitive(!case_sensitive)
                .build(pattern)
                .ok()
        });
        let mut grep_searcher = grep_pattern.as_ref().map(|_| {
            SearcherBuilder::new()
                .binary_detection(BinaryDetection::quit(b'\0'))
                .line_number(true)
                .build()
        });
        Box::new(move |entry: Result<ignore::DirEntry, ignore::Error>| {
            if capped_flag.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                return ignore::WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return ignore::WalkState::Continue;
            };
            let path = entry.path();
            if !entry.file_type().is_some_and(|ty| ty.is_file()) || !plan.is_file_allowed(path) {
                return ignore::WalkState::Continue;
            }
            let file = if let (Some(matcher), Some(searcher)) =
                (grep_matcher.as_ref(), grep_searcher.as_mut())
            {
                match project_search_grep::search_project_file_grep(
                    path,
                    &plan,
                    needle.as_slice(),
                    matcher,
                    searcher,
                    case_sensitive,
                    &caps,
                    &profile,
                    &capped_flag,
                ) {
                    project_search_grep::ProjectSearchGrepResult::Complete(file) => file,
                    project_search_grep::ProjectSearchGrepResult::NeedsDecodedFallback => {
                        let file = search_project_file(
                            path,
                            &plan,
                            needle.as_slice(),
                            case_finder.as_ref(),
                            case_sensitive,
                            unicode_case_fallback,
                            &mut file_buf,
                            &caps,
                            &profile,
                            &capped_flag,
                        );
                        trim_project_search_buffer(&mut file_buf);
                        file
                    }
                }
            } else {
                let file = search_project_file(
                    path,
                    &plan,
                    needle.as_slice(),
                    case_finder.as_ref(),
                    case_sensitive,
                    unicode_case_fallback,
                    &mut file_buf,
                    &caps,
                    &profile,
                    &capped_flag,
                );
                trim_project_search_buffer(&mut file_buf);
                file
            };
            let Some(file) = file else {
                return ignore::WalkState::Continue;
            };
            let elapsed_ms = started.elapsed().as_millis();
            let _ = tx.send(ProjectSearchWorkerMessage::File {
                generation,
                file,
                elapsed_ms,
            });
            ignore::WalkState::Continue
        })
            as Box<dyn FnMut(Result<ignore::DirEntry, ignore::Error>) -> ignore::WalkState + Send>
    };
    builder.build_parallel().run(visitor);
}

fn trim_project_search_buffer(buf: &mut Vec<u8>) {
    if buf.capacity() > PROJECT_SEARCH_BUFFER_KEEP_BYTES {
        buf.clear();
        buf.shrink_to(PROJECT_SEARCH_BUFFER_KEEP_BYTES);
    }
}

fn is_definitely_binary_project_search_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "ico"
            | "bmp"
            | "tif"
            | "tiff"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "eot"
            | "pdf"
            | "zip"
            | "gz"
            | "tgz"
            | "xz"
            | "bz2"
            | "zst"
            | "7z"
            | "rar"
            | "tar"
            | "pack"
            | "idx"
            | "so"
            | "dylib"
            | "dll"
            | "a"
            | "rlib"
            | "rmeta"
            | "class"
            | "pyc"
            | "pyo"
            | "o"
            | "obj"
            | "wasm"
    )
}

fn search_project_file(
    path: &Path,
    plan: &SearchPatternPlan,
    needle: &[u8],
    case_finder: Option<&Finder<'static>>,
    case_sensitive: bool,
    unicode_case_fallback: bool,
    buf: &mut Vec<u8>,
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
    let read_started = Instant::now();
    let Ok(mut file) = std::fs::File::open(path) else {
        return None;
    };
    buf.clear();
    if (&mut file)
        .take(PROJECT_SEARCH_FILE_CAP_BYTES.saturating_add(1))
        .read_to_end(buf)
        .is_err()
        || buf.len() as u64 > PROJECT_SEARCH_FILE_CAP_BYTES
    {
        return None;
    }
    profile.files_read.fetch_add(1, Ordering::Relaxed);
    profile
        .bytes_read
        .fetch_add(buf.len() as u64, Ordering::Relaxed);
    profile
        .read_ms
        .fetch_add(elapsed_ms_u64(read_started), Ordering::Relaxed);
    let mut matches = Vec::new();
    let room = {
        let caps = crate::platform::lock_recover(caps);
        if caps.capped
            || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP
            || caps.matches >= PROJECT_SEARCH_MATCH_CAP
        {
            capped_flag.store(true, Ordering::Relaxed);
            return None;
        }
        PROJECT_SEARCH_MATCH_CAP.saturating_sub(caps.matches)
    };
    let text = project_search_text(buf)?;
    let mut ranges = Vec::new();
    let scan_started = Instant::now();
    if unicode_case_fallback {
        collect_unicode_case_insensitive_matches(&text, needle, |start, end| {
            ranges.push((start, end));
            ranges.len() < room
        });
    } else if case_sensitive {
        let finder = case_finder?;
        for start in finder.find_iter(text.as_bytes()) {
            ranges.push((start, start + needle.len()));
            if ranges.len() >= room {
                break;
            }
        }
    } else {
        collect_ascii_case_insensitive_matches(text.as_bytes(), needle, room, &mut ranges);
    }
    profile
        .scan_ms
        .fetch_add(elapsed_ms_u64(scan_started), Ordering::Relaxed);
    if ranges.is_empty() {
        return None;
    }
    push_project_search_ranges(&text, &ranges, &mut matches, profile);
    if matches.is_empty() {
        return None;
    }
    if !commit_project_search_file_matches(&mut matches, caps, capped_flag) {
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
        matches,
    })
}

fn commit_project_search_file_matches(
    matches: &mut Vec<ProjectSearchMatch>,
    caps: &Mutex<SearchCaps>,
    capped_flag: &AtomicBool,
) -> bool {
    if matches.is_empty() {
        return false;
    }
    let mut caps = crate::platform::lock_recover(caps);
    if caps.capped
        || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP
        || caps.matches >= PROJECT_SEARCH_MATCH_CAP
    {
        caps.capped = true;
        capped_flag.store(true, Ordering::Relaxed);
        return false;
    }
    let room = PROJECT_SEARCH_MATCH_CAP - caps.matches;
    if matches.len() > room {
        matches.truncate(room);
        caps.capped = true;
    }
    if matches.is_empty() {
        capped_flag.store(caps.capped, Ordering::Relaxed);
        return false;
    }
    caps.matches += matches.len();
    caps.files += 1;
    if caps.matches >= PROJECT_SEARCH_MATCH_CAP || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP {
        caps.capped = true;
    }
    if caps.capped {
        capped_flag.store(true, Ordering::Relaxed);
    }
    true
}

fn project_search_text(buf: &[u8]) -> Option<Cow<'_, str>> {
    let has_text_bom = buf.starts_with(&[0xef, 0xbb, 0xbf])
        || buf.starts_with(&[0xff, 0xfe])
        || buf.starts_with(&[0xfe, 0xff]);
    if !has_text_bom && memchr::memchr(b'\0', buf).is_some() {
        return None;
    }
    if !has_text_bom && memchr::memchr(b'\r', buf).is_none() {
        if let Ok(text) = std::str::from_utf8(buf) {
            return Some(Cow::Borrowed(text));
        }
    }
    platform::decode_text_bytes(buf)
        .ok()
        .map(|decoded| Cow::Owned(decoded.text))
}

fn collect_ascii_case_insensitive_matches(
    haystack: &[u8],
    needle: &[u8],
    room: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    if needle.is_empty() || room == 0 || haystack.len() < needle.len() {
        return;
    }
    let first = needle[0];
    let lower = first.to_ascii_lowercase();
    let upper = first.to_ascii_uppercase();
    if lower == upper {
        for start in memchr::memchr_iter(first, haystack) {
            push_ascii_case_match(haystack, needle, start, room, ranges);
            if ranges.len() >= room {
                break;
            }
        }
    } else {
        for start in memchr::memchr2_iter(lower, upper, haystack) {
            push_ascii_case_match(haystack, needle, start, room, ranges);
            if ranges.len() >= room {
                break;
            }
        }
    }
}

fn push_ascii_case_match(
    haystack: &[u8],
    needle: &[u8],
    start: usize,
    room: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    let end = start + needle.len();
    if end <= haystack.len()
        && haystack[start..end].eq_ignore_ascii_case(needle)
        && ranges.len() < room
    {
        ranges.push((start, end));
    }
}

fn collect_unicode_case_insensitive_matches(
    text: &str,
    needle: &[u8],
    mut emit: impl FnMut(usize, usize) -> bool,
) {
    let Ok(query) = std::str::from_utf8(needle) else {
        return;
    };
    let query = query.to_lowercase();
    if query.is_empty() {
        return;
    }
    let mut lower = String::with_capacity(text.len());
    let mut lower_byte_spans = Vec::with_capacity(text.len());
    for (idx, ch) in text.char_indices() {
        let source_end = idx + ch.len_utf8();
        for lowered in ch.to_lowercase() {
            let mut buf = [0u8; 4];
            let encoded = lowered.encode_utf8(&mut buf);
            for _ in 0..encoded.len() {
                lower_byte_spans.push((idx, source_end));
            }
            lower.push(lowered);
        }
    }
    for (idx, found) in lower.match_indices(&query) {
        let end = idx + found.len();
        let start_orig = lower_byte_spans
            .get(idx)
            .map(|(start, _)| *start)
            .unwrap_or(text.len());
        let end_orig = end
            .checked_sub(1)
            .and_then(|last| lower_byte_spans.get(last))
            .map(|(_, source_end)| *source_end)
            .unwrap_or(start_orig);
        if end_orig >= start_orig && !emit(start_orig, end_orig) {
            break;
        }
    }
}
