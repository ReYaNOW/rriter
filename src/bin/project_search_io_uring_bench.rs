#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("project_search_io_uring_bench is available only on Linux");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
fn main() {
    linux_impl::run();
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use io_uring::{IoUring, opcode, squeue, types};
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Instant;

    const QUERY: &str = "as";
    const DEFAULT_ITERS: usize = 10;
    const MAX_THREADS: usize = 8;
    const RING_DEPTH: u32 = 256;
    const MAX_IN_FLIGHT: usize = 96;
    const READ_CHUNK_BYTES: usize = 64 * 1024;
    const FILE_CAP_BYTES: u64 = 8 * 1024 * 1024;
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

    const OP_OPEN: u64 = 0;
    const OP_READ: u64 = 1;
    const OP_CLOSE: u64 = 2;

    pub(super) fn run() {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        let iters = args
            .first()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(DEFAULT_ITERS)
            .max(1);
        let query = args
            .get(1)
            .map(|value| value.as_bytes().to_vec())
            .unwrap_or_else(|| QUERY.as_bytes().to_vec());
        let roots = workspace_roots();
        if roots.is_empty() {
            eprintln!("[IO_URING_BENCH] no workspace roots found");
            return;
        }
        for run in 1..=iters {
            let started = Instant::now();
            let walk_started = Instant::now();
            let paths = collect_candidate_paths(&roots);
            let walk_ms = walk_started.elapsed().as_millis();
            let path_count = paths.len();
            let search_started = Instant::now();
            let stats = run_io_uring_search(paths, &query);
            println!(
                "[IO_URING_BENCH] run={} total={}ms walk={}ms search={}ms files={} result_files={} matches={} bytes={}KiB errors={} query={:?} threads={} chunk={}KiB inflight={}",
                run,
                started.elapsed().as_millis(),
                walk_ms,
                search_started.elapsed().as_millis(),
                path_count,
                stats.files_with_matches,
                stats.matches,
                stats.bytes_read / 1024,
                stats.errors,
                String::from_utf8_lossy(&query),
                search_threads(),
                READ_CHUNK_BYTES / 1024,
                MAX_IN_FLIGHT,
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

    fn collect_candidate_paths(roots: &[PathBuf]) -> Vec<PathBuf> {
        let Some((first, rest)) = roots.split_first() else {
            return Vec::new();
        };
        let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
        let paths_for_walk = Arc::clone(&paths);
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
            let paths = Arc::clone(&paths_for_walk);
            Box::new(move |entry: Result<ignore::DirEntry, ignore::Error>| {
                let Ok(entry) = entry else {
                    return ignore::WalkState::Continue;
                };
                let path = entry.path();
                if entry.file_type().is_some_and(|ty| ty.is_file()) && !is_binary_by_extension(path)
                {
                    if let Ok(mut paths) = paths.lock() {
                        paths.push(path.to_path_buf());
                    }
                }
                ignore::WalkState::Continue
            })
                as Box<
                    dyn FnMut(Result<ignore::DirEntry, ignore::Error>) -> ignore::WalkState + Send,
                >
        };
        builder.build_parallel().run(visitor);
        Arc::try_unwrap(paths)
            .ok()
            .and_then(|paths| paths.into_inner().ok())
            .unwrap_or_default()
    }

    #[derive(Default, Clone, Copy)]
    struct SearchStats {
        files_with_matches: u64,
        matches: u64,
        bytes_read: u64,
        errors: u64,
    }

    impl SearchStats {
        fn add(&mut self, other: Self) {
            self.files_with_matches = self
                .files_with_matches
                .saturating_add(other.files_with_matches);
            self.matches = self.matches.saturating_add(other.matches);
            self.bytes_read = self.bytes_read.saturating_add(other.bytes_read);
            self.errors = self.errors.saturating_add(other.errors);
        }
    }

    fn run_io_uring_search(paths: Vec<PathBuf>, needle: &[u8]) -> SearchStats {
        let paths = Arc::new(paths);
        let next = Arc::new(AtomicUsize::new(0));
        let workers = search_threads().min(paths.len().max(1));
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let paths = Arc::clone(&paths);
            let next = Arc::clone(&next);
            let needle = needle.to_vec();
            handles.push(thread::spawn(move || {
                run_io_uring_worker(paths, next, &needle)
            }));
        }
        let mut stats = SearchStats::default();
        for handle in handles {
            if let Ok(worker_stats) = handle.join() {
                stats.add(worker_stats);
            } else {
                stats.errors = stats.errors.saturating_add(1);
            }
        }
        stats
    }

    fn run_io_uring_worker(
        paths: Arc<Vec<PathBuf>>,
        next: Arc<AtomicUsize>,
        needle: &[u8],
    ) -> SearchStats {
        let mut ring = match IoUring::new(RING_DEPTH) {
            Ok(ring) => ring,
            Err(_) => {
                return SearchStats {
                    errors: 1,
                    ..SearchStats::default()
                };
            }
        };
        let mut slots = (0..MAX_IN_FLIGHT).map(|_| None).collect::<Vec<_>>();
        let mut stats = SearchStats::default();
        let mut active = 0usize;
        loop {
            while active < MAX_IN_FLIGHT {
                let Some(slot) = slots.iter().position(Option::is_none) else {
                    break;
                };
                let idx = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = paths.get(idx).cloned() else {
                    break;
                };
                let Some(task) = FileTask::new(path, needle.len()) else {
                    stats.errors = stats.errors.saturating_add(1);
                    continue;
                };
                slots[slot] = Some(task);
                if submit_open(&mut ring, slot, slots[slot].as_ref().unwrap()) {
                    active += 1;
                } else {
                    slots[slot] = None;
                    stats.errors = stats.errors.saturating_add(1);
                }
            }
            if active == 0 {
                break;
            }
            if ring.submit_and_wait(1).is_err() {
                stats.errors = stats.errors.saturating_add(active as u64);
                break;
            }
            let mut completions = Vec::new();
            {
                let mut cq = ring.completion();
                while let Some(cqe) = cq.next() {
                    completions.push((cqe.user_data(), cqe.result()));
                }
            }
            for (user_data, result) in completions {
                let slot = user_slot(user_data);
                if slot >= slots.len() || slots[slot].is_none() {
                    continue;
                }
                match user_op(user_data) {
                    OP_OPEN => {
                        if result < 0 {
                            stats.errors = stats.errors.saturating_add(1);
                            slots[slot] = None;
                            active = active.saturating_sub(1);
                        } else if let Some(task) = slots[slot].as_mut() {
                            task.fd = result;
                            if !submit_read(&mut ring, slot, task) {
                                stats.errors = stats.errors.saturating_add(1);
                                let _ = submit_close(&mut ring, slot, task.fd);
                            }
                        }
                    }
                    OP_READ => {
                        let action = if let Some(task) = slots[slot].as_mut() {
                            task.handle_read(result, needle, &mut stats)
                        } else {
                            ReadAction::Drop
                        };
                        match action {
                            ReadAction::ReadMore => {
                                if let Some(task) = slots[slot].as_mut()
                                    && !submit_read(&mut ring, slot, task)
                                {
                                    stats.errors = stats.errors.saturating_add(1);
                                    let _ = submit_close(&mut ring, slot, task.fd);
                                }
                            }
                            ReadAction::Close => {
                                if let Some(task) = slots[slot].as_ref() {
                                    let _ = submit_close(&mut ring, slot, task.fd);
                                }
                            }
                            ReadAction::Drop => {
                                slots[slot] = None;
                                active = active.saturating_sub(1);
                            }
                        }
                    }
                    OP_CLOSE => {
                        if let Some(task) = slots[slot].take() {
                            task.finish(&mut stats);
                        }
                        active = active.saturating_sub(1);
                    }
                    _ => {}
                }
            }
        }
        let _ = ring.submit();
        stats
    }

    struct FileTask {
        path: CString,
        fd: i32,
        offset: u64,
        bytes_read: u64,
        matches: u64,
        has_nul: bool,
        prev_tail: Vec<u8>,
        buf: Vec<u8>,
    }

    impl FileTask {
        fn new(path: PathBuf, needle_len: usize) -> Option<Self> {
            let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
            Some(Self {
                path: c_path,
                fd: -1,
                offset: 0,
                bytes_read: 0,
                matches: 0,
                has_nul: false,
                prev_tail: Vec::with_capacity(needle_len.saturating_sub(1)),
                buf: vec![0u8; READ_CHUNK_BYTES],
            })
        }

        fn handle_read(
            &mut self,
            result: i32,
            needle: &[u8],
            stats: &mut SearchStats,
        ) -> ReadAction {
            if result < 0 {
                stats.errors = stats.errors.saturating_add(1);
                return ReadAction::Close;
            }
            let len = result as usize;
            if len == 0 {
                return ReadAction::Close;
            }
            self.bytes_read = self.bytes_read.saturating_add(len as u64);
            if self.bytes_read > FILE_CAP_BYTES {
                self.matches = 0;
                return ReadAction::Close;
            }
            let chunk = &self.buf[..len];
            if memchr::memchr(b'\0', chunk).is_some() {
                self.has_nul = true;
                self.matches = 0;
                return ReadAction::Close;
            }
            self.matches = self
                .matches
                .saturating_add(count_ascii_case_insensitive_stream(
                    &mut self.prev_tail,
                    chunk,
                    needle,
                ));
            self.offset = self.offset.saturating_add(len as u64);
            if len < READ_CHUNK_BYTES {
                ReadAction::Close
            } else {
                ReadAction::ReadMore
            }
        }

        fn finish(self, stats: &mut SearchStats) {
            stats.bytes_read = stats.bytes_read.saturating_add(self.bytes_read);
            if !self.has_nul && self.matches > 0 {
                stats.files_with_matches = stats.files_with_matches.saturating_add(1);
                stats.matches = stats.matches.saturating_add(self.matches);
            }
        }
    }

    enum ReadAction {
        ReadMore,
        Close,
        Drop,
    }

    fn submit_open(ring: &mut IoUring, slot: usize, task: &FileTask) -> bool {
        let entry = opcode::OpenAt::new(types::Fd(libc::AT_FDCWD), task.path.as_ptr())
            .flags(libc::O_RDONLY | libc::O_CLOEXEC)
            .build()
            .user_data(user_data(slot, OP_OPEN));
        push_entry(ring, entry)
    }

    fn submit_read(ring: &mut IoUring, slot: usize, task: &mut FileTask) -> bool {
        let remaining = FILE_CAP_BYTES
            .saturating_add(1)
            .saturating_sub(task.offset)
            .min(READ_CHUNK_BYTES as u64) as usize;
        if remaining == 0 {
            return false;
        }
        let entry = opcode::Read::new(types::Fd(task.fd), task.buf.as_mut_ptr(), remaining as u32)
            .offset(task.offset)
            .build()
            .user_data(user_data(slot, OP_READ));
        push_entry(ring, entry)
    }

    fn submit_close(ring: &mut IoUring, slot: usize, fd: i32) -> bool {
        if fd < 0 {
            return false;
        }
        let entry = opcode::Close::new(types::Fd(fd))
            .build()
            .user_data(user_data(slot, OP_CLOSE));
        push_entry(ring, entry)
    }

    fn push_entry(ring: &mut IoUring, entry: squeue::Entry) -> bool {
        loop {
            let pushed = unsafe { ring.submission().push(&entry) };
            if pushed.is_ok() {
                return true;
            }
            if ring.submit().is_err() {
                return false;
            }
        }
    }

    fn user_data(slot: usize, op: u64) -> u64 {
        ((slot as u64) << 2) | op
    }

    fn user_slot(user_data: u64) -> usize {
        (user_data >> 2) as usize
    }

    fn user_op(user_data: u64) -> u64 {
        user_data & 0b11
    }

    fn count_ascii_case_insensitive_stream(
        prev_tail: &mut Vec<u8>,
        chunk: &[u8],
        needle: &[u8],
    ) -> u64 {
        if needle.is_empty() || chunk.is_empty() {
            return 0;
        }
        if needle.len() == 1 {
            return count_ascii_case_insensitive(chunk, needle);
        }
        let tail_len = needle.len().saturating_sub(1);
        let mut combined = Vec::with_capacity(prev_tail.len() + chunk.len());
        combined.extend_from_slice(prev_tail);
        combined.extend_from_slice(chunk);
        let min_start = prev_tail.len().saturating_sub(tail_len);
        let count = count_ascii_case_insensitive_from(&combined, needle, min_start);
        prev_tail.clear();
        let keep = tail_len.min(combined.len());
        prev_tail.extend_from_slice(&combined[combined.len() - keep..]);
        count
    }

    fn count_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> u64 {
        count_ascii_case_insensitive_from(haystack, needle, 0)
    }

    fn count_ascii_case_insensitive_from(haystack: &[u8], needle: &[u8], min_start: usize) -> u64 {
        if needle.is_empty() || haystack.len() < needle.len() {
            return 0;
        }
        let first = needle[0];
        let lower = first.to_ascii_lowercase();
        let upper = first.to_ascii_uppercase();
        let mut count = 0u64;
        if lower == upper {
            for start in memchr::memchr_iter(first, haystack) {
                if start >= min_start && ascii_slice_matches(haystack, needle, start) {
                    count = count.saturating_add(1);
                }
            }
        } else {
            for start in memchr::memchr2_iter(lower, upper, haystack) {
                if start >= min_start && ascii_slice_matches(haystack, needle, start) {
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
}
