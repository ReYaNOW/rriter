impl ProjectSearchState {
    pub(crate) fn focused_keyboard_field(&mut self) -> Option<ProjectSearchField> {
        let field = self.focused?;
        if field == ProjectSearchField::Filter && !self.filter_enabled() {
            self.focused = None;
            return None;
        }
        Some(field)
    }

    pub(crate) fn apply_keyboard_input(
        &mut self,
        field: ProjectSearchField,
        physical_key: winit::keyboard::PhysicalKey,
        logical_text: Option<&str>,
        ctrl: bool,
        word: bool,
        shift: bool,
        allow_text_input: bool,
        clipboard_text: Option<String>,
    ) -> ProjectSearchInputOutcome {
        use winit::keyboard::{KeyCode, PhysicalKey};

        let mut outcome = ProjectSearchInputOutcome::default();
        match physical_key {
            PhysicalKey::Code(KeyCode::Escape) => self.focused = None,
            PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter)
                if ctrl =>
            {
                outcome.should_run = field != ProjectSearchField::Filter;
            }
            PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                if field == ProjectSearchField::Query {
                    self.project_search_editor_mut(field).insert_str("\n");
                    outcome.edited = true;
                } else if field != ProjectSearchField::Filter {
                    outcome.should_run = true;
                }
            }
            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                let editor = self.project_search_editor_mut(field);
                if word {
                    editor.move_word_left(shift);
                } else {
                    editor.move_left(shift);
                }
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => {
                let editor = self.project_search_editor_mut(field);
                if word {
                    editor.move_word_right(shift);
                } else {
                    editor.move_right(shift);
                }
            }
            PhysicalKey::Code(KeyCode::ArrowUp) => {
                if field == ProjectSearchField::Query {
                    self.move_project_search_query_line(-1, shift);
                }
            }
            PhysicalKey::Code(KeyCode::ArrowDown) => {
                if field == ProjectSearchField::Query {
                    self.move_project_search_query_line(1, shift);
                }
            }
            PhysicalKey::Code(KeyCode::Home) => {
                self.project_search_editor_mut(field).move_home(shift);
            }
            PhysicalKey::Code(KeyCode::End) => {
                self.project_search_editor_mut(field).move_end(shift);
            }
            PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                self.project_search_editor_mut(field).select_all();
            }
            PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                outcome.clipboard = self
                    .project_search_editor_mut(field)
                    .get_selection()
                    .map(|text| ProjectSearchClipboardWrite { text, cut: false });
            }
            PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                outcome.clipboard = self
                    .project_search_editor_mut(field)
                    .get_selection()
                    .map(|text| ProjectSearchClipboardWrite { text, cut: true });
            }
            PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                if let Some(text) = clipboard_text {
                    let text = if field == ProjectSearchField::Query {
                        text
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    if !text.is_empty() {
                        self.project_search_editor_mut(field).insert_str(&text);
                        outcome.edited = true;
                    }
                }
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                outcome.edited = if word {
                    self.project_search_editor_mut(field)
                        .delete_word_backward()
                        .is_some()
                } else {
                    self.project_search_editor_mut(field).backspace().is_some()
                };
            }
            PhysicalKey::Code(KeyCode::Delete) => {
                outcome.edited = if word {
                    self.project_search_editor_mut(field)
                        .delete_word_forward()
                        .is_some()
                } else {
                    self.project_search_editor_mut(field)
                        .delete_forward()
                        .is_some()
                };
            }
            _ if allow_text_input => {
                if let Some(text) = logical_text {
                    let text = if field == ProjectSearchField::Query {
                        text.to_string()
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    if !text.is_empty() {
                        self.project_search_editor_mut(field).insert_str(&text);
                        outcome.edited = true;
                    }
                }
            }
            _ => {}
        }
        outcome
    }

    pub(crate) fn finish_keyboard_edit(
        &mut self,
        field: ProjectSearchField,
        edited: bool,
    ) -> ProjectSearchPostEdit {
        if !edited {
            return ProjectSearchPostEdit::default();
        }
        self.project_search_editor_mut(field).sync_edits.clear();
        if field == ProjectSearchField::Filter {
            self.apply_live_filter();
        } else {
            self.dirty = true;
        }
        let persist = matches!(field, ProjectSearchField::Include | ProjectSearchField::Exclude);
        let refresh_query_width = field == ProjectSearchField::Query;
        ProjectSearchPostEdit {
            refresh_query_width,
            persist_panel_state: persist,
        }
    }

    pub(crate) fn project_search_editor_mut(
        &mut self,
        field: crate::app::project_search::ProjectSearchField,
    ) -> &mut Editor {
        match field {
            crate::app::project_search::ProjectSearchField::Query => {
                &mut self.query_editor
            }
            crate::app::project_search::ProjectSearchField::Include => {
                &mut self.include_editor
            }
            crate::app::project_search::ProjectSearchField::Exclude => {
                &mut self.exclude_editor
            }
            crate::app::project_search::ProjectSearchField::Filter => {
                &mut self.filter_editor
            }
        }
    }

    pub(crate) fn move_project_search_query_line(&mut self, delta: i32, shift: bool) {
        let editor = &mut self.query_editor;
        let text = editor.get_full_text();
        let current_line = editor
            .line_offsets
            .partition_point(|&offset| offset <= editor.cursor)
            .saturating_sub(1);
        let target_line = if delta < 0 {
            current_line.saturating_sub(1)
        } else {
            (current_line + 1).min(editor.line_offsets.len().saturating_sub(1))
        };
        if target_line == current_line {
            return;
        }
        let current_start = editor.line_offsets.get(current_line).copied().unwrap_or(0);
        let current_col = editor.cursor.saturating_sub(current_start);
        let target_start = editor.line_offsets.get(target_line).copied().unwrap_or(0);
        let mut target_end = editor
            .line_offsets
            .get(target_line + 1)
            .copied()
            .unwrap_or(text.len())
            .min(text.len());
        if target_end > target_start && text.as_bytes().get(target_end - 1) == Some(&b'\n') {
            target_end -= 1;
        }
        let mut target = (target_start + current_col).min(target_end);
        while target < target_end && !text.is_char_boundary(target) {
            target += 1;
        }
        if shift {
            if editor.selection_anchor.is_none() {
                editor.selection_anchor = Some(editor.cursor);
            }
        } else {
            editor.selection_anchor = None;
        }
        editor.cursor = target;
    }
}

#[derive(Default)]
pub(crate) struct ProjectSearchInputOutcome {
    pub(crate) edited: bool,
    pub(crate) should_run: bool,
    pub(crate) clipboard: Option<ProjectSearchClipboardWrite>,
}

pub(crate) struct ProjectSearchClipboardWrite {
    pub(crate) text: String,
    pub(crate) cut: bool,
}

#[derive(Default)]
pub(crate) struct ProjectSearchPostEdit {
    pub(crate) refresh_query_width: bool,
    pub(crate) persist_panel_state: bool,
}

impl ProjectSearchState {
    pub(crate) fn prepare_search_request(
        &mut self,
        workspaces: Vec<PathBuf>,
        ignore_patterns: Vec<String>,
    ) -> Option<ProjectSearchRequest> {
        let query = self.query_editor.get_full_text();
        self.cancel_running_worker();
        let generation = self.advance_generation();
        self.has_run = true;
        self.error = None;
        self.elapsed_ms = None;
        self.capped = false;
        self.results.clear();
        self.flat_rows.clear();
        self.collapsed.clear();
        self.reset_preview_worker();
        self.total_matches = 0;
        self.scroll.reset();
        if self.focused == Some(ProjectSearchField::Filter) {
            self.focused = None;
            self.dragging_field = None;
        }
        if query.is_empty() {
            self.running_generation = None;
            self.rx = None;
            self.worker_cancel = None;
            return None;
        }
        self.running_generation = Some(generation);
        Some(ProjectSearchRequest {
            generation,
            query,
            include: self.include_editor.get_full_text(),
            exclude: self.exclude_editor.get_full_text(),
            case_sensitive: self.case_sensitive,
            workspaces,
            ignore_patterns,
        })
    }

    pub(crate) fn poll_worker_messages(&mut self) -> bool {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = &self.rx {
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
            updated |= self.apply_message(message);
        }
        if disconnected {
            updated |= self.handle_worker_disconnect();
        }
        updated
    }

    pub(crate) fn poll_preview_messages(&mut self) -> bool {
        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(rx) = &self.preview_rx {
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
            updated |= self.apply_preview_message(message);
        }
        if disconnected {
            self.handle_preview_disconnect();
            updated = true;
        }
        updated
    }

    pub(crate) fn set_focused_field(&mut self, field: ProjectSearchField) -> bool {
        if field == ProjectSearchField::Filter && !self.filter_enabled() {
            self.focused = None;
            return false;
        }
        self.focused = Some(field);
        true
    }

    pub(crate) fn set_field_cursor(
        &mut self,
        field: ProjectSearchField,
        target: usize,
        reset_anchor: bool,
    ) {
        let editor = self.project_search_editor_mut(field);
        editor.cursor = target;
        if reset_anchor || editor.selection_anchor.is_none() {
            editor.selection_anchor = Some(target);
        }
    }

    pub(crate) fn selected_match_position(
        &self,
        file_idx: usize,
        match_idx: usize,
    ) -> Option<ProjectSearchMatchPosition> {
        self.results.get(file_idx).and_then(|file| {
            file.matches.get(match_idx).map(|mat| {
                ProjectSearchMatchPosition {
                    path: file.path.clone(),
                    start_line: mat.start_line,
                    start_col: mat.start_col,
                    end_line: mat.end_line,
                    end_col: mat.end_col,
                }
            })
        })
    }
}

pub(crate) struct ProjectSearchMatchPosition {
    pub(crate) path: PathBuf,
    pub(crate) start_line: u32,
    pub(crate) start_col: u32,
    pub(crate) end_line: u32,
    pub(crate) end_col: u32,
}

#[cfg(test)]
mod input_state_tests {
    use super::*;

        #[test]
        fn project_search_flat_rows_respect_collapsed_files() {
            let mut state = ProjectSearchState::default();
            state.results.push(ProjectSearchFile {
                path: PathBuf::from("/w/src/a.rs"),
                relative_path: "src/a.rs".to_string(),
                icon_key: "rust",
                matches: vec![
                    ProjectSearchMatch {
                        byte_start: 0,
                        byte_end: 1,
                        line_byte_start: 0,
                        start_line: 0,
                        start_col: 0,
                        end_line: 0,
                        end_col: 1,
                        preview: "a".to_string(),
                        preview_match_start: 0,
                        preview_match_end: 1,
                        preview_ready: true,
                        extra_lines: 0,
                    },
                    ProjectSearchMatch {
                        byte_start: 2,
                        byte_end: 3,
                        line_byte_start: 2,
                        start_line: 1,
                        start_col: 0,
                        end_line: 1,
                        end_col: 1,
                        preview: "b".to_string(),
                        preview_match_start: 0,
                        preview_match_end: 1,
                        preview_ready: true,
                        extra_lines: 0,
                    },
                ],
            });
            state.rebuild_flat_rows();
            assert_eq!(state.flat_rows.len(), 3);
            state.toggle_file(0);
            assert_eq!(state.flat_rows, vec![ProjectSearchFlatRow::File(0)]);
        }

        #[test]
        fn project_search_live_filter_rebuilds_visible_rows_only() {
            let mut state = ProjectSearchState::default();
            state.has_run = true;
            state.results.push(ProjectSearchFile {
                path: PathBuf::from("/w/src/a.rs"),
                relative_path: "src/a.rs".to_string(),
                icon_key: "rust",
                matches: Vec::new(),
            });
            state.results.push(ProjectSearchFile {
                path: PathBuf::from("/w/src/a.py"),
                relative_path: "src/a.py".to_string(),
                icon_key: "python",
                matches: Vec::new(),
            });
            state.filter_editor.insert_str("*.rs");

            state.apply_live_filter();

            assert_eq!(state.flat_rows, vec![ProjectSearchFlatRow::File(0)]);
        }

        #[test]
        fn query_scroll_reveals_cursor_and_exposes_both_scrollbars() {
            let mut state = ProjectSearchState::default();
            state
                .query_editor
                .insert_str("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight");
            state.query_editor.cursor = state.query_editor.len();
            state.query_content_width = 420.0;
            let rect = ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 180.0,
                h: PROJECT_SEARCH_QUERY_H,
            };

            state.reveal_query_cursor(rect, 1.0, 390.0);

            assert!(state.query_scroll_y.current > 0.0);
            assert!(state.query_scroll_x.current > 0.0);
            assert!(
                project_search_query_scrollbar_thumb(
                    rect,
                    &state,
                    ProjectSearchQueryScrollAxis::Vertical,
                    1.0,
                )
                .is_some()
            );
            assert!(
                project_search_query_scrollbar_thumb(
                    rect,
                    &state,
                    ProjectSearchQueryScrollAxis::Horizontal,
                    1.0,
                )
                .is_some()
            );
        }

        #[test]
        fn query_scrollbar_drag_reuses_shared_scroll_math() {
            let mut state = ProjectSearchState::default();
            state.query_editor.insert_str("a\nb\nc\nd\ne\nf\ng\nh");
            state.query_content_width = 400.0;
            let rect = ProjectSearchRect {
                x: 10.0,
                y: 20.0,
                w: 180.0,
                h: PROJECT_SEARCH_QUERY_H,
            };
            let viewport = project_search_query_viewport(rect, 1.0);

            assert!(state.start_query_scrollbar_drag(
                rect,
                ProjectSearchQueryScrollAxis::Vertical,
                viewport.vertical_track.y + viewport.vertical_track.h,
                1.0,
            ));
            assert!(state.query_scroll_y.target > 0.0);
            assert_eq!(state.query_scroll_y.current, 0.0);
            assert_ne!(state.query_scroll_y.current, state.query_scroll_y.target);
            assert!(state.query_scroll_y.is_dragging);
            let y_offset = state.query_scroll_y.drag_offset;
            let y_target = state.query_scroll_y.target;
            assert!(state.drag_query_scrollbar_to(
                rect,
                ProjectSearchQueryScrollAxis::Vertical,
                viewport.vertical_track.y + viewport.vertical_track.h * 0.6,
                1.0,
            ));
            assert_eq!(state.query_scroll_y.current, 0.0);
            assert_ne!(state.query_scroll_y.target, y_target);
            assert_eq!(state.query_scroll_y.drag_offset, y_offset);

            assert!(state.start_query_scrollbar_drag(
                rect,
                ProjectSearchQueryScrollAxis::Horizontal,
                viewport.horizontal_track.x + viewport.horizontal_track.w,
                1.0,
            ));
            assert!(state.query_scroll_x.target > 0.0);
            assert_eq!(state.query_scroll_x.current, 0.0);
            assert_ne!(state.query_scroll_x.current, state.query_scroll_x.target);
            assert!(state.query_scroll_x.is_dragging);
        }

        #[test]
        fn query_wheel_scroll_clamps_vertical_target() {
            let mut state = ProjectSearchState::default();
            state.query_editor.insert_str("a\nb\nc\nd\ne\nf\ng\nh");
            let rect = ProjectSearchRect {
                x: 0.0,
                y: 0.0,
                w: 180.0,
                h: PROJECT_SEARCH_QUERY_H,
            };
            let max_scroll = state.query_max_scroll_y(rect, 1.0);

            state.scroll_query_y_by(rect, 1.0, 10_000.0);
            assert_eq!(state.query_scroll_y.target, max_scroll);

            state.scroll_query_y_by(rect, 1.0, -10_000.0);
            assert_eq!(state.query_scroll_y.target, 0.0);
        }
}
