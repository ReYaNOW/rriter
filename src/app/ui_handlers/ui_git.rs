use crate::app::App;
use crate::ui_system::UiId;
use super::{UiClickFlow, repeated_ui_click};

impl App {
    pub(super) fn handle_git_ui_click(&mut self, id: UiId, same_click_target: bool) -> UiClickFlow {
        match id {

            // Git panel
            UiId::GitWorkspaceToggle(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFile(workspace_idx, file_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_file_stage(workspace_idx, file_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFileDiff(workspace_idx, file_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                let (mx, my) = self
                    .renderer
                    .as_ref()
                    .map(|r| (r.last_mouse_x, r.last_mouse_y))
                    .unwrap_or((0.0, 0.0));
                let now = std::time::Instant::now();
                let dx = mx - self.last_click_pos.0;
                let dy = my - self.last_click_pos.1;
                let double_click = repeated_ui_click(
                    same_click_target,
                    now.duration_since(self.last_click_time),
                    dx,
                    dy,
                );
                self.ide_panel.git.selected_file = Some((workspace_idx, file_idx));
                self.last_click_time = now;
                self.last_click_pos = (mx, my);
                if double_click {
                    self.open_git_diff_tab(workspace_idx, file_idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFolderStage(workspace_idx, row_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_folder_stage(workspace_idx, row_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFolder(workspace_idx, row_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_tree_folder(workspace_idx, row_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommit => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.commit_git_panel();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitMenuToggle => {
                self.ide_panel.git.close_repo_action_menu();
                if self.ide_panel.git.commit_enabled() && !self.ide_panel.git.pending {
                    self.ide_panel
                        .git
                        .toggle_commit_menu(std::time::Instant::now());
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitMenuItem(idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                if self.ide_panel.git.commit_enabled() && !self.ide_panel.git.pending {
                    self.commit_git_panel_option(idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitOptionsToggle => {
                self.ide_panel.git.close_repo_action_menu();
                if self.ide_panel.git.commit_enabled() && !self.ide_panel.git.pending {
                    self.ide_panel
                        .git
                        .toggle_commit_options_menu(std::time::Instant::now());
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitOptionsItem(0) => {
                if !self.ide_panel.git.pending {
                    self.ide_panel.git.commit_options.skip_hooks =
                        !self.ide_panel.git.commit_options.skip_hooks;
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitOptionsItem(_) => {}
            UiId::GitPush(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.push_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitRollbackStaged(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.open_git_rollback_staged_dialog(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitStageAll(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.stage_all_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitUnstageAll(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.open_git_unstage_all_dialog(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitRepoActionMenu(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                if !self.ide_panel.git.pending {
                    self.ide_panel
                        .git
                        .toggle_repo_action_menu(workspace_idx, std::time::Instant::now());
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFetch(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.fetch_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitPull(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.pull_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitConfirmAction => {
                self.ide_panel.git.close_repo_action_menu();
                self.confirm_git_dialog();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitConfirmCancel => {
                self.ide_panel.git.confirm_dialog = None;
                self.ide_panel.git.close_repo_action_menu();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitRefresh => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.refresh_git_panel_window();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphToggle => {
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_graph();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitLogsToggle => {
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_logs();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitLogsClear => {
                self.ide_panel.git.clear_git_logs();
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.reset_git_logs_layout();
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphWorkspace(workspace_idx) => {
                self.ide_panel.git.close_repo_action_menu();
                self.select_git_graph_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphResize
            | UiId::GitGraphScroll
            | UiId::GitGraphCommit(_, _)
            | UiId::GitLogsBody
            | UiId::GitLogsScroll
            | UiId::GitWorkspaceScroll => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphCopyCommit(workspace_idx, commit_idx) => {
                self.copy_git_graph_commit(workspace_idx, commit_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphOpenCommit(workspace_idx, commit_idx) => {
                self.open_git_graph_commit(workspace_idx, commit_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitMessageInput => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                let was_focused = self.ide_panel.git.message_focused;
                self.ide_panel.git.message_focused = true;
                self.search_focused = false;
                self.ide_panel.term_search_focused = false;
                self.ide_panel.lsp_log_filter_focused = false;
                self.ide_panel.file_tree_focused = false;
                self.is_dragging_search = true;
                let input_rect = self.ui_registry.rect_for(UiId::GitMessageInput);
                if let (Some(rect), Some(r)) = (input_rect, self.renderer.as_mut()) {
                    if !was_focused {
                        r.git_commit_scroll_x = 0.0;
                    }
                    let x_offset = (r.last_mouse_x - (rect.0 + 5.0 * r.scale_factor)
                        + r.git_commit_scroll_x)
                        .max(0.0);
                    let text = self.ide_panel.git.message_editor.get_full_text();
                    let target_idx = r.one_line_cursor_from_x(&text, x_offset, 1.0);
                    self.ide_panel.git.message_editor.cursor = target_idx;
                    self.ide_panel.git.message_editor.selection_anchor = Some(target_idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            _ => return UiClickFlow::NotMine,
        }
        UiClickFlow::Handled
    }
}
