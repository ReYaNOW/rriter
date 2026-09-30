// Ctrl+C on the VCS log selection: who may take the copy (`git_logs_keyboard_copy_eligible`)
// and the tests of that hand-off. A child module of `main_keys` (split off to keep it under the
// file line limit).

/// The owned git-log selection is copied by Ctrl+C unless a downstream keyboard owner (git
/// message, visible API field, LSP log, terminal, search) has the focus.
pub(super) fn git_logs_keyboard_copy_eligible(
    is_ide_mode: bool,
    show_search: bool,
    search_focused: bool,
    api_keyboard_surface_visible: bool,
    panels: &crate::app::IdePanelState,
) -> bool {
    if !is_ide_mode || !panels.is_open(crate::app::PanelId::Git) || !panels.git.owns_git_logs_copy()
    {
        return false;
    }

    let downstream_keyboard_owner = panels.git.message_focused
        || (api_keyboard_surface_visible && panels.api.focused.is_some())
        || (panels.is_open(crate::app::PanelId::LspServers) && panels.lsp_logs_focused.is_some())
        || (panels.is_open(crate::app::PanelId::Terminal)
            && panels.term_show_search
            && panels.term_search_focused)
        || (show_search && search_focused)
        || (panels.is_open(crate::app::PanelId::Terminal) && panels.terminal_focused);

    !downstream_keyboard_owner
}

#[cfg(test)]
mod tests {
    use super::super::apply_terminal_alt_q_shortcut;
    use super::git_logs_keyboard_copy_eligible;

    fn panels_with_owned_vcs_copy() -> crate::app::IdePanelState {
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Git);
        panels.git.toggle_logs_pane();
        panels.git.seed_git_log_for_test("selected");
        let line = panels.git.git_logs.display_line_at(0).unwrap();
        assert!(panels.git.git_logs.set_selection(
            crate::app::git_panel::GitLogTextPoint {
                line: line.id(),
                byte: 0,
            },
            crate::app::git_panel::GitLogTextPoint {
                line: line.id(),
                byte: line.byte_len(),
            },
        ));
        panels.git.claim_git_logs_copy_owner();
        panels
    }

    fn claim_vcs_selection_after_text_focus_handoff(app: &mut crate::app::App) {
        app.focus_document_text_surface();
        let line = app.ide_panel.git.git_logs.display_line_at(0).unwrap();
        assert!(app.ide_panel.git.git_logs.set_selection(
            crate::app::git_panel::GitLogTextPoint {
                line: line.id(),
                byte: 0,
            },
            crate::app::git_panel::GitLogTextPoint {
                line: line.id(),
                byte: line.byte_len(),
            },
        ));
        app.ide_panel.git.claim_git_logs_copy_owner();
    }

    fn app_with_vcs_log() -> crate::app::App {
        let mut app = crate::app::app_behavior_tests::test_app().unwrap();
        app.ide_panel.open(crate::app::PanelId::Git);
        app.ide_panel.git.toggle_logs_pane();
        app.ide_panel.git.seed_git_log_for_test("selected");
        app
    }

    #[test]
    fn vcs_text_selection_takes_copy_focus_from_git_message() {
        let mut app = app_with_vcs_log();
        app.ide_panel.git.message_focused = true;

        claim_vcs_selection_after_text_focus_handoff(&mut app);

        assert!(!app.ide_panel.git.message_focused);
        assert!(app.ide_panel.git.owns_git_logs_copy());
        assert!(git_logs_keyboard_copy_eligible(
            true,
            false,
            false,
            false,
            &app.ide_panel
        ));
        assert_eq!(
            app.ide_panel.git.copy_owned_git_logs_selection().as_deref(),
            Some("selected")
        );
    }

    #[test]
    fn search_to_vcs_selection_then_search_takeover_preserves_selection() {
        let mut app = app_with_vcs_log();
        app.show_search = true;
        app.search_focused = true;

        claim_vcs_selection_after_text_focus_handoff(&mut app);

        assert!(!app.search_focused);
        assert!(git_logs_keyboard_copy_eligible(
            true,
            true,
            app.search_focused,
            false,
            &app.ide_panel
        ));

        app.search_focused = true;
        assert!(!git_logs_keyboard_copy_eligible(
            true,
            true,
            app.search_focused,
            false,
            &app.ide_panel
        ));
        assert_eq!(
            app.ide_panel.git.copy_owned_git_logs_selection().as_deref(),
            Some("selected"),
            "Search takeover must not destroy the prior VCS selection"
        );
    }

    #[test]
    fn stale_hidden_api_focus_does_not_block_vcs_but_visible_api_focus_does() {
        let mut app = app_with_vcs_log();
        claim_vcs_selection_after_text_focus_handoff(&mut app);
        app.ide_panel.api.focused = Some(crate::app::api_client::ApiFocus::RouteFilter);

        assert!(git_logs_keyboard_copy_eligible(
            true,
            false,
            false,
            false,
            &app.ide_panel
        ));
        assert!(!git_logs_keyboard_copy_eligible(
            true,
            false,
            false,
            true,
            &app.ide_panel
        ));
        assert_eq!(
            app.ide_panel.git.copy_owned_git_logs_selection().as_deref(),
            Some("selected")
        );
    }

    #[test]
    fn vcs_copy_yields_to_keyboard_focused_global_search() {
        let panels = panels_with_owned_vcs_copy();
        assert!(panels.git.owns_git_logs_copy());
        assert!(!git_logs_keyboard_copy_eligible(
            true, true, true, false, &panels
        ));
        assert_eq!(
            panels.git.copy_owned_git_logs_selection().as_deref(),
            Some("selected"),
            "Search takeover must gate VCS copy without destroying its selection"
        );
    }

    #[test]
    fn vcs_copy_yields_to_terminal_focused_by_alt_q() {
        let mut panels = panels_with_owned_vcs_copy();
        assert!(!apply_terminal_alt_q_shortcut(&mut panels, false, true));
        assert!(panels.git.owns_git_logs_copy());
        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(panels.terminal_focused);
        assert!(!git_logs_keyboard_copy_eligible(
            true, false, false, false, &panels
        ));
    }

    #[test]
    fn vcs_copy_yields_to_terminal_search_keyboard_focus() {
        let mut panels = panels_with_owned_vcs_copy();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = false;
        panels.term_show_search = true;
        panels.term_search_focused = true;
        assert!(panels.git.owns_git_logs_copy());
        assert!(!git_logs_keyboard_copy_eligible(
            true, false, false, false, &panels
        ));
    }

    #[test]
    fn vcs_copy_yields_to_other_downstream_keyboard_owners() {
        let mut git_message = panels_with_owned_vcs_copy();
        git_message.git.message_focused = true;
        assert!(!git_logs_keyboard_copy_eligible(
            true,
            false,
            false,
            false,
            &git_message
        ));

        let mut api = panels_with_owned_vcs_copy();
        api.api.focused = Some(crate::app::api_client::ApiFocus::RouteFilter);
        assert!(!git_logs_keyboard_copy_eligible(
            true, false, false, true, &api
        ));

        let mut lsp = panels_with_owned_vcs_copy();
        lsp.open(crate::app::PanelId::LspServers);
        lsp.lsp_logs_focused = Some("rust-analyzer".to_string());
        assert!(!git_logs_keyboard_copy_eligible(
            true, false, false, false, &lsp
        ));
    }

    #[test]
    fn vcs_copy_remains_eligible_for_owned_non_empty_selection() {
        let panels = panels_with_owned_vcs_copy();
        assert!(git_logs_keyboard_copy_eligible(
            true, false, false, false, &panels
        ));
        assert_eq!(
            panels.git.copy_owned_git_logs_selection().as_deref(),
            Some("selected")
        );
    }
}
