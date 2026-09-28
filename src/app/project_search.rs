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
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[path = "project_search_grep.rs"]
mod project_search_grep;
#[path = "project_search_preview.rs"]
mod project_search_preview;
pub(crate) use project_search_preview::project_search_scrollbar;
#[cfg(test)]
pub(crate) use project_search_preview::{
    ProjectSearchPreviewKey, ProjectSearchPreviewRequest, ProjectSearchPreviewWorkerMessage,
};

include!("project_search_matcher.rs");
include!("project_search_engine.rs");
include!("project_search_input_state.rs");

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
        self.error.get_or_insert_with(|| {
            "Предпросмотр результатов поиска неожиданно завершился".to_string()
        });
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
        let content_h =
            self.query_editor.line_offsets.len() as f32 * project_search_query_line_height(scale);
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

    pub(crate) fn scroll_query_y_by(&mut self, rect: ProjectSearchRect, scale: f32, delta: f32) {
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
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset)
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
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset)
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

pub(crate) fn project_search_line_end(text: &str, line_start: usize, mut line_end: usize) -> usize {
    line_end = line_end.min(text.len());
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\n') {
        line_end -= 1;
    }
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\r') {
        line_end -= 1;
    }
    line_end
}

pub(crate) fn project_search_query_scrollbar(
    rect: ProjectSearchRect,
    state: &ProjectSearchState,
    axis: ProjectSearchQueryScrollAxis,
    scale: f32,
) -> Option<crate::render_view::scrollbar_widget::Scrollbar> {
    use crate::render_view::scrollbar_widget::{Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle};
    let viewport = project_search_query_viewport(rect, scale);
    let thickness = (5.0 * scale).round().max(2.0);
    let min_thumb = (18.0 * scale).round().max(8.0);
    match axis {
        ProjectSearchQueryScrollAxis::Vertical => {
            let content_h = state.query_editor.line_offsets.len() as f32
                * project_search_query_line_height(scale);
            Some(Scrollbar {
                style: ScrollbarStyle {
                    thumb_thickness: thickness,
                    edge_gap: None,
                    track_pad: 0.0,
                    min_thumb,
                    radius: Some(3.0 * scale),
                    track_color: Some([1.0, 1.0, 1.0, 0.035]),
                    thumb_color: [0.48, 0.48, 0.56, 0.68],
                },
                axis: ScrollbarAxis::Vertical,
                lane: (viewport.vertical_track.x, viewport.vertical_track.y,
                    viewport.vertical_track.w, viewport.vertical_track.h),
                extent: ScrollbarExtent::new(
                    viewport.text.h, content_h, state.query_scroll_y.current,
                ),
            })
        }
        ProjectSearchQueryScrollAxis::Horizontal => {
            Some(Scrollbar {
                style: ScrollbarStyle {
                    thumb_thickness: thickness,
                    edge_gap: None,
                    track_pad: 0.0,
                    min_thumb,
                    radius: Some(3.0 * scale),
                    track_color: Some([1.0, 1.0, 1.0, 0.035]),
                    thumb_color: [0.48, 0.48, 0.56, 0.68],
                },
                axis: ScrollbarAxis::Horizontal,
                lane: (viewport.horizontal_track.x, viewport.horizontal_track.y,
                    viewport.horizontal_track.w, viewport.horizontal_track.h),
                extent: ScrollbarExtent::new(
                    viewport.text.w, state.query_content_width, state.query_scroll_x.current,
                ),
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
    let geometry = project_search_query_scrollbar(rect, state, axis, scale)?.geometry(1.0)?;
    match axis {
        ProjectSearchQueryScrollAxis::Vertical | ProjectSearchQueryScrollAxis::Horizontal => {
            let grab_offset = drag_offset.unwrap_or_else(|| {
                geometry.press_target(pointer).map_or(0.0, |(offset, _)| offset)
            });
            geometry.drag_target(pointer, grab_offset).map(|target| (grab_offset, target))
        }
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
    let controls_w = if show_buttons {
        button * 2.0 + gap * 2.0
    } else {
        0.0
    };
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
    let exclude = ProjectSearchRect {
        x: include.x,
        y: y + label_h,
        w: field_w,
        h: include.h,
    };
    y = exclude.y + exclude.h + 22.0 * scale;
    let filter = ProjectSearchRect {
        x: include.x,
        y: y + label_h,
        w: field_w,
        h: include.h,
    };
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
