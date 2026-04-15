pub mod core_text;
pub mod lsp_ui;
pub mod settings_ui;
pub mod ui;
pub mod sticky;
pub mod search;

use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::{Renderer, Vertex};
use crate::widgets::IconButton;
use glow::HasContext;

#[derive(Clone, Copy)]
pub struct ModInterval {
    pub top: f32,
    pub bottom: f32,
    pub state: crate::editor::LineModState,
}

impl Renderer {
    pub fn draw(
        &mut self,
        editor: &mut Editor,
        scroll_x: f32,
        scroll_y: f32,
        blink_alpha: f32,
        show_fps: bool,
        spans: &[ColorSpan],
        dialog_window_open: bool,
        is_resizing: bool,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
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
        lsp_diagnostics: &[crate::lsp::Diagnostic],
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> (bool, Vec<(usize, usize)>) {
        if show_welcome {
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
        let mx = if show_settings || dialog_window_open { -1.0 } else { self.last_mouse_x };
        let my = if show_settings || dialog_window_open { -1.0 } else { self.last_mouse_y };

        let panel_left_w = if is_ide_mode && ide_panel.any_top_open() {
            ide_panel.left_width * s
        } else {
            0.0
        };
        let panel_bottom_h = if is_ide_mode && ide_panel.any_bottom_open() {
            ide_panel.bottom_height * s
        } else {
            0.0
        };
        let real_height = self.height;
        // Нижняя панель полупрозрачная, поэтому текст, скролл и мини-карта
        // рендерятся на всю высоту окна, проходя ПОД панелью без сжатия.
        let editor_height = real_height;

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
        let render_scroll_y = scroll_y.round();

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
            let sb_w = 48.0 * s;

            // Сайдбар рисуется на полную высоту окна (real_height)self.push_rect(0.0, 0.0, sb_w, real_height, sidebar_bg);
            self.push_rect(sb_w - 1.0, 0.0, 1.0, real_height, [1.0, 1.0, 1.0, 0.12]);

                        let btn_size = sb_w;
                        let btn_gap = 0.0;
            let btn_x = 0.0;
            let top_start_y = 0.0;

            let mut top_idx = 0usize;
            let mut bottom_idx = 0usize;

            for slot in &ide_panel.slots {
                let is_dragging_this = ide_panel
                    .drag
                    .as_ref()
                    .map(|d| d.panel_id == slot.id && d.threshold_passed)
                    .unwrap_or(false);
                if is_dragging_this {
                    if slot.group == crate::app::PanelGroup::Top {
                        top_idx += 1;
                    } else {
                        bottom_idx += 1;
                    }
                    continue;
                }

                let btn_y = if slot.group == crate::app::PanelGroup::Top {
                    let y = top_start_y + top_idx as f32 * (btn_size + btn_gap);
                    top_idx += 1;
                    y
                } else {
                    // Кнопки нижней группы фиксированы у дна окна, независимо от панели
                    let y = real_height - btn_size - bottom_idx as f32 * btn_size;
                    bottom_idx += 1;
                    y
                };

                let btn = IconButton {
                    x: btn_x,
                    y: btn_y,
                    size: btn_size,
                    icon: Some(slot.id.icon()),
                    is_active: slot.open,
                    icon_size: Some(36.0 * s),
                    active_square_width: Some(sb_w),
                };
                ui_registry.register_icon_button(
                    crate::ui_system::UiId::SidebarSlot(slot.id),
                    &btn,
                    self,
                    mx,
                    my,
                    s,
                    false,
                );
            }

            // Призрак перетаскиваемой кнопки + разделитель
            if let Some(drag) = &ide_panel.drag {
                if drag.threshold_passed {
                    if let Some(slot) = ide_panel.slots.iter().find(|sl| sl.id == drag.panel_id) {
                        let ghost_y =
                            (drag.current_y - btn_size / 2.0).clamp(0.0, real_height - btn_size);
                        let ghost = IconButton {
                            x: btn_x,
                            y: ghost_y,
                            size: btn_size,
                            icon: Some(slot.id.icon()),
                            is_active: false,
                            icon_size: Some(36.0 * s),
                            active_square_width: None,
                        };
                        ghost.render(self, -1.0, -1.0, s, false);
                    }
                    // Горизонтальный разделитель посередине сайдбара
                    let sep_y = (real_height / 2.0).round();
                    self.push_rect(
                        2.0 * s,
                        sep_y - 1.0,
                        sb_w - 4.0 * s,
                        2.0,
                        [0.60, 0.35, 0.85, 0.9],
                    );
                }
            }

            // Левая панель (для групп Top)
            if panel_left_w > 0.0 {
                let panel_x = sb_w;
                let panel_bg = [
                    0.129, // #21
                    0.133, // #22
                    0.173, // #2c
                    1.0,
                ];
                // Левая панель не заходит под нижнюю — используем editor_height
                self.push_rect(panel_x, 0.0, panel_left_w, editor_height, panel_bg);
                self.push_rect(
                    panel_x + panel_left_w - 1.0,
                    0.0,
                    1.0,
                    editor_height,
                    [1.0, 1.0, 1.0, 0.12],
                );
                // Тонкая линия-разделитель между левой панелью и зоной номеров строк (аналог Indent Guide)
                let sep_x = (panel_x + panel_left_w).round();
                self.push_rect(
                    sep_x,
                    0.0,
                    1.0,
                    editor_height,
                    [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
                );

                let title_h = 32.0 * s;
                let title_bg = [
                    (self.theme.bg[0] + 0.07).min(1.0),
                    (self.theme.bg[1] + 0.07).min(1.0),
                    (self.theme.bg[2] + 0.08).min(1.0),
                    1.0,
                ];
                self.push_rect(panel_x, 0.0, panel_left_w, title_h, title_bg);

                let open_top: Vec<_> = ide_panel
                    .slots
                    .iter()
                    .filter(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                    .collect();

                if open_top.len() == 1 {
                    let label = open_top[0].id.label();
                    self.draw_string_scaled(
                        label,
                        panel_x + 12.0 * s,
                        title_h / 2.0 + 6.0 * s,
                        self.theme.fg,
                        0.9,
                    );
                } else {
                    let mut tx = panel_x + 6.0 * s;
                    for (i, slot) in open_top.iter().enumerate() {
                        let label = slot.id.label();
                        let tw = self.measure_ui_width(label, 0.85) + 20.0 * s;
                        if i == 0 {
                            let act_bg = [
                                (self.theme.bg[0] + 0.12).min(1.0),
                                (self.theme.bg[1] + 0.12).min(1.0),
                                (self.theme.bg[2] + 0.13).min(1.0),
                                1.0,
                            ];
                            self.push_rect(tx, 0.0, tw, title_h, act_bg);
                            self.push_rect(tx, title_h - 2.0, tw, 2.0, [0.60, 0.35, 0.85, 1.0]);
                        }
                        self.draw_string_scaled(
                            label,
                            tx + 10.0 * s,
                            title_h / 2.0 + 6.0 * s,
                            self.theme.fg,
                            0.85,
                        );
                        tx += tw;
                    }
                }

                // Подсветка ручки ресайза (wants_pointer=false — курсор управляется в events.rs через EwResize)
                let resize_x = panel_x + panel_left_w;
                if mx >= resize_x - 4.0 * s
                    && mx <= resize_x + 4.0 * s
                    && my >= 0.0
                    && my <= editor_height
                {
                    self.push_rect(
                        resize_x - 2.0,
                        0.0,
                        2.0,
                        editor_height,
                        [0.60, 0.35, 0.85, 0.4],
                    );
                }
                ui_registry.register_rect(
                    crate::ui_system::UiId::ResizeLeft,
                    resize_x - 4.0 * s,
                    0.0,
                    8.0 * s,
                    editor_height,
                    mx,
                    my,
                );

                // --- LSP серверы ---
                if ide_panel.is_open(crate::app::PanelId::LspServers) {
                    let is_top = ide_panel.slots.iter().any(|s| {
                        s.id == crate::app::PanelId::LspServers
                            && s.group == crate::app::PanelGroup::Top
                    });
                    if is_top {
                                                self.draw_lsp_servers_panel(
                            panel_x,
                            title_h,
                            panel_left_w,
                            editor_height - title_h,
                            s,
                            ide_panel,
                            !lsp_diagnostics.is_empty(),
                            ui_registry,
                        );
                    }
                }

                // --- Дерево файлов проводника ---
                if ide_panel.is_open(crate::app::PanelId::Explorer) {
                    self.flush();
                    unsafe {
                        self.gl.enable(glow::SCISSOR_TEST);
                        self.gl.scissor(
                            panel_x as i32,
                            0,
                            panel_left_w as i32,
                            (editor_height - title_h) as i32,
                        );
                    }

                    let row_h = 28.0 * s;
                    let indent_w = 18.0 * s;
                    let scroll = ide_panel.explorer_scroll.current.round();
                    let content_h = editor_height - title_h;
                    let total_nodes = ide_panel.file_tree_nodes.len();

                    let tree_text_scale = 17.0 / 18.0;
                    if total_nodes == 0 {
                        let hint = "Нет папок в проекте";
                        let tw = self.measure_ui_width(hint, tree_text_scale);
                        let tx = panel_x + (panel_left_w - tw) / 2.0;
                        self.draw_string_scaled(
                            hint,
                            tx,
                            title_h + 30.0 * s,
                            [0.45, 0.45, 0.45, 1.0],
                            tree_text_scale,
                        );
                    } else {
                        let first_vis = (scroll / row_h).floor() as usize;
                        let last_vis =
                            (((scroll + content_h) / row_h).ceil() as usize + 1).min(total_nodes);

                        for i in first_vis..last_vis {
                            let node = &ide_panel.file_tree_nodes[i];
                            let row_y = title_h + i as f32 * row_h - scroll;

                            ui_registry.register_rect(
                                crate::ui_system::UiId::FileTreeNode(i),
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                mx,
                                my,
                            );

                            if ide_panel.file_tree_hovered_idx == Some(i)
                                || ui_registry.hovered()
                                    == Some(crate::ui_system::UiId::FileTreeNode(i))
                            {
                                self.push_rect(
                                    panel_x,
                                    row_y,
                                    panel_left_w,
                                    row_h,
                                    [1.0, 1.0, 1.0, 0.06],
                                );
                            }

                            let indent_x = panel_x + 8.0 * s + node.depth as f32 * indent_w;
                            let text_y = row_y + row_h / 2.0 + 5.5 * s;
                            let color: [f32; 4] = if node.is_ignored {
                                [0.973, 0.584, 0.502, 0.8] // DRACULA_ORANGE (dimmed)
                            } else if node.is_dir {
                                [0.78, 0.68, 1.0, 1.0]
                            } else {
                                [0.651, 0.686, 0.918, 1.0] // #a6afea
                            };

                            let icon_size = 20.0 * s;
                            let icon_y = row_y + (row_h - icon_size) / 2.0;

                            if node.is_dir {
                                // Стрелка ▶/▼ — такая же как fold-стрелки в гаттере
                                let arrow_str = if node.is_expanded { "▼" } else { "▶" };
                                let arrow_x = indent_x - 2.0 * s;
                                let arrow_y = row_y + row_h / 2.0 + 5.5 * s;
                                let arrow_color = if node.is_ignored {
                                    [0.973, 0.584, 0.502, 0.6]
                                } else {
                                    [0.78, 0.68, 1.0, 0.7]
                                };
                                self.draw_string_scaled(
                                    arrow_str,
                                    arrow_x,
                                    arrow_y,
                                    arrow_color,
                                    1.0,
                                );
                                // Иконка папки — правее стрелки
                                let dir_icon_x = indent_x + 18.0 * s;
                                self.draw_file_icon(
                                    node.icon_key,
                                    true,
                                    dir_icon_x,
                                    icon_y,
                                    icon_size,
                                );
                                self.draw_string_scaled(
                                    &node.name,
                                    dir_icon_x + icon_size + 4.0 * s,
                                    text_y,
                                    color,
                                    tree_text_scale,
                                );
                            } else {
                                let file_icon_x = indent_x + 10.0 * s;
                                self.draw_file_icon(
                                    node.icon_key,
                                    false,
                                    file_icon_x,
                                    icon_y,
                                    icon_size,
                                );
                                self.draw_string_scaled(
                                    &node.name,
                                    file_icon_x + icon_size + 4.0 * s,
                                    text_y,
                                    color,
                                    tree_text_scale,
                                );
                            }
                        }

                        // Тонкий скроллбар
                        let total_h = total_nodes as f32 * row_h;
                        if total_h > content_h {
                            let max_s = (total_h - content_h).max(1.0);
                            let ratio = (scroll / max_s).clamp(0.0, 1.0);
                            let thumb_h =
                                (content_h / total_h * (content_h - 8.0 * s)).max(20.0 * s);
                            let thumb_y =
                                title_h + 4.0 * s + ratio * (content_h - 8.0 * s - thumb_h);
                            self.push_rounded_rect(
                                panel_x + panel_left_w - 5.0 * s,
                                thumb_y,
                                3.0 * s,
                                thumb_h,
                                1.5 * s,
                                [1.0, 1.0, 1.0, 0.22],
                            );
                        }
                    }

                    self.flush();
                    unsafe {
                        self.gl.disable(glow::SCISSOR_TEST);
                    }
                }
            }
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
        let mut identical_words = Vec::new();
        let mut target_word = None;
        let is_valid_word = |s: &str| -> bool {
            s.chars().next().map_or(false, |c| !c.is_ascii_digit())
                && s.as_bytes()
                    .iter()
                    .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
        };

        if sel_start != sel_end {
            let slen = sel_end - sel_start;
            if slen < 100 {
                if let Some(text) = editor.get_selection() {
                    if is_valid_word(&text) {
                        target_word = Some(text);
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
                let slen = p_end - p_start;
                let mut res = Vec::with_capacity(slen);
                for i in p_start..p_end {
                    res.push(editor.byte_at(i));
                }
                let w = String::from_utf8_lossy(&res).into_owned();
                if is_valid_word(&w) {
                    target_word = Some(w);
                }
            }
        }

                        if let Some(word) = target_word {
            let (first, second) = editor.text_parts();
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
            while let Some(idx) = first[start..].find(&word) {
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
                    identical_words.push((abs_idx, abs_idx + w_len));
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
                            identical_words.push((i, i + w_len));
                        }
                    }
                }
            }

            let mut start = 0;
            while let Some(idx) = second[start..].find(&word) {
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
                    identical_words.push((abs_idx, abs_idx + w_len));
                }
                start = start + idx + w_len;
            }
        }

        let max_scroll = self.get_max_scroll(editor, editor_height);
        let render_scroll_y = render_scroll_y.min(max_scroll.max(0.0));
        let scrollbar_width = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };

                let minimap_w = self.minimap_width;
                let minimap_x = self.width - minimap_w;
        let scrollbar_x = minimap_x - scrollbar_width;

        ui_registry.register_text_input(crate::ui_system::UiId::EditorTextBody,
            self.left_padding,
            0.0,
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

        if cursor_line_y > -self.line_height * 2.0
            && cursor_line_y < editor_height + self.line_height
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

        let guide_color = [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.15];
        let space_adv = self.char_advance(' ');

        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let phys_idx = v_line.physical_line - 1;

            if let Some(&depth) = indent_levels.get(phys_idx) {
                if depth > 0 {
                    let y_top = v_line.y_offset - render_scroll_y;
                    let text_start_x = self.left_padding + v_line.whitespace_px_width;
                    let text_end_x = text_start_x + v_line.text_px_width;

                    for level in 1..=depth {
                        let guide_x = self.left_padding + (level as f32 * 4.0 * space_adv);
                        let margin = space_adv * 0.5;
                        let overlaps = v_line.text_px_width > 0.0
                            && text_start_x <= guide_x + margin
                            && text_end_x >= guide_x - margin;

                        if !overlaps {
                            self.push_rect(
                                (guide_x - render_scroll_x).round(),
                                y_top,
                                1.0,
                                self.line_height,
                                guide_color,
                            );
                        }
                    }
                }
            }
        }

        let mut intervals = Vec::with_capacity(64);
        let mut last_phys_line = None;
        let mut last_bottom_y = 0.0;

        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let phys_idx = v_line.physical_line - 1;
            let y_top = v_line.y_offset - render_scroll_y;
            let y_bottom = y_top + self.line_height;
            last_bottom_y = y_bottom;

            if !v_line.is_soft_wrap {
                if let Some(st) = editor.deleted_gaps.get(phys_idx).copied().flatten() {
                    intervals.push(ModInterval {
                        top: y_top - 3.0,
                        bottom: y_top + 3.0,
                        state: st,
                    });
                }
            }

            if let Some(st) = editor.get_line_modification_state(phys_idx) {
                intervals.push(ModInterval {
                    top: y_top,
                    bottom: y_bottom,
                    state: st,
                });
            }
            last_phys_line = Some(phys_idx);
        }

        if end_visual_line == self.visual_lines.len() {
            if let Some(phys_idx) = last_phys_line {
                if let Some(st) = editor.deleted_gaps.get(phys_idx + 1).copied().flatten() {
                    intervals.push(ModInterval {
                        top: last_bottom_y - 3.0,
                        bottom: last_bottom_y + 3.0,
                        state: st,
                    });
                }
            }
        }

        let mut merged: Vec<ModInterval> = Vec::with_capacity(64);
        for int in intervals {
            if let Some(last) = merged.last_mut() {
                if int.top <= last.bottom + 0.1 && int.state == last.state {
                    last.bottom = last.bottom.max(int.bottom);
                    continue;
                }
            }
            merged.push(int);
        }

        let mut cursor_pos = None;

        for i in skip_visual_lines..end_visual_line {
            let v_line_info = self.visual_lines[i];
            let start_byte = v_line_info.byte_idx;

            let end_byte = if v_line_info.is_folded {
                let phys_idx = v_line_info.physical_line - 1;
                if phys_idx + 1 < editor.line_offsets.len() {
                    editor.line_offsets[phys_idx + 1].saturating_sub(1)
                } else {
                    len
                }
            } else if i + 1 < self.visual_lines.len() {
                self.visual_lines[i + 1].byte_idx
            } else {
                let phys_idx = v_line_info.physical_line - 1;
                if phys_idx + 1 < editor.line_offsets.len() {
                    editor.line_offsets[phys_idx + 1]
                } else {
                    len
                }
            };

            let y = self.baseline_offset + v_line_info.y_offset - render_scroll_y;
            let mut x = self.left_padding;

            let mut span_idx = match spans.binary_search_by_key(&start_byte, |s| s.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            let mut search_idx = search_results.partition_point(|&(_, e)| e <= start_byte);
            let mut identical_idx = identical_words.partition_point(|&(_, e)| e <= start_byte);

            let mut current_offset = start_byte;
            let mut current_chunk_offset = start_byte;

            let mut out_of_bounds = false;

            while current_chunk_offset < end_byte {
                if self.vertices.len() > crate::renderer::MAX_VERTICES - 2000 {
                    self.flush();
                }

                let s = if current_chunk_offset < first_len {
                    let s_end = end_byte.min(first_len);
                    &first[current_chunk_offset..s_end]
                } else {
                    let s_start = current_chunk_offset - first_len;
                    let s_end = end_byte - first_len;
                    &second[s_start..s_end]
                };

                for c in s.chars() {
                    if x - render_scroll_x > self.width + 150.0 {
                        out_of_bounds = true;
                        break;
                    }

                    let char_len = c.len_utf8();

                    if cursor_pos.is_none()
                        && editor.cursor >= current_offset
                        && editor.cursor < current_offset + char_len
                    {
                        cursor_pos = Some((x - render_scroll_x, y));
                    }

                    while span_idx < spans.len() && spans[span_idx].end <= current_offset {
                        span_idx += 1;
                    }

                    while search_idx < search_results.len()
                        && search_results[search_idx].1 <= current_offset
                    {
                        search_idx += 1;
                    }
                    while identical_idx < identical_words.len()
                        && identical_words[identical_idx].1 <= current_offset
                    {
                        identical_idx += 1;
                    }

                    let is_newline = c == '\n';
                    let is_hidden = c == '\u{FE0F}' || c == '\u{200D}';
                    let adv = if is_newline || is_hidden {
                        0.0
                    } else {
                        self.char_advance(c)
                    };

                    let mut is_search_res = false;
                    let mut is_active_search = false;

                    if search_idx < search_results.len()
                        && current_offset >= search_results[search_idx].0
                    {
                        is_search_res = true;
                        if Some(search_idx) == search_current_idx {
                            is_active_search = true;
                        }
                    }

                    let is_identical = identical_idx < identical_words.len()
                        && current_offset >= identical_words[identical_idx].0;

                    let is_bracket = if let Some((b1, b2)) = bracket_pairs {
                        current_offset == b1 || current_offset == b2
                    } else {
                        false
                    };

                    // Приоритеты фонов: 1. Выделение, 2. Поиск, 3. Одинаковые слова
                    if current_offset >= sel_start && current_offset < sel_end {
                        let w = if is_newline { 10.0 } else { adv };
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            self.theme.sel,
                        );
                    } else if is_search_res {
                        let w = if is_newline { 10.0 } else { adv };
                        let color = if is_active_search {
                            [1.0, 0.6, 0.0, 0.5]
                        } else {
                            [0.6, 0.6, 0.6, 0.35]
                        };
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            color,
                        );
                    } else if is_identical {
                        let w = if is_newline { 10.0 } else { adv };
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.3],
                        );
                    }

                    if is_bracket && !is_newline && !is_hidden {
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            adv,
                            self.line_height,
                            [0.6, 0.6, 0.6, 0.3],
                        );
                    }

                    if !is_newline && !is_hidden && c != ' ' && c != '\t' {
                        if x - render_scroll_x + adv > 0.0 {
                            if let Some(g) = self.get_glyph(c) {
                                let mut current_color = self.theme.fg;
                                if span_idx < spans.len() && spans[span_idx].start <= current_offset
                                {
                                    current_color = spans[span_idx].color;
                                }

                                self.push_quad(
                                    x - render_scroll_x + g.offset_x,
                                    y - g.offset_y,
                                    g.width,
                                    g.height,
                                    g.u,
                                    g.v,
                                    g.uw,
                                    g.vh,
                                    current_color,
                                    g.is_emoji,
                                );

                                if c == '.' || c == ':' {
                                    self.push_quad(
                                        x - render_scroll_x + g.offset_x + 1.0,
                                        y - g.offset_y,
                                        g.width,
                                        g.height,
                                        g.u,
                                        g.v,
                                        g.uw,
                                        g.vh,
                                        current_color,
                                        g.is_emoji,
                                    );
                                }
                            }
                        }
                    }

                    x += adv;
                    current_offset += char_len;
                }

                if out_of_bounds {
                    break;
                }

                if current_chunk_offset < first_len {
                    current_chunk_offset = first_len;
                } else {
                    current_chunk_offset = end_byte;
                }
            }

            if v_line_info.is_folded {
                let dots_str = "...";
                let dots_adv = self.measure_ui_width(dots_str, 1.0);

                let phys_idx = v_line_info.physical_line - 1;
                let actual_end_byte = if let Some(&fold_end) = editor.foldable_lines.get(&phys_idx)
                {
                    if fold_end + 1 < editor.line_offsets.len() {
                        editor.line_offsets[fold_end + 1].saturating_sub(1)
                    } else {
                        len
                    }
                } else {
                    end_byte
                };

                let is_dots_selected = sel_start != sel_end
                    && sel_start <= actual_end_byte.saturating_sub(1)
                    && sel_end >= actual_end_byte.saturating_sub(1);

                let dots_bg = if is_dots_selected {
                    self.theme.sel
                } else {
                    [
                        self.theme.bg[0] + 0.08,
                        self.theme.bg[1] + 0.08,
                        self.theme.bg[2] + 0.12,
                        1.0,
                    ]
                };

                let box_x = x - render_scroll_x + 2.0 * s;
                let box_w = dots_adv + 6.0 * s;
                let box_y_draw = y - self.baseline_offset + 4.0 * s;
                let box_h_draw = self.line_height - 8.0 * s;

                let next_x = box_x + box_w + 2.0 * s;
                let mut final_x = next_x;
                for i in 0..v_line_info.fold_suffix_len {
                    final_x += self.char_advance(v_line_info.fold_suffix[i as usize]);
                }

                let hit_y_top = y - self.line_height;
                let hit_y_bottom = y + 5.0 * s;
                let hit_w = next_x + 10.0 * s - (box_x - 2.0 * s);
                ui_registry.register_rect(
                    crate::ui_system::UiId::EditorFoldDots(phys_idx),
                    box_x - 2.0 * s,
                    hit_y_top,
                    hit_w,
                    hit_y_bottom - hit_y_top,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );

                if cursor_pos.is_none()
                    && editor.cursor >= end_byte
                    && editor.cursor <= actual_end_byte
                {
                    cursor_pos = Some((final_x, y));
                }

                self.push_rounded_rect(box_x, box_y_draw, box_w, box_h_draw, 4.0 * s, dots_bg);

                self.draw_string_scaled(dots_str, box_x + 3.0 * s, y, self.theme.fg, 1.0);

                let mut suffix_draw_x = next_x;
                for i in 0..v_line_info.fold_suffix_len {
                    let c = v_line_info.fold_suffix[i as usize];
                    let c_adv = self.char_advance(c);

                    if is_dots_selected {
                        self.push_rect(
                            suffix_draw_x,
                            y - self.baseline_offset + 2.0,
                            c_adv,
                            self.line_height,
                            self.theme.sel,
                        );
                    }

                    if let Some(g) = self.get_glyph(c) {
                        self.push_quad(
                            suffix_draw_x + g.offset_x,
                            y - g.offset_y,
                            g.width,
                            g.height,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            self.theme.fg,
                            g.is_emoji,
                        );
                    }
                    suffix_draw_x += c_adv;
                }
            }
        }

        if cursor_pos.is_none() && editor.cursor == len {
            if let Some(last_line) = self.visual_lines.last() {
                let y = self.baseline_offset + last_line.y_offset - render_scroll_y;
                let (first, second) = editor.text_parts();
                let x = self.left_padding
                    + self.measure_width(first, second, last_line.byte_idx, editor.cursor);
                cursor_pos = Some((x - render_scroll_x, y));
            }
        }

        if let Some((cx_screen, cy)) = cursor_pos {
            if sel_start == sel_end
                && blink_alpha > 0.5
                && !dialog_window_open
                && !search_focused
                && !show_settings
            {
                if cy > -self.line_height
                    && cy < self.height + self.line_height
                    && cx_screen < scrollbar_x
                    && cx_screen >= self.left_padding
                {
                    self.push_rect(
                        cx_screen,
                        cy - self.baseline_offset + 2.0,
                        2.0,
                        self.line_height - 2.0,
                        self.theme.fg,
                    );
                }
            }
        }

        self.flush();

                let mut hovered_diag = None;

        // LSP squiggles — волнистые подчёркивания диагностик
        if !lsp_diagnostics.is_empty() {
            let render_scroll_y = scroll_y.round();
            let render_scroll_x = scroll_x.round();
            for (idx, diag) in lsp_diagnostics.iter().enumerate() {
                // Цвет по severity
                let color: [f32; 4] = match diag.severity {
                    crate::lsp::DiagSeverity::Error => [0.96, 0.26, 0.21, 0.90],
                    crate::lsp::DiagSeverity::Warning => [0.98, 0.75, 0.18, 0.90],
                    crate::lsp::DiagSeverity::Info => [0.26, 0.73, 0.90, 0.80],
                    crate::lsp::DiagSeverity::Hint => [0.50, 0.50, 0.50, 0.70],
                };
                                let line = diag.start_line as usize;
                if line >= editor.line_offsets.len() {
                    continue;
                }

                let mut v_line_opt = None;
                for vl in &self.visual_lines {
                    if vl.physical_line == line + 1 {
                        v_line_opt = Some(vl);
                        break;
                    }
                }
                let v_line = match v_line_opt {
                    Some(vl) => vl,
                    None => continue,
                };

                let line_y = self.baseline_offset + v_line.y_offset - render_scroll_y;
                let squiggle_y = line_y + 2.0 * self.scale_factor;

                // Точный расчёт X-позиции: идём по символам строки, считая UTF-16 единицы
                                let avg_adv = self.char_advance('a');
                let mut x_start_px = 0.0f32;
                let mut x_end_px = 0.0f32;
                let mut cur_x = 0.0f32;
                let mut start_found = false;
                let mut end_found = false;
                editor.utf16_col_to_byte_advance(line, |ch, utf16_before, _pos| {
                    if !start_found && utf16_before >= diag.start_col {
                        x_start_px = cur_x;
                        start_found = true;
                    }
                    if diag.end_line == diag.start_line && !end_found && utf16_before >= diag.end_col {
                        x_end_px = cur_x;
                        end_found = true;
                    }
                    cur_x += if ch == '\t' {
                        self.char_advance(' ') * 4.0
                    } else {
                        self.char_advance(ch)
                    };
                });
                // Если col за концом строки — ставим на конец
                if !start_found {
                    x_start_px = cur_x;
                }
                if !end_found {
                    x_end_px = if diag.end_line == diag.start_line {
                        cur_x
                    } else {
                        x_start_px + avg_adv * 8.0
                    };
                }
                                let x_start = self.left_padding + x_start_px - render_scroll_x;
                let x_end = self.left_padding + x_end_px - render_scroll_x;
                let squiggle_w = (x_end - x_start).max(avg_adv * 2.0);

                // Расширяем хитбокс, если мы УЖЕ смотрим на этот pop-up (Hysteresis)
                let is_active = self.last_hovered_diag == Some(idx);
                let hit_margin_x = if is_active { 150.0 * s } else { 0.0 };
                let hit_margin_y = if is_active { 50.0 * s } else { 0.0 };

                if mx >= x_start - hit_margin_x && mx <= x_start + squiggle_w + hit_margin_x && 
                   my >= line_y - hit_margin_y && my <= line_y + self.line_height + hit_margin_y {
                    hovered_diag = Some((idx, diag.clone(), x_start, line_y, line_y + self.line_height));
                }

                if x_end < self.left_padding || x_start > self.width {
                    continue;
                }

                self.push_squiggle(
                    x_start.max(self.left_padding),
                    squiggle_y,
                    squiggle_w,
                    color,
                );
            }
            self.flush();
        }

        let gutter_x = if is_ide_mode {
            48.0 * s + panel_left_w
        } else {
            0.0
        };
        // Гаттер рисуем только в зоне редактора (не заходим на нижнюю панель)
        self.push_rect(
            gutter_x,
            0.0,
            self.left_padding - gutter_x,
            editor_height,
            solid_minimap_bg,
        );
        // Левая граница гаттера (отделяет IDE панель от зоны номеров строк)
        if is_ide_mode && panel_left_w > 0.0 {
            self.push_rect(
                gutter_x,
                0.0,
                1.0,
                editor_height,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
            );
        }
        // Правая граница гаттера (тонкая линия, как у Indent Guide)
        self.push_rect(
            self.left_padding - 1.0,
            0.0,
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

        for m in merged {
            if m.bottom < 0.0 || m.top > editor_height {
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

        self.push_rect(minimap_x, 0.0, minimap_w, editor_height, solid_minimap_bg);

                self.draw_minimap(
            editor,
            spans,
            render_scroll_y,
            max_scroll,
            total_lines,
            visible_cursor_line,
        );

        ui_registry.register_rect(
            crate::ui_system::UiId::EditorMinimap,
            minimap_x,
            0.0,
            minimap_w,
            editor_height,
            mx,
            my,
        );

        if self.max_scroll_x > 0.0 {
            let track_w = scrollbar_x - self.left_padding;
            let track_h_bg = 14.0 * s;
            let track_y_bg = editor_height - track_h_bg;

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

            let thumb_y = editor_height - 10.0 * s;
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

                let target_sticky_lines = self.draw_sticky_lines(
            editor,
            spans,
            current_sticky_lines,
            render_scroll_y,
            render_scroll_x,
            sticky_anim_progress,
            sticky_anim_is_adding,
            gutter_x,
            ui_registry,
        );

        if scrollbar_width > 0.0 {
            let scroll_ratio_y = (render_scroll_y / max_scroll).clamp(0.0, 1.0);
            let total_content_height = (total_lines as f32 + 2.0) * self.line_height;
            let thumb_h = (editor_height / total_content_height.max(editor_height) * editor_height)
                .max(20.0 * s);
            let thumb_y = scroll_ratio_y * (editor_height - thumb_h);
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
                0.0,
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
                editor_height - 14.0 * s,
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
            let sb_w = 48.0 * s;
            let panel_x = sb_w;
            let panel_y = self.height - panel_bottom_h;
            let panel_w = self.width - panel_x;

            let is_terminal = ide_panel.slots.iter().any(|sl| {
                sl.group == crate::app::PanelGroup::Bottom
                    && sl.open
                    && sl.id == crate::app::PanelId::Terminal
            });
            let panel_alpha = if is_terminal { 0.82 } else { 1.0 };

            let panel_bg = [
                0.129, // #21
                0.133, // #22
                0.173, // #2c
                panel_alpha,
            ];
            // Ручка ресайза (1px линия вверху панели)self.push_rect(panel_x, panel_y, panel_w, 1.0,[1.0, 1.0, 1.0, 0.15]);
            self.push_rect(
                panel_x,
                panel_y + 1.0,
                panel_w,
                panel_bottom_h - 1.0,
                panel_bg,
            );

                                    // Непрозрачная панель полностью перехватывает мышь (курсор не меняется, клики не проваливаются)
                        if !is_terminal {
                let blocked = ui_registry.register_blocker(
                    crate::ui_system::UiId::BottomPanelBody,
                    panel_x,
                    panel_y,
                    panel_w,
                    panel_bottom_h,
                    mx,
                    my,
                );
                if blocked {
                    ui_registry.reset_cursor_state();
                }
            }

            let tab_h = 32.0 * s;
            let tab_bar_bg = [
                (self.theme.bg[0] + 0.07).min(1.0),
                (self.theme.bg[1] + 0.07).min(1.0),
                (self.theme.bg[2] + 0.08).min(1.0),
                panel_alpha,
            ];
            self.push_rect(panel_x, panel_y + 1.0, panel_w, tab_h, tab_bar_bg);

            let open_bottom: Vec<_> = ide_panel
                .slots
                .iter()
                .filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
                .collect();
            let mut tx = panel_x + 8.0 * s;
            for (i, slot) in open_bottom.iter().enumerate() {
                let label = slot.id.label();
                let tw = self.measure_ui_width(label, 0.9) + 20.0 * s;
                if i == 0 {
                    let act_bg = [
                        (self.theme.bg[0] + 0.12).min(1.0),
                        (self.theme.bg[1] + 0.12).min(1.0),
                        (self.theme.bg[2] + 0.13).min(1.0),
                        1.0,
                    ];
                    self.push_rect(tx, panel_y + 1.0, tw, tab_h, act_bg);
                    self.push_rect(tx, panel_y + tab_h - 1.0, tw, 2.0, [0.60, 0.35, 0.85, 1.0]);
                }
                self.draw_string_scaled(
                    label,
                    tx + 10.0 * s,
                    panel_y + 1.0 + tab_h / 2.0 + 5.5 * s,
                    self.theme.fg,
                    0.9,
                );
                tx += tw;
            }

                                    // Подсветка ручки ресайза при наведении (wants_pointer=false — курсор через NsResize)
            if my >= panel_y - 4.0 * s && my <= panel_y + 4.0 * s && mx >= panel_x {
                self.push_rect(panel_x, panel_y, panel_w, 2.0,[0.60, 0.35, 0.85, 0.4]);
            }
            ui_registry.register_rect(
                crate::ui_system::UiId::ResizeBottom,
                panel_x,
                panel_y - 4.0 * s,
                panel_w,
                8.0 * s,
                mx,
                my,
            );

            // Плейсхолдер контента
            let content_y = panel_y + 1.0 + tab_h;
            let content_h = panel_bottom_h - 1.0 - tab_h;
            if content_h > 8.0 * s {
                if let Some(slot) = open_bottom.first() {
                    if slot.id == crate::app::PanelId::LspServers {
                                                self.draw_lsp_servers_panel(
                            panel_x,
                            content_y,
                            panel_w,
                            content_h,
                            s,
                            ide_panel,
                            !lsp_diagnostics.is_empty(),
                            ui_registry,
                        );
                    } else {
                        let label = slot.id.label();
                        let lw = self.measure_ui_width(label, 0.85);
                        let col = [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.18];
                        self.draw_string_scaled(
                            label,
                            panel_x + (panel_w - lw) / 2.0,
                            content_y + content_h / 2.0 + 6.0 * s,
                            col,
                            0.85,
                        );
                    }
                }
            }
        }

                        if dialog_window_open {
            self.push_rect(0.0, 0.0, self.width, self.height,[0.0, 0.0, 0.0, 0.6]);
        }

                // --- LSP Diagnostic Tooltip ---
        self.last_hovered_diag = hovered_diag.as_ref().map(|h| h.0);

        if let Some((idx, diag, diag_x, line_y_top, diag_y_bottom)) = hovered_diag {
            let s = self.scale_factor;
            let pad = 12.0 * s;

            // Заголовок (код ошибки + источник)
            let mut title = String::new();
            if let Some(src) = &diag.source {
                title.push_str(src);
            }
            if let Some(code) = &diag.code {
                if !title.is_empty() { title.push_str(" "); }
                title.push_str(code);
            }
            if title.is_empty() {
                title.push_str("Ошибка");
            }

            let msg = &diag.message;
            let msg_w = self.measure_ui_width(msg, 0.9);
            let title_w = self.measure_ui_width(&title, 0.85);
            let box_w = (msg_w.max(title_w) + pad * 2.0 + 100.0 * s).clamp(250.0 * s, 600.0 * s);

            let mut lines = Vec::new();
            let mut cur_line = String::new();
            let max_text_w = box_w - pad * 2.0;
            for word in msg.split_whitespace() {
                let w = self.measure_ui_width(&format!("{} {}", cur_line, word), 0.9);
                if w > max_text_w && !cur_line.is_empty() {
                    lines.push(cur_line);
                    cur_line = word.to_string();
                } else {
                    if !cur_line.is_empty() { cur_line.push(' '); }
                    cur_line.push_str(word);
                }
            }
            if !cur_line.is_empty() { lines.push(cur_line); }

                        let line_h = 20.0 * s;
            let box_h = pad * 2.0 + 24.0 * s + lines.len() as f32 * line_h;

            let mut bx = diag_x;
            if bx + box_w > self.width - 20.0 * s {
                bx = self.width - box_w - 20.0 * s;
            }

            // Пытаемся показать сверху (на строчку выше)
            let mut by = line_y_top - box_h - 8.0 * s;
            // Если сверху не влезает, показываем снизу
            if by < 0.0 {
                by = diag_y_bottom + 8.0 * s;
            }

            let border_color = match diag.severity {
                crate::lsp::DiagSeverity::Error =>[0.96, 0.26, 0.21, 0.6],
                crate::lsp::DiagSeverity::Warning => [0.98, 0.75, 0.18, 0.6],
                crate::lsp::DiagSeverity::Info =>[0.26, 0.73, 0.90, 0.6],
                crate::lsp::DiagSeverity::Hint =>[0.50, 0.50, 0.50, 0.6],
            };

            self.push_rounded_rect(bx - 1.0, by - 1.0, box_w + 2.0, box_h + 2.0, 6.0 * s, border_color);
            self.push_rounded_rect(bx, by, box_w, box_h, 6.0 * s,[0.15, 0.16, 0.20, 1.0]);

            let title_color = match diag.severity {
                crate::lsp::DiagSeverity::Error =>[0.96, 0.46, 0.41, 1.0],
                crate::lsp::DiagSeverity::Warning =>[0.98, 0.85, 0.38, 1.0],
                _ =>[0.7, 0.7, 0.75, 1.0],
            };
            self.draw_string_scaled(&title, bx + pad, by + pad + 6.0 * s, title_color, 0.85);

            let is_copied = if let Some(copied_idx) = ide_panel.diag_copied_idx {
                if copied_idx == idx {
                    if let Some(time) = ide_panel.diag_copied_time {
                        if time.elapsed().as_secs_f32() < 2.0 {
                            true
                        } else { false }
                    } else { false }
                } else { false }
            } else { false };

            let copy_str = if is_copied { "✓ Скопировано" } else { "Копировать" };
            let copy_w = self.measure_ui_width(copy_str, 0.8);
            let copy_x = bx + box_w - pad - copy_w - 8.0 * s;
            let copy_y = by + pad - 2.0 * s;

            let mx = if show_settings || dialog_window_open { -1.0 } else { self.last_mouse_x };
            let my = if show_settings || dialog_window_open { -1.0 } else { self.last_mouse_y };

            let copy_hover = mx >= copy_x - 4.0 * s && mx <= copy_x + copy_w + 4.0 * s && my >= copy_y && my <= copy_y + 18.0 * s;
            if copy_hover {
                self.push_rounded_rect(copy_x - 4.0 * s, copy_y, copy_w + 8.0 * s, 18.0 * s, 3.0 * s, [1.0, 1.0, 1.0, 0.1]);
                wants_pointer = true;
            }

            ui_registry.register_rect(
                crate::ui_system::UiId::CopyDiagnostic(idx),
                copy_x - 4.0 * s, copy_y, copy_w + 8.0 * s, 18.0 * s, mx, my
            );

            let copy_col = if is_copied {[0.3, 0.9, 0.4, 1.0] } else if copy_hover {[0.9, 0.9, 0.9, 1.0] } else {[0.6, 0.6, 0.6, 1.0] };
            self.draw_string_scaled(copy_str, copy_x, copy_y + 10.0 * s, copy_col, 0.8);

            self.push_rect(bx + pad, by + pad + 18.0 * s, box_w - pad * 2.0, 1.0, [1.0, 1.0, 1.0, 0.1]);

            let mut text_y = by + pad + 24.0 * s + 10.0 * s;
            for line in lines {
                self.draw_string_scaled(&line, bx + pad, text_y,[0.9, 0.9, 0.9, 1.0], 0.9);
                text_y += line_h;
            }

            ui_registry.register_blocker(
                crate::ui_system::UiId::BottomPanelBody,
                bx, by, box_w, box_h, mx, my
            );
        }

        self.flush();

        (wants_pointer | ui_registry.wants_pointer(), target_sticky_lines)
    }

    fn draw_minimap(
        &mut self,
        editor: &Editor,
        spans: &[ColorSpan],
        render_scroll_y: f32,
        max_scroll: f32,
        total_lines: usize,
        visible_cursor_line: usize,
    ) {
        let scroll_ratio_y = if max_scroll > 0.0 {
            (render_scroll_y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let minimap_w = self.minimap_width;
        let minimap_x = self.width - minimap_w;
        let total_lines_f32 = total_lines as f32;

        let minimap_line_h = (self.height / (total_lines_f32 + 2.0).max(200.0))
            .max(self.height / 1250.0)
            .max(1.5);

        let max_minimap_scroll = ((total_lines_f32 + 2.0) * minimap_line_h - self.height).max(0.0);
        let current_minimap_scroll = (scroll_ratio_y * max_minimap_scroll).round();

        let current_visible_top_line = render_scroll_y / self.line_height;
        let viewport_y =
            (current_visible_top_line * minimap_line_h - current_minimap_scroll).round();
        let visible_lines = self.height / self.line_height;
        let max_viewport_lines = visible_lines.min(total_lines_f32 + 2.0);
        let viewport_h = (max_viewport_lines * minimap_line_h).max(4.0);

        let view_bg = [
            self.theme.sel[0],
            self.theme.sel[1],
            self.theme.sel[2],
            0.15,
        ];
        let view_border = [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 1.0];

        self.push_rect(minimap_x, viewport_y, minimap_w, viewport_h, view_bg);
        self.push_rect(minimap_x, viewport_y, minimap_w, 2.0, view_border);
        self.push_rect(
            minimap_x,
            viewport_y + viewport_h - 2.0,
            minimap_w,
            2.0,
            view_border,
        );
        self.push_rect(minimap_x, viewport_y, 2.0, viewport_h, view_border);
        self.flush();

        let map_bg = self.theme.minimap_bg;
        let mut current_y: f32 = 0.0;
        let mut phys_line = 0;
        let rect_h = minimap_line_h.ceil().max(1.0);

        let view_top = current_minimap_scroll;
        let view_bottom = current_minimap_scroll + self.height;

        let (first, second) = editor.text_parts();
        let first_len = first.len();

        while phys_line < editor.line_offsets.len() {
            let start_byte = editor.line_offsets[phys_line];
            let is_folded = editor.folded_lines.contains(&phys_line)
                && editor.foldable_lines.contains_key(&phys_line);

            if current_y > view_bottom {
                break;
            }

            if current_y + minimap_line_h >= view_top {
                let mut end_byte = if phys_line + 1 < editor.line_offsets.len() {
                    editor.line_offsets[phys_line + 1]
                } else {
                    editor.len()
                };

                if is_folded {
                    end_byte -= 1;
                }

                let mut current_x = minimap_x + 5.0;
                let mut cur_byte = start_byte;

                let mut span_idx_mini = match spans.binary_search_by_key(&cur_byte, |s| s.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };

                let y1 = (current_y - current_minimap_scroll).round();
                let y2 = y1 + rect_h;

                while cur_byte < end_byte {
                    let text_chunk = if cur_byte < first_len {
                        &first[cur_byte..end_byte.min(first_len)]
                    } else {
                        &second[cur_byte - first_len..end_byte - first_len]
                    };

                    let mut spaces_len = 0;
                    for c in text_chunk.chars() {
                        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                            spaces_len += c.len_utf8();
                        } else {
                            break;
                        }
                    }

                    if spaces_len > 0 {
                        let capped_spaces = spaces_len.min(5);
                        current_x += 1.5 * (capped_spaces as f32);
                        cur_byte += spaces_len;
                        if current_x >= minimap_x + minimap_w - 5.0 {
                            break;
                        }
                        continue;
                    }

                    while span_idx_mini < spans.len() && spans[span_idx_mini].end <= cur_byte {
                        span_idx_mini += 1;
                    }

                    let (span_end, raw_color) = if span_idx_mini < spans.len() {
                        let sp = &spans[span_idx_mini];
                        if sp.start <= cur_byte {
                            (sp.end.min(end_byte), sp.color)
                        } else {
                            (sp.start.min(end_byte), self.theme.fg)
                        }
                    } else {
                        (end_byte, self.theme.fg)
                    };

                    let color = [
                        raw_color[0] * 0.8 + map_bg[0] * 0.2,
                        raw_color[1] * 0.8 + map_bg[1] * 0.2,
                        raw_color[2] * 0.8 + map_bg[2] * 0.2,
                        1.0,
                    ];

                    let mut word_len = 0;
                    for c in text_chunk.chars() {
                        if cur_byte + word_len >= span_end
                            || c == ' '
                            || c == '\t'
                            || c == '\n'
                            || c == '\r'
                        {
                            break;
                        }
                        word_len += c.len_utf8();
                    }

                    if word_len == 0 {
                        if let Some(c) = text_chunk.chars().next() {
                            word_len = c.len_utf8();
                        }
                    }

                    let w = (word_len as f32 * 1.5).min(minimap_x + minimap_w - 5.0 - current_x);

                    if w > 0.0 {
                        let x1 = current_x.round();
                        let x2 = (current_x + w).round();

                        let sdf = [0.0, 0.0, 0.0];
                        let v1 = Vertex {
                            pos: [x1, y1],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };
                        let v2 = Vertex {
                            pos: [x2, y1],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };
                        let v3 = Vertex {
                            pos: [x2, y2],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };
                        let v4 = Vertex {
                            pos: [x1, y2],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };

                        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
                        if self.vertices.len() >= crate::renderer::MAX_VERTICES - 6 {
                            self.flush();
                        }
                        current_x += w;
                    }

                    cur_byte += word_len;
                    if current_x >= minimap_x + minimap_w - 5.0 {
                        break;
                    }
                }
            }

            current_y += minimap_line_h;

            if is_folded {
                if let Some(&fold_end) = editor.foldable_lines.get(&phys_line) {
                    phys_line = fold_end;
                }
            }
            phys_line += 1;
        }

        self.flush();

        let y_cursor =
            (visible_cursor_line as f32 * minimap_line_h - current_minimap_scroll).round();
        self.push_rect(
            minimap_x,
            y_cursor,
            minimap_w,
            2.0,
            self.theme.minimap_cursor,
        );
    }

    

    
}
