use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkState;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const QUERY: &str = "as";
const DEFAULT_ITERS: usize = 10;
const MAX_THREADS: usize = 8;
const WORKSPACES: &[&str] = &[
    "/home/reyan/projects/car-wash-api",
    "/home/reyan/projects/construction-api",
    "/home/reyan/projects/rriter",
    "/home/reyan/repos/git",
    "/home/reyan/repos/gpui-calculator",
];

const DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    "__pycache__",
    ".idea",
    ".vscode",
    ".DS_Store",
    "node_modules",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".gradle",
    ".dart_tool",
    ".flutter-plugins",
    ".flutter-plugins-dependencies",
    "*.pyc",
    "*.pyo",
    "*.class",
    "*.o",
    "*.obj",
    ".cache",
    ".env",
    "venv",
    ".venv",
    "Thumbs.db",
    "*.swp",
    "*.swo",
    ".git",
];

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let iters = args
        .first()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_ITERS)
        .max(1);
    let query = args.get(1).map(String::as_str).unwrap_or(QUERY);
    let roots = workspace_roots();
    if roots.is_empty() {
        eprintln!("[GREP_SEARCHER_BENCH] no workspace roots found");
        return;
    }
    for run in 1..=iters {
        let result = run_grep_searcher(&roots, query);
        println!(
            "[GREP_SEARCHER_BENCH] run={} total={}ms fused_walk_search={}ms files={} result_files={} matches={} errors={} query={:?} threads={}",
            run,
            result.total_ms,
            result.fused_ms,
            result.files_seen,
            result.files_with_matches,
            result.matches,
            result.errors,
            query,
            search_threads(),
        );
    }
}

fn workspace_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for workspace in WORKSPACES {
        let path = PathBuf::from(workspace);
        if path.is_dir() {
            roots.push(path.canonicalize().unwrap_or(path));
        }
    }
    roots
}

#[derive(Default)]
struct BenchCounters {
    files_seen: AtomicU64,
    files_with_matches: AtomicU64,
    matches: AtomicU64,
    errors: AtomicU64,
}

struct BenchResult {
    total_ms: u128,
    fused_ms: u128,
    files_seen: u64,
    files_with_matches: u64,
    matches: u64,
    errors: u64,
}

fn run_grep_searcher(roots: &[PathBuf], query: &str) -> BenchResult {
    let total_started = Instant::now();
    let roots = roots.to_vec();
    let Some((first, rest)) = roots.split_first() else {
        return BenchResult {
            total_ms: 0,
            fused_ms: 0,
            files_seen: 0,
            files_with_matches: 0,
            matches: 0,
            errors: 0,
        };
    };
    let counters = Arc::new(BenchCounters::default());
    let query_bytes = Arc::new(query.as_bytes().to_vec());
    let regex_pattern = Arc::<str>::from(regex::escape(query));
    let counters_for_walk = Arc::clone(&counters);
    let mut builder = ignore::WalkBuilder::new(first);
    for root in rest {
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
        .threads(search_threads())
        .filter_entry(|entry| !is_default_ignored_path(entry.path()));
    let visitor = move || {
        let counters = Arc::clone(&counters_for_walk);
        let query_bytes = Arc::clone(&query_bytes);
        let regex_pattern = Arc::clone(&regex_pattern);
        let matcher = match RegexMatcherBuilder::new()
            .case_insensitive(true)
            .build(&regex_pattern)
        {
            Ok(matcher) => matcher,
            Err(_) => {
                counters.errors.fetch_add(1, Ordering::Relaxed);
                return Box::new(|_| WalkState::Quit)
                    as Box<dyn FnMut(Result<ignore::DirEntry, ignore::Error>) -> WalkState + Send>;
            }
        };
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\0'))
            .line_number(false)
            .build();
        Box::new(move |entry: Result<ignore::DirEntry, ignore::Error>| {
            let Ok(entry) = entry else {
                counters.errors.fetch_add(1, Ordering::Relaxed);
                return WalkState::Continue;
            };
            let path = entry.path();
            if !entry.file_type().is_some_and(|ty| ty.is_file()) || is_binary_by_extension(path) {
                return WalkState::Continue;
            }
            counters.files_seen.fetch_add(1, Ordering::Relaxed);
            let mut sink = OccurrenceSink::new(Arc::clone(&query_bytes));
            if searcher.search_path(&matcher, path, &mut sink).is_err() {
                counters.errors.fetch_add(1, Ordering::Relaxed);
                return WalkState::Continue;
            }
            if sink.matches > 0 {
                counters
                    .files_with_matches
                    .fetch_add(1, Ordering::Relaxed);
                counters.matches.fetch_add(sink.matches, Ordering::Relaxed);
            }
            WalkState::Continue
        })
            as Box<dyn FnMut(Result<ignore::DirEntry, ignore::Error>) -> WalkState + Send>
    };
    builder.build_parallel().run(visitor);
    let total_ms = total_started.elapsed().as_millis();
    BenchResult {
        total_ms,
        fused_ms: total_ms,
        files_seen: counters.files_seen.load(Ordering::Relaxed),
        files_with_matches: counters.files_with_matches.load(Ordering::Relaxed),
        matches: counters.matches.load(Ordering::Relaxed),
        errors: counters.errors.load(Ordering::Relaxed),
    }
}

struct OccurrenceSink {
    needle: Arc<Vec<u8>>,
    matches: u64,
}

impl OccurrenceSink {
    fn new(needle: Arc<Vec<u8>>) -> Self {
        Self { needle, matches: 0 }
    }
}

impl Sink for OccurrenceSink {
    type Error = std::io::Error;

    fn matched(&mut self, _: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.matches = self
            .matches
            .saturating_add(count_ascii_case_insensitive(mat.bytes(), self.needle.as_slice()));
        Ok(true)
    }
}

fn count_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> u64 {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    let first = needle[0];
    let lower = first.to_ascii_lowercase();
    let upper = first.to_ascii_uppercase();
    let mut count = 0u64;
    if lower == upper {
        for start in memchr::memchr_iter(first, haystack) {
            if ascii_slice_matches(haystack, needle, start) {
                count = count.saturating_add(1);
            }
        }
    } else {
        for start in memchr::memchr2_iter(lower, upper, haystack) {
            if ascii_slice_matches(haystack, needle, start) {
                count = count.saturating_add(1);
            }
        }
    }
    count
}

fn ascii_slice_matches(haystack: &[u8], needle: &[u8], start: usize) -> bool {
    let end = start.saturating_add(needle.len());
    end <= haystack.len() && haystack[start..end].eq_ignore_ascii_case(needle)
}

fn search_threads() -> usize {
    std::thread::available_parallelism()
        .map(|threads| threads.get().clamp(1, MAX_THREADS))
        .unwrap_or(4)
}

fn is_default_ignored_path(path: &Path) -> bool {
    path.components().any(|component| {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        name.to_str()
            .is_some_and(|name| matches_ignore_pattern(name, DEFAULT_IGNORE_PATTERNS))
    })
}

fn matches_ignore_pattern(name: &str, patterns: &[&str]) -> bool {
    for pattern in patterns {
        let p = pattern.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(suffix) = p.strip_prefix('*') {
            if name.ends_with(suffix) {
                return true;
            }
        } else if let Some(prefix) = p.strip_suffix('*') {
            if name.starts_with(prefix) {
                return true;
            }
        } else if name == p {
            return true;
        }
    }
    false
}

fn is_binary_by_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "tif" | "tiff" | "ttf"
            | "otf" | "woff" | "woff2" | "eot" | "pdf" | "zip" | "gz" | "tgz" | "xz"
            | "bz2" | "zst" | "7z" | "rar" | "tar" | "pack" | "idx" | "so" | "dylib"
            | "dll" | "a" | "rlib" | "rmeta" | "class" | "pyc" | "pyo" | "o" | "obj"
            | "wasm"
    )
}
