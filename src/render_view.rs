pub mod core_text;
pub mod lsp_ui;
pub mod search;
pub mod settings_ui;
pub mod sticky;
pub mod ui;

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
                let tab_bar_h = if show_welcome { 0.0 } else { 44.0 * s };
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
                let panel_bg =[
                    0.129, // #21
                    0.133, // #22
                    0.173, // #2c
                    1.0,
                ];
                // Левая панель не заходит под нижнюю — используем editor_height
                self.push_rect(panel_x, 0.0, panel_left_w, real_height, panel_bg);
                self.push_rect(
                    panel_x + panel_left_w - 1.0,
                    0.0,
                    1.0,
                    real_height,
                    [1.0, 1.0, 1.0, 0.12],
                );
                // Тонкая линия-разделитель между левой панелью и зоной номеров строк (аналог Indent Guide)
                let sep_x = (panel_x + panel_left_w).round();
                self.push_rect(
                    sep_x,
                    0.0,
                    1.0,
                    real_height,
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
                    && my <= real_height
                {
                    self.push_rect(
                        resize_x - 2.0,
                        0.0,
                        2.0,
                        real_height,
                        [0.60, 0.35, 0.85, 0.4],
                    );
                }
                                ui_registry.register_rect(
                    crate::ui_system::UiId::ResizeLeft,
                    resize_x - 4.0 * s,
                    0.0,
                    8.0 * s,
                    real_height,
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
                            real_height - title_h,
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
                            (real_height - title_h) as i32,
                        );
                    }

                    let row_h = 28.0 * s;
                    let indent_w = 18.0 * s;
                    let scroll = ide_panel.explorer_scroll.current.round();
                    let content_h = real_height - title_h;
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

                ui_registry.register_text_input(
            crate::ui_system::UiId::EditorTextBody,
            self.left_padding,
            tab_bar_h,
            scrollbar_x - self.left_padding,
            editor_height,
            mx,
            my,
        );

        let solid_minimap_bg =[self.theme.minimap_bg[0],
            self.theme.minimap_bg[1],
            self.theme.minimap_bg[2],
            1.0,
        ];

        let cursor_line_y = self.baseline_offset - render_scroll_y
            + (visible_cursor_line as f32 * self.line_height);

                if cursor_line_y > -self.line_height * 2.0
            && cursor_line_y < real_height + self.line_height
        {
            self.push_rect(self.left_padding,
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

                        let mut hovered_diags = Vec::new();
        let mut mouse_in_popup = false;
        if let Some(rect) = self.last_diag_popup_rect {
            // Расширяем "зону безопасности" попапа на 40px во все стороны, 
            // чтобы можно было вести мышь по диагонали от волнистой линии, и он не закрывался.
            let pad = 40.0 * self.scale_factor;
            if mx >= rect.0 - pad && mx <= rect.0 + rect.2 + pad && my >= rect.1 - pad && my <= rect.1 + rect.3 + pad {
                mouse_in_popup = true;
            }
        }

                // LSP squiggles — волнистые подчёркивания диагностик
        if !lsp_diagnostics.is_empty() {
            let render_scroll_x = scroll_x.round();
            for (idx, diag) in lsp_diagnostics.iter().enumerate() {
                // Цвет по severity
                let color:[f32; 4] = match diag.severity {
                    crate::lsp::DiagSeverity::Error =>[0.96, 0.26, 0.21, 0.90],
                    crate::lsp::DiagSeverity::Warning =>[0.95, 0.9, 0.3, 0.90],
                    crate::lsp::DiagSeverity::Info =>[0.26, 0.73, 0.90, 0.80],
                    crate::lsp::DiagSeverity::Hint =>[0.50, 0.50, 0.50, 0.70],
                };
                let line = diag.start_line as usize;
                if line >= editor.line_offsets.len() {
                    continue;
                }

                let mut v_line_opt = None;
                for vl in &self.visual_lines {
                    if vl.physical_line == line + 1 {
                        v_line_opt = Some(*vl);
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
                    if diag.end_line == diag.start_line
                        && !end_found
                        && utf16_before >= diag.end_col
                    {
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
                let squiggle_w = (x_end - x_start).max(avg_adv / 2.0);

                let top_y = v_line.y_offset - render_scroll_y;

                let mut in_hitbox = false;
                let is_under_panel = top_y > self.height - panel_bottom_h - self.line_height;

                if !self.hide_popups_until_mouse_move && !is_under_panel {
                    let squiggle_hit_y_top = top_y;
                    let squiggle_hit_y_bottom = top_y + self.line_height;

                    if mx >= x_start
                        && mx <= x_start + squiggle_w
                        && my >= squiggle_hit_y_top
                        && my <= squiggle_hit_y_bottom
                    {
                        in_hitbox = true;
                    }

                    if mouse_in_popup && self.last_hovered_diags.contains(&idx) {
                        in_hitbox = true;
                    }
                }

                if in_hitbox {
                    hovered_diags.push((idx, diag.clone(), x_start, top_y, top_y + self.line_height));
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
            tab_bar_h,
            self.left_padding - gutter_x,
            editor_height,
            solid_minimap_bg,
        );
        // Левая граница гаттера (отделяет IDE панель от зоны номеров строк)
        if is_ide_mode && panel_left_w > 0.0 {
            self.push_rect(
                gutter_x,
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

                for m in merged {
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

                                self.push_rect(minimap_x, tab_bar_h, minimap_w, editor_height, solid_minimap_bg);

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

                ui_registry.register_rect(crate::ui_system::UiId::EditorMinimap,
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
                [0.7, 0.33, 0.54, 1.0],                        );
                    }

                                        // --- 8.5. Линейка диагностики на скроллбаре ---
                    if !is_resizing && is_ide_mode && !dialog_window_open {
                        self.draw_diagnostics_ruler(editor, lsp_diagnostics, self.height);
                    }

                    if !show_welcome {
                        let tab_x = gutter_x;
                        let tab_w = self.width - tab_x;
                        self.draw_tab_bar(tabs, active_tab, editor, editor_title, editor_path, tab_x, 0.0, tab_w, tab_bar_h, s, mx, my, ui_registry);
                        self.flush();
                    }

                    let target_sticky_lines = self.draw_sticky_lines(editor,
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
                self.push_rect(panel_x, panel_y, panel_w, 2.0, [0.60, 0.35, 0.85, 0.4]);
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
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.6]);
        }

                // --- LSP Diagnostic Tooltip ---
        if hovered_diags.is_empty() {
            self.last_diag_popup_rect = None;
            self.last_hovered_diags.clear();
        }

        let now = std::time::Instant::now();
        let popup_dt = self
            .last_draw_instant
            .map(|t| now.duration_since(t).as_secs_f32().min(0.1))
            .unwrap_or(0.0);
        self.last_draw_instant = Some(now);

        let current_hovered_indices: Vec<usize> = hovered_diags.iter().map(|h| h.0).collect();
        let first_idx = current_hovered_indices.first().copied();

        if first_idx != self.diag_hover_timer_idx {
            self.diag_hover_timer_idx = first_idx;
            self.diag_hover_timer = 0.0;
        } else if first_idx.is_some() {
            self.diag_hover_timer += popup_dt;
        }
        let popup_ready = self.diag_hover_timer >= 0.2;

        self.last_hovered_diags = current_hovered_indices;

        if popup_ready && !hovered_diags.is_empty() {
            let s = self.scale_factor;
            let pad = 12.0 * s;
            let line_h = 22.0 * s;
            let icon_sz = 20.0 * s;
            let max_text_w = (self.width - 100.0 * s).max(300.0 * s).min(700.0 * s);

            struct DiagLayout {
                idx: usize,
                lines: Vec<String>,
                source_prefix: String,
                source_suffix: String,
                has_href: bool,
                source_on_new_line: bool,
                total_text_h: f32,
                severity: crate::lsp::DiagSeverity,
                is_copied: bool,
            }

            let mut layouts = Vec::new();
            let mut global_max_w = 180.0 * s;

            for (idx, diag, _, _, _) in &hovered_diags {
                let source_prefix = match (&diag.source, &diag.code) {
                    (Some(src), Some(_)) => format!("({} ", src),
                    (Some(src), None) => format!("({})", src),
                    (None, Some(_)) => "(".to_string(),
                    (None, None) => "(LSP)".to_string(),
                };
                let source_suffix = match &diag.code {
                    Some(code) => code.clone(),
                    None => String::new(),
                };
                let close_paren_w = if source_suffix.is_empty() { 0.0 } else { self.measure_ui_width(")", 1.0) };
                let source_full_w = self.measure_ui_width(&source_prefix, 1.0) + self.measure_ui_width(&source_suffix, 1.0) + close_paren_w;

                let mut lines = Vec::new();
                let mut cur_line = String::new();
                for word in diag.message.split_whitespace() {
                    let w = self.measure_ui_width(&format!("{} {}", cur_line, word), 1.0);
                    if w > max_text_w && !cur_line.is_empty() {
                        lines.push(cur_line);
                        cur_line = word.to_string();
                    } else {
                        if !cur_line.is_empty() { cur_line.push(' '); }
                        cur_line.push_str(word);
                    }
                }
                if !cur_line.is_empty() { lines.push(cur_line.clone()); }

                let last_line_w = self.measure_ui_width(lines.last().map(|st| st.as_str()).unwrap_or(""), 1.0);
                let mut source_on_new_line = false;

                if last_line_w + source_full_w + 10.0 * s > max_text_w {
                    lines.push(String::new());
                    source_on_new_line = true;
                }

                let mut box_text_w = 0.0f32;
                for l in &lines {
                    let lw = self.measure_ui_width(l, 1.0);
                    if lw > box_text_w { box_text_w = lw; }
                }
                if source_on_new_line && source_full_w > box_text_w {
                    box_text_w = source_full_w;
                } else if !source_on_new_line {
                    let combined_w = last_line_w + 8.0 * s + source_full_w;
                    if combined_w > box_text_w { box_text_w = combined_w; }
                }

                let item_w = box_text_w + pad * 2.0 + icon_sz + 16.0 * s;
                if item_w > global_max_w { global_max_w = item_w; }

                let total_text_h = lines.len() as f32 * line_h;
                layouts.push(DiagLayout {
                    idx: *idx,
                    lines,
                    source_prefix,
                    source_suffix,
                    has_href: diag.code_href.is_some(),
                    source_on_new_line,
                    total_text_h,
                    severity: diag.severity,
                    is_copied: ide_panel.diag_copied_idx == Some(*idx),
                });
            }

            let box_w = global_max_w;
            let mut total_h = pad * 2.0;
            for l in &layouts {
                total_h += l.total_text_h;
            }
            total_h += (layouts.len() as f32 - 1.0) * (line_h * 0.5);

            let (_, _, first_diag_x, first_line_y_top, first_diag_y_bottom) = hovered_diags[0];
            let mut bx = first_diag_x;
            if bx + box_w > self.width - 20.0 * s {
                bx = self.width - box_w - 20.0 * s;
            }
            let mut by = first_line_y_top - total_h - 8.0 * s;
            if by < 0.0 {
                by = first_diag_y_bottom + 8.0 * s;
            }

            self.last_diag_popup_rect = Some((bx, by, box_w, total_h));

            ui_registry.register_blocker(crate::ui_system::UiId::BottomPanelBody, bx, by, box_w, total_h, mx, my);

            let popup_hovered = mx >= bx && mx <= bx + box_w && my >= by && my <= by + total_h;
            if popup_hovered && !wants_pointer {
                ui_registry.reset_cursor_state();
            }

            self.push_rounded_rect(bx.round() - 1.0, by.round() - 1.0, box_w.round() + 2.0, total_h.round() + 2.0, 6.0 * s, [0.4, 0.4, 0.45, 0.6]);
            self.push_rounded_rect(bx.round(), by.round(), box_w.round(), total_h.round(), 6.0 * s, [self.theme.minimap_bg[0], self.theme.minimap_bg[1], self.theme.minimap_bg[2], 1.0]);

            let mut current_y = by + pad;

            for layout in layouts {
                let border_color = match layout.severity {
                    crate::lsp::DiagSeverity::Error =>[0.96, 0.26, 0.21, 1.0],
                    crate::lsp::DiagSeverity::Warning =>[0.95, 0.9, 0.3, 1.0],
                    crate::lsp::DiagSeverity::Info =>[0.26, 0.73, 0.90, 1.0],
                    crate::lsp::DiagSeverity::Hint => [0.50, 0.50, 0.50, 1.0],
                };

                self.push_rect(bx + 4.0 * s, current_y, 3.0 * s, layout.total_text_h, border_color);

                let mut text_y = current_y + line_h * 0.75;
                for (i, line) in layout.lines.iter().enumerate() {
                    self.draw_string_scaled(line, (bx + pad).round(), text_y.round(),[0.9, 0.9, 0.9, 1.0], 1.0);

                    if i == layout.lines.len() - 1 {
                        let line_w = self.measure_ui_width(line, 1.0);
                        let sx = if layout.source_on_new_line { (bx + pad).round() } else { (bx + pad + line_w + 8.0 * s).round() };
                        let sy = text_y.round();

                        let prefix_w = self.measure_ui_width(&layout.source_prefix, 1.0);
                        self.draw_string_scaled(&layout.source_prefix, sx, sy,[0.55, 0.55, 0.6, 1.0], 1.0);

                        if !layout.source_suffix.is_empty() {
                            let sfx_x = sx + prefix_w;
                            let sfx_w = self.measure_ui_width(&layout.source_suffix, 1.0);
                            let sfx_hovered = layout.has_href && mx >= sfx_x - 1.0 && mx <= sfx_x + sfx_w + 1.0 && my >= sy - line_h && my <= sy + 2.0 * s;
                            let link_color: [f32; 4] =[0.72, 0.52, 1.0, 1.0];
                            let sfx_color = if sfx_hovered { link_color } else { [link_color[0], link_color[1], link_color[2], 0.85] };

                            if layout.has_href {
                                let ul_alpha = if sfx_hovered { 0.9 } else { 0.55 };
                                self.push_rect(sfx_x, sy + 1.0, sfx_w, 1.0, [link_color[0], link_color[1], link_color[2], ul_alpha]);
                                if sfx_hovered { wants_pointer = true; }
                            }
                            self.draw_string_scaled(&layout.source_suffix, sfx_x, sy, sfx_color, 1.0);
                            self.draw_string_scaled(")", sfx_x + sfx_w, sy,[0.55, 0.55, 0.6, 1.0], 1.0);

                            if layout.has_href {
                                if let Some((_, diag, _, _, _)) = hovered_diags.iter().find(|h| h.0 == layout.idx) {
                                    self.last_diag_href = diag.code_href.clone();
                                }
                                ui_registry.register_rect(crate::ui_system::UiId::OpenDiagUrl(layout.idx), sfx_x - 1.0, sy - line_h, sfx_w + 2.0, line_h + 2.0 * s, mx, my);
                            }
                        }
                    }
                    text_y += line_h;
                }

                let btn_x = (bx + box_w - pad - icon_sz).round();
                let btn_y = (current_y + (layout.total_text_h - icon_sz) / 2.0).round();
                let btn_hovered = mx >= btn_x - 4.0 * s && mx <= btn_x + icon_sz + 4.0 * s && my >= btn_y - 2.0 * s && my <= btn_y + icon_sz + 4.0 * s;

                if btn_hovered {
                    self.push_rounded_rect(btn_x - 4.0 * s, btn_y - 2.0 * s, icon_sz + 8.0 * s, icon_sz + 4.0 * s, 4.0 * s, [1.0, 1.0, 1.0, 0.1]);
                    wants_pointer = true;
                }
                let icon_type = if layout.is_copied { crate::widgets::IconType::Check } else { crate::widgets::IconType::Copy };
                let icon_color = if layout.is_copied {[0.3, 0.9, 0.4, 1.0] } else { self.theme.fg };
                let icon_render_sz = 16.0 * s;
                let offset = (icon_sz - icon_render_sz) / 2.0;
                self.draw_atlas_icon(icon_type, btn_x + offset, btn_y + offset, icon_render_sz, icon_color);

                ui_registry.register_rect(crate::ui_system::UiId::CopyDiagnostic(layout.idx), btn_x - 4.0 * s, btn_y - 2.0 * s, icon_sz + 8.0 * s, icon_sz + 4.0 * s, mx, my);

                current_y += layout.total_text_h + line_h * 0.5;
            }
        }

        self.flush();

        (
            wants_pointer | ui_registry.wants_pointer(),
            target_sticky_lines,
        )
    }

        fn draw_tab_bar(
        &mut self,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        editor: &Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        mx: f32,
        my: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        self.push_rect(x, y, w, h, [self.theme.bg[0] - 0.02, self.theme.bg[1] - 0.02, self.theme.bg[2] - 0.02, 1.0]);

        let mut current_x = x;
                let tab_pad = 14.0 * s;

        for (i, tab) in tabs.iter().enumerate() {
            let is_active = i == active_tab;
            let title = if is_active {
                if editor_title.is_empty() { "Безымянный" } else { editor_title }
            } else {
                if tab.base_title.is_empty() { "Безымянный" } else { &tab.base_title }
            };

                                                let title_w = self.measure_ui_width(title, 1.0);
            let icon_size_tab = 20.0 * s;
                                                                        let tab_w = tab_pad * 2.0 + icon_size_tab + 5.0 * s + title_w + 26.0 * s;

            let is_hovered = mx >= current_x && mx <= current_x + tab_w && my >= y && my <= y + h;

            let bg_color = if is_active {
                [self.theme.bg[0] + 0.05, self.theme.bg[1] + 0.05, self.theme.bg[2] + 0.05, 1.0]
            } else if is_hovered {[self.theme.bg[0] + 0.02, self.theme.bg[1] + 0.02, self.theme.bg[2] + 0.02, 1.0]
            } else {[0.0, 0.0, 0.0, 0.0]
            };

            if bg_color[3] > 0.0 {
                self.push_rect(current_x, y, tab_w, h, bg_color);
            }

            if is_active {
                self.push_rect(current_x, y + h - 2.0 * s, tab_w, 2.0 * s,[0.60, 0.35, 0.85, 1.0]);
            }

                        let is_dirty = if is_active { editor.is_dirty() } else { tab.editor.is_dirty() };

            let icon_key = if is_active {
                                                crate::app::file_icons::file_icon_key(&title.to_lowercase())} else {
                tab.icon_key
            };

            let icon_y = y + (h - icon_size_tab) / 2.0;
            self.draw_file_icon(icon_key, false, current_x + tab_pad, icon_y, icon_size_tab);

            let text_color = if is_active { self.theme.fg } else { [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.6] };
                        let text_x = current_x + tab_pad + icon_size_tab + 5.0 * s;
            self.draw_string_scaled(title, text_x, y + h / 2.0 + 5.0 * s, text_color, 1.0);

                                                                                    ui_registry.register_rect(crate::ui_system::UiId::EditorTab(i), current_x, y, tab_w, h, mx, my);

                                                {
                                                                                                                                                                                                                let close_size = 20.0 * s;
                                                    let close_x = current_x + tab_w - tab_pad - close_size;
                                                    let close_y = y + (h - close_size) / 2.0;
                                                    let close_hovered = mx >= close_x - 4.0 * s && mx <= close_x + close_size + 4.0 * s && my >= close_y - 4.0 * s && my <= close_y + close_size + 4.0 * s;

                                                    let show_close = is_active || is_hovered;
                                                    if show_close {
                                                        if is_dirty && !close_hovered {
                                                            // Точка вместо крестика (VS Code стиль)
                                                            self.draw_string_scaled("●", close_x + close_size / 2.0 - 4.0 * s, close_y + close_size / 2.0 + 4.0 * s, [0.9, 0.9, 0.9, 1.0], 0.8);
                                                        } else {
                                                            if close_hovered {
                                                                self.push_rounded_rect(close_x - 4.0 * s, close_y - 4.0 * s, close_size + 8.0 * s, close_size + 8.0 * s, 4.0 * s, [1.0, 1.0, 1.0, 0.1]);
                                                            }
                                                                                                                                                                                    let icon_col = if close_hovered { [1.0, 1.0, 1.0, 1.0] } else { [1.0, 1.0, 1.0, 0.8] };
                                                            self.draw_atlas_icon(crate::widgets::IconType::Close, close_x, close_y, close_size, icon_col);
                                                        }
                                                    }

                                                    ui_registry.register_rect(crate::ui_system::UiId::EditorTabClose(i), close_x - 4.0 * s, close_y - 4.0 * s, close_size + 8.0 * s, close_size + 8.0 * s, mx, my);
                                                }

                        current_x += tab_w + 1.0;
            if i + 1 < tabs.len() {
                self.push_rect(current_x - 1.0, y + h * 0.2, 1.0, h * 0.6, [1.0, 1.0, 1.0, 0.1]);
            }
        }
    }

    fn draw_minimap(
        &mut self,
        editor: &Editor,
        spans: &[ColorSpan],
        render_scroll_y: f32,
        max_scroll: f32,
        total_lines: usize,
        visible_cursor_line: usize,
        editor_height: f32,
        tab_bar_h: f32,
    ) {
        let scroll_ratio_y = if max_scroll > 0.0 {
            (render_scroll_y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let minimap_w = self.minimap_width;
        let minimap_x = self.width - minimap_w;
                let total_lines_f32 = total_lines as f32;

        let minimap_line_h = (editor_height / (total_lines_f32 + 2.0).max(200.0))
            .max(editor_height / 1250.0)
            .max(1.5);

        let max_minimap_scroll = ((total_lines_f32 + 2.0) * minimap_line_h - editor_height).max(0.0);
        let current_minimap_scroll = (scroll_ratio_y * max_minimap_scroll).round();

        let current_visible_top_line = render_scroll_y / self.line_height;
        let viewport_y =
            tab_bar_h + (current_visible_top_line * minimap_line_h - current_minimap_scroll).round();
        let visible_lines = editor_height / self.line_height;
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
        let view_bottom = current_minimap_scroll + editor_height;

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

                let y1 = tab_bar_h + (current_y - current_minimap_scroll).round();
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
            tab_bar_h + (visible_cursor_line as f32 * minimap_line_h - current_minimap_scroll).round();
        self.push_rect(
            minimap_x,
            y_cursor,
            minimap_w,
            2.0,
            self.theme.minimap_cursor,
        );
    }
}
