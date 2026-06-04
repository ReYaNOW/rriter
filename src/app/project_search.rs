use crate::editor::Editor;
use crate::scroll::ScrollState;
use globset::{Glob, GlobSet, GlobSetBuilder};
use memchr::{memchr2_iter, memchr_iter};
use rustc_hash::FxHashSet;
use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const PROJECT_SEARCH_FILE_CAP_BYTES: u64 = 8 * 1024 * 1024;
pub const PROJECT_SEARCH_MATCH_CAP: usize = 10_000;
pub const PROJECT_SEARCH_FILE_RESULT_CAP: usize = 1_000;
pub const PROJECT_SEARCH_ROW_H: f32 = 24.0;
pub const PROJECT_SEARCH_PAD_X: f32 = 10.0;
pub const PROJECT_SEARCH_QUERY_H: f32 = 78.0;
pub const PROJECT_SEARCH_SINGLE_H: f32 = 30.0;
const PROJECT_SEARCH_PREVIEW_CHARS: usize = 220;
const PROJECT_SEARCH_PREVIEW_CONTEXT_CHARS: usize = 60;
const PROJECT_SEARCH_THREADS_PER_ROOT: usize = 2;
const PROJECT_SEARCH_MAX_ACTIVE_ROOTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectSearchField {
    Query,
    Include,
    Exclude,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSearchMatch {
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub preview: String,
    pub preview_match_start: usize,
    pub preview_match_end: usize,
    pub extra_lines: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSearchFile {
    pub path: PathBuf,
    pub relative_path: String,
    pub icon_key: &'static str,
    pub matches: Vec<ProjectSearchMatch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectSearchFlatRow {
    File(usize),
    Match(usize, usize),
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct ProjectSearchWorkerResult {
    pub files: Vec<ProjectSearchFile>,
    pub total_matches: usize,
    pub elapsed_ms: u128,
    pub capped: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ProjectSearchWorkerMessage {
    File {
        generation: u64,
        file: ProjectSearchFile,
        elapsed_ms: u128,
    },
    Done {
        generation: u64,
        elapsed_ms: u128,
        capped: bool,
        error: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct ProjectSearchRequest {
    pub generation: u64,
    pub query: String,
    pub include: String,
    pub exclude: String,
    pub case_sensitive: bool,
    pub workspaces: Vec<PathBuf>,
    pub ignore_patterns: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectSearchRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectSearchLayout {
    pub query: ProjectSearchRect,
    pub include: ProjectSearchRect,
    pub exclude: ProjectSearchRect,
    pub case_button: ProjectSearchRect,
    pub run_button: ProjectSearchRect,
    pub help_button: ProjectSearchRect,
    pub stats_y: f32,
    pub list: ProjectSearchRect,
}

pub struct ProjectSearchState {
    pub query_editor: Editor,
    pub include_editor: Editor,
    pub exclude_editor: Editor,
    pub focused: Option<ProjectSearchField>,
    pub case_sensitive: bool,
    pub help_open: bool,
    pub dragging_field: Option<ProjectSearchField>,
    pub dirty: bool,
    pub generation: u64,
    pub running_generation: Option<u64>,
    pub rx: Option<Receiver<ProjectSearchWorkerMessage>>,
    pub results: Vec<ProjectSearchFile>,
    pub flat_rows: Vec<ProjectSearchFlatRow>,
    pub collapsed: FxHashSet<PathBuf>,
    pub scroll: ScrollState,
    pub has_run: bool,
    pub total_matches: usize,
    pub elapsed_ms: Option<u128>,
    pub capped: bool,
    pub error: Option<String>,
}

impl Default for ProjectSearchState {
    fn default() -> Self {
        Self {
            query_editor: Editor::new(512),
            include_editor: Editor::new(256),
            exclude_editor: Editor::new(256),
            focused: None,
            case_sensitive: false,
            help_open: false,
            dragging_field: None,
            dirty: true,
            generation: 0,
            running_generation: None,
            rx: None,
            results: Vec::new(),
            flat_rows: Vec::new(),
            collapsed: FxHashSet::default(),
            scroll: ScrollState::new(7.0),
            has_run: false,
            total_matches: 0,
            elapsed_ms: None,
            capped: false,
            error: None,
        }
    }
}

impl ProjectSearchState {
    pub fn rebuild_flat_rows(&mut self) {
        self.flat_rows.clear();
        for (file_idx, file) in self.results.iter().enumerate() {
            self.flat_rows.push(ProjectSearchFlatRow::File(file_idx));
            if !self.collapsed.contains(&file.path) {
                for match_idx in 0..file.matches.len() {
                    self.flat_rows
                        .push(ProjectSearchFlatRow::Match(file_idx, match_idx));
                }
            }
        }
    }

    pub fn toggle_file(&mut self, file_idx: usize) {
        let Some(path) = self.results.get(file_idx).map(|file| file.path.clone()) else {
            return;
        };
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path);
        }
        self.rebuild_flat_rows();
    }

    pub fn apply_message(&mut self, message: ProjectSearchWorkerMessage) -> bool {
        let generation = match &message {
            ProjectSearchWorkerMessage::File { generation, .. }
            | ProjectSearchWorkerMessage::Done { generation, .. } => *generation,
        };
        if Some(generation) != self.running_generation || generation < self.generation {
            return false;
        }
        match message {
            ProjectSearchWorkerMessage::File {
                file, elapsed_ms, ..
            } => {
                self.total_matches = self.total_matches.saturating_add(file.matches.len());
                self.elapsed_ms = Some(elapsed_ms);
                self.results.push(file);
                self.rebuild_flat_rows();
                true
            }
            ProjectSearchWorkerMessage::Done {
                elapsed_ms,
                capped,
                error,
                ..
            } => {
                self.running_generation = None;
                self.rx = None;
                self.elapsed_ms = Some(elapsed_ms);
                self.capped = capped;
                self.error = error;
                self.rebuild_flat_rows();
                true
            }
        }
    }

    pub fn max_scroll(&self, list_h: f32, scale: f32) -> f32 {
        let row_h = PROJECT_SEARCH_ROW_H * scale;
        (self.flat_rows.len() as f32 * row_h - list_h).max(0.0)
    }
}

pub fn project_search_layout(
    content_x: f32,
    content_y: f32,
    content_w: f32,
    content_h: f32,
    scale: f32,
) -> ProjectSearchLayout {
    let pad = PROJECT_SEARCH_PAD_X * scale;
    let gap = 7.0 * scale;
    let button = PROJECT_SEARCH_SINGLE_H * scale;
    let help = (22.0 * scale).round().max(18.0);
    let label_h = 18.0 * scale;
    let mut y = content_y + 9.0 * scale;
    let query_w = (content_w - pad * 2.0 - button * 2.0 - gap * 2.0).max(40.0 * scale);
    let query = ProjectSearchRect {
        x: content_x + pad,
        y: y + label_h,
        w: query_w,
        h: PROJECT_SEARCH_QUERY_H * scale,
    };
    let case_button = ProjectSearchRect {
        x: query.x + query.w + gap,
        y: query.y,
        w: button,
        h: button,
    };
    let run_button = ProjectSearchRect {
        x: case_button.x + case_button.w + gap,
        y: query.y,
        w: button,
        h: button,
    };
    let help_button = ProjectSearchRect {
        x: content_x + content_w - pad - help,
        y: (query.y - 24.0 * scale).max(content_y + 2.0 * scale),
        w: help,
        h: help,
    };

    y = query.y + query.h + 9.0 * scale;
    let include = ProjectSearchRect {
        x: content_x + pad,
        y: y + label_h,
        w: (content_w - pad * 2.0).max(40.0 * scale),
        h: PROJECT_SEARCH_SINGLE_H * scale,
    };
    y = include.y + include.h + 7.0 * scale;
    let exclude = ProjectSearchRect {
        x: content_x + pad,
        y: y + label_h,
        w: include.w,
        h: PROJECT_SEARCH_SINGLE_H * scale,
    };
    let stats_y = exclude.y + exclude.h + 26.0 * scale;
    let list_y = stats_y + 8.0 * scale;
    ProjectSearchLayout {
        query,
        include,
        exclude,
        case_button,
        run_button,
        help_button,
        stats_y,
        list: ProjectSearchRect {
            x: content_x,
            y: list_y,
            w: content_w,
            h: (content_y + content_h - list_y).max(0.0),
        },
    }
}

pub fn start_project_search_worker(request: ProjectSearchRequest) -> Receiver<ProjectSearchWorkerMessage> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        stream_project_search(request, tx);
    });
    rx
}

#[cfg(test)]
pub fn run_project_search(request: ProjectSearchRequest) -> ProjectSearchWorkerResult {
    let (tx, rx) = channel();
    stream_project_search(request, tx);
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

#[derive(Default)]
struct SearchCaps {
    matches: usize,
    files: usize,
    capped: bool,
}

fn stream_project_search(
    request: ProjectSearchRequest,
    tx: std::sync::mpsc::Sender<ProjectSearchWorkerMessage>,
) {
    let started = Instant::now();
    let generation = request.generation;
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

    let needle = Arc::new(request.query.into_bytes());
    let unicode_case_fallback = !request.case_sensitive && !needle.is_ascii();
    let settings_ignore = Arc::new(SearchIgnoreMatcher::new(request.ignore_patterns));
    let plan = Arc::new(plan);
    let caps = Arc::new(Mutex::new(SearchCaps::default()));
    let capped_flag = Arc::new(AtomicBool::new(false));
    let roots = plan
        .walk_roots()
        .into_iter()
        .filter(|root| root.is_dir())
        .map(Path::to_path_buf)
        .collect::<VecDeque<_>>();
    let active_roots = roots
        .len()
        .min(PROJECT_SEARCH_MAX_ACTIVE_ROOTS)
        .max(1);
    let roots = Arc::new(Mutex::new(roots));
    let mut root_workers = Vec::with_capacity(active_roots);
    for _ in 0..active_roots {
        let roots = Arc::clone(&roots);
        let plan = Arc::clone(&plan);
        let settings_ignore = Arc::clone(&settings_ignore);
        let caps = Arc::clone(&caps);
        let capped_flag = Arc::clone(&capped_flag);
        let tx = tx.clone();
        let needle = Arc::clone(&needle);
        let case_sensitive = request.case_sensitive;
        root_workers.push(std::thread::spawn(move || {
            loop {
                if capped_flag.load(Ordering::Relaxed) {
                    break;
                }
                let root = match roots.lock().ok().and_then(|mut roots| roots.pop_front()) {
                    Some(root) => root,
                    None => break,
                };
                run_project_search_root(
                    root,
                    generation,
                    started,
                    Arc::clone(&plan),
                    Arc::clone(&settings_ignore),
                    Arc::clone(&caps),
                    Arc::clone(&capped_flag),
                    tx.clone(),
                    Arc::clone(&needle),
                    case_sensitive,
                    unicode_case_fallback,
                );
            }
        }));
    }
    for worker in root_workers {
        let _ = worker.join();
    }
    let capped = caps.lock().map(|caps| caps.capped).unwrap_or(true);
    let _ = tx.send(ProjectSearchWorkerMessage::Done {
        generation,
        elapsed_ms: started.elapsed().as_millis(),
        capped,
        error: None,
    });
}

fn run_project_search_root(
    root: PathBuf,
    generation: u64,
    started: Instant,
    plan: Arc<SearchPatternPlan>,
    settings_ignore: Arc<SearchIgnoreMatcher>,
    caps: Arc<Mutex<SearchCaps>>,
    capped_flag: Arc<AtomicBool>,
    tx: Sender<ProjectSearchWorkerMessage>,
    needle: Arc<Vec<u8>>,
    case_sensitive: bool,
    unicode_case_fallback: bool,
) {
    let settings_ignore_for_walk = Arc::clone(&settings_ignore);
    let settings_workspaces = plan.workspaces.clone();
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .ignore(true)
        .parents(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .threads(PROJECT_SEARCH_THREADS_PER_ROOT)
        .filter_entry(move |entry| {
            !settings_ignore_for_walk.matches_path(entry.path(), &settings_workspaces)
        });
    let visitor = move || {
        let plan = Arc::clone(&plan);
        let settings_ignore = Arc::clone(&settings_ignore);
        let caps = Arc::clone(&caps);
        let capped_flag = Arc::clone(&capped_flag);
        let tx = tx.clone();
        let needle = Arc::clone(&needle);
        Box::new(move |entry: Result<ignore::DirEntry, ignore::Error>| {
            if capped_flag.load(Ordering::Relaxed) {
                return ignore::WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return ignore::WalkState::Continue;
            };
            let path = entry.path();
            if !entry.file_type().is_some_and(|ty| ty.is_file())
                || !plan.is_file_allowed(path)
                || settings_ignore.matches_path(path, &plan.workspaces)
            {
                return ignore::WalkState::Continue;
            }
            let Some(file) = search_project_file(
                path,
                &plan,
                &needle,
                case_sensitive,
                unicode_case_fallback,
                &caps,
                &capped_flag,
            ) else {
                return ignore::WalkState::Continue;
            };
            let elapsed_ms = started.elapsed().as_millis();
            let _ = tx.send(ProjectSearchWorkerMessage::File {
                generation,
                file,
                elapsed_ms,
            });
            ignore::WalkState::Continue
        }) as Box<dyn FnMut(Result<ignore::DirEntry, ignore::Error>) -> ignore::WalkState + Send>
    };
    builder.build_parallel().run(visitor);
}

fn search_project_file(
    path: &Path,
    plan: &SearchPatternPlan,
    needle: &[u8],
    case_sensitive: bool,
    unicode_case_fallback: bool,
    caps: &Mutex<SearchCaps>,
    capped_flag: &AtomicBool,
) -> Option<ProjectSearchFile> {
    if capped_flag.load(Ordering::Relaxed) {
        return None;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return None;
    };
    if metadata.len() > PROJECT_SEARCH_FILE_CAP_BYTES {
        return None;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return None;
    };
    let mut buf = Vec::with_capacity(metadata.len() as usize);
    if file.read_to_end(&mut buf).is_err() || memchr::memchr(b'\0', &buf).is_some() {
        return None;
    }
    let mut matches = Vec::new();
    let room = match caps.lock() {
        Ok(caps) if caps.capped
            || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP
            || caps.matches >= PROJECT_SEARCH_MATCH_CAP =>
        {
            capped_flag.store(true, Ordering::Relaxed);
            return None;
        }
        Ok(caps) => PROJECT_SEARCH_MATCH_CAP.saturating_sub(caps.matches),
        Err(_) => {
            capped_flag.store(true, Ordering::Relaxed);
            return None;
        }
    };
    if unicode_case_fallback {
        let Ok(text) = std::str::from_utf8(&buf) else {
            return None;
        };
        let mut line_offsets = Vec::new();
        collect_unicode_case_insensitive_matches(text, needle, |start, end| {
            push_match(text, start, end, &mut line_offsets, &mut matches);
            matches.len() < room
        });
    } else if case_sensitive {
        let mut ranges = Vec::new();
        let finder = memchr::memmem::Finder::new(needle);
        for start in finder.find_iter(&buf) {
            ranges.push((start, start + needle.len()));
            if ranges.len() >= room {
                break;
            }
        }
        if ranges.is_empty() {
            return None;
        }
        let Ok(text) = std::str::from_utf8(&buf) else {
            return None;
        };
        let mut line_offsets = Vec::new();
        for (start, end) in ranges {
            push_match(text, start, end, &mut line_offsets, &mut matches);
        }
    } else {
        let mut ranges = Vec::new();
        collect_ascii_case_insensitive_matches(&buf, needle, |start, end| {
            ranges.push((start, end));
            ranges.len() < room
        });
        if ranges.is_empty() {
            return None;
        }
        let Ok(text) = std::str::from_utf8(&buf) else {
            return None;
        };
        let mut line_offsets = Vec::new();
        for (start, end) in ranges {
            push_match(text, start, end, &mut line_offsets, &mut matches);
        }
    }
    if matches.is_empty() {
        return None;
    }
    let reached_cap = {
        let mut caps = match caps.lock() {
            Ok(caps) => caps,
            Err(_) => {
                capped_flag.store(true, Ordering::Relaxed);
                return None;
            }
        };
        if caps.capped
            || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP
            || caps.matches >= PROJECT_SEARCH_MATCH_CAP
        {
            caps.capped = true;
            true
        } else {
            let room = PROJECT_SEARCH_MATCH_CAP - caps.matches;
            if matches.len() > room {
                matches.truncate(room);
                caps.capped = true;
            }
            caps.matches += matches.len();
            caps.files += 1;
            if caps.matches >= PROJECT_SEARCH_MATCH_CAP
                || caps.files >= PROJECT_SEARCH_FILE_RESULT_CAP
            {
                caps.capped = true;
            }
            caps.capped
        }
    };
    if reached_cap {
        capped_flag.store(true, Ordering::Relaxed);
    }
    if matches.is_empty() {
        return None;
    }
    let relative_path = plan.relative_display(path);
    let icon_key = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| crate::app::file_icons::file_icon_key(&name.to_lowercase()))
        .unwrap_or("default_file");
    crate::app::file_tree::pre_rasterize_icon(icon_key, false);
    Some(ProjectSearchFile {
        path: path.to_path_buf(),
        relative_path,
        icon_key,
        matches,
    })
}

fn collect_ascii_case_insensitive_matches(
    hay: &[u8],
    needle: &[u8],
    mut emit: impl FnMut(usize, usize) -> bool,
) {
    if needle.is_empty() || hay.len() < needle.len() {
        return;
    }
    let first = needle[0];
    let lower = first.to_ascii_lowercase();
    let upper = first.to_ascii_uppercase();
    if lower != upper {
        for idx in memchr2_iter(lower, upper, hay) {
            if idx + needle.len() <= hay.len() && ascii_eq_at(hay, idx, needle) {
                if !emit(idx, idx + needle.len()) {
                    break;
                }
            }
        }
    } else {
        for idx in memchr_iter(first, hay) {
            if idx + needle.len() <= hay.len() && ascii_eq_at(hay, idx, needle) {
                if !emit(idx, idx + needle.len()) {
                    break;
                }
            }
        }
    }
}

fn ascii_eq_at(hay: &[u8], start: usize, needle: &[u8]) -> bool {
    needle
        .iter()
        .enumerate()
        .all(|(i, &b)| hay[start + i].to_ascii_lowercase() == b.to_ascii_lowercase())
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
    let mut byte_to_original = Vec::with_capacity(text.len() + 1);
    for (idx, ch) in text.char_indices() {
        for lowered in ch.to_lowercase() {
            let mut buf = [0u8; 4];
            let encoded = lowered.encode_utf8(&mut buf);
            for _ in 0..encoded.len() {
                byte_to_original.push(idx);
            }
            lower.push(lowered);
        }
    }
    byte_to_original.push(text.len());
    for (idx, found) in lower.match_indices(&query) {
        let end = idx + found.len();
        let start_orig = byte_to_original.get(idx).copied().unwrap_or(text.len());
        let end_orig = byte_to_original.get(end).copied().unwrap_or(text.len());
        if end_orig >= start_orig && !emit(start_orig, end_orig) {
            break;
        }
    }
}

struct SearchIgnoreMatcher {
    patterns: Vec<String>,
}

impl SearchIgnoreMatcher {
    fn new(mut patterns: Vec<String>) -> Self {
        for pattern in crate::app::file_tree::DEFAULT_IGNORE_PATTERNS {
            if !patterns.iter().any(|existing| existing == pattern) {
                patterns.push((*pattern).to_string());
            }
        }
        if !patterns.iter().any(|existing| existing == ".git") {
            patterns.push(".git".to_string());
        }
        Self { patterns }
    }

    fn matches_path(&self, path: &Path, workspaces: &[PathBuf]) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let Some(rel) = workspaces
            .iter()
            .find_map(|workspace| path.strip_prefix(workspace).ok())
        else {
            return false;
        };
        rel.components().any(|component| {
            let std::path::Component::Normal(name) = component else {
                return false;
            };
            name.to_str().is_some_and(|name| {
                crate::app::file_tree::matches_ignore_pattern_strings(name, &self.patterns)
            })
        })
    }
}

fn push_match(
    text: &str,
    start: usize,
    end: usize,
    line_offsets: &mut Vec<usize>,
    matches: &mut Vec<ProjectSearchMatch>,
) {
    let start = floor_char_boundary(text, start.min(text.len()));
    let end = ceil_char_boundary(text, end.min(text.len()));
    if line_offsets.is_empty() {
        *line_offsets = line_offsets_for_text(text);
    }
    let offsets = line_offsets.as_slice();
    let (start_line, start_col) = crate::lsp::offset_to_lsp_pos(text, start, offsets);
    let (end_line, end_col) = crate::lsp::offset_to_lsp_pos(text, end, offsets);
    let line_idx = start_line as usize;
    let line_start = offsets.get(line_idx).copied().unwrap_or(0);
    let mut line_end = offsets
        .get(line_idx + 1)
        .copied()
        .unwrap_or(text.len())
        .min(text.len());
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\n') {
        line_end -= 1;
    }
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\r') {
        line_end -= 1;
    }
    let (preview, preview_match_start, preview_match_end) = text
        .get(line_start..line_end)
        .map(|line| preview_line_with_match(line, start - line_start, end - line_start))
        .unwrap_or_else(|| (String::new(), 0, 0));
    let extra_lines = text
        .get(start..end)
        .map(|matched| matched.bytes().filter(|&b| b == b'\n').count())
        .unwrap_or(0);
    matches.push(ProjectSearchMatch {
        byte_start: start,
        byte_end: end,
        start_line,
        start_col,
        end_line,
        end_col,
        preview,
        preview_match_start,
        preview_match_end,
        extra_lines,
    });
}

fn line_offsets_for_text(text: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    offsets.push(0);
    for (idx, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            offsets.push(idx + 1);
        }
    }
    offsets
}

fn floor_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn preview_line_with_match(line: &str, start: usize, end: usize) -> (String, usize, usize) {
    let start = floor_char_boundary(line, start.min(line.len()));
    let end = ceil_char_boundary(line, end.min(line.len())).max(start);
    let total_chars = line.chars().count();
    let start_char = byte_to_char_idx(line, start);
    let end_char = byte_to_char_idx(line, end).max(start_char);
    let (first_char, last_char) = if total_chars <= PROJECT_SEARCH_PREVIEW_CHARS {
        (0, total_chars)
    } else {
        let available = PROJECT_SEARCH_PREVIEW_CHARS.saturating_sub(6).max(32);
        let mut first = start_char.saturating_sub(PROJECT_SEARCH_PREVIEW_CONTEXT_CHARS);
        let mut last = (first + available).min(total_chars);
        if end_char > last {
            last = end_char.min(total_chars);
            first = last.saturating_sub(available);
        }
        (first, last)
    };
    let segment_start = char_to_byte_idx(line, first_char);
    let segment_end = char_to_byte_idx(line, last_char);
    let mut preview = String::with_capacity((segment_end - segment_start).min(line.len()) + 6);
    if segment_start > 0 {
        preview.push_str("...");
    }
    let prefix_len = preview.len();
    if let Some(segment) = line.get(segment_start..segment_end) {
        for ch in segment.chars() {
            preview.push(if ch == '\t' { ' ' } else { ch });
        }
    }
    let match_start = prefix_len + start.max(segment_start).min(segment_end) - segment_start;
    let match_end = prefix_len + end.max(segment_start).min(segment_end) - segment_start;
    if segment_end < line.len() {
        preview.push_str("...");
    }
    (preview, match_start, match_end.max(match_start))
}

fn byte_to_char_idx(text: &str, byte_idx: usize) -> usize {
    text[..floor_char_boundary(text, byte_idx.min(text.len()))]
        .chars()
        .count()
}

fn char_to_byte_idx(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

struct SearchPatternPlan {
    workspaces: Vec<PathBuf>,
    include_roots: Vec<PathBuf>,
    include_globs: Option<GlobSet>,
    exclude_roots: Vec<PathBuf>,
    exclude_globs: Option<GlobSet>,
    include_has_glob: bool,
    include_all: bool,
}

impl SearchPatternPlan {
    fn new(workspaces: &[PathBuf], include: &str, exclude: &str) -> Result<Self, String> {
        let mut plan = Self {
            workspaces: normalized_workspaces(workspaces),
            include_roots: Vec::new(),
            include_globs: None,
            exclude_roots: Vec::new(),
            exclude_globs: None,
            include_has_glob: false,
            include_all: include.trim().is_empty(),
        };
        let include_tokens = split_pattern_tokens(include);
        let exclude_tokens = split_pattern_tokens(exclude);
        let mut include_builder = GlobSetBuilder::new();
        let mut include_glob_count = 0usize;
        for token in include_tokens {
            if token_has_glob(token) {
                plan.include_has_glob = true;
                for pattern in glob_patterns_for_token(token, &plan.workspaces) {
                    include_builder.add(Glob::new(&pattern).map_err(|err| err.to_string())?);
                    include_glob_count += 1;
                }
            } else {
                for path in expand_path_token(token, &plan.workspaces) {
                    push_unique_path(&mut plan.include_roots, path);
                }
            }
        }
        if include_glob_count > 0 {
            plan.include_globs = Some(include_builder.build().map_err(|err| err.to_string())?);
        }
        if plan.include_roots.is_empty() && !plan.include_has_glob {
            plan.include_all = true;
        }

        let mut exclude_builder = GlobSetBuilder::new();
        let mut exclude_glob_count = 0usize;
        for token in exclude_tokens {
            if token_has_glob(token) {
                for pattern in glob_patterns_for_token(token, &plan.workspaces) {
                    exclude_builder.add(Glob::new(&pattern).map_err(|err| err.to_string())?);
                    exclude_glob_count += 1;
                }
            } else {
                for path in expand_path_token(token, &plan.workspaces) {
                    push_unique_path(&mut plan.exclude_roots, path);
                }
            }
        }
        if exclude_glob_count > 0 {
            plan.exclude_globs = Some(exclude_builder.build().map_err(|err| err.to_string())?);
        }
        Ok(plan)
    }

    fn walk_roots(&self) -> Vec<&Path> {
        if self.include_all || self.include_has_glob {
            self.workspaces.iter().map(PathBuf::as_path).collect()
        } else {
            self.include_roots.iter().map(PathBuf::as_path).collect()
        }
    }

    fn is_file_allowed(&self, path: &Path) -> bool {
        let Some((workspace, rel)) = self.workspace_relative(path) else {
            return false;
        };
        if !self.include_all && self.include_has_glob {
            let prefix_match = self.include_roots.iter().any(|root| path.starts_with(root));
            let glob_match = self
                .include_globs
                .as_ref()
                .is_some_and(|set| set.is_match(to_slash(rel)));
            if !prefix_match && !glob_match {
                return false;
            }
        } else if !self.include_all && !self.include_roots.iter().any(|root| path.starts_with(root))
        {
            return false;
        }
        if !path.starts_with(workspace) {
            return false;
        }
        if self.exclude_roots.iter().any(|root| path.starts_with(root)) {
            return false;
        }
        if self
            .exclude_globs
            .as_ref()
            .is_some_and(|set| set.is_match(to_slash(rel)))
        {
            return false;
        }
        true
    }

    fn workspace_relative<'a>(&'a self, path: &'a Path) -> Option<(&'a Path, &'a Path)> {
        for workspace in &self.workspaces {
            if let Ok(rel) = path.strip_prefix(workspace) {
                return Some((workspace.as_path(), rel));
            }
        }
        None
    }

    fn relative_display(&self, path: &Path) -> String {
        if let Some((workspace, rel)) = self.workspace_relative(path) {
            let workspace_name = workspace
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace");
            let rel = to_slash(rel);
            if rel.is_empty() {
                workspace_name.to_string()
            } else {
                format!("{}/{}", workspace_name, rel)
            }
        } else {
            path.to_string_lossy().replace('\\', "/")
        }
    }
}

fn normalized_workspaces(workspaces: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for workspace in workspaces {
        let path = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.to_path_buf());
        if path.is_dir() {
            push_unique_path(&mut out, path);
        }
    }
    out
}

fn split_pattern_tokens(text: &str) -> Vec<&str> {
    text.split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect()
}

fn token_has_glob(token: &str) -> bool {
    token
        .bytes()
        .any(|b| matches!(b, b'*' | b'?' | b'[' | b']' | b'{' | b'}'))
}

fn expand_path_token(token: &str, workspaces: &[PathBuf]) -> Vec<PathBuf> {
    let token = token.trim();
    if token.is_empty() {
        return Vec::new();
    }
    let raw = Path::new(token);
    if raw.is_absolute() {
        let path = raw.canonicalize().unwrap_or_else(|_| raw.to_path_buf());
        if workspaces.iter().any(|workspace| path.starts_with(workspace)) {
            return vec![path];
        }
        return Vec::new();
    }
    let rel = token.strip_prefix("./").unwrap_or(token);
    let rel = if rel == "." { "" } else { rel };
    workspaces.iter().map(|workspace| workspace.join(rel)).collect()
}

fn glob_patterns_for_token(token: &str, workspaces: &[PathBuf]) -> Vec<String> {
    let token = token.trim();
    let raw = Path::new(token);
    if raw.is_absolute() {
        let mut out = Vec::new();
        for workspace in workspaces {
            if let Ok(rel) = raw.strip_prefix(workspace) {
                let pattern = to_slash(rel);
                if !pattern.is_empty() {
                    out.push(pattern);
                }
            }
        }
        out
    } else {
        vec![to_slash(Path::new(token.strip_prefix("./").unwrap_or(token)))]
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl crate::app::App {
    pub fn open_project_search_panel(&mut self) {
        self.ide_panel.open(crate::app::PanelId::Search);
        self.ide_panel.project_search.focused = Some(ProjectSearchField::Query);
        self.search_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        crate::save_panel_state(&self.ide_panel);
    }

    pub fn start_project_search(&mut self) {
        let query = self.ide_panel.project_search.query_editor.get_full_text();
        self.ide_panel.project_search.generation =
            self.ide_panel.project_search.generation.saturating_add(1);
        let generation = self.ide_panel.project_search.generation;
        self.ide_panel.project_search.has_run = true;
        self.ide_panel.project_search.error = None;
        self.ide_panel.project_search.elapsed_ms = None;
        self.ide_panel.project_search.capped = false;
        self.ide_panel.project_search.results.clear();
        self.ide_panel.project_search.flat_rows.clear();
        self.ide_panel.project_search.collapsed.clear();
        self.ide_panel.project_search.total_matches = 0;
        self.ide_panel.project_search.scroll.target = 0.0;
        self.ide_panel.project_search.scroll.current = 0.0;
        if query.is_empty() {
            self.ide_panel.project_search.running_generation = None;
            self.ide_panel.project_search.rx = None;
            return;
        }
        let request = ProjectSearchRequest {
            generation,
            query,
            include: self.ide_panel.project_search.include_editor.get_full_text(),
            exclude: self.ide_panel.project_search.exclude_editor.get_full_text(),
            case_sensitive: self.ide_panel.project_search.case_sensitive,
            workspaces: self.ide_workspaces.clone(),
            ignore_patterns: self.ide_ignore_patterns.clone(),
        };
        self.ide_panel.project_search.running_generation = Some(generation);
        self.ide_panel.project_search.rx = Some(start_project_search_worker(request));
        self.ide_panel.project_search.dirty = false;
    }

    pub fn poll_project_search(&mut self) -> bool {
        let mut messages = Vec::new();
        if let Some(rx) = &self.ide_panel.project_search.rx {
            while let Ok(message) = rx.try_recv() {
                messages.push(message);
            }
        }
        if messages.is_empty() {
            return false;
        }
        let mut updated = false;
        for message in messages {
            updated |= self.ide_panel.project_search.apply_message(message);
        }
        updated
    }

    pub fn project_search_panel_layout(&self) -> Option<ProjectSearchLayout> {
        if !self.is_ide_mode || !self.ide_panel.is_open(crate::app::PanelId::Search) {
            return None;
        }
        let renderer = self.renderer.as_ref()?;
        let scale = renderer.scale_factor;
        let wh = self
            .window
            .as_ref()
            .map(|window| window.inner_size().height as f32)
            .unwrap_or(renderer.height);
        let panel_bottom_h = if self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * scale
        } else {
            0.0
        };
        let content_bottom = crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, scale);
        Some(project_search_layout(
            48.0 * scale,
            32.0 * scale,
            self.ide_panel.left_width * scale,
            (content_bottom - 32.0 * scale).max(0.0),
            scale,
        ))
    }

    pub fn focus_project_search_field(&mut self, field: ProjectSearchField) {
        self.ide_panel.project_search.focused = Some(field);
        self.search_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        self.ide_panel.lsp_logs_focused = None;
        self.place_project_search_cursor_from_mouse(field);
    }

    pub fn place_project_search_cursor_from_mouse(&mut self, field: ProjectSearchField) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        self.set_project_search_cursor_at(field, renderer.last_mouse_x, renderer.last_mouse_y, true);
    }

    pub fn drag_project_search_cursor_to(&mut self, field: ProjectSearchField, x: f32, y: f32) {
        self.set_project_search_cursor_at(field, x, y, false);
    }

    fn set_project_search_cursor_at(
        &mut self,
        field: ProjectSearchField,
        mouse_x: f32,
        mouse_y: f32,
        reset_anchor: bool,
    ) {
        let Some(layout) = self.project_search_panel_layout() else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let rect = match field {
            ProjectSearchField::Query => layout.query,
            ProjectSearchField::Include => layout.include,
            ProjectSearchField::Exclude => layout.exclude,
        };
        let text_scale = 0.82;
        let line_h = (18.0 * renderer.scale_factor).round().max(1.0);
        let editor = match field {
            ProjectSearchField::Query => &mut self.ide_panel.project_search.query_editor,
            ProjectSearchField::Include => &mut self.ide_panel.project_search.include_editor,
            ProjectSearchField::Exclude => &mut self.ide_panel.project_search.exclude_editor,
        };
        let text = editor.get_full_text();
        let visual_line = if field == ProjectSearchField::Query {
            ((mouse_y - rect.y - 5.0 * renderer.scale_factor).max(0.0) / line_h) as usize
        } else {
            0
        };
        let visible_lines = if field == ProjectSearchField::Query {
            ((rect.h - 8.0 * renderer.scale_factor) / line_h)
                .floor()
                .max(1.0) as usize
        } else {
            1
        };
        let cursor_line = editor
            .line_offsets
            .partition_point(|&offset| offset <= editor.cursor)
            .saturating_sub(1);
        let first_line = if field == ProjectSearchField::Query {
            cursor_line.saturating_sub(visible_lines.saturating_sub(1))
        } else {
            0
        };
        let line = (first_line + visual_line).min(editor.line_offsets.len().saturating_sub(1));
        let line_start = editor.line_offsets.get(line).copied().unwrap_or(0);
        let mut line_end = editor
            .line_offsets
            .get(line + 1)
            .copied()
            .unwrap_or(text.len())
            .min(text.len());
        if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\n') {
            line_end -= 1;
        }
        if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\r') {
            line_end -= 1;
        }
        let x_offset = (mouse_x - (rect.x + 7.0 * renderer.scale_factor)).max(0.0);
        let mut current_x = 0.0;
        let mut target = line_end;
        if let Some(line_text) = text.get(line_start..line_end) {
            for (rel_idx, ch) in line_text.char_indices() {
                let adv = renderer
                    .get_ui_glyph(ch)
                    .map(|glyph| glyph.advance)
                    .unwrap_or(10.0)
                    * text_scale;
                if x_offset <= current_x + adv * 0.5 {
                    target = line_start + rel_idx;
                    break;
                }
                current_x += adv;
            }
        }
        editor.cursor = target;
        if reset_anchor || editor.selection_anchor.is_none() {
            editor.selection_anchor = Some(target);
        }
    }

    pub fn handle_project_search_match_click(&mut self, file_idx: usize, match_idx: usize) {
        let Some((path, start_line, start_col, end_line, end_col)) =
            self.ide_panel.project_search.results.get(file_idx).and_then(|file| {
                file.matches.get(match_idx).map(|mat| {
                    (
                        file.path.clone(),
                        mat.start_line,
                        mat.start_col,
                        mat.end_line,
                        mat.end_col,
                    )
                })
            })
        else {
            return;
        };
        let was_active = self.current_abs_path().as_ref() == Some(&self.abs_path_for_workspace(&path));
        self.jump_to_project_search_position(path, true, start_line, start_col, end_line, end_col);
        if !was_active {
            self.scroll_y.current = self.scroll_y.target;
            self.scroll_y.velocity = 0.0;
            self.scroll_x.current = self.scroll_x.target;
            self.scroll_x.velocity = 0.0;
        }
    }

    fn jump_to_project_search_position(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) {
        let was_open = self.jump_to_lsp_position_in_file(path, start_line, start_col, add_to_history, 0.45);
        let text = self.editor.get_full_text();
        let start = crate::lsp::lsp_pos_to_offset(&text, start_line, start_col).min(self.editor.len());
        let end = crate::lsp::lsp_pos_to_offset(&text, end_line, end_col).min(self.editor.len());
        self.editor.selection_anchor = Some(start.min(end));
        self.editor.cursor = end.max(start);
        if !was_open {
            self.reprioritize_highlighter_around_cursor();
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn project_search_pattern_plan_clamps_absolute_paths_to_workspace() {
        let root = temp_workspace("pattern");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let plan = SearchPatternPlan::new(
            &[root.clone()],
            &format!("{}, /tmp/not-in-workspace", root.join("src").display()),
            "/tmp/also-outside",
        )
        .unwrap();

        assert_eq!(plan.include_roots, vec![root.join("src")]);
        assert!(plan.exclude_roots.is_empty());
        let _ = std::fs::remove_dir_all(root);
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
    fn project_search_preview_keeps_match_visible_and_ranged() {
        let line = format!("{}needle{}", "a".repeat(120), "b".repeat(120));
        let (preview, start, end) = preview_line_with_match(&line, 120, 126);

        assert!(preview.starts_with("..."));
        assert!(preview.contains("needle"));
        assert_eq!(&preview[start..end], "needle");
    }

    #[test]
    fn project_search_flat_rows_respect_collapsed_files() {
        let mut state = ProjectSearchState::default();
        state.results.push(ProjectSearchFile {
            path: PathBuf::from("/w/src/a.rs"),
            relative_path: "src/a.rs".to_string(),
            icon_key: "rust",
            matches: vec![
                ProjectSearchMatch {
                    byte_start: 0,
                    byte_end: 1,
                    start_line: 0,
                    start_col: 0,
                    end_line: 0,
                    end_col: 1,
                    preview: "a".to_string(),
                    preview_match_start: 0,
                    preview_match_end: 1,
                    extra_lines: 0,
                },
                ProjectSearchMatch {
                    byte_start: 2,
                    byte_end: 3,
                    start_line: 1,
                    start_col: 0,
                    end_line: 1,
                    end_col: 1,
                    preview: "b".to_string(),
                    preview_match_start: 0,
                    preview_match_end: 1,
                    extra_lines: 0,
                },
            ],
        });
        state.rebuild_flat_rows();
        assert_eq!(state.flat_rows.len(), 3);
        state.toggle_file(0);
        assert_eq!(state.flat_rows, vec![ProjectSearchFlatRow::File(0)]);
    }
}
