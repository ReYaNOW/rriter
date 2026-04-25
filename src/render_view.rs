pub mod core_text;
pub mod diag_popup_ui;
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
            let sb_w = 48.0 * s;

            // Сайдбар рисуется на полную высоту окна (real_height)self.push_rect(0.0, 0.0, sb_w, real_height, sidebar_bg);
            self.push_rect(sb_w - 1.0, 0.0, 1.0, real_height, [1.0, 1.0, 1.0, 0.12]);

            let btn_size = sb_w;
            let btn_gap = 0.0;
            let btn_x = 0.0;
            let top_start_y = 0.0;

            let mut top_idx = 0usize;
            let mut bottom_idx = 0usize;

            let lsp_has_issues = lsp.map_or(false, |l| {
                l.diagnostics.values().any(|diags| {
                    diags.iter().any(|d| {
                        d.severity == crate::lsp::DiagSeverity::Error
                            || d.severity == crate::lsp::DiagSeverity::Warning
                    })
                })
            });

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

                let custom_color = if slot.id == crate::app::PanelId::Problems {
                    if lsp_has_issues {
                        Some([1.0, 0.8, 0.1, 1.0])
                    } else {
                        Some([0.69, 0.745, 0.773, 1.0])
                    }
                } else {
                    None
                };

                let btn = IconButton {
                    x: btn_x,
                    y: btn_y,
                    size: btn_size,
                    icon: Some(slot.id.icon()),
                    is_active: slot.open,
                    icon_size: Some(36.0 * s),
                    active_square_width: Some(sb_w),
                    custom_color,
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
                        let ghost_color = if slot.id == crate::app::PanelId::Problems {
                            if lsp_has_issues {
                                Some([1.0, 0.8, 0.1, 1.0])
                            } else {
                                Some([0.69, 0.745, 0.773, 1.0])
                            }
                        } else {
                            None
                        };
                        let ghost = IconButton {
                            x: btn_x,
                            y: ghost_y,
                            size: btn_size,
                            icon: Some(slot.id.icon()),
                            is_active: false,
                            icon_size: Some(36.0 * s),
                            active_square_width: None,
                            custom_color: ghost_color,
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

                let open_top_count = ide_panel
                    .slots
                    .iter()
                    .filter(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                    .count();

                if open_top_count == 1 {
                    let slot = ide_panel
                        .slots
                        .iter()
                        .find(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                        .unwrap();
                    let label = slot.id.label();
                    self.draw_string_scaled(
                        label,
                        panel_x + 12.0 * s,
                        title_h / 2.0 + 6.0 * s,
                        self.theme.fg,
                        0.9,
                    );
                } else {
                    let mut tx = panel_x + 6.0 * s;
                    for (i, slot) in ide_panel
                        .slots
                        .iter()
                        .filter(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                        .enumerate()
                    {
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

                // (Ручка ресайза была здесь, перенесена в конец блока левой панели)

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

                            let is_hovered = ide_panel.file_tree_hovered_idx == Some(i)
                                || ui_registry.hovered()
                                    == Some(crate::ui_system::UiId::FileTreeNode(i));

                            if is_hovered && !is_ui_disabled {
                                self.push_rect(
                                    panel_x,
                                    row_y,
                                    panel_left_w,
                                    row_h,
                                    [1.0, 1.0, 1.0, 0.06],
                                );
                            }

                            let indent_x = panel_x + 8.0 * s + node.depth as f32 * indent_w;
                            let mut has_error = false;
                            let mut has_warn = false;
                            if !node.is_ignored {
                                if let Some(l) = lsp {
                                    for (p, diags) in &l.diagnostics {
                                        if !diags.is_empty() && p.starts_with(&node.path) {
                                            for d in diags {
                                                if d.severity == crate::lsp::DiagSeverity::Error {
                                                    has_error = true;
                                                    break;
                                                } else if d.severity
                                                    == crate::lsp::DiagSeverity::Warning
                                                {
                                                    has_warn = true;
                                                }
                                            }
                                            if has_error {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            let text_y = row_y + row_h / 2.0 + 5.5 * s;
                            let color: [f32; 4] = if node.is_ignored {
                                [0.973, 0.584, 0.502, 0.8]
                            } else if node.is_dir {
                                [0.78, 0.68, 1.0, 1.0]
                            } else {
                                [0.651, 0.686, 0.918, 1.0]
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
                                let text_x = dir_icon_x + icon_size + 4.0 * s;
                                self.draw_string_scaled(
                                    &node.name,
                                    text_x,
                                    text_y,
                                    color,
                                    tree_text_scale,
                                );
                                if has_error || has_warn {
                                    let sq_w = self.measure_ui_width(&node.name, tree_text_scale);
                                    let sq_color = if has_error {
                                        self.theme.diag_error
                                    } else {
                                        self.theme.diag_warn
                                    };
                                    self.push_squiggle(text_x, text_y + 2.0 * s, sq_w, sq_color);
                                }
                            } else {
                                let file_icon_x = indent_x + 10.0 * s;
                                self.draw_file_icon(
                                    node.icon_key,
                                    false,
                                    file_icon_x,
                                    icon_y,
                                    icon_size,
                                );
                                let text_x = file_icon_x + icon_size + 4.0 * s;
                                self.draw_string_scaled(
                                    &node.name,
                                    text_x,
                                    text_y,
                                    color,
                                    tree_text_scale,
                                );
                                if has_error || has_warn {
                                    let sq_w = self.measure_ui_width(&node.name, tree_text_scale);
                                    let sq_color = if has_error {
                                        self.theme.diag_error
                                    } else {
                                        self.theme.diag_warn
                                    };
                                    self.push_squiggle(text_x, text_y + 2.0 * s, sq_w, sq_color);
                                }
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

                // Подсветка ручки ресайза (wants_pointer=false — курсор управляется в events.rs через EwResize)
                // Не подсвечиваем, когда терминал в фокусе
                let resize_x = panel_x + panel_left_w;
                if !is_ui_disabled
                    && mx >= resize_x - 8.0 * s
                    && mx <= resize_x + 8.0 * s
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
            }
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
                        let screen_x = guide_x - render_scroll_x;

                        if screen_x > self.width {
                            break;
                        }
                        if screen_x < self.left_padding - 10.0 {
                            continue;
                        }

                        let margin = space_adv * 0.5;
                        let overlaps = v_line.text_px_width > 0.0
                            && text_start_x <= guide_x + margin
                            && text_end_x >= guide_x - margin;

                        if !overlaps {
                            self.push_rect(
                                screen_x.round(),
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

        self.mod_intervals_cache.clear();
        self.merged_intervals_cache.clear();
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
                    self.mod_intervals_cache.push(ModInterval {
                        top: y_top - 3.0,
                        bottom: y_top + 3.0,
                        state: st,
                    });
                }
            }

            if let Some(st) = editor.get_line_modification_state(phys_idx) {
                self.mod_intervals_cache.push(ModInterval {
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
                    self.mod_intervals_cache.push(ModInterval {
                        top: last_bottom_y - 3.0,
                        bottom: last_bottom_y + 3.0,
                        state: st,
                    });
                }
            }
        }

        for int in &self.mod_intervals_cache {
            let mut merged = false;
            if let Some(last) = self.merged_intervals_cache.last_mut() {
                if int.top <= last.bottom + 0.1 && int.state == last.state {
                    last.bottom = last.bottom.max(int.bottom);
                    merged = true;
                }
            }
            if !merged {
                self.merged_intervals_cache.push(*int);
            }
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
            let mut identical_idx = self
                .identical_words_cache
                .partition_point(|&(_, e)| e <= start_byte);
            let mut unused_idx = unused_spans.partition_point(|&(_, e)| e <= start_byte);

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
                    while identical_idx < self.identical_words_cache.len()
                        && self.identical_words_cache[identical_idx].1 <= current_offset
                    {
                        identical_idx += 1;
                    }
                    while unused_idx < unused_spans.len()
                        && unused_spans[unused_idx].1 <= current_offset
                    {
                        unused_idx += 1;
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

                    let is_identical = identical_idx < self.identical_words_cache.len()
                        && current_offset >= self.identical_words_cache[identical_idx].0;

                    let is_unused = unused_idx < unused_spans.len()
                        && current_offset >= unused_spans[unused_idx].0;

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

                                if is_unused {
                                    current_color = self.theme.unused;
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
        let mut mouse_in_popup = false;

        let type_rect = crate::app::mouse::HOVER_STATE.with(|s| s.borrow().rect);
        let diag_rect = self.last_diag_popup_rect;

        if let Some((rx, ry, rw, rh, anchor_x_start, anchor_x_end, anchor_y)) = diag_rect {
            let anchor_x = (anchor_x_start + anchor_x_end) * 0.5;
            if crate::app::mouse::is_in_hover_popup_or_bridge(
                mx,
                my,
                (rx, ry, rw, rh),
                anchor_x,
                anchor_y,
                anchor_y - 10.0 * self.scale_factor,
                anchor_y + 10.0 * self.scale_factor,
                self.width,
                self.scale_factor,
            ) {
                mouse_in_popup = true;
            }
        }

        if !mouse_in_popup {
            if let Some(tr) = type_rect {
                let (has_type_meta, anchor_x, anchor_y, line_top_y, line_bottom_y) =
                    crate::app::mouse::HOVER_STATE.with(|s| {
                        let s = s.borrow();
                        if let Some(popup) = &s.popup {
                            // Assuming phys_line mapping happens correctly in mouse.rs, here we just do a pad fallback
                            // if we don't have all the exact details. Or we can just use the popup rect + padding.
                            (
                                true,
                                popup.anchor_x,
                                popup.anchor_y,
                                popup.anchor_y - 10.0 * self.scale_factor,
                                popup.anchor_y + 10.0 * self.scale_factor,
                            )
                        } else {
                            (false, 0.0, 0.0, 0.0, 0.0)
                        }
                    });

                if has_type_meta {
                    if crate::app::mouse::is_in_hover_popup_or_bridge(
                        mx,
                        my,
                        tr,
                        anchor_x,
                        anchor_y,
                        line_top_y,
                        line_bottom_y,
                        self.width,
                        self.scale_factor,
                    ) {
                        mouse_in_popup = true;
                    }
                } else {
                    let pad = 24.0 * self.scale_factor;
                    if mx >= tr.0 - pad
                        && mx <= tr.0 + tr.2 + pad
                        && my >= tr.1 - pad
                        && my <= tr.1 + tr.3 + pad
                    {
                        mouse_in_popup = true;
                    }
                }
            }
        }

        // LSP squiggles — волнистые подчёркивания диагностик
        self.hovered_diags_cache.clear();
        let mut hovered_diag_type_target = None;
        if !lsp_diagnostics.is_empty() {
            let render_scroll_x = scroll_x.round();
            for (idx, diag) in lsp_diagnostics.iter().enumerate() {
                // Цвет по severity
                let color: [f32; 4] = match diag.severity {
                    crate::lsp::DiagSeverity::Error => [0.96, 0.26, 0.21, 0.90],
                    crate::lsp::DiagSeverity::Warning => [0.95, 0.9, 0.3, 0.90],
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
                let display_end_col = if diag.end_line == diag.start_line {
                    diag.end_col
                } else {
                    u32::MAX
                };
                let diag_hover_range = crate::app::mouse::diagnostic_hover_byte_range_on_line(
                    editor,
                    line,
                    diag.start_col,
                    display_end_col,
                );
                let hit_start_byte = diag_hover_range.map(|range| range.0);
                let hit_end_byte = diag_hover_range.map(|range| range.1);
                let hit_type_target = diag_hover_range.map(|range| range.2);

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
                    if !end_found && utf16_before >= display_end_col {
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
                        cur_x.max(x_start_px + avg_adv * 4.0)
                    };
                }

                let mut hit_x_start_px = x_start_px;
                let mut hit_x_end_px = x_end_px;
                let mut hit_cur_x = 0.0f32;
                let mut hit_start_found = hit_start_byte.is_none();
                let mut hit_end_found = hit_end_byte.is_none();
                editor.utf16_col_to_byte_advance(line, |ch, _utf16_before, pos| {
                    if let Some(hit_start_byte) = hit_start_byte {
                        if !hit_start_found && pos >= hit_start_byte {
                            hit_x_start_px = hit_cur_x;
                            hit_start_found = true;
                        }
                    }
                    if let Some(hit_end_byte) = hit_end_byte {
                        if !hit_end_found && pos >= hit_end_byte {
                            hit_x_end_px = hit_cur_x;
                            hit_end_found = true;
                        }
                    }
                    hit_cur_x += if ch == '\t' {
                        self.char_advance(' ') * 4.0
                    } else {
                        self.char_advance(ch)
                    };
                });
                if !hit_start_found {
                    hit_x_start_px = hit_cur_x;
                }
                if !hit_end_found {
                    hit_x_end_px = hit_cur_x.max(hit_x_start_px + avg_adv * 4.0);
                }

                let x_start = self.left_padding + x_start_px - render_scroll_x;
                let x_end = self.left_padding + x_end_px - render_scroll_x;
                let squiggle_w = (x_end - x_start).max(avg_adv / 2.0);
                let hit_x_start = self.left_padding + hit_x_start_px - render_scroll_x;
                let hit_x_end = self.left_padding + hit_x_end_px - render_scroll_x;
                let hit_w = (hit_x_end - hit_x_start).max(avg_adv / 2.0);

                let top_y = v_line.y_offset - render_scroll_y;

                let mut in_hitbox = false;
                let mut effective_bottom_h = panel_bottom_h;
                if is_ide_mode
                    && ide_panel.is_open(crate::app::PanelId::Terminal)
                    && !is_ui_disabled
                {
                    effective_bottom_h = 0.0;
                }
                let is_under_panel = top_y > self.height - effective_bottom_h - self.line_height;

                if !self.hide_popups_until_mouse_move && !is_under_panel {
                    let squiggle_hit_y_top = top_y;
                    let squiggle_hit_y_bottom = top_y + self.line_height;

                    if mouse_in_popup {
                        if self.last_hovered_diags.contains(&idx) {
                            in_hitbox = true;
                        }
                    } else if mx >= hit_x_start
                        && mx <= hit_x_start + hit_w
                        && my >= squiggle_hit_y_top
                        && my <= squiggle_hit_y_bottom
                    {
                        in_hitbox = true;
                    }
                }

                if in_hitbox {
                    if hovered_diag_type_target.is_none() {
                        hovered_diag_type_target = hit_type_target;
                    }

                    self.hovered_diags_cache.push((
                        idx,
                        x_start,
                        top_y,
                        top_y + self.line_height,
                        x_end,
                    ));
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
            let sb_w = 48.0 * s;
            let panel_x = sb_w;
            let panel_y = self.height - panel_bottom_h;
            let panel_w = self.width - panel_x;

            let is_terminal = ide_panel.slots.iter().any(|sl| {
                sl.group == crate::app::PanelGroup::Bottom
                    && sl.open
                    && sl.id == crate::app::PanelId::Terminal
            });
            // Прозрачность терминала (0.0 - полностью прозрачный, 1.0 - непрозрачный)
            let panel_alpha = if is_terminal { 0.80 } else { 1.0 };

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
            if !is_terminal || is_ui_disabled {
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

            let mut tx = panel_x + 8.0 * s;
            for (i, slot) in ide_panel
                .slots
                .iter()
                .filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
                .enumerate()
            {
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
            if my >= panel_y - 8.0 * s && my <= panel_y + 8.0 * s && mx >= panel_x {
                self.push_rect(panel_x, panel_y, panel_w, 2.0, [0.60, 0.35, 0.85, 0.4]);
            }

            // Плейсхолдер контента
            let content_y = panel_y + 1.0 + tab_h;
            let content_h = panel_bottom_h - 1.0 - tab_h;
            if content_h > 8.0 * s {
                if let Some(slot) = ide_panel
                    .slots
                    .iter()
                    .find(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
                {
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
                    } else if slot.id == crate::app::PanelId::Problems {
                        self.draw_problems_panel(
                            panel_x,
                            content_y,
                            panel_w,
                            content_h,
                            s,
                            lsp,
                            ide_panel,
                            ui_registry,
                        );
                    } else if slot.id == crate::app::PanelId::Terminal {
                        self.draw_terminal_panel(
                            panel_x,
                            content_y,
                            panel_w,
                            content_h,
                            s,
                            ide_panel,
                            ui_registry,
                            mx,
                            my,
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
        if self.hovered_diags_cache.is_empty() {
            self.last_diag_popup_rect = None;
            self.last_hovered_diags.clear();
        }

        let now = std::time::Instant::now();
        let popup_dt = self
            .last_draw_instant
            .map(|t| now.duration_since(t).as_secs_f32().min(0.1))
            .unwrap_or(0.0);
        self.last_draw_instant = Some(now);

        let (
            has_type_popup,
            is_hover_pending,
            hover_timer,
            has_byte_offset,
            hover_byte_offset,
            type_popup_byte,
        ) = crate::app::mouse::HOVER_STATE.with(|s| {
            let state = s.borrow();
            (
                state.popup.is_some(),
                state.request_id.is_some() || state.definition_request_id.is_some(),
                state.timer,
                state.byte_offset.is_some(),
                state
                    .byte_offset
                    .or_else(|| state.popup.as_ref().map(|p| p.byte_offset)),
                state.popup.as_ref().map(|p| p.byte_offset),
            )
        });

        if self.hovered_diags_cache.is_empty() {
            if let Some(byte_offset) = hover_byte_offset {
                let hover_line = editor
                    .line_offsets
                    .partition_point(|&o| o <= byte_offset)
                    .saturating_sub(1);

                if hover_line < editor.line_offsets.len() {
                    let mut found = None;

                    for (idx, diag) in lsp_diagnostics.iter().enumerate() {
                        if hover_line < diag.start_line as usize
                            || hover_line > diag.end_line as usize
                        {
                            continue;
                        }

                        let display_start_col = if hover_line == diag.start_line as usize {
                            diag.start_col
                        } else {
                            0
                        };
                        let display_end_col = if hover_line == diag.end_line as usize {
                            diag.end_col
                        } else {
                            u32::MAX
                        };

                        if let Some((hit_start_byte, hit_end_byte, type_target)) =
                            crate::app::mouse::diagnostic_hover_byte_range_on_line(
                                editor,
                                hover_line,
                                display_start_col,
                                display_end_col,
                            )
                        {
                            if byte_offset >= hit_start_byte && byte_offset < hit_end_byte {
                                found = Some((
                                    idx,
                                    diag,
                                    display_start_col,
                                    display_end_col,
                                    type_target,
                                ));
                                break;
                            }
                        }
                    }

                    if let Some((idx, _diag, display_start_col, display_end_col, type_target)) =
                        found
                    {
                        let mut v_line_opt = None;
                        for vl in &self.visual_lines {
                            if vl.physical_line == hover_line + 1 {
                                v_line_opt = Some(*vl);
                                break;
                            }
                        }

                        if let Some(v_line) = v_line_opt {
                            let render_scroll_x = scroll_x.round();
                            let top_y = v_line.y_offset - render_scroll_y;
                            let avg_adv = self.char_advance('a');
                            let mut x_start_px = 0.0f32;
                            let mut x_end_px = 0.0f32;
                            let mut cur_x = 0.0f32;
                            let mut start_found = false;
                            let mut end_found = false;

                            editor.utf16_col_to_byte_advance(
                                hover_line,
                                |ch, utf16_before, _pos| {
                                    if !start_found && utf16_before >= display_start_col {
                                        x_start_px = cur_x;
                                        start_found = true;
                                    }
                                    if !end_found && utf16_before >= display_end_col {
                                        x_end_px = cur_x;
                                        end_found = true;
                                    }
                                    cur_x += if ch == '\t' {
                                        self.char_advance(' ') * 4.0
                                    } else {
                                        self.char_advance(ch)
                                    };
                                },
                            );

                            if !start_found {
                                x_start_px = cur_x;
                            }
                            if !end_found {
                                x_end_px = cur_x.max(x_start_px + avg_adv * 4.0);
                            }

                            hovered_diag_type_target = Some(type_target);
                            self.hovered_diags_cache.push((
                                idx,
                                self.left_padding + x_start_px - render_scroll_x,
                                top_y,
                                top_y + self.line_height,
                                self.left_padding + x_end_px - render_scroll_x,
                            ));
                        }
                    }
                }
            }
        }

        self.last_hovered_diags.clear();
        self.last_hovered_diags
            .extend(self.hovered_diags_cache.iter().map(|h| h.0));
        let first_idx = self.last_hovered_diags.first().copied();

        let type_in_progress = has_byte_offset
            && !has_type_popup
            && (hover_timer < crate::app::mouse::HOVER_REQUEST_DELAY_SEC || is_hover_pending);
        let is_error_hovered = !self.hovered_diags_cache.is_empty();

        if first_idx != self.diag_hover_timer_idx {
            self.diag_hover_timer_idx = first_idx;
            self.diag_hover_timer = 0.0;
        } else if first_idx.is_some() || has_type_popup || type_in_progress {
            self.diag_hover_timer += popup_dt;
        }

        let error_timer_ready = self.diag_hover_timer >= 0.2;

        let (show_error, show_type, show_combined) = crate::app::mouse::compute_hover_visibility(
            is_error_hovered,
            error_timer_ready,
            has_type_popup,
            hovered_diag_type_target,
            type_popup_byte,
        );

        let (attached_hover_w, attached_hover_h) = if show_combined {
            crate::app::mouse::HOVER_STATE.with(|s| {
                let mut state = s.borrow_mut();
                if let Some(popup) = state.popup.as_mut() {
                    let scale = self.scale_factor;
                    let pad = 12.0 * scale;
                    let line_h = 22.0 * scale;
                    let max_text_w = (self.width - 80.0 * scale)
                        .min(820.0 * scale)
                        .max(320.0 * scale);
                    let cache_valid = popup.layout_cache.as_ref().is_some_and(|cache| {
                        cache.scale_factor == self.scale_factor
                            && cache.max_text_w == max_text_w
                            && cache.span_count == popup.spans.len()
                            && cache.text_len == popup.text.len()
                    });
                    if !cache_valid {
                        popup.layout_cache =
                            Some(self.build_hover_popup_layout(popup, max_text_w, line_h));
                    }
                    if let Some(layout) = popup.layout_cache.as_ref() {
                        (
                            layout.max_line_w + pad * 2.0,
                            (self.height * 0.35).min(layout.total_text_h + pad * 2.0),
                        )
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                }
            })
        } else {
            (0.0, 0.0)
        };

        if !show_type {
            crate::app::mouse::HOVER_STATE.with(|s| {
                s.borrow_mut().rect = None;
            });
        }

        if show_error {
            self.draw_diagnostic_popup(
                &lsp_diagnostics,
                ide_panel,
                ui_registry,
                attached_hover_w,
                attached_hover_h,
                mx,
                my,
                &mut wants_pointer,
            );
        } else {
            self.last_diag_popup_rect = None;
            self.max_diag_popup_scroll = 0.0;
            self.diag_popup_scroll.target = 0.0;
            self.diag_popup_scroll.current = 0.0;
        }

        crate::app::mouse::HOVER_STATE.with(|s| {
            let mut state = s.borrow_mut();
            if show_type {
                let selection = match (state.selection_anchor, state.selection_cursor) {
                    (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
                    _ => None,
                };
                if let Some(popup) = state.popup.as_mut() {
                    let (bx, by, bw, bh, ms) = self.draw_hover_popup(
                        popup,
                        selection,
                        editor,
                        ui_registry,
                        mx,
                        my,
                        render_scroll_y,
                        &mut wants_pointer,
                    );
                    state.rect = Some((bx, by, bw, bh));
                    state.max_scroll = ms;
                } else {
                    state.rect = None;
                }
            } else {
                state.rect = None;
            }
        });

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
