pub mod core_text;
mod editor_text_layer;
mod hover_overlays;
mod ide_panels;
pub mod lsp_ui;
pub mod minimap_ui;
pub mod search;
pub mod settings_ui;
pub mod sticky;
pub mod tabs_ui;
pub mod terminal_ui;
pub mod ui;

use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::Renderer;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static TELEMETRY: RefCell<Telemetry> = RefCell::new(Telemetry::default());
}

struct Telemetry {
    render_time: f32,
    render_count: u32,
    scroll_time: f32,
    scroll_count: u32,
    type_time: f32,
    type_count: u32,
    last_print: Instant,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            render_time: 0.0,
            render_count: 0,
            scroll_time: 0.0,
            scroll_count: 0,
            type_time: 0.0,
            type_count: 0,
            last_print: Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_default_starts_with_empty_counters_and_fresh_print_time() {
        let before = Instant::now();
        let telemetry = Telemetry::default();
        let after = Instant::now();

        assert_eq!(telemetry.render_time, 0.0);
        assert_eq!(telemetry.render_count, 0);
        assert_eq!(telemetry.scroll_time, 0.0);
        assert_eq!(telemetry.scroll_count, 0);
        assert_eq!(telemetry.type_time, 0.0);
        assert_eq!(telemetry.type_count, 0);
        assert!(telemetry.last_print >= before);
        assert!(telemetry.last_print <= after);
        assert!(!TELEMETRY_ENABLED.load(Ordering::Relaxed));
    }
}
use glow::HasContext;

#[derive(Clone, Copy)]
pub struct ModInterval {
    pub top: f32,
    pub bottom: f32,
    pub state: crate::editor::LineModState,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub fn draw(
        &mut self,
        editor: &mut Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        scroll_x: f32,
        scroll_y: f32,
        blink_alpha: f32,
        show_fps: bool,
        spans: &[ColorSpan],
        dialog_window_open: bool,
        is_resizing: bool,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        show_search: bool,
        search_anim_y: f32,
        search_editor: &Editor,
        search_focused: bool,
        search_case_sensitive: bool,
        show_welcome: bool,
        recent_files: &[std::path::PathBuf],
        current_sticky_lines: &[(usize, usize)],
        sticky_anim_progress: f32,
        sticky_anim_is_adding: bool,
        is_ide_mode: bool,
        ide_panel: &crate::app::IdePanelState,
        show_settings: bool,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        tab_scroll_x: f32,
        _syntax_errors: &[(usize, usize)],
        ctrl_definition_range: Option<(usize, usize)>,
    ) -> (bool, Vec<(usize, usize)>) {
        let frame_start_time = Instant::now();
        let was_typing = self.last_editor_version_for_typing != editor.version;
        let was_scrolling = (self.last_scroll_y - scroll_y).abs() > 0.1
            || (self.last_scroll_x - scroll_x).abs() > 0.1;

        let cursor_phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= editor.cursor)
            .saturating_sub(1);

        let (diag_version, instant_raw) = if let Some(l) = lsp {
            if let Some(p) = editor_path {
                l.get_instant_diagnostics_with_version(p)
            } else {
                (0, &[] as &[crate::lsp::Diagnostic])
            }
        } else {
            (0, &[] as &[crate::lsp::Diagnostic])
        };

        let get_byte_offset = |line: u32, utf16_col: u32| -> usize {
            let line = line as usize;
            if line >= editor.line_offsets.len() {
                return editor.len();
            }
            let start = editor.line_offsets[line];
            let end = editor
                .line_offsets
                .get(line + 1)
                .copied()
                .unwrap_or(editor.len());
            let mut current_utf16 = 0;
            let mut current_byte = start;
            let (first, second) = editor.text_parts();
            let first_len = first.len();

            while current_byte < end {
                if current_utf16 >= utf16_col {
                    break;
                }
                let ch = if current_byte < first_len {
                    first[current_byte..].chars().next().unwrap_or('\0')
                } else {
                    second[current_byte - first_len..]
                        .chars()
                        .next()
                        .unwrap_or('\0')
                };
                current_utf16 += ch.len_utf16() as u32;
                current_byte += ch.len_utf8();
            }
            current_byte
        };

        let mut lsp_diagnostics_filtered = Vec::new();
        let mut unused_spans = Vec::new();
        for d in instant_raw {
            let diag_line = d.start_line as usize;
            let mut suppress = false;

            if diag_line == cursor_phys_line {
                if (diag_version as u64) < editor.version {
                    suppress = true;
                }

                let code = d.code.as_deref().unwrap_or("");
                if code == "W291" || code == "W293" {
                    suppress = true;
                }
            }

            if !suppress {
                lsp_diagnostics_filtered.push(d.clone());
                if d.tags.contains(&1) || d.tags.contains(&2) {
                    let start = get_byte_offset(d.start_line, d.start_col);
                    let end = get_byte_offset(d.end_line, d.end_col);
                    if start < end {
                        unused_spans.push((start, end));
                    }
                }
            }
        }
        unused_spans.sort_unstable_by_key(|&(s, _)| s);
        let lsp_diagnostics = &lsp_diagnostics_filtered;

        let delayed_diagnostics = if let Some(l) = lsp {
            if let Some(p) = editor_path {
                l.get_diagnostics(p)
            } else {
                &[]
            }
        } else {
            &[]
        };

        if show_welcome && !is_ide_mode {
            return (self.draw_welcome(recent_files, ui_registry), Vec::new());
        }

        let mut wants_pointer = false;

        if show_fps {
            let now = std::time::Instant::now();
            if let Some(last) = self.last_frame_time {
                let dt = now.duration_since(last).as_secs_f32();
                self.frame_count += 1;
                self.time_acc += dt;
                if self.time_acc >= 0.5 {
                    self.fps = self.frame_count as f32 / self.time_acc;
                    self.frame_count = 0;
                    self.time_acc = 0.0;

                    use std::fmt::Write;
                    self.fps_string.clear();
                    let _ = write!(&mut self.fps_string, "FPS: {:.0}", self.fps);
                }
            }
            self.last_frame_time = Some(now);
        } else {
            self.last_frame_time = None;
        }

        let cursor_phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= editor.cursor)
            .saturating_sub(1);

        self.phys_to_visual.clear();
        self.phys_to_visual.resize(editor.line_offsets.len(), 0);

        let mut visible_lines_count = 0;
        let mut visible_cursor_line = 0;
        let mut temp_phys = 0;
        while temp_phys < editor.line_offsets.len() {
            self.phys_to_visual[temp_phys] = visible_lines_count;
            if temp_phys == cursor_phys_line {
                visible_cursor_line = visible_lines_count;
            }
            let is_folded = editor.folded_lines.contains(&temp_phys)
                && editor.foldable_lines.contains_key(&temp_phys);
            let fold_end = if is_folded {
                editor.foldable_lines.get(&temp_phys).copied()
            } else {
                None
            };
            visible_lines_count += 1;
            if let Some(end) = fold_end {
                if cursor_phys_line > temp_phys && cursor_phys_line <= end {
                    visible_cursor_line = visible_lines_count - 1;
                }
                while temp_phys < end {
                    temp_phys += 1;
                    if temp_phys < editor.line_offsets.len() {
                        self.phys_to_visual[temp_phys] = visible_lines_count - 1;
                    }
                }
            }
            temp_phys += 1;
        }

        let total_lines = visible_lines_count.max(1);
        let s = self.scale_factor;
        let mx = if show_settings || dialog_window_open {
            -1.0
        } else {
            self.last_mouse_x
        };
        let my = if show_settings || dialog_window_open {
            -1.0
        } else {
            self.last_mouse_y
        };

        // Используем фактическую ширину панели, а не проверку any_top_open()
        // Потому что панель может быть открыта через bottom группу
        let panel_left_w = if is_ide_mode {
            ide_panel.left_width * s
        } else {
            0.0
        };
        let panel_bottom_h = if is_ide_mode && ide_panel.any_bottom_open() {
            ide_panel.bottom_height * s
        } else {
            0.0
        };
        let is_ui_disabled = is_ide_mode && ide_panel.terminal_focused;

        if (self.last_mouse_x, self.last_mouse_y) != self.last_known_mouse {
            self.hide_popups_until_mouse_move = false;
            self.last_known_mouse = (self.last_mouse_x, self.last_mouse_y);
        }
        if self.last_editor_version_for_typing != editor.version
            || self.last_cursor_for_popups != editor.cursor
            || (self.last_scroll_y - scroll_y).abs() > 0.1
            || (self.last_scroll_x - scroll_x).abs() > 0.1
        {
            self.hide_popups_until_mouse_move = true;
            self.last_editor_version_for_typing = editor.version;
            self.last_cursor_for_popups = editor.cursor;
        }

        let real_height = self.height;
        let tab_bar_h = if show_welcome || !is_ide_mode {
            0.0
        } else {
            44.0 * s
        };
        let editor_height = real_height - tab_bar_h;

        let target_minimap_w = 119.0 * s;

        if (self.minimap_width - target_minimap_w).abs() > 0.5 {
            self.minimap_width = target_minimap_w;
            self.visual_lines.clear();
        }

        let sidebar_w = if is_ide_mode { 48.0 * s } else { 0.0 };
        let digits = editor.line_offsets.len().to_string().len().max(3);
        let target_padding =
            (30.0 * s + digits as f32 * 10.0 * s + sidebar_w + panel_left_w).round();
        if (self.left_padding - target_padding).abs() > 0.5 {
            self.left_padding = target_padding;
            self.visual_lines.clear();
        }

        // self.height = real_height — текст рендерится на полную высоту окна,
        // включая зону нижней панели (нужно для работы прозрачности панели).
        self.update_cache(editor, scroll_x, scroll_y, is_resizing);

        let render_scroll_x = scroll_x.round();
        let render_scroll_y = scroll_y.round() - tab_bar_h;

        if self.last_editor_version_for_scroll_x != editor.version
            || (self.last_width - self.width).abs() > 0.5
        {
            let longest_idx = editor.longest_line_idx;
            let start_byte = editor.line_offsets.get(longest_idx).copied().unwrap_or(0);
            let end_byte = editor
                .line_offsets
                .get(longest_idx + 1)
                .copied()
                .unwrap_or(editor.len());
            let (first, second) = editor.text_parts();
            let longest_width = self.measure_width(first, second, start_byte, end_byte);
            let view_w = self.width - self.minimap_width - self.left_padding;

            if longest_width > view_w {
                self.max_scroll_x = longest_width - view_w + 100.0;
            } else {
                self.max_scroll_x = 0.0;
            }

            self.last_editor_version_for_scroll_x = editor.version;
        }

        // С этого момента self.height = real_height на всём протяжении кадра.
        // Матрица проекции в flush() всегда корректна.
        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.clear_color(
                0.173, // #2c
                0.180, // #2e
                0.224, // #39
                1.0,
            );
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        editor.ensure_indent_cache_updated();
        let indent_levels = editor.get_cached_indent_levels();
        let (first, second) = editor.text_parts();

        if is_ide_mode {
            self.draw_ide_side_panels(
                ide_panel,
                lsp,
                ui_registry,
                !lsp_diagnostics.is_empty(),
                s,
                mx,
                my,
                real_height,
                panel_left_w,
                is_ui_disabled,
            );
        }
        // IDE с пустыми вкладками — показываем cowsay экран вместо редактора
        if is_ide_mode && tabs.is_empty() {
            self.draw_empty_ide(panel_left_w);
            return (false, Vec::new());
        } else {
            self.was_empty_ide = false;
        }

        let first_len = first.len();
        let len = first_len + second.len();

        // --- Подсветка скобок ---
        let mut bracket_pairs = None;
        let find_matching_bracket = |pos: usize, b: u8| -> Option<usize> {
            let (open, close, dir) = match b {
                b'(' => (b'(', b')', 1isize),
                b'[' => (b'[', b']', 1isize),
                b'{' => (b'{', b'}', 1isize),
                b')' => (b')', b'(', -1isize),
                b']' => (b']', b'[', -1isize),
                b'}' => (b'}', b'{', -1isize),
                _ => return None,
            };
            let mut depth = 1;
            let mut curr = pos as isize + dir;
            while curr >= 0 && curr < len as isize {
                let cb = editor.byte_at(curr as usize);
                if cb == open {
                    depth += 1;
                } else if cb == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(curr as usize);
                    }
                }
                curr += dir;
            }
            None
        };

        if editor.cursor < len {
            let b = editor.byte_at(editor.cursor);
            if let Some(matching) = find_matching_bracket(editor.cursor, b) {
                bracket_pairs = Some((editor.cursor, matching));
            }
        }
        if bracket_pairs.is_none() && editor.cursor > 0 {
            let b = editor.byte_at(editor.cursor - 1);
            if let Some(matching) = find_matching_bracket(editor.cursor - 1, b) {
                bracket_pairs = Some((editor.cursor - 1, matching));
            }
        }

        let sel_start = editor
            .selection_anchor
            .map(|a| a.min(editor.cursor))
            .unwrap_or(editor.cursor);
        let sel_end = editor
            .selection_anchor
            .map(|a| a.max(editor.cursor))
            .unwrap_or(editor.cursor);

        // --- Одинаковые слова (Word Highlighting) ---
        self.identical_words_cache.clear();
        let mut target_word_str: Option<&str> = None;
        let is_valid_word = |s: &str| -> bool {
            s.chars().next().map_or(false, |c| !c.is_ascii_digit())
                && s.as_bytes()
                    .iter()
                    .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
        };

        if sel_start != sel_end {
            let slen = sel_end - sel_start;
            if slen < 100 {
                if sel_end <= first_len {
                    if let Some(s) = first.get(sel_start..sel_end) {
                        if is_valid_word(s) {
                            target_word_str = Some(s);
                        }
                    }
                } else if sel_start >= first_len {
                    if let Some(s) = second.get((sel_start - first_len)..(sel_end - first_len)) {
                        if is_valid_word(s) {
                            target_word_str = Some(s);
                        }
                    }
                }
            }
        } else {
            let mut p_start = editor.cursor;
            while p_start > 0 {
                let b = editor.byte_at(p_start - 1);
                if !(b.is_ascii_alphanumeric() || b == b'_') {
                    break;
                }
                p_start -= 1;
            }
            let mut p_end = editor.cursor;
            while p_end < len {
                let b = editor.byte_at(p_end);
                if !(b.is_ascii_alphanumeric() || b == b'_') {
                    break;
                }
                p_end += 1;
            }
            if p_end > p_start {
                if p_end <= first_len {
                    if let Some(s) = first.get(p_start..p_end) {
                        if is_valid_word(s) {
                            target_word_str = Some(s);
                        }
                    }
                } else if p_start >= first_len {
                    if let Some(s) = second.get((p_start - first_len)..(p_end - first_len)) {
                        if is_valid_word(s) {
                            target_word_str = Some(s);
                        }
                    }
                }
            }
        }

        if let Some(word) = target_word_str {
            let first_bytes = first.as_bytes();
            let second_bytes = second.as_bytes();
            let w_len = word.len();
            let full_len = first.len() + second.len();

            let get_byte = |idx: usize| -> u8 {
                if idx < first.len() {
                    first_bytes[idx]
                } else {
                    second_bytes[idx - first.len()]
                }
            };

            let mut start = 0;
            while let Some(idx) = first[start..].find(word) {
                let abs_idx = start + idx;
                let left_ok = if abs_idx == 0 {
                    true
                } else {
                    let b = first_bytes[abs_idx - 1];
                    !(b.is_ascii_alphanumeric() || b == b'_')
                };
                let right_ok = if abs_idx + w_len == full_len {
                    true
                } else {
                    let b = get_byte(abs_idx + w_len);
                    !(b.is_ascii_alphanumeric() || b == b'_')
                };
                if left_ok && right_ok {
                    self.identical_words_cache.push((abs_idx, abs_idx + w_len));
                }
                start = abs_idx + w_len;
            }

            let boundary_start = first.len().saturating_sub(w_len - 1);
            for i in boundary_start..first.len() {
                if i + w_len <= full_len {
                    let mut matches = true;
                    let w_bytes = word.as_bytes();
                    for j in 0..w_len {
                        if get_byte(i + j) != w_bytes[j] {
                            matches = false;
                            break;
                        }
                    }
                    if matches {
                        let left_ok = if i == 0 {
                            true
                        } else {
                            let b = get_byte(i - 1);
                            !(b.is_ascii_alphanumeric() || b == b'_')
                        };
                        let right_ok = if i + w_len == full_len {
                            true
                        } else {
                            let b = get_byte(i + w_len);
                            !(b.is_ascii_alphanumeric() || b == b'_')
                        };
                        if left_ok && right_ok {
                            self.identical_words_cache.push((i, i + w_len));
                        }
                    }
                }
            }

            let mut start = 0;
            while let Some(idx) = second[start..].find(word) {
                let abs_idx = first.len() + start + idx;
                let left_ok = if abs_idx == 0 {
                    true
                } else {
                    let b = get_byte(abs_idx - 1);
                    !(b.is_ascii_alphanumeric() || b == b'_')
                };
                let right_ok = if abs_idx + w_len == full_len {
                    true
                } else {
                    let b = second_bytes[start + idx + w_len];
                    !(b.is_ascii_alphanumeric() || b == b'_')
                };
                if left_ok && right_ok {
                    self.identical_words_cache.push((abs_idx, abs_idx + w_len));
                }
                start = start + idx + w_len;
            }
        }

        let total_render_height = total_lines as f32 * self.line_height;
        let raw_max = (total_render_height - editor_height + self.line_height * 2.0).max(0.0);
        let max_scroll = (raw_max / self.line_height).ceil() * self.line_height;

        let render_scroll_y = render_scroll_y.min(max_scroll.max(0.0));
        let scrollbar_width = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };

        let minimap_w = self.minimap_width;
        let minimap_x = self.width - minimap_w;
        let scrollbar_x = minimap_x - scrollbar_width;

        ui_registry.register_text_input(
            crate::ui_system::UiId::EditorTextBody,
            self.left_padding,
            tab_bar_h,
            scrollbar_x - self.left_padding,
            editor_height,
            mx,
            my,
        );

        let solid_minimap_bg = [
            self.theme.minimap_bg[0],
            self.theme.minimap_bg[1],
            self.theme.minimap_bg[2],
            1.0,
        ];

        let cursor_line_y = self.baseline_offset - render_scroll_y
            + (visible_cursor_line as f32 * self.line_height);

        if cursor_line_y > -self.line_height * 2.0 && cursor_line_y < real_height + self.line_height
        {
            self.push_rect(
                self.left_padding,
                cursor_line_y - self.baseline_offset + 2.0,
                scrollbar_x - self.left_padding,
                self.line_height,
                [0.9, 0.9, 0.9, 0.12],
            );
        }

        let skip_visual_lines = 0;
        let end_visual_line = self.visual_lines.len();

        self.draw_editor_visible_text(
            editor,
            spans,
            search_results,
            search_current_idx,
            &unused_spans,
            first,
            second,
            indent_levels,
            first_len,
            len,
            bracket_pairs,
            sel_start,
            sel_end,
            render_scroll_x,
            render_scroll_y,
            scrollbar_x,
            blink_alpha,
            dialog_window_open,
            search_focused,
            show_settings,
            s,
            skip_visual_lines,
            end_visual_line,
            ui_registry,
            ctrl_definition_range,
        );

        self.flush();
        let mouse_in_popup = crate::app::mouse::HOVER_STATE.with(|s| {
            s.borrow()
                .popup_or_bridge_contains(mx, my, self.width, self.scale_factor)
                .0
        });

        let hovered_diag_type_target = self.draw_lsp_squiggles_and_collect_hovered_diag(
            editor,
            lsp_diagnostics,
            scroll_x,
            render_scroll_y,
            panel_bottom_h,
            is_ide_mode,
            is_ui_disabled,
            ide_panel,
            mx,
            my,
            mouse_in_popup,
        );

        let gutter_x = if is_ide_mode {
            48.0 * s + panel_left_w
        } else {
            0.0
        };
        // Гаттер рисуем только в зоне редактора (не заходим на нижнюю панель)
        self.push_rect(
            gutter_x.round() + 1.0,
            tab_bar_h,
            (self.left_padding - (gutter_x.round() + 1.0)).max(0.0),
            editor_height,
            solid_minimap_bg,
        );
        // Левая граница гаттера (отделяет IDE панель от зоны номеров строк)
        if is_ide_mode && panel_left_w > 0.0 {
            self.push_rect(
                gutter_x.round() + 1.0,
                tab_bar_h,
                1.0,
                editor_height,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
            );
        }
        // Правая граница гаттера (тонкая линия, как у Indent Guide)
        self.push_rect(
            self.left_padding - 1.0,
            tab_bar_h,
            1.0,
            editor_height,
            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
        );

        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let y = self.baseline_offset + v_line.y_offset - render_scroll_y;
            let phys_idx = v_line.physical_line - 1;

            if editor.foldable_lines.contains_key(&phys_idx) {
                let arrow_x = self.left_padding - 20.0 * s;
                let is_folded = editor.folded_lines.contains(&phys_idx);
                let arrow_str = if is_folded { "▶" } else { "▼" };
                self.draw_string_scaled(arrow_str, arrow_x, y - 1.0 * s, self.theme.line_num, 1.0);
                ui_registry.register_rect(
                    crate::ui_system::UiId::EditorFoldArrow(phys_idx),
                    arrow_x - 5.0 * s,
                    y - self.line_height,
                    20.0 * s,
                    self.line_height + 5.0 * s,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }

            let mut n = v_line.physical_line;
            let mut buf = [0u8; 20];
            let mut idx = 20;
            if n == 0 {
                idx -= 1;
                buf[idx] = b'0';
            } else {
                while n > 0 {
                    idx -= 1;
                    buf[idx] = b'0' + (n % 10) as u8;
                    n /= 10;
                }
            }
            if let Ok(num_str) = std::str::from_utf8(&buf[idx..]) {
                let num_w = self.measure_mono_width(num_str, 1.0);
                let draw_x = self.left_padding - 24.0 * s - num_w;
                self.draw_string_mono_scaled(num_str, draw_x, y, self.theme.line_num, 1.0);
            }
        }

        for i in 0..self.merged_intervals_cache.len() {
            let m = self.merged_intervals_cache[i];
            if m.bottom < 0.0 || m.top > real_height {
                continue;
            }
            let color = if m.state == crate::editor::LineModState::ModifiedUnsaved {
                self.theme.modified_unsaved
            } else {
                self.theme.modified_saved
            };
            let draw_top = m.top + 2.0;
            let draw_bottom = m.bottom + 2.0;
            let draw_h = (draw_bottom - draw_top).max(4.0);
            self.push_rounded_rect(
                self.left_padding - 4.0 * s,
                draw_top,
                4.0 * s,
                draw_h,
                2.0 * s,
                color,
            );
        }

        self.flush();

        self.push_rect(
            minimap_x,
            tab_bar_h,
            minimap_w,
            editor_height,
            solid_minimap_bg,
        );

        self.draw_minimap(
            editor,
            spans,
            render_scroll_y,
            max_scroll,
            total_lines,
            visible_cursor_line,
            editor_height,
            tab_bar_h,
        );

        ui_registry.register_rect(
            crate::ui_system::UiId::EditorMinimap,
            minimap_x,
            tab_bar_h,
            minimap_w,
            editor_height,
            mx,
            my,
        );

        if self.max_scroll_x > 0.0 {
            let track_w = scrollbar_x - self.left_padding;
            let track_h_bg = 14.0 * s;
            let track_y_bg = real_height - track_h_bg;

            self.push_rect(
                self.left_padding,
                track_y_bg,
                track_w,
                track_h_bg,
                [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
            );

            let thumb_w =
                (track_w / (self.max_scroll_x + track_w).max(1.0) * track_w).max(40.0 * s);
            let scroll_ratio_x = (render_scroll_x / self.max_scroll_x).clamp(0.0, 1.0);
            let thumb_x = self.left_padding + scroll_ratio_x * (track_w - thumb_w);

            let thumb_y = real_height - 10.0 * s;
            let thumb_h = 6.0 * s;

            self.push_rounded_rect(
                thumb_x,
                thumb_y,
                thumb_w,
                thumb_h,
                3.0 * s,
                [0.7, 0.33, 0.54, 1.0],
            );
        }

        // --- 8.5. Линейка диагностики на скроллбаре ---
        if !is_resizing && is_ide_mode && !dialog_window_open {
            self.draw_diagnostics_ruler(editor, delayed_diagnostics, self.height);
        }

        let mut tab_tooltip = None;
        if show_welcome && is_ide_mode {
            let anim_w = self.width - gutter_x;
            let anim_h = self.height - panel_bottom_h;
            self.push_rect(gutter_x, 0.0, anim_w, anim_h, [0.173, 0.180, 0.224, 1.0]);

            ui_registry.register_blocker(
                crate::ui_system::UiId::BottomPanelBody,
                gutter_x,
                0.0,
                anim_w,
                anim_h,
                mx,
                my,
            );

            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f32();
            let text = "RRiter";
            let sub_text = "Нет открытых файлов";
            let scale_t = 3.0;
            let scale_sub = 1.2;
            let text_w = self
                .measure_ui_width(text, scale_t)
                .max(self.measure_ui_width(sub_text, scale_sub));
            let text_h = 70.0 * s;

            let eff_w = (anim_w - text_w).max(1.0);
            let eff_h = (anim_h - text_h).max(1.0);
            let px = (t * 100.0 * s) % (eff_w * 2.0);
            let rx = if px < eff_w { px } else { eff_w * 2.0 - px };
            let py = (t * 75.0 * s) % (eff_h * 2.0);
            let ry = if py < eff_h { py } else { eff_h * 2.0 - py };

            let r = (t * 2.0).sin() * 0.2 + 0.6;
            let g = (t * 3.0).sin() * 0.2 + 0.6;
            let b = (t * 5.0).sin() * 0.2 + 0.8;

            let draw_x = gutter_x + rx;
            let draw_y = ry + 40.0 * s;
            self.draw_string_scaled(text, draw_x, draw_y, [r, g, b, 1.0], scale_t);
            self.draw_string_scaled(
                sub_text,
                draw_x,
                draw_y + 30.0 * s,
                [0.5, 0.5, 0.6, 1.0],
                scale_sub,
            );
            self.flush();
        } else if !show_welcome && is_ide_mode {
            let tab_x = gutter_x.round() + 1.0;
            let tab_w = self.width - tab_x;
            tab_tooltip = self.draw_tab_bar(
                tabs,
                active_tab,
                editor,
                editor_title,
                editor_path,
                tab_x,
                0.0,
                tab_w,
                tab_bar_h,
                s,
                mx,
                my,
                ui_registry,
                tab_scroll_x,
                ide_panel.tab_drag.as_ref(),
            );
            self.flush();
        }

        let target_sticky_lines = if show_welcome {
            Vec::new()
        } else {
            self.draw_sticky_lines(
                editor,
                spans,
                current_sticky_lines,
                render_scroll_y,
                render_scroll_x,
                sticky_anim_progress,
                sticky_anim_is_adding,
                gutter_x.round() + 1.0,
                ui_registry,
                tab_bar_h,
            )
        };

        if scrollbar_width > 0.0 {
            let scroll_ratio_y = (render_scroll_y / max_scroll).clamp(0.0, 1.0);
            let total_content_height = (total_lines as f32 + 2.0) * self.line_height;
            let thumb_h = (editor_height / total_content_height.max(editor_height) * editor_height)
                .max(20.0 * s);
            let thumb_y = tab_bar_h + scroll_ratio_y * (editor_height - thumb_h);
            self.push_rounded_rect(
                scrollbar_x + 1.0 * s,
                thumb_y,
                scrollbar_width - 2.0 * s,
                thumb_h,
                (scrollbar_width - 2.0 * s) / 2.0,
                [0.7, 0.33, 0.54, 0.8],
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::EditorScrollbarY,
                scrollbar_x,
                tab_bar_h,
                scrollbar_width,
                editor_height,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }

        if self.max_scroll_x > 0.0 {
            let track_w = scrollbar_x - self.left_padding;
            ui_registry.register_rect(
                crate::ui_system::UiId::EditorScrollbarX,
                self.left_padding,
                real_height - 14.0 * s,
                track_w,
                14.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }

        if show_fps {
            let center_x = (self.width - minimap_w) / 2.0;
            self.push_rect(center_x - 45.0, 5.0, 90.0, 25.0, [0.1, 0.1, 0.1, 0.8]);

            let fps_text = std::mem::take(&mut self.fps_string);
            self.draw_string(&fps_text, center_x - 40.0, 24.0, [0.0, 1.0, 0.0, 1.0]);
            self.fps_string = fps_text;
        }

        if search_anim_y > -100.0 * self.scale_factor {
            wants_pointer |= self.draw_search_panel(
                show_search,
                search_anim_y,
                search_editor,
                search_focused,
                search_case_sensitive,
                search_results,
                search_current_idx,
                blink_alpha,
                scrollbar_width,
                ui_registry,
            );
        }

        // self.height уже = real_height на всём протяжении, ничего восстанавливать не нужно

        if is_ide_mode && panel_bottom_h > 0.0 {
            self.draw_ide_bottom_panel(
                ide_panel,
                lsp,
                ui_registry,
                !lsp_diagnostics.is_empty(),
                s,
                mx,
                my,
                panel_bottom_h,
                is_ui_disabled,
            );
        }

        if dialog_window_open {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.6]);
        }

        self.draw_hover_overlays(
            editor,
            lsp_diagnostics,
            ide_panel,
            ui_registry,
            mx,
            my,
            scroll_x,
            render_scroll_y,
            hovered_diag_type_target,
            &mut wants_pointer,
        );

        if let Some((path, tx, ty)) = tab_tooltip {
            self.draw_tab_tooltip(&path, tx, ty, s);
        }

        self.flush();

        // Регистрация хитбоксов ресайза в самом конце, чтобы они перекрывали все панели и блокираторы
        // Блокируем resize, когда терминал в фокусе
        if is_ide_mode && panel_left_w > 0.0 && !is_ui_disabled {
            let resize_x = 48.0 * s + panel_left_w;
            ui_registry.register_blocker(
                crate::ui_system::UiId::ResizeLeft,
                resize_x - 8.0 * s,
                0.0,
                16.0 * s,
                real_height,
                mx,
                my,
            );
        }
        if is_ide_mode && panel_bottom_h > 0.0 && !is_ui_disabled {
            let panel_y = self.height - panel_bottom_h;
            ui_registry.register_blocker(
                crate::ui_system::UiId::ResizeBottom,
                48.0 * s,
                panel_y - 8.0 * s,
                self.width - 48.0 * s,
                16.0 * s,
                mx,
                my,
            );
        }

        if TELEMETRY_ENABLED.load(Ordering::Relaxed) {
            let elapsed = frame_start_time.elapsed().as_secs_f32();
            TELEMETRY.with(|t| {
                let mut t = t.borrow_mut();
                if was_typing {
                    t.type_time += elapsed;
                    t.type_count += 1;
                } else if was_scrolling {
                    t.scroll_time += elapsed;
                    t.scroll_count += 1;
                } else {
                    t.render_time += elapsed;
                    t.render_count += 1;
                }

                if t.last_print.elapsed().as_secs() >= 10 {
                    let r_avg = if t.render_count > 0 {
                        (t.render_time / t.render_count as f32) * 1000.0
                    } else {
                        0.0
                    };
                    let s_avg = if t.scroll_count > 0 {
                        (t.scroll_time / t.scroll_count as f32) * 1000.0
                    } else {
                        0.0
                    };
                    let ty_avg = if t.type_count > 0 {
                        (t.type_time / t.type_count as f32) * 1000.0
                    } else {
                        0.0
                    };
                    println!(
                        "📊 Telemetry (10s): Idle Render {:.2}ms | Scroll {:.2}ms | Type {:.2}ms",
                        r_avg, s_avg, ty_avg
                    );

                    t.render_time = 0.0;
                    t.render_count = 0;
                    t.scroll_time = 0.0;
                    t.scroll_count = 0;
                    t.type_time = 0.0;
                    t.type_count = 0;
                    t.last_print = Instant::now();
                }
            });
        }

        (
            wants_pointer | ui_registry.wants_pointer(),
            target_sticky_lines,
        )
    }
}
