#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_inline_git_text_line(
        &mut self,
        text: &str,
        spans: &[ColorSpan],
        base_offset: Option<usize>,
        x: f32,
        y: f32,
        max_x: f32,
        scale: f32,
    ) {
        let mut draw_x = x;
        let mut current_offset = base_offset.unwrap_or(usize::MAX);
        let mut span_idx = base_offset
            .map(
                |offset| match spans.binary_search_by_key(&offset, |s| s.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                },
            )
            .unwrap_or(0);

        for c in text.chars() {
            if draw_x > max_x {
                break;
            }
            if c == '\r' || c == '\n' {
                break;
            }
            let char_len = c.len_utf8();
            let adv = self.char_advance(c);
            if c != ' '
                && c != '\t'
                && let Some(g) = self.get_glyph(c)
            {
                let mut color = self.theme.fg;
                if base_offset.is_some() {
                    while span_idx < spans.len() && spans[span_idx].end <= current_offset {
                        span_idx += 1;
                    }
                    if span_idx < spans.len() && spans[span_idx].start <= current_offset {
                        color = spans[span_idx].color;
                    }
                }
                self.push_quad(
                    draw_x + g.offset_x,
                    y - g.offset_y,
                    g.width,
                    g.height,
                    g.u,
                    g.v,
                    g.uw,
                    g.vh,
                    color,
                    g.is_emoji,
                );
                if c == '.' || c == ':' {
                    self.push_quad(
                        draw_x + g.offset_x + 1.0 * scale,
                        y - g.offset_y,
                        g.width,
                        g.height,
                        g.u,
                        g.v,
                        g.uw,
                        g.vh,
                        color,
                        g.is_emoji,
                    );
                }
            }
            draw_x += adv;
            current_offset = current_offset.saturating_add(char_len);
        }
    }

    fn draw_inline_git_popup_panel(
        &mut self,
        editor: &Editor,
        inline_git_popup: Option<&crate::app::InlineGitPopup>,
        active_git_diff_present: bool,
        show_welcome: bool,
        render_scroll_x: f32,
        render_scroll_y: f32,
        editor_height: f32,
        tab_bar_h: f32,
        scrollbar_x: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        if active_git_diff_present || show_welcome {
            return;
        }
        let Some(popup) = inline_git_popup else {
            return;
        };
        let Some(line_y) = self
            .visual_lines
            .iter()
            .find(|line| line.physical_line == popup.anchor_line.saturating_add(1))
            .map(|line| line.y_offset - render_scroll_y)
        else {
            return;
        };

        let max_rows = ((editor_height - 64.0 * s) / self.line_height.max(1.0))
            .floor()
            .max(4.0)
            .min(24.0) as usize;
        let visible_rows = popup.lines.len().min(max_rows);
        let truncated = popup.lines.len() > visible_rows;
        let diff_rows = visible_rows + usize::from(truncated);
        let row_h = self.line_height;
        let text_x = self.left_padding - render_scroll_x;
        let panel_x = text_x - 8.0 * s;
        let panel_w = (scrollbar_x - panel_x - 18.0 * s)
            .max(360.0 * s)
            .min(920.0 * s);
        let toolbar_h = 42.0 * s;
        let panel_h = toolbar_h + diff_rows.max(1) as f32 * row_h;
        let top_limit = tab_bar_h + 8.0 * s;
        let bottom_limit = tab_bar_h + editor_height - 8.0 * s;
        let gap = 4.0 * s;
        let panel_y = if line_y + panel_h <= bottom_limit {
            line_y
        } else if line_y - panel_h - gap >= top_limit {
            line_y + self.line_height - panel_h
        } else {
            line_y
                .max(top_limit)
                .min((bottom_limit - panel_h).max(top_limit))
        };
        ui_registry.register_blocker(
            crate::ui_system::UiId::InlineGitPanelBody,
            panel_x - 2.0 * s,
            panel_y - 2.0 * s,
            panel_w + 4.0 * s,
            panel_h + 4.0 * s,
            mx,
            my,
        );
        self.push_rounded_rect(
            panel_x - 3.0 * s,
            panel_y - 3.0 * s,
            panel_w + 6.0 * s,
            panel_h + 6.0 * s,
            8.0 * s,
            [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 1.0],
        );
        self.push_rounded_rect(
            panel_x,
            panel_y,
            panel_w,
            panel_h,
            7.0 * s,
            [
                self.theme.minimap_bg[0],
                self.theme.minimap_bg[1],
                self.theme.minimap_bg[2],
                1.0,
            ],
        );

        let btn_y = panel_y + 5.0 * s;
        let icon_size = 32.0 * s;
        let hunk_total = editor.git_hunks.len().max(1);
        let controls_enabled = hunk_total > 1;
        let mut current_buf = [0u8; 20];
        let mut total_buf = [0u8; 20];
        let current_s = decimal_usize_buf(&mut current_buf, popup.hunk_idx + 1);
        let total_s = decimal_usize_buf(&mut total_buf, hunk_total);
        let count_x = panel_x + 12.0 * s;
        self.draw_string_scaled(current_s, count_x, btn_y + 22.0 * s, self.theme.fg, 0.88);
        let slash_x = count_x + self.measure_ui_width(current_s, 0.88);
        self.draw_string_scaled("/", slash_x, btn_y + 22.0 * s, self.theme.line_num, 0.88);
        self.draw_string_scaled(
            total_s,
            slash_x + 7.0 * s,
            btn_y + 22.0 * s,
            self.theme.fg,
            0.88,
        );
        let nav_x = panel_x + 62.0 * s;
        let disabled_col = [
            self.theme.line_num[0],
            self.theme.line_num[1],
            self.theme.line_num[2],
            0.42,
        ];
        let up_btn = crate::widgets::IconButton {
            x: nav_x,
            y: btn_y,
            size: icon_size,
            icon: Some(crate::widgets::IconType::Up),
            is_active: false,
            icon_size: Some(30.0 * s),
            active_square_width: None,
            custom_color: (!controls_enabled).then_some(disabled_col),
        };
        let down_btn = crate::widgets::IconButton {
            x: nav_x + 34.0 * s,
            y: btn_y,
            size: icon_size,
            icon: Some(crate::widgets::IconType::Down),
            is_active: false,
            icon_size: Some(30.0 * s),
            active_square_width: None,
            custom_color: (!controls_enabled).then_some(disabled_col),
        };
        let rollback_btn = crate::widgets::IconButton {
            x: nav_x + 76.0 * s,
            y: btn_y,
            size: icon_size,
            icon: Some(crate::widgets::IconType::Rollback),
            is_active: false,
            icon_size: Some(22.0 * s),
            active_square_width: None,
            custom_color: None,
        };

        let max_text_x = panel_x + panel_w - 10.0 * s;
        let mut row_y = panel_y + toolbar_h;
        for (row_idx, line) in popup.lines.iter().take(visible_rows).enumerate() {
            let color = match line.kind {
                crate::app::git_diff::DiffLineKind::Added
                | crate::app::git_diff::DiffLineKind::ModifiedNew => {
                    Some([0.18, 0.82, 0.34, 0.26])
                }
                crate::app::git_diff::DiffLineKind::Deleted
                | crate::app::git_diff::DiffLineKind::ModifiedOld => {
                    Some([0.76, 0.78, 0.84, 0.24])
                }
                crate::app::git_diff::DiffLineKind::Context => None,
            };
            if let Some(color) = color {
                let prev_kind = row_idx
                    .checked_sub(1)
                    .and_then(|idx| popup.lines.get(idx))
                    .map(|prev| prev.kind);
                if !truncated && row_idx + 1 == visible_rows && prev_kind != Some(line.kind) {
                    self.push_rounded_rect(panel_x, row_y, panel_w, row_h, 7.0 * s, color);
                } else {
                    self.push_rect(panel_x, row_y, panel_w, row_h, color);
                }
            }
            self.draw_inline_git_text_line(
                &line.text,
                &popup.spans,
                Some(line.display_start),
                text_x,
                row_y + self.baseline_offset,
                max_text_x,
                s,
            );
            row_y += row_h;
        }
        if truncated {
            let color = [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.08];
            self.push_rounded_rect(panel_x, row_y, panel_w, row_h, 7.0 * s, color);
            self.draw_inline_git_text_line(
                "...",
                &popup.spans,
                None,
                text_x,
                row_y + self.baseline_offset,
                max_text_x,
                s,
            );
        }

        if controls_enabled {
            ui_registry.register_icon_button(
                crate::ui_system::UiId::InlineGitPrevHunk,
                &up_btn,
                self,
                mx,
                my,
                s,
                false,
            );
            ui_registry.register_icon_button(
                crate::ui_system::UiId::InlineGitNextHunk,
                &down_btn,
                self,
                mx,
                my,
                s,
                false,
            );
        } else {
            up_btn.render(self, mx, my, s, false);
            down_btn.render(self, mx, my, s, false);
        }
        ui_registry.register_icon_button(
            crate::ui_system::UiId::InlineGitRollbackHunk,
            &rollback_btn,
            self,
            mx,
            my,
            s,
            false,
        );
    }

    fn draw_git_diff_hunk_panel(
        &mut self,
        state: Option<&crate::app::git_diff::GitDiffState>,
        show_welcome: bool,
        minimap_w: f32,
        scrollbar_width: f32,
        gutter_x: f32,
        tab_bar_h: f32,
        render_scroll_y: f32,
        editor_scroll_height: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let Some(state) = state else {
            return;
        };
        if state.hunks.is_empty() || show_welcome {
            return;
        }

        let search_w = 480.0 * s;
        let search_h = 52.0 * s;
        let scrollbar_x = self.width - minimap_w - scrollbar_width;
        let search_x = scrollbar_x - search_w - 20.0 * s;
        let panel_w = 220.0 * s;
        let panel_h = search_h;
        let panel_x = (search_x + search_w - panel_w).max(gutter_x + 8.0 * s);
        let panel_y = tab_bar_h + 10.0 * s + search_h + 8.0 * s;
        ui_registry.register_blocker(
            crate::ui_system::UiId::GitDiffPanelBody,
            panel_x,
            panel_y,
            panel_w,
            panel_h,
            mx,
            my,
        );
        self.push_rounded_rect(
            panel_x - 1.0,
            panel_y - 1.0,
            panel_w + 2.0,
            panel_h + 2.0,
            6.0 * s,
            [
                self.theme.sel[0],
                self.theme.sel[1],
                self.theme.sel[2],
                0.55,
            ],
        );
        self.push_rounded_rect(
            panel_x,
            panel_y,
            panel_w,
            panel_h,
            6.0 * s,
            [
                self.theme.minimap_bg[0],
                self.theme.minimap_bg[1],
                self.theme.minimap_bg[2],
                1.0,
            ],
        );

        let current_line = ((render_scroll_y.max(0.0)
            + editor_scroll_height * crate::app::git_diff::GIT_DIFF_FOCUS_RATIO)
            / self.line_height.max(1.0))
        .floor()
        .max(0.0) as usize;
        let current_idx = state
            .current_hunk_idx
            .or_else(|| {
                state
                    .hunks
                    .iter()
                    .rposition(|hunk| hunk.display_start_line <= current_line)
            })
            .unwrap_or(0)
            + 1;
        let mut current_buf = [0u8; 20];
        let mut total_buf = [0u8; 20];
        let current_s = decimal_usize_buf(&mut current_buf, current_idx.min(state.hunks.len()));
        let total_s = decimal_usize_buf(&mut total_buf, state.hunks.len());
        self.draw_string_scaled(
            "Diff",
            panel_x + 12.0 * s,
            panel_y + panel_h * 0.5 + 5.0 * s,
            self.theme.fg,
            0.9,
        );
        let nums_x = panel_x + 54.0 * s;
        self.draw_string_scaled(
            current_s,
            nums_x,
            panel_y + panel_h * 0.5 + 5.0 * s,
            self.theme.fg,
            0.9,
        );
        let slash_x = nums_x + self.measure_ui_width(current_s, 0.9);
        self.draw_string_scaled(
            "/",
            slash_x,
            panel_y + panel_h * 0.5 + 5.0 * s,
            self.theme.line_num,
            0.9,
        );
        self.draw_string_scaled(
            total_s,
            slash_x + 7.0 * s,
            panel_y + panel_h * 0.5 + 5.0 * s,
            self.theme.fg,
            0.9,
        );

        let btn_y = panel_y + 8.0 * s;
        let btn_size = 36.0 * s;
        let mut current_x = panel_x + panel_w - 10.0 * s;

        current_x -= btn_size;
        let btn_down = crate::widgets::IconButton {
            x: current_x,
            y: btn_y,
            size: btn_size,
            icon: Some(crate::widgets::IconType::Down),
            is_active: false,
            icon_size: Some(37.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        current_x -= 10.0 * s;

        current_x -= btn_size;
        let btn_up = crate::widgets::IconButton {
            x: current_x,
            y: btn_y,
            size: btn_size,
            icon: Some(crate::widgets::IconType::Up),
            is_active: false,
            icon_size: Some(37.0 * s),
            active_square_width: None,
            custom_color: None,
        };

        ui_registry.register_icon_button(
            crate::ui_system::UiId::GitDiffPrevHunk,
            &btn_up,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_registry.register_icon_button(
            crate::ui_system::UiId::GitDiffNextHunk,
            &btn_down,
            self,
            mx,
            my,
            s,
            false,
        );
    }

}
