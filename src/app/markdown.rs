use std::ops::Range;

use super::{App, EditorTabKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkdownMode {
    #[default]
    Edit,
    Read,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkdownReadWheelResult {
    NotRead,
    Blocked,
    Scrolled,
}

pub(crate) fn scroll_markdown_read(
    read_scroll: &mut crate::scroll::ScrollState,
    read_max_scroll: f32,
    dy: f32,
) {
    read_scroll.anim_speed = 7.0;
    read_scroll.scroll_by(dy);
    read_scroll.clamp_target(0.0, read_max_scroll);
}

pub(crate) fn handle_markdown_read_wheel(
    mode: MarkdownMode,
    hovered: Option<crate::ui_system::UiId>,
    read_scroll: &mut crate::scroll::ScrollState,
    read_max_scroll: f32,
    dy: f32,
) -> MarkdownReadWheelResult {
    if mode != MarkdownMode::Read {
        return MarkdownReadWheelResult::NotRead;
    }
    if !matches!(
        hovered,
        Some(
            crate::ui_system::UiId::MarkdownReadBody
                | crate::ui_system::UiId::MarkdownCodeCopy(_)
        )
    ) {
        return MarkdownReadWheelResult::Blocked;
    }
    scroll_markdown_read(read_scroll, read_max_scroll, dy);
    MarkdownReadWheelResult::Scrolled
}

pub(crate) fn is_markdown_extension(extension: &str) -> bool {
    matches!(extension, "md" | "markdown")
}

pub struct MarkdownTabState {
    pub mode: MarkdownMode,
    pub read_scroll_y: crate::scroll::ScrollState,
    pub(crate) read_model: Option<crate::languages::markdown::MarkdownDocument>,
    pub(crate) read_source: String,
    pub(crate) read_model_version: Option<u64>,
    pub(crate) read_parser: Option<crate::languages::markdown::MarkdownParseState>,
    #[cfg(test)]
    parser_creation_count: usize,
    #[cfg(test)]
    semantic_refresh_count: usize,
    pub(crate) read_layout: crate::render_view::markdown_read::MarkdownReadLayoutCache,
    pub(crate) read_max_scroll: f32,
    pub(crate) read_selection_anchor: Option<usize>,
    pub(crate) read_selection_cursor: Option<usize>,
    pub(crate) read_selecting: bool,
    pub(crate) copied_code_block: Option<usize>,
    pub(crate) code_copy_hover_valid: bool,
}

impl Default for MarkdownTabState {
    fn default() -> Self {
        Self {
            mode: MarkdownMode::Edit,
            read_scroll_y: crate::scroll::ScrollState::new(15.0),
            read_model: None,
            read_source: String::new(),
            read_model_version: None,
            read_parser: None,
            #[cfg(test)]
            parser_creation_count: 0,
            #[cfg(test)]
            semantic_refresh_count: 0,
            read_layout: crate::render_view::markdown_read::MarkdownReadLayoutCache::default(),
            read_max_scroll: 0.0,
            read_selection_anchor: None,
            read_selection_cursor: None,
            read_selecting: false,
            copied_code_block: None,
            code_copy_hover_valid: false,
        }
    }
}

impl MarkdownTabState {
    pub(crate) fn refresh_read_model(&mut self, version: u64, source: String) -> bool {
        #[cfg(test)]
        {
            self.semantic_refresh_count = self.semantic_refresh_count.saturating_add(1);
        }
        if self.read_model_version == Some(version)
            && self.read_source == source
            && self.read_model.is_some()
        {
            return true;
        }

        self.clear_read_selection();
        self.clear_code_copy_transient();

        let edit = if self.read_parser.is_some() && self.read_model_version.is_some() {
            Some(markdown_replacement_edit(&self.read_source, &source))
        } else {
            None
        };
        if self.read_parser.is_none() {
            self.read_parser = Some(crate::languages::markdown::MarkdownParseState::default());
            #[cfg(test)]
            {
                self.parser_creation_count = self.parser_creation_count.saturating_add(1);
            }
        }
        let Some(parser) = self.read_parser.as_mut() else {
            return false;
        };
        if let Some(edit) = edit.as_ref() {
            parser.apply_edit(edit);
        }

        let parsed = parser.parse(&source);
        if let Some(document) = parsed {
            self.read_model = Some(document);
            self.read_source = source;
            self.read_model_version = Some(version);
        } else {
            self.read_model = None;
            self.read_source = source;
            self.read_model_version = Some(version);
            self.read_parser = None;
        }
        self.read_layout.invalidate();
        self.read_max_scroll = 0.0;
        self.read_model.is_some()
    }

    pub(crate) fn read_document(
        &self,
        version: u64,
    ) -> Option<&crate::languages::markdown::MarkdownDocument> {
        (self.read_model_version == Some(version))
            .then_some(self.read_model.as_ref())
            .flatten()
    }

    fn needs_read_model_refresh(&self, version: u64) -> bool {
        self.read_document(version).is_none()
    }

    pub(crate) fn read_selection_range(&self) -> Option<Range<usize>> {
        let anchor = self.read_selection_anchor?;
        let cursor = self.read_selection_cursor?;
        (anchor != cursor).then_some(anchor.min(cursor)..anchor.max(cursor))
    }

    pub(crate) fn begin_read_selection(&mut self, byte: usize) {
        let byte = byte.min(self.read_source.len());
        self.read_selection_anchor = Some(byte);
        self.read_selection_cursor = Some(byte);
        self.read_selecting = true;
    }

    pub(crate) fn update_read_selection(&mut self, byte: usize) {
        if self.read_selecting {
            self.read_selection_cursor = Some(byte.min(self.read_source.len()));
        }
    }

    pub(crate) fn finish_read_selection(&mut self) {
        self.read_selecting = false;
    }

    pub(crate) fn clear_read_selection(&mut self) {
        self.read_selection_anchor = None;
        self.read_selection_cursor = None;
        self.read_selecting = false;
    }

    pub(crate) fn mark_code_copy_hover_valid(&mut self) -> bool {
        let changed = !self.code_copy_hover_valid;
        self.code_copy_hover_valid = true;
        changed
    }

    pub(crate) fn update_code_copy_hover(&mut self, hovered_block: Option<usize>) -> bool {
        if self.copied_code_block.is_some() && self.copied_code_block != hovered_block {
            self.copied_code_block = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn clear_code_copy_transient(&mut self) -> bool {
        let changed = self.code_copy_hover_valid || self.copied_code_block.is_some();
        self.code_copy_hover_valid = false;
        self.copied_code_block = None;
        changed
    }

    pub(crate) fn update_read_scroll(&mut self, dt: f32) -> bool {
        let changed = self.read_scroll_y.update(dt);
        if changed {
            self.copied_code_block = None;
        }
        changed
    }

    pub(crate) fn selected_read_text(&self) -> Option<String> {
        let range = self.read_selection_range()?;
        let text = self
            .read_layout
            .copy_source_selection(self.read_source.as_str(), &range);
        (!text.is_empty()).then_some(text)
    }

    #[cfg(test)]
    fn parser_creation_count(&self) -> usize {
        self.parser_creation_count
    }

    #[cfg(test)]
    fn semantic_refresh_count(&self) -> usize {
        self.semantic_refresh_count
    }
}

fn markdown_replacement_edit(old: &str, new: &str) -> tree_sitter::InputEdit {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while start > 0 && (!old.is_char_boundary(start) || !new.is_char_boundary(start)) {
        start -= 1;
    }
    let max_suffix = old
        .len()
        .saturating_sub(start)
        .min(new.len().saturating_sub(start));
    let mut suffix = old.as_bytes()[old.len() - max_suffix..]
        .iter()
        .rev()
        .zip(new.as_bytes()[new.len() - max_suffix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while suffix > 0 {
        let old_end = old.len() - suffix;
        let new_end = new.len() - suffix;
        if old.is_char_boundary(old_end) && new.is_char_boundary(new_end) {
            break;
        }
        suffix -= 1;
    }
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    tree_sitter::InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: markdown_point(old, start),
        old_end_position: markdown_point(old, old_end),
        new_end_position: markdown_point(new, new_end),
    }
}

fn markdown_point(text: &str, byte: usize) -> tree_sitter::Point {
    let prefix = &text[..byte.min(text.len())];
    let row = prefix.as_bytes().iter().filter(|&&b| b == b'\n').count();
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len());
    tree_sitter::Point::new(row, column)
}

impl App {
    pub fn active_document_is_markdown(&self) -> bool {
        let markdown_extension = is_markdown_extension(&self.file_extension);
        let normal_document = !self.is_ide_mode
            || self
                .tabs
                .get(self.active_tab)
                .is_some_and(|tab| matches!(tab.kind, EditorTabKind::Normal));
        markdown_extension && normal_document && self.file_path.is_some()
    }

    pub fn markdown_mode(&self) -> MarkdownMode {
        if self.active_document_is_markdown() {
            self.markdown.mode
        } else {
            MarkdownMode::Edit
        }
    }

    pub fn set_markdown_mode(&mut self, mode: MarkdownMode) {
        if !self.active_document_is_markdown() || self.markdown.mode == mode {
            return;
        }
        self.markdown.clear_code_copy_transient();
        if mode == MarkdownMode::Read {
            self.close_autocomplete();
            self.lsp_actions_menu = None;
            self.pending_fix_all_id = None;
        }
        if mode == MarkdownMode::Read
            && self.markdown.needs_read_model_refresh(self.editor.version)
        {
            let source = self.editor.get_full_text();
            self.markdown.refresh_read_model(self.editor.version, source);
        }
        self.markdown.mode = mode;
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn toggle_markdown_mode(&mut self) {
        let next = match self.markdown_mode() {
            MarkdownMode::Edit => MarkdownMode::Read,
            MarkdownMode::Read => MarkdownMode::Edit,
        };
        self.set_markdown_mode(next);
    }

    pub(crate) fn refresh_markdown_read_model_if_stale(&mut self) -> bool {
        if self.markdown_mode() != MarkdownMode::Read
            || !self.markdown.needs_read_model_refresh(self.editor.version)
        {
            return false;
        }
        let source = self.editor.get_full_text();
        self.markdown.refresh_read_model(self.editor.version, source);
        true
    }

    pub(crate) fn begin_markdown_read_selection_at(&mut self, x: f32, y: f32) -> bool {
        if self.markdown_mode() != MarkdownMode::Read {
            return false;
        }
        let Some(frame) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        else {
            return false;
        };
        let version = self.editor.version;
        let byte = {
            let markdown = &self.markdown;
            let Some(renderer) = self.renderer.as_mut() else {
                return false;
            };
            renderer.markdown_read_source_byte_at(markdown, version, frame, x, y)
        };
        let Some(byte) = byte else {
            return false;
        };
        self.markdown.begin_read_selection(byte);
        self.is_dragging = false;
        self.is_editor_drag_pending = false;
        true
    }

    pub(crate) fn update_markdown_read_selection_at(&mut self, x: f32, y: f32) -> bool {
        if !self.markdown.read_selecting || self.markdown_mode() != MarkdownMode::Read {
            return false;
        }
        let Some(frame) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        else {
            return false;
        };
        let version = self.editor.version;
        let byte = {
            let markdown = &self.markdown;
            let Some(renderer) = self.renderer.as_mut() else {
                return false;
            };
            renderer.markdown_read_source_byte_at(markdown, version, frame, x, y)
        };
        let Some(byte) = byte else {
            return false;
        };
        self.markdown.update_read_selection(byte);
        true
    }

    pub(crate) fn copy_markdown_read_selection(&mut self) -> bool {
        let Some(text) = self.markdown.selected_read_text() else {
            return false;
        };
        self.set_clipboard_text(text);
        true
    }

    pub(crate) fn copy_markdown_read_code_block(&mut self, block_id: usize) -> bool {
        if self.markdown_mode() != MarkdownMode::Read {
            return false;
        }
        let Some(text) = self
            .markdown
            .read_layout
            .code_block_copy_text(self.markdown.read_source.as_str(), block_id)
        else {
            return false;
        };
        self.set_clipboard_text(text);
        self.markdown.copied_code_block = Some(block_id);
        true
    }

    pub(crate) fn update_markdown_code_copy_hover_at(&mut self, x: f32, y: f32) -> bool {
        if self.markdown_mode() != MarkdownMode::Read {
            return self.markdown.clear_code_copy_transient();
        }
        let Some(frame) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        else {
            return self.markdown.clear_code_copy_transient();
        };
        let mut changed = self.markdown.mark_code_copy_hover_valid();
        if self.markdown.copied_code_block.is_none() {
            return changed;
        }
        let hovered = self.renderer.as_ref().and_then(|renderer| {
            renderer.markdown_read_code_block_at(
                &self.markdown,
                self.editor.version,
                frame,
                x,
                y,
            )
        });
        changed |= self.markdown.update_code_copy_hover(hovered);
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::app_behavior_tests::{editor_with, tab_with, test_app};
    use std::path::PathBuf;

    #[test]
    fn markdown_tab_state_default_is_edit_and_parser_is_lazy() {
        let state = MarkdownTabState::default();
        assert_eq!(state.mode, MarkdownMode::Edit);
        assert!(state.read_parser.is_none());
        assert_eq!(state.parser_creation_count(), 0);
    }

    #[test]
    fn non_markdown_tab_never_creates_markdown_parser() {
        let tab = tab_with("note.txt", Some("/tmp/note.txt"), "plain text\n");
        assert!(tab.markdown.read_parser.is_none());
        assert_eq!(tab.markdown.parser_creation_count(), 0);

        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/note.txt"));
        app.file_extension = "txt".to_string();
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.markdown.mode, MarkdownMode::Edit);
        assert!(app.markdown.read_parser.is_none());
        assert_eq!(app.markdown.parser_creation_count(), 0);
    }

    #[test]
    fn markdown_edit_tab_keeps_parser_lazy_until_first_read() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n");

        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert!(app.markdown.read_parser.is_none());
        assert_eq!(app.markdown.parser_creation_count(), 0);

        app.set_markdown_mode(MarkdownMode::Read);
        assert!(app.markdown.read_parser.is_some());
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 1);
        assert!(app.markdown.read_document(app.editor.version).is_some());
    }

    #[test]
    fn markdown_unchanged_read_toggle_reuses_model_without_refresh() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n\ntext 😀\n");

        app.set_markdown_mode(MarkdownMode::Read);
        let version = app.editor.version;
        let model_ptr = app
            .markdown
            .read_document(version)
            .map(|document| document as *const _)
            .expect("first read model");
        assert_eq!(app.markdown.semantic_refresh_count(), 1);
        assert_eq!(app.markdown.parser_creation_count(), 1);

        app.set_markdown_mode(MarkdownMode::Edit);
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown.semantic_refresh_count(), 1);
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.read_model_version, Some(version));
        assert_eq!(
            app.markdown
                .read_document(version)
                .map(|document| document as *const _),
            Some(model_ptr)
        );
    }

    #[test]
    fn markdown_incremental_refresh_reuses_existing_parser() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n\ntext\n");
        app.set_markdown_mode(MarkdownMode::Read);
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 1);

        app.set_markdown_mode(MarkdownMode::Edit);
        app.editor.cursor = app.editor.len();
        let _ = app.editor.insert_str("\n> новое 😀\n");
        let edited_version = app.editor.version;
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 2);
        assert_eq!(app.markdown.read_model_version, Some(edited_version));
        assert!(app.markdown.read_document(edited_version).is_some());
    }

    #[test]
    fn markdown_missing_model_retries_with_fresh_parser() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n");
        app.set_markdown_mode(MarkdownMode::Read);
        let version = app.editor.version;
        assert_eq!(app.markdown.parser_creation_count(), 1);
        assert_eq!(app.markdown.semantic_refresh_count(), 1);

        // This is the state left by refresh_read_model after a parse failure:
        // version/source are known, but the model and parser are cleared.
        app.markdown.read_model = None;
        app.markdown.read_parser = None;
        app.set_markdown_mode(MarkdownMode::Edit);
        app.set_markdown_mode(MarkdownMode::Read);

        assert_eq!(app.markdown.parser_creation_count(), 2);
        assert_eq!(app.markdown.semantic_refresh_count(), 2);
        assert!(app.markdown.read_document(version).is_some());
    }

    #[test]
    fn markdown_read_wheel_scrolls_preview_and_blocks_hidden_source_path() {
        let mut read_scroll = crate::scroll::ScrollState::new(15.0);
        read_scroll.jump_to(40.0);
        let mut source_x = crate::scroll::ScrollState::new(15.0);
        let mut source_y = crate::scroll::ScrollState::new(15.0);
        source_x.jump_to(37.0);
        source_y.jump_to(213.0);
        let source_before = (
            source_x.current,
            source_x.target,
            source_y.current,
            source_y.target,
        );

        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Read,
                Some(crate::ui_system::UiId::MarkdownReadBody),
                &mut read_scroll,
                500.0,
                80.0,
            ),
            MarkdownReadWheelResult::Scrolled
        );
        assert!(read_scroll.target > 40.0);
        let before_copy_button_wheel = read_scroll.target;
        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Read,
                Some(crate::ui_system::UiId::MarkdownCodeCopy(123)),
                &mut read_scroll,
                500.0,
                80.0,
            ),
            MarkdownReadWheelResult::Scrolled
        );
        assert!(read_scroll.target > before_copy_button_wheel);
        assert_eq!(
            (
                source_x.current,
                source_x.target,
                source_y.current,
                source_y.target,
            ),
            source_before
        );

        let read_before = (read_scroll.current, read_scroll.target);
        for hovered in [
            None,
            Some(crate::ui_system::UiId::EditorTab(0)),
            Some(crate::ui_system::UiId::StatusBar),
        ] {
            assert_eq!(
                handle_markdown_read_wheel(
                    MarkdownMode::Read,
                    hovered,
                    &mut read_scroll,
                    500.0,
                    80.0,
                ),
                MarkdownReadWheelResult::Blocked
            );
        }
        assert_eq!((read_scroll.current, read_scroll.target), read_before);
        assert_eq!(
            (
                source_x.current,
                source_x.target,
                source_y.current,
                source_y.target,
            ),
            source_before
        );
    }

    #[test]
    fn markdown_edit_wheel_keeps_normal_source_route_available() {
        let mut read_scroll = crate::scroll::ScrollState::new(15.0);
        assert_eq!(
            handle_markdown_read_wheel(
                MarkdownMode::Edit,
                Some(crate::ui_system::UiId::MarkdownReadBody),
                &mut read_scroll,
                500.0,
                80.0,
            ),
            MarkdownReadWheelResult::NotRead
        );
    }

    #[test]
    fn markdown_read_round_trip_preserves_source_state_and_edit_scroll() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.show_welcome = false;
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# Заголовок 😀\n\nТекст **strong**.\n");
        app.editor.cursor = "# Заголовок".len();
        app.editor.selection_anchor = Some(0);
        app.scroll_y.jump_to(213.0);
        app.scroll_x.jump_to(37.0);
        app.markdown.read_scroll_y.jump_to(51.0);

        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        let cursor = app.editor.cursor;
        let selection = app.editor.selection_anchor;
        let source_scroll = (
            app.scroll_x.current,
            app.scroll_x.target,
            app.scroll_y.current,
            app.scroll_y.target,
        );

        app.set_markdown_mode(MarkdownMode::Read);
        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
        assert!(app.markdown.read_document(version).is_some());
        assert_eq!(app.markdown.read_scroll_y.current, 51.0);

        app.toggle_markdown_mode();
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert_eq!(app.editor.cursor, cursor);
        assert_eq!(app.editor.selection_anchor, selection);
        assert_eq!(
            (
                app.scroll_x.current,
                app.scroll_x.target,
                app.scroll_y.current,
                app.scroll_y.target,
            ),
            source_scroll
        );
    }

    #[test]
    fn markdown_mode_and_read_scroll_are_independent_between_tabs() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs = vec![
            tab_with("a.md", Some("/tmp/a.md"), "# A\n"),
            tab_with("b.md", Some("/tmp/b.md"), "# B\n"),
        ];
        app.active_tab = 0;
        app.sync_active_tab();

        app.set_markdown_mode(MarkdownMode::Read);
        app.markdown.read_scroll_y.jump_to(91.0);
        app.sync_active_tab();

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.markdown.read_scroll_y.current, 0.0);
        app.markdown.read_scroll_y.jump_to(17.0);
        app.sync_active_tab();

        app.active_tab = 0;
        app.sync_active_tab();
        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
        assert_eq!(app.markdown.read_scroll_y.current, 91.0);
        app.sync_active_tab();

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.markdown.read_scroll_y.current, 17.0);
    }

    #[test]
    fn markdown_preview_click_does_not_move_hidden_source_cursor_or_selection() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("abcdef\n");
        app.editor.cursor = 5;
        app.editor.selection_anchor = Some(2);
        app.is_dragging = true;
        app.is_editor_drag_pending = true;

        app.handle_ui_click(crate::ui_system::UiId::MarkdownReadBody);

        assert_eq!(app.editor.cursor, 5);
        assert_eq!(app.editor.selection_anchor, Some(2));
        assert!(!app.is_dragging);
        assert!(!app.is_editor_drag_pending);
    }

    #[test]
    fn markdown_reader_document_focus_handoff_clears_keyboard_owners_and_preserves_source_state() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.show_welcome = false;
        app.is_ide_mode = true;
        app.editor = editor_with("source text\n");
        app.editor.cursor = 8;
        app.editor.selection_anchor = Some(2);
        app.markdown.mode = MarkdownMode::Read;
        app.markdown.read_source = "source text\n".to_string();

        app.ide_panel.open(crate::app::PanelId::Terminal);
        app.ide_panel.open(crate::app::PanelId::ApiClient);
        app.ide_panel.terminal_focused = true;
        app.ide_panel.term_show_search = true;
        app.ide_panel.term_search_focused = true;
        app.show_search = true;
        app.search_focused = true;
        app.ide_panel.file_tree_focused = true;
        app.ide_panel.lsp_logs_focused = Some("rust-analyzer".to_string());
        app.ide_panel.lsp_log_filter_focused = true;
        app.ide_panel.git.message_focused = true;
        app.settings_ignore_focused = true;
        app.ide_panel.api.route_filter = "old".to_string();
        app.ide_panel.api.input_editor = editor_with("new filter");
        app.ide_panel.api.focused = Some(crate::app::api_client::ApiFocus::RouteFilter);

        let hidden_source = (app.editor.cursor, app.editor.selection_anchor);
        assert!(!app.editor_has_input_focus());

        app.focus_document_text_surface();
        app.markdown.begin_read_selection(1);
        app.markdown.update_read_selection(6);

        assert!(!app.ide_panel.terminal_focused);
        assert!(app.ide_panel.is_open(crate::app::PanelId::Terminal));
        assert!(!app.ide_panel.term_search_focused);
        assert!(!app.search_focused);
        assert!(app.show_search);
        assert!(!app.ide_panel.file_tree_focused);
        assert!(app.ide_panel.lsp_logs_focused.is_none());
        assert!(!app.ide_panel.lsp_log_filter_focused);
        assert!(!app.ide_panel.git.message_focused);
        assert!(!app.settings_ignore_focused);
        assert!(app.ide_panel.api.focused.is_none());
        assert_eq!(app.ide_panel.api.route_filter, "new filter");
        assert!(app.ide_panel.is_open(crate::app::PanelId::ApiClient));
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden_source);
        assert_eq!(app.markdown.read_selection_range(), Some(1..6));
        assert!(app.markdown.read_selecting);
        assert!(app.editor_has_input_focus());
    }

    #[test]
    fn markdown_reader_reverse_selection_is_independent_from_hidden_editor_selection() {
        let Some(mut app) = markdown_source_app("# Heading\n\nText **strong** with `code λ`.\n") else {
            return;
        };
        app.editor.cursor = 7;
        app.editor.selection_anchor = Some(2);
        let source = app.editor.get_full_text();
        let hidden = (app.editor.cursor, app.editor.selection_anchor);
        assert!(app.markdown.refresh_read_model(app.editor.version, source.clone()));
        app.markdown.read_layout =
            crate::render_view::markdown_read::build_test_markdown_read_layout(&source, 500.0);

        app.markdown.begin_read_selection(source.len());
        app.markdown.update_read_selection(0);
        app.markdown.finish_read_selection();

        assert_eq!(app.markdown.read_selection_range(), Some(0..source.len()));
        let copied = app.markdown.selected_read_text().expect("reader selection text");
        assert!(copied.contains("Heading"));
        assert!(copied.contains("Text strong with code λ."));
        assert!(!copied.contains("**"));
        assert!(!copied.contains('`'));
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);
    }

    #[test]
    fn markdown_reader_selection_is_per_tab() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs = vec![
            tab_with("a.md", Some("/tmp/a.md"), "alpha\n"),
            tab_with("b.md", Some("/tmp/b.md"), "bravo\n"),
        ];
        app.active_tab = 0;
        app.sync_active_tab();
        app.markdown.read_selection_anchor = Some(1);
        app.markdown.read_selection_cursor = Some(4);
        app.sync_active_tab();

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown.read_selection_range(), None);
        app.markdown.read_selection_anchor = Some(0);
        app.markdown.read_selection_cursor = Some(2);
        app.sync_active_tab();

        app.active_tab = 0;
        app.sync_active_tab();
        assert_eq!(app.markdown.read_selection_range(), Some(1..4));

        app.active_tab = 1;
        app.sync_active_tab();
        assert_eq!(app.markdown.read_selection_range(), Some(0..2));
    }

    #[test]
    fn markdown_code_copy_hover_lifecycle_clears_on_leave_and_reenters_as_copy() {
        let mut state = MarkdownTabState::default();

        assert!(state.mark_code_copy_hover_valid());
        assert!(state.code_copy_hover_valid);
        assert!(state.clear_code_copy_transient());
        assert!(!state.code_copy_hover_valid);
        assert_eq!(state.copied_code_block, None);

        assert!(state.mark_code_copy_hover_valid());
        state.copied_code_block = Some(10);
        assert!(!state.update_code_copy_hover(Some(10)));
        assert!(state.clear_code_copy_transient());
        assert!(!state.code_copy_hover_valid);
        assert_eq!(state.copied_code_block, None);

        assert!(state.mark_code_copy_hover_valid());
        assert!(!state.update_code_copy_hover(Some(10)));
        assert_eq!(state.copied_code_block, None);
        state.copied_code_block = Some(10);
        assert!(state.update_code_copy_hover(Some(20)));
        assert_eq!(state.copied_code_block, None);
    }

    #[test]
    fn markdown_code_copy_scroll_change_drops_check_without_losing_pointer_validity() {
        let mut state = MarkdownTabState::default();
        state.code_copy_hover_valid = true;
        state.copied_code_block = Some(10);
        state.read_scroll_y.animate_to(120.0);

        assert!(state.update_read_scroll(1.0 / 60.0));
        assert_eq!(state.copied_code_block, None);
        assert!(state.code_copy_hover_valid);

        state.read_scroll_y.animate_to(0.0);
        assert!(state.update_read_scroll(1.0 / 60.0));
        assert_eq!(state.copied_code_block, None);
        assert!(state.code_copy_hover_valid);
    }

    #[test]
    fn markdown_code_copy_transient_state_does_not_survive_tab_switch() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs = vec![
            tab_with("a.md", Some("/tmp/a.md"), "```rust\na\n```\n"),
            tab_with("b.md", Some("/tmp/b.md"), "```bash\nb\n```\n"),
        ];
        app.active_tab = 0;
        app.sync_active_tab();
        app.markdown.code_copy_hover_valid = true;
        app.markdown.copied_code_block = Some(1);

        app.switch_to_tab(1);
        assert!(!app.markdown.code_copy_hover_valid);
        assert_eq!(app.markdown.copied_code_block, None);

        app.switch_to_tab(0);
        assert!(!app.markdown.code_copy_hover_valid);
        assert_eq!(app.markdown.copied_code_block, None);
    }

    #[test]
    fn markdown_code_copy_does_not_touch_hidden_source_cursor_or_selection() {
        let source = "```rust\nlet x = 1;\n```\n";
        let Some(mut app) = markdown_source_app(source) else {
            return;
        };
        app.editor.cursor = 7;
        app.editor.selection_anchor = Some(2);
        let hidden = (app.editor.cursor, app.editor.selection_anchor);
        app.set_markdown_mode(MarkdownMode::Read);
        app.markdown.read_layout =
            crate::render_view::markdown_read::build_test_markdown_read_layout(source, 500.0);

        assert!(app.copy_markdown_read_code_block(0));
        assert_eq!(app.markdown.copied_code_block, Some(0));
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);
    }

    #[test]
    fn markdown_reader_search_scrolls_read_surface_without_touching_hidden_selection() {
        let mut source = String::from("# top\n\nfirst needle\n\n");
        for i in 0..120 {
            source.push_str("paragraph ");
            source.push_str(&i.to_string());
            source.push_str(" with padding words\n\n");
        }
        source.push_str("last needle\n");
        let Some(mut app) = markdown_source_app(&source) else {
            return;
        };
        app.editor.cursor = 5;
        app.editor.selection_anchor = Some(1);
        let hidden = (app.editor.cursor, app.editor.selection_anchor);
        app.set_markdown_mode(MarkdownMode::Read);
        app.markdown.read_layout =
            crate::render_view::markdown_read::build_test_markdown_read_layout(&source, 320.0);
        app.markdown.read_max_scroll = app.markdown.read_layout.content_height();

        let first = source.find("needle").expect("first match");
        let last = source.rfind("needle").expect("last match");
        app.search_results = vec![
            (first, first + "needle".len()),
            (last, last + "needle".len()),
        ];
        app.search_current_idx = Some(0);
        app.jump_to_search_result();
        let first_target = app.markdown.read_scroll_y.target;
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);

        app.search_current_idx = Some(1);
        app.jump_to_search_result();
        let last_target = app.markdown.read_scroll_y.target;
        assert!(last_target > first_target);
        assert_eq!((app.editor.cursor, app.editor.selection_anchor), hidden);
    }

    #[test]
    fn markdown_edit_search_still_selects_source_match() {
        let Some(mut app) = markdown_source_app("before needle after\n") else {
            return;
        };
        let start = app.editor.get_full_text().find("needle").expect("match");
        let end = start + "needle".len();
        app.search_results = vec![(start, end)];
        app.search_current_idx = Some(0);

        app.jump_to_search_result();

        assert_eq!(app.editor.cursor, end);
        assert_eq!(app.editor.selection_anchor, Some(start));
    }

    #[test]
    fn markdown_read_mode_is_unavailable_for_special_tabs() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.file_path = Some(PathBuf::from("/tmp/diff.md"));
        app.file_extension = "md".to_string();
        let mut special = tab_with("diff.md", Some("/tmp/diff.md"), "# diff\n");
        special.kind = EditorTabKind::GitDiff(
            crate::app::git_diff::GitDiffTabMeta {
                repo_root: PathBuf::from("/tmp"),
                rel_path: "diff.md".to_string(),
                old_rel_path: None,
                status: crate::app::git_panel::GitFileStatus::Modified,
                workspace_idx: 0,
            },
            crate::app::git_diff::GitDiffState::loading(1),
        );
        app.tabs.push(special);
        app.active_tab = 0;

        app.set_markdown_mode(MarkdownMode::Read);

        assert!(!app.active_document_is_markdown());
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.markdown.mode, MarkdownMode::Edit);
        assert!(app.markdown.read_parser.is_none());
    }
    #[test]
    fn markdown_mode_toggle_ui_click_uses_central_mode_api() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.editor = editor_with("# title\n");

        app.handle_ui_click(crate::ui_system::UiId::MarkdownModeToggle);
        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
        app.handle_ui_click(crate::ui_system::UiId::MarkdownModeToggle);
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
    }

    fn markdown_source_app(text: &str) -> Option<App> {
        let mut app = test_app()?;
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        app.base_title = "readme.md".to_string();
        app.editor = editor_with(text);
        Some(app)
    }

    fn workspace_edit_replacing_first_line(
        path: &std::path::Path,
        replacement: &str,
    ) -> crate::lsp::WorkspaceEdit {
        let mut changes = std::collections::HashMap::new();
        changes.insert(
            path.to_path_buf(),
            vec![crate::lsp::TextChange {
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 7,
                new_text: replacement.to_string(),
            }],
        );
        crate::lsp::WorkspaceEdit { changes }
    }

    #[test]
    fn entering_markdown_read_invalidates_source_edit_transients() {
        let Some(mut app) = markdown_source_app("# title\n") else {
            return;
        };
        let path = app.file_path.clone().expect("markdown path");
        app.autocomplete_active = true;
        app.autocomplete_pending_request_id = Some(41);
        app.autocomplete_pending_request_mode = Some(crate::app::AutocompleteMode::LspContext);
        app.autocomplete_pending_request_path = Some(path);
        app.autocomplete_pending_context_key = Some("ctx".to_string());
        app.autocomplete_signature_request_id = Some(42);
        app.autocomplete_detail_request_id = Some(43);
        app.autocomplete_detail_word = Some("title".to_string());
        app.lsp_actions_menu = Some(crate::app::LspActionsMenu {
            cursor_line: 0,
            items: vec![crate::app::LspActionItem::AddNoqaAll],
            selected: 0,
            menu_x: 0.0,
            menu_y: 0.0,
            pending_request_id: Some(44),
        });
        app.pending_fix_all_id = Some(45);

        app.set_markdown_mode(MarkdownMode::Read);

        assert!(!app.autocomplete_active);
        assert_eq!(app.autocomplete_pending_request_id, None);
        assert_eq!(app.autocomplete_pending_request_mode, None);
        assert_eq!(app.autocomplete_pending_request_path, None);
        assert_eq!(app.autocomplete_pending_context_key, None);
        assert_eq!(app.autocomplete_signature_request_id, None);
        assert_eq!(app.autocomplete_detail_request_id, None);
        assert_eq!(app.autocomplete_detail_word, None);
        assert!(app.lsp_actions_menu.is_none());
        assert_eq!(app.pending_fix_all_id, None);

        app.set_markdown_mode(MarkdownMode::Edit);
        assert_eq!(app.pending_fix_all_id, None);
        assert!(app.lsp_actions_menu.is_none());
        assert!(!app.autocomplete_active);
    }

    #[test]
    fn markdown_read_workspace_edit_is_blocked_and_edit_mode_recovers() {
        let Some(mut app) = markdown_source_app("# title\nbody\n") else {
            return;
        };
        let path = app.file_path.clone().expect("markdown path");
        let edit = workspace_edit_replacing_first_line(&path, "# changed");
        app.set_markdown_mode(MarkdownMode::Read);
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.apply_workspace_edit(&edit, true);

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(app.readonly_notice_until.is_some());

        app.set_markdown_mode(MarkdownMode::Edit);
        app.apply_workspace_edit(&edit, true);
        assert_eq!(app.editor.get_full_text(), "# changed\nbody\n");
        assert!(app.editor.version > version);
        assert!(app.editor.is_dirty());
    }

    #[test]
    fn markdown_read_noqa_is_blocked_at_central_mutation_boundary() {
        let Some(mut app) = markdown_source_app("value = 1\n") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.insert_noqa_comment(0, &["F401".to_string()]);

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(app.readonly_notice_until.is_some());
    }

    #[test]
    fn markdown_read_autocomplete_cannot_mutate_hidden_source() {
        let Some(mut app) = markdown_source_app("prin") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        app.autocomplete_active = true;
        app.autocomplete_selected_idx = 0;
        app.autocomplete_options = vec![(
            crate::app::AutocompleteItem {
                word: "print".to_string(),
                kind: crate::highlighter::SymbolKind::Function,
                scope_start: 0,
                scope_end: usize::MAX,
                module: None,
                module_path: None,
                detail: None,
                insert_text: Some("print".to_string()),
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            Vec::new(),
        )];
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.apply_autocomplete();

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(!app.autocomplete_active);
        assert!(app.readonly_notice_until.is_some());
    }

    #[test]
    fn markdown_read_lsp_menu_action_uses_central_readonly_barrier() {
        let Some(mut app) = markdown_source_app("value = 1\n") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        app.lsp_actions_menu = Some(crate::app::LspActionsMenu {
            cursor_line: 0,
            items: vec![crate::app::LspActionItem::AddNoqaAll],
            selected: 0,
            menu_x: 0.0,
            menu_y: 0.0,
            pending_request_id: None,
        });
        let source = app.editor.get_full_text();
        let version = app.editor.version;
        let dirty = app.editor.is_dirty();
        app.readonly_notice_until = None;

        app.apply_selected_lsp_action();

        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.version, version);
        assert_eq!(app.editor.is_dirty(), dirty);
        assert!(app.lsp_actions_menu.is_none());
        assert!(app.readonly_notice_until.is_some());
    }

    #[test]
    fn markdown_read_lsp_panel_fix_all_is_rejected_before_request_path() {
        let Some(mut app) = markdown_source_app("# title\n") else {
            return;
        };
        app.set_markdown_mode(MarkdownMode::Read);
        app.readonly_notice_until = None;

        app.handle_ui_click(crate::ui_system::UiId::LspServerFixAll(0));

        assert_eq!(app.pending_fix_all_id, None);
        assert!(app.readonly_notice_until.is_some());
    }

}
