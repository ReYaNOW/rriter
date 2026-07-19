use crate::editor::Editor;
use crate::platform::{self, PathKey};
use crate::scroll::ScrollState;
use globset::{Glob, GlobSet, GlobSetBuilder};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use memchr::memmem::Finder;
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[path = "project_search_grep.rs"]
mod project_search_grep;
#[path = "project_search_preview.rs"]
mod project_search_preview;
pub(crate) use project_search_preview::project_search_scrollbar_thumb;
#[cfg(test)]
pub(crate) use project_search_preview::{
    ProjectSearchPreviewKey, ProjectSearchPreviewRequest, ProjectSearchPreviewWorkerMessage,
};

pub const PROJECT_SEARCH_FILE_CAP_BYTES: u64 = 8 * 1024 * 1024;
pub const PROJECT_SEARCH_MATCH_CAP: usize = 10_000;
pub const PROJECT_SEARCH_FILE_RESULT_CAP: usize = 1_000;
pub const PROJECT_SEARCH_ROW_H: f32 = 24.0;
pub const PROJECT_SEARCH_PAD_X: f32 = 10.0;
pub const PROJECT_SEARCH_QUERY_H: f32 = 78.0;
pub const PROJECT_SEARCH_SINGLE_H: f32 = 30.0;
const PROJECT_SEARCH_QUERY_SCROLLBAR_SIZE: f32 = 10.0;
const PROJECT_SEARCH_QUERY_TEXT_PAD_X: f32 = 7.0;
const PROJECT_SEARCH_QUERY_TEXT_PAD_Y: f32 = 5.0;
const PROJECT_SEARCH_PREVIEW_CHARS: usize = 220;
const PROJECT_SEARCH_PREVIEW_CONTEXT_CHARS: usize = 60;
const PROJECT_SEARCH_BUFFER_KEEP_BYTES: usize = 1024 * 1024;
const PROJECT_SEARCH_MAX_THREADS: usize = 8;

fn project_search_threads_for_available(available: usize) -> usize {
    available.clamp(1, PROJECT_SEARCH_MAX_THREADS)
}

fn project_search_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|threads| project_search_threads_for_available(threads.get()))
        .unwrap_or(4)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectSearchField {
    Query,
    Include,
    Exclude,
    Filter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSearchQueryScrollAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProjectSearchQueryViewport {
    pub(crate) text: ProjectSearchRect,
    pub(crate) vertical_track: ProjectSearchRect,
    pub(crate) horizontal_track: ProjectSearchRect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSearchMatch {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_byte_start: usize,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub preview: String,
    pub preview_match_start: usize,
    pub preview_match_end: usize,
    pub preview_ready: bool,
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
    pub filter: ProjectSearchRect,
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
    pub filter_editor: Editor,
    pub focused: Option<ProjectSearchField>,
    pub case_sensitive: bool,
    pub help_open: bool,
    pub dragging_field: Option<ProjectSearchField>,
    pub dirty: bool,
    pub generation: u64,
    pub running_generation: Option<u64>,
    pub rx: Option<Receiver<ProjectSearchWorkerMessage>>,
    pub worker_cancel: Option<Arc<AtomicBool>>,
    pub preview_tx: Option<Sender<project_search_preview::ProjectSearchPreviewRequest>>,
    pub preview_rx: Option<Receiver<project_search_preview::ProjectSearchPreviewWorkerMessage>>,
    pub preview_pending: FxHashSet<project_search_preview::ProjectSearchPreviewKey>,
    pub results: Vec<ProjectSearchFile>,
    pub flat_rows: Vec<ProjectSearchFlatRow>,
    pub collapsed: FxHashSet<PathKey>,
    pub scroll: ScrollState,
    pub(crate) query_scroll_y: ScrollState,
    pub(crate) query_scroll_x: ScrollState,
    pub(crate) query_content_width: f32,
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
            filter_editor: Editor::new(256),
            focused: None,
            case_sensitive: false,
            help_open: false,
            dragging_field: None,
            dirty: true,
            generation: 0,
            running_generation: None,
            rx: None,
            worker_cancel: None,
            preview_tx: None,
            preview_rx: None,
            preview_pending: FxHashSet::default(),
            results: Vec::new(),
            flat_rows: Vec::new(),
            collapsed: FxHashSet::default(),
            scroll: ScrollState::new(7.0),
            query_scroll_y: ScrollState::new(7.0),
            query_scroll_x: ScrollState::new(7.0),
            query_content_width: 0.0,
            has_run: false,
            total_matches: 0,
            elapsed_ms: None,
            capped: false,
            error: None,
        }
    }
}

impl ProjectSearchState {
    pub(crate) fn advance_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }

    pub(crate) fn cancel_running_worker(&mut self) {
        if let Some(cancel) = self.worker_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.rx = None;
        self.running_generation = None;
    }

    pub(crate) fn handle_worker_disconnect(&mut self) -> bool {
        self.rx = None;
        self.worker_cancel = None;
        if self.running_generation.take().is_some() {
            self.error = Some("Поиск по проекту неожиданно завершился".to_string());
            true
        } else {
            false
        }
    }

    pub(crate) fn handle_preview_disconnect(&mut self) {
        self.preview_tx = None;
        self.preview_rx = None;
        self.preview_pending.clear();
        self.error.get_or_insert_with(||
            "Предпросмотр результатов поиска неожиданно завершился".to_string()
        );
    }

    pub fn filter_enabled(&self) -> bool {
        self.has_run && self.running_generation.is_none() && !self.results.is_empty()
    }

    pub fn filter_active(&self) -> bool {
        self.filter_enabled() && !self.filter_editor.get_full_text().trim().is_empty()
    }

    pub fn rebuild_flat_rows(&mut self) {
        self.flat_rows.clear();
        let filter = self
            .filter_enabled()
            .then(|| self.filter_editor.get_full_text())
            .unwrap_or_default();
        for (file_idx, file) in self.results.iter().enumerate() {
            if !project_search_filter_matches_path(&file.relative_path, &filter) {
                continue;
            }
            self.flat_rows.push(ProjectSearchFlatRow::File(file_idx));
            if !self.collapsed.contains(&PathKey::new(&file.path)) {
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
        let key = PathKey::new(&path);
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
        }
        self.rebuild_flat_rows();
    }

    pub fn apply_live_filter(&mut self) {
        self.rebuild_flat_rows();
        self.scroll.reset();
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

    pub(crate) fn query_max_scroll_y(&self, rect: ProjectSearchRect, scale: f32) -> f32 {
        let viewport = project_search_query_viewport(rect, scale);
        let content_h = self.query_editor.line_offsets.len() as f32
            * project_search_query_line_height(scale);
        (content_h - viewport.text.h).max(0.0)
    }

    pub(crate) fn query_max_scroll_x(&self, rect: ProjectSearchRect, scale: f32) -> f32 {
        let viewport = project_search_query_viewport(rect, scale);
        (self.query_content_width - viewport.text.w).max(0.0)
    }

    pub(crate) fn clamp_query_scrolls(&mut self, rect: ProjectSearchRect, scale: f32) {
        let max_y = self.query_max_scroll_y(rect, scale);
        let max_x = self.query_max_scroll_x(rect, scale);
        self.query_scroll_y.clamp_target(0.0, max_y);
        self.query_scroll_y.clamp_current(0.0, max_y);
        self.query_scroll_x.clamp_target(0.0, max_x);
        self.query_scroll_x.clamp_current(0.0, max_x);
    }

    pub(crate) fn reveal_query_cursor(
        &mut self,
        rect: ProjectSearchRect,
        scale: f32,
        cursor_x: f32,
    ) {
        let viewport = project_search_query_viewport(rect, scale);
        let line_h = project_search_query_line_height(scale);
        let cursor_line = self
            .query_editor
            .line_offsets
            .partition_point(|&offset| offset <= self.query_editor.cursor)
            .saturating_sub(1);
        let cursor_top = cursor_line as f32 * line_h;
        let cursor_bottom = cursor_top + line_h;
        let mut target_y = self.query_scroll_y.current;
        if cursor_top < target_y {
            target_y = cursor_top;
        } else if cursor_bottom > target_y + viewport.text.h {
            target_y = cursor_bottom - viewport.text.h;
        }

        let cursor_w = (2.0 * scale).max(1.0);
        let mut target_x = self.query_scroll_x.current;
        if cursor_x < target_x {
            target_x = cursor_x;
        } else if cursor_x + cursor_w > target_x + viewport.text.w {
            target_x = cursor_x + cursor_w - viewport.text.w;
        }

        let max_y = self.query_max_scroll_y(rect, scale);
        let max_x = self.query_max_scroll_x(rect, scale);
        set_scroll_immediate(&mut self.query_scroll_y, target_y, max_y);
        set_scroll_immediate(&mut self.query_scroll_x, target_x, max_x);
    }

    pub(crate) fn scroll_query_y_by(
        &mut self,
        rect: ProjectSearchRect,
        scale: f32,
        delta: f32,
    ) {
        let max_scroll = self.query_max_scroll_y(rect, scale);
        self.query_scroll_y.anim_speed = 7.0;
        self.query_scroll_y.scroll_by(delta);
        self.query_scroll_y.clamp_target(0.0, max_scroll);
    }

    pub(crate) fn start_query_scrollbar_drag(
        &mut self,
        rect: ProjectSearchRect,
        axis: ProjectSearchQueryScrollAxis,
        pointer: f32,
        scale: f32,
    ) -> bool {
        let Some((drag_offset, target)) =
            project_search_query_scrollbar_drag_target(rect, self, axis, pointer, scale, None)
        else {
            return false;
        };
        let scroll = match axis {
            ProjectSearchQueryScrollAxis::Horizontal => &mut self.query_scroll_x,
            ProjectSearchQueryScrollAxis::Vertical => &mut self.query_scroll_y,
        };
        scroll.jump_to(target);
        scroll.drag_offset = drag_offset;
        scroll.anim_speed = 15.0;
        scroll.is_dragging = true;
        true
    }

    pub(crate) fn drag_query_scrollbar_to(
        &mut self,
        rect: ProjectSearchRect,
        axis: ProjectSearchQueryScrollAxis,
        pointer: f32,
        scale: f32,
    ) -> bool {
        let drag_offset = match axis {
            ProjectSearchQueryScrollAxis::Horizontal => self.query_scroll_x.drag_offset,
            ProjectSearchQueryScrollAxis::Vertical => self.query_scroll_y.drag_offset,
        };
        let Some((_, target)) = project_search_query_scrollbar_drag_target(
            rect,
            self,
            axis,
            pointer,
            scale,
            Some(drag_offset),
        ) else {
            return false;
        };
        let scroll = match axis {
            ProjectSearchQueryScrollAxis::Horizontal => &mut self.query_scroll_x,
            ProjectSearchQueryScrollAxis::Vertical => &mut self.query_scroll_y,
        };
        if (scroll.target - target).abs() < 0.5 {
            return false;
        }
        scroll.jump_to(target);
        scroll.drag_offset = drag_offset;
        scroll.anim_speed = 15.0;
        scroll.is_dragging = true;
        true
    }

    pub fn max_scroll(&self, list_h: f32, scale: f32) -> f32 {
        let row_h = PROJECT_SEARCH_ROW_H * scale;
        (self.flat_rows.len() as f32 * row_h - list_h).max(0.0)
    }
}

fn set_scroll_immediate(scroll: &mut ScrollState, target: f32, max_scroll: f32) {
    let target = target.clamp(0.0, max_scroll);
    scroll.jump_to(target);
}

pub(crate) fn project_search_query_line_height(scale: f32) -> f32 {
    (18.0 * scale).round().max(1.0)
}

pub(crate) fn project_search_query_viewport(
    rect: ProjectSearchRect,
    scale: f32,
) -> ProjectSearchQueryViewport {
    let scrollbar = (PROJECT_SEARCH_QUERY_SCROLLBAR_SIZE * scale)
        .round()
        .max(6.0);
    let pad_x = (PROJECT_SEARCH_QUERY_TEXT_PAD_X * scale).round();
    let pad_y = (PROJECT_SEARCH_QUERY_TEXT_PAD_Y * scale).round();
    let track_pad = (2.0 * scale).round();
    ProjectSearchQueryViewport {
        text: ProjectSearchRect {
            x: rect.x + pad_x,
            y: rect.y + pad_y,
            w: (rect.w - pad_x * 2.0 - scrollbar).max(0.0),
            h: (rect.h - pad_y * 2.0 - scrollbar).max(0.0),
        },
        vertical_track: ProjectSearchRect {
            x: rect.x + rect.w - scrollbar,
            y: rect.y + track_pad,
            w: scrollbar,
            h: (rect.h - scrollbar - track_pad * 2.0).max(0.0),
        },
        horizontal_track: ProjectSearchRect {
            x: rect.x + track_pad,
            y: rect.y + rect.h - scrollbar,
            w: (rect.w - scrollbar - track_pad * 2.0).max(0.0),
            h: scrollbar,
        },
    }
}

pub(crate) fn project_search_line_end(
    text: &str,
    line_start: usize,
    mut line_end: usize,
) -> usize {
    line_end = line_end.min(text.len());
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\n') {
        line_end -= 1;
    }
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\r') {
        line_end -= 1;
    }
    line_end
}

pub(crate) fn project_search_query_scrollbar_thumb(
    rect: ProjectSearchRect,
    state: &ProjectSearchState,
    axis: ProjectSearchQueryScrollAxis,
    scale: f32,
) -> Option<ProjectSearchRect> {
    let viewport = project_search_query_viewport(rect, scale);
    let thickness = (5.0 * scale).round().max(2.0);
    let min_thumb = (18.0 * scale).round().max(8.0);
    match axis {
        ProjectSearchQueryScrollAxis::Vertical => {
            let content_h = state.query_editor.line_offsets.len() as f32
                * project_search_query_line_height(scale);
            let thumb = crate::scroll::scrollbar_thumb(
                viewport.vertical_track.y,
                viewport.vertical_track.h,
                viewport.text.h,
                content_h,
                state.query_scroll_y.current,
                min_thumb,
            )?;
            Some(ProjectSearchRect {
                x: viewport.vertical_track.x
                    + (viewport.vertical_track.w - thickness) * 0.5,
                y: thumb.start,
                w: thickness,
                h: thumb.len,
            })
        }
        ProjectSearchQueryScrollAxis::Horizontal => {
            let thumb = crate::scroll::scrollbar_thumb(
                viewport.horizontal_track.x,
                viewport.horizontal_track.w,
                viewport.text.w,
                state.query_content_width,
                state.query_scroll_x.current,
                min_thumb,
            )?;
            Some(ProjectSearchRect {
                x: thumb.start,
                y: viewport.horizontal_track.y
                    + (viewport.horizontal_track.h - thickness) * 0.5,
                w: thumb.len,
                h: thickness,
            })
        }
    }
}

fn project_search_query_scrollbar_drag_target(
    rect: ProjectSearchRect,
    state: &ProjectSearchState,
    axis: ProjectSearchQueryScrollAxis,
    pointer: f32,
    scale: f32,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    let viewport = project_search_query_viewport(rect, scale);
    let thumb_rect = project_search_query_scrollbar_thumb(rect, state, axis, scale)?;
    match axis {
        ProjectSearchQueryScrollAxis::Vertical => crate::scroll::scrollbar_drag_target(
            pointer,
            viewport.vertical_track.y,
            viewport.vertical_track.h,
            crate::scroll::ScrollbarThumb {
                start: thumb_rect.y,
                len: thumb_rect.h,
            },
            state.query_max_scroll_y(rect, scale),
            drag_offset,
        ),
        ProjectSearchQueryScrollAxis::Horizontal => crate::scroll::scrollbar_drag_target(
            pointer,
            viewport.horizontal_track.x,
            viewport.horizontal_track.w,
            crate::scroll::ScrollbarThumb {
                start: thumb_rect.x,
                len: thumb_rect.w,
            },
            state.query_max_scroll_x(rect, scale),
            drag_offset,
        ),
    }
}

pub fn project_search_layout(
    content_x: f32,
    content_y: f32,
    content_w: f32,
    content_h: f32,
    scale: f32,
) -> ProjectSearchLayout {
    let content_w = content_w.max(0.0);
    let content_h = content_h.max(0.0);
    let pad = (PROJECT_SEARCH_PAD_X * scale).min(content_w * 0.5);
    let gap = 7.0 * scale;
    let desired_button = PROJECT_SEARCH_SINGLE_H * scale;
    let label_h = 18.0 * scale;
    let inner_w = (content_w - pad * 2.0).max(0.0);
    let show_buttons = inner_w >= desired_button * 2.0 + gap * 2.0 + 24.0 * scale;
    let button = if show_buttons { desired_button } else { 0.0 };
    let controls_w = if show_buttons { button * 2.0 + gap * 2.0 } else { 0.0 };
    let mut y = content_y + 9.0 * scale;
    let query_w = (inner_w - controls_w).max(0.0);
    let query = ProjectSearchRect {
        x: content_x + pad,
        y: y + label_h,
        w: query_w,
        h: PROJECT_SEARCH_QUERY_H * scale,
    };
    let case_button = ProjectSearchRect {
        x: query.x + query.w + if show_buttons { gap } else { 0.0 },
        y: query.y,
        w: button,
        h: button,
    };
    let run_button = ProjectSearchRect {
        x: case_button.x + case_button.w + if show_buttons { gap } else { 0.0 },
        y: query.y,
        w: button,
        h: button,
    };
    let help = (22.0 * scale).round().max(18.0).min(inner_w);
    let help_button = ProjectSearchRect {
        x: (content_x + content_w - pad - help).max(content_x + pad),
        y: (query.y - 24.0 * scale).max(content_y + 2.0 * scale),
        w: help.max(0.0),
        h: help.max(0.0),
    };

    y = query.y + query.h + 9.0 * scale;
    let field_w = inner_w.max(0.0);
    let include = ProjectSearchRect {
        x: content_x + pad,
        y: y + label_h,
        w: field_w,
        h: PROJECT_SEARCH_SINGLE_H * scale,
    };
    y = include.y + include.h + 7.0 * scale;
    let exclude = ProjectSearchRect { x: include.x, y: y + label_h, w: field_w, h: include.h };
    y = exclude.y + exclude.h + 22.0 * scale;
    let filter = ProjectSearchRect { x: include.x, y: y + label_h, w: field_w, h: include.h };
    let stats_y = filter.y + filter.h + 26.0 * scale;
    let list_y = stats_y + 8.0 * scale;
    ProjectSearchLayout {
        query,
        include,
        exclude,
        filter,
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
    let grep_pattern = (!unicode_case_fallback
        && !query.as_bytes().contains(&b'\n')
        && !query.as_bytes().contains(&b'\r'))
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
                project_search_grep::search_project_file_grep(
                    path,
                    &plan,
                    needle.as_slice(),
                    matcher,
                    searcher,
                    case_sensitive,
                    &caps,
                    &profile,
                    &capped_flag,
                )
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
    let reached_cap = {
        let mut caps = crate::platform::lock_recover(caps);
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
        .map(crate::app::file_icons::file_icon_key_for_name)
        .unwrap_or("default_file");
    Some(ProjectSearchFile {
        path: path.to_path_buf(),
        relative_path,
        icon_key,
        matches,
    })
}

fn project_search_text(buf: &[u8]) -> Option<Cow<'_, str>> {
    let has_text_bom = buf.starts_with(&[0xef, 0xbb, 0xbf])
        || buf.starts_with(&[0xff, 0xfe])
        || buf.starts_with(&[0xfe, 0xff]);
    if !has_text_bom && memchr::memchr(b'\0', buf).is_some() {
        return None;
    }
    if !has_text_bom && memchr::memchr(b'\r', buf).is_none() {
        return std::str::from_utf8(buf).ok().map(Cow::Borrowed);
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
            .find_map(|workspace| platform::relative_to(path, workspace))
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

#[derive(Default)]
struct ProjectSearchLineCursor {
    scan: usize,
    line: u32,
    line_start: usize,
}

impl ProjectSearchLineCursor {
    fn lsp_pos(&mut self, text: &str, offset: usize) -> (u32, u32, usize) {
        let offset = offset.min(text.len());
        if offset < self.scan {
            *self = Self::default();
        }
        let bytes = text.as_bytes();
        while self.scan < offset {
            if bytes.get(self.scan) == Some(&b'\n') {
                self.line = self.line.saturating_add(1);
                self.line_start = self.scan + 1;
            }
            self.scan += 1;
        }
        (
            self.line,
            utf16_units_between(text, self.line_start, offset),
            self.line_start,
        )
    }
}

fn push_match(
    text: &str,
    start: usize,
    end: usize,
    cursor: &mut ProjectSearchLineCursor,
    matches: &mut Vec<ProjectSearchMatch>,
) {
    let start = floor_char_boundary(text, start.min(text.len()));
    let end = ceil_char_boundary(text, end.min(text.len()));
    let (start_line, start_col, line_start) = cursor.lsp_pos(text, start);
    let (end_line, end_col, _) = cursor.lsp_pos(text, end);
    let extra_lines = end_line.saturating_sub(start_line) as usize;
    matches.push(ProjectSearchMatch {
        byte_start: start,
        byte_end: end,
        line_byte_start: line_start,
        start_line,
        start_col,
        end_line,
        end_col,
        preview: String::new(),
        preview_match_start: 0,
        preview_match_end: 0,
        preview_ready: false,
        extra_lines,
    });
}

fn utf16_units_between(text: &str, start: usize, end: usize) -> u32 {
    let Some(slice) = text.get(start.min(text.len())..end.min(text.len())) else {
        return 0;
    };
    if slice.is_ascii() {
        slice.len() as u32
    } else {
        slice.chars().map(|ch| ch.len_utf16() as u32).sum()
    }
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
    let (segment_start, segment_end) = if line.len() <= PROJECT_SEARCH_PREVIEW_CHARS {
        (0, line.len())
    } else {
        let available = PROJECT_SEARCH_PREVIEW_CHARS.saturating_sub(6).max(32);
        let mut first = start.saturating_sub(PROJECT_SEARCH_PREVIEW_CONTEXT_CHARS);
        let mut last = (first + available).min(line.len());
        if end > last {
            last = end.min(line.len());
            first = last.saturating_sub(available);
        }
        (
            floor_char_boundary(line, first),
            ceil_char_boundary(line, last),
        )
    };
    let mut preview = String::with_capacity((segment_end - segment_start).min(line.len()) + 6);
    if segment_start > 0 {
        preview.push_str("...");
    }
    let prefix_len = preview.len();
    if let Some(segment) = line.get(segment_start..segment_end) {
        if segment.as_bytes().contains(&b'\t') {
            for ch in segment.chars() {
                preview.push(if ch == '\t' { ' ' } else { ch });
            }
        } else {
            preview.push_str(segment);
        }
    }
    let match_start = prefix_len + start.max(segment_start).min(segment_end) - segment_start;
    let match_end = prefix_len + end.max(segment_start).min(segment_end) - segment_start;
    if segment_end < line.len() {
        preview.push_str("...");
    }
    (preview, match_start, match_end.max(match_start))
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
            let prefix_match = self
                .include_roots
                .iter()
                .any(|root| platform::path_is_within(path, root));
            let glob_match = self
                .include_globs
                .as_ref()
                .is_some_and(|set| set.is_match(to_slash(&rel)));
            if !prefix_match && !glob_match {
                return false;
            }
        } else if !self.include_all
            && !self
                .include_roots
                .iter()
                .any(|root| platform::path_is_within(path, root))
        {
            return false;
        }
        if !platform::path_is_within(path, workspace) {
            return false;
        }
        if self
            .exclude_roots
            .iter()
            .any(|root| platform::path_is_within(path, root))
        {
            return false;
        }
        if self
            .exclude_globs
            .as_ref()
            .is_some_and(|set| set.is_match(to_slash(&rel)))
        {
            return false;
        }
        true
    }

    fn workspace_relative<'a>(&'a self, path: &Path) -> Option<(&'a Path, PathBuf)> {
        self.workspaces.iter().find_map(|workspace| {
            platform::relative_to(path, workspace)
                .map(|relative| (workspace.as_path(), relative))
        })
    }

    fn relative_display(&self, path: &Path) -> String {
        if let Some((workspace, rel)) = self.workspace_relative(path) {
            let workspace_name = workspace
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace");
            let rel = to_slash(&rel);
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
        let path = platform::canonicalize_or_absolutize(workspace);
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
    if platform::is_absolute(raw) {
        let path = platform::canonicalize_or_absolutize(raw);
        if workspaces
            .iter()
            .any(|workspace| platform::path_is_within(&path, workspace))
        {
            return vec![path];
        }
        return Vec::new();
    }
    let rel = token.strip_prefix("./").unwrap_or(token);
    let rel = if rel == "." { "" } else { rel };
    workspaces
        .iter()
        .map(|workspace| platform::canonicalize_or_absolutize(&workspace.join(rel)))
        .collect()
}

fn glob_patterns_for_token(token: &str, workspaces: &[PathBuf]) -> Vec<String> {
    let token = token.trim();
    let raw = Path::new(token);
    if platform::is_absolute(raw) {
        let mut out = Vec::new();
        for workspace in workspaces {
            if let Some(rel) = platform::relative_to(raw, workspace) {
                let pattern = to_slash(&rel);
                if !pattern.is_empty() {
                    out.push(pattern);
                }
            }
        }
        out
    } else {
        vec![to_slash(Path::new(
            token.strip_prefix("./").unwrap_or(token),
        ))]
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths
        .iter()
        .any(|existing| platform::paths_equal(existing, &path))
    {
        paths.push(path);
    }
}

fn to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn project_search_filter_matches_path(relative_path: &str, filter: &str) -> bool {
    let tokens = split_pattern_tokens(filter);
    if tokens.is_empty() {
        return true;
    }
    let path = relative_path.to_lowercase();
    tokens
        .iter()
        .any(|token| project_search_filter_token_matches_path(&path, token))
}

fn project_search_filter_token_matches_path(path: &str, token: &str) -> bool {
    let token = token.trim().to_lowercase();
    if token.is_empty() {
        return true;
    }
    if let Some(suffix) = token.strip_prefix("*.").filter(|suffix| {
        !suffix.is_empty()
            && !suffix.as_bytes().contains(&b'*')
            && !suffix.contains('/')
            && !suffix.contains('\\')
    }) {
        return path
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.as_bytes().last() == Some(&b'.'));
    }
    if token.as_bytes().contains(&b'*') {
        return false;
    }
    path.contains(&token)
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
        let outside = root
            .parent()
            .unwrap_or_else(|| Path::new(std::path::MAIN_SEPARATOR_STR))
            .join("rriter-not-in-workspace");
        let excluded = outside.join("also-outside");
        let plan = SearchPatternPlan::new(
            &[root.clone()],
            &format!("{}, {}", root.join("src").display(), outside.display()),
            &excluded.to_string_lossy(),
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
    fn project_search_preserves_positions_for_crlf_bom_and_utf16_files() {
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
            ),
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
            ),
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
        assert_eq!(result.total_matches, 3);
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
    fn project_search_thread_count_never_oversubscribes_small_cpus() {
        assert_eq!(project_search_threads_for_available(1), 1);
        assert_eq!(project_search_threads_for_available(2), 2);
        assert_eq!(project_search_threads_for_available(4), 4);
        assert_eq!(
            project_search_threads_for_available(16),
            PROJECT_SEARCH_MAX_THREADS
        );
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
                    line_byte_start: 0,
                    start_line: 0,
                    start_col: 0,
                    end_line: 0,
                    end_col: 1,
                    preview: "a".to_string(),
                    preview_match_start: 0,
                    preview_match_end: 1,
                    preview_ready: true,
                    extra_lines: 0,
                },
                ProjectSearchMatch {
                    byte_start: 2,
                    byte_end: 3,
                    line_byte_start: 2,
                    start_line: 1,
                    start_col: 0,
                    end_line: 1,
                    end_col: 1,
                    preview: "b".to_string(),
                    preview_match_start: 0,
                    preview_match_end: 1,
                    preview_ready: true,
                    extra_lines: 0,
                },
            ],
        });
        state.rebuild_flat_rows();
        assert_eq!(state.flat_rows.len(), 3);
        state.toggle_file(0);
        assert_eq!(state.flat_rows, vec![ProjectSearchFlatRow::File(0)]);
    }

    #[test]
    fn project_search_live_filter_rebuilds_visible_rows_only() {
        let mut state = ProjectSearchState::default();
        state.has_run = true;
        state.results.push(ProjectSearchFile {
            path: PathBuf::from("/w/src/a.rs"),
            relative_path: "src/a.rs".to_string(),
            icon_key: "rust",
            matches: Vec::new(),
        });
        state.results.push(ProjectSearchFile {
            path: PathBuf::from("/w/src/a.py"),
            relative_path: "src/a.py".to_string(),
            icon_key: "python",
            matches: Vec::new(),
        });
        state.filter_editor.insert_str("*.rs");

        state.apply_live_filter();

        assert_eq!(state.flat_rows, vec![ProjectSearchFlatRow::File(0)]);
    }

    #[test]
    fn query_scroll_reveals_cursor_and_exposes_both_scrollbars() {
        let mut state = ProjectSearchState::default();
        state
            .query_editor
            .insert_str("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight");
        state.query_editor.cursor = state.query_editor.len();
        state.query_content_width = 420.0;
        let rect = ProjectSearchRect {
            x: 0.0,
            y: 0.0,
            w: 180.0,
            h: PROJECT_SEARCH_QUERY_H,
        };

        state.reveal_query_cursor(rect, 1.0, 390.0);

        assert!(state.query_scroll_y.current > 0.0);
        assert!(state.query_scroll_x.current > 0.0);
        assert!(project_search_query_scrollbar_thumb(
            rect,
            &state,
            ProjectSearchQueryScrollAxis::Vertical,
            1.0,
        )
        .is_some());
        assert!(project_search_query_scrollbar_thumb(
            rect,
            &state,
            ProjectSearchQueryScrollAxis::Horizontal,
            1.0,
        )
        .is_some());
    }

    #[test]
    fn query_scrollbar_drag_reuses_shared_scroll_math() {
        let mut state = ProjectSearchState::default();
        state.query_editor.insert_str("a\nb\nc\nd\ne\nf\ng\nh");
        state.query_content_width = 400.0;
        let rect = ProjectSearchRect {
            x: 10.0,
            y: 20.0,
            w: 180.0,
            h: PROJECT_SEARCH_QUERY_H,
        };
        let viewport = project_search_query_viewport(rect, 1.0);

        assert!(state.start_query_scrollbar_drag(
            rect,
            ProjectSearchQueryScrollAxis::Vertical,
            viewport.vertical_track.y + viewport.vertical_track.h,
            1.0,
        ));
        assert!(state.query_scroll_y.target > 0.0);
        assert_eq!(state.query_scroll_y.current, state.query_scroll_y.target);
        assert!(state.query_scroll_y.is_dragging);

        assert!(state.start_query_scrollbar_drag(
            rect,
            ProjectSearchQueryScrollAxis::Horizontal,
            viewport.horizontal_track.x + viewport.horizontal_track.w,
            1.0,
        ));
        assert!(state.query_scroll_x.target > 0.0);
        assert_eq!(state.query_scroll_x.current, state.query_scroll_x.target);
        assert!(state.query_scroll_x.is_dragging);
    }

    #[test]
    fn query_wheel_scroll_clamps_vertical_target() {
        let mut state = ProjectSearchState::default();
        state.query_editor.insert_str("a\nb\nc\nd\ne\nf\ng\nh");
        let rect = ProjectSearchRect {
            x: 0.0,
            y: 0.0,
            w: 180.0,
            h: PROJECT_SEARCH_QUERY_H,
        };
        let max_scroll = state.query_max_scroll_y(rect, 1.0);

        state.scroll_query_y_by(rect, 1.0, 10_000.0);
        assert_eq!(state.query_scroll_y.target, max_scroll);

        state.scroll_query_y_by(rect, 1.0, -10_000.0);
        assert_eq!(state.query_scroll_y.target, 0.0);
    }
}
