use super::*;

const API_PANEL: &str =
    include_str!("render_view/api_client_panel/api_client_panel_main_renderer.rs");
const API_TAB: &str = include_str!("render_view/api_client_tab/api_client_tab_main_renderer.rs");
const API_AUTH: &str = include_str!("render_view/api_client_tab/api_client_tab_auth_renderer.rs");
const SETTINGS: &str = include_str!("render_view/settings_ui.rs");
const SETTINGS_TOOL_ROWS: &str = include_str!("render_view/settings_tool_rows.rs");
const GIT_UI: &str = include_str!("render_view/ide_panels/ide_panel_git_workspace_renderer.rs");
const TABS_UI: &str = include_str!("render_view/tabs_ui.rs");
const TAB_INPUT: &str = include_str!("app/mouse/input.rs");
const PROJECT_SEARCH_UI: &str =
    include_str!("render_view/ide_panels/ide_panel_project_search_renderer.rs");
const FILE_TREE_UI: &str = include_str!("render_view/ide_panels/ide_panel_side_renderer.rs");
const TERMINAL_UI: &str = include_str!("render_view/terminal_ui.rs");
const SEARCH_UI: &str = include_str!("render_view/search.rs");
const STICKY_UI: &str = include_str!("render_view/sticky.rs");
const CORE_TEXT: &str = include_str!("render_view/core_text.rs");
const EDITOR_TEXT: &str = include_str!("render_view/editor_text_layer.rs");
const LSP_UI: &str = include_str!("render_view/lsp_ui.rs");
const UI_SYSTEM: &str = include_str!("ui_system.rs");
const API_STATE: &str = include_str!("app/api_client.rs");
const API_REQUESTS: &str = include_str!("app/api_client/api_client_app_request_methods.rs");
const API_TEXT: &str = include_str!("app/api_client/api_client_app_text_methods.rs");
const FILE_TREE_SCAN: &str = include_str!("app/file_tree_scan.rs");
const PROJECT_SEARCH: &str = include_str!("app/project_search.rs");
const PROJECT_PREVIEW: &str = include_str!("app/project_search_preview.rs");
const TERMINAL_PROCESS: &str = include_str!("app/terminal_process.rs");
const GIT_DIFF: &str = include_str!("app/git_diff.rs");
const GIT_ACTIONS: &str = include_str!("app/git_panel/git_panel_app_action_methods.rs");
const MAIN_SOURCE: &str = include_str!("main.rs");

fn has_all(source: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            source.contains(needle),
            "missing source invariant: {needle}"
        );
    }
}

fn has_none(source: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            !source.contains(needle),
            "forbidden source pattern remains: {needle}"
        );
    }
}

macro_rules! clipped_control_test {
    ($name:ident, $source:expr, $id:literal) => {
        #[test]
        fn $name() {
            has_all($source, &["push_clip", $id]);
            has_all(UI_SYSTEM, &["active_clip", "intersect"]);
        }
    };
}

clipped_control_test!(
    r2_015_import_menu_items_follow_panel_clip,
    API_PANEL,
    "ApiImportFile"
);
clipped_control_test!(
    r2_016_import_url_input_and_confirm_follow_panel_clip,
    API_PANEL,
    "ApiImportUrlConfirm"
);
clipped_control_test!(
    r2_017_mock_server_toolbar_follows_panel_clip,
    API_PANEL,
    "ApiMockServerToggle"
);
clipped_control_test!(
    r2_018_copy_status_and_details_follow_panel_clip,
    API_PANEL,
    "ApiMockServerCopyUrl"
);
clipped_control_test!(
    r2_019_export_and_mock_actions_follow_panel_clip,
    API_PANEL,
    "ApiMockExportOpenApi"
);
clipped_control_test!(
    r2_020_spec_cards_follow_panel_clip,
    API_PANEL,
    "ApiSpecSelect"
);
clipped_control_test!(
    r2_021_spec_refresh_follows_panel_clip,
    API_PANEL,
    "ApiSpecRefresh"
);
clipped_control_test!(
    r2_022_auth_root_tags_and_filter_clear_follow_panel_clip,
    API_PANEL,
    "ApiRouteFilterClear"
);
clipped_control_test!(
    r2_023_route_filter_input_follows_panel_clip,
    API_PANEL,
    "ApiRouteFilterInput"
);
clipped_control_test!(
    r2_024_route_group_controls_follow_panel_clip,
    API_PANEL,
    "ApiRouteTag"
);
clipped_control_test!(
    r2_025_auth_scheme_fields_follow_tab_clip,
    API_TAB,
    "push_clip"
);
#[test]
fn r2_026_auth_related_routes_follow_tab_clip() {
    has_all(API_TAB, &["push_clip", "UiId::ApiRouteRow"]);
    has_all(UI_SYSTEM, &["active_clip", "intersect"]);
}

#[test]
fn r2_027_auth_row_width_is_derived_from_available_width() {
    has_all(
        API_AUTH,
        &["ApiAuthRowLayout", "api_auth_row_layout", "compact_actions"],
    );
    has_none(API_AUTH, &["input_w: 260.0", "save_w: 78.0"]);
}

#[test]
fn r2_028_auth_actions_use_compact_labels_when_space_is_tight() {
    has_all(API_AUTH, &["layout.compact_actions", "\"✓\"", "\"×\""]);
}

clipped_control_test!(
    r2_029_scrolled_api_text_inputs_follow_tab_clip,
    API_TAB,
    "ApiTabBody"
);
clipped_control_test!(
    r2_030_hidden_send_button_cannot_send,
    API_TAB,
    "ApiTryRequest"
);
clipped_control_test!(
    r2_031_response_body_input_follows_tab_clip,
    API_TAB,
    "ApiResponseBody"
);
clipped_control_test!(
    r2_032_server_chips_follow_tab_clip,
    API_TAB,
    "ApiServerSelect"
);
clipped_control_test!(
    r2_033_response_tabs_and_actions_follow_tab_clip,
    API_TAB,
    "ApiResponseBodyTab"
);
clipped_control_test!(
    r2_034_partial_request_rows_follow_tab_clip,
    API_TAB,
    "ApiPathParamInput"
);

#[test]
fn r2_035_settings_controls_share_one_body_clip() {
    has_all(
        SETTINGS,
        &[
            "push_clip",
            "SettingsToolInstall",
            "SettingsIdeAddWorkspace",
        ],
    );
}

#[test]
fn r2_036_settings_ignore_input_has_one_registration_path() {
    assert_eq!(SETTINGS.matches("UiId::SettingsIdeIgnoreInput").count(), 1);
}

#[test]
fn r2_037_ignore_input_does_not_disable_the_outer_settings_scissor() {
    let input = SETTINGS.find("SettingsIdeIgnoreInput").unwrap();
    let following = &SETTINGS[input..SETTINGS.len().min(input + 1800)];
    assert!(!following.contains("disable(glow::SCISSOR_TEST)"));
}

#[test]
fn r2_038_settings_restores_scissor_only_after_body_rendering() {
    has_all(
        SETTINGS,
        &[
            "ui_registry.pop_clip()",
            "self.gl.disable(glow::SCISSOR_TEST)",
        ],
    );
    assert!(
        SETTINGS.rfind("ui_registry.pop_clip()").unwrap()
            < SETTINGS
                .rfind("self.gl.disable(glow::SCISSOR_TEST)")
                .unwrap()
    );
}

#[test]
fn r2_039_settings_content_width_is_responsive() {
    has_all(SETTINGS, &["content_w", ".max(0.0)"]);
    has_none(SETTINGS, &["let content_w = 460.0"]);
}

#[test]
fn r2_040_settings_action_rows_fit_available_width() {
    has_all(
        SETTINGS_TOOL_ROWS,
        &[
            "action_count",
            "action_right",
            "action_left",
            "action_gap",
            "action_w",
        ],
    );
}

#[test]
fn r2_041_settings_ignore_uses_shared_one_line_renderer() {
    has_all(
        SETTINGS,
        &[
            "draw_one_line_input_with_chrome",
            "one_line_scroll_for_cursor",
        ],
    );
    has_none(SETTINGS, &["get_glyph(ch, 1.0)"]);
}

#[test]
fn r2_042_offscreen_settings_buttons_are_disabled_by_clip_stack() {
    has_all(SETTINGS, &["push_clip", "pop_clip"]);
    has_all(UI_SYSTEM, &["interactions_enabled", "active_clip"]);
}

#[test]
fn r2_043_git_commit_has_dedicated_horizontal_scroll() {
    has_all(
        GIT_UI,
        &["git_commit_scroll_x", "one_line_scroll_for_cursor"],
    );
}

#[test]
fn r2_044_git_commit_uses_shared_input_renderer() {
    has_all(GIT_UI, &["draw_one_line_input_with_chrome"]);
    has_none(GIT_UI, &["get_glyph(ch, 1.0)"]);
}

clipped_control_test!(r2_045_git_rows_follow_list_clip, GIT_UI, "UiId::GitFile");

#[test]
fn r2_046_git_rows_disable_interaction_while_scrolling() {
    has_all(
        GIT_UI,
        &[
            "push_interactions_enabled(hover_settled)",
            "pop_interactions_enabled",
        ],
    );
}

clipped_control_test!(
    r2_047_git_row_actions_follow_list_clip,
    GIT_UI,
    "UiId::GitFileDiff"
);

#[test]
fn r2_048_api_tab_drag_and_renderer_share_width_helper() {
    has_all(TABS_UI, &["editor_tab_width"]);
    has_all(TAB_INPUT, &["editor_tab_width(tab, title, s)"]);
}

#[test]
fn r2_049_close_hitbox_exists_only_when_close_icon_is_visible() {
    has_all(
        TABS_UI,
        &[
            "let show_close = is_active || is_hovered",
            "if show_close",
            "if show_close && close_rect_right",
        ],
    );
}

#[test]
fn r2_050_stale_tab_drag_index_is_bounds_checked() {
    has_all(TAB_INPUT, &["drag.start_idx < self.tabs.len()"]);
}

clipped_control_test!(
    r2_051_project_file_rows_follow_list_clip,
    PROJECT_SEARCH_UI,
    "ProjectSearchFileToggle"
);
clipped_control_test!(
    r2_052_project_match_rows_follow_list_clip,
    PROJECT_SEARCH_UI,
    "ProjectSearchMatchJump"
);

#[test]
fn r2_053_project_search_disables_rows_during_smooth_scroll() {
    has_all(
        PROJECT_SEARCH_UI,
        &["push_interactions_enabled(interactions_settled)"],
    );
}

clipped_control_test!(
    r2_054_file_tree_nodes_follow_tree_clip,
    FILE_TREE_UI,
    "FileTreeNode"
);
clipped_control_test!(
    r2_055_file_tree_disclosure_follows_tree_clip,
    FILE_TREE_UI,
    "FileTreeArrow"
);

#[test]
fn r2_056_file_tree_disables_rows_during_inertial_scroll() {
    has_all(FILE_TREE_UI, &["push_interactions_enabled(hover_settled)"]);
}

#[test]
fn r2_057_terminal_tabs_fit_reserved_bar_width() {
    for panel_w in [0.0, 20.0, 50.0, 100.0, 640.0] {
        for tab_count in [0, 1, 2, 20, 200] {
            let layout = crate::render_view::terminal_ui::terminal_tabs_metrics(
                10.0, panel_w, tab_count, 1.0,
            );
            let used =
                layout.per_tab * tab_count as f32 + layout.gap * tab_count.saturating_sub(1) as f32;
            assert!(used <= layout.available + 0.001);
            assert!(layout.add_x >= 10.0);
            assert!(layout.add_x + layout.add_size <= 10.0 + panel_w.max(0.0) + 0.001);
        }
    }
}

#[test]
fn r2_058_terminal_close_and_add_follow_tab_bar_clip() {
    has_all(
        TERMINAL_UI,
        &["push_clip", "TerminalTabClose", "TerminalAdd"],
    );
}

#[test]
fn r2_059_terminal_search_width_is_clamped_to_viewport() {
    for panel_w in [0.0, 4.0, 20.0, 100.0, 640.0] {
        let layout = crate::render_view::terminal_ui::terminal_search_geometry(25.0, panel_w, 1.0);
        assert!(layout.x >= 25.0);
        assert!(layout.w >= 0.0);
        assert!(layout.x + layout.w <= 25.0 + panel_w.max(0.0) + 0.001);
        assert!(layout.input_w >= 0.0);
        assert!(layout.close_x >= layout.x);
        assert!(layout.close_x + layout.close_size <= layout.x + layout.w + 0.001);
    }
}

#[test]
fn r2_060_terminal_query_has_horizontal_scroll() {
    has_all(
        TERMINAL_UI,
        &["terminal_search_scroll_x", "one_line_scroll_for_cursor"],
    );
}

#[test]
fn r2_061_terminal_search_uses_shared_input_renderer() {
    has_all(TERMINAL_UI, &["draw_one_line_input_with_chrome"]);
    has_none(TERMINAL_UI, &["get_glyph(ch, 1.0)"]);
}

#[test]
fn r2_062_main_search_never_uses_negative_x() {
    for scrollbar_x in [-100.0, 0.0, 4.0, 20.0, 100.0, 640.0] {
        let layout = crate::render_view::search::search_panel_geometry(scrollbar_x, 1.0);
        assert!(layout.x >= 0.0);
        assert!(layout.w >= 0.0);
        assert!(layout.close_x >= layout.x);
        assert!(layout.close_x + layout.close_size <= layout.x + layout.w + 0.001);
    }
}

#[test]
fn r2_063_main_search_blocker_is_bounded_by_viewport() {
    has_all(SEARCH_UI, &["search_x", "search_w", "register_blocker"]);
}

#[test]
fn r2_064_main_search_uses_shared_input_renderer() {
    has_all(
        SEARCH_UI,
        &[
            "draw_one_line_input_with_chrome",
            "one_line_scroll_for_cursor",
        ],
    );
    has_none(SEARCH_UI, &["get_glyph(ch, 1.0)"]);
}

#[test]
fn r2_065_sticky_lines_use_the_same_editor_font_renderer() {
    has_all(STICKY_UI, &["draw_spanned_editor_line_alpha"]);
    has_none(STICKY_UI, &["draw_spanned_ui_line_pixel_snapped_alpha"]);
    has_all(CORE_TEXT, &["fn push_editor_glyph", "self.get_glyph(ch)"]);
    has_all(EDITOR_TEXT, &["self.push_editor_glyph"]);
}

clipped_control_test!(
    r2_066_partial_lsp_card_buttons_follow_outer_clip,
    LSP_UI,
    "LspServerRestart"
);

#[test]
fn r2_067_lsp_action_widths_are_fitted_to_card() {
    has_all(
        LSP_UI,
        &[
            "fit_lsp_action_widths",
            "usable",
            "minimum_width",
            "natural_total",
        ],
    );
}

clipped_control_test!(
    r2_068_lsp_fold_arrow_follows_inner_log_clip,
    LSP_UI,
    "LspLogFoldToggle"
);
clipped_control_test!(
    r2_069_lsp_folded_dots_follow_inner_log_clip,
    LSP_UI,
    "LspLogArea"
);

#[test]
fn r2_073_old_api_load_success_cannot_finish_new_generation() {
    let mut state = crate::app::api_client::ApiClientState::default();
    let id = crate::app::api_client::ApiSpecId(7);
    let old = state.begin_load(id, false);
    let new = state.begin_load(id, false);
    assert_ne!(old, new);
    assert!(state.finish_load(id, old).is_none());
    assert!(state.loading.contains(&id));
}

#[test]
fn r2_074_old_api_load_error_is_rejected_by_generation_gate() {
    let mut state = crate::app::api_client::ApiClientState::default();
    let id = crate::app::api_client::ApiSpecId(8);
    let old = state.begin_load(id, false);
    let new = state.begin_load(id, false);
    assert!(!state.is_current_load(id, old));
    assert!(state.is_current_load(id, new));
}

#[test]
fn r2_075_first_old_refresh_does_not_clear_current_loading() {
    let mut state = crate::app::api_client::ApiClientState::default();
    let id = crate::app::api_client::ApiSpecId(9);
    let first = state.begin_load(id, false);
    let current = state.begin_load(id, false);
    assert!(state.finish_load(id, first).is_none());
    assert!(state.loading.contains(&id));
    assert!(state.finish_load(id, current).is_some());
    assert!(!state.loading.contains(&id));
}

#[test]
fn r2_076_removing_spec_removes_its_load_ticket() {
    has_all(
        API_STATE,
        &["self.loading.remove(&id)", "self.load_tickets.remove(&id)"],
    );
}

#[test]
fn r2_077_background_refresh_does_not_select_spec() {
    has_all(API_TEXT, &["begin_load(id, false)"]);
    assert!(API_TEXT.matches("begin_load(id, false)").count() >= 2);
}

#[test]
fn r2_078_spec_id_wraparound_skips_live_ids() {
    let mut state = crate::app::api_client::ApiClientState::default();
    state.next_id = u64::MAX;
    state.load_tickets.insert(
        crate::app::api_client::ApiSpecId(u64::MAX),
        crate::app::api_client::ApiLoadTicket {
            generation: 1,
            select_on_success: false,
        },
    );
    state.load_tickets.insert(
        crate::app::api_client::ApiSpecId(1),
        crate::app::api_client::ApiLoadTicket {
            generation: 2,
            select_on_success: false,
        },
    );
    assert_eq!(state.alloc_spec_id(), crate::app::api_client::ApiSpecId(2));
}

#[test]
fn r2_079_request_id_allocator_wraps_and_checks_active_ids() {
    has_all(
        API_REQUESTS,
        &[
            "allocate_api_request_id",
            "wrapping_add(1).max(1)",
            "active_receiver",
            "active_tab",
        ],
    );
}

#[test]
fn r2_080_spec_persistence_error_is_stored_logged_and_rendered() {
    has_all(
        API_STATE,
        &[
            "persistence_error = result.err()",
            "eprintln!(\"RRiter: {error}\")",
        ],
    );
    has_all(API_PANEL, &["api.persistence_error.as_deref()"]);
}

#[test]
fn r2_081_auth_persistence_error_reaches_shared_error_state() {
    has_all(
        API_STATE,
        &["save_api_auth(&self.auth)", "API credentials не сохранены"],
    );
}

#[test]
fn r2_082_mock_persistence_error_reaches_shared_error_state() {
    has_all(
        API_STATE,
        &[
            "save_api_mocks(&self.mock)",
            "API mock configuration не сохранена",
        ],
    );
}

#[test]
fn r2_083_open_tabs_and_panel_persistence_errors_are_logged() {
    has_all(
        MAIN_SOURCE,
        &[
            "failed to persist open tabs",
            "failed to persist panel state",
        ],
    );
}

#[test]
fn r2_084_api_workers_use_fallible_named_spawns() {
    let handle = crate::platform::spawn_named("r2-fallible-worker", || 84usize)
        .expect("named worker should be created in the test environment");
    assert_eq!(handle.join().expect("named worker should finish"), 84);
}

#[test]
fn r2_085_file_tree_and_project_search_workers_use_fallible_spawns() {
    for source in [FILE_TREE_SCAN, PROJECT_SEARCH, PROJECT_PREVIEW] {
        assert!(source.contains("spawn_named"));
        assert!(!source.contains("std::thread::spawn"));
    }
    has_all(
        PROJECT_SEARCH,
        &[
            "ProjectSearchWorkerMessage::Done",
            "не удалось запустить поиск",
        ],
    );
}

#[test]
fn r2_086_terminal_io_threads_propagate_spawn_failure() {
    assert_eq!(
        TERMINAL_PROCESS
            .matches("spawn_named(\"rriter-terminal-")
            .count(),
        3
    );
    has_all(
        TERMINAL_PROCESS,
        &[
            "install_terminal_io_threads",
            "-> io::Result<()>",
            "terminate_forcefully",
        ],
    );
    assert!(!TERMINAL_PROCESS.contains("std::thread::spawn"));
}

#[test]
fn r2_087_git_workers_use_fallible_spawns_and_error_events() {
    for source in [GIT_DIFF, GIT_ACTIONS] {
        assert!(source.contains("spawn_named"));
        assert!(!source.contains("std::thread::spawn"));
    }
    has_all(
        GIT_DIFF,
        &[
            "Не удалось запустить загрузку Git diff",
            "Не удалось запустить inline Git diff",
        ],
    );
    has_all(
        GIT_ACTIONS,
        &[
            "Не удалось запустить Git worker",
            "Не удалось запустить Git stage worker",
        ],
    );
}

#[test]
fn r2_088_event_loop_creation_is_handled_without_unwrap() {
    has_all(
        MAIN_SOURCE,
        &[
            "match event_loop_builder.build()",
            "не удалось создать event loop",
        ],
    );
    assert!(!MAIN_SOURCE.contains("event_loop_builder.build().unwrap()"));
}

#[test]
fn r2_089_event_loop_runtime_error_is_reported_without_unwrap() {
    has_all(
        MAIN_SOURCE,
        &[
            "if let Err(error) = event_loop.run_app(&mut app)",
            "event loop завершился с ошибкой",
        ],
    );
    assert!(!MAIN_SOURCE.contains("event_loop.run_app(&mut app).unwrap()"));
    assert_eq!(
        event_loop_error_message("stage", &"failure"),
        "RRiter: stage: failure"
    );
}
