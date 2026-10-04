use crate::app::App;
use crate::keymap::{Chord, Command, KeyContext, COMMANDS};
use crate::ui_system::UiId;

pub(crate) enum CommandOutcome {
    Done,
    Unavailable(&'static str),
}

impl App {
    pub(crate) fn run_bound_commands(
        &mut self,
        chord: Option<Chord>,
        context: Option<KeyContext>,
        repeat: bool,
    ) -> bool {
        let Some(chord) = chord else { return false; };
        if self.command_terminal_owns_chord(chord) {
            return false;
        }
        // A database tab whose sibling context owns the chord reports why the
        // command cannot run instead of letting the chord fall through.
        let mut sibling = None;
        for info in COMMANDS.iter().skip(Command::EditorGitDiffPrevHunk as usize) {
            if context.is_some_and(|context| info.context != context)
                || !self.keymap.hit(info.command, chord)
            {
                continue;
            }
            if !self.command_context_active(info.context) {
                if sibling.is_none()
                    && matches!(info.context, KeyContext::DatabaseTable | KeyContext::DatabaseQuery)
                    && self.active_tab_is_database()
                {
                    sibling = Some(info.command);
                }
                continue;
            }
            return self.run_bound_command(info.command, repeat);
        }
        sibling.is_some_and(|command| self.run_bound_command(command, repeat))
    }

    fn run_bound_command(&mut self, command: Command, repeat: bool) -> bool {
        if !repeat {
            if let CommandOutcome::Unavailable(message) = self.run_command(command) {
                self.show_command_unavailable(message);
            }
        }
        true
    }

    pub(crate) fn run_command(&mut self, command: Command) -> CommandOutcome {
        use Command as C;
        match command {
            C::EditorGitDiffPrevHunk | C::EditorGitDiffNextHunk => {
                if self.editor.git_hunks.is_empty() {
                    return CommandOutcome::Unavailable("Нет изменений Git в документе");
                }
                self.handle_ui_click(if command == C::EditorGitDiffPrevHunk {
                    UiId::InlineGitPrevHunk
                } else {
                    UiId::InlineGitNextHunk
                });
            }
            C::GitStageAll | C::GitUnstageAll | C::GitPush | C::GitFetch | C::GitPull => {
                let Some(workspace) = self.active_git_workspace() else {
                    return CommandOutcome::Unavailable("Нет активного репозитория");
                };
                let id = match command {
                    C::GitStageAll => UiId::GitStageAll(workspace),
                    C::GitUnstageAll => UiId::GitUnstageAll(workspace),
                    C::GitPush => UiId::GitPush(workspace),
                    C::GitFetch => UiId::GitFetch(workspace),
                    C::GitPull => UiId::GitPull(workspace),
                    _ => return CommandOutcome::Unavailable("Команда недоступна"),
                };
                self.handle_ui_click(id);
            }
            C::GitRefresh => {
                if self.active_git_workspace().is_none() {
                    return CommandOutcome::Unavailable("Нет активного репозитория");
                }
                self.handle_ui_click(UiId::GitRefresh);
            }
            C::GitToggleGraph | C::GitToggleLogs => {
                if !self.is_ide_mode {
                    return CommandOutcome::Unavailable("Доступно в IDE");
                }
                self.handle_ui_click(if command == C::GitToggleGraph {
                    UiId::GitGraphToggle
                } else {
                    UiId::GitLogsToggle
                });
            }
            C::LspRestartServer | C::LspFixAll => {
                if command == C::LspRestartServer {
                    let extension = self.file_path
                        .as_deref()
                        .and_then(|path| path.extension())
                        .and_then(|extension| extension.to_str())
                        .unwrap_or_default();
                    let names = crate::lsp::server_names_for_extension(extension);
                    let indices: Vec<usize> = self.ide_panel.lsp_servers.iter().enumerate()
                        .filter(|(_, server)| names.contains(&server.name))
                        .map(|(index, _)| index)
                        .collect();
                    if indices.is_empty() {
                        return CommandOutcome::Unavailable("Нет LSP-сервера активного документа");
                    }
                    for index in indices {
                        self.handle_ui_click(UiId::LspServerRestart(index));
                    }
                    return CommandOutcome::Done;
                }
                let Some(index) = self.ide_panel.lsp_servers.first().map(|_| 0) else {
                    return CommandOutcome::Unavailable("Нет LSP-сервера активного документа");
                };
                if command == C::LspFixAll
                    && (self.markdown_mode() == crate::app::MarkdownMode::Read
                        || self.file_path.is_none())
                {
                    return CommandOutcome::Unavailable("Исправления недоступны");
                }
                self.handle_ui_click(UiId::LspServerFixAll(index));
            }
            C::ApiImportOpenapiFile | C::ApiSendRequest | C::ApiMockToggleServer | C::ApiMockExportOpenapi => {
                let api_open = self.active_tab_is_api_client()
                    || self.ide_panel.is_open(crate::app::PanelId::ApiClient);
                if !api_open || (command == C::ApiSendRequest && self.active_api_tab().is_none()) {
                    return CommandOutcome::Unavailable("Нет активного запроса");
                }
                self.handle_ui_click(match command {
                    C::ApiImportOpenapiFile => UiId::ApiImportFile,
                    C::ApiSendRequest => UiId::ApiTryRequest,
                    C::ApiMockToggleServer => UiId::ApiMockServerToggle,
                    C::ApiMockExportOpenapi => UiId::ApiMockExportOpenApi,
                    _ => return CommandOutcome::Unavailable("Команда недоступна"),
                });
            }
            C::DatabaseRefreshSelected => {
                if self.ide_panel.database.selected_connection.is_none() {
                    return CommandOutcome::Unavailable("Не выбрано подключение БД");
                }
                self.handle_ui_click(UiId::DatabaseRefresh);
            }
            C::DatabaseTableRefresh | C::DatabaseTableSave | C::DatabaseTablePreviewSql => {
                if !self.active_tab_is_database_table() {
                    return CommandOutcome::Unavailable("Нет открытой таблицы БД");
                }
                if command == C::DatabaseTableSave
                    && !self.active_database_table_has_pending_changes()
                {
                    return CommandOutcome::Unavailable("Нет изменений таблицы");
                }
                self.handle_ui_click(match command {
                    C::DatabaseTableRefresh => UiId::DatabaseTableRefresh,
                    C::DatabaseTableSave => UiId::DatabaseTableSave,
                    C::DatabaseTablePreviewSql => UiId::DatabaseTablePreview,
                    _ => return CommandOutcome::Unavailable("Команда недоступна"),
                });
            }
            C::DatabaseQueryExplain
            | C::DatabaseQueryExplainAnalyze
            | C::DatabaseQueryFormat
            | C::DatabaseQueryNextDiagnostic => {
                if !self.active_tab_is_database_query() {
                    return CommandOutcome::Unavailable("SQL-консоль не активна");
                }
                self.handle_ui_click(match command {
                    C::DatabaseQueryExplain => UiId::DatabaseQueryExplain,
                    C::DatabaseQueryExplainAnalyze => UiId::DatabaseQueryExplainAnalyze,
                    C::DatabaseQueryFormat => UiId::DatabaseQueryFormat,
                    C::DatabaseQueryNextDiagnostic => UiId::DatabaseQueryNextDiagnostic,
                    _ => return CommandOutcome::Unavailable("Команда недоступна"),
                });
            }
            _ => return CommandOutcome::Unavailable("Команда недоступна"),
        }
        CommandOutcome::Done
    }

    fn active_git_workspace(&self) -> Option<usize> {
        self.ide_panel.git.selected_file.map(|(workspace, _)| workspace)
            .or_else(|| self.ide_panel.git.staged_workspace_lock())
            .or_else(|| self.ide_panel.git.snapshot.workspaces.iter()
                .find(|workspace| workspace.repo_root.is_some())
                .map(|workspace| workspace.workspace_idx))
    }

    pub(crate) fn command_context_active(&self, context: KeyContext) -> bool {
        match context {
            KeyContext::Global => true,
            KeyContext::Editor => self.editor_has_input_focus()
                && !self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_pdf() || tab.kind.is_image())
                && !self.active_tab_is_database()
                && !self.active_tab_is_api_client(),
            KeyContext::Git => self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Git),
            KeyContext::DatabaseTable => self.active_tab_is_database_table(),
            KeyContext::DatabaseQuery => self.active_tab_is_database_query(),
            KeyContext::ApiClient => self.active_tab_is_api_client()
                || self.ide_panel.is_open(crate::app::PanelId::ApiClient),
            _ => false,
        }
    }

    pub(crate) fn database_table_command_context_unowned(&self) -> bool {
        !self.show_settings
            && self.ide_panel.database.table_modal.is_none()
            && self.active_database_table_tab_id()
                .and_then(|tab_id| self.database_table_meta_state(tab_id))
                .is_some_and(|(_, state)| {
                    state.grid.focused_input.is_none() && !state.unavailable_text_focused
                })
    }

    fn command_terminal_owns_chord(&self, chord: Chord) -> bool {
        crate::app::keyboard::input_owner::terminal_intercepts(
            chord,
            crate::platform::CURRENT_PLATFORM,
        ) && crate::app::keyboard::input_owner::terminal_keyboard_owner(
            self.is_ide_mode,
            self.show_settings,
            self.show_search,
            self.search_focused,
            &self.ide_panel,
        ) && !(self.ide_panel.term_show_search && self.ide_panel.term_search_focused)
    }

    fn show_command_unavailable(&mut self, message: &'static str) {
        self.show_notice(message);
    }

    fn active_database_table_has_pending_changes(&self) -> bool {
        self.active_database_table_tab_id()
            .and_then(|tab_id| self.database_table_meta_state(tab_id))
            .is_some_and(|(_, state)| state.grid.dirty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_new_command_has_a_command_runner_arm() {
        let source = include_str!("commands.rs")
            .split("\n#[cfg(test)]")
            .next()
            .expect("production command runner");
        for info in COMMANDS.iter().skip(Command::EditorGitDiffPrevHunk as usize) {
            assert!(source.contains(&format!("C::{:?}", info.command)), "missing {:?}", info.command);
        }
    }

    #[test]
    fn bound_command_uses_its_context_and_respects_terminal_ownership() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else { return; };
        let chord = Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+q")
            .expect("test chord");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(crate::platform::CURRENT_PLATFORM, Command::GitPush, chord);
        overrides.add_chord(
            crate::platform::CURRENT_PLATFORM,
            Command::DatabaseQueryExplain,
            chord,
        );
        app.keymap = crate::keymap::Keymap::build(&overrides);

        assert!(!app.run_bound_commands(Some(chord), Some(KeyContext::DatabaseQuery), false));
        app.is_ide_mode = true;
        app.ide_panel.terminal_focused = false;
        assert!(app.run_bound_commands(Some(chord), Some(KeyContext::Global), false));
        app.ide_panel.open(crate::app::PanelId::Terminal);
        app.ide_panel.terminal_focused = true;
        assert!(!app.run_bound_commands(Some(chord), Some(KeyContext::Global), false));
    }
}
