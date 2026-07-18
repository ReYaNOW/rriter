use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use super::{
    PROJECT_SEARCH_ROW_H, ProjectSearchFlatRow, ProjectSearchLayout, ProjectSearchMatch,
    ProjectSearchRect, ProjectSearchState, preview_line_with_match,
};

const PROJECT_SEARCH_PREVIEW_REQUEST_BUDGET: usize = 96;
const PROJECT_SEARCH_PREVIEW_PREFETCH_ROWS: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProjectSearchPreviewKey {
    pub file_idx: usize,
    pub match_idx: usize,
}

#[derive(Clone, Debug)]
pub struct ProjectSearchPreviewRequest {
    pub generation: u64,
    pub key: ProjectSearchPreviewKey,
    pub path: PathBuf,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Clone, Debug)]
pub enum ProjectSearchPreviewWorkerMessage {
    Preview {
        generation: u64,
        key: ProjectSearchPreviewKey,
        preview: String,
        preview_match_start: usize,
        preview_match_end: usize,
    },
}

pub struct ProjectSearchPreviewWorker {
    pub tx: Sender<ProjectSearchPreviewRequest>,
    pub rx: Receiver<ProjectSearchPreviewWorkerMessage>,
}

pub fn start_project_search_preview_worker() -> std::io::Result<ProjectSearchPreviewWorker> {
    let (request_tx, request_rx) = channel::<ProjectSearchPreviewRequest>();
    let (message_tx, message_rx) = channel::<ProjectSearchPreviewWorkerMessage>();
    crate::platform::spawn_named("rriter-project-search-preview", move || {
        run_project_search_preview_worker(request_rx, message_tx);
    })?;
    Ok(ProjectSearchPreviewWorker {
        tx: request_tx,
        rx: message_rx,
    })
}

impl ProjectSearchState {
    pub fn reset_preview_worker(&mut self) {
        self.preview_tx = None;
        self.preview_rx = None;
        self.preview_pending.clear();
    }

    pub fn start_preview_worker(&mut self) {
        self.reset_preview_worker();
        match start_project_search_preview_worker() {
            Ok(worker) => {
                self.preview_tx = Some(worker.tx);
                self.preview_rx = Some(worker.rx);
            }
            Err(err) => {
                self.error = Some(format!("Не удалось запустить предпросмотр поиска: {err}"));
            }
        }
    }

    pub fn has_pending_previews(&self) -> bool {
        !self.preview_pending.is_empty()
    }

    pub fn apply_preview_message(&mut self, message: ProjectSearchPreviewWorkerMessage) -> bool {
        let ProjectSearchPreviewWorkerMessage::Preview {
            generation,
            key,
            preview,
            preview_match_start,
            preview_match_end,
        } = message;
        if generation != self.generation {
            return false;
        }
        self.preview_pending.remove(&key);
        let Some(mat) = self
            .results
            .get_mut(key.file_idx)
            .and_then(|file| file.matches.get_mut(key.match_idx))
        else {
            return false;
        };
        if mat.preview_ready {
            return false;
        }
        mat.preview = preview;
        mat.preview_match_start = preview_match_start;
        mat.preview_match_end = preview_match_end.max(preview_match_start);
        mat.preview_ready = true;
        true
    }

    pub fn queue_visible_preview_requests(
        &mut self,
        layout: &ProjectSearchLayout,
        scale: f32,
    ) -> bool {
        if self.flat_rows.is_empty() || self.preview_tx.is_none() {
            return false;
        }
        let row_h = PROJECT_SEARCH_ROW_H * scale;
        let range = visible_preview_row_range(
            self.scroll.current.max(self.scroll.target),
            layout.list.h,
            row_h,
            self.flat_rows.len(),
        );
        if range.is_empty() {
            return false;
        }
        let Some(tx) = self.preview_tx.as_ref().cloned() else {
            return false;
        };
        let mut sent = 0usize;
        for row_idx in range {
            let ProjectSearchFlatRow::Match(file_idx, match_idx) = self.flat_rows[row_idx] else {
                continue;
            };
            let key = ProjectSearchPreviewKey {
                file_idx,
                match_idx,
            };
            if self.preview_pending.contains(&key) {
                continue;
            }
            let Some((path, mat)) = self.results.get(file_idx).and_then(|file| {
                file.matches
                    .get(match_idx)
                    .map(|mat| (file.path.clone(), mat))
            }) else {
                continue;
            };
            if mat.preview_ready {
                continue;
            }
            let request = preview_request_for_match(self.generation, key, path, mat);
            if tx.send(request).is_err() {
                self.reset_preview_worker();
                self.start_preview_worker();
                self.error.get_or_insert_with(||
                    "Предпросмотр поиска был перезапущен после сбоя".to_string()
                );
                return sent > 0;
            }
            self.preview_pending.insert(key);
            sent += 1;
            if sent >= PROJECT_SEARCH_PREVIEW_REQUEST_BUDGET {
                break;
            }
        }
        sent > 0
    }

    pub fn start_scrollbar_drag(
        &mut self,
        layout: &ProjectSearchLayout,
        mouse_y: f32,
        scale: f32,
    ) -> bool {
        let Some((drag_offset, target)) =
            project_search_scrollbar_drag_target(mouse_y, layout, self, scale, None)
        else {
            return false;
        };
        self.scroll.drag_offset = drag_offset;
        self.scroll.target = target;
        self.scroll.anim_speed = 15.0;
        self.scroll.is_dragging = true;
        true
    }

    pub fn drag_scrollbar_to(
        &mut self,
        layout: &ProjectSearchLayout,
        mouse_y: f32,
        scale: f32,
    ) -> bool {
        let Some((_, target)) = project_search_scrollbar_drag_target(
            mouse_y,
            layout,
            self,
            scale,
            Some(self.scroll.drag_offset),
        ) else {
            return false;
        };
        if (self.scroll.target - target).abs() < 0.5 {
            return false;
        }
        self.scroll.target = target;
        self.scroll.anim_speed = 15.0;
        true
    }
}

fn preview_request_for_match(
    generation: u64,
    key: ProjectSearchPreviewKey,
    path: PathBuf,
    mat: &ProjectSearchMatch,
) -> ProjectSearchPreviewRequest {
    ProjectSearchPreviewRequest {
        generation,
        key,
        path,
        start_line: mat.start_line,
        start_col: mat.start_col,
        end_line: mat.end_line,
        end_col: mat.end_col,
    }
}

fn run_project_search_preview_worker(
    rx: Receiver<ProjectSearchPreviewRequest>,
    tx: Sender<ProjectSearchPreviewWorkerMessage>,
) {
    let mut cached_path: Option<PathBuf> = None;
    let mut cached_text = String::new();
    while let Ok(request) = rx.recv() {
        let cache_matches = cached_path
            .as_ref()
            .is_some_and(|path| crate::platform::paths_equal(path, &request.path));
        if !cache_matches {
            match read_project_search_preview_text(&request.path) {
                Some(text) => {
                    cached_path = Some(request.path.clone());
                    cached_text = text;
                }
                None => {
                    cached_path = None;
                    cached_text.clear();
                }
            }
        }
        let (preview, preview_match_start, preview_match_end) = if cached_path.is_some() {
            build_project_search_preview(&request, &cached_text)
                .unwrap_or_else(fallback_project_search_preview)
        } else {
            fallback_project_search_preview()
        };
        let _ = tx.send(ProjectSearchPreviewWorkerMessage::Preview {
            generation: request.generation,
            key: request.key,
            preview,
            preview_match_start,
            preview_match_end,
        });
    }
}

fn read_project_search_preview_text(path: &std::path::Path) -> Option<String> {
    crate::platform::read_text_file(path)
        .ok()
        .map(|decoded| decoded.text)
}

fn fallback_project_search_preview() -> (String, usize, usize) {
    ("...".to_string(), 0, 0)
}

fn build_project_search_preview(
    request: &ProjectSearchPreviewRequest,
    text: &str,
) -> Option<(String, usize, usize)> {
    let (line_start, line_end) = project_search_line_bounds(text, request.start_line)?;
    let line = text.get(line_start..line_end)?;
    let local_start = utf16_column_to_byte(line, request.start_col);
    let local_end = if request.end_line == request.start_line {
        utf16_column_to_byte(line, request.end_col)
    } else {
        line.len()
    };
    Some(preview_line_with_match(
        line,
        local_start,
        local_end.max(local_start),
    ))
}

fn project_search_line_bounds(text: &str, line: u32) -> Option<(usize, usize)> {
    let mut start = 0usize;
    for _ in 0..line {
        let next = memchr::memchr(b'\n', text.as_bytes().get(start..)?)?;
        start = start.saturating_add(next).saturating_add(1);
    }
    let end = memchr::memchr(b'\n', text.as_bytes().get(start..)?)
        .map(|offset| start + offset)
        .unwrap_or(text.len());
    Some((start, end))
}

fn utf16_column_to_byte(line: &str, column: u32) -> usize {
    let mut utf16 = 0u32;
    for (idx, ch) in line.char_indices() {
        if utf16 >= column {
            return idx;
        }
        utf16 = utf16.saturating_add(ch.len_utf16() as u32);
    }
    line.len()
}

fn visible_preview_row_range(
    scroll: f32,
    list_h: f32,
    row_h: f32,
    total_rows: usize,
) -> Range<usize> {
    if total_rows == 0 || list_h <= 0.0 || row_h <= 0.0 {
        return 0..0;
    }
    let first = (scroll / row_h).floor().max(0.0) as usize;
    let visible = (list_h / row_h).ceil().max(1.0) as usize;
    let start = first.saturating_sub(PROJECT_SEARCH_PREVIEW_PREFETCH_ROWS / 4);
    let end = first
        .saturating_add(visible)
        .saturating_add(PROJECT_SEARCH_PREVIEW_PREFETCH_ROWS)
        .min(total_rows);
    start..end
}

pub fn project_search_scrollbar_thumb(
    layout: &ProjectSearchLayout,
    state: &ProjectSearchState,
    scale: f32,
) -> Option<ProjectSearchRect> {
    if !state.has_run || state.running_generation.is_some() {
        return None;
    }
    let row_h = PROJECT_SEARCH_ROW_H * scale;
    let total_h = state.flat_rows.len() as f32 * row_h;
    if total_h <= layout.list.h || layout.list.h <= 0.0 {
        return None;
    }
    let thumb = crate::scroll::scrollbar_thumb(
        layout.list.y,
        layout.list.h,
        layout.list.h,
        total_h,
        state.scroll.current,
        22.0 * scale,
    )?;
    Some(ProjectSearchRect {
        x: layout.list.x + layout.list.w - 10.0 * scale,
        y: thumb.start,
        w: 5.0 * scale,
        h: thumb.len,
    })
}

fn project_search_scrollbar_drag_target(
    mouse_y: f32,
    layout: &ProjectSearchLayout,
    state: &ProjectSearchState,
    scale: f32,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    let thumb = project_search_scrollbar_thumb(layout, state, scale)?;
    let row_h = PROJECT_SEARCH_ROW_H * scale;
    let total_h = state.flat_rows.len() as f32 * row_h;
    let max_scroll = (total_h - layout.list.h).max(0.0);
    if max_scroll <= 0.0 {
        return None;
    }
    crate::scroll::scrollbar_drag_target(
        mouse_y,
        layout.list.y,
        layout.list.h,
        crate::scroll::ScrollbarThumb {
            start: thumb.y,
            len: thumb.h,
        },
        max_scroll,
        drag_offset,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_row_range_prefetches_visible_without_exceeding_rows() {
        let range = visible_preview_row_range(240.0, 120.0, 24.0, 20);
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 20);

        let empty = visible_preview_row_range(0.0, 0.0, 24.0, 20);
        assert!(empty.is_empty());
    }

    #[test]
    fn preview_line_bounds_follow_normalized_lines() {
        assert_eq!(project_search_line_bounds("abc\nnext", 0), Some((0, 3)));
        assert_eq!(project_search_line_bounds("abc\nnext", 1), Some((4, 8)));
        assert_eq!(project_search_line_bounds("abc\n", 1), Some((4, 4)));
        assert_eq!(project_search_line_bounds("abc", 1), None);
    }

    #[test]
    fn preview_uses_utf16_columns_and_multiline_extent() {
        let request = ProjectSearchPreviewRequest {
            generation: 1,
            key: ProjectSearchPreviewKey {
                file_idx: 0,
                match_idx: 0,
            },
            path: PathBuf::from("unused"),
            start_line: 1,
            start_col: 3,
            end_line: 1,
            end_col: 9,
        };
        let (preview, start, end) =
            build_project_search_preview(&request, "first\na😀needle tail").unwrap();
        assert_eq!(&preview[start..end], "needle");

        let multiline = ProjectSearchPreviewRequest {
            end_line: 2,
            end_col: 2,
            ..request
        };
        let (preview, start, end) =
            build_project_search_preview(&multiline, "first\na😀needle tail\nxx").unwrap();
        assert_eq!(&preview[start..end], "needle tail");
    }

    #[test]
    fn preview_reader_decodes_utf16_and_normalizes_crlf() {
        let path = std::env::temp_dir().join(format!(
            "rriter_project_search_preview_{}_{}.txt",
            std::process::id(),
            crate::platform::CURRENT_PLATFORM as u8
        ));
        let format = crate::platform::TextFileFormat {
            encoding: crate::platform::TextEncoding::Utf16Le,
            line_ending: crate::platform::LineEnding::CrLf,
        };
        std::fs::write(&path, crate::platform::encode_text("first\nneedle", format)).unwrap();
        assert_eq!(
            read_project_search_preview_text(&path).as_deref(),
            Some("first\nneedle")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scrollbar_drag_target_keeps_ratio_stable() {
        let mut state = ProjectSearchState::default();
        state.flat_rows = (0..100)
            .map(|idx| {
                if idx % 2 == 0 {
                    ProjectSearchFlatRow::File(0)
                } else {
                    ProjectSearchFlatRow::Match(0, 0)
                }
            })
            .collect();
        state.has_run = true;
        state.scroll.current = 240.0;
        let layout = ProjectSearchLayout {
            query: ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            include: ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            exclude: ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            filter: ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            case_button: ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            run_button: ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            help_button: ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            stats_y: 0.0,
            list: ProjectSearchRect {
                x: 10.0,
                y: 20.0,
                w: 200.0,
                h: 240.0,
            },
        };
        let Some((offset, target)) =
            project_search_scrollbar_drag_target(100.0, &layout, &state, 1.0, None)
        else {
            panic!("scrollbar should be visible");
        };
        assert!(offset >= 0.0);
        assert!(target >= 0.0);
    }
}
