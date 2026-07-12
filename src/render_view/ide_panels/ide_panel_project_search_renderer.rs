use crate::app::project_search::{
    PROJECT_SEARCH_ROW_H, ProjectSearchField, ProjectSearchFlatRow, ProjectSearchLayout,
    ProjectSearchQueryScrollAxis, ProjectSearchRect,
};
use crate::editor::Editor;

fn project_search_row_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    (row_y.round() + row_h.round() * 0.5 + (4.5 * scale).round()).round()
}

fn project_search_scaled_step(px: f32, scale: f32) -> f32 {
    (px * scale).round()
}

fn project_search_input_line_y(rect_y: f32, line_idx: usize, line_h: f32, scale: f32) -> f32 {
    let row_y = (rect_y + project_search_scaled_step(5.0, scale)).round()
        + line_idx as f32 * line_h.round();
    project_search_row_text_y(row_y, line_h, scale)
}

fn project_search_label_text_y(input_y: f32, scale: f32) -> f32 {
    let row_h = (18.0 * scale).round().max(1.0);
    let gap = project_search_scaled_step(4.0, scale);
    project_search_row_text_y(input_y.round() - row_h - gap, row_h, scale)
}

fn project_search_prefix_len_for_width(
    text: &str,
    max_w: f32,
    glyph_w: &mut impl FnMut(char) -> f32,
) -> usize {
    if max_w <= 0.0 {
        return 0;
    }
    let mut width = 0.0;
    let mut end = 0;
    for (idx, ch) in text.char_indices() {
        let next = width + glyph_w(ch);
        if next > max_w {
            break;
        }
        width = next;
        end = idx + ch.len_utf8();
    }
    end
}

fn project_search_suffix_start_for_width(
    text: &str,
    max_w: f32,
    glyph_w: &mut impl FnMut(char) -> f32,
) -> usize {
    if max_w <= 0.0 {
        return text.len();
    }
    let mut width = 0.0;
    let mut start = text.len();
    for (idx, ch) in text.char_indices().rev() {
        let next = width + glyph_w(ch);
        if next > max_w {
            break;
        }
        width = next;
        start = idx;
    }
    start
}

fn project_search_text_width(text: &str, glyph_w: &mut impl FnMut(char) -> f32) -> f32 {
    text.chars().map(glyph_w).sum()
}

fn project_search_visible_match_preview(
    text: &str,
    highlight_start: usize,
    highlight_end: usize,
    max_w: f32,
    mut glyph_w: impl FnMut(char) -> f32,
) -> (String, usize, usize) {
    let mut full_width_glyph = |ch| glyph_w(ch);
    let full_w = project_search_text_width(text, &mut full_width_glyph);
    if full_w <= max_w {
        return (
            text.to_string(),
            highlight_start.min(text.len()),
            highlight_end.min(text.len()).max(highlight_start.min(text.len())),
        );
    }

    let hs = floor_char_boundary_local(text, highlight_start.min(text.len()));
    let he = ceil_char_boundary_local(text, highlight_end.min(text.len())).max(hs);
    let hit = text.get(hs..he).unwrap_or("");
    let ellipsis = "…";
    let ellipsis_w = project_search_text_width(ellipsis, &mut glyph_w);
    let hit_w = project_search_text_width(hit, &mut glyph_w);
    let left_ellipsis = hs > 0;
    let right_ellipsis = he < text.len();
    let reserved = hit_w
        + if left_ellipsis { ellipsis_w } else { 0.0 }
        + if right_ellipsis { ellipsis_w } else { 0.0 };
    if reserved >= max_w {
        let fit = project_search_prefix_len_for_width(hit, max_w, &mut glyph_w);
        let mut out = String::new();
        out.push_str(hit.get(..fit).unwrap_or(""));
        return (out, 0, fit);
    }

    let context_budget = (max_w - reserved).max(0.0);
    let before = text.get(..hs).unwrap_or("");
    let after = text.get(he..).unwrap_or("");
    let right_budget = context_budget * 0.45;
    let right_len = project_search_prefix_len_for_width(after, right_budget, &mut glyph_w);
    let right = after.get(..right_len).unwrap_or("");
    let right_w = project_search_text_width(right, &mut glyph_w);
    let left_budget = (context_budget - right_w).max(0.0);
    let left_start = project_search_suffix_start_for_width(before, left_budget, &mut glyph_w);
    let left = before.get(left_start..).unwrap_or("");

    let mut out = String::with_capacity(left.len() + hit.len() + right.len() + 6);
    if left_ellipsis {
        out.push_str(ellipsis);
    }
    out.push_str(left);
    let out_hs = out.len();
    out.push_str(hit);
    let out_he = out.len();
    out.push_str(right);
    if right_ellipsis {
        out.push_str(ellipsis);
    }
    (out, out_hs, out_he)
}

fn floor_char_boundary_local(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary_local(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_project_search_text_stable(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        self.draw_string_scaled_stable(text, x, y, color, scale);
    }

    fn draw_project_search_label_text_stable(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        self.draw_string_scaled_pixel_snapped(text, x, y, color, scale);
    }

    pub(crate) fn project_search_stable_text_width(&mut self, text: &str, scale: f32) -> f32 {
        text.chars()
            .filter_map(|c| self.get_ui_glyph(c))
            .map(|g| Self::snapped_text_advance(g.advance, scale))
            .sum::<f32>()
            .round()
    }

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

        self.draw_project_search_label_text_stable(
            "Поиск",
            (layout.query.x).round(),
            project_search_label_text_y(layout.query.y, scale),
            label_color,
            label_scale,
        );
        self.draw_project_search_help_button(layout.help_button, ide_panel, ui_registry, scale);
        self.draw_project_search_input(
            layout.query,
            &ide_panel.project_search.query_editor,
            ide_panel.project_search.focused == Some(ProjectSearchField::Query),
            true,
            true,
            crate::ui_system::UiId::ProjectSearchQueryInput,
            ui_registry,
            blink_alpha,
            Some(&ide_panel.project_search),
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

        self.draw_project_search_label_text_stable(
            "Файлы включить",
            (layout.include.x).round(),
            project_search_label_text_y(layout.include.y, scale),
            label_color,
            label_scale,
        );
        self.draw_project_search_input(
            layout.include,
            &ide_panel.project_search.include_editor,
            ide_panel.project_search.focused == Some(ProjectSearchField::Include),
            false,
            true,
            crate::ui_system::UiId::ProjectSearchIncludeInput,
            ui_registry,
            blink_alpha,
            None,
        );

        self.draw_project_search_label_text_stable(
            "Файлы исключить",
            (layout.exclude.x).round(),
            project_search_label_text_y(layout.exclude.y, scale),
            label_color,
            label_scale,
        );
        self.draw_project_search_input(
            layout.exclude,
            &ide_panel.project_search.exclude_editor,
            ide_panel.project_search.focused == Some(ProjectSearchField::Exclude),
            false,
            true,
            crate::ui_system::UiId::ProjectSearchExcludeInput,
            ui_registry,
            blink_alpha,
            None,
        );

        self.push_rect(
            layout.filter.x,
            (layout.filter.y - 18.0 * scale).round(),
            layout.filter.w,
            1.0,
            [1.0, 1.0, 1.0, 0.05],
        );
        let filter_enabled = ide_panel.project_search.filter_enabled();
        let filter_label_color = if filter_enabled {
            label_color
        } else {
            [0.40, 0.41, 0.45, 1.0]
        };
        self.draw_project_search_label_text_stable(
            "Фильтровать",
            (layout.filter.x).round(),
            project_search_label_text_y(layout.filter.y, scale),
            filter_label_color,
            label_scale,
        );
        self.draw_project_search_input(
            layout.filter,
            &ide_panel.project_search.filter_editor,
            filter_enabled && ide_panel.project_search.focused == Some(ProjectSearchField::Filter),
            false,
            filter_enabled,
            crate::ui_system::UiId::ProjectSearchFilterInput,
            ui_registry,
            blink_alpha,
            None,
        );

        self.push_rect(
            content_x,
            (layout.stats_y - 16.0 * scale).round(),
            content_w,
            1.0,
            [1.0, 1.0, 1.0, 0.06],
        );
        self.draw_project_search_stats(&layout, ide_panel, pad, scale);
        self.draw_project_search_results(&layout, ide_panel, ui_registry, scale);
    }

    fn draw_project_search_help_button(
        &mut self,
        rect: ProjectSearchRect,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        scale: f32,
    ) {
        ui_registry.register_rect(
            crate::ui_system::UiId::ProjectSearchHelp,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        let hovered = ui_registry.hovered() == Some(crate::ui_system::UiId::ProjectSearchHelp);
        let active = ide_panel.project_search.help_open;
        let fill = if active {
            [0.741, 0.576, 0.976, 0.30]
        } else if hovered {
            [1.0, 1.0, 1.0, 0.10]
        } else {
            [1.0, 1.0, 1.0, 0.05]
        };
        self.push_rounded_rect(rect.x, rect.y, rect.w, rect.h, rect.h * 0.5, fill);
        let tw = self.measure_ui_width("?", 0.82);
        self.draw_project_search_text_stable(
            "?",
            (rect.x + (rect.w - tw) * 0.5).round(),
            project_search_row_text_y(rect.y, rect.h, scale),
            [0.88, 0.88, 0.92, 1.0],
            0.82,
        );
    }

    pub(crate) fn draw_project_search_help_overlay(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        scale: f32,
    ) -> bool {
        if !ide_panel.project_search.help_open {
            return false;
        }
        ui_registry.mark_overlay_start();
        ui_registry.reset_cursor_state();
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);

        let w = (crate::app::file_tree::FILE_TREE_DIALOG_W * scale)
            .min((self.width - 32.0 * scale).max(220.0 * scale))
            .round();
        let h = (430.0 * scale)
            .min((self.height - 32.0 * scale).max(300.0 * scale))
            .round();
        let x = ((self.width - w) * 0.5).round();
        let y = ((self.height - h) * 0.5).round();
        let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * scale;
        ui_registry.register_blocker(
            crate::ui_system::UiId::ProjectSearchHelpPopup,
            x,
            y,
            w,
            h,
            mx,
            my,
        );
        self.draw_file_tree_dialog_shell(x, y, w, h, scale);
        self.draw_string_scaled(
            "Поиск по рабочим областям",
            x + side_pad,
            y + 38.0 * scale,
            self.theme.fg,
            1.0,
        );
        let popup_y = (y + 58.0 * scale).round();
        let mut cy = project_search_row_text_y(
            popup_y,
            project_search_scaled_step(28.0, scale).max(1.0),
            scale,
        );
        let content_x = x + side_pad;
        self.draw_project_search_help_line(
            "Ищет только внутри открытых рабочих областей.",
            content_x,
            cy,
            0.72,
            [0.80, 0.82, 0.88, 1.0],
        );
        cy += project_search_scaled_step(20.0, scale);
        self.draw_project_search_help_line(
            "Учитывает .gitignore, .ignore и настройки игнора.",
            content_x,
            cy,
            0.72,
            [0.80, 0.82, 0.88, 1.0],
        );
        cy += project_search_scaled_step(26.0, scale);
        self.draw_project_search_help_line(
            "Шаблоны include / exclude",
            content_x,
            cy,
            0.78,
            [0.96, 0.94, 1.0, 1.0],
        );
        cy += project_search_scaled_step(21.0, scale);
        self.draw_project_search_help_line(
            "Через запятую. Пустое include = все файлы.",
            content_x,
            cy,
            0.70,
            [0.76, 0.78, 0.84, 1.0],
        );
        cy += project_search_scaled_step(23.0, scale);
        self.draw_project_search_help_code_row(
            content_x,
            cy,
            &["./src", "./app", "**/*.py"],
            scale,
        );
        cy += project_search_scaled_step(25.0, scale);
        self.draw_project_search_help_code_row(
            content_x,
            cy,
            &["src/**/*.rs", "target", "*.lock"],
            scale,
        );
        cy += project_search_scaled_step(28.0, scale);
        self.draw_project_search_help_line(
            "Фильтровать",
            content_x,
            cy,
            0.78,
            [0.96, 0.94, 1.0, 1.0],
        );
        cy += project_search_scaled_step(21.0, scale);
        self.draw_project_search_help_line(
            "После поиска скрывает файлы по тексту или простому *.rs.",
            content_x,
            cy,
            0.70,
            [0.76, 0.78, 0.84, 1.0],
        );
        cy += project_search_scaled_step(20.0, scale);
        self.draw_project_search_help_line(
            "Сложные glob-шаблоны тут не применяются.",
            content_x,
            cy,
            0.70,
            [0.76, 0.78, 0.84, 1.0],
        );
        cy += project_search_scaled_step(28.0, scale);
        self.draw_project_search_help_line(
            "Поиск",
            content_x,
            cy,
            0.78,
            [0.96, 0.94, 1.0, 1.0],
        );
        cy += project_search_scaled_step(21.0, scale);
        self.draw_project_search_help_line(
            "Literal-only. Ctrl+Enter или кнопка запуска.",
            content_x,
            cy,
            0.70,
            [0.76, 0.78, 0.84, 1.0],
        );
        cy += project_search_scaled_step(20.0, scale);
        self.draw_project_search_help_line(
            "Кнопка Aa включает чувствительность к регистру.",
            content_x,
            cy,
            0.70,
            [0.76, 0.78, 0.84, 1.0],
        );
        let btn_w = 112.0 * scale;
        let btn_h = 32.0 * scale;
        let btn_x = x + (w - btn_w) * 0.5;
        let btn_y = y + h - 64.0 * scale;
        self.draw_file_tree_dialog_buttons(
            ui_registry,
            [(crate::ui_system::UiId::ProjectSearchHelp, "Закрыть", btn_x)],
            btn_y,
            btn_w,
            btn_h,
            scale,
            mx,
            my,
        )
    }

    fn draw_project_search_help_line(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        color: [f32; 4],
    ) {
        self.draw_project_search_text_stable(text, x.round(), y.round(), color, scale);
    }

    fn draw_project_search_help_code_row(
        &mut self,
        mut x: f32,
        y: f32,
        parts: &[&str],
        scale: f32,
    ) {
        for part in parts {
            let text_scale = 0.68;
            let w = self.measure_ui_width(part, text_scale) + 10.0 * scale;
            self.push_rounded_rect(
                x.round(),
                (y - 13.0 * scale).round(),
                w.round(),
                20.0 * scale,
                4.0 * scale,
                [0.16, 0.17, 0.20, 1.0],
            );
            self.draw_project_search_text_stable(
                part,
                (x + 5.0 * scale).round(),
                y.round(),
                [0.91, 0.86, 1.0, 1.0],
                text_scale,
            );
            x += w + 6.0 * scale;
        }
    }

    fn draw_project_search_stats(
        &mut self,
        layout: &ProjectSearchLayout,
        ide_panel: &crate::app::IdePanelState,
        pad: f32,
        scale: f32,
    ) {
        let state = &ide_panel.project_search;
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        scratch.clear();
        let x = (layout.list.x + pad).round();
        let row_h = project_search_scaled_step(20.0, scale).max(1.0);
        let y = project_search_row_text_y(
            layout.stats_y - row_h * 0.5 - project_search_scaled_step(4.5, scale),
            row_h,
            scale,
        );
        if state.running_generation.is_some() {
            scratch.push_str("Ищет...");
            self.draw_project_search_text_stable(&scratch, x, y, [0.741, 0.576, 0.976, 1.0], 0.80);
        } else if let Some(error) = &state.error {
            scratch.push_str(error);
            self.draw_project_search_text_stable(&scratch, x, y, [0.95, 0.35, 0.45, 1.0], 0.80);
        } else if state.has_run {
            use std::fmt::Write;
            let _ = write!(
                &mut scratch,
                "{} в {} файлах",
                state.total_matches,
                state.results.len()
            );
            let base_w = self.project_search_stable_text_width(&scratch, 0.80);
            self.draw_project_search_text_stable(&scratch, x, y, [0.90, 0.91, 0.94, 1.0], 0.80);
            if let Some(ms) = state.elapsed_ms {
                scratch.clear();
                let _ = write!(&mut scratch, " ({} мс)", ms);
                self.draw_project_search_text_stable(
                    &scratch,
                    (x + base_w).round(),
                    y,
                    [0.62, 0.86, 0.62, 1.0],
                    0.80,
                );
                let time_w = self.project_search_stable_text_width(&scratch, 0.80);
                if state.capped {
                    self.draw_project_search_text_stable(
                        " limit",
                        (x + base_w + time_w).round(),
                        y,
                        [0.90, 0.91, 0.94, 1.0],
                        0.80,
                    );
                }
            } else if state.capped {
                self.draw_project_search_text_stable(
                    " limit",
                    (x + base_w).round(),
                    y,
                    [0.90, 0.91, 0.94, 1.0],
                    0.80,
                );
            }
        }
        self.scratch_buffer = scratch;
    }

    fn draw_project_search_input(
        &mut self,
        rect: ProjectSearchRect,
        editor: &Editor,
        focused: bool,
        multiline: bool,
        enabled: bool,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        blink_alpha: f32,
        query_state: Option<&crate::app::project_search::ProjectSearchState>,
    ) {
        let scale = self.scale_factor;
        let border = if focused && enabled {
            [0.741, 0.576, 0.976, 1.0]
        } else if enabled {
            [1.0, 1.0, 1.0, 0.16]
        } else {
            [1.0, 1.0, 1.0, 0.07]
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
            if enabled {
                [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0]
            } else {
                [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 0.58]
            },
        );
        if enabled {
            ui_registry.register_text_input(
                id,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }

        let query_viewport = query_state.map(|_| {
            crate::app::project_search::project_search_query_viewport(rect, scale)
        });
        let clip_rect = query_viewport.map(|viewport| viewport.text).unwrap_or(rect);
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (clip_rect.y + clip_rect.h)).round() as i32;
            self.gl.scissor(
                clip_rect.x.round() as i32,
                sy,
                clip_rect.w.round() as i32,
                clip_rect.h.round() as i32,
            );
        }

        let text = editor.get_full_text();
        let text_scale = 0.82;
        let line_h = crate::app::project_search::project_search_query_line_height(scale);
        let scroll_y = query_state
            .map(|state| state.query_scroll_y.current.round())
            .unwrap_or(0.0);
        let scroll_x = query_state
            .map(|state| state.query_scroll_x.current.round())
            .unwrap_or(0.0);
        let first_line = if multiline {
            (scroll_y / line_h).floor().max(0.0) as usize
        } else {
            0
        };
        let line_offset_y = scroll_y - first_line as f32 * line_h;
        let visible_lines = if multiline {
            (clip_rect.h / line_h).ceil().max(1.0) as usize + 1
        } else {
            1
        };
        let sel_anchor = editor.selection_anchor.unwrap_or(editor.cursor);
        let sel_start = sel_anchor.min(editor.cursor);
        let sel_end = sel_anchor.max(editor.cursor);
        let draw_x = if let Some(viewport) = query_viewport {
            (viewport.text.x - scroll_x).round()
        } else {
            (rect.x + 7.0 * scale).round()
        };
        let max_line = editor.line_offsets.len().min(first_line + visible_lines);

        for line_idx in first_line..max_line {
            let Some(&line_start) = editor.line_offsets.get(line_idx) else {
                continue;
            };
            let line_end = crate::app::project_search::project_search_line_end(
                &text,
                line_start,
                editor
                    .line_offsets
                    .get(line_idx + 1)
                    .copied()
                    .unwrap_or(text.len()),
            );
            let visual_idx = line_idx - first_line;
            let text_y = project_search_input_line_y(rect.y, visual_idx, line_h, scale)
                - line_offset_y.round();
            if let Some(line_text) = text.get(line_start..line_end) {
                if enabled && sel_start < line_end && sel_end > line_start {
                    let row_sel_start = sel_start.max(line_start).min(line_end);
                    let row_sel_end = sel_end.max(line_start).min(line_end);
                    let prefix = text.get(line_start..row_sel_start).unwrap_or("");
                    let selected = text.get(row_sel_start..row_sel_end).unwrap_or("");
                    let x1 = draw_x + self.project_search_stable_text_width(prefix, text_scale);
                    let sw = self.project_search_stable_text_width(selected, text_scale);
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
                let text_color = if enabled {
                    self.theme.fg
                } else {
                    [0.48, 0.49, 0.54, 1.0]
                };
                self.draw_project_search_text_stable(line_text, draw_x, text_y, text_color, text_scale);
            }

            if enabled
                && focused
                && blink_alpha > 0.5
                && editor.cursor >= line_start
                && editor.cursor <= line_end
            {
                let prefix = text.get(line_start..editor.cursor).unwrap_or("");
                let cursor_x = draw_x + self.project_search_stable_text_width(prefix, text_scale);
                self.push_rect(
                    cursor_x,
                    (text_y - 13.0 * scale).round(),
                    (2.0 * scale).max(1.0),
                    (line_h - 2.0 * scale).max(1.0),
                    self.theme.fg,
                );
            }
        }

        if enabled
            && focused
            && blink_alpha > 0.5
            && text.is_empty()
            && first_line == 0
        {
            let text_y = project_search_input_line_y(rect.y, 0, line_h, scale)
                - line_offset_y.round();
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
        if let Some(state) = query_state {
            self.draw_project_search_query_scrollbars(rect, state, ui_registry, scale);
        }
    }

    fn draw_project_search_query_scrollbars(
        &mut self,
        rect: ProjectSearchRect,
        state: &crate::app::project_search::ProjectSearchState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        scale: f32,
    ) {
        let viewport = crate::app::project_search::project_search_query_viewport(rect, scale);
        for (axis, id, track) in [
            (
                ProjectSearchQueryScrollAxis::Vertical,
                crate::ui_system::UiId::ProjectSearchQueryScrollbarY,
                viewport.vertical_track,
            ),
            (
                ProjectSearchQueryScrollAxis::Horizontal,
                crate::ui_system::UiId::ProjectSearchQueryScrollbarX,
                viewport.horizontal_track,
            ),
        ] {
            let Some(thumb) = crate::app::project_search::project_search_query_scrollbar_thumb(
                rect, state, axis, scale,
            ) else {
                continue;
            };
            ui_registry.register_rect(
                id,
                track.x,
                track.y,
                track.w,
                track.h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            self.push_rounded_rect(
                track.x.round(),
                track.y.round(),
                track.w,
                track.h,
                3.0 * scale,
                [1.0, 1.0, 1.0, 0.035],
            );
            self.push_rounded_rect(
                thumb.x.round(),
                thumb.y.round(),
                thumb.w,
                thumb.h,
                3.0 * scale,
                [0.48, 0.48, 0.56, 0.68],
            );
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
                let hint = if state.filter_active() {
                    "Нет файлов по фильтру"
                } else if state.total_matches == 0 {
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
        self.draw_project_search_scrollbar(layout, state, ui_registry, scale);

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
        if !mat.preview_ready {
            self.draw_project_search_text_stable(
                "...",
                preview_x.round(),
                text_y,
                [0.50, 0.52, 0.58, 1.0],
                0.78,
            );
            return;
        }
        scratch.clear();
        if mat.extra_lines > 0 {
            let _ = write!(scratch, "(+{}) ", mat.extra_lines);
        }
        let highlight_start = scratch.len() + mat.preview_match_start;
        let highlight_end = scratch.len() + mat.preview_match_end;
        scratch.push_str(&mat.preview);
        let max_w = (layout.list.x + layout.list.w - preview_x - 16.0 * scale).max(0.0);
        self.draw_project_search_match_preview(
            scratch.as_str(),
            highlight_start,
            highlight_end,
            preview_x.round(),
            text_y,
            max_w,
            0.78,
            clip_scratch,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_project_search_match_preview(
        &mut self,
        text: &str,
        highlight_start: usize,
        highlight_end: usize,
        x: f32,
        y: f32,
        max_w: f32,
        scale: f32,
        scratch: &mut String,
    ) {
        if max_w <= 0.0 {
            return;
        }
        scratch.clear();
        let (visible, hs, he) =
            project_search_visible_match_preview(text, highlight_start, highlight_end, max_w, |ch| {
                self.get_ui_glyph(ch)
                    .map(|g| g.advance * scale)
                    .unwrap_or(0.0)
            });
        scratch.push_str(&visible);
        if hs < he && scratch.is_char_boundary(hs) && scratch.is_char_boundary(he) {
            let before = &scratch[..hs];
            let hit = &scratch[hs..he];
            let hx = x + self.measure_ui_width(before, scale).round();
            let hw = self.measure_ui_width(hit, scale).round();
            if hw > 0.0 {
                self.push_rounded_rect(
                    hx.round(),
                    (y - 13.0 * self.scale_factor).round(),
                    hw,
                    17.0 * self.scale_factor,
                    3.0 * self.scale_factor,
                    [0.741, 0.576, 0.976, 0.26],
                );
            }
        }
        self.draw_string_scaled_stable(scratch, x, y, [0.75, 0.77, 0.82, 1.0], scale);
        if hs < he && scratch.is_char_boundary(hs) && scratch.is_char_boundary(he) {
            let before = &scratch[..hs];
            let hit = &scratch[hs..he];
            let hx = x + self.measure_ui_width(before, scale).round();
            self.draw_string_scaled_stable(hit, hx.round(), y, [0.98, 0.95, 1.0, 1.0], scale);
        }
    }

    fn draw_project_search_scrollbar(
        &mut self,
        layout: &ProjectSearchLayout,
        state: &crate::app::project_search::ProjectSearchState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        scale: f32,
    ) {
        let Some(thumb) =
            crate::app::project_search::project_search_scrollbar_thumb(layout, state, scale)
        else {
            return;
        };
        if state.running_generation.is_some() || !state.has_run {
            return;
        }
        ui_registry.register_rect(
            crate::ui_system::UiId::ProjectSearchScrollbar,
            layout.list.x + layout.list.w - 14.0 * scale,
            layout.list.y,
            12.0 * scale,
            layout.list.h,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        self.push_rounded_rect(
            thumb.x.round(),
            thumb.y.round(),
            thumb.w,
            thumb.h,
            3.0 * scale,
            [0.48, 0.48, 0.56, 0.55],
        );
    }
}
