use crate::app::project_search::{
    PROJECT_SEARCH_ROW_H, ProjectSearchField, ProjectSearchFlatRow, ProjectSearchLayout,
    ProjectSearchRect,
};
use crate::editor::Editor;

fn project_search_row_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    row_y.round() + row_h.round() * 0.5 + (4.5 * scale).round()
}

fn project_search_input_line_y(rect_y: f32, line_idx: usize, line_h: f32, scale: f32) -> f32 {
    let row_y = rect_y.round() + (5.0 * scale).round() + line_idx as f32 * line_h.round();
    project_search_row_text_y(row_y, line_h, scale)
}

fn project_search_line_end(text: &str, line_start: usize, mut line_end: usize) -> usize {
    line_end = line_end.min(text.len());
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\n') {
        line_end -= 1;
    }
    if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\r') {
        line_end -= 1;
    }
    line_end
}

fn project_search_cursor_line(editor: &Editor) -> usize {
    editor
        .line_offsets
        .partition_point(|&offset| offset <= editor.cursor)
        .saturating_sub(1)
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_project_search_panel(
        &mut self,
        content_x: f32,
        content_y: f32,
        content_w: f32,
        content_h: f32,
        scale: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        blink_alpha: f32,
    ) {
        let layout = crate::app::project_search::project_search_layout(
            content_x, content_y, content_w, content_h, scale,
        );
        ui_registry.register_blocker(
            crate::ui_system::UiId::ProjectSearchPanelBody,
            content_x,
            content_y,
            content_w,
            content_h,
            self.last_mouse_x,
            self.last_mouse_y,
        );

        let pad = crate::app::project_search::PROJECT_SEARCH_PAD_X * scale;
        let label_color = [0.66, 0.68, 0.72, 1.0];
        let label_scale = 0.74;

        self.draw_string_scaled_stable(
            "Search",
            (layout.query.x).round(),
            (layout.query.y - 6.0 * scale).round(),
            label_color,
            label_scale,
        );
        self.draw_project_search_input(
            layout.query,
            &ide_panel.project_search.query_editor,
            ide_panel.project_search.focused == Some(ProjectSearchField::Query),
            true,
            crate::ui_system::UiId::ProjectSearchQueryInput,
            ui_registry,
            blink_alpha,
        );

        let case_btn = IconButton {
            x: layout.case_button.x,
            y: layout.case_button.y,
            size: layout.case_button.w,
            icon: Some(crate::widgets::IconType::CaseMatch),
            is_active: ide_panel.project_search.case_sensitive,
            icon_size: Some(22.0 * scale),
            active_square_width: None,
            custom_color: None,
        };
        ui_registry.register_icon_button(
            crate::ui_system::UiId::ProjectSearchCaseToggle,
            &case_btn,
            self,
            self.last_mouse_x,
            self.last_mouse_y,
            scale,
            false,
        );

        let run_btn = IconButton {
            x: layout.run_button.x,
            y: layout.run_button.y,
            size: layout.run_button.w,
            icon: Some(crate::widgets::IconType::Search),
            is_active: ide_panel.project_search.running_generation.is_some(),
            icon_size: Some(22.0 * scale),
            active_square_width: None,
            custom_color: None,
        };
        ui_registry.register_icon_button(
            crate::ui_system::UiId::ProjectSearchRun,
            &run_btn,
            self,
            self.last_mouse_x,
            self.last_mouse_y,
            scale,
            false,
        );

        self.draw_string_scaled_stable(
            "files to include",
            (layout.include.x).round(),
            (layout.include.y - 6.0 * scale).round(),
            label_color,
            label_scale,
        );
        self.draw_project_search_input(
            layout.include,
            &ide_panel.project_search.include_editor,
            ide_panel.project_search.focused == Some(ProjectSearchField::Include),
            false,
            crate::ui_system::UiId::ProjectSearchIncludeInput,
            ui_registry,
            blink_alpha,
        );

        self.draw_string_scaled_stable(
            "files to exclude",
            (layout.exclude.x).round(),
            (layout.exclude.y - 6.0 * scale).round(),
            label_color,
            label_scale,
        );
        self.draw_project_search_input(
            layout.exclude,
            &ide_panel.project_search.exclude_editor,
            ide_panel.project_search.focused == Some(ProjectSearchField::Exclude),
            false,
            crate::ui_system::UiId::ProjectSearchExcludeInput,
            ui_registry,
            blink_alpha,
        );

        self.push_rect(
            content_x,
            (layout.stats_y - 12.0 * scale).round(),
            content_w,
            1.0,
            [1.0, 1.0, 1.0, 0.06],
        );
        self.draw_project_search_stats(&layout, ide_panel, pad, scale);
        self.draw_project_search_results(&layout, ide_panel, ui_registry, scale);
    }

    fn draw_project_search_stats(
        &mut self,
        layout: &ProjectSearchLayout,
        ide_panel: &crate::app::IdePanelState,
        pad: f32,
        _scale: f32,
    ) {
        let state = &ide_panel.project_search;
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        scratch.clear();
        let color = if state.running_generation.is_some() {
            scratch.push_str("Ищет...");
            [0.741, 0.576, 0.976, 1.0]
        } else if let Some(error) = &state.error {
            scratch.push_str(error);
            [0.95, 0.35, 0.45, 1.0]
        } else if state.has_run {
            use std::fmt::Write;
            let _ = write!(
                &mut scratch,
                "{} в {} файлах",
                state.total_matches,
                state.results.len()
            );
            if let Some(ms) = state.elapsed_ms {
                let _ = write!(&mut scratch, " ({} мс)", ms);
            }
            if state.capped {
                scratch.push_str(" limit");
            }
            if state.total_matches == 0 {
                [0.66, 0.68, 0.72, 1.0]
            } else {
                [0.62, 0.86, 0.62, 1.0]
            }
        } else {
            scratch.push_str("0 в 0 файлах");
            [0.50, 0.52, 0.58, 1.0]
        };
        self.draw_string_scaled_stable(
            &scratch,
            (layout.list.x + pad).round(),
            layout.stats_y.round(),
            color,
            0.80,
        );
        self.scratch_buffer = scratch;
    }

    fn draw_project_search_input(
        &mut self,
        rect: ProjectSearchRect,
        editor: &Editor,
        focused: bool,
        multiline: bool,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        blink_alpha: f32,
    ) {
        let scale = self.scale_factor;
        let border = if focused {
            [0.741, 0.576, 0.976, 1.0]
        } else {
            [1.0, 1.0, 1.0, 0.16]
        };
        self.push_rounded_rect(
            rect.x - 1.0,
            rect.y - 1.0,
            rect.w + 2.0,
            rect.h + 2.0,
            4.0 * scale,
            border,
        );
        self.push_rounded_rect(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            4.0 * scale,
            [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
        );
        ui_registry.register_text_input(
            id,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            self.last_mouse_x,
            self.last_mouse_y,
        );

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (rect.y + rect.h)).round() as i32;
            self.gl.scissor(
                rect.x.round() as i32,
                sy,
                rect.w.round() as i32,
                rect.h.round() as i32,
            );
        }

        let text = editor.get_full_text();
        let text_scale = 0.82;
        let line_h = (18.0 * scale).round().max(1.0);
        let visible_lines = if multiline {
            ((rect.h - 8.0 * scale) / line_h).floor().max(1.0) as usize
        } else {
            1
        };
        let cursor_line = project_search_cursor_line(editor);
        let first_line = if multiline {
            cursor_line.saturating_sub(visible_lines.saturating_sub(1))
        } else {
            0
        };
        let sel_anchor = editor.selection_anchor.unwrap_or(editor.cursor);
        let sel_start = sel_anchor.min(editor.cursor);
        let sel_end = sel_anchor.max(editor.cursor);
        let draw_x = (rect.x + 7.0 * scale).round();
        let max_line = editor.line_offsets.len().min(first_line + visible_lines);

        for line_idx in first_line..max_line {
            let Some(&line_start) = editor.line_offsets.get(line_idx) else {
                continue;
            };
            let line_end = project_search_line_end(
                &text,
                line_start,
                editor
                    .line_offsets
                    .get(line_idx + 1)
                    .copied()
                    .unwrap_or(text.len()),
            );
            let visual_idx = line_idx - first_line;
            let text_y = project_search_input_line_y(rect.y, visual_idx, line_h, scale);
            if let Some(line_text) = text.get(line_start..line_end) {
                if sel_start < line_end && sel_end > line_start {
                    let row_sel_start = sel_start.max(line_start).min(line_end);
                    let row_sel_end = sel_end.max(line_start).min(line_end);
                    let prefix = text.get(line_start..row_sel_start).unwrap_or("");
                    let selected = text.get(row_sel_start..row_sel_end).unwrap_or("");
                    let x1 = draw_x + self.measure_ui_width(prefix, text_scale).round();
                    let sw = self.measure_ui_width(selected, text_scale).round();
                    if sw > 0.0 {
                        self.push_rect(
                            x1,
                            (text_y - 13.0 * scale).round(),
                            sw,
                            (line_h - 2.0 * scale).max(1.0),
                            [0.50, 0.34, 0.78, 0.55],
                        );
                    }
                }
                self.draw_string_scaled_stable(line_text, draw_x, text_y, self.theme.fg, text_scale);
            }

            if focused
                && blink_alpha > 0.5
                && editor.cursor >= line_start
                && editor.cursor <= line_end
            {
                let prefix = text.get(line_start..editor.cursor).unwrap_or("");
                let cursor_x = draw_x + self.measure_ui_width(prefix, text_scale).round();
                self.push_rect(
                    cursor_x,
                    (text_y - 13.0 * scale).round(),
                    (2.0 * scale).max(1.0),
                    (line_h - 2.0 * scale).max(1.0),
                    self.theme.fg,
                );
            }
        }

        if focused
            && blink_alpha > 0.5
            && text.is_empty()
            && first_line == 0
        {
            let text_y = project_search_input_line_y(rect.y, 0, line_h, scale);
            self.push_rect(
                draw_x,
                (text_y - 13.0 * scale).round(),
                (2.0 * scale).max(1.0),
                (line_h - 2.0 * scale).max(1.0),
                self.theme.fg,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    fn draw_project_search_results(
        &mut self,
        layout: &ProjectSearchLayout,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        scale: f32,
    ) {
        let state = &ide_panel.project_search;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (layout.list.y + layout.list.h)).round() as i32;
            self.gl.scissor(
                layout.list.x.round() as i32,
                sy,
                layout.list.w.round() as i32,
                layout.list.h.round() as i32,
            );
        }

        if state.flat_rows.is_empty() {
            if state.has_run && state.running_generation.is_none() {
                let hint = if state.total_matches == 0 {
                    "Нет совпадений"
                } else {
                    ""
                };
                if !hint.is_empty() {
                    let tw = self.measure_ui_width(hint, 0.84);
                    self.draw_string_scaled_stable(
                        hint,
                        (layout.list.x + (layout.list.w - tw) * 0.5).round(),
                        (layout.list.y + 32.0 * scale).round(),
                        [0.45, 0.45, 0.50, 1.0],
                        0.84,
                    );
                }
            }
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
            return;
        }

        let row_h = PROJECT_SEARCH_ROW_H * scale;
        let scroll = state.scroll.current.round();
        let hover_settled = (state.scroll.current - state.scroll.target).abs() < 0.5;
        let first = (scroll / row_h).floor().max(0.0) as usize;
        let last = (((scroll + layout.list.h) / row_h).ceil() as usize + 1)
            .min(state.flat_rows.len());
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        let mut clip_scratch = String::new();
        for row_idx in first..last {
            let row_y = layout.list.y + row_idx as f32 * row_h - scroll;
            match state.flat_rows[row_idx] {
                ProjectSearchFlatRow::File(file_idx) => {
                    self.draw_project_search_file_row(
                        file_idx,
                        row_y,
                        row_h,
                        layout,
                        ide_panel,
                        ui_registry,
                        hover_settled,
                        scale,
                        &mut scratch,
                    );
                }
                ProjectSearchFlatRow::Match(file_idx, match_idx) => {
                    self.draw_project_search_match_row(
                        file_idx,
                        match_idx,
                        row_y,
                        row_h,
                        layout,
                        ide_panel,
                        ui_registry,
                        hover_settled,
                        scale,
                        &mut scratch,
                        &mut clip_scratch,
                    );
                }
            }
        }
        self.scratch_buffer = scratch;
        self.draw_project_search_scrollbar(layout, state, scale);

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_project_search_file_row(
        &mut self,
        file_idx: usize,
        row_y: f32,
        row_h: f32,
        layout: &ProjectSearchLayout,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover_settled: bool,
        scale: f32,
        scratch: &mut String,
    ) {
        let Some(file) = ide_panel.project_search.results.get(file_idx) else {
            return;
        };
        ui_registry.register_rect(
            crate::ui_system::UiId::ProjectSearchFileToggle(file_idx),
            layout.list.x,
            row_y,
            layout.list.w - 10.0 * scale,
            row_h,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        if hover_settled
            && ui_registry.hovered()
                == Some(crate::ui_system::UiId::ProjectSearchFileToggle(file_idx))
        {
            self.push_rect(
                layout.list.x,
                row_y.round(),
                layout.list.w - 10.0 * scale,
                row_h,
                [1.0, 1.0, 1.0, 0.05],
            );
        }

        let collapsed = ide_panel.project_search.collapsed.contains(&file.path);
        let arrow = if collapsed {
            crate::widgets::IconType::Up
        } else {
            crate::widgets::IconType::Down
        };
        let icon_size = 17.0 * scale;
        let arrow_x = layout.list.x + 8.0 * scale;
        let icon_y = row_y + (row_h - icon_size) * 0.5;
        self.draw_atlas_icon(
            arrow,
            arrow_x.round(),
            icon_y.round(),
            icon_size,
            [0.60, 0.60, 0.66, 1.0],
        );

        let file_icon_x = arrow_x + icon_size + 4.0 * scale;
        self.draw_file_icon(
            file.icon_key,
            false,
            file_icon_x.round(),
            icon_y.round(),
            icon_size,
        );

        let badge_text = file.matches.len().to_string();
        let badge_h = 18.0 * scale;
        let badge_w = (self.measure_ui_width(&badge_text, 0.72) + 10.0 * scale).max(badge_h);
        let badge_x = layout.list.x + layout.list.w - badge_w - 14.0 * scale;
        let badge_y = row_y + (row_h - badge_h) * 0.5;
        self.push_rounded_rect(
            badge_x.round(),
            badge_y.round(),
            badge_w,
            badge_h,
            badge_h * 0.5,
            [0.741, 0.576, 0.976, 0.28],
        );
        let badge_text_x =
            badge_x + (badge_w - self.measure_ui_width(&badge_text, 0.72)) * 0.5;
        self.draw_string_scaled_stable(
            &badge_text,
            badge_text_x.round(),
            project_search_row_text_y(row_y, row_h, scale),
            [0.86, 0.80, 0.96, 1.0],
            0.72,
        );

        let text_x = file_icon_x + icon_size + 6.0 * scale;
        let max_w = (badge_x - text_x - 8.0 * scale).max(0.0);
        self.draw_tree_label_clipped(
            &file.relative_path,
            text_x.round(),
            project_search_row_text_y(row_y, row_h, scale),
            max_w,
            self.theme.fg,
            0.82,
            scratch,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_project_search_match_row(
        &mut self,
        file_idx: usize,
        match_idx: usize,
        row_y: f32,
        row_h: f32,
        layout: &ProjectSearchLayout,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover_settled: bool,
        scale: f32,
        scratch: &mut String,
        clip_scratch: &mut String,
    ) {
        let Some(file) = ide_panel.project_search.results.get(file_idx) else {
            return;
        };
        let Some(mat) = file.matches.get(match_idx) else {
            return;
        };
        ui_registry.register_rect(
            crate::ui_system::UiId::ProjectSearchMatchJump(file_idx, match_idx),
            layout.list.x,
            row_y,
            layout.list.w - 10.0 * scale,
            row_h,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        if hover_settled
            && ui_registry.hovered()
                == Some(crate::ui_system::UiId::ProjectSearchMatchJump(file_idx, match_idx))
        {
            self.push_rect(
                layout.list.x,
                row_y.round(),
                layout.list.w - 10.0 * scale,
                row_h,
                [1.0, 1.0, 1.0, 0.045],
            );
        }
        let text_y = project_search_row_text_y(row_y, row_h, scale);
        let indent = 30.0 * scale;
        let line_x = layout.list.x + indent;
        scratch.clear();
        use std::fmt::Write;
        let _ = write!(scratch, "{}", mat.start_line + 1);
        let line_w = self.measure_ui_width(scratch.as_str(), 0.72).round();
        self.draw_string_scaled_stable(
            scratch.as_str(),
            line_x.round(),
            text_y,
            [0.52, 0.55, 0.64, 1.0],
            0.72,
        );

        let preview_x = line_x + line_w + 10.0 * scale;
        scratch.clear();
        if mat.extra_lines > 0 {
            let _ = write!(scratch, "(+{}) ", mat.extra_lines);
        }
        scratch.push_str(&mat.preview);
        let max_w = (layout.list.x + layout.list.w - preview_x - 16.0 * scale).max(0.0);
        self.draw_tree_label_clipped(
            scratch.as_str(),
            preview_x.round(),
            text_y,
            max_w,
            [0.75, 0.77, 0.82, 1.0],
            0.78,
            clip_scratch,
        );
    }

    fn draw_project_search_scrollbar(
        &mut self,
        layout: &ProjectSearchLayout,
        state: &crate::app::project_search::ProjectSearchState,
        scale: f32,
    ) {
        let row_h = PROJECT_SEARCH_ROW_H * scale;
        let total_h = state.flat_rows.len() as f32 * row_h;
        if total_h <= layout.list.h || layout.list.h <= 0.0 {
            return;
        }
        let max_scroll = (total_h - layout.list.h).max(0.0);
        let ratio = (state.scroll.current / max_scroll.max(1.0)).clamp(0.0, 1.0);
        let track_h = layout.list.h;
        let thumb_h = (layout.list.h / total_h * track_h).max(22.0 * scale);
        let thumb_y = layout.list.y + ratio * (track_h - thumb_h);
        self.push_rounded_rect(
            layout.list.x + layout.list.w - 10.0 * scale,
            thumb_y.round(),
            5.0 * scale,
            thumb_h,
            3.0 * scale,
            [0.48, 0.48, 0.56, 0.55],
        );
    }
}
