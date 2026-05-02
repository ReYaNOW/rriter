use crate::renderer::Renderer;
use glow::HasContext;

fn lsp_action_label<'a>(
    item: &'a crate::app::LspActionItem,
    scratch: &'a mut String,
) -> std::borrow::Cow<'a, str> {
    scratch.clear();
    match item {
        crate::app::LspActionItem::CodeAction(action) => {
            if let Some(c) = &action.code {
                let _ = std::fmt::Write::write_fmt(
                    scratch,
                    format_args!("Исправить ({}): {}", c, action.title),
                );
            } else {
                let _ = std::fmt::Write::write_fmt(
                    scratch,
                    format_args!("Исправить: {}", action.title),
                );
            }
            std::borrow::Cow::Borrowed(scratch.as_str())
        }
        crate::app::LspActionItem::AddNoqa { codes } => {
            if codes.is_empty() {
                std::borrow::Cow::Borrowed("Игнорировать ошибку (# noqa)")
            } else {
                scratch.push_str("Игнорировать ");
                for (i, code) in codes.iter().enumerate() {
                    if i > 0 {
                        scratch.push_str(", ");
                    }
                    scratch.push_str(code);
                }
                scratch.push_str(" (# noqa)");
                std::borrow::Cow::Borrowed(scratch.as_str())
            }
        }
        crate::app::LspActionItem::AddNoqaAll => {
            std::borrow::Cow::Borrowed("Игнорировать всё в файле (# noqa)")
        }
        crate::app::LspActionItem::FixAll => {
            std::borrow::Cow::Borrowed("Исправить все доступные ошибки")
        }
        crate::app::LspActionItem::OrganizeImports => {
            std::borrow::Cow::Borrowed("Упорядочить импорты")
        }
        crate::app::LspActionItem::CompleteImports => {
            std::borrow::Cow::Borrowed("Подсказки импортов ty")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lsp_action_label;

    #[test]
    fn lsp_action_label_formats_code_actions_and_noqa_codes() {
        let mut scratch = String::new();
        let action = crate::app::LspActionItem::CodeAction(crate::lsp::CodeAction {
            title: "remove unused import".to_string(),
            kind: Some("quickfix".to_string()),
            edit: None,
            code: Some("F401".to_string()),
        });

        assert_eq!(
            lsp_action_label(&action, &mut scratch),
            "Исправить (F401): remove unused import"
        );

        let noqa = crate::app::LspActionItem::AddNoqa {
            codes: vec!["F401".to_string(), "F821".to_string()],
        };
        assert_eq!(
            lsp_action_label(&noqa, &mut scratch),
            "Игнорировать F401, F821 (# noqa)"
        );
    }

    #[test]
    fn lsp_action_label_formats_static_actions() {
        let mut scratch = String::new();
        assert_eq!(
            lsp_action_label(
                &crate::app::LspActionItem::AddNoqa { codes: Vec::new() },
                &mut scratch
            ),
            "Игнорировать ошибку (# noqa)"
        );
        assert_eq!(
            lsp_action_label(&crate::app::LspActionItem::AddNoqaAll, &mut scratch),
            "Игнорировать всё в файле (# noqa)"
        );
        assert_eq!(
            lsp_action_label(&crate::app::LspActionItem::FixAll, &mut scratch),
            "Исправить все доступные ошибки"
        );
        assert_eq!(
            lsp_action_label(&crate::app::LspActionItem::OrganizeImports, &mut scratch),
            "Упорядочить импорты"
        );
        assert_eq!(
            lsp_action_label(&crate::app::LspActionItem::CompleteImports, &mut scratch),
            "Подсказки импортов ty"
        );
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    /// Рисует содержимое панели LSP серверов (левая панель)
    pub fn draw_lsp_servers_panel(
        &mut self,
        content_x: f32,
        content_y: f32,
        content_w: f32,
        content_h: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        fix_all_active: bool,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let scroll_y = ide_panel.lsp_scroll_y.current;
        let servers = &ide_panel.lsp_servers;
        let expanded_logs = &ide_panel.lsp_logs_expanded;
        let lsp_log_editors = &ide_panel.lsp_log_editors;
        let lsp_logs_focused = &ide_panel.lsp_logs_focused;
        let lsp_log_filter_text = ide_panel.lsp_log_filter_editor.get_full_text();

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (content_y + content_h)).round() as i32;
            self.gl.scissor(
                content_x.round() as i32,
                sy,
                content_w.round() as i32,
                content_h.round() as i32,
            );
        }

        let pad_x = 12.0 * s;
        let text_scale = 0.92;
        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        if servers.is_empty() {
            let hint = "Нет активных серверов";
            let tw = self.measure_ui_width(hint, text_scale);
            self.draw_string_scaled(
                hint,
                (content_x + (content_w - tw) / 2.0).round(),
                (content_y + 32.0 * s).round(),
                [0.45, 0.45, 0.45, 1.0],
                text_scale,
            );
        }

        let get_inner_size =
            |info: &crate::lsp::LspServerInfo, renderer: &mut Self| -> (f32, f32) {
                if let Some(log_ed) = lsp_log_editors.get(info.name) {
                    let mut lines = 0;
                    let mut max_w = 0.0f32;
                    let mut phys_line = 0;
                    let (first, second) = log_ed.text_parts();
                    while phys_line < log_ed.line_offsets.len() {
                        let start = log_ed.line_offsets[phys_line];
                        let end = if phys_line + 1 < log_ed.line_offsets.len() {
                            log_ed.line_offsets[phys_line + 1].saturating_sub(1)
                        } else {
                            log_ed.len()
                        };
                        let w = renderer.measure_width(first, second, start, end) * 0.7;
                        if w > max_w {
                            max_w = w;
                        }
                        lines += 1;
                        if log_ed.folded_lines.contains(&phys_line) {
                            if let Some(&fold_end) = log_ed.foldable_lines.get(&phys_line) {
                                phys_line = fold_end;
                            }
                        }
                        phys_line += 1;
                    }
                    (lines as f32 * 16.0 * s, max_w)
                } else {
                    (0.0, 0.0)
                }
            };

        let mut total_h = 8.0 * s;
        let mut log_sizes = Vec::with_capacity(servers.len());

        for info in servers.iter() {
            let is_expanded = expanded_logs.contains(info.name);
            let mut layout_logs_h = 0.0;
            let mut inner_h = 0.0;
            let mut inner_w = 0.0;
            if is_expanded {
                (inner_h, inner_w) = get_inner_size(info, self);
                layout_logs_h =
                    crate::app::lsp_actions::lsp_server_logs_h_for_content(inner_h, content_h, s);
            }
            log_sizes.push((layout_logs_h, inner_h, inner_w));
            total_h += 136.0 * s + layout_logs_h + 16.0 * s;
        }

        let mut current_y = content_y + 8.0 * s - scroll_y;
        for (server_idx, info) in servers.iter().enumerate() {
            let is_expanded = expanded_logs.contains(info.name);
            let (layout_logs_h, inner_total_h, inner_max_w) = log_sizes[server_idx];
            let logs_h = if is_expanded {
                crate::app::lsp_actions::lsp_server_logs_h_for_row(
                    inner_total_h,
                    content_y,
                    content_h,
                    current_y,
                    s,
                )
            } else {
                0.0
            };
            let base_h = 136.0 * s;
            let row_h = base_h + logs_h;
            let layout_row_h = base_h + layout_logs_h;

            if current_y + row_h > content_y && current_y < content_y + content_h {
                let card_x = content_x + 12.0 * s;
                let card_w = content_w - 24.0 * s;

                // Тень и бордер карточки
                self.push_rounded_rect(
                    card_x - 1.0,
                    current_y - 1.0,
                    card_w + 2.0,
                    row_h + 2.0,
                    7.0 * s,
                    [0.35, 0.30, 0.45, 0.4],
                );
                self.push_rounded_rect(
                    card_x,
                    current_y,
                    card_w,
                    row_h,
                    6.0 * s,
                    [0.18, 0.19, 0.24, 1.0],
                );

                let dot_r = 5.0 * s;
                let dot_x = card_x + pad_x + dot_r;
                let dot_y = current_y + 16.0 * s;

                let (dot_color, status_text) = match info.status {
                    crate::lsp::LspServerStatus::Running => ([0.28, 0.85, 0.45, 1.0], "Работает"),
                    crate::lsp::LspServerStatus::Starting => ([0.85, 0.75, 0.25, 1.0], "Запуск..."),
                    crate::lsp::LspServerStatus::Crashed => ([0.90, 0.30, 0.30, 1.0], "Упал"),
                    crate::lsp::LspServerStatus::Disabled => ([0.45, 0.45, 0.45, 1.0], "Отключён"),
                };
                self.push_rounded_rect(
                    dot_x - dot_r,
                    dot_y - dot_r,
                    dot_r * 2.0,
                    dot_r * 2.0,
                    dot_r,
                    dot_color,
                );

                self.draw_string_scaled(
                    info.name,
                    (card_x + pad_x + dot_r * 2.0 + 8.0 * s).round(),
                    (dot_y + dot_r).round(),
                    self.theme.fg,
                    text_scale,
                );

                let status_color = dot_color;
                self.draw_string_scaled(
                    status_text,
                    (card_x + pad_x + dot_r * 2.0 + 8.0 * s).round(),
                    (dot_y + dot_r + 18.0 * s).round(),
                    status_color,
                    0.78,
                );

                let btn_h = 24.0 * s;
                let btn_y1 = current_y + 56.0 * s;
                let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                let btn_pad = 10.0 * s;

                let label_restart = "Перезапуск";
                let label_toggle = if matches!(info.status, crate::lsp::LspServerStatus::Disabled) {
                    "Включить"
                } else {
                    "Отключить"
                };
                let label_stop = "Остановить";
                let label_logs = if is_expanded {
                    "Скрыть логи"
                } else {
                    "Логи"
                };
                let label_fix_all = "Fix All";
                let label_clear_logs = "Очистить";

                let bw_restart = self.measure_ui_width(label_restart, 0.8) + btn_pad * 2.0;
                let bw_toggle = self.measure_ui_width(label_toggle, 0.8) + btn_pad * 2.0;
                let bw_stop = self.measure_ui_width(label_stop, 0.8) + btn_pad * 2.0;
                let bw_logs = self.measure_ui_width(label_logs, 0.8) + btn_pad * 2.0;
                let bw_fix_all = self.measure_ui_width(label_fix_all, 0.8) + btn_pad * 2.0;
                let bw_clear_logs =
                    self.measure_ui_width(label_clear_logs, 0.8) + btn_pad * 2.0;

                let btn_x_restart = card_x + pad_x;
                let btn_x_toggle = btn_x_restart + bw_restart + 6.0 * s;
                let btn_x_stop = btn_x_toggle + bw_toggle + 6.0 * s;

                let btn_x_fix_all = card_x + pad_x;
                let btn_x_logs = btn_x_fix_all + bw_fix_all + 6.0 * s;
                let btn_x_clear_logs = btn_x_logs + bw_logs + 6.0 * s;

                let hover_restart = ui_registry.register_rect(
                    crate::ui_system::UiId::LspServerRestart(server_idx),
                    btn_x_restart,
                    btn_y1,
                    bw_restart,
                    btn_h,
                    mx,
                    my,
                );
                let hover_toggle = ui_registry.register_rect(
                    crate::ui_system::UiId::LspServerToggle(server_idx),
                    btn_x_toggle,
                    btn_y1,
                    bw_toggle,
                    btn_h,
                    mx,
                    my,
                );
                let is_stopped = matches!(
                    info.status,
                    crate::lsp::LspServerStatus::Disabled | crate::lsp::LspServerStatus::Crashed
                );
                let hover_stop = if !is_stopped {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::LspServerStop(server_idx),
                        btn_x_stop,
                        btn_y1,
                        bw_stop,
                        btn_h,
                        mx,
                        my,
                    )
                } else {
                    false
                };

                let hover_logs = ui_registry.register_rect(
                    crate::ui_system::UiId::LspServerLogs(server_idx),
                    btn_x_logs,
                    btn_y2,
                    bw_logs,
                    btn_h,
                    mx,
                    my,
                );
                let clear_logs_enabled = is_expanded && !info.logs.is_empty();
                let clear_logs_fits = btn_x_clear_logs + bw_clear_logs <= card_x + card_w - pad_x;
                let hover_clear_logs = if clear_logs_enabled && clear_logs_fits {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::LspServerClearLogs(server_idx),
                        btn_x_clear_logs,
                        btn_y2,
                        bw_clear_logs,
                        btn_h,
                        mx,
                        my,
                    )
                } else {
                    false
                };
                let fix_enabled = !is_stopped && fix_all_active;
                let hover_fix_all = if fix_enabled {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::LspServerFixAll(server_idx),
                        btn_x_fix_all,
                        btn_y2,
                        bw_fix_all,
                        btn_h,
                        mx,
                        my,
                    )
                } else {
                    false
                };

                let btn_bg_restart = if hover_restart {
                    [0.35, 0.35, 0.40, 1.0]
                } else {
                    [0.26, 0.26, 0.32, 1.0]
                };
                let btn_bg_toggle = if hover_toggle {
                    [0.35, 0.35, 0.40, 1.0]
                } else {
                    [0.26, 0.26, 0.32, 1.0]
                };
                let btn_bg_logs = if hover_logs {
                    [0.35, 0.35, 0.40, 1.0]
                } else {
                    [0.26, 0.26, 0.32, 1.0]
                };
                let btn_bg_clear_logs = if !clear_logs_enabled || !clear_logs_fits {
                    [0.18, 0.18, 0.22, 0.6]
                } else if hover_clear_logs {
                    [0.36, 0.30, 0.20, 1.0]
                } else {
                    [0.28, 0.22, 0.16, 1.0]
                };
                let btn_bg_stop = if is_stopped {
                    [0.20, 0.20, 0.25, 0.6]
                } else if hover_stop {
                    [0.45, 0.22, 0.22, 1.0]
                } else {
                    [0.32, 0.15, 0.15, 1.0]
                };
                let btn_bg_fix_all = if !fix_enabled {
                    [0.18, 0.18, 0.22, 0.6]
                } else if hover_fix_all {
                    [0.22, 0.42, 0.28, 1.0]
                } else {
                    [0.15, 0.30, 0.20, 1.0]
                };

                let text_color_stop = if is_stopped {
                    [0.55, 0.55, 0.60, 1.0]
                } else {
                    [0.95, 0.55, 0.55, 1.0]
                };
                let text_color_fix_all = if !fix_enabled {
                    [0.40, 0.40, 0.44, 1.0]
                } else {
                    [0.55, 0.95, 0.65, 1.0]
                };
                let text_color_clear_logs = if !clear_logs_enabled || !clear_logs_fits {
                    [0.40, 0.40, 0.44, 1.0]
                } else {
                    [0.95, 0.78, 0.55, 1.0]
                };

                let text_y1 = (btn_y1 + btn_h / 2.0 + 4.0 * s).round();
                let text_y2 = (btn_y2 + btn_h / 2.0 + 4.0 * s).round();

                self.push_rounded_rect(
                    btn_x_restart,
                    btn_y1,
                    bw_restart,
                    btn_h,
                    3.0 * s,
                    btn_bg_restart,
                );
                self.draw_string_scaled(
                    label_restart,
                    (btn_x_restart + btn_pad).round(),
                    text_y1,
                    self.theme.fg,
                    0.8,
                );

                self.push_rounded_rect(
                    btn_x_toggle,
                    btn_y1,
                    bw_toggle,
                    btn_h,
                    3.0 * s,
                    btn_bg_toggle,
                );
                self.draw_string_scaled(
                    label_toggle,
                    (btn_x_toggle + btn_pad).round(),
                    text_y1,
                    self.theme.fg,
                    0.8,
                );

                self.push_rounded_rect(btn_x_stop, btn_y1, bw_stop, btn_h, 3.0 * s, btn_bg_stop);
                self.draw_string_scaled(
                    label_stop,
                    (btn_x_stop + btn_pad).round(),
                    text_y1,
                    text_color_stop,
                    0.8,
                );

                self.push_rounded_rect(
                    btn_x_fix_all,
                    btn_y2,
                    bw_fix_all,
                    btn_h,
                    3.0 * s,
                    btn_bg_fix_all,
                );
                self.draw_string_scaled(
                    label_fix_all,
                    (btn_x_fix_all + btn_pad).round(),
                    text_y2,
                    text_color_fix_all,
                    0.8,
                );

                self.push_rounded_rect(btn_x_logs, btn_y2, bw_logs, btn_h, 3.0 * s, btn_bg_logs);
                self.draw_string_scaled(
                    label_logs,
                    (btn_x_logs + btn_pad).round(),
                    text_y2,
                    [0.8, 0.85, 1.0, 1.0],
                    0.8,
                );
                if is_expanded {
                    self.push_rounded_rect(
                        btn_x_clear_logs,
                        btn_y2,
                        bw_clear_logs,
                        btn_h,
                        3.0 * s,
                        btn_bg_clear_logs,
                    );
                    self.draw_string_scaled(
                        label_clear_logs,
                        (btn_x_clear_logs + btn_pad).round(),
                        text_y2,
                        text_color_clear_logs,
                        0.8,
                    );
                }

                if is_expanded && logs_h > 0.0 {
                    let log_bg_x = card_x + pad_x;
                    let filter_y = btn_y2 + btn_h + 10.0 * s;
                    let filter_h = 30.0 * s;
                    let filter_gap = 4.0 * s;
                    let log_bg_y = filter_y + filter_h + filter_gap;
                    let log_bg_w = card_w - pad_x * 2.0;
                    let log_bg_h = logs_h - filter_h - filter_gap - 18.0 * s;

                    let chip_h = 22.0 * s;
                    let chip_y = filter_y + 4.0 * s;
                    let mut chip_x = log_bg_x;
                    let clear_w = 24.0 * s;
                    let chip_pad = 8.0 * s;
                    let label_case = if ide_panel.lsp_log_filter_case_sensitive {
                        "Aa"
                    } else {
                        "aa"
                    };
                    let label_send = "SEND";
                    let label_recv = "RECV";
                    let label_other = "ERR";
                    let case_w = self.measure_ui_width(label_case, 0.72) + chip_pad * 2.0;
                    let send_w = self.measure_ui_width(label_send, 0.72) + chip_pad * 2.0;
                    let recv_w = self.measure_ui_width(label_recv, 0.72) + chip_pad * 2.0;
                    let other_w = self.measure_ui_width(label_other, 0.72) + chip_pad * 2.0;
                    let chips_w =
                        clear_w + case_w + send_w + recv_w + other_w + 5.0 * 6.0 * s;
                    let input_w = (log_bg_w - chips_w).max(70.0 * s);
                    let input_hover = ui_registry.register_text_input(
                        crate::ui_system::UiId::LspLogsFilterInput,
                        chip_x,
                        filter_y,
                        input_w,
                        filter_h,
                        mx,
                        my,
                    );
                    let input_border = if ide_panel.lsp_log_filter_focused || input_hover {
                        [0.44, 0.28, 0.75, 0.9]
                    } else {
                        [0.18, 0.18, 0.22, 1.0]
                    };
                    self.push_rounded_rect(
                        chip_x - 1.0,
                        filter_y - 1.0,
                        input_w + 2.0,
                        filter_h + 2.0,
                        4.0 * s,
                        input_border,
                    );
                    self.push_rounded_rect(
                        chip_x,
                        filter_y,
                        input_w,
                        filter_h,
                        4.0 * s,
                        [0.10, 0.10, 0.13, 1.0],
                    );
                    let mut clipped_filter = String::new();
                    let filter_draw = if lsp_log_filter_text.is_empty() {
                        "Фильтр"
                    } else {
                        let max_text_w = (input_w - 16.0 * s).max(0.0);
                        let mut used_w = 0.0;
                        for c in lsp_log_filter_text.chars() {
                            let adv = self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0)
                                * 0.78;
                            if used_w + adv > max_text_w {
                                clipped_filter.push('…');
                                break;
                            }
                            clipped_filter.push(c);
                            used_w += adv;
                        }
                        clipped_filter.as_str()
                    };
                    let filter_color = if lsp_log_filter_text.is_empty() {
                        [0.45, 0.45, 0.50, 1.0]
                    } else {
                        self.theme.fg
                    };
                    self.draw_string_scaled(
                        filter_draw,
                        chip_x + 8.0 * s,
                        filter_y + filter_h / 2.0 + 5.0 * s,
                        filter_color,
                        0.78,
                    );
                    chip_x += input_w + 5.0 * s;

                    let draw_chip = |renderer: &mut Self,
                                     ui_registry: &mut crate::ui_system::UiRegistry,
                                     id: crate::ui_system::UiId,
                                         label: &str,
                                         active: bool,
                                         x: f32,
                                         w: f32| {
                        let hovered = ui_registry.register_rect(id, x, chip_y, w, chip_h, mx, my);
                        let bg = if active {
                            if hovered {
                                [0.28, 0.32, 0.42, 1.0]
                            } else {
                                [0.20, 0.24, 0.34, 1.0]
                            }
                        } else if hovered {
                            [0.24, 0.24, 0.28, 1.0]
                        } else {
                            [0.14, 0.14, 0.17, 1.0]
                        };
                        renderer.push_rounded_rect(x, chip_y, w, chip_h, 3.0 * s, bg);
                        renderer.draw_string_scaled(
                            label,
                            x + chip_pad,
                            chip_y + chip_h / 2.0 + 4.0 * s,
                            if active {
                                [0.72, 0.86, 1.0, 1.0]
                            } else {
                                [0.52, 0.52, 0.57, 1.0]
                            },
                            0.72,
                        );
                    };

                    draw_chip(
                        self,
                        ui_registry,
                        crate::ui_system::UiId::LspLogsFilterClear,
                        "×",
                        !lsp_log_filter_text.is_empty(),
                        chip_x,
                        clear_w,
                    );
                    chip_x += clear_w + 5.0 * s;
                    draw_chip(
                        self,
                        ui_registry,
                        crate::ui_system::UiId::LspLogsFilterCase,
                        label_case,
                        ide_panel.lsp_log_filter_case_sensitive,
                        chip_x,
                        case_w,
                    );
                    chip_x += case_w + 5.0 * s;
                    draw_chip(
                        self,
                        ui_registry,
                        crate::ui_system::UiId::LspLogsFilterSend,
                        label_send,
                        ide_panel.lsp_log_filter_show_send,
                        chip_x,
                        send_w,
                    );
                    chip_x += send_w + 5.0 * s;
                    draw_chip(
                        self,
                        ui_registry,
                        crate::ui_system::UiId::LspLogsFilterRecv,
                        label_recv,
                        ide_panel.lsp_log_filter_show_recv,
                        chip_x,
                        recv_w,
                    );
                    chip_x += recv_w + 5.0 * s;
                    draw_chip(
                        self,
                        ui_registry,
                        crate::ui_system::UiId::LspLogsFilterOther,
                        label_other,
                        ide_panel.lsp_log_filter_show_other,
                        chip_x,
                        other_w,
                    );

                    let border_color = if lsp_logs_focused.as_deref() == Some(info.name) {
                        [0.44, 0.28, 0.75, 0.8]
                    } else {
                        [0.1, 0.1, 0.12, 1.0]
                    };

                    self.push_rounded_rect(
                        log_bg_x - 1.0,
                        log_bg_y - 1.0,
                        log_bg_w + 2.0,
                        log_bg_h + 2.0,
                        4.0 * s,
                        border_color,
                    );
                    self.push_rounded_rect(
                        log_bg_x,
                        log_bg_y,
                        log_bg_w,
                        log_bg_h,
                        4.0 * s,
                        [0.08, 0.08, 0.10, 1.0],
                    );

                    self.flush();
                    let inter_y1 = log_bg_y.max(content_y);
                    let inter_y2 = (log_bg_y + log_bg_h).min(content_y + content_h);
                    let inter_h = (inter_y2 - inter_y1).max(0.0);

                    ui_registry.register_blocker(
                        crate::ui_system::UiId::LspLogArea(server_idx),
                        log_bg_x,
                        log_bg_y,
                        log_bg_w,
                        log_bg_h,
                        mx,
                        my,
                    );

                    if inter_h > 0.0 {
                        unsafe {
                            self.gl.enable(glow::SCISSOR_TEST);
                            let sy = (self.height - inter_y2).round() as i32;
                            self.gl.scissor(
                                log_bg_x.round() as i32,
                                sy,
                                log_bg_w.round() as i32,
                                inter_h.round() as i32,
                            );
                        }

                        let line_h = 16.0 * s;
                        let inner_scroll_y = ide_panel
                            .lsp_logs_scroll_y
                            .get(info.name)
                            .map(|ss| ss.current)
                            .unwrap_or(0.0);
                        let inner_scroll_x = ide_panel
                            .lsp_logs_scroll_x
                            .get(info.name)
                            .map(|ss| ss.current)
                            .unwrap_or(0.0);
                        let _first_visible_line = (inner_scroll_y / line_h).floor() as usize;

                        let mut sel_lo = 0;
                        let mut sel_hi = 0;
                        if let Some(log_ed) = lsp_log_editors.get(info.name) {
                            let (lo, hi) = match log_ed.selection_anchor {
                                Some(anchor) => {
                                    (anchor.min(log_ed.cursor), anchor.max(log_ed.cursor))
                                }
                                None => (log_ed.cursor, log_ed.cursor),
                            };
                            sel_lo = lo;
                            sel_hi = hi;
                        }

                        let mut text_y = log_bg_y + 16.0 * s - inner_scroll_y;

                        if let Some(log_ed) = lsp_log_editors.get(info.name) {
                            let mut phys_line = 0;
                            let (first, second) = log_ed.text_parts();
                            let first_len = first.len();
                            let len = first_len + second.len();

                            let mut entry_idx = 0;
                            let mut entry_start = 0;

                            while phys_line < log_ed.line_offsets.len() {
                                let is_folded = log_ed.folded_lines.contains(&phys_line)
                                    && log_ed.foldable_lines.contains_key(&phys_line);
                                let fold_end = if is_folded {
                                    log_ed.foldable_lines.get(&phys_line).copied()
                                } else {
                                    None
                                };

                                if text_y > inter_y2 + line_h {
                                    break;
                                }

                                if text_y + line_h > inter_y1 {
                                    let start_byte = log_ed.line_offsets[phys_line];
                                    let end_byte = if phys_line + 1 < log_ed.line_offsets.len() {
                                        log_ed.line_offsets[phys_line + 1].saturating_sub(1)
                                    } else {
                                        len
                                    };

                                    if sel_lo < sel_hi && sel_lo <= end_byte && sel_hi >= start_byte
                                    {
                                        let in_s = sel_lo.saturating_sub(start_byte);
                                        let in_e = sel_hi
                                            .saturating_sub(start_byte)
                                            .min(end_byte - start_byte);
                                        let x1 = log_bg_x
                                            + 20.0 * s
                                            + self.measure_width(
                                                first,
                                                second,
                                                start_byte,
                                                start_byte + in_s,
                                            ) * 0.7
                                            - inner_scroll_x;
                                        let x2 = log_bg_x
                                            + 20.0 * s
                                            + self.measure_width(
                                                first,
                                                second,
                                                start_byte,
                                                start_byte + in_e,
                                            ) * 0.7
                                            - inner_scroll_x;
                                        let ry = text_y - 14.0 * s;
                                        let x1c = x1.max(log_bg_x);
                                        let x2c = x2.min(log_bg_x + log_bg_w);
                                        if x2c > x1c {
                                            self.push_rounded_rect(
                                                x1c,
                                                ry,
                                                x2c - x1c,
                                                line_h,
                                                0.0,
                                                [0.40, 0.28, 0.72, 0.45],
                                            );
                                        }
                                    }

                                    let mut current_x = log_bg_x + 20.0 * s - inner_scroll_x;
                                    let mut current_chunk_offset = start_byte;

                                    while current_chunk_offset < end_byte {
                                        let chunk = if current_chunk_offset < first_len {
                                            let s_end = end_byte.min(first_len);
                                            &first[current_chunk_offset..s_end]
                                        } else {
                                            let s_start = current_chunk_offset - first_len;
                                            let s_end = end_byte - first_len;
                                            &second[s_start..s_end]
                                        };

                                        for c in chunk.chars() {
                                            let is_newline = c == '\n';
                                            let is_hidden = c == '\u{FE0F}' || c == '\u{200D}';
                                            let adv = if is_newline || is_hidden {
                                                0.0
                                            } else {
                                                self.char_advance(c) * 0.7
                                            };

                                            if current_x + adv > log_bg_x
                                                && current_x < log_bg_x + log_bg_w
                                            {
                                                if !is_newline
                                                    && !is_hidden
                                                    && c != ' '
                                                    && c != '\t'
                                                {
                                                    while entry_idx < info.logs.len()
                                                        && current_chunk_offset
                                                            >= entry_start
                                                                + info.logs[entry_idx].text.len()
                                                                + 1
                                                    {
                                                        entry_start +=
                                                            info.logs[entry_idx].text.len() + 1;
                                                        entry_idx += 1;
                                                    }

                                                    let mut color = [0.875, 0.882, 0.902, 1.0];
                                                    if entry_idx < info.logs.len() {
                                                        let rel_offset =
                                                            current_chunk_offset - entry_start;
                                                        for span in &info.logs[entry_idx].spans {
                                                            if rel_offset >= span.start
                                                                && rel_offset < span.end
                                                            {
                                                                color = span.color;
                                                                break;
                                                            }
                                                        }
                                                    }

                                                    if let Some(g) = self.get_glyph(c) {
                                                        let q_x =
                                                            (current_x + g.offset_x * 0.7).round();
                                                        let q_y =
                                                            (text_y - g.offset_y * 0.7).round();
                                                        let q_w = (current_x
                                                            + g.offset_x * 0.7
                                                            + g.width * 0.7)
                                                            .round()
                                                            - q_x;
                                                        let q_h = (text_y - g.offset_y * 0.7
                                                            + g.height * 0.7)
                                                            .round()
                                                            - q_y;
                                                        self.push_quad(
                                                            q_x, q_y, q_w, q_h, g.u, g.v, g.uw,
                                                            g.vh, color, g.is_emoji,
                                                        );
                                                    }
                                                }
                                            }
                                            current_x += adv;
                                            current_chunk_offset += c.len_utf8();
                                        }
                                    }

                                    let is_foldable =
                                        log_ed.foldable_lines.contains_key(&phys_line);
                                    if is_foldable {
                                        let arrow_str = if is_folded { "▶" } else { "▼" };
                                        let icon_x = log_bg_x + 4.0 * s - inner_scroll_x;
                                        let is_hovered = ui_registry.register_rect(
                                            crate::ui_system::UiId::LspLogFoldToggle(
                                                server_idx, phys_line,
                                            ),
                                            icon_x - 4.0 * s,
                                            text_y - 14.0 * s,
                                            16.0 * s,
                                            line_h,
                                            mx,
                                            my,
                                        );
                                        let color = if is_hovered {
                                            [0.8, 0.8, 0.9, 1.0]
                                        } else {
                                            [0.5, 0.5, 0.55, 1.0]
                                        };
                                        self.draw_string_scaled(
                                            arrow_str,
                                            icon_x,
                                            text_y - 2.0 * s,
                                            color,
                                            0.8,
                                        );
                                    }

                                    if is_folded {
                                        let dots_str = "...";
                                        let dots_adv =
                                            self.measure_ui_width(dots_str, 0.7) + 8.0 * s;
                                        let box_x = current_x + 4.0 * s;

                                        ui_registry.register_rect(
                                            crate::ui_system::UiId::LspLogFoldToggle(
                                                server_idx, phys_line,
                                            ),
                                            box_x,
                                            text_y - 12.0 * s,
                                            dots_adv,
                                            line_h - 2.0 * s,
                                            mx,
                                            my,
                                        );

                                        self.push_rounded_rect(
                                            box_x,
                                            text_y - 12.0 * s,
                                            dots_adv,
                                            line_h - 2.0 * s,
                                            3.0 * s,
                                            [
                                                self.theme.bg[0] + 0.08,
                                                self.theme.bg[1] + 0.08,
                                                self.theme.bg[2] + 0.12,
                                                1.0,
                                            ],
                                        );
                                        self.draw_string_scaled(
                                            dots_str,
                                            box_x + 4.0 * s,
                                            text_y,
                                            self.theme.fg,
                                            0.7,
                                        );
                                    }
                                }

                                if is_folded {
                                    phys_line = fold_end.unwrap();
                                }
                                phys_line += 1;
                                text_y += line_h;
                            }
                        }

                        self.flush();

                        if inner_total_h > log_bg_h {
                            let max_y = (inner_total_h - log_bg_h).max(0.0);
                            let ratio = (inner_scroll_y / max_y).clamp(0.0, 1.0);
                            let track_h = log_bg_h - 14.0 * s;
                            let thumb_h = (log_bg_h / inner_total_h * track_h).max(20.0 * s);
                            let thumb_y = log_bg_y + 7.0 * s + ratio * (track_h - thumb_h);
                            self.push_rounded_rect(
                                log_bg_x + log_bg_w - 8.0 * s,
                                thumb_y,
                                4.0 * s,
                                thumb_h,
                                2.0 * s,
                                [1.0, 1.0, 1.0, 0.22],
                            );
                            ui_registry.register_rect(
                                crate::ui_system::UiId::LspLogScrollY(server_idx),
                                log_bg_x + log_bg_w - 14.0 * s,
                                log_bg_y,
                                14.0 * s,
                                log_bg_h,
                                mx,
                                my,
                            );
                        }

                        if inner_max_w + 20.0 * s > log_bg_w {
                            let max_x = (inner_max_w + 20.0 * s - log_bg_w).max(0.0);
                            let ratio = (inner_scroll_x / max_x).clamp(0.0, 1.0);
                            let track_w = log_bg_w - 14.0 * s;
                            let thumb_w =
                                (log_bg_w / (inner_max_w + 20.0 * s) * track_w).max(20.0 * s);
                            let thumb_x = log_bg_x + 7.0 * s + ratio * (track_w - thumb_w);
                            self.push_rounded_rect(
                                thumb_x,
                                log_bg_y + log_bg_h - 8.0 * s,
                                thumb_w,
                                4.0 * s,
                                2.0 * s,
                                [1.0, 1.0, 1.0, 0.22],
                            );
                            ui_registry.register_rect(
                                crate::ui_system::UiId::LspLogScrollX(server_idx),
                                log_bg_x,
                                log_bg_y + log_bg_h - 14.0 * s,
                                log_bg_w,
                                14.0 * s,
                                mx,
                                my,
                            );
                        }

                        unsafe {
                            let sy = (self.height - (content_y + content_h)).round() as i32;
                            self.gl.scissor(
                                content_x.round() as i32,
                                sy,
                                content_w.round() as i32,
                                content_h.round() as i32,
                            );
                        }
                    }
                }
            }
            current_y += layout_row_h + 16.0 * s;
        }

        let max_scroll_y = (total_h - content_h).max(0.0);
        if max_scroll_y > 0.0 {
            let ratio = (scroll_y / max_scroll_y).clamp(0.0, 1.0);
            let track_h = content_h - 10.0 * s;
            let thumb_h = (content_h / total_h * track_h).max(40.0 * s);
            let thumb_y = content_y + 5.0 * s + ratio * (track_h - thumb_h);
            self.push_rounded_rect(
                content_x + content_w - 12.0 * s,
                thumb_y,
                10.0 * s,
                thumb_h,
                5.0 * s,
                [1.0, 1.0, 1.0, 0.22],
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::LspScrollY,
                content_x + content_w - 12.0 * s,
                content_y,
                10.0 * s,
                content_h,
                mx,
                my,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    /// Рисует плавающее меню Alt+Enter (LSP быстрые действия)
    /// Возвращает true если мышь над меню
    pub fn draw_lsp_actions_menu(
        &mut self,
        menu: &crate::app::LspActionsMenu,
        _blink_alpha: f32,
    ) -> bool {
        if menu.items.is_empty() {
            // Показываем "Загрузка..." если ждём code actions
            if menu.pending_request_id.is_some() {
                let s = self.scale_factor;
                let w = 180.0 * s;
                let h = 36.0 * s;
                let x = menu.menu_x;
                let y = menu.menu_y;
                self.push_rounded_rect(
                    x - 2.0,
                    y - 2.0,
                    w + 4.0,
                    h + 4.0,
                    5.0 * s,
                    [0.20, 0.20, 0.25, 1.0],
                );
                self.push_rounded_rect(x, y, w, h, 4.0 * s, [0.14, 0.15, 0.19, 1.0]);
                self.draw_string_scaled(
                    "Загрузка...",
                    x + 12.0 * s,
                    y + h / 2.0 + 6.0 * s,
                    [0.5, 0.5, 0.5, 1.0],
                    0.9,
                );
            }
            return false;
        }

        let s = self.scale_factor;
        let item_h = 36.0 * s;

        let mut max_item_w = 320.0 * s;
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        for item in &menu.items {
            let label_str = lsp_action_label(item, &mut scratch);
            let w = self.measure_ui_width(&label_str, 0.9) + 40.0 * s;
            if w > max_item_w {
                max_item_w = w;
            }
        }
        let menu_w = max_item_w;
        let menu_h = menu.items.len() as f32 * item_h + 8.0 * s;

        // Подгоняем к экрану
        let max_x = self.width - menu_w - 4.0 * s;
        let max_y = self.height - menu_h - 4.0 * s;
        let mx_pos = menu.menu_x.min(max_x).max(0.0);
        let my_pos = menu.menu_y.min(max_y).max(0.0);

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        let hovered =
            mx >= mx_pos && mx <= mx_pos + menu_w && my >= my_pos && my <= my_pos + menu_h;

        // Тень
        self.push_rounded_rect(
            mx_pos + 4.0 * s,
            my_pos + 4.0 * s,
            menu_w,
            menu_h,
            6.0 * s,
            [0.0, 0.0, 0.0, 0.45],
        );
        // Фон меню + рамка
        self.push_rounded_rect(
            mx_pos - 1.0,
            my_pos - 1.0,
            menu_w + 2.0,
            menu_h + 2.0,
            6.0 * s,
            [0.35, 0.25, 0.50, 0.6],
        );
        self.push_rounded_rect(
            mx_pos,
            my_pos,
            menu_w,
            menu_h,
            5.0 * s,
            [0.12, 0.13, 0.17, 1.0],
        );

        let mut prev_group = 0;
        for (i, item) in menu.items.iter().enumerate() {
            let item_y = my_pos + 4.0 * s + i as f32 * item_h;

            let group = match item {
                crate::app::LspActionItem::CodeAction(_) => 1,
                crate::app::LspActionItem::AddNoqa { .. }
                | crate::app::LspActionItem::AddNoqaAll => 2,
                crate::app::LspActionItem::FixAll
                | crate::app::LspActionItem::OrganizeImports
                | crate::app::LspActionItem::CompleteImports => 3,
            };

            if prev_group != 0 && group != prev_group {
                self.push_rect(
                    mx_pos + 12.0 * s,
                    item_y - 1.0,
                    menu_w - 24.0 * s,
                    1.5,
                    [1.0, 1.0, 1.0, 0.08],
                );
            }
            prev_group = group;

            let is_selected = i == menu.selected;
            let is_hovered =
                mx >= mx_pos && mx <= mx_pos + menu_w && my >= item_y && my <= item_y + item_h;

            let group_color = match group {
                1 => [0.38, 0.75, 1.0, 1.0],  // Синий для фиксов
                2 => [0.75, 0.50, 1.0, 1.0],  // Фиолетовый для noqa
                3 => [0.45, 0.90, 0.60, 1.0], // Зеленый для глобальных действий
                _ => [1.0, 1.0, 1.0, 1.0],
            };

            if is_selected || is_hovered {
                let alpha = if is_selected { 0.35 } else { 0.15 };
                let hi_color = [
                    group_color[0] * 0.4 + 0.15,
                    group_color[1] * 0.4 + 0.15,
                    group_color[2] * 0.4 + 0.15,
                    alpha,
                ];
                self.push_rounded_rect(
                    mx_pos + 3.0 * s,
                    item_y + 1.0,
                    menu_w - 6.0 * s,
                    item_h - 2.0,
                    4.0 * s,
                    hi_color,
                );
            }

            // Цветная полоска группы слева
            self.push_rounded_rect(
                mx_pos + 8.0 * s,
                item_y + 8.0 * s,
                3.0 * s,
                item_h - 16.0 * s,
                1.5 * s,
                group_color,
            );

            let label_str = lsp_action_label(item, &mut scratch);
            let label_color = match item {
                crate::app::LspActionItem::FixAll
                | crate::app::LspActionItem::OrganizeImports
                | crate::app::LspActionItem::CompleteImports => group_color,
                _ => self.theme.fg,
            };

            let text_y = item_y + item_h / 2.0 + 6.0 * s;
            self.draw_string_scaled(&label_str, mx_pos + 18.0 * s, text_y, label_color, 0.9);
        }
        self.scratch_buffer = scratch;

        self.flush();
        hovered
    }
}
