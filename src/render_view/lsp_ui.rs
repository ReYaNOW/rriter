use crate::renderer::Renderer;
use glow::HasContext;

impl Renderer {
    /// Рисует содержимое панели LSP серверов (левая панель)
    pub fn draw_lsp_servers_panel(
        &mut self,
        content_x: f32,
        content_y: f32,
        content_w: f32,
        content_h: f32,
        s: f32,
        servers: &[crate::lsp::LspServerInfo],
        expanded_logs: &rustc_hash::FxHashSet<String>,
        scroll_y: f32,
        scroll_x: f32,
        lsp_log_editors: &rustc_hash::FxHashMap<String, crate::editor::Editor>,
        lsp_logs_focused: &Option<String>,
        fix_all_active: bool,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
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

        let mut current_y = content_y + 8.0 * s - scroll_y.round();
        let mut total_h = 8.0 * s;
        let mut max_log_w = 0.0f32;
        for info in servers.iter() {
            let is_expanded = expanded_logs.contains(info.name);
            let mut logs_h = 0.0;
            if is_expanded {
                let mut lines = 0;
                let mut skip_until = None;
                if let Some(ed) = lsp_log_editors.get(info.name) {
                    for i in 0..ed.line_offsets.len() {
                        if let Some(tgt) = skip_until {
                            if i < tgt {
                                continue;
                            }
                            skip_until = None;
                        }
                        lines += 1;
                        if ed.folded_lines.contains(&i) {
                            skip_until = Some(ed.foldable_lines[&i]);
                        }
                    }
                } else {
                    for entry in &info.logs {
                        lines += entry.text.split('\n').count();
                    }
                }
                logs_h = (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s;

                for entry in &info.logs {
                    for line in entry.text.split('\n') {
                        let lw = self.measure_mono_width(line, 0.7);
                        if lw > max_log_w {
                            max_log_w = lw;
                        }
                    }
                }
            }
            total_h += 136.0 * s + logs_h + 16.0 * s;
        }

        for (server_idx, info) in servers.iter().enumerate() {
            let is_expanded = expanded_logs.contains(info.name);
            let mut logs_h = 0.0;
            if is_expanded {
                let mut lines = 0;
                let mut skip_until = None;
                if let Some(ed) = lsp_log_editors.get(info.name) {
                    for i in 0..ed.line_offsets.len() {
                        if let Some(tgt) = skip_until {
                            if i < tgt {
                                continue;
                            }
                            skip_until = None;
                        }
                        lines += 1;
                        if ed.folded_lines.contains(&i) {
                            skip_until = Some(ed.foldable_lines[&i]);
                        }
                    }
                } else {
                    for entry in &info.logs {
                        lines += entry.text.split('\n').count();
                    }
                }
                logs_h = (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s;
            }
            let base_h = 136.0 * s;
            let row_h = base_h + logs_h;

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

                let bw_restart = self.measure_ui_width(label_restart, 0.8) + btn_pad * 2.0;
                let bw_toggle = self.measure_ui_width(label_toggle, 0.8) + btn_pad * 2.0;
                let bw_stop = self.measure_ui_width(label_stop, 0.8) + btn_pad * 2.0;
                let bw_logs = self.measure_ui_width(label_logs, 0.8) + btn_pad * 2.0;
                let bw_fix_all = self.measure_ui_width(label_fix_all, 0.8) + btn_pad * 2.0;

                let btn_x_restart = card_x + pad_x;
                let btn_x_toggle = btn_x_restart + bw_restart + 6.0 * s;
                let btn_x_stop = btn_x_toggle + bw_toggle + 6.0 * s;

                let btn_x_fix_all = card_x + pad_x;
                let btn_x_logs = btn_x_fix_all + bw_fix_all + 6.0 * s;

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
                    let log_bg_x = card_x + pad_x;
                    let log_bg_y = btn_y2 + btn_h + 10.0 * s;
                    let log_bg_w = card_w - pad_x * 2.0;
                    let log_bg_h = logs_h - 18.0 * s;

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

                    ui_registry.register_rect(
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
                        let _first_visible_line = (scroll_y / line_h).floor() as usize;

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

                        let mut text_y = log_bg_y + 16.0 * s;
                        let mut global_line_count = 0;
                        let mut global_byte_off: usize = 0;
                        let mut skip_until = None;

                        'entry_loop: for entry in &info.logs {
                            let mut line_byte_off_in_entry = 0;
                            for line in entry.text.split('\n') {
                                if let Some(tgt) = skip_until {
                                    if global_line_count < tgt {
                                        global_line_count += 1;
                                        line_byte_off_in_entry += line.len() + 1;
                                        continue;
                                    } else {
                                        skip_until = None;
                                    }
                                }

                                if text_y > inter_y2 + line_h {
                                    break 'entry_loop;
                                }

                                if text_y + line_h > inter_y1 {
                                    let abs_line_start = global_byte_off + line_byte_off_in_entry;
                                    let abs_line_end = abs_line_start + line.len();

                                    if sel_lo < sel_hi
                                        && sel_lo <= abs_line_end
                                        && sel_hi > abs_line_start
                                    {
                                        let in_s =
                                            sel_lo.saturating_sub(abs_line_start).min(line.len());
                                        let in_e =
                                            sel_hi.saturating_sub(abs_line_start).min(line.len());
                                        let in_s = (0..=in_s)
                                            .rev()
                                            .find(|&i| line.is_char_boundary(i))
                                            .unwrap_or(0);
                                        let in_e = (in_e..=line.len())
                                            .find(|&i| line.is_char_boundary(i))
                                            .unwrap_or(line.len());
                                        let x1 = log_bg_x
                                            + 20.0 * s
                                            + self.measure_mono_width(&line[..in_s], 0.7)
                                            - scroll_x.round();
                                        let x2 = log_bg_x
                                            + 20.0 * s
                                            + self.measure_mono_width(&line[..in_e], 0.7)
                                            - scroll_x.round();
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

                                    let mut current_x = log_bg_x + 20.0 * s - scroll_x.round();
                                    let mut i = 0;
                                    let abs_entry_start = global_byte_off + line_byte_off_in_entry;
                                    while i < line.len() {
                                        let abs_byte = abs_entry_start + i;
                                        let mut color = [0.875, 0.882, 0.902, 1.0];
                                        let mut chunk_end = line.len();

                                        for span in &entry.spans {
                                            if abs_byte >= span.start && abs_byte < span.end {
                                                color = span.color;
                                                chunk_end = chunk_end
                                                    .min(span.end.saturating_sub(abs_entry_start));
                                                break;
                                            } else if span.start > abs_byte {
                                                chunk_end = chunk_end.min(
                                                    span.start.saturating_sub(abs_entry_start),
                                                );
                                            }
                                        }
                                        // Гарантируем продвижение: chunk_end всегда > i
                                        let chunk_end = chunk_end.min(line.len()).max(i + 1);
                                        // Выровнять по границе UTF-8
                                        let chunk_end = (chunk_end..=line.len())
                                            .find(|&e| line.is_char_boundary(e))
                                            .unwrap_or(line.len());

                                        let text_chunk = &line[i..chunk_end];
                                        self.draw_string_mono_scaled(
                                            text_chunk, current_x, text_y, color, 0.7,
                                        );
                                        current_x += self.measure_mono_width(text_chunk, 0.7);
                                        i = chunk_end;
                                    }

                                    if let Some(log_ed) = lsp_log_editors.get(info.name) {
                                        let is_folded =
                                            log_ed.folded_lines.contains(&global_line_count);
                                        let is_foldable =
                                            log_ed.foldable_lines.contains_key(&global_line_count);
                                        if is_foldable {
                                            let icon =
                                                if is_folded { "\u{f0115}" } else { "\u{f0114}" };
                                            let icon_x = log_bg_x + 4.0 * s - scroll_x.round();
                                            let is_hovered = ui_registry.register_rect(
                                                crate::ui_system::UiId::LspLogFoldToggle(
                                                    server_idx,
                                                    global_line_count,
                                                ),
                                                icon_x,
                                                text_y - 12.0 * s,
                                                16.0 * s,
                                                16.0 * s,
                                                mx,
                                                my,
                                            );
                                            let color = if is_hovered {
                                                [0.8, 0.8, 0.9, 1.0]
                                            } else {
                                                [0.6, 0.6, 0.7, 1.0]
                                            };
                                            self.draw_string_scaled(
                                                icon, icon_x, text_y, color, 0.75,
                                            );
                                        }
                                        if is_folded {
                                            let btn_w =
                                                self.measure_ui_width("...", 0.8) + 10.0 * s;
                                            self.push_rounded_rect(
                                                current_x + 5.0 * s,
                                                text_y - 12.0 * s,
                                                btn_w,
                                                line_h - 2.0 * s,
                                                3.0 * s,
                                                [1.0, 1.0, 1.0, 0.1],
                                            );
                                            self.draw_string_scaled(
                                                "...",
                                                current_x + 10.0 * s,
                                                text_y,
                                                [0.8, 0.8, 0.8, 1.0],
                                                0.8,
                                            );
                                            skip_until =
                                                Some(log_ed.foldable_lines[&global_line_count]);
                                        }
                                    }
                                }
                                text_y += line_h;
                                line_byte_off_in_entry += line.len() + 1;
                                global_line_count += 1;
                            }
                            global_byte_off += entry.text.len() + 1;
                        }

                        self.flush();
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
            current_y += row_h + 16.0 * s;
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

        let max_scroll_x = (max_log_w + 20.0 * s - (content_w - 32.0 * s)).max(0.0);
        if max_scroll_x > 0.0 {
            let ratio = (scroll_x / max_scroll_x).clamp(0.0, 1.0);
            let track_w = content_w - 30.0 * s;
            let thumb_w = (content_w / (max_log_w + 20.0 * s) * track_w).max(40.0 * s);
            let thumb_x = content_x + 10.0 * s + ratio * (track_w - thumb_w);
            self.push_rounded_rect(
                thumb_x,
                content_y + content_h - 12.0 * s,
                thumb_w,
                10.0 * s,
                5.0 * s,
                [1.0, 1.0, 1.0, 0.22],
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::LspScrollX,
                content_x,
                content_y + content_h - 12.0 * s,
                content_w,
                10.0 * s,
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
        let menu_w = 320.0 * s;
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

        for (i, item) in menu.items.iter().enumerate() {
            let item_y = my_pos + 4.0 * s + i as f32 * item_h;

            let is_selected = i == menu.selected;
            let is_hovered =
                mx >= mx_pos && mx <= mx_pos + menu_w && my >= item_y && my <= item_y + item_h;

            if is_selected || is_hovered {
                let hi_color = if is_selected {
                    [0.30, 0.20, 0.45, 1.0]
                } else {
                    [0.20, 0.20, 0.28, 1.0]
                };
                self.push_rounded_rect(
                    mx_pos + 3.0 * s,
                    item_y + 1.0,
                    menu_w - 6.0 * s,
                    item_h - 2.0,
                    4.0 * s,
                    hi_color,
                );
            }

            let (icon_str, label, label_color) = match item {
                crate::app::LspActionItem::CodeAction(action) => {
                    let label = action.title.as_str();
                    ("⚡", label, self.theme.fg)
                }
                crate::app::LspActionItem::AddNoqa { codes } => {
                    // временная строка не нужна — выводим отдельно
                    let _ = codes;
                    ("🔇", "Добавить # noqa: …", [0.80, 0.75, 0.55, 1.0])
                }
                crate::app::LspActionItem::AddNoqaAll => (
                    "🔕",
                    "Добавить # noqa (отключить все)",
                    [0.65, 0.60, 0.50, 1.0],
                ),
            };

            let text_y = item_y + item_h / 2.0 + 6.0 * s;
            self.draw_string_scaled(icon_str, mx_pos + 10.0 * s, text_y, label_color, 0.9);

            // Для AddNoqa с кодами — собираем строку из кодов
            let label_str: std::borrow::Cow<str> = match item {
                crate::app::LspActionItem::AddNoqa { codes } if !codes.is_empty() => {
                    let s = format!("Добавить # noqa: {}", codes.join(", "));
                    std::borrow::Cow::Owned(s)
                }
                _ => std::borrow::Cow::Borrowed(label),
            };
            self.draw_string_scaled(&label_str, mx_pos + 28.0 * s, text_y, label_color, 0.9);

            // Подсказка по типу действия (quickfix/source)
            if let crate::app::LspActionItem::CodeAction(action) = item {
                if let Some(kind) = &action.kind {
                    let kind_short = if kind.contains("fixAll") {
                        "fix all"
                    } else if kind.contains("quickfix") {
                        "quickfix"
                    } else {
                        kind.as_str()
                    };
                    let kind_w = self.measure_ui_width(kind_short, 0.72);
                    self.draw_string_scaled(
                        kind_short,
                        mx_pos + menu_w - kind_w - 10.0 * s,
                        text_y,
                        [0.45, 0.45, 0.45, 1.0],
                        0.72,
                    );
                }
            }
        }

        self.flush();
        hovered
    }
}
