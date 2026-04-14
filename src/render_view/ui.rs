use crate::editor::Editor;
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
                (content_y + 32.0 * s).round(),[0.45, 0.45, 0.45, 1.0],
                text_scale,
            );
        }

        let mut current_y = content_y + 8.0 * s - scroll_y.round();
                                let mut total_h = 8.0 * s;
                                let mut max_log_w = 0.0f32;
                                for info in servers.iter() {
                                    let is_expanded = expanded_logs.contains(info.name);
                                    let logs_h = if is_expanded { 350.0 * s } else { 0.0 };
                                    total_h += 136.0 * s + logs_h + 16.0 * s;
                                    if is_expanded {
                                        for entry in &info.logs {
                                            for line in entry.text.split('\n') {
                                                let lw = self.measure_mono_width(line, 0.7);
                                                if lw > max_log_w { max_log_w = lw; }
                                            }
                                        }
                                    }
                                }

                                        for info in servers.iter() {
                                    let is_expanded = expanded_logs.contains(info.name);
                                    let logs_h = if is_expanded { 350.0 * s } else { 0.0 };
            let base_h = 136.0 * s;
            let row_h = base_h + logs_h;

            if current_y + row_h > content_y && current_y < content_y + content_h {
                let card_x = content_x + 12.0 * s;
                let card_w = content_w - 24.0 * s;

                // Тень и бордер карточки
                self.push_rounded_rect(card_x - 1.0, current_y - 1.0, card_w + 2.0, row_h + 2.0, 7.0 * s,[0.35, 0.30, 0.45, 0.4]);
                self.push_rounded_rect(card_x, current_y, card_w, row_h, 6.0 * s,[0.18, 0.19, 0.24, 1.0]);

            let dot_r = 5.0 * s;
            let dot_x = card_x + pad_x + dot_r;
            let dot_y = current_y + 16.0 * s;

            let (dot_color, status_text) = match info.status {
                crate::lsp::LspServerStatus::Running  => ([0.28, 0.85, 0.45, 1.0], "Работает"),
                crate::lsp::LspServerStatus::Starting => ([0.85, 0.75, 0.25, 1.0], "Запуск..."),
                crate::lsp::LspServerStatus::Crashed  => ([0.90, 0.30, 0.30, 1.0], "Упал"),
                crate::lsp::LspServerStatus::Disabled => ([0.45, 0.45, 0.45, 1.0], "Отключён"),
            };
            self.push_rounded_rect(dot_x - dot_r, dot_y - dot_r, dot_r * 2.0, dot_r * 2.0, dot_r, dot_color);

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

            let label_restart  = "Перезапуск";
            let label_toggle   = if matches!(info.status, crate::lsp::LspServerStatus::Disabled) { "Включить" } else { "Отключить" };
            let label_stop     = "Остановить";
            let label_logs     = if is_expanded { "Скрыть логи" } else { "Логи" };
            let label_fix_all  = "Fix All";

            let bw_restart  = self.measure_ui_width(label_restart,  0.8) + btn_pad * 2.0;
            let bw_toggle   = self.measure_ui_width(label_toggle,   0.8) + btn_pad * 2.0;
            let bw_stop     = self.measure_ui_width(label_stop,     0.8) + btn_pad * 2.0;
            let bw_logs     = self.measure_ui_width(label_logs,     0.8) + btn_pad * 2.0;
            let bw_fix_all  = self.measure_ui_width(label_fix_all,  0.8) + btn_pad * 2.0;

                        let btn_x_restart  = card_x + pad_x;
            let btn_x_toggle   = btn_x_restart + bw_restart + 6.0 * s;
            let btn_x_stop     = btn_x_toggle + bw_toggle + 6.0 * s;

            let btn_x_fix_all = card_x + pad_x;
            let btn_x_logs    = btn_x_fix_all + bw_fix_all + 6.0 * s;

            let hover_restart  = mx >= btn_x_restart  && mx <= btn_x_restart  + bw_restart  && my >= btn_y1 && my <= btn_y1 + btn_h;
            let hover_toggle   = mx >= btn_x_toggle   && mx <= btn_x_toggle   + bw_toggle   && my >= btn_y1 && my <= btn_y1 + btn_h;
            let is_stopped = matches!(info.status, crate::lsp::LspServerStatus::Disabled | crate::lsp::LspServerStatus::Crashed);
            let hover_stop    = !is_stopped && mx >= btn_x_stop    && mx <= btn_x_stop    + bw_stop    && my >= btn_y1 && my <= btn_y1 + btn_h;

            let hover_logs     = mx >= btn_x_logs     && mx <= btn_x_logs     + bw_logs     && my >= btn_y2 && my <= btn_y2 + btn_h;
            let fix_enabled   = !is_stopped && fix_all_active;
            let hover_fix_all = fix_enabled  && mx >= btn_x_fix_all && mx <= btn_x_fix_all + bw_fix_all && my >= btn_y2 && my <= btn_y2 + btn_h;

            let btn_bg_restart = if hover_restart  {[0.35, 0.35, 0.40, 1.0] } else {[0.26, 0.26, 0.32, 1.0] };
            let btn_bg_toggle  = if hover_toggle   {[0.35, 0.35, 0.40, 1.0] } else {[0.26, 0.26, 0.32, 1.0] };
            let btn_bg_logs    = if hover_logs     {[0.35, 0.35, 0.40, 1.0] } else {[0.26, 0.26, 0.32, 1.0] };
            let btn_bg_stop    = if is_stopped     {[0.20, 0.20, 0.25, 0.6] } else if hover_stop    {[0.45, 0.22, 0.22, 1.0] } else {[0.32, 0.15, 0.15, 1.0] };
            let btn_bg_fix_all = if !fix_enabled   {[0.18, 0.18, 0.22, 0.6] } else if hover_fix_all {[0.22, 0.42, 0.28, 1.0] } else {[0.15, 0.30, 0.20, 1.0] };

            let text_color_stop    = if is_stopped   {[0.55, 0.55, 0.60, 1.0] } else {[0.95, 0.55, 0.55, 1.0] };
            let text_color_fix_all = if !fix_enabled {[0.40, 0.40, 0.44, 1.0] } else {[0.55, 0.95, 0.65, 1.0] };

            let text_y1 = (btn_y1 + btn_h / 2.0 + 4.0 * s).round();
            let text_y2 = (btn_y2 + btn_h / 2.0 + 4.0 * s).round();

            self.push_rounded_rect(btn_x_restart, btn_y1, bw_restart, btn_h, 3.0 * s, btn_bg_restart);
            self.draw_string_scaled(label_restart, (btn_x_restart + btn_pad).round(), text_y1, self.theme.fg, 0.8);

            self.push_rounded_rect(btn_x_toggle, btn_y1, bw_toggle, btn_h, 3.0 * s, btn_bg_toggle);
            self.draw_string_scaled(label_toggle, (btn_x_toggle + btn_pad).round(), text_y1, self.theme.fg, 0.8);

            self.push_rounded_rect(btn_x_stop, btn_y1, bw_stop, btn_h, 3.0 * s, btn_bg_stop);
            self.draw_string_scaled(label_stop, (btn_x_stop + btn_pad).round(), text_y1, text_color_stop, 0.8);

            self.push_rounded_rect(btn_x_fix_all, btn_y2, bw_fix_all, btn_h, 3.0 * s, btn_bg_fix_all);
            self.draw_string_scaled(label_fix_all, (btn_x_fix_all + btn_pad).round(), text_y2, text_color_fix_all, 0.8);

            self.push_rounded_rect(btn_x_logs, btn_y2, bw_logs, btn_h, 3.0 * s, btn_bg_logs);
            self.draw_string_scaled(label_logs, (btn_x_logs + btn_pad).round(), text_y2,[0.8, 0.85, 1.0, 1.0], 0.8);

                                                if is_expanded {
                let log_bg_x = card_x + pad_x;
                let log_bg_y = btn_y2 + btn_h + 10.0 * s;
                let log_bg_w = card_w - pad_x * 2.0;
                let log_bg_h = logs_h - 18.0 * s;

                                let border_color = if lsp_logs_focused.as_deref() == Some(info.name) {[0.44, 0.28, 0.75, 0.8]
                } else {[0.1, 0.1, 0.12, 1.0]
                };

                self.push_rounded_rect(log_bg_x - 1.0, log_bg_y - 1.0, log_bg_w + 2.0, log_bg_h + 2.0, 4.0 * s, border_color);
                self.push_rounded_rect(log_bg_x, log_bg_y, log_bg_w, log_bg_h, 4.0 * s,[0.08, 0.08, 0.10, 1.0]);

                self.flush();
                let inter_y1 = log_bg_y.max(content_y);
                let inter_y2 = (log_bg_y + log_bg_h).min(content_y + content_h);
                let inter_h = (inter_y2 - inter_y1).max(0.0);

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
                    let max_lines_vis = (log_bg_h / line_h) as usize + 1;

                    let mut total_lines = 0;
                    let mut lines_per_entry = Vec::with_capacity(info.logs.len());
                    for entry in &info.logs {
                        let cnt = entry.text.split('\n').count();
                        total_lines += cnt;
                        lines_per_entry.push(cnt);
                    }
                    let start_line = total_lines.saturating_sub(max_lines_vis);

                    let mut entry_idx = 0;
                    let mut lines_skipped = 0;
                    while entry_idx < info.logs.len() {
                        if lines_skipped + lines_per_entry[entry_idx] > start_line {
                            break;
                        }
                        lines_skipped += lines_per_entry[entry_idx];
                        entry_idx += 1;
                    }

                    let mut sel_lo = 0;
                    let mut sel_hi = 0;
                    if let Some(log_ed) = lsp_log_editors.get(info.name) {
                        let (lo, hi) = match log_ed.selection_anchor {
                            Some(anchor) => (anchor.min(log_ed.cursor), anchor.max(log_ed.cursor)),
                            None => (log_ed.cursor, log_ed.cursor),
                        };
                        sel_lo = lo;
                        sel_hi = hi;
                    }

                    let mut text_y = log_bg_y + 16.0 * s;
                    let mut current_global_line = lines_skipped;
                    let mut global_byte_off: usize = info.logs[..entry_idx].iter().map(|e| e.text.len() + 1).sum();

                    for entry in &info.logs[entry_idx..] {
                        let mut line_byte_off = 0;
                        for line in entry.text.split('\n') {
                            if current_global_line >= start_line {
                                let abs_line_start = global_byte_off + line_byte_off;
                                let abs_line_end = abs_line_start + line.len();

                                if sel_lo < sel_hi && sel_lo <= abs_line_end && sel_hi > abs_line_start {
                                    let in_s = sel_lo.saturating_sub(abs_line_start).min(line.len());
                                    let in_e = sel_hi.saturating_sub(abs_line_start).min(line.len());
                                    let in_s = (0..=in_s).rev()
                                        .find(|&i| line.is_char_boundary(i)).unwrap_or(0);
                                    let in_e = (in_e..=line.len())
                                        .find(|&i| line.is_char_boundary(i)).unwrap_or(line.len());
                                    let x1 = log_bg_x + 6.0 * s
                                        + self.measure_mono_width(&line[..in_s], 0.7)
                                        - scroll_x.round();
                                    let x2 = log_bg_x + 6.0 * s
                                        + self.measure_mono_width(&line[..in_e], 0.7)
                                        - scroll_x.round();
                                    let ry = text_y - 14.0 * s;
                                    let x1c = x1.max(log_bg_x);
                                    let x2c = x2.min(log_bg_x + log_bg_w);
                                    if x2c > x1c {
                                        self.push_rounded_rect(x1c, ry, x2c - x1c, line_h, 0.0,[0.40, 0.28, 0.72, 0.45]);
                                    }
                                }

                                let mut current_x = log_bg_x + 6.0 * s - scroll_x.round();
                                let mut i = 0;
                                while i < line.len() {
                                    let abs_byte = line_byte_off + i;
                                    let mut color =[0.875, 0.882, 0.902, 1.0];
                                    let mut chunk_end = line.len();

                                    for span in &entry.spans {
                                        if abs_byte >= span.start && abs_byte < span.end {
                                            color = span.color;
                                            chunk_end = chunk_end.min(span.end - line_byte_off);
                                            break;
                                        } else if span.start > abs_byte {
                                            chunk_end = chunk_end.min(span.start - line_byte_off);
                                        }
                                    }

                                    let text_chunk = &line[i..chunk_end];
                                    self.draw_string_mono_scaled(text_chunk, current_x, text_y, color, 0.7);
                                    current_x += self.measure_mono_width(text_chunk, 0.7);
                                    i = chunk_end;
                                }
                                text_y += line_h;
                            }
                            line_byte_off += line.len() + 1;
                            current_global_line += 1;
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
                5.0 * s,[1.0, 1.0, 1.0, 0.22],
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
                5.0 * s,[1.0, 1.0, 1.0, 0.22],
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
                self.push_rounded_rect(x - 2.0, y - 2.0, w + 4.0, h + 4.0, 5.0 * s, [0.20, 0.20, 0.25, 1.0]);
                self.push_rounded_rect(x, y, w, h, 4.0 * s, [0.14, 0.15, 0.19, 1.0]);
                self.draw_string_scaled("Загрузка...", x + 12.0 * s, y + h / 2.0 + 6.0 * s, [0.5, 0.5, 0.5, 1.0], 0.9);
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

        let hovered = mx >= mx_pos && mx <= mx_pos + menu_w && my >= my_pos && my <= my_pos + menu_h;

        // Тень
        self.push_rounded_rect(mx_pos + 4.0 * s, my_pos + 4.0 * s, menu_w, menu_h, 6.0 * s, [0.0, 0.0, 0.0, 0.45]);
        // Фон меню + рамка
        self.push_rounded_rect(mx_pos - 1.0, my_pos - 1.0, menu_w + 2.0, menu_h + 2.0, 6.0 * s, [0.35, 0.25, 0.50, 0.6]);
        self.push_rounded_rect(mx_pos, my_pos, menu_w, menu_h, 5.0 * s, [0.12, 0.13, 0.17, 1.0]);

        for (i, item) in menu.items.iter().enumerate() {
            let item_y = my_pos + 4.0 * s + i as f32 * item_h;

            let is_selected = i == menu.selected;
            let is_hovered = mx >= mx_pos && mx <= mx_pos + menu_w && my >= item_y && my <= item_y + item_h;

            if is_selected || is_hovered {
                let hi_color = if is_selected {
                    [0.30, 0.20, 0.45, 1.0]
                } else {
                    [0.20, 0.20, 0.28, 1.0]
                };
                self.push_rounded_rect(mx_pos + 3.0 * s, item_y + 1.0, menu_w - 6.0 * s, item_h - 2.0, 4.0 * s, hi_color);
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
                crate::app::LspActionItem::AddNoqaAll => {
                    ("🔕", "Добавить # noqa (отключить все)", [0.65, 0.60, 0.50, 1.0])
                }
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
                    let kind_short = if kind.contains("fixAll") { "fix all" }
                        else if kind.contains("quickfix") { "quickfix" }
                        else { kind.as_str() };
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

    pub fn draw_icon(&mut self, tex: &glow::Texture, x: f32, y: f32, w: f32, h: f32) {
        self.flush();
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
        }
        self.push_quad(x, y, w, h, 0.0, 0.0, 1.0, 1.0, [1.0, 1.0, 1.0, 1.0], 1.0);
        self.flush();
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        }
    }

    pub fn draw_atlas_icon(
        &mut self,
        icon: crate::widgets::IconType,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
    ) {
        if let Some(&tex) = self.icons.get(&icon) {
            self.flush(); // Сбрасываем батч, чтобы сменить текстуру
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            }
            self.push_quad(x, y, size, size, 0.0, 0.0, 1.0, 1.0, color, 5.0);
            self.flush();
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture)); // Возвращаем шрифтовой атлас
            }
        }
    }

    /// Рисует SVG-иконку из кэша file_icon_cache.
    /// Загружает текстуру при первом обращении (не в draw-цикле — только при промахе кэша).
    pub fn draw_file_icon(
        &mut self,
        key: &'static str,
        _is_folder: bool,
        x: f32,
        y: f32,
        size: f32,
    ) {
                if !self.file_icon_cache.contains_key(key) {
            let pre_rasterized = {
                let mut cache = crate::app::file_tree::RASTERIZED_ICONS.lock().unwrap();
                if let Some(opt_data) = cache.get_mut(key) {
                    opt_data.take()
                } else {
                    None
                }
            };

            if let Some(data) = pre_rasterized {
                let target = 64i32;
                let tex = unsafe {
                    let tex = self.gl.create_texture().unwrap();
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    self.gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA8 as i32,
                        target,
                        target,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(Some(&data)),
                    );
                    self.gl.generate_mipmap(glow::TEXTURE_2D);
                    self.gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::LINEAR_MIPMAP_LINEAR as i32,
                    );
                    self.gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::LINEAR as i32,
                    );
                    self.gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_S,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    self.gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_T,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                    tex
                };
                self.file_icon_cache.insert(key, tex);
            } else {
                // Иконка еще не растеризована фоновым потоком.
                // Возвращаемся без блокировки UI! Фоновый поток пришлет сигнал, когда закончит.
                return;
            }
        }

        if let Some(&tex) = self.file_icon_cache.get(key) {
            self.flush();
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            }
            self.push_quad(
                x,
                y,
                size,
                size,
                0.0,
                0.0,
                1.0,
                1.0,
                [1.0, 1.0, 1.0, 1.0],
                5.0,
            );
            self.flush();
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            }
        }
    }

    // (функции удалены)

    pub fn draw_autocomplete(
        &mut self,
        x: f32,
        mut y: f32,
        options: &[(crate::highlighter::CompletionItem, Vec<usize>)],
        selected_idx: usize,
        anim_progress: f32,
        scroll_y: f32,
        hovered_idx: Option<usize>,
    ) -> (f32, f32, f32, f32) {
        let scale = self.scale_factor;

        let step = 36.0 * scale;
        let item_h = 28.0 * scale;
        let padding_top = 8.0 * scale;
        let padding_bottom = 8.0 * scale;

        let mut max_w = 195.0 * scale;
        for (opt, _) in options {
            let w = self.measure_width(opt.word.as_str(), "", 0, opt.word.len());
            if w + 60.0 * scale > max_w {
                max_w = w + 60.0 * scale;
            }
        }

        max_w = max_w.min(450.0 * scale);

        let visible_items = options.len().min(7);

        let target_h = visible_items as f32 * step + padding_top + padding_bottom;
        let total_h = options.len() as f32 * step + padding_top + padding_bottom;

        let current_h = target_h * anim_progress;

        if y + target_h > self.height {
            y -= target_h + 10.0 * scale;
        } else {
            y += 10.0 * scale;
        }

        // --- 1. Отрисовка Тени ---
        for i in 1..=5 {
            let offset = i as f32 * scale;
            let alpha = (0.15 - (i as f32 * 0.03)) * anim_progress;
            self.push_rounded_rect(
                x - offset,
                y - offset,
                max_w + offset * 2.0,
                current_h + offset * 2.0,
                6.0 * scale,
                [0.0, 0.0, 0.0, alpha],
            );
        }

        // --- 2. Рамка и Фон ---
        let bg_color = [0.15, 0.16, 0.20, 1.0];
        let border_color = [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.8];

        // ИСПРАВЛЕНИЕ: Делаем рамку толще и математически синхронизируем внутренний радиус
        let border_width = 1.5 * scale;
        self.push_rounded_rect(
            x - border_width,
            y - border_width,
            max_w + border_width * 2.0,
            current_h + border_width * 2.0,
            5.5 * scale, // Внешний радиус
            border_color,
        );
        self.push_rounded_rect(
            x,
            y,
            max_w,
            current_h,
            4.0 * scale, // Внутренний радиус (ровно 5.5 - 1.5), чтобы не было "точек" на углах
            bg_color,
        );

        self.flush();

        // --- 3. Scissor Test ---
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (y + current_h)).round() as i32;
            self.gl.scissor(
                x.round() as i32,
                sy,
                max_w.round() as i32,
                current_h.round() as i32,
            );
        }

        // --- 4. Отрисовка элементов ---
        let mut current_y = y + padding_top - scroll_y;

        for (i, (item, matches)) in options.iter().enumerate() {
            if current_y + step < y || current_y > y + current_h {
                current_y += step;
                continue;
            }

            let sel_rect_y = (current_y + (step - item_h) / 2.0).round();

            if i == selected_idx {
                self.push_rounded_rect(
                    x + 4.0 * scale,
                    sel_rect_y,
                    max_w - 8.0 * scale,
                    item_h,
                    4.0 * scale,
                    [0.25, 0.27, 0.35, 1.0],
                );
            } else if Some(i) == hovered_idx {
                self.push_rounded_rect(
                    x + 4.0 * scale,
                    sel_rect_y,
                    max_w - 8.0 * scale,
                    item_h,
                    4.0 * scale,
                    [0.20, 0.21, 0.28, 1.0],
                );
            }

            let mut cx = x + 12.0 * scale;

            let (icon_char, icon_fg) = match item.kind {
                crate::highlighter::SymbolKind::Class => ("\u{f03d7}", [0.8, 0.9, 1.0, 1.0]),
                crate::highlighter::SymbolKind::Function => ("\u{f0295}", [0.8, 1.0, 0.8, 1.0]),
                crate::highlighter::SymbolKind::Variable => ("\u{f0ae7}", [0.9, 0.8, 1.0, 1.0]),
                crate::highlighter::SymbolKind::Parameter => ("\u{f03ea}", [1.0, 0.9, 0.8, 1.0]),
                crate::highlighter::SymbolKind::Keyword => ("\u{f030b}", [1.0, 0.8, 0.9, 1.0]),
                crate::highlighter::SymbolKind::Unknown => ("\u{f03d7}", [0.65, 0.65, 0.65, 1.0]),
            };

            let icon_sz = 20.0 * scale;

            if let Some(g) = self.get_glyph(icon_char.chars().next().unwrap()) {
                let char_scale = 0.8;
                let actual_w = g.width * char_scale * scale;
                let actual_h = g.height * char_scale * scale;

                let char_x = cx + (icon_sz - actual_w) / 2.0;
                let char_y = sel_rect_y + (item_h - actual_h) / 2.0;

                self.push_quad(
                    char_x.round(),
                    char_y.round(),
                    actual_w,
                    actual_h,
                    g.u,
                    g.v,
                    g.uw,
                    g.vh,
                    icon_fg,
                    0.0,
                );
            }
            cx += icon_sz + 8.0 * scale;

            let cy = sel_rect_y + item_h * 0.72;

            let mut truncated = false;
            for (j, c) in item.word.chars().enumerate() {
                if let Some(g) = self.get_glyph(c) {
                    if cx + g.advance > x + max_w - 30.0 * scale {
                        truncated = true;
                        break;
                    }

                    let color = if matches.contains(&j) {
                        [1.0, 0.474, 0.776, 1.0]
                    } else {
                        self.theme.fg
                    };

                    self.push_quad(
                        (cx + g.offset_x).round(),
                        (cy - g.offset_y).round(),
                        g.width,
                        g.height,
                        g.u,
                        g.v,
                        g.uw,
                        g.vh,
                        color,
                        g.is_emoji,
                    );
                    cx += g.advance;
                }
            }

            if truncated {
                self.draw_string_scaled("...", cx.round(), cy.round(), [0.5, 0.5, 0.55, 1.0], 1.0);
            }

            current_y += step;
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        // --- 5. Отрисовка Скроллбара (стиль как в главном окне) ---
        if total_h > target_h {
            let max_scroll = (total_h - target_h).max(0.0);
            let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);

            let track_margin = 8.0 * scale;
            let track_h = current_h - track_margin * 2.0;
            let thumb_h = (current_h / total_h * track_h).max(20.0 * scale);
            let thumb_y = y + track_margin + scroll_ratio * (track_h - thumb_h);

            let alpha = (anim_progress * 1.5).clamp(0.0, 0.8);

            self.push_rounded_rect(
                x + max_w - 10.0 * scale,
                thumb_y,
                6.0 * scale,
                thumb_h,
                3.0 * scale,
                [0.7, 0.33, 0.54, alpha],
            );
        }

        self.flush();

        (x, y, max_w, current_h)
    }

    pub fn draw_dialog_window(&mut self, base_title: &str) -> bool {
        let s = self.scale_factor;
        let box_w = 660.0 * s;
        let box_h = 260.0 * s;
        let box_x = 0.0;
        let box_y = 0.0;

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        self.push_vertical_gradient(box_x, box_y, box_w, box_h, top_color, bottom_color);

        let pad_h = 24.0 * s;
        let pad_v = 18.0 * s;
        let btn_h = 44.0 * s;
        let btn_margin = 12.0 * s;
        let content_x = (box_x + pad_h).round();
        let content_y = (box_y + pad_v).round();
        let content_w = (box_w - pad_h * 2.0).round();
        let content_h = (box_h - pad_v - btn_h - btn_margin * 2.0 - pad_v).round();

        self.push_rounded_rect(
            content_x - 1.0,
            content_y - 1.0,
            content_w + 2.0,
            content_h + 2.0,
            8.0 * s,
            [0.224, 0.231, 0.251, 0.8],
        );
        self.push_rounded_rect(
            content_x,
            content_y,
            content_w,
            content_h,
            8.0 * s,
            [0.15, 0.16, 0.20, 1.0],
        );

        let msg1 = format!("Документ «{}» был изменен.", base_title);
        let msg2 = "Сохранить или отклонить изменения?";

        let icon_sz = 120.0 * s;
        let gap = 45.0 * s;
        let padding_inner = 20.0 * s;

        let icon_x = content_x + padding_inner;
        let icon_y = content_y + (content_h - icon_sz) / 2.0;

        self.draw_atlas_icon(
            crate::widgets::IconType::Warning,
            icon_x,
            icon_y,
            icon_sz,
            [1.0, 1.0, 1.0, 1.0],
        );

        let text_x = icon_x + icon_sz + gap;
        let fg = self.theme.fg;
        let text_scale = 1.05;
        let line_h = 28.0 * s;
        let text_block_h = line_h * 2.0;
        let text_y_start = content_y + (content_h - text_block_h) / 2.0 + line_h * 0.85;

        self.draw_string_scaled(&msg1, text_x, text_y_start, fg, text_scale);
        self.draw_string_scaled(
            msg2,
            text_x,
            text_y_start + line_h,
            [0.75, 0.75, 0.80, 1.0],
            text_scale,
        );

        let (btn_save, btn_discard, btn_cancel) =
            crate::widgets::get_dialog_buttons(box_x, box_y, box_w, box_h, s, self);

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        let mut wants_pointer = false;
        wants_pointer |= btn_save.render(self, mx, my, s, false);
        wants_pointer |= btn_discard.render(self, mx, my, s, false);
        wants_pointer |= btn_cancel.render(self, mx, my, s, false);

        self.flush();
        wants_pointer
    }

    pub fn get_faq_max_scroll(&mut self, faq_editor: &Editor, dialog_height: f32) -> f32 {
        let scale = self.scale_factor;
        let mut total_h = 0.0;

        for line in faq_editor.get_full_text().split('\n') {
            if line.starts_with("# ") {
                total_h += 50.0 * scale;
            } else if line.contains('\t') {
                total_h += 38.0 * scale;
            } else if !line.trim().is_empty() {
                total_h += 30.0 * scale;
            } else {
                total_h += 15.0 * scale;
            }
        }

        total_h += 80.0 * scale;
        let pad_top = 35.0 * scale;
        let pad_bottom = 30.0 * scale;
        let title_h = 40.0 * scale;
        let content_h = dialog_height - pad_top - pad_bottom - title_h - 20.0 * scale;

        (total_h - content_h).max(0.0)
    }

    pub fn draw_welcome(&mut self, recent_files: &[std::path::PathBuf]) -> bool {
        let scale = self.scale_factor;

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl
                .clear_color(bottom_color[0], bottom_color[1], bottom_color[2], 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.push_vertical_gradient(
            -1.0,
            -1.0,
            self.width + 2.0,
            self.height + 2.0,
            top_color,
            bottom_color,
        );
        self.flush();

        let content_x = 40.0 * scale;
        let content_y = 40.0 * scale;
        let content_w = self.width - 80.0 * scale;
        let content_h = self.height - 80.0 * scale;

        let card_bg = [0.169, 0.176, 0.188, 0.95];
        let card_border = [0.224, 0.231, 0.251, 1.0];

        self.push_rounded_rect(
            content_x - 1.0,
            content_y - 1.0,
            content_w + 2.0,
            content_h + 2.0,
            10.0 * scale,
            card_border,
        );
        self.push_rounded_rect(
            content_x,
            content_y,
            content_w,
            content_h,
            10.0 * scale,
            card_bg,
        );

        let title_x = content_x + 40.0 * scale;
        let mut y = content_y + 60.0 * scale;

        if let Some(tex) = self.icon_logo {
            let icon_y = y - 40.0 * scale;
            self.draw_icon(&tex, title_x, icon_y, 110.0 * scale, 110.0 * scale);
        }

        self.draw_string_scaled(
            "Добро пожаловать в RRiter",
            title_x + 130.0 * scale,
            y,
            [0.741, 0.576, 0.976, 1.0],
            1.0,
        );
        y += 40.0 * scale;
        self.draw_string_scaled(
            "Молниеносный текстовый редактор с GPU-рендерингом",
            title_x + 130.0 * scale,
            y,
            [0.7, 0.7, 0.75, 1.0],
            1.0,
        );

        y += 60.0 * scale;
        let (btn_new, btn_open, btn_ide) =
            crate::widgets::get_welcome_buttons(content_w, title_x, y, scale, self);

        let mut wants_pointer = false;
        wants_pointer |= btn_new.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);
        wants_pointer |= btn_open.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);
        wants_pointer |= btn_ide.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);

        y += 80.0 * scale;
        self.draw_string_scaled(
            "Недавние файлы",
            title_x,
            y,
            [0.741, 0.576, 0.976, 1.0],
            1.0,
        );

        let line_y = y + 20.0 * scale;
        self.push_rect(
            title_x,
            line_y,
            content_w - 80.0 * scale,
            1.0,
            [1.0, 1.0, 1.0, 0.08],
        );

        y += 35.0 * scale;

        let item_h = 44.0 * scale;
        for path in recent_files {
            if y + item_h > content_y + content_h - 60.0 * scale {
                break;
            }

            let is_hovered = self.last_mouse_x >= title_x - 10.0 * scale
                && self.last_mouse_x <= title_x + content_w - 70.0 * scale
                && self.last_mouse_y >= y
                && self.last_mouse_y < y + item_h;

            if is_hovered {
                wants_pointer = true;
                self.push_rounded_rect(
                    title_x - 10.0 * scale,
                    y,
                    content_w - 60.0 * scale,
                    item_h,
                    6.0 * scale,
                    [1.0, 1.0, 1.0, 0.05],
                );
            }

            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let full_dir = path
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy();

            self.draw_string_scaled(&name, title_x, y + 25.0 * scale, [0.9, 0.9, 0.9, 1.0], 1.0);
            let name_w = self.measure_ui_width(&name, 1.0);
            self.draw_string_scaled(
                &full_dir,
                title_x + name_w + 15.0 * scale,
                y + 25.0 * scale,
                [0.5, 0.5, 0.5, 1.0],
                0.95,
            );

            self.push_rect(
                title_x,
                y + item_h - 1.0,
                content_w - 80.0 * scale,
                1.0,
                [1.0, 1.0, 1.0, 0.04],
            );

            y += item_h;
        }

        let hint_str_1 = "F1";
        let hint_str_2 = " — Настройки редактора";
        let scale_hint = 0.9;

        let w1 = self.measure_ui_width(hint_str_1, scale_hint) + 16.0 * scale;
        let w2 = self.measure_ui_width(hint_str_2, scale_hint);
        let hint_total_w = w1 + w2;

        let hint_x = content_x + content_w - hint_total_w - 30.0 * scale;
        let hint_y = content_y + content_h - 30.0 * scale;

        let kbd_bg = [0.224, 0.231, 0.251, 1.0];
        let kbd_border = [0.306, 0.318, 0.341, 1.0];
        let kbd_text_color = [0.875, 0.882, 0.902, 1.0];

        let kbd_h = 22.0 * scale;
        let kbd_draw_y = hint_y - 16.0 * scale;

        self.push_rounded_rect(
            hint_x - 1.0,
            kbd_draw_y - 1.0,
            w1 + 2.0,
            kbd_h + 2.0,
            4.0 * scale,
            kbd_border,
        );
        self.push_rounded_rect(hint_x, kbd_draw_y, w1, kbd_h, 4.0 * scale, kbd_bg);

        self.draw_string_scaled(
            hint_str_1,
            hint_x + 8.0 * scale,
            hint_y,
            kbd_text_color,
            scale_hint,
        );

        self.draw_string_scaled(
            hint_str_2,
            hint_x + w1,
            hint_y,
            [0.5, 0.5, 0.55, 1.0],
            scale_hint,
        );

        self.flush();
        wants_pointer
    }

    pub fn draw_settings(
        &mut self,
        anim_progress: f32,
        active_tab: usize,
        faq_editor: &Editor,
        scroll_y: f32,
        ide_workspaces: &[std::path::PathBuf],
        ide_ignore_patterns: &[String],
        settings_ignore_editor: &Editor,
        settings_ignore_focused: bool,
        settings_ignore_scroll_x: &mut f32,
        ide_scroll_y: f32,
        blink_alpha: f32,
    ) -> u8 {
        if anim_progress <= 0.0 {
            return 0;
        }
        let s = self.scale_factor;
        let mut wants_pointer = false;
        let mut wants_text = false;

        let overlay_alpha = ((anim_progress - 0.04) * (0.4 / 0.96)).max(0.0);
        self.push_rect(
            0.0,
            0.0,
            self.width,
            self.height,
            [0.0, 0.0, 0.0, overlay_alpha],
        );

        let w = (1000.0 * s).min(self.width - 40.0 * s);
        let h = (700.0 * s).min(self.height - 40.0 * s);

        let start_y = self.height + 100.0 * s;
        let target_y = (self.height - h) / 2.0;
        let raw_y = start_y + (target_y - start_y) * anim_progress;
        let y = raw_y.round();
        let x = ((self.width - w) / 2.0).round();

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        // 1. Внешнее окно с градиентом
        self.push_rounded_rect(
            x - 1.0,
            y - 1.0,
            w + 2.0,
            h + 2.0,
            10.0 * s,
            [0.224, 0.231, 0.251, 1.0],
        );
        self.push_rounded_rect_gradient(x, y, w, h, 10.0 * s, top_color, bottom_color);

        // 2. Внутренняя панель
        let pad_top = 35.0 * s;
        let pad_bottom = 30.0 * s;
        let pad_h = 40.0 * s;
        let ix = x + pad_h;
        let iy = y + pad_top;
        let iw = w - pad_h * 2.0;
        let ih = h - pad_top - pad_bottom;

        self.push_rounded_rect(
            ix - 1.0,
            iy - 1.0,
            iw + 2.0,
            ih + 2.0,
            8.0 * s,
            [0.224, 0.231, 0.251, 0.8],
        );
        self.push_rounded_rect(ix, iy, iw, ih, 8.0 * s, [0.15, 0.16, 0.20, 1.0]);

        self.flush();

        let sidebar_w = 200.0 * s;
        self.push_rect(ix + sidebar_w, iy, 1.0, ih, [1.0, 1.0, 1.0, 0.05]);

        let tabs = ["IDE", "Основные", "Редактор", "Внешний вид", "Помощь"];
        let mut tab_y = iy + 20.0 * s;
        for (i, title) in tabs.iter().enumerate() {
            let tab_rect_y = tab_y;
            let tab_rect_h = 36.0 * s;

            let is_hovered = self.last_mouse_x >= ix + 10.0 * s
                && self.last_mouse_x <= ix + sidebar_w - 10.0 * s
                && self.last_mouse_y >= tab_rect_y
                && self.last_mouse_y <= tab_rect_y + tab_rect_h;

            if is_hovered {
                wants_pointer = true;
            }

            if i == active_tab {
                self.push_rounded_rect(
                    ix + 10.0 * s,
                    tab_rect_y,
                    sidebar_w - 20.0 * s,
                    tab_rect_h,
                    6.0 * s,
                    [1.0, 1.0, 1.0, 0.1],
                );
            } else if is_hovered {
                self.push_rounded_rect(
                    ix + 10.0 * s,
                    tab_rect_y,
                    sidebar_w - 20.0 * s,
                    tab_rect_h,
                    6.0 * s,
                    [1.0, 1.0, 1.0, 0.05],
                );
            }

            let color = if i == active_tab {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.7, 0.7, 0.7, 1.0]
            };
            self.draw_string_scaled(title, ix + 25.0 * s, tab_y + 24.0 * s, color, 0.95);
            tab_y += tab_rect_h + 4.0 * s;
        }

        let content_x = ix + sidebar_w + 30.0 * s;
        let content_title_x = content_x - 14.0 * s;
        let mut content_y = iy + 40.0 * s;

        let tab_title = tabs[active_tab];
        let pill_w = self.measure_ui_width(tab_title, 1.1) + 28.0 * s;
        let pill_h = 30.0 * s;
        let pill_y = content_y - 22.0 * s;
        self.push_rounded_rect(
            content_title_x - 1.0,
            pill_y - 1.0,
            pill_w + 2.0,
            pill_h + 2.0,
            6.0 * s,
            [0.35, 0.26, 0.48, 1.0],
        );
        self.push_rounded_rect(
            content_title_x,
            pill_y,
            pill_w,
            pill_h,
            6.0 * s,
            [0.26, 0.20, 0.36, 1.0],
        );
        self.draw_string_scaled(
            tab_title,
            content_title_x + 14.0 * s,
            content_y,
            [1.0, 1.0, 1.0, 1.0],
            1.1,
        );
        content_y += if active_tab == 4 { 30.0 * s } else { 46.0 * s };

        if active_tab == 0 {
            // ── Scissor для скролла вкладки IDE ──────────────────────────────
            // Начало scissor = iy + 52.0 * s (ниже пилюли заголовка iy+18..iy+48)
            let ide_content_area_x = ix + sidebar_w;
            let ide_content_area_w = iw - sidebar_w;
            let ide_content_area_h = ih - 52.0 * s;
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (iy + 52.0 * s + ide_content_area_h);
                self.gl.scissor(
                    ide_content_area_x.round() as i32,
                    scissor_y.round() as i32,
                    ide_content_area_w.round() as i32,
                    ide_content_area_h.round() as i32,
                );
            }

            content_y -= ide_scroll_y.round();

            self.draw_string_scaled(
                "Рабочие области",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 40.0 * s;

            for path in ide_workspaces {
                let path_str = path.to_string_lossy();
                let item_w = 460.0 * s;
                let item_h = 36.0 * s;

                self.push_rounded_rect(
                    content_x - 1.0,
                    content_y - 1.0,
                    item_w + 2.0,
                    item_h + 2.0,
                    6.0 * s,
                    [0.306, 0.318, 0.341, 1.0],
                );
                self.push_rounded_rect(
                    content_x,
                    content_y,
                    item_w,
                    item_h,
                    6.0 * s,
                    [0.224, 0.231, 0.251, 1.0],
                );

                self.draw_string_scaled(
                    &path_str,
                    content_x + 14.0 * s,
                    (content_y + 24.0 * s).round(),
                    [0.9, 0.9, 0.9, 1.0],
                    0.95,
                );

                let btn_del = crate::widgets::IconButton {
                    x: content_x + item_w - 34.0 * s,
                    y: content_y + 3.0 * s,
                    size: 30.0 * s,
                    icon: Some(crate::widgets::IconType::Discard),
                    is_active: false,
                    icon_size: Some(18.0 * s),
                    active_square_width: None,
                };
                wants_pointer |=
                    btn_del.render(self, self.last_mouse_x, self.last_mouse_y, s, false);

                content_y += 46.0 * s;
            }

            let btn_add = crate::widgets::Button {
                x: content_x,
                y: content_y.round(),
                w: 190.0 * s,
                h: 36.0 * s,
                text: "Добавить папку".to_string(),
                icon: Some(crate::widgets::IconType::Plus),
                text_scale: 1.0,
                icon_size: 20.0 * s,
            };
            wants_pointer |= btn_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            content_y += 56.0 * s;

            // ── Разделитель ───────────────────────────────────────────────
            self.push_rect(content_x, content_y, 460.0 * s, 1.0, [1.0, 1.0, 1.0, 0.07]);
            content_y += 20.0 * s;

            // ── Заголовок секции игноров ──────────────────────────────────
            self.draw_string_scaled(
                "Игнорируемые файлы и папки",
                content_x,
                content_y.round(),
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 28.0 * s;

            // Пояснение
            self.draw_string_scaled(
                "Эти файлы и папки не будут показаны в дереве проекта.",
                content_x,
                content_y.round(),
                [0.45, 0.47, 0.55, 1.0],
                0.85,
            );
            content_y += 22.0 * s;
            self.draw_string_scaled(
                "Примеры: *.log  temp/  .DS_Store  *.min.js  build  dist",
                content_x,
                content_y.round(),
                [0.35, 0.37, 0.44, 1.0],
                0.82,
            );
            content_y += 20.0 * s;

            // ── Поле ввода + кнопка «Добавить» ───────────────────────────
            let input_w = 330.0 * s;
            let input_h = 34.0 * s;
            let text_scale_input = 0.95f32; // Округленный скейл для ровного бейзлайна

            let input_hovered = self.last_mouse_x >= content_x
                && self.last_mouse_x <= content_x + input_w
                && self.last_mouse_y >= content_y
                && self.last_mouse_y <= content_y + input_h;
            if input_hovered {
                wants_text = true;
            }

            let border_col = if settings_ignore_focused {
                [0.55, 0.35, 0.80, 1.0]
            } else if input_hovered {
                [0.40, 0.28, 0.60, 1.0]
            } else {
                [0.28, 0.29, 0.35, 1.0]
            };
            self.push_rounded_rect(
                content_x - 1.0,
                content_y - 1.0,
                input_w + 2.0,
                input_h + 2.0,
                6.0 * s,
                border_col,
            );
            self.push_rounded_rect(
                content_x,
                content_y,
                input_w,
                input_h,
                6.0 * s,
                [0.11, 0.12, 0.16, 1.0],
            );

            let text_y_mid = (content_y + input_h * 0.70).round();
            let start_x = (content_x + 8.0 * s).round();
            let full_text = settings_ignore_editor.get_full_text();

            if full_text.is_empty() {
                self.draw_string_scaled(
                    "Паттерн или имя файла...",
                    start_x,
                    text_y_mid,
                    [0.30, 0.32, 0.40, 1.0],
                    text_scale_input,
                );
                if settings_ignore_focused && blink_alpha > 0.5 {
                    self.push_rect(
                        start_x,
                        (content_y + 6.0 * s).round(),
                        (1.5 * s).max(1.0),
                        input_h - 12.0 * s,
                        [0.75, 0.45, 1.0, 1.0],
                    );
                }
            } else {
                let mut cursor_total_x = 0.0;
                let mut total_text_width = 0.0;
                for (byte_idx, c) in full_text.char_indices() {
                    let adv =
                        self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0) * text_scale_input;
                    if byte_idx < settings_ignore_editor.cursor {
                        cursor_total_x += adv;
                    }
                    total_text_width += adv;
                }

                let max_text_w_exact = input_w - 16.0 * s;
                if cursor_total_x - *settings_ignore_scroll_x > max_text_w_exact {
                    *settings_ignore_scroll_x = cursor_total_x - max_text_w_exact;
                }
                if cursor_total_x - *settings_ignore_scroll_x < 0.0 {
                    *settings_ignore_scroll_x = cursor_total_x;
                }
                *settings_ignore_scroll_x = (*settings_ignore_scroll_x)
                    .min((total_text_width - max_text_w_exact).max(0.0))
                    .max(0.0);

                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    let scissor_y = self.height - (content_y + input_h);
                    self.gl.scissor(
                        content_x.round() as i32,
                        scissor_y.round() as i32,
                        input_w.round() as i32,
                        input_h.round() as i32,
                    );
                }

                let sel_start = settings_ignore_editor
                    .selection_anchor
                    .unwrap_or(settings_ignore_editor.cursor)
                    .min(settings_ignore_editor.cursor);
                let sel_end = settings_ignore_editor
                    .selection_anchor
                    .unwrap_or(settings_ignore_editor.cursor)
                    .max(settings_ignore_editor.cursor);

                let mut current_x = start_x - *settings_ignore_scroll_x;
                let mut byte_idx = 0;
                let mut cursor_draw_x = current_x;

                for c in full_text.chars() {
                    if byte_idx == settings_ignore_editor.cursor {
                        cursor_draw_x = current_x;
                    }
                    let adv =
                        self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0) * text_scale_input;

                    if byte_idx >= sel_start && byte_idx < sel_end {
                        self.push_rect(
                            current_x,
                            (content_y + 4.0 * s).round(),
                            adv,
                            input_h - 8.0 * s,
                            [0.55, 0.35, 0.80, 0.50],
                        );
                    }

                    if let Some(g) = self.get_ui_glyph(c) {
                        self.push_quad(
                            current_x + g.offset_x * text_scale_input,
                            text_y_mid - g.offset_y * text_scale_input,
                            g.width * text_scale_input,
                            g.height * text_scale_input,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            [0.90, 0.88, 0.95, 1.0],
                            g.is_emoji,
                        );
                    }
                    current_x += adv;
                    byte_idx += c.len_utf8();
                }
                if byte_idx == settings_ignore_editor.cursor {
                    cursor_draw_x = current_x;
                }

                if settings_ignore_focused && sel_start == sel_end && blink_alpha > 0.5 {
                    self.push_rect(
                        cursor_draw_x,
                        (content_y + 6.0 * s).round(),
                        (1.5 * s).max(1.0),
                        input_h - 12.0 * s,
                        [0.75, 0.45, 1.0, 1.0],
                    );
                }

                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }
            }

            // Кнопка «Добавить» — неактивна если поле пустое или только пробелы
            let trimmed_input = full_text.trim();
            let btn_add_x = content_x + input_w + 10.0 * s;
            let btn_add_y = content_y;
            let btn_add_w = 110.0 * s;
            if trimmed_input.is_empty() {
                // Строго копируем математику округления из widgets.rs (Button::render)
                let bx = btn_add_x.round();
                let by = btn_add_y.round();
                let bw = btn_add_w.round();
                let bh = input_h.round();

                self.push_rounded_rect(
                    bx - 1.0,
                    by - 1.0,
                    bw + 2.0,
                    bh + 2.0,
                    6.0 * s,
                    [0.20, 0.21, 0.26, 1.0],
                );
                self.push_rounded_rect(bx, by, bw, bh, 6.0 * s, [0.15, 0.16, 0.20, 1.0]);

                let icon_sz = 15.0 * s;
                let text_scale = 0.88;
                let text_w = self.measure_ui_width("Добавить", text_scale);
                let content_w = text_w + icon_sz + 8.0 * s;

                let mut content_x = bx + (bw - content_w) / 2.0;
                let icon_y = by + (bh - icon_sz) / 2.0;
                let text_y = by + bh / 2.0 + 5.0 * s;

                self.draw_atlas_icon(
                    crate::widgets::IconType::Plus,
                    content_x,
                    icon_y,
                    icon_sz,
                    [0.35, 0.36, 0.42, 1.0],
                );
                content_x += icon_sz + 8.0 * s;

                self.draw_string_scaled(
                    "Добавить",
                    content_x,
                    text_y,
                    [0.35, 0.36, 0.42, 1.0],
                    text_scale,
                );
            } else {
                let btn_ignore_add = crate::widgets::Button {
                    x: btn_add_x,
                    y: btn_add_y,
                    w: btn_add_w,
                    h: input_h,
                    text: "Добавить".to_string(),
                    icon: Some(crate::widgets::IconType::Plus),
                    text_scale: 0.88,
                    icon_size: 15.0 * s,
                };
                wants_pointer |=
                    btn_ignore_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            }
            content_y += input_h + 16.0 * s;

            // ── Чипы пользовательских паттернов ──────────────────────────
            let chip_h = 28.0 * s;
            let chip_r = chip_h / 2.0;
            let pad_x = 12.0 * s;
            let chip_gap_x = 8.0 * s;
            let chip_gap_y = 8.0 * s;
            let max_row_w = 460.0 * s;
            let mut chip_x = content_x;

            for pattern in ide_ignore_patterns.iter() {
                let text_w = self.measure_ui_width(pattern, 0.88);
                let close_area = 22.0 * s;
                let chip_w = text_w + pad_x * 2.0 + close_area;

                if chip_x + chip_w > content_x + max_row_w && chip_x > content_x {
                    chip_x = content_x;
                    content_y += chip_h + chip_gap_y;
                }

                let chip_hov = self.last_mouse_x >= chip_x
                    && self.last_mouse_x <= chip_x + chip_w
                    && self.last_mouse_y >= content_y
                    && self.last_mouse_y <= content_y + chip_h;

                let close_hov = self.last_mouse_x >= chip_x + chip_w - close_area - 2.0 * s
                    && self.last_mouse_x <= chip_x + chip_w
                    && self.last_mouse_y >= content_y
                    && self.last_mouse_y <= content_y + chip_h;

                if chip_hov {
                    wants_pointer = true;
                }

                let bg = if chip_hov {
                    [0.30, 0.18, 0.44, 1.0]
                } else {
                    [0.20, 0.13, 0.30, 1.0]
                };
                let border = if chip_hov {
                    [0.58, 0.34, 0.82, 1.0]
                } else {
                    [0.35, 0.22, 0.52, 1.0]
                };

                self.push_rounded_rect(
                    chip_x - 1.0,
                    content_y - 1.0,
                    chip_w + 2.0,
                    chip_h + 2.0,
                    chip_r + 1.0,
                    border,
                );
                self.push_rounded_rect(chip_x, content_y, chip_w, chip_h, chip_r, bg);

                self.draw_string_scaled(
                    pattern,
                    chip_x + pad_x,
                    (content_y + chip_h * 0.70).round(),
                    [0.82, 0.68, 1.0, 1.0],
                    0.88,
                );

                let cross_color = if close_hov {
                    [1.0, 0.38, 0.58, 1.0]
                } else {
                    [0.50, 0.40, 0.65, 1.0]
                };
                self.draw_string_scaled(
                    "×",
                    chip_x + chip_w - close_area + 1.0 * s,
                    (content_y + chip_h * 0.70).round(),
                    cross_color,
                    0.95,
                );

                chip_x += chip_w + chip_gap_x;
            }

            if ide_ignore_patterns.is_empty() {
                self.draw_string_scaled(
                    "Нет пользовательских правил",
                    content_x,
                    (content_y + chip_h * 0.70).round(),
                    [0.28, 0.30, 0.36, 1.0],
                    0.88,
                );
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }

            // ── Скроллбар для вкладки IDE ─────────────────────────────────
            let ide_total_h = {
                let workspace_h = ide_workspaces.len() as f32 * 46.0 * s + 126.0 * s;
                let ignore_h = {
                    let chip_rows = if ide_ignore_patterns.is_empty() {
                        1
                    } else {
                        let mut rows = 1usize;
                        let mut cx2 = 0.0f32;
                        for p in ide_ignore_patterns.iter() {
                            let tw = self.measure_ui_width(p, 0.88);
                            let cw2 = tw + pad_x * 2.0 + 22.0 * s;
                            if cx2 + cw2 > max_row_w && cx2 > 0.0 {
                                rows += 1;
                                cx2 = 0.0;
                            }
                            cx2 += cw2 + chip_gap_x;
                        }
                        rows
                    };
                    // Убрана плашка «Скрыты всегда» (-dlabel_h - 18.0 * s)
                    160.0 * s + chip_rows as f32 * (chip_h + chip_gap_y)
                };
                workspace_h + ignore_h
            };
            let max_scroll = (ide_total_h - ide_content_area_h).max(0.0);
            if max_scroll > 0.0 {
                let ratio = (ide_scroll_y / max_scroll).clamp(0.0, 1.0);
                let track_h = ide_content_area_h;
                let thumb_h = (ide_content_area_h / ide_total_h * track_h).max(40.0 * s);
                let thumb_y = (iy + 52.0 * s + ratio * (track_h - thumb_h)).round();
                let sb_x = (ix + iw - 14.0 * s).round();
                self.push_rounded_rect(
                    sb_x,
                    thumb_y,
                    6.0 * s,
                    thumb_h,
                    3.0 * s,
                    [0.7, 0.33, 0.54, 1.0],
                );
            }
        } else if active_tab == 1 {
            self.draw_string_scaled(
                "Скоро здесь появятся настройки...",
                content_x,
                content_y,
                [0.6, 0.6, 0.6, 1.0],
                1.0,
            );
        } else if active_tab == 2 {
            self.draw_string_scaled(
                "Размер шрифта: 14px",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 30.0 * s;
            self.draw_string_scaled(
                "Межстрочный интервал: 1.5",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
        } else if active_tab == 3 {
            self.draw_string_scaled(
                "Тема: Dracula (По умолчанию)",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
        } else if active_tab == 4 {
            self.flush();
            let text_area_y = content_y;
            let text_area_h = ih - (text_area_y - iy) - 20.0 * s;

            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (text_area_y + text_area_h);
                self.gl.scissor(
                    (content_x - 10.0 * s).round() as i32,
                    scissor_y.round() as i32,
                    (iw - sidebar_w - 10.0 * s).round() as i32,
                    text_area_h.round() as i32,
                );
            }

            let start_x = content_x;
            let main_header_x = content_x - 14.0 * s;
            let render_scroll_y = scroll_y.round();
            let mut text_y = text_area_y + 20.0 * s - render_scroll_y;
            let text = faq_editor.get_full_text();

            let left_col_w = 260.0 * s;
            let cw = iw - sidebar_w - 76.0 * s;
            let mut main_header_drawn = false;

            for line in text.split('\n') {
                let is_header = line.starts_with("# ");

                if is_header {
                    let header_text = &line[2..];
                    let is_main = !main_header_drawn && header_text == tab_title;

                    if is_main {
                        let pill_w = self.measure_ui_width(header_text, 1.05) + 24.0 * s;
                        let pill_h = 26.0 * s;
                        let pill_y = text_y - 19.0 * s;

                        self.push_rounded_rect(
                            main_header_x - 1.0,
                            pill_y - 1.0,
                            pill_w + 2.0,
                            pill_h + 2.0,
                            5.0 * s,
                            [0.35, 0.26, 0.48, 1.0],
                        );
                        self.push_rounded_rect(
                            main_header_x,
                            pill_y,
                            pill_w,
                            pill_h,
                            5.0 * s,
                            [0.26, 0.20, 0.36, 1.0],
                        );
                        self.draw_string_scaled(
                            header_text,
                            main_header_x + 12.0 * s,
                            text_y,
                            [1.0, 1.0, 1.0, 1.0],
                            1.05,
                        );
                        main_header_drawn = true;
                    } else {
                        let sep_y = text_y + 10.0 * s;
                        let sep_x = main_header_x;
                        let sep_w = (cw - 10.0 * s).max(0.0);
                        self.draw_string_scaled(
                            header_text,
                            start_x,
                            text_y,
                            [0.875, 0.882, 0.902, 1.0],
                            1.05,
                        );
                        self.push_rect(sep_x, sep_y, sep_w, 1.0, [1.0, 1.0, 1.0, 0.10]);
                    }

                    text_y += 50.0 * s;
                    continue;
                }

                if let Some(tab_idx) = line.find('\t') {
                    let shortcut = &line[..tab_idx];
                    let description = &line[tab_idx + 1..];

                    let kbd_bg = [0.224, 0.231, 0.251, 1.0];
                    let kbd_border = [0.306, 0.318, 0.341, 1.0];
                    let kbd_text_color = [0.875, 0.882, 0.902, 1.0];

                    let kbd_w = self.measure_ui_width(shortcut, 0.95) + 20.0 * s;
                    let kbd_h = 24.0 * s;
                    let kbd_x = start_x;
                    let kbd_y = text_y - 18.0 * s;

                    self.push_rounded_rect(
                        kbd_x - 1.0,
                        kbd_y - 1.0,
                        kbd_w + 2.0,
                        kbd_h + 2.0,
                        4.0 * s,
                        kbd_border,
                    );
                    self.push_rounded_rect(kbd_x, kbd_y, kbd_w, kbd_h, 4.0 * s, kbd_bg);
                    self.draw_string_scaled(
                        shortcut,
                        kbd_x + 10.0 * s,
                        text_y - 1.0 * s,
                        kbd_text_color,
                        0.95,
                    );

                    let desc_color = [0.663, 0.690, 0.729, 1.0];
                    self.draw_string_scaled(
                        description,
                        start_x + left_col_w,
                        text_y,
                        desc_color,
                        1.0,
                    );

                    text_y += 38.0 * s;
                    continue;
                }

                if !line.trim().is_empty() {
                    let normal_color = [0.875, 0.882, 0.902, 1.0];
                    self.draw_string_scaled(line.trim(), start_x, text_y, normal_color, 1.0);
                    text_y += 30.0 * s;
                } else {
                    text_y += 15.0 * s;
                }
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }

            let max_scroll = self.get_faq_max_scroll(faq_editor, h);
            let total_content_h = text_area_h + max_scroll;

            if max_scroll > 0.0 {
                let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
                let track_h = text_area_h;
                let thumb_h = (text_area_h / total_content_h * track_h).max(40.0 * s);
                let thumb_y = (text_area_y + scroll_ratio * (track_h - thumb_h)).round();
                let scroll_x = (start_x + cw + 5.0 * s).round();

                self.push_rounded_rect(
                    scroll_x,
                    thumb_y,
                    6.0 * s,
                    thumb_h,
                    3.0 * s,
                    [0.7, 0.33, 0.54, 1.0],
                );
            }
        }

        self.flush();
        if wants_text {
            2
        } else if wants_pointer {
            1
        } else {
            0
        }
    }
}
