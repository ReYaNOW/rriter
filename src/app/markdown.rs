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
    if hovered != Some(crate::ui_system::UiId::MarkdownReadBody) {
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
