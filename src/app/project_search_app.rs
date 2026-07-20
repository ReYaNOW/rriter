use crate::app::project_search::{
    ProjectSearchField, ProjectSearchLayout, ProjectSearchQueryScrollAxis, ProjectSearchRequest,
    project_search_layout, project_search_line_end, project_search_query_viewport,
    start_project_search_worker_cancellable,
};
use std::path::PathBuf;

pub(crate) fn project_search_field_for_ui_id(
    id: crate::ui_system::UiId,
) -> Option<ProjectSearchField> {
    match id {
        crate::ui_system::UiId::ProjectSearchQueryInput => Some(ProjectSearchField::Query),
        crate::ui_system::UiId::ProjectSearchIncludeInput => Some(ProjectSearchField::Include),
        crate::ui_system::UiId::ProjectSearchExcludeInput => Some(ProjectSearchField::Exclude),
        crate::ui_system::UiId::ProjectSearchFilterInput => Some(ProjectSearchField::Filter),
        _ => None,
    }
}

impl crate::app::App {
    pub fn open_project_search_panel(&mut self) {
        self.ide_panel.open(crate::app::PanelId::Search);
        self.ide_panel.project_search.focused = Some(ProjectSearchField::Query);
        self.search_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        self.sync_project_search_query_scroll(true);
        crate::save_panel_state(&self.ide_panel);
    }

    pub fn start_project_search(&mut self) {
        let query = self.ide_panel.project_search.query_editor.get_full_text();
        self.ide_panel.project_search.cancel_running_worker();
        let generation = self.ide_panel.project_search.advance_generation();
        self.ide_panel.project_search.has_run = true;
        self.ide_panel.project_search.error = None;
        self.ide_panel.project_search.elapsed_ms = None;
        self.ide_panel.project_search.capped = false;
        self.ide_panel.project_search.results.clear();
        self.ide_panel.project_search.flat_rows.clear();
        self.ide_panel.project_search.collapsed.clear();
        self.ide_panel.project_search.reset_preview_worker();
        self.ide_panel.project_search.total_matches = 0;
        self.ide_panel.project_search.scroll.reset();
        if self.ide_panel.project_search.focused == Some(ProjectSearchField::Filter) {
            self.ide_panel.project_search.focused = None;
            self.ide_panel.project_search.dragging_field = None;
        }
        if query.is_empty() {
            self.ide_panel.project_search.running_generation = None;
            self.ide_panel.project_search.rx = None;
            self.ide_panel.project_search.worker_cancel = None;
            return;
        }
        let request = ProjectSearchRequest {
            generation,
            query,
            include: self.ide_panel.project_search.include_editor.get_full_text(),
            exclude: self.ide_panel.project_search.exclude_editor.get_full_text(),
            case_sensitive: self.ide_panel.project_search.case_sensitive,
            workspaces: self.ide_workspaces.clone(),
            ignore_patterns: self.ide_ignore_patterns.clone(),
        };
        self.ide_panel.project_search.running_generation = Some(generation);
        let (rx, cancel) = start_project_search_worker_cancellable(request);
        self.ide_panel.project_search.rx = Some(rx);
        self.ide_panel.project_search.worker_cancel = Some(cancel);
        self.ide_panel.project_search.start_preview_worker();
        self.ide_panel.project_search.dirty = false;
    }

    pub fn poll_project_search(&mut self) -> bool {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = &self.ide_panel.project_search.rx {
            loop {
                match rx.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut updated = false;
        for message in messages {
            updated |= self.ide_panel.project_search.apply_message(message);
        }
        if disconnected {
            updated |= self.ide_panel.project_search.handle_worker_disconnect();
        }
        updated
    }

    pub fn poll_project_search_previews(&mut self) -> bool {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = &self.ide_panel.project_search.preview_rx {
            loop {
                match rx.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut updated = false;
        for message in messages {
            updated |= self.ide_panel.project_search.apply_preview_message(message);
        }
        if disconnected {
            self.ide_panel.project_search.handle_preview_disconnect();
            updated = true;
        }
        updated
    }

    pub fn queue_visible_project_search_previews(&mut self) -> bool {
        let Some(layout) = self.project_search_panel_layout() else {
            return false;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        self.ide_panel
            .project_search
            .queue_visible_preview_requests(&layout, renderer.scale_factor)
    }

    pub fn project_search_has_pending_previews(&self) -> bool {
        self.ide_panel.project_search.has_pending_previews()
    }

    pub fn start_project_search_scrollbar_drag(&mut self, mouse_y: f32) -> bool {
        let Some(layout) = self.project_search_panel_layout() else {
            return false;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        self.ide_panel
            .project_search
            .start_scrollbar_drag(&layout, mouse_y, renderer.scale_factor)
    }

    pub fn drag_project_search_scrollbar_to(&mut self, mouse_y: f32) -> bool {
        let Some(layout) = self.project_search_panel_layout() else {
            return false;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        self.ide_panel
            .project_search
            .drag_scrollbar_to(&layout, mouse_y, renderer.scale_factor)
    }

    pub(crate) fn start_project_search_query_scrollbar_drag(
        &mut self,
        axis: ProjectSearchQueryScrollAxis,
        pointer: f32,
    ) -> bool {
        let Some(layout) = self.project_search_panel_layout() else {
            return false;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        self.ide_panel.project_search.start_query_scrollbar_drag(
            layout.query,
            axis,
            pointer,
            renderer.scale_factor,
        )
    }

    pub(crate) fn drag_project_search_query_scrollbar_to(
        &mut self,
        axis: ProjectSearchQueryScrollAxis,
        pointer: f32,
    ) -> bool {
        let Some(layout) = self.project_search_panel_layout() else {
            return false;
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        self.ide_panel.project_search.drag_query_scrollbar_to(
            layout.query,
            axis,
            pointer,
            renderer.scale_factor,
        )
    }

    pub(crate) fn sync_project_search_query_scroll(&mut self, refresh_content_width: bool) {
        let Some(layout) = self.project_search_panel_layout() else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let scale = renderer.scale_factor;
        let state = &mut self.ide_panel.project_search;
        let text = state.query_editor.get_full_text();

        if refresh_content_width {
            let mut max_width = 0.0f32;
            for line_idx in 0..state.query_editor.line_offsets.len() {
                let line_start = state
                    .query_editor
                    .line_offsets
                    .get(line_idx)
                    .copied()
                    .unwrap_or(0);
                let line_end = project_search_line_end(
                    &text,
                    line_start,
                    state
                        .query_editor
                        .line_offsets
                        .get(line_idx + 1)
                        .copied()
                        .unwrap_or(text.len()),
                );
                if let Some(line_text) = text.get(line_start..line_end) {
                    max_width =
                        max_width.max(renderer.project_search_stable_text_width(line_text, 0.82));
                }
            }
            state.query_content_width = max_width + (2.0 * scale).max(1.0);
        }

        let cursor_line = state
            .query_editor
            .line_offsets
            .partition_point(|&offset| offset <= state.query_editor.cursor)
            .saturating_sub(1);
        let line_start = state
            .query_editor
            .line_offsets
            .get(cursor_line)
            .copied()
            .unwrap_or(0);
        let cursor = state.query_editor.cursor.min(text.len());
        let cursor_x = text
            .get(line_start.min(cursor)..cursor)
            .map(|prefix| renderer.project_search_stable_text_width(prefix, 0.82))
            .unwrap_or(0.0);
        state.reveal_query_cursor(layout.query, scale, cursor_x);
    }

    pub fn project_search_panel_layout(&self) -> Option<ProjectSearchLayout> {
        if !self.is_ide_mode || !self.ide_panel.is_open(crate::app::PanelId::Search) {
            return None;
        }
        let renderer = self.renderer.as_ref()?;
        let scale = renderer.scale_factor;
        let (panel_x, panel_y, panel_w, panel_h, _) =
            crate::app::mouse::app_panel_scroll_rect(self, crate::app::PanelId::Search, scale);
        Some(project_search_layout(
            panel_x, panel_y, panel_w, panel_h, scale,
        ))
    }

    pub fn focus_project_search_field(&mut self, field: ProjectSearchField) {
        if field == ProjectSearchField::Filter && !self.ide_panel.project_search.filter_enabled() {
            self.ide_panel.project_search.focused = None;
            return;
        }
        self.ide_panel.project_search.focused = Some(field);
        self.search_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        self.ide_panel.lsp_logs_focused = None;
        self.place_project_search_cursor_from_mouse(field);
    }

    pub fn place_project_search_cursor_from_mouse(&mut self, field: ProjectSearchField) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        self.set_project_search_cursor_at(
            field,
            renderer.last_mouse_x,
            renderer.last_mouse_y,
            true,
        );
    }

    pub fn drag_project_search_cursor_to(&mut self, field: ProjectSearchField, x: f32, y: f32) {
        self.set_project_search_cursor_at(field, x, y, false);
    }

    fn set_project_search_cursor_at(
        &mut self,
        field: ProjectSearchField,
        mouse_x: f32,
        mouse_y: f32,
        reset_anchor: bool,
    ) {
        if field == ProjectSearchField::Filter && !self.ide_panel.project_search.filter_enabled() {
            return;
        }
        let Some(layout) = self.project_search_panel_layout() else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let rect = match field {
            ProjectSearchField::Query => layout.query,
            ProjectSearchField::Include => layout.include,
            ProjectSearchField::Exclude => layout.exclude,
            ProjectSearchField::Filter => layout.filter,
        };
        let text_scale = 0.82;
        let line_h =
            crate::app::project_search::project_search_query_line_height(renderer.scale_factor);
        let query_viewport = project_search_query_viewport(rect, renderer.scale_factor);
        let query_scroll_x = self.ide_panel.project_search.query_scroll_x.current.round();
        let query_scroll_y = self.ide_panel.project_search.query_scroll_y.current.round();
        let editor = match field {
            ProjectSearchField::Query => &mut self.ide_panel.project_search.query_editor,
            ProjectSearchField::Include => &mut self.ide_panel.project_search.include_editor,
            ProjectSearchField::Exclude => &mut self.ide_panel.project_search.exclude_editor,
            ProjectSearchField::Filter => &mut self.ide_panel.project_search.filter_editor,
        };
        let text = editor.get_full_text();
        let line = if field == ProjectSearchField::Query {
            (((mouse_y - query_viewport.text.y + query_scroll_y).max(0.0) / line_h).floor()
                as usize)
                .min(editor.line_offsets.len().saturating_sub(1))
        } else {
            0
        };
        let line_start = editor.line_offsets.get(line).copied().unwrap_or(0);
        let line_end = project_search_line_end(
            &text,
            line_start,
            editor
                .line_offsets
                .get(line + 1)
                .copied()
                .unwrap_or(text.len()),
        );
        let x_offset = if field == ProjectSearchField::Query {
            (mouse_x - query_viewport.text.x + query_scroll_x).max(0.0)
        } else {
            (mouse_x - (rect.x + 7.0 * renderer.scale_factor)).max(0.0)
        };
        let mut current_x = 0.0;
        let mut target = line_end;
        if let Some(line_text) = text.get(line_start..line_end) {
            for (rel_idx, ch) in line_text.char_indices() {
                let adv = renderer
                    .get_ui_glyph(ch)
                    .map(|glyph| {
                        crate::renderer::Renderer::snapped_text_advance(glyph.advance, text_scale)
                    })
                    .unwrap_or_else(|| {
                        crate::renderer::Renderer::snapped_text_advance(10.0, text_scale)
                    });
                if x_offset <= current_x + adv * 0.5 {
                    target = line_start + rel_idx;
                    break;
                }
                current_x += adv;
            }
        }
        editor.cursor = target;
        if reset_anchor || editor.selection_anchor.is_none() {
            editor.selection_anchor = Some(target);
        }
        if field == ProjectSearchField::Query {
            self.sync_project_search_query_scroll(false);
        }
    }

    pub fn handle_project_search_match_click(&mut self, file_idx: usize, match_idx: usize) {
        let Some((path, start_line, start_col, end_line, end_col)) = self
            .ide_panel
            .project_search
            .results
            .get(file_idx)
            .and_then(|file| {
                file.matches.get(match_idx).map(|mat| {
                    (
                        file.path.clone(),
                        mat.start_line,
                        mat.start_col,
                        mat.end_line,
                        mat.end_col,
                    )
                })
            })
        else {
            return;
        };
        let absolute = self.abs_path_for_workspace(&path);
        let was_active = self
            .current_abs_path()
            .as_ref()
            .is_some_and(|current| crate::platform::paths_equal(current, &absolute));
        self.jump_to_project_search_position(path, true, start_line, start_col, end_line, end_col);
        if !was_active {
            self.scroll_y.jump_to(self.scroll_y.target);
            self.scroll_x.jump_to(self.scroll_x.target);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn jump_to_project_search_position(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) {
        let was_open = self.path_is_open_in_tabs(&path);
        self.open_file_in_tab_internal_options(path, add_to_history, was_open, was_open);
        let text = self.editor.get_full_text();
        let (start, end) =
            project_search_offsets_for_position(&text, start_line, start_col, end_line, end_col);
        self.editor.selection_anchor = Some(start);
        self.editor.cursor = end;
        if !was_open {
            self.reprioritize_highlighter_around_cursor();
            self.wait_for_current_highlight();
        }
        self.scroll_cursor_near_center(0.45, !was_open);
    }
}

fn project_search_offsets_for_position(
    text: &str,
    start_line: u32,
    start_col: u32,
    end_line: u32,
    end_col: u32,
) -> (usize, usize) {
    let start = crate::lsp::lsp_pos_to_offset(text, start_line, start_col);
    let end = crate::lsp::lsp_pos_to_offset(text, end_line, end_col);
    (start.min(end), start.max(end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_search_positions_map_utf16_columns_in_normalized_text() {
        let text = "first\na😀needle\nlast";
        let (start, end) = project_search_offsets_for_position(text, 1, 3, 1, 9);
        assert_eq!(&text[start..end], "needle");

        let (start, end) = project_search_offsets_for_position(text, 2, 4, 1, 3);
        assert!(start <= end);
        assert_eq!(start, crate::lsp::lsp_pos_to_offset(text, 1, 3));
    }

    #[test]
    fn project_search_field_ui_ids_are_the_only_focus_targets() {
        assert_eq!(
            project_search_field_for_ui_id(crate::ui_system::UiId::ProjectSearchQueryInput),
            Some(ProjectSearchField::Query)
        );
        assert_eq!(
            project_search_field_for_ui_id(crate::ui_system::UiId::ProjectSearchIncludeInput),
            Some(ProjectSearchField::Include)
        );
        assert_eq!(
            project_search_field_for_ui_id(crate::ui_system::UiId::ProjectSearchExcludeInput),
            Some(ProjectSearchField::Exclude)
        );
        assert_eq!(
            project_search_field_for_ui_id(crate::ui_system::UiId::ProjectSearchFilterInput),
            Some(ProjectSearchField::Filter)
        );
        assert_eq!(
            project_search_field_for_ui_id(crate::ui_system::UiId::ProjectSearchPanelBody),
            None
        );
        assert_eq!(
            project_search_field_for_ui_id(crate::ui_system::UiId::ProjectSearchRun),
            None
        );
    }
}
