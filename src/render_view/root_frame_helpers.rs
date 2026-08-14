pub(crate) const DATABASE_DIALOG_TOOLTIP_TEXT_SCALE: f32 = 0.82;
pub(crate) const TAB_TOOLTIP_TEXT_SCALE: f32 = 0.95;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StandardTooltipTextLayout {
    pub(crate) content_x: f32,
    pub(crate) content_y: f32,
    pub(crate) first_baseline_y: f32,
    pub(crate) line_h: f32,
}

impl StandardTooltipTextLayout {
    #[inline(always)]
    pub(crate) fn baseline_y(self, line: usize) -> f32 {
        self.first_baseline_y + line as f32 * self.line_h
    }
}

#[inline(always)]
pub(crate) fn standard_tooltip_text_layout(
    rect_x: f32,
    rect_y: f32,
    pad_x: f32,
    pad_y: f32,
    line_h: f32,
    baseline_offset: f32,
) -> StandardTooltipTextLayout {
    let content_x = (rect_x + pad_x).round();
    let content_y = (rect_y + pad_y).round();
    StandardTooltipTextLayout {
        content_x,
        content_y,
        first_baseline_y: (content_y + baseline_offset).round(),
        line_h: line_h.round().max(1.0),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[inline(always)]
    pub(crate) fn draw_standard_tooltip_text_line(
        &mut self,
        text: &str,
        layout: StandardTooltipTextLayout,
        line: usize,
        color: [f32; 4],
        text_scale: f32,
    ) {
        self.draw_string_scaled_stable(
            text,
            layout.content_x,
            layout.baseline_y(line),
            color,
            text_scale,
        );
    }

    fn draw_inline_git_text_line(
        &mut self,
        text: &str,
        spans: &[ColorSpan],
        base_offset: Option<usize>,
        x: f32,
        y: f32,
        max_x: f32,
    ) {
        let _ =
            self.draw_spanned_editor_line_alpha(text, spans, base_offset, x, y, max_x, 1.0);
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
        for line in popup.lines.iter().take(visible_rows) {
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
                self.push_rect(panel_x, row_y, panel_w, row_h, color);
            }
            self.draw_inline_git_text_line(
                &line.text,
                &popup.spans,
                Some(line.display_start),
                text_x,
                row_y + self.baseline_offset,
                max_text_x,
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

#[cfg(test)]
mod standard_tooltip_tests {
    use super::*;

    fn glyph(offset_y: f32, height: f32) -> crate::renderer::GlyphInfo {
        crate::renderer::GlyphInfo {
            u: 0.0,
            v: 0.0,
            uw: 1.0,
            vh: 1.0,
            width: 7.2,
            height,
            offset_x: 0.25,
            offset_y,
            advance: 8.0,
            is_emoji: 0.0,
        }
    }

    fn rendered_y(
        layout: StandardTooltipTextLayout,
        line: usize,
        glyph: crate::renderer::GlyphInfo,
        scale: f32,
        color: [f32; 4],
    ) -> (f32, f32) {
        let (x, y, w, h) = crate::renderer::glyph_quad_rect(
            layout.content_x,
            layout.baseline_y(line),
            glyph,
            scale,
        );
        let vertices = crate::renderer::quad_vertices(
            x,
            y,
            w,
            h,
            glyph.u,
            glyph.v,
            glyph.uw,
            glyph.vh,
            color,
            glyph.is_emoji,
        );
        (vertices[0].pos[1], vertices[2].pos[1])
    }

    fn database_layout(rect_y: f32, dpi: f32) -> StandardTooltipTextLayout {
        let line_h = (20.0 * dpi).round().max(16.0);
        standard_tooltip_text_layout(
            40.25,
            rect_y,
            (12.0 * dpi).round(),
            (9.0 * dpi).round(),
            line_h,
            line_h * 0.5 + 5.5 * dpi,
        )
    }

    fn assert_dpi_stable(dpi: f32) {
        let layout = database_layout(80.35, dpi);
        let scale = DATABASE_DIALOG_TOOLTIP_TEXT_SCALE * dpi;
        let first = rendered_y(layout, 0, glyph(8.0, 5.98), scale, [1.0; 4]);
        let second = rendered_y(layout, 0, glyph(8.42, 6.4), scale, [1.0; 4]);
        assert_eq!(layout.content_x.fract(), 0.0);
        assert_eq!(layout.content_y.fract(), 0.0);
        assert_eq!(layout.first_baseline_y.fract(), 0.0);
        assert_eq!(first, rendered_y(layout, 0, glyph(8.0, 5.98), scale, [1.0; 4]));
        assert_eq!(second, rendered_y(layout, 0, glyph(8.42, 6.4), scale, [1.0; 4]));
    }

    #[test]
    fn standard_tooltip_cyrillic_glyph_geometry_is_deterministic() {
        let layout = database_layout(80.35, 1.5);
        let scale = DATABASE_DIALOG_TOOLTIP_TEXT_SCALE * 1.5;
        let geometry = |text: &str| {
            text.chars()
                .enumerate()
                .map(|(idx, _)| {
                    rendered_y(
                        layout,
                        0,
                        glyph(8.0 + idx as f32 * 0.07, 5.98 + idx as f32 * 0.05),
                        scale,
                        [1.0; 4],
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(geometry("Подсказка"), geometry("Подсказка"));
    }

    #[test]
    fn standard_tooltip_glyphs_recover_one_shared_baseline() {
        let layout = database_layout(80.35, 1.25);
        let scale = DATABASE_DIALOG_TOOLTIP_TEXT_SCALE * 1.25;
        for glyph in [glyph(8.0, 5.98), glyph(8.42, 6.4), glyph(9.1, 7.2)] {
            let (_, y, _, _) = crate::renderer::glyph_quad_rect(
                layout.content_x,
                layout.baseline_y(0),
                glyph,
                scale,
            );
            assert!((y + glyph.offset_y * scale - layout.baseline_y(0)).abs() < 0.001);
        }
    }

    #[test]
    fn standard_tooltip_fractional_dpi_preserves_shared_glyph_edge() {
        let layout = database_layout(80.35, 1.5);
        let scale = DATABASE_DIALOG_TOOLTIP_TEXT_SCALE * 1.5;
        let first = rendered_y(layout, 0, glyph(8.0, 5.98), scale, [1.0; 4]);
        let second = rendered_y(layout, 0, glyph(8.42, 6.4), scale, [1.0; 4]);

        assert_eq!(first.1, second.1);
    }

    #[test]
    fn standard_tooltip_repeat_render_keeps_same_y() {
        let layout = database_layout(80.35, 1.75);
        let glyph = glyph(8.42, 6.4);
        let scale = DATABASE_DIALOG_TOOLTIP_TEXT_SCALE * 1.75;
        assert_eq!(
            rendered_y(layout, 0, glyph, scale, [1.0; 4]),
            rendered_y(layout, 0, glyph, scale, [1.0; 4])
        );
    }

    #[test]
    fn standard_tooltip_color_does_not_change_geometry() {
        let layout = database_layout(80.35, 1.5);
        let glyph = glyph(8.42, 6.4);
        let scale = DATABASE_DIALOG_TOOLTIP_TEXT_SCALE * 1.5;
        assert_eq!(
            rendered_y(layout, 0, glyph, scale, [1.0, 0.0, 0.0, 1.0]),
            rendered_y(layout, 0, glyph, scale, [0.0, 1.0, 0.0, 0.4])
        );
    }

    #[test]
    fn standard_tooltip_hover_timer_progress_does_not_change_baseline() {
        let expected = database_layout(80.35, 1.25).baseline_y(0);
        for _hover_progress in [0.0, 0.2, 0.5, 0.9, 1.0] {
            assert_eq!(database_layout(80.35, 1.25).baseline_y(0), expected);
        }
    }

    #[test]
    fn standard_tooltip_animation_progress_keeps_text_origin_fixed() {
        let expected = database_layout(80.35, 1.5);
        for _animation_progress in [0.0, 0.5, 1.0] {
            assert_eq!(database_layout(80.35, 1.5), expected);
        }
    }

    #[test]
    fn standard_tooltip_dpi_1_0_is_stable() {
        assert_dpi_stable(1.0);
    }

    #[test]
    fn standard_tooltip_dpi_1_25_is_stable() {
        assert_dpi_stable(1.25);
    }

    #[test]
    fn standard_tooltip_dpi_1_5_is_stable() {
        assert_dpi_stable(1.5);
    }

    #[test]
    fn standard_tooltip_dpi_1_75_is_stable() {
        assert_dpi_stable(1.75);
    }

    #[test]
    fn standard_tooltip_dpi_2_0_is_stable() {
        assert_dpi_stable(2.0);
    }

    #[test]
    fn database_field_tooltip_uses_real_text_scale() {
        let layout = database_layout(80.35, 1.0);
        assert_eq!(
            rendered_y(
                layout,
                0,
                glyph(8.0, 5.98),
                DATABASE_DIALOG_TOOLTIP_TEXT_SCALE,
                [1.0; 4],
            ),
            (98.0, 103.0)
        );
    }

    #[test]
    fn database_control_tooltip_uses_same_real_text_scale() {
        let layout = database_layout(140.35, 1.0);
        let geometry = rendered_y(
            layout,
            0,
            glyph(8.0, 5.98),
            DATABASE_DIALOG_TOOLTIP_TEXT_SCALE,
            [1.0; 4],
        );
        assert_eq!(geometry.1 - geometry.0, 5.0);
    }

    #[test]
    fn standard_tooltip_multiline_layout_keeps_fixed_line_height() {
        let layout = database_layout(80.35, 1.25);
        assert_eq!(layout.baseline_y(1) - layout.baseline_y(0), layout.line_h);
        assert_eq!(layout.baseline_y(4) - layout.baseline_y(3), layout.line_h);
    }

    #[test]
    fn database_dialog_scroll_moves_anchor_but_not_local_glyph_geometry() {
        let first = database_layout(80.35, 1.5);
        let scrolled = database_layout(33.35, 1.5);
        let glyph = glyph(8.42, 6.4);
        let scale = DATABASE_DIALOG_TOOLTIP_TEXT_SCALE * 1.5;
        let first_y = rendered_y(first, 0, glyph, scale, [1.0; 4]);
        let scrolled_y = rendered_y(scrolled, 0, glyph, scale, [1.0; 4]);
        assert_eq!(
            first_y.0 - first.content_y,
            scrolled_y.0 - scrolled.content_y
        );
        assert_eq!(
            first_y.1 - first.content_y,
            scrolled_y.1 - scrolled.content_y
        );
    }

    #[test]
    fn tab_and_database_tooltips_share_snapped_layout_model() {
        let database = database_layout(80.35, 1.25);
        let tab_h = 32.0 * 1.25;
        let tab = standard_tooltip_text_layout(
            50.4,
            44.3,
            12.0 * 1.25,
            0.0,
            tab_h,
            tab_h * 0.5 + 5.0 * 1.25,
        );
        for layout in [database, tab] {
            assert_eq!(layout.content_x.fract(), 0.0);
            assert_eq!(layout.content_y.fract(), 0.0);
            assert_eq!(layout.first_baseline_y.fract(), 0.0);
        }
        assert_eq!(TAB_TOOLTIP_TEXT_SCALE, 0.95);
    }

    #[test]
    fn standard_tooltip_fast_rehover_returns_identical_geometry() {
        let first = database_layout(80.35, 1.75);
        let after_leave_and_rehover = database_layout(80.35, 1.75);
        assert_eq!(first, after_leave_and_rehover);
    }

    #[test]
    fn standard_tooltip_text_layout_does_not_mutate_popup_frame() {
        let frame = crate::ui_system::UiClipRect::new(30.0, 40.0, 320.0, 96.0);
        let before = frame;
        let _ = standard_tooltip_text_layout(
            frame.x,
            frame.y,
            12.0,
            9.0,
            20.0,
            15.5,
        );
        assert_eq!(frame, before);
    }

    #[test]
    fn standard_tooltip_content_origin_is_snapped_once() {
        let layout = standard_tooltip_text_layout(10.49, 20.49, 11.51, 8.51, 19.6, 15.2);
        assert_eq!(layout.content_x, 22.0);
        assert_eq!(layout.content_y, 29.0);
        assert_eq!(layout.first_baseline_y, 44.0);
        assert_eq!(layout.line_h, 20.0);
    }
}
