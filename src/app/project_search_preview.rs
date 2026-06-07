use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use super::{
    PROJECT_SEARCH_ROW_H, ProjectSearchFlatRow, ProjectSearchLayout, ProjectSearchMatch,
    ProjectSearchRect, ProjectSearchState, preview_line_with_match,
};

const PROJECT_SEARCH_PREVIEW_REQUEST_BUDGET: usize = 96;
const PROJECT_SEARCH_PREVIEW_PREFETCH_ROWS: usize = 80;
const PROJECT_SEARCH_PREVIEW_LINE_CAP_BYTES: u64 = 256 * 1024;

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
    pub line_byte_start: usize,
    pub byte_start: usize,
    pub byte_end: usize,
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

pub fn start_project_search_preview_worker() -> ProjectSearchPreviewWorker {
    let (request_tx, request_rx) = channel::<ProjectSearchPreviewRequest>();
    let (message_tx, message_rx) = channel::<ProjectSearchPreviewWorkerMessage>();
    std::thread::spawn(move || run_project_search_preview_worker(request_rx, message_tx));
    ProjectSearchPreviewWorker {
        tx: request_tx,
        rx: message_rx,
    }
}

impl ProjectSearchState {
    pub fn reset_preview_worker(&mut self) {
        self.preview_tx = None;
        self.preview_rx = None;
        self.preview_pending.clear();
    }

    pub fn start_preview_worker(&mut self) {
        self.reset_preview_worker();
        let worker = start_project_search_preview_worker();
        self.preview_tx = Some(worker.tx);
        self.preview_rx = Some(worker.rx);
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
        line_byte_start: mat.line_byte_start,
        byte_start: mat.byte_start,
        byte_end: mat.byte_end,
    }
}

fn run_project_search_preview_worker(
    rx: Receiver<ProjectSearchPreviewRequest>,
    tx: Sender<ProjectSearchPreviewWorkerMessage>,
) {
    while let Ok(request) = rx.recv() {
        let (preview, preview_match_start, preview_match_end) =
            build_project_search_preview(&request).unwrap_or_else(fallback_project_search_preview);
        let _ = tx.send(ProjectSearchPreviewWorkerMessage::Preview {
            generation: request.generation,
            key: request.key,
            preview,
            preview_match_start,
            preview_match_end,
        });
    }
}

fn fallback_project_search_preview() -> (String, usize, usize) {
    ("...".to_string(), 0, 0)
}

fn build_project_search_preview(
    request: &ProjectSearchPreviewRequest,
) -> Option<(String, usize, usize)> {
    if request.byte_end < request.byte_start || request.byte_start < request.line_byte_start {
        return None;
    }
    let mut file = std::fs::File::open(&request.path).ok()?;
    file.seek(SeekFrom::Start(request.line_byte_start as u64))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(PROJECT_SEARCH_PREVIEW_LINE_CAP_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let line_len = project_search_preview_line_len(&bytes);
    bytes.truncate(line_len);
    let line = std::str::from_utf8(&bytes).ok()?;
    let local_start = request.byte_start.saturating_sub(request.line_byte_start);
    let local_end = request.byte_end.saturating_sub(request.line_byte_start);
    if local_start > line.len() {
        return None;
    }
    Some(preview_line_with_match(line, local_start, local_end))
}

fn project_search_preview_line_len(bytes: &[u8]) -> usize {
    let mut len = memchr::memchr(b'\n', bytes).unwrap_or(bytes.len());
    if len > 0 && bytes.get(len - 1) == Some(&b'\r') {
        len -= 1;
    }
    len
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
    let max_scroll = (total_h - layout.list.h).max(0.0);
    let ratio = (state.scroll.current / max_scroll.max(1.0)).clamp(0.0, 1.0);
    let track_h = layout.list.h;
    let thumb_h = (layout.list.h / total_h * track_h).max(22.0 * scale);
    let thumb_y = layout.list.y + ratio * (track_h - thumb_h);
    Some(ProjectSearchRect {
        x: layout.list.x + layout.list.w - 10.0 * scale,
        y: thumb_y,
        w: 5.0 * scale,
        h: thumb_h,
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
    let offset = drag_offset.unwrap_or_else(|| {
        if mouse_y >= thumb.y && mouse_y <= thumb.y + thumb.h {
            mouse_y - thumb.y
        } else {
            thumb.h * 0.5
        }
    });
    let denom = (layout.list.h - thumb.h).max(0.0001);
    let ratio = (mouse_y - layout.list.y - offset) / denom;
    Some((offset, (ratio * max_scroll).clamp(0.0, max_scroll)))
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
    fn project_search_preview_line_len_trims_newline_and_crlf() {
        assert_eq!(project_search_preview_line_len(b"abc\nnext"), 3);
        assert_eq!(project_search_preview_line_len(b"abc\r\nnext"), 3);
        assert_eq!(project_search_preview_line_len(b"abc"), 3);
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
