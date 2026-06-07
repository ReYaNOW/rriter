use crate::app::project_search::{
    ProjectSearchField, ProjectSearchLayout, ProjectSearchRequest, project_search_layout,
    start_project_search_worker,
};
use std::path::PathBuf;

impl crate::app::App {
    pub fn open_project_search_panel(&mut self) {
        self.ide_panel.open(crate::app::PanelId::Search);
        self.ide_panel.project_search.focused = Some(ProjectSearchField::Query);
        self.search_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        crate::save_panel_state(&self.ide_panel);
    }

    pub fn start_project_search(&mut self) {
        let query = self.ide_panel.project_search.query_editor.get_full_text();
        self.ide_panel.project_search.generation =
            self.ide_panel.project_search.generation.saturating_add(1);
        let generation = self.ide_panel.project_search.generation;
        self.ide_panel.project_search.has_run = true;
        self.ide_panel.project_search.error = None;
        self.ide_panel.project_search.elapsed_ms = None;
        self.ide_panel.project_search.capped = false;
        self.ide_panel.project_search.results.clear();
        self.ide_panel.project_search.flat_rows.clear();
        self.ide_panel.project_search.collapsed.clear();
        self.ide_panel.project_search.reset_preview_worker();
        self.ide_panel.project_search.total_matches = 0;
        self.ide_panel.project_search.scroll.target = 0.0;
        self.ide_panel.project_search.scroll.current = 0.0;
        if self.ide_panel.project_search.focused == Some(ProjectSearchField::Filter) {
            self.ide_panel.project_search.focused = None;
            self.ide_panel.project_search.dragging_field = None;
        }
        if query.is_empty() {
            self.ide_panel.project_search.running_generation = None;
            self.ide_panel.project_search.rx = None;
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
        self.ide_panel.project_search.rx = Some(start_project_search_worker(request));
        self.ide_panel.project_search.start_preview_worker();
        self.ide_panel.project_search.dirty = false;
    }

    pub fn poll_project_search(&mut self) -> bool {
        let mut messages = Vec::new();
        if let Some(rx) = &self.ide_panel.project_search.rx {
            while let Ok(message) = rx.try_recv() {
                messages.push(message);
            }
        }
        if messages.is_empty() {
            return false;
        }
        let mut updated = false;
        for message in messages {
            updated |= self.ide_panel.project_search.apply_message(message);
        }
        updated
    }

    pub fn poll_project_search_previews(&mut self) -> bool {
        let mut messages = Vec::new();
        if let Some(rx) = &self.ide_panel.project_search.preview_rx {
            while let Ok(message) = rx.try_recv() {
                messages.push(message);
            }
        }
        if messages.is_empty() {
            return false;
        }
        let mut updated = false;
        for message in messages {
            updated |= self.ide_panel.project_search.apply_preview_message(message);
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

    pub fn project_search_panel_layout(&self) -> Option<ProjectSearchLayout> {
        if !self.is_ide_mode || !self.ide_panel.is_open(crate::app::PanelId::Search) {
            return None;
        }
        let renderer = self.renderer.as_ref()?;
        let scale = renderer.scale_factor;
        let wh = self
            .window
            .as_ref()
            .map(|window| window.inner_size().height as f32)
            .unwrap_or(renderer.height);
        let panel_bottom_h = if self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * scale
        } else {
            0.0
        };
        let content_bottom = crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, scale);
        Some(project_search_layout(
            48.0 * scale,
            32.0 * scale,
            self.ide_panel.left_width * scale,
            (content_bottom - 32.0 * scale).max(0.0),
            scale,
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
        let line_h = (18.0 * renderer.scale_factor).round().max(1.0);
        let editor = match field {
            ProjectSearchField::Query => &mut self.ide_panel.project_search.query_editor,
            ProjectSearchField::Include => &mut self.ide_panel.project_search.include_editor,
            ProjectSearchField::Exclude => &mut self.ide_panel.project_search.exclude_editor,
            ProjectSearchField::Filter => &mut self.ide_panel.project_search.filter_editor,
        };
        let text = editor.get_full_text();
        let visual_line = if field == ProjectSearchField::Query {
            ((mouse_y - rect.y - 5.0 * renderer.scale_factor).max(0.0) / line_h) as usize
        } else {
            0
        };
        let visible_lines = if field == ProjectSearchField::Query {
            ((rect.h - 8.0 * renderer.scale_factor) / line_h)
                .floor()
                .max(1.0) as usize
        } else {
            1
        };
        let cursor_line = editor
            .line_offsets
            .partition_point(|&offset| offset <= editor.cursor)
            .saturating_sub(1);
        let first_line = if field == ProjectSearchField::Query {
            cursor_line.saturating_sub(visible_lines.saturating_sub(1))
        } else {
            0
        };
        let line = (first_line + visual_line).min(editor.line_offsets.len().saturating_sub(1));
        let line_start = editor.line_offsets.get(line).copied().unwrap_or(0);
        let mut line_end = editor
            .line_offsets
            .get(line + 1)
            .copied()
            .unwrap_or(text.len())
            .min(text.len());
        if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\n') {
            line_end -= 1;
        }
        if line_end > line_start && text.as_bytes().get(line_end - 1) == Some(&b'\r') {
            line_end -= 1;
        }
        let x_offset = (mouse_x - (rect.x + 7.0 * renderer.scale_factor)).max(0.0);
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
    }

    pub fn handle_project_search_match_click(&mut self, file_idx: usize, match_idx: usize) {
        let Some((path, byte_start, byte_end)) = self
            .ide_panel
            .project_search
            .results
            .get(file_idx)
            .and_then(|file| {
                file.matches
                    .get(match_idx)
                    .map(|mat| (file.path.clone(), mat.byte_start, mat.byte_end))
            })
        else {
            return;
        };
        let was_active =
            self.current_abs_path().as_ref() == Some(&self.abs_path_for_workspace(&path));
        self.jump_to_project_search_byte_range(path, true, byte_start, byte_end);
        if !was_active {
            self.scroll_y.current = self.scroll_y.target;
            self.scroll_y.velocity = 0.0;
            self.scroll_x.current = self.scroll_x.target;
            self.scroll_x.velocity = 0.0;
        }
    }

    fn jump_to_project_search_byte_range(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        byte_start: usize,
        byte_end: usize,
    ) {
        let was_open = self.path_is_open_in_tabs(&path);
        self.open_file_in_tab_internal_options(path, add_to_history, was_open, was_open);
        let text = self.editor.get_full_text();
        let start = floor_char_boundary_for_project_search(&text, byte_start.min(text.len()));
        let end = ceil_char_boundary_for_project_search(&text, byte_end.min(text.len()));
        self.editor.selection_anchor = Some(start.min(end));
        self.editor.cursor = end.max(start);
        if !was_open {
            self.reprioritize_highlighter_around_cursor();
            self.wait_for_current_highlight();
        }
        self.scroll_cursor_near_center(0.45, !was_open);
    }
}

fn floor_char_boundary_for_project_search(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary_for_project_search(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}
