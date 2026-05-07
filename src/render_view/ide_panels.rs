use crate::renderer::Renderer;
use crate::render_view::{
    cursor_line_and_character, diagnostic_error_warning_counts, ide_bottom_panel_y,
    ide_status_bar_height, ide_status_bar_y, language_display_name_for_ext, selected_char_count,
};
use crate::widgets::IconButton;
use glow::HasContext;

fn clipped_label_prefix_len<F>(
    text: &str,
    max_w: f32,
    ellipsis_w: f32,
    mut char_advance: F,
) -> usize
where
    F: FnMut(char) -> f32,
{
    if max_w <= ellipsis_w {
        return 0;
    }
    let mut used = 0.0;
    let mut prefix_len = 0usize;
    for (idx, ch) in text.char_indices() {
        let adv = char_advance(ch);
        if used + adv + ellipsis_w > max_w {
            return prefix_len;
        }
        used += adv;
        prefix_len = idx + ch.len_utf8();
    }
    text.len()
}

fn centered_dialog_button_positions(x: f32, w: f32, btn_w: f32, gap: f32) -> (f32, f32) {
    let total_w = btn_w * 2.0 + gap;
    let first_x = x + (w - total_w) / 2.0;
    (first_x, first_x + btn_w + gap)
}

fn file_tree_menu_group(action: crate::app::file_tree::FileTreeMenuAction) -> u8 {
    match action {
        crate::app::file_tree::FileTreeMenuAction::CreateFile
        | crate::app::file_tree::FileTreeMenuAction::CreateDirectory
        | crate::app::file_tree::FileTreeMenuAction::Paste => 0,
        crate::app::file_tree::FileTreeMenuAction::Delete
        | crate::app::file_tree::FileTreeMenuAction::Copy
        | crate::app::file_tree::FileTreeMenuAction::Cut
        | crate::app::file_tree::FileTreeMenuAction::Rename => 1,
        crate::app::file_tree::FileTreeMenuAction::OpenContainedFolder
        | crate::app::file_tree::FileTreeMenuAction::CopyAbsolutePath
        | crate::app::file_tree::FileTreeMenuAction::CopyRelativePath => 2,
    }
}

fn file_tree_menu_separator_before(
    entries: &[crate::app::file_tree::FileTreeMenuAction],
    idx: usize,
) -> bool {
    idx > 0 && file_tree_menu_group(entries[idx - 1]) != file_tree_menu_group(entries[idx])
}

fn file_tree_menu_separator_count(entries: &[crate::app::file_tree::FileTreeMenuAction]) -> usize {
    (1..entries.len())
        .filter(|&idx| file_tree_menu_separator_before(entries, idx))
        .count()
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_tree_label_clipped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        color: [f32; 4],
        scale: f32,
        scratch: &mut String,
    ) -> f32 {
        if max_w <= 0.0 {
            return 0.0;
        }
        let full_w = self.measure_ui_width(text, scale);
        if full_w <= max_w {
            self.draw_string_scaled(text, x, y, color, scale);
            return full_w;
        }

        let ellipsis = "…";
        let ellipsis_w = self.measure_ui_width(ellipsis, scale);
        if ellipsis_w > max_w {
            return 0.0;
        }
        let prefix_len = clipped_label_prefix_len(text, max_w, ellipsis_w, |ch| {
            self.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(0.0)
        });
        scratch.clear();
        scratch.push_str(&text[..prefix_len]);
        scratch.push_str(ellipsis);
        self.draw_string_scaled(scratch, x, y, color, scale);
        self.measure_ui_width(scratch, scale).min(max_w)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_ide_side_panels(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        lsp_has_diagnostics: bool,
        s: f32,
        mx: f32,
        my: f32,
        real_height: f32,
        panel_left_w: f32,
        is_ui_disabled: bool,
    ) {
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
                        lsp_has_diagnostics,
                        ui_registry,
                    );
                }
            }

            // --- Дерево файлов проводника ---
            if ide_panel.is_open(crate::app::PanelId::Explorer) {
                let file_tree_overlay_open =
                    crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel);
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

                let tree_text_scale = 1.0;
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
                    let mut label_scratch = String::new();

                    for i in first_vis..last_vis {
                        let node = &ide_panel.file_tree_nodes[i];
                        let row_y = title_h + i as f32 * row_h - scroll;

                        if !file_tree_overlay_open {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::FileTreeNode(i),
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                mx,
                                my,
                            );
                        }

                        let is_hovered = !file_tree_overlay_open
                            && (ide_panel.file_tree_hovered_idx == Some(i)
                                || ui_registry.hovered()
                                    == Some(crate::ui_system::UiId::FileTreeNode(i)));
                        let is_selected = ide_panel.file_tree_selection.contains(&node.path);

                        if is_selected {
                            self.push_rect(
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                [0.60, 0.35, 0.85, 0.24],
                            );
                        }

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
                            if !file_tree_overlay_open {
                                ui_registry.register_rect(
                                    crate::ui_system::UiId::FileTreeArrow(i),
                                    arrow_x - 4.0 * s,
                                    row_y,
                                    18.0 * s,
                                    row_h,
                                    mx,
                                    my,
                                );
                            }
                            let arrow_color = if node.is_ignored {
                                [0.973, 0.584, 0.502, 0.6]
                            } else {
                                [0.78, 0.68, 1.0, 0.7]
                            };
                            self.draw_string_scaled(arrow_str, arrow_x, arrow_y, arrow_color, 1.0);
                            // Иконка папки — правее стрелки
                            let dir_icon_x = indent_x + 18.0 * s;
                            self.draw_file_icon(node.icon_key, true, dir_icon_x, icon_y, icon_size);
                            let text_x = dir_icon_x + icon_size + 4.0 * s;
                            let max_text_w = (panel_x + panel_left_w - 10.0 * s - text_x).max(0.0);
                            let label_w = self.draw_tree_label_clipped(
                                &node.name,
                                text_x,
                                text_y,
                                max_text_w,
                                color,
                                tree_text_scale,
                                &mut label_scratch,
                            );
                            if has_error || has_warn {
                                let sq_color = if has_error {
                                    self.theme.diag_error
                                } else {
                                    self.theme.diag_warn
                                };
                                self.push_squiggle(text_x, text_y + 2.0 * s, label_w, sq_color);
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
                            let max_text_w = (panel_x + panel_left_w - 10.0 * s - text_x).max(0.0);
                            let label_w = self.draw_tree_label_clipped(
                                &node.name,
                                text_x,
                                text_y,
                                max_text_w,
                                color,
                                tree_text_scale,
                                &mut label_scratch,
                            );
                            if has_error || has_warn {
                                let sq_color = if has_error {
                                    self.theme.diag_error
                                } else {
                                    self.theme.diag_warn
                                };
                                self.push_squiggle(text_x, text_y + 2.0 * s, label_w, sq_color);
                            }
                        }
                    }

                    if let Some(drag) = &ide_panel.file_tree_drag {
                        if drag.threshold_passed {
                            if let Some(target_idx) = drag.target_idx {
                                if target_idx < total_nodes {
                                    let row_y = title_h + target_idx as f32 * row_h - scroll;
                                    self.push_rect(
                                        panel_x,
                                        row_y,
                                        panel_left_w,
                                        row_h,
                                        [0.52, 0.78, 0.58, 0.22],
                                    );
                                    self.push_rect(
                                        panel_x,
                                        row_y + row_h - 2.0,
                                        panel_left_w,
                                        2.0,
                                        [0.52, 0.78, 0.58, 0.85],
                                    );
                                }
                            }
                            let label = if drag.paths.len() == 1 {
                                drag.paths[0]
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("1 элемент")
                                    .to_string()
                            } else {
                                format!("{} элементов", drag.paths.len())
                            };
                            let ghost_w = self.measure_ui_width(&label, tree_text_scale) + 18.0 * s;
                            let ghost_x = drag.current_x + 12.0 * s;
                            let ghost_y = drag.current_y + 10.0 * s;
                            self.push_rounded_rect(
                                ghost_x,
                                ghost_y,
                                ghost_w,
                                26.0 * s,
                                5.0 * s,
                                [0.12, 0.13, 0.18, 0.92],
                            );
                            self.draw_string_scaled(
                                &label,
                                ghost_x + 9.0 * s,
                                ghost_y + 18.0 * s,
                                self.theme.fg,
                                tree_text_scale,
                            );
                        }
                    }

                    // Тонкий скроллбар
                    let total_h = total_nodes as f32 * row_h;
                    if total_h > content_h {
                        let max_s = (total_h - content_h).max(1.0);
                        let ratio = (scroll / max_s).clamp(0.0, 1.0);
                        let thumb_h = (content_h / total_h * (content_h - 8.0 * s)).max(20.0 * s);
                        let thumb_y = title_h + 4.0 * s + ratio * (content_h - 8.0 * s - thumb_h);
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_ide_bottom_panel(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        lsp_has_diagnostics: bool,
        s: f32,
        mx: f32,
        my: f32,
        panel_bottom_h: f32,
        _is_ui_disabled: bool,
    ) {
        let sb_w = 48.0 * s;
        let panel_x = sb_w;
        let panel_y = ide_bottom_panel_y(self.height, panel_bottom_h, s);
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
                        lsp_has_diagnostics,
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

    pub(crate) fn draw_status_bar(
        &mut self,
        editor: &crate::editor::Editor,
        editor_path: Option<&std::path::PathBuf>,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        s: f32,
        mx: f32,
        my: f32,
        panel_bottom_h: f32,
    ) {
        let bar_h = ide_status_bar_height(s).round();
        let bar_y = ide_status_bar_y(self.height, panel_bottom_h, s).round();
        let bar_x = (48.0 * s).round();
        let bar_w = (self.width - bar_x).max(0.0);
        if bar_w <= 1.0 || bar_h <= 1.0 {
            return;
        }

        self.push_rect(bar_x, bar_y, bar_w, bar_h, [0.118, 0.125, 0.165, 1.0]);
        self.push_rect(
            bar_x,
            bar_y,
            bar_w,
            1.0,
            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.12],
        );
        ui_registry.register_blocker(
            crate::ui_system::UiId::StatusBar,
            bar_x,
            bar_y,
            bar_w,
            bar_h,
            mx,
            my,
        );

        let (error_count, warning_count) = lsp
            .map(|l| diagnostic_error_warning_counts(l.diagnostics.values().map(|v| v.as_slice())))
            .unwrap_or((0, 0));

        let icon_sz = 20.0 * s;
        let text_scale = 0.95;
        let pad_x = 10.0 * s;
        let icon_gap = 5.0 * s;
        let item_gap = 16.0 * s;
        let diag_x = bar_x + pad_x;
        let icon_y = bar_y + (bar_h - icon_sz) / 2.0;
        let text_y = bar_y + bar_h / 2.0 + 5.0 * s;

        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", error_count));
        let error_w = self.measure_ui_width(&scratch, text_scale).round();
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", warning_count));
        let warning_w = self.measure_ui_width(&scratch, text_scale).round();

        let diagnostics_w =
            icon_sz + icon_gap + error_w + item_gap + icon_sz + icon_gap + warning_w + pad_x;
        let diagnostics_hovered = ui_registry.register_rect(
            crate::ui_system::UiId::StatusDiagnostics,
            diag_x - 4.0 * s,
            bar_y,
            diagnostics_w,
            bar_h,
            mx,
            my,
        );
        if diagnostics_hovered {
            self.push_rect(
                diag_x - 4.0 * s,
                bar_y,
                diagnostics_w,
                bar_h,
                [1.0, 1.0, 1.0, 0.07],
            );
        }

        self.draw_atlas_icon(
            crate::widgets::IconType::Error,
            diag_x,
            icon_y,
            icon_sz,
            [1.0, 1.0, 1.0, 1.0],
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", error_count));
        let error_text_x = diag_x + icon_sz + icon_gap;
        self.draw_string_scaled(
            &scratch,
            error_text_x,
            text_y,
            self.theme.fg,
            text_scale,
        );

        let warn_icon_x = error_text_x + error_w + item_gap;
        self.draw_atlas_icon(
            crate::widgets::IconType::Warning,
            warn_icon_x,
            icon_y,
            icon_sz,
            [1.0, 1.0, 1.0, 1.0],
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", warning_count));
        self.draw_string_scaled(
            &scratch,
            warn_icon_x + icon_sz + icon_gap,
            text_y,
            self.theme.fg,
            text_scale,
        );

        let ext = editor_path
            .and_then(|path| path.extension())
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let lang = language_display_name_for_ext(ext);
        scratch.clear();
        scratch.push_str(lang);
        let lang_w = self.measure_ui_width(&scratch, text_scale).round();
        let lang_x = (bar_x + bar_w - pad_x - lang_w).max(diag_x);
        self.draw_string_scaled(&scratch, lang_x, text_y, self.theme.fg, text_scale);

        let (line, character) = cursor_line_and_character(editor);
        const ZERO_SAMPLE: &str = "00000000000000000000";
        let item_gap = 14.0 * s;
        let digit_gap = 4.0 * s;
        let line_digits = line.to_string();
        let char_digits = character.to_string();
        let line_digits_w = self
            .measure_mono_width(
                &ZERO_SAMPLE[..line_digits.len().max(2).min(ZERO_SAMPLE.len())],
                text_scale,
            )
            .round();
        let char_digits_w = self
            .measure_mono_width(
                &ZERO_SAMPLE[..char_digits.len().max(2).min(ZERO_SAMPLE.len())],
                text_scale,
            )
            .round();
        let line_label_w = self.measure_ui_width("Стр", text_scale).round();
        let char_label_w = self.measure_ui_width("Сим", text_scale).round();
        let line_block_w = line_label_w + digit_gap + line_digits_w;
        let char_block_w = char_label_w + digit_gap + char_digits_w;
        let selected_count = selected_char_count(editor);
        let selected_count_digits = selected_count.map(|count| count.to_string());
        let selected_block_w = selected_count_digits
            .as_ref()
            .map(|digits| {
                self.measure_ui_width("(", text_scale).round()
                    + self
                        .measure_mono_width(
                            &ZERO_SAMPLE[..digits.len().max(2).min(ZERO_SAMPLE.len())],
                            text_scale,
                        )
                        .round()
                    + self.measure_ui_width(" выделено)", text_scale).round()
            })
            .unwrap_or(0.0);
        let pos_color = self.theme.fg;
        let mut group_w = line_block_w + item_gap + char_block_w;
        if selected_block_w > 0.0 {
            group_w += item_gap + selected_block_w;
        }
        let line_x = lang_x - 22.0 * s - group_w;
        if line_x > diag_x + diagnostics_w + 8.0 * s {
            self.draw_string_scaled("Стр", line_x, text_y, pos_color, text_scale);
            self.draw_string_mono_scaled(
                &line_digits,
                line_x + line_label_w + digit_gap,
                text_y,
                pos_color,
                text_scale,
            );
            let char_x = line_x + line_block_w + item_gap;
            self.draw_string_scaled("Сим", char_x, text_y, pos_color, text_scale);
            self.draw_string_mono_scaled(
                &char_digits,
                char_x + char_label_w + digit_gap,
                text_y,
                pos_color,
                text_scale,
            );
            if let Some(digits) = selected_count_digits.as_deref() {
                let selected_x = char_x + char_block_w + item_gap;
                self.draw_string_scaled("(", selected_x, text_y, pos_color, text_scale);
                let digit_x = selected_x + self.measure_ui_width("(", text_scale).round();
                self.draw_string_mono_scaled(digits, digit_x, text_y, pos_color, text_scale);
                let suffix_x = digit_x
                    + self
                        .measure_mono_width(
                            &ZERO_SAMPLE[..digits.len().max(2).min(ZERO_SAMPLE.len())],
                            text_scale,
                        )
                        .round();
                self.draw_string_scaled(" выделено)", suffix_x, text_y, pos_color, text_scale);
            }
        }

        if diagnostics_hovered {
            scratch.clear();
            let _ = std::fmt::Write::write_fmt(
                &mut scratch,
                format_args!(
                    "Ляпы: {} ошибок, {} предупреждений",
                    error_count, warning_count
                ),
            );
            let tip_w = self.measure_ui_width(&scratch, text_scale).round() + 16.0 * s;
            let tip_h = 24.0 * s;
            let tip_x = (diag_x - 4.0 * s)
                .min(self.width - tip_w - 6.0 * s)
                .max(6.0 * s);
            let tip_y = (bar_y - tip_h - 6.0 * s).max(6.0 * s);
            self.push_rounded_rect_border(
                tip_x,
                tip_y,
                tip_w,
                tip_h,
                5.0 * s,
                1.0,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.18],
                [0.08, 0.085, 0.115, 0.96],
            );
            self.draw_string_scaled(
                &scratch,
                tip_x + 8.0 * s,
                tip_y + 18.0 * s,
                self.theme.fg,
                text_scale,
            );
        }

        self.scratch_buffer = scratch;
    }

    fn draw_file_tree_dialog_shell(&mut self, x: f32, y: f32, w: f32, h: f32, s: f32) {
        let border = 2.0 * s;
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            10.0 * s,
            border,
            self.theme.sel,
            [0.15, 0.16, 0.20, 1.0],
        );
    }

    fn draw_file_tree_dialog_input(
        &mut self,
        editor: &crate::editor::Editor,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        blink_alpha: f32,
    ) {
        let s = self.scale_factor;
        let text_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
        let pad_x = 8.0 * s;
        let text_y = input_y + 23.0 * s;
        let text_start_x = input_x + pad_x;
        let visible_width = (input_w - pad_x * 2.0).max(0.0);

        self.push_rounded_rect(
            input_x,
            input_y,
            input_w,
            input_h,
            5.0 * s,
            [0.08, 0.09, 0.12, 1.0],
        );

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (input_y + input_h);
            self.gl.scissor(
                input_x as i32,
                scissor_y as i32,
                input_w as i32,
                input_h as i32,
            );

            let text = editor.get_full_text();
            let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                &text,
                editor.cursor,
                visible_width,
                |c| {
                    let char_to_render = if c == '\n' { '↵' } else { c };
                    self.get_ui_glyph(char_to_render)
                        .map(|g| g.advance * text_scale)
                        .unwrap_or(10.0 * text_scale)
                },
            );

            let sel_start = editor
                .selection_anchor
                .unwrap_or(editor.cursor)
                .min(editor.cursor);
            let sel_end = editor
                .selection_anchor
                .unwrap_or(editor.cursor)
                .max(editor.cursor);

            let mut current_x = text_start_x - scroll_x;
            let mut byte_idx = 0usize;
            let mut cursor_draw_x = current_x;
            for c in text.chars() {
                if byte_idx == editor.cursor {
                    cursor_draw_x = current_x;
                }

                let char_to_render = if c == '\n' { '↵' } else { c };
                let adv = self
                    .get_ui_glyph(char_to_render)
                    .map(|g| g.advance * text_scale)
                    .unwrap_or(10.0 * text_scale);

                if byte_idx >= sel_start && byte_idx < sel_end {
                    self.push_rect(
                        current_x,
                        input_y + 7.0 * s,
                        adv,
                        input_h - 14.0 * s,
                        self.theme.sel,
                    );
                }

                if current_x + adv >= input_x && current_x <= input_x + input_w {
                    if let Some(g) = self.get_ui_glyph(char_to_render) {
                        self.push_quad(
                            current_x + g.offset_x * text_scale,
                            text_y - g.offset_y * text_scale,
                            g.width * text_scale,
                            g.height * text_scale,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            self.theme.fg,
                            g.is_emoji,
                        );
                    }
                }

                current_x += adv;
                byte_idx += c.len_utf8();
            }
            if byte_idx == editor.cursor {
                cursor_draw_x = current_x;
            }

            if sel_start == sel_end && blink_alpha > 0.5 {
                self.push_rect(
                    cursor_draw_x,
                    input_y + 7.0 * s,
                    2.0 * s,
                    input_h - 14.0 * s,
                    self.theme.fg,
                );
            }

            self.flush();
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    pub(crate) fn draw_file_tree_overlays(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) -> bool {
        let s = self.scale_factor;
        let mut wants_pointer = false;
        if crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel) {
            ui_registry.mark_overlay_start();
            ui_registry.reset_cursor_state();
        }

        if let Some(menu) = &ide_panel.file_tree_context_menu {
            let row_h = 28.0 * s;
            let pad_x = 12.0 * s;
            let border = 2.0 * s;
            let separator_h = 8.0 * s;
            let mut menu_w = 190.0 * s;
            for action in &menu.entries {
                menu_w = menu_w.max(self.measure_ui_width(action.label(), 0.88) + pad_x * 2.0);
            }
            let menu_h = menu.entries.len() as f32 * row_h
                + file_tree_menu_separator_count(&menu.entries) as f32 * separator_h
                + border * 2.0;
            let x = menu.x.min((self.width - menu_w - 6.0 * s).max(6.0 * s));
            let y = menu.y.min((self.height - menu_h - 6.0 * s).max(6.0 * s));
            let anim_progress = crate::app::file_tree::file_tree_context_menu_anim_progress(
                menu.opened_at,
                std::time::Instant::now(),
            );
            let visible_h = (menu_h * anim_progress).max(border * 2.0);
            self.push_rounded_rect_border(
                x,
                y,
                menu_w,
                visible_h,
                6.0 * s,
                border,
                self.theme.sel,
                [0.09, 0.10, 0.14, 1.0],
            );

            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let sy = (self.height - (y + visible_h)).round() as i32;
                self.gl.scissor(
                    x.round() as i32,
                    sy,
                    menu_w.round() as i32,
                    visible_h.round() as i32,
                );
            }

            let mut row_y = y + border;
            let visible_bottom = y + visible_h;
            for (idx, action) in menu.entries.iter().enumerate() {
                if file_tree_menu_separator_before(&menu.entries, idx) {
                    let line_y = row_y + separator_h / 2.0;
                    self.push_rect(
                        x + border + pad_x,
                        line_y.round(),
                        menu_w - border * 2.0 - pad_x * 2.0,
                        1.0,
                        [1.0, 1.0, 1.0, 0.16],
                    );
                    row_y += separator_h;
                }
                if row_y >= visible_bottom {
                    break;
                }
                let visible_row_h = (visible_bottom - row_y).min(row_h).max(0.0);
                let hovered = ui_registry.register_rect(
                    crate::ui_system::UiId::FileTreeMenuItem(idx),
                    x,
                    row_y,
                    menu_w,
                    visible_row_h,
                    mx,
                    my,
                );
                if hovered {
                    wants_pointer = true;
                    self.push_rect(
                        x + border,
                        row_y,
                        menu_w - border * 2.0,
                        visible_row_h,
                        [1.0, 1.0, 1.0, 0.10],
                    );
                }
                self.draw_string_scaled(
                    action.label(),
                    x + pad_x,
                    row_y + row_h / 2.0 + 5.0 * s,
                    self.theme.fg,
                    0.88,
                );
                row_y += row_h;
            }
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_create_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s).min(self.width - 32.0 * s);
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                dialog.kind.title(),
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let (path_prefix, input_x, input_w) =
                crate::app::file_tree::file_tree_path_input_layout(
                    x,
                    w,
                    s,
                    &dialog.parent_dir,
                    |text| self.measure_ui_width(text, path_scale),
                );
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            self.draw_string_scaled(
                &path_prefix,
                x + side_pad,
                input_y + 23.0 * s,
                [0.55, 0.57, 0.64, 1.0],
                path_scale,
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeCreateInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 112.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeCreateConfirm,
                    "Создать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeCreateCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_rename_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s).min(self.width - 32.0 * s);
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Переименовать",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let (path_prefix, input_x, input_w) = if let Some(parent_dir) = dialog.path.parent() {
                crate::app::file_tree::file_tree_path_input_layout(x, w, s, parent_dir, |text| {
                    self.measure_ui_width(text, path_scale)
                })
            } else {
                (String::new(), x + side_pad, w - side_pad * 2.0)
            };
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            if !path_prefix.is_empty() {
                self.draw_string_scaled(
                    &path_prefix,
                    x + side_pad,
                    input_y + 23.0 * s,
                    [0.55, 0.57, 0.64, 1.0],
                    path_scale,
                );
            }
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeRenameInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 130.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeRenameConfirm,
                    "Переименовать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeRenameCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_move_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Подтвердить перемещение",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_move_dialog_message(
                &dialog.sources,
                &dialog.target_dir,
            );
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeMoveConfirm,
                    "Переместить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeMoveCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        if let Some(dialog) = &ide_panel.file_tree_delete_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить в корзину",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_delete_dialog_message(&dialog.paths);
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            for (id, label, bx) in [
                (
                    crate::ui_system::UiId::FileTreeDeleteConfirm,
                    "В корзину",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeDeleteCancel,
                    "Отмена",
                    cancel_x,
                ),
            ] {
                let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
                if hovered {
                    wants_pointer = true;
                }
                let bg = if hovered {
                    [0.30, 0.32, 0.38, 1.0]
                } else {
                    [0.22, 0.23, 0.28, 1.0]
                };
                self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
                let tw = self.measure_ui_width(label, 0.86);
                self.draw_string_scaled(
                    label,
                    bx + (btn_w - tw) / 2.0,
                    btn_y + 21.0 * s,
                    self.theme.fg,
                    0.86,
                );
            }
        }

        wants_pointer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_label_prefix_len_reserves_ellipsis_and_keeps_utf8_boundary() {
        assert_eq!(clipped_label_prefix_len("abcdef", 38.0, 8.0, |_| 10.0), 3);
        assert_eq!(
            clipped_label_prefix_len("абвг", 18.0, 8.0, |_| 5.0),
            "аб".len()
        );
        assert_eq!(clipped_label_prefix_len("abc", 4.0, 8.0, |_| 3.0), 0);
    }

    #[test]
    fn centered_dialog_button_positions_keep_pair_centered() {
        let (ok_x, cancel_x) = centered_dialog_button_positions(100.0, 420.0, 112.0, 10.0);

        assert_eq!(ok_x, 193.0);
        assert_eq!(cancel_x, 315.0);
        assert_eq!((ok_x + cancel_x + 112.0) / 2.0, 310.0);
    }

    #[test]
    fn file_tree_context_menu_groups_insert_logical_separators() {
        use crate::app::file_tree::FileTreeMenuAction;

        let entries = [
            FileTreeMenuAction::CreateFile,
            FileTreeMenuAction::CreateDirectory,
            FileTreeMenuAction::Paste,
            FileTreeMenuAction::Delete,
            FileTreeMenuAction::Rename,
            FileTreeMenuAction::OpenContainedFolder,
            FileTreeMenuAction::CopyRelativePath,
        ];

        assert!(!file_tree_menu_separator_before(&entries, 0));
        assert!(!file_tree_menu_separator_before(&entries, 2));
        assert!(file_tree_menu_separator_before(&entries, 3));
        assert!(file_tree_menu_separator_before(&entries, 5));
        assert_eq!(file_tree_menu_separator_count(&entries), 2);
    }
}
