use super::*;
use arboard::Clipboard;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn test_theme() -> crate::renderer::Theme {
    crate::renderer::Theme {
        bg: [0.156, 0.164, 0.211, 1.0],
        fg: [0.972, 0.972, 0.949, 1.0],
        sel: [0.55, 0.55, 0.55, 1.0],
        minimap_bg: [0.129, 0.133, 0.172, 1.0],
        line_num: [0.384, 0.447, 0.643, 1.0],
        minimap_cursor: [0.55, 0.55, 0.55, 1.0],
        modified_unsaved: [1.0, 0.474, 0.776, 1.0],
        modified_saved: [0.313, 0.980, 0.482, 1.0],
        diag_warn: [0.945, 0.980, 0.549, 1.0],
        diag_error: [1.0, 0.333, 0.333, 1.0],
        unused: [0.48, 0.48, 0.48, 0.6],
    }
}

fn editor_with(text: &str) -> Editor {
    let mut editor = Editor::new(text.len() + 64);
    let _ = editor.insert_str(text);
    editor.cursor = text.len();
    editor.clear_history();
    editor.set_original_text();
    editor.sync_edits.clear();
    editor
}

fn tab_with(title: &str, path: Option<&str>, text: &str) -> EditorTab {
    EditorTab {
        editor: editor_with(text),
        file_path: path.map(PathBuf::from),
        base_title: title.to_string(),
        file_extension: path
            .and_then(|p| std::path::Path::new(p).extension())
            .map(|ext| ext.to_string_lossy().to_string())
            .unwrap_or_default(),
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        spans: Vec::new(),
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        last_sent_version: 0,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: false,
        icon_key: "default_file",
        syntax_errors: Vec::new(),
        kind: EditorTabKind::Normal,
    }
}

fn test_app() -> Option<App> {
    let now = Instant::now();
    Some(App {
        pending_key_log: None,
        gl_config: None,
        gl_context: None,
        gl_surface: None,
        window: None,
        dialog_window: None,
        dialog_gl_surface: None,
        settings_scroll: crate::scroll::ScrollState::new(15.0),
        tab_scroll: crate::scroll::ScrollState::new(15.0),
        renderer: None,
        editor: Editor::new(128),
        clipboard: Clipboard::new().ok(),
        theme: test_theme(),
        base_title: "Безымянный".to_string(),
        file_path: None,
        file_extension: String::new(),
        highlighter: crate::highlighter::Highlighter::new(),
        last_sent_version: u64::MAX,
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        last_frame: now,
        last_action: now,
        last_blink_state: true,
        modifiers: winit::keyboard::ModifiersState::empty(),
        is_dragging: false,
        is_focused: true,
        render_suspended: false,
        current_cursor: winit::window::CursorIcon::Default,
        show_fps: false,
        window_width: 1000.0,
        window_height: 800.0,
        last_resize_time: None,
        last_click_time: now,
        click_count: 0,
        last_click_pos: (0.0, 0.0),
        pending_action: PendingAction::Quit,
        open_file_rx: None,
        save_file_rx: None,
        show_welcome: true,
        recent_files: Vec::new(),
        is_ide_mode: false,
        ide_workspaces: Vec::new(),
        ide_ignore_patterns: Vec::new(),
        settings_ignore_editor: Editor::new(128),
        settings_ignore_focused: false,
        settings_ignore_scroll_x: 0.0,
        is_dragging_settings_ignore: false,
        open_folder_rx: None,
        show_search: false,
        search_anim_y: -120.0,
        search_editor: Editor::new(256),
        search_focused: false,
        search_case_sensitive: false,
        search_results: Vec::new(),
        search_current_idx: None,
        is_dragging_search: false,
        is_dragging_lsp_log: false,
        faq_editor: Editor::new(128),
        is_ready: false,
        is_highlighted_once: false,
        tried_maximize: false,
        should_maximize: false,
        autocomplete_active: false,
        autocomplete_options: Vec::new(),
        autocomplete_selected_idx: 0,
        autocomplete_anim_progress: 0.0,
        autocomplete_scroll: crate::scroll::ScrollState::new(15.0),
        autocomplete_hovered_idx: None,
        autocomplete_rect: None,
        autocomplete_anchor: None,
        autocomplete_mode: AutocompleteMode::TreeSitter,
        autocomplete_pending_request_id: None,
        autocomplete_pending_request_mode: None,
        autocomplete_pending_request_path: None,
        autocomplete_pending_context_key: None,
        autocomplete_detail_request_id: None,
        autocomplete_detail_word: None,
        autocomplete_detail_request_path: None,
        autocomplete_detail_context_key: None,
        autocomplete_detail_popup: None,
        autocomplete_detail_rect: None,
        autocomplete_detail_placement: None,
        autocomplete_detail_max_scroll: 0.0,
        autocomplete_min_width: 0.0,
        autocomplete_detail_min_width: 0.0,
        autocomplete_detail_min_height: 0.0,
        autocomplete_detail_selection_anchor: None,
        autocomplete_detail_selection_cursor: None,
        autocomplete_detail_selecting: false,
        autocomplete_apply_pending_response: false,
        autocomplete_cache: None,
        autocomplete_detail_cache: None,
        current_sticky_lines: Vec::new(),
        target_sticky_lines: Vec::new(),
        sticky_anim_progress: 1.0,
        sticky_anim_is_adding: false,
        show_settings: false,
        settings_anim_progress: 0.0,
        settings_y: 10000.0,
        settings_tab: 0,
        settings_ide_scroll: crate::scroll::ScrollState::new(7.0),
        ide_panel: IdePanelState::default(),
        file_tree_rx: None,
        file_tree_notify_rx: None,
        external_changes_rx: None,
        git_diff_rx: Vec::new(),
        readonly_notice_until: None,
        lsp: None,
        lsp_actions_menu: None,
        pending_fix_all_id: None,
        ctrl_definition: CtrlDefinitionState::default(),
        ui_registry: crate::ui_system::UiRegistry::new(),
        tabs: Vec::new(),
        active_tab: 0,
        run_ide_on_startup: false,
    })
}

fn completion(
    word: &str,
    kind: SymbolKind,
    scope_start: usize,
    scope_end: usize,
) -> CompletionItem {
    CompletionItem {
        word: word.to_string(),
        kind,
        scope_start,
        scope_end,
    }
}

#[test]
fn search_update_finds_nearest_match_preserves_previous_and_honors_case() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("alpha beta\nAlpha beta\nbeta tail");
    app.editor.cursor = 18;
    app.search_editor = editor_with("beta");

    app.update_search();
    assert_eq!(app.search_results.len(), 3);
    assert_eq!(app.search_current_idx, Some(1));

    let previous = app.search_current_idx;
    app.update_search();
    assert_eq!(app.search_current_idx, previous);

    app.search_case_sensitive = true;
    app.search_editor = editor_with("Alpha");
    app.update_search();
    assert_eq!(app.search_results, vec![(11, 16)]);
    assert_eq!(app.search_current_idx, Some(0));

    app.search_editor = Editor::new(32);
    app.update_search();
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_current_idx, None);
}

#[test]
fn autocomplete_filters_scores_scrolls_and_applies_selected_completion() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("pri");
    app.editor.cursor = 3;
    app.highlighter.completions = vec![
        completion("print", SymbolKind::Function, 0, 100),
        completion("private_value", SymbolKind::Variable, 1, 100),
        completion("printf", SymbolKind::Function, 10, 20),
        completion("pri", SymbolKind::Variable, 0, 100),
    ];

    app.update_autocomplete();
    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_selected_idx, 0);
    assert_eq!(app.autocomplete_options.len(), 2);
    assert_eq!(app.autocomplete_options[0].0.word, "private_value");
    assert_eq!(app.autocomplete_options[1].0.word, "print");

    app.autocomplete_selected_idx = 1;
    app.autocomplete_scroll.target = 200.0;
    app.ensure_autocomplete_visible();
    assert!(app.autocomplete_scroll.target <= 36.0);

    app.apply_autocomplete();
    assert_eq!(app.editor.get_full_text(), "print");
    assert!(!app.autocomplete_active);
    assert_eq!(app.autocomplete_selected_idx, 0);
    assert_eq!(app.autocomplete_scroll.target, 0.0);
}

#[test]
fn ty_import_autocomplete_waits_for_prefix_and_requires_module() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("");
    app.autocomplete_mode = AutocompleteMode::TyImports;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "Path".to_string(),
        kind: SymbolKind::Class,
        module: Some("pathlib".to_string()),
        detail: Some("type[Path]".to_string()),
        insert_text: Some("Path".to_string()),
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);
    assert!(app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());

    app.editor = editor_with("Pa");
    app.autocomplete_mode = AutocompleteMode::TyImports;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "Path".to_string(),
            kind: SymbolKind::Class,
            module: Some("pathlib".to_string()),
            detail: Some("type[Path]".to_string()),
            insert_text: Some("Path".to_string()),
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "ParamSpec".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("typing special form".to_string()),
            insert_text: Some("ParamSpec".to_string()),
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);
    assert_eq!(app.autocomplete_options.len(), 1);
    assert_eq!(app.autocomplete_options[0].0.word, "Path");
    assert_eq!(
        app.autocomplete_options[0].0.module.as_deref(),
        Some("pathlib")
    );
}

#[test]
fn ty_import_autocomplete_promotes_unknown_items_with_module() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("pa");
    app.autocomplete_mode = AutocompleteMode::TyImports;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "pathlib".to_string(),
        kind: SymbolKind::Unknown,
        module: Some("stdlib".to_string()),
        detail: None,
        insert_text: Some("pathlib".to_string()),
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    assert_eq!(app.autocomplete_options.len(), 1);
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Module);
}

#[test]
fn ty_context_top_level_variable_keeps_variable_kind_and_hides_type_source() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("bo");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "box".to_string(),
        kind: SymbolKind::Class,
        module: Some("BoxRead".to_string()),
        detail: Some("(variable) box: BoxRead".to_string()),
        insert_text: Some("box".to_string()),
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    assert_eq!(app.autocomplete_options.len(), 1);
    assert_eq!(app.autocomplete_options[0].0.word, "box");
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Variable);
    assert_eq!(app.autocomplete_options[0].0.module, None);
    assert_eq!(app.autocomplete_options[0].0.module_path, None);
}

#[test]
fn ty_context_top_level_lowercase_type_source_is_variable_without_detail() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("bo");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "box".to_string(),
        kind: SymbolKind::Class,
        module: Some("BoxRead".to_string()),
        detail: None,
        insert_text: Some("box".to_string()),
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    assert_eq!(app.autocomplete_options.len(), 1);
    assert_eq!(app.autocomplete_options[0].0.word, "box");
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Variable);
    assert_eq!(app.autocomplete_options[0].0.module, None);
    assert_eq!(app.autocomplete_options[0].0.module_path, None);
}

#[test]
fn autocomplete_detail_merge_does_not_flip_top_level_variable_to_type() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("bo");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_detail_word = Some("box".to_string());
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "box".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: usize::MAX,
            module: None,
            module_path: None,
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "box".to_string(),
        kind: SymbolKind::Class,
        module: Some("BoxRead".to_string()),
        detail: Some("(variable) box: BoxRead".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Variable);
    assert_eq!(app.autocomplete_options[0].0.module, None);
    assert_eq!(app.autocomplete_options[0].0.module_path, None);
}

#[test]
fn autocomplete_detail_merge_hides_lowercase_type_source_without_detail() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("bo");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_detail_word = Some("box".to_string());
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "box".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: usize::MAX,
            module: None,
            module_path: None,
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "box".to_string(),
        kind: SymbolKind::Class,
        module: Some("BoxRead".to_string()),
        detail: None,
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Variable);
    assert_eq!(app.autocomplete_options[0].0.module, None);
    assert_eq!(app.autocomplete_options[0].0.module_path, None);
}

#[test]
fn autocomplete_detail_merge_keeps_parameter_badge_when_ty_reports_type() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("def lifespan(_: Litestar, arg: str):\n    ar");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TreeSitter;
    app.autocomplete_detail_word = Some("arg".to_string());
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "arg".to_string(),
            kind: SymbolKind::Parameter,
            scope_start: 0,
            scope_end: usize::MAX,
            module: None,
            module_path: None,
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "arg".to_string(),
        kind: SymbolKind::Class,
        module: Some("builtins.str".to_string()),
        detail: Some("str".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let item = &app.autocomplete_options[0].0;
    assert_eq!(item.kind, SymbolKind::Parameter);
    assert_eq!(item.detail.as_deref(), Some("str"));
}

#[test]
fn autocomplete_orders_magic_names_after_regular_members_and_merges_lazy_detail() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("st");
    app.editor.cursor = 2;
    app.highlighter.completions = vec![
        completion("__str__", SymbolKind::Function, 0, 100),
        completion("strip", SymbolKind::Function, 0, 100),
    ];

    app.update_autocomplete();
    assert_eq!(app.autocomplete_options[0].0.word, "strip");
    assert_eq!(app.autocomplete_options[1].0.word, "__str__");
    assert!(app.autocomplete_options[0].0.detail.is_none());

    app.autocomplete_detail_word = Some("strip".to_string());
    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "strip".to_string(),
        kind: SymbolKind::Function,
        module: Some("str".to_string()),
        detail: Some("(chars: str | None = None) -> str".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);
    assert_eq!(
        app.autocomplete_options[0].0.detail.as_deref(),
        Some("(chars: str | None = None) -> str")
    );

    app.autocomplete_detail_word = Some("strip".to_string());
    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "strip".to_string(),
        kind: SymbolKind::Function,
        module: Some("builtins.str".to_string()),
        detail: None,
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);
    assert_eq!(
        app.autocomplete_options[0].0.module.as_deref(),
        Some("builtins.str")
    );
}

#[test]
fn autocomplete_detail_popup_uses_hover_text_and_selection_state() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("box.");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "id".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("BoxReadPublic".to_string()),
            module_path: None,
            detail: Some("(variable) id: int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(popup.text, "(variable) BoxReadPublic.id: int");
    assert!(popup.line_kinds.len() >= 1);

    app.autocomplete_detail_selection_anchor = Some(11);
    app.autocomplete_detail_selection_cursor = Some(24);
    assert_eq!(
        app.selected_autocomplete_detail_text().as_deref(),
        Some("BoxReadPublic")
    );
}

#[test]
fn autocomplete_detail_size_grows_without_shrinking_during_navigation() {
    let Some(mut app) = test_app() else {
        return;
    };
    assert_eq!(
        app.stable_autocomplete_detail_size(80.0, 40.0, 120.0),
        (80.0, 40.0)
    );
    assert_eq!(
        app.stable_autocomplete_detail_size(300.0, 160.0, 120.0),
        (300.0, 120.0)
    );
    assert_eq!(
        app.stable_autocomplete_detail_size(90.0, 50.0, 120.0),
        (300.0, 120.0)
    );
    app.reset_autocomplete_detail_size();
    assert_eq!(
        app.stable_autocomplete_detail_size(90.0, 50.0, 120.0),
        (90.0, 50.0)
    );
}

#[test]
fn autocomplete_detail_request_replaces_stale_popup_with_placeholder() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("c");
    app.file_path = Some(PathBuf::from("/tmp/current.py"));
    app.file_extension = "py".to_string();
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TreeSitter;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "config".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("car_wash.config".to_string()),
            module_path: Some("car_wash.config".to_string()),
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];
    app.autocomplete_detail_popup = Some(crate::app::mouse::HoverPopup {
        text: "class BookingChangeToPreviousStateError".to_string(),
        spans: Vec::new(),
        line_kinds: vec![crate::lsp::HoverLineKindPublic::Text],
        inline_code_ranges: Vec::new(),
        byte_offset: 0,
        anchor_x: 0.0,
        anchor_y: 0.0,
        offset_x: Some(0.0),
        offset_y: Some(0.0),
        anim_progress: 1.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });

    app.request_autocomplete_detail_for_index(0);

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(popup.text, "Unknown");
    assert!(app.autocomplete_detail_rect.is_none());
    assert_eq!(app.autocomplete_detail_placement, None);
}

#[test]
fn autocomplete_detail_popup_prepends_full_module_path() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("RepoBase.initialize_all");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "initialize_all".to_string(),
            kind: SymbolKind::Function,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("RepoBase".to_string()),
            module_path: Some("car_wash.core.db.repo_base.RepoBase".to_string()),
            detail: Some("def RepoBase.initialize_all() -> None".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(
        popup
            .text
            .starts_with("[[MODULE]] car_wash.core.db.repo_base\n")
    );
    assert_eq!(
        popup.line_kinds.first().copied(),
        Some(crate::lsp::HoverLineKindPublic::Text)
    );
}

#[test]
fn autocomplete_detail_popup_cleans_source_attr_class_type_union() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("self.repository");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "repository".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("BookingService".to_string()),
            module_path: Some(
                "car_wash.domains.washes.bookings.repository.BookingRepository".to_string(),
            ),
            detail: Some(
                "<class 'BookingRepository'> | type[AsyncpgRepository[Unknown, Unknown]]"
                    .to_string(),
            ),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.starts_with(
        "[[MODULE]] car_wash.domains.washes.bookings.repository\nclass BookingRepository"
    ));
    assert!(!popup.text.contains("Unknown"));
    assert!(!popup.spans.is_empty());
}

#[test]
fn autocomplete_detail_popup_formats_python_overload_docs() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("getattr");
    app.autocomplete_active = true;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "getattr".to_string(),
            kind: SymbolKind::Function,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("builtins".to_string()),
            module_path: Some("builtins.getattr".to_string()),
            detail: Some("Overload[(o: object, name: str, /) -> Any]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.contains("@overload"));
    assert!(popup.text.contains("def getattr(__o: object,"));
    assert!(popup.text.contains("Get a named attribute from an object"));
    assert!(!popup.spans.is_empty());
    assert!(
        popup
            .line_kinds
            .iter()
            .any(|kind| *kind == crate::lsp::HoverLineKindPublic::Separator)
    );

    app.autocomplete_options[0].0 = AutocompleteItem {
        word: "asynccontextmanager".to_string(),
        kind: SymbolKind::Function,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("contextlib".to_string()),
        module_path: None,
        detail: Some("Overload[[**_P, _T_co](func: (**_P) -> AsyncIterator[_T_co])]".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    };

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.contains("def asynccontextmanager("));
    assert!(popup.text.contains("@asynccontextmanager decorator"));
    assert!(!popup.spans.is_empty());
    let paramspec_pos = popup.text.find("ParamSpec").unwrap();
    assert!(popup.spans.iter().any(|span| {
        span.start <= paramspec_pos
            && paramspec_pos < span.end
            && span.color == crate::highlighter::DRACULA_CYAN
    }));

    app.autocomplete_options[0].0 = AutocompleteItem {
        word: "cast".to_string(),
        kind: SymbolKind::Function,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("typing".to_string()),
        module_path: Some("typing".to_string()),
        detail: Some(
            "Overload[[_T](typ: type[_T], val: Any) -> _T, (typ: str, val: Any) -> Any, (typ: object, val: Any) -> Any]"
                .to_string(),
        ),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    };

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.contains("def cast(typ: type[_T],"));
    assert!(popup.text.contains("Cast a value to a type"));
    assert!(!popup.text.contains("Overload["));
    assert!(!popup.spans.is_empty());
}

#[test]
fn close_autocomplete_clears_detail_popup_and_selection() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.autocomplete_active = true;
    app.autocomplete_selected_idx = 4;
    app.autocomplete_hovered_idx = Some(2);
    app.autocomplete_rect = Some((9.0, 8.0, 7.0, 6.0));
    app.autocomplete_anchor = Some((10.0, 20.0));
    app.autocomplete_pending_request_id = Some(1);
    app.autocomplete_detail_request_id = Some(2);
    app.autocomplete_detail_word = Some("id".to_string());
    app.autocomplete_detail_popup = Some(crate::app::mouse::HoverPopup {
        text: "detail".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 0,
        anchor_x: 0.0,
        anchor_y: 0.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 1.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    app.autocomplete_detail_rect = Some((1.0, 2.0, 3.0, 4.0));
    app.autocomplete_detail_placement = Some(1);
    app.autocomplete_min_width = 240.0;
    app.autocomplete_detail_selection_anchor = Some(0);
    app.autocomplete_detail_selection_cursor = Some(3);
    app.autocomplete_detail_selecting = true;

    app.close_autocomplete();

    assert!(!app.autocomplete_active);
    assert_eq!(app.autocomplete_selected_idx, 0);
    assert_eq!(app.autocomplete_hovered_idx, None);
    assert_eq!(app.autocomplete_rect, None);
    assert_eq!(app.autocomplete_anchor, None);
    assert_eq!(app.autocomplete_pending_request_id, None);
    assert_eq!(app.autocomplete_detail_request_id, None);
    assert_eq!(app.autocomplete_detail_word, None);
    assert!(app.autocomplete_detail_popup.is_none());
    assert!(app.autocomplete_detail_rect.is_none());
    assert_eq!(app.autocomplete_detail_placement, None);
    assert_eq!(app.autocomplete_min_width, 0.0);
    assert_eq!(app.autocomplete_detail_selection_anchor, None);
    assert_eq!(app.autocomplete_detail_selection_cursor, None);
    assert!(!app.autocomplete_detail_selecting);
}

#[test]
fn tree_sitter_refresh_does_not_close_active_ty_member_completion() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("box.");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_pending_request_id = Some(77);

    app.update_autocomplete();

    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_mode, AutocompleteMode::TyContext);
    assert_eq!(app.autocomplete_pending_request_id, Some(77));
}

#[test]
fn ty_context_completion_closes_after_deleted_dot_or_empty_argument() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_pending_request_id = Some(9);
    app.editor = editor_with("box");

    app.update_autocomplete();
    assert!(!app.autocomplete_active);
    assert_eq!(app.autocomplete_pending_request_id, None);

    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_pending_request_id = Some(10);
    app.editor = editor_with("call(");

    app.update_autocomplete();
    assert!(!app.autocomplete_active);
    assert_eq!(app.autocomplete_pending_request_id, None);
}

#[test]
fn pending_ty_context_enter_applies_first_response_without_newline() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("box.");
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_pending_request_id = Some(7);
    app.autocomplete_apply_pending_response = true;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "id".to_string(),
            kind: SymbolKind::Variable,
            module: Some("BoxRead".to_string()),
            detail: Some("(variable) BoxRead.id: int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "name".to_string(),
            kind: SymbolKind::Variable,
            module: Some("BoxRead".to_string()),
            detail: Some("(variable) BoxRead.name: str".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    assert_eq!(app.editor.get_full_text(), "box.id");
    assert!(!app.autocomplete_apply_pending_response);
    assert!(!app.editor.get_full_text().contains('\n'));
}

#[test]
fn ty_completion_cache_uses_request_context_not_current_cursor() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.file_path = Some(PathBuf::from("/tmp/main.py"));
    app.autocomplete_mode = AutocompleteMode::TyContext;

    let request_text = "box.";
    let request_editor = editor_with(request_text);
    let request_key = ty_autocomplete_context_key(
        request_text,
        &request_editor.line_offsets,
        request_editor.cursor,
        "",
        AutocompleteMode::TyContext,
    );
    app.autocomplete_pending_request_id = Some(8);
    app.autocomplete_pending_request_mode = Some(AutocompleteMode::TyContext);
    app.autocomplete_pending_request_path = app.file_path.clone();
    app.autocomplete_pending_context_key = Some(request_key.clone());

    app.editor = editor_with("box.user_id.");
    app.remember_ty_autocomplete_cache(vec![crate::lsp::LspCompletionItem {
        label: "user_id".to_string(),
        kind: SymbolKind::Variable,
        module: Some("BoxRead".to_string()),
        detail: Some("(variable) BoxRead.user_id: int | None".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let cache = app.autocomplete_cache.as_ref().unwrap();
    assert_eq!(cache.context_key, request_key);
    assert_ne!(
        cache.context_key,
        ty_autocomplete_context_key(
            &app.editor.get_full_text(),
            &app.editor.line_offsets,
            app.editor.cursor,
            "",
            AutocompleteMode::TyContext,
        )
    );
    assert_eq!(app.autocomplete_pending_request_mode, None);
    assert_eq!(app.autocomplete_pending_request_path, None);
    assert_eq!(app.autocomplete_pending_context_key, None);
}

#[test]
fn ty_completion_cache_ignores_uncacheable_prefix_response() {
    let Some(mut app) = test_app() else {
        return;
    };
    let path = PathBuf::from("/tmp/main.py");
    app.autocomplete_cache = Some(AutocompleteCacheEntry {
        mode: AutocompleteMode::TyContext,
        path: path.clone(),
        context_key: "TyContext|box.".to_string(),
        items: vec![
            crate::lsp::LspCompletionItem {
                label: "user_id".to_string(),
                kind: SymbolKind::Variable,
                module: Some("BoxRead".to_string()),
                detail: Some("(variable) BoxRead.user_id: int | None".to_string()),
                insert_text: None,
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            crate::lsp::LspCompletionItem {
                label: "car_wash".to_string(),
                kind: SymbolKind::Variable,
                module: Some("BoxRead".to_string()),
                detail: Some("(variable) BoxRead.car_wash: CarWashRead".to_string()),
                insert_text: None,
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
        ],
    });

    app.autocomplete_pending_request_mode = None;
    app.autocomplete_pending_request_path = None;
    app.autocomplete_pending_context_key = None;
    app.remember_ty_autocomplete_cache(vec![crate::lsp::LspCompletionItem {
        label: "user_id".to_string(),
        kind: SymbolKind::Variable,
        module: Some("BoxRead".to_string()),
        detail: Some("(variable) BoxRead.user_id: int | None".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let cache = app.autocomplete_cache.as_ref().unwrap();
    assert_eq!(cache.path, path);
    assert_eq!(cache.context_key, "TyContext|box.");
    assert_eq!(cache.items.len(), 2);
    assert!(cache.items.iter().any(|item| item.label == "car_wash"));
}

#[test]
fn ty_context_prioritizes_call_argument_completions() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("repo.find_one(id, a");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "apple".to_string(),
            kind: SymbolKind::Variable,
            module: None,
            detail: Some("(variable) apple: str".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "allow_none".to_string(),
            kind: SymbolKind::Variable,
            module: None,
            detail: Some("(parameter) allow_none: bool = True".to_string()),
            insert_text: Some("allow_none=".to_string()),
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    assert_eq!(app.autocomplete_options[0].0.word, "allow_none");
}

#[test]
fn ty_context_preserves_explicit_owner_and_hides_attribute_types() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("box.");
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "id".to_string(),
            kind: SymbolKind::Variable,
            module: Some("BoxReadPublic".to_string()),
            detail: Some("(variable) BoxReadPublic.id: int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "model_dump".to_string(),
            kind: SymbolKind::Function,
            module: Some("car_wash.domains.washes.boxes.output.BoxRead".to_string()),
            detail: Some("def BoxRead.model_dump(self) -> dict".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);
    let id = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "id")
        .unwrap();
    assert_eq!(id.0.module.as_deref(), Some("BoxReadPublic"));
    let model_dump = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "model_dump")
        .unwrap();
    assert_eq!(model_dump.0.module.as_deref(), Some("BoxRead"));

    app.editor = editor_with("box.");
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "id".to_string(),
            kind: SymbolKind::Class,
            module: Some("int".to_string()),
            detail: Some("(variable) id: int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "car_wash".to_string(),
            kind: SymbolKind::Class,
            module: Some("CarWashRead".to_string()),
            detail: Some("(variable) BoxRead.car_wash: CarWashRead".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "model_dump".to_string(),
            kind: SymbolKind::Function,
            module: Some("BoxRead".to_string()),
            detail: Some("def BoxRead.model_dump(self) -> dict".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);
    let typed_id = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "id")
        .unwrap();
    assert_eq!(typed_id.0.module.as_deref(), Some("BoxRead"));
    let typed_car_wash = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "car_wash")
        .unwrap();
    assert_eq!(typed_car_wash.0.module.as_deref(), Some("BoxRead"));

    app.editor = editor_with("value");
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "value".to_string(),
        kind: SymbolKind::Variable,
        module: None,
        detail: None,
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);
    assert!(!app.autocomplete_active);
}

#[test]
fn ty_context_sources_drop_types_and_signatures_without_fallback() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("d");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "dir".to_string(),
            kind: SymbolKind::Function,
            module: Some("def dir(o: object = ..., /) -> list[str]".to_string()),
            detail: Some("def dir(o: object = ..., /) -> list[str]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "data".to_string(),
            kind: SymbolKind::Variable,
            module: Some("str".to_string()),
            detail: Some("(variable) data: str".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let dir = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "dir")
        .unwrap();
    assert_eq!(dir.0.module, None);
    assert_eq!(dir.0.module_path, None);

    let data = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "data")
        .unwrap();
    assert_eq!(data.0.module, None);
    assert_eq!(data.0.module_path, None);
}

#[test]
fn ty_context_top_level_sources_use_parent_module_and_hide_value_types() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("B");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "bool".to_string(),
            kind: SymbolKind::Class,
            module: Some("builtins.bool".to_string()),
            detail: Some("type[bool]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "BookingCreate".to_string(),
            kind: SymbolKind::Class,
            module: Some("car_wash.domains.washes.bookings.BookingCreate".to_string()),
            detail: Some("type[BookingCreate]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "box".to_string(),
            kind: SymbolKind::Class,
            module: Some("BoxRead".to_string()),
            detail: Some("(variable) box: BoxRead".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let bool_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "bool")
        .unwrap();
    assert_eq!(bool_item.0.kind, SymbolKind::Class);
    assert_eq!(bool_item.0.module.as_deref(), Some("builtins"));

    let booking = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "BookingCreate")
        .unwrap();
    assert_eq!(
        booking.0.module.as_deref(),
        Some("car_wash.domains.washes.bookings")
    );

    let box_value = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "box")
        .unwrap();
    assert_eq!(box_value.0.module, None);
}

#[test]
fn ty_context_top_level_uses_import_modules_and_builtin_fallbacks_when_ty_omits_source() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor =
        editor_with("from car_wash.domains.washes.bookings.service import BookingService\nBo");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "BookingService".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("type[BookingService]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "bool".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("<class 'bool'>".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let service = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "BookingService")
        .unwrap();
    assert_eq!(
        service.0.module.as_deref(),
        Some("car_wash.domains.washes.bookings.service")
    );
    assert_eq!(
        service.0.module_path.as_deref(),
        Some("car_wash.domains.washes.bookings.service")
    );

    let bool_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "bool")
        .unwrap();
    assert_eq!(bool_item.0.kind, SymbolKind::Class);
    assert_eq!(bool_item.0.module.as_deref(), Some("builtins"));
    assert_eq!(bool_item.0.module_path.as_deref(), Some("builtins.bool"));
}

#[test]
fn inherited_python_attr_owner_uses_declaring_base_class() {
    let source = "class BoxReadPublic(BasedStruct, kw_only=True):\n    id: int\n    created_at: dt.datetime\n\nclass BoxRead(BoxReadPublic, kw_only=True):\n    percentage: int | None\n    user_id: int | None = None\n";

    assert_eq!(
        python_class_attr_owner_in_source(source, "BoxRead", "id").as_deref(),
        Some("BoxReadPublic")
    );
    assert_eq!(
        python_class_attr_owner_in_source(source, "BoxRead", "created_at").as_deref(),
        Some("BoxReadPublic")
    );
    assert_eq!(
        python_class_attr_owner_in_source(source, "BoxRead", "user_id").as_deref(),
        Some("BoxRead")
    );
    let type_checking_source = "class BasedStruct(msgspec.Struct):\n    if TYPE_CHECKING:\n        _registered_properties: ClassVar[list[str]]\n\nclass BoxRead(BasedStruct, kw_only=True):\n    id: int\n";
    assert_eq!(
        python_class_attr_owner_in_source(
            type_checking_source,
            "BoxRead",
            "_registered_properties"
        )
        .as_deref(),
        Some("BasedStruct")
    );

    let imports = "from car_wash.domains.washes.boxes.output import BoxRead\n";
    assert_eq!(
        imported_python_module_for_symbol(imports, "BoxRead").as_deref(),
        Some("car_wash.domains.washes.boxes.output")
    );
    let aliased = "from car_wash.core.db.repo_base import RepoBase as BaseRepo\nimport asyncpg, car_wash.core.db.repo_base as repo_base\n";
    let imported = imported_python_symbols(aliased);
    assert_eq!(
        imported.get("BaseRepo").map(String::as_str),
        Some("car_wash.core.db.repo_base")
    );
    assert_eq!(imported.get("asyncpg").map(String::as_str), Some("asyncpg"));
    assert_eq!(
        imported.get("repo_base").map(String::as_str),
        Some("car_wash.core.db.repo_base")
    );
    assert_eq!(python_member_dot_receiver("RepoBase.", 9), Some("RepoBase"));
    assert_eq!(
        python_member_dot_receiver("RepoBase.exe", 12),
        Some("RepoBase")
    );

    let pep695_source = "class RepoBase[TModel: Base, TReadStruct: BasedStruct]:\n    model: ClassVar[type[SQLAlchemyModel | Base]]\n    read_struct: ClassVar[type[Struct]]\n    table_verbose_name: str\n\n    @typing.overload\n    async def execute_sql(\n        self,\n        sql: str,\n        args: list[SQLType],\n        *,\n        fetch_many: Literal[True],\n        modify: bool = False,\n    ) -> list[Record]: ...\n\n    @typing.overload\n    async def execute_sql(\n        self, sql: str, args: list[SQLType], *, modify: bool = False\n    ) -> str: ...\n";
    assert_eq!(
        python_class_attr_owner_in_source(pep695_source, "RepoBase", "model").as_deref(),
        Some("RepoBase")
    );
    assert_eq!(
        python_class_attr_owner_in_source(pep695_source, "RepoBase", "read_struct").as_deref(),
        Some("RepoBase")
    );
    let overload_detail =
        python_class_method_overload_detail(pep695_source, "RepoBase", "execute_sql").unwrap();
    assert!(overload_detail.contains("@overload"));
    assert!(overload_detail.contains("async def execute_sql("));
    assert!(!overload_detail.contains("Overload["));
}

#[test]
fn tree_sitter_autocomplete_shows_import_path_for_python_symbols() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "from car_wash.core.db.repo_base import RepoBase\n\nRepoBase.initialize_all()\nR",
    );
    app.highlighter.completions = vec![completion("RepoBase", SymbolKind::Variable, 0, 200)];

    app.update_autocomplete();

    assert_eq!(app.autocomplete_options[0].0.word, "RepoBase");
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Class);
    assert_eq!(
        app.autocomplete_options[0].0.module.as_deref(),
        Some("car_wash.core.db.repo_base")
    );
}

#[test]
fn tree_sitter_autocomplete_keeps_top_level_python_prefix_local_and_fast() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.is_ide_mode = true;
    app.editor = editor_with(
        "from car_wash.core.db.repo_base import RepoBase\n\nRepoBase.initialize_all()\nR",
    );
    app.highlighter.completions = vec![
        completion("RepoBase", SymbolKind::Variable, 0, 200),
        completion("RatingType", SymbolKind::Unknown, 0, 200),
    ];

    app.update_autocomplete();

    let words = app
        .autocomplete_options
        .iter()
        .map(|(item, _)| item.word.as_str())
        .collect::<Vec<_>>();
    assert_eq!(app.autocomplete_mode, AutocompleteMode::TreeSitter);
    assert_eq!(words, vec!["RepoBase", "RatingType"]);
    assert_eq!(
        app.autocomplete_options[0].0.module.as_deref(),
        Some("car_wash.core.db.repo_base")
    );
}

#[test]
fn tree_sitter_autocomplete_classifies_imported_python_symbols_before_detail() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();

    app.editor = editor_with(
        "from contextlib import asynccontextmanager\nfrom typing import cast\nfrom car_wash.config import config\na",
    );
    app.highlighter.completions = vec![completion(
        "asynccontextmanager",
        SymbolKind::Unknown,
        0,
        200,
    )];
    app.update_autocomplete();
    assert_eq!(app.autocomplete_options[0].0.word, "asynccontextmanager");
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Function);
    assert_eq!(
        app.autocomplete_options[0].0.module.as_deref(),
        Some("contextlib")
    );

    app.editor = editor_with(
        "from contextlib import asynccontextmanager\nfrom typing import cast\nfrom car_wash.config import config\nc",
    );
    app.highlighter.completions = vec![
        completion("cast", SymbolKind::Unknown, 0, 200),
        completion("config", SymbolKind::Unknown, 0, 200),
    ];
    app.update_autocomplete();

    let cast_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "cast")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(cast_item.kind, SymbolKind::Function);
    assert_eq!(cast_item.module.as_deref(), Some("typing"));

    let config_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "config")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(config_item.kind, SymbolKind::Variable);
    assert_eq!(config_item.module.as_deref(), Some("car_wash.config"));
    assert_eq!(config_item.module_path.as_deref(), Some("car_wash.config"));
}

#[test]
fn tree_sitter_autocomplete_always_shows_import_module_for_python_variables() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "from car_wash.domains.cars.controller import cars_router\n\
from car_wash.utils.openapi.custom_docs import (\n\
    docs_router,\n\
)\n\
\nR",
    );
    app.highlighter.completions = vec![
        completion("cars_router", SymbolKind::Variable, 0, 200),
        completion("docs_router", SymbolKind::Unknown, 0, 200),
    ];

    app.update_autocomplete();

    let cars_router = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "cars_router")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(cars_router.kind, SymbolKind::Variable);
    assert_eq!(
        cars_router.module.as_deref(),
        Some("car_wash.domains.cars.controller")
    );
    assert_eq!(
        cars_router.module_path.as_deref(),
        Some("car_wash.domains.cars.controller")
    );

    let docs_router = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "docs_router")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(docs_router.kind, SymbolKind::Variable);
    assert_eq!(
        docs_router.module.as_deref(),
        Some("car_wash.utils.openapi.custom_docs")
    );
    assert_eq!(
        docs_router.module_path.as_deref(),
        Some("car_wash.utils.openapi.custom_docs")
    );
}

#[test]
fn autocomplete_detail_merge_keeps_import_module_and_uses_source_declaration() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_ts_detail_import_{stamp}"));
    let package_dir = root.join("car_wash/utils/openapi");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
        package_dir.join("custom_docs.py"),
        "docs_router = Router(path='/docs', route_handlers=[rapidoc, rapidoc_js, openapi_spec, unprocessable_entity_error_handler, final_handler])\n",
    )
    .unwrap();

    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root.clone()];
    app.editor = editor_with(
        "from car_wash.utils.openapi.custom_docs import (\n\
    docs_router,\n\
)\n\
\nrou",
    );
    app.highlighter.completions = vec![completion("docs_router", SymbolKind::Unknown, 0, 200)];

    app.update_autocomplete();

    let docs_idx = app
        .autocomplete_options
        .iter()
        .position(|(item, _)| item.word == "docs_router")
        .unwrap();
    app.autocomplete_selected_idx = docs_idx;
    app.autocomplete_detail_word = Some("docs_router".to_string());
    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "docs_router".to_string(),
        kind: SymbolKind::Variable,
        module: Some("Router".to_string()),
        detail: Some("Router".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let item = &app.autocomplete_options[docs_idx].0;
    assert_eq!(
        item.module.as_deref(),
        Some("car_wash.utils.openapi.custom_docs")
    );
    assert_eq!(
        item.module_path.as_deref(),
        Some("car_wash.utils.openapi.custom_docs")
    );
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(
        popup.text.contains("\ndocs_router: Router = Router("),
        "unexpected popup text: {}",
        popup.text
    );
    assert!(popup.text.contains("..."));
    assert_ne!(popup.text, "Router");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tree_sitter_autocomplete_remaps_builtins_to_class_or_function() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();

    app.editor = editor_with("pr");
    app.highlighter.completions = vec![completion("print", SymbolKind::Builtin, 0, 200)];
    app.update_autocomplete();
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Function);

    app.editor = editor_with("bo");
    app.highlighter.completions = vec![completion("bool", SymbolKind::Builtin, 0, 200)];
    app.update_autocomplete();
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Class);

    app.editor = editor_with("Ex");
    app.highlighter.completions = vec![completion("Exception", SymbolKind::Builtin, 0, 200)];
    app.update_autocomplete();
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Class);

    app.editor = editor_with("ra");
    app.highlighter.completions = vec![completion("range", SymbolKind::Builtin, 0, 200)];
    app.update_autocomplete();
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Class);
}

#[test]
fn tree_sitter_autocomplete_shows_builtin_module_for_python_builtins() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("b");
    app.highlighter.completions = vec![completion("bool", SymbolKind::Builtin, 0, 200)];

    app.update_autocomplete();

    let bool_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "bool")
        .unwrap();
    assert_eq!(bool_item.0.kind, SymbolKind::Class);
    assert_eq!(bool_item.0.module.as_deref(), Some("builtins"));
    assert_eq!(bool_item.0.module_path.as_deref(), Some("builtins.bool"));
}

#[test]
fn tree_sitter_autocomplete_prioritizes_scoped_self() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    let text = "class Box:\n    def method(self):\n        s";
    app.editor = editor_with(text);
    app.highlighter.completions = vec![
        completion("set", SymbolKind::Builtin, 0, usize::MAX),
        completion("str", SymbolKind::Builtin, 0, usize::MAX),
        completion(
            "self",
            SymbolKind::Parameter,
            text.find("self").unwrap(),
            text.len(),
        ),
    ];

    app.update_autocomplete();

    assert_eq!(app.autocomplete_options[0].0.word, "self");
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Parameter);
}

#[test]
fn ty_context_suppresses_unscoped_self_member_completion() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.file_path = Some(PathBuf::from("/tmp/example.py"));
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.editor = editor_with("async def lifespan(_: Litestar):\n    self.");

    app.request_ty_autocomplete(AutocompleteMode::TyContext, Some("."));

    assert!(!app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());
}

#[test]
fn scoped_self_member_completion_is_allowed() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    let text = "class Box:\n    def method(self):\n        self.";
    app.editor = editor_with(text);
    app.highlighter.completions = vec![completion(
        "self",
        SymbolKind::Parameter,
        text.find("self").unwrap(),
        text.len(),
    )];

    assert!(!app.python_member_dot_receiver_is_unavailable_self());
}

#[test]
fn tree_sitter_autocomplete_keeps_exact_word_when_other_matches_exist() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("box = 1\nbox");
    app.highlighter.completions = vec![
        completion("box", SymbolKind::Variable, 0, 200),
        completion("BoxRead", SymbolKind::Class, 0, 200),
    ];

    app.update_autocomplete();

    assert!(app.autocomplete_active);
    assert!(
        app.autocomplete_options
            .iter()
            .any(|(item, _)| item.word == "box")
    );
    assert!(
        app.autocomplete_options
            .iter()
            .any(|(item, _)| item.word == "BoxRead")
    );

    app.highlighter.completions = vec![completion("box", SymbolKind::Variable, 0, 200)];
    app.update_autocomplete();

    assert!(!app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());
}

#[test]
fn tree_sitter_autocomplete_ignores_current_token_matches() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("BoxR");
    app.highlighter.completions = vec![
        completion("BoxR", SymbolKind::Variable, 0, 200),
        completion("BoxRead", SymbolKind::Class, 0, 200),
    ];

    app.update_autocomplete();

    assert!(app.autocomplete_active);
    assert!(
        app.autocomplete_options
            .iter()
            .all(|(item, _)| item.word != "BoxR")
    );
    assert_eq!(app.autocomplete_options[0].0.word, "BoxRead");

    app.editor = editor_with("Box");
    app.highlighter.completions = vec![
        completion("BoxR", SymbolKind::Variable, 0, 200),
        completion("BoxRead", SymbolKind::Class, 0, 200),
    ];

    app.update_autocomplete();

    assert!(app.autocomplete_active);
    assert!(
        app.autocomplete_options
            .iter()
            .all(|(item, _)| item.word != "BoxR")
    );
    assert_eq!(app.autocomplete_options[0].0.word, "BoxRead");
}

#[test]
fn tree_sitter_autocomplete_resets_scroll_after_filter_change() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("Rep");
    app.highlighter.completions = vec![
        completion("RepoBase", SymbolKind::Class, 0, 200),
        completion("RepoSession", SymbolKind::Class, 0, 200),
    ];

    app.update_autocomplete();
    assert!(app.autocomplete_active);
    app.autocomplete_scroll.current = 480.0;
    app.autocomplete_scroll.target = 480.0;
    app.autocomplete_selected_idx = 1;

    app.editor = editor_with("RepoB");
    app.update_autocomplete();

    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_scroll.current, 0.0);
    assert_eq!(app.autocomplete_scroll.target, 0.0);
    assert_eq!(app.autocomplete_selected_idx, 0);
    assert_eq!(app.autocomplete_options[0].0.word, "RepoBase");
}

#[test]
fn tree_sitter_autocomplete_keeps_parameter_kind_over_later_usage() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("class BookingService:\n    def __init__(self, session):\n        s");
    app.highlighter.completions = vec![
        completion("self", SymbolKind::Parameter, 35, 120),
        completion("self", SymbolKind::Class, 55, 120),
        completion("session", SymbolKind::Parameter, 41, 120),
        completion("session", SymbolKind::Class, 70, 120),
    ];

    app.update_autocomplete();

    assert_eq!(
        app.autocomplete_options
            .iter()
            .find(|(item, _)| item.word == "self")
            .map(|(item, _)| item.kind),
        Some(SymbolKind::Parameter)
    );
    assert_eq!(
        app.autocomplete_options
            .iter()
            .find(|(item, _)| item.word == "session")
            .map(|(item, _)| item.kind),
        Some(SymbolKind::Parameter)
    );
}

#[test]
fn autocomplete_shows_self_owner_for_current_class() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor =
        editor_with("class BookingService:\n    async def create_booking(self):\n        s");
    app.highlighter.completions = vec![completion("self", SymbolKind::Parameter, 50, 80)];

    app.update_autocomplete();

    let self_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "self")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(self_item.kind, SymbolKind::Parameter);
    assert_eq!(self_item.module.as_deref(), Some("BookingService"));

    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "self".to_string(),
        kind: SymbolKind::Class,
        module: None,
        detail: Some("BookingService".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let self_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "self")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(self_item.kind, SymbolKind::Parameter);
    assert_eq!(self_item.module.as_deref(), Some("BookingService"));
}

#[test]
fn ty_context_keeps_cls_parameter_in_multiline_class_header_after_late_response() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "class GenericCRUDService(\n    Generic[TReadStruct, TListResponseStruct],\n):\n    def __init_subclass__(cls, **kwargs: Any):\n        super().__init_subclass__(**kwargs)\n        c",
    );
    app.highlighter.completions = vec![completion(
        "cls",
        SymbolKind::Parameter,
        app.editor.get_full_text().find("cls").unwrap(),
        app.editor.len(),
    )];

    app.update_autocomplete();

    let cls_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "cls")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(cls_item.kind, SymbolKind::Parameter);
    assert_eq!(cls_item.module.as_deref(), Some("GenericCRUDService"));

    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "cls".to_string(),
        kind: SymbolKind::Class,
        module: Some("type[GenericCRUDService]".to_string()),
        detail: Some("type[GenericCRUDService]".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let cls_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "cls")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(cls_item.kind, SymbolKind::Parameter);
    assert_eq!(cls_item.module.as_deref(), Some("GenericCRUDService"));
}

#[test]
fn autocomplete_detail_merge_keeps_cls_parameter_from_multiline_class_header() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "class GenericCRUDService(\n    Generic[TReadStruct, TListResponseStruct],  # noqa: UP046\n):\n    def __init_subclass__(cls, **kwargs: Any):\n        super().__init_subclass__(**kwargs)\n        c",
    );
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TreeSitter;
    app.autocomplete_detail_word = Some("cls".to_string());
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "cls".to_string(),
            kind: SymbolKind::Parameter,
            scope_start: 0,
            scope_end: app.editor.len(),
            module: Some("GenericCRUDService".to_string()),
            module_path: None,
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        vec![0],
    )];

    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "cls".to_string(),
        kind: SymbolKind::Class,
        module: Some("GenericCRUDService".to_string()),
        detail: Some("type[GenericCRUDService]".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let cls_item = &app.autocomplete_options[0].0;
    assert_eq!(cls_item.kind, SymbolKind::Parameter);
    assert_eq!(cls_item.module.as_deref(), Some("GenericCRUDService"));
}

#[test]
fn autocomplete_detail_merge_does_not_downgrade_classvar_to_unknown() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "from typing import ClassVar\nclass GenericCRUDService:\n    list_resp_struct: ClassVar[type[GenericListResponse]]\n    Cla",
    );
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TreeSitter;
    app.autocomplete_detail_word = Some("ClassVar".to_string());
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "ClassVar".to_string(),
            kind: SymbolKind::Class,
            scope_start: 0,
            scope_end: app.editor.len(),
            module: Some("typing".to_string()),
            module_path: Some("typing.ClassVar".to_string()),
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        vec![0, 1, 2],
    )];

    app.merge_autocomplete_details(vec![crate::lsp::LspCompletionItem {
        label: "ClassVar".to_string(),
        kind: SymbolKind::Unknown,
        module: Some("typing".to_string()),
        detail: None,
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let item = &app.autocomplete_options[0].0;
    assert_eq!(item.kind, SymbolKind::Class);
    assert_eq!(item.module.as_deref(), Some("typing"));
}

#[test]
fn ty_context_initial_typing_cast_is_function_before_detail_hover() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("from typing import cast\nca");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "cast".to_string(),
        kind: SymbolKind::Unknown,
        module: Some("typing".to_string()),
        detail: Some(
            "Overload[[_T](typ: type[_T], val: Any) -> _T, (typ: str, val: Any) -> Any]"
                .to_string(),
        ),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let item = &app.autocomplete_options[0].0;
    assert_eq!(item.word, "cast");
    assert_eq!(item.kind, SymbolKind::Function);
    assert_eq!(item.module.as_deref(), Some("typing"));
}

#[test]
fn ty_context_initial_overload_completion_is_function_before_detail_hover() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("from contextlib import asynccontextmanager\nasy");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "asynccontextmanager".to_string(),
        kind: SymbolKind::Variable,
        module: Some("contextlib".to_string()),
        detail: Some("Overload[[**_P, _T_co](func: (**_P) -> AsyncIterator[_T_co])]".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let item = &app.autocomplete_options[0].0;
    assert_eq!(item.word, "asynccontextmanager");
    assert_eq!(item.kind, SymbolKind::Function);
    assert_eq!(item.module.as_deref(), Some("contextlib"));
}

#[test]
fn ty_context_initial_keywords_and_imported_config_have_stable_kinds() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("from car_wash.config import config\nc");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "case".to_string(),
            kind: SymbolKind::Unknown,
            module: None,
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "config".to_string(),
            kind: SymbolKind::Unknown,
            module: Some("Config".to_string()),
            detail: Some("Config".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let case_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "case")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(case_item.kind, SymbolKind::Keyword);

    let config_item = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "config")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(config_item.kind, SymbolKind::Variable);
    assert_eq!(config_item.module.as_deref(), Some("car_wash.config"));
    assert_eq!(config_item.module_path.as_deref(), Some("car_wash.config"));

    let config_idx = app
        .autocomplete_options
        .iter()
        .position(|(item, _)| item.word == "config")
        .unwrap();
    app.autocomplete_selected_idx = config_idx;
    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(
        popup
            .text
            .starts_with("[[MODULE]] car_wash.config\n(variable) config: Config")
    );
}

#[test]
fn ty_context_imported_router_variable_shows_import_module_and_variable_detail() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("from car_wash.domains.cars.controller import cars_router\ncar");
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "cars_router".to_string(),
        kind: SymbolKind::Variable,
        module: Some("Router".to_string()),
        detail: Some("Router".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let item = &app.autocomplete_options[0].0;
    assert_eq!(item.word, "cars_router");
    assert_eq!(item.kind, SymbolKind::Variable);
    assert_eq!(
        item.module.as_deref(),
        Some("car_wash.domains.cars.controller")
    );
    assert_eq!(
        item.module_path.as_deref(),
        Some("car_wash.domains.cars.controller")
    );

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.starts_with(
        "[[MODULE]] car_wash.domains.cars.controller\n(variable) cars_router: Router"
    ));
}

#[test]
fn autocomplete_detail_uses_module_path_when_module_is_only_type_label() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with("from car_wash.domains.cars.controller import cars_router\ncar");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "cars_router".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: app.editor.len(),
            module: Some("Router".to_string()),
            module_path: Some("car_wash.domains.cars.controller".to_string()),
            detail: Some("Router".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        vec![0, 1],
    )];

    app.refresh_autocomplete_detail_popup();

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(
        popup.text,
        "[[MODULE]] car_wash.domains.cars.controller\n(variable) cars_router: Router"
    );
}

#[test]
fn autocomplete_detail_variable_uses_source_declaration_with_middle_ellipsis() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_autocomplete_source_detail_{stamp}"));
    let package_dir = root.join("car_wash/domains/cars");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
        package_dir.join("controller.py"),
        "cars_router = Router(path='/cars', route_handlers=[rapidoc, rapidoc_js, openapi_spec, unprocessable_entity_error_handler, final_handler])\n",
    )
    .unwrap();

    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root.clone()];
    app.editor = editor_with("from car_wash.domains.cars.controller import cars_router\ncar");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "cars_router".to_string(),
            kind: SymbolKind::Variable,
            scope_start: 0,
            scope_end: app.editor.len(),
            module: Some("Router".to_string()),
            module_path: Some("car_wash.domains.cars.controller".to_string()),
            detail: Some("Router".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        vec![0, 1],
    )];

    app.refresh_autocomplete_detail_popup();

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(
        popup
            .text
            .starts_with("[[MODULE]] car_wash.domains.cars\nVariable cars_router of controller\n"),
        "unexpected popup text: {}",
        popup.text
    );
    assert!(popup.text.contains("\ncars_router: Router = Router("));
    assert!(popup.text.contains("..."));
    assert!(popup.text.contains("final_handler])"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ty_context_pasted_litestar_app_imports_keep_router_sources() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "from contextlib import asynccontextmanager\n\
from typing import Any, AsyncGenerator\n\
\n\
from litestar import Litestar, get\n\
from car_wash.domains.auth.controller import AuthController\n\
from car_wash.domains.cars.controller import cars_router\n\
from car_wash.domains.users.controller import users_router\n\
from car_wash.domains.washes.controller import car_washes_router\n\
\n\
setup_logging()\n\
\n\
@asynccontextmanager\n\
async def lifespan(_: Litestar, arg: str) -> AsyncGenerator[None, Any]:\n\
    RepoBase.initialize_all()\n\
    ca",
    );
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "cars_router".to_string(),
            kind: SymbolKind::Variable,
            module: Some("Router".to_string()),
            detail: Some("Router".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "car_washes_router".to_string(),
            kind: SymbolKind::Variable,
            module: Some("Router".to_string()),
            detail: Some("Router".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "case".to_string(),
            kind: SymbolKind::Unknown,
            module: None,
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let cars = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "cars_router")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(cars.kind, SymbolKind::Variable);
    assert_eq!(
        cars.module.as_deref(),
        Some("car_wash.domains.cars.controller")
    );
    assert_eq!(
        cars.module_path.as_deref(),
        Some("car_wash.domains.cars.controller")
    );
    assert_eq!(cars.detail.as_deref(), Some("Router"));

    let washes = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "car_washes_router")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(
        washes.module.as_deref(),
        Some("car_wash.domains.washes.controller")
    );
    assert_eq!(
        washes.module_path.as_deref(),
        Some("car_wash.domains.washes.controller")
    );
}

#[test]
fn ty_context_cls_member_completion_labels_multiline_class_owner() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "class GenericCRUDService(\n    Generic[TReadStruct, TListResponseStruct],\n):\n    repository: type[AsyncpgRepository]\n\n    def __init_subclass__(cls, **kwargs: Any):\n        cls.",
    );
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "read_entity".to_string(),
            kind: SymbolKind::Function,
            module: None,
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "repository".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("type[AsyncpgRepository]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    for word in ["read_entity", "repository"] {
        let item = app
            .autocomplete_options
            .iter()
            .find(|(item, _)| item.word == word)
            .map(|(item, _)| item)
            .unwrap();
        assert_eq!(item.module.as_deref(), Some("GenericCRUDService"));
    }
}

#[test]
fn ty_context_self_member_completion_uses_booking_service_instance_attrs() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_self_owner_repo_test_{stamp}"));
    let repo_dir = root.join("car_wash/domains/cars");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("repository.py"),
        "class UserCarRepository:\n    async def paginate_entities(self):\n        pass\n    tasks: list | None\n    add_serv: object\n    ban_repo: object\n    box_repo: object\n    user_repo: object\n    crud_repo: object\n",
    )
    .unwrap();
    let current = root.join("car_wash/domains/washes/bookings/service.py");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();

    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(current);
    app.ide_workspaces = vec![root];
    app.file_extension = "py".to_string();
    app.editor = editor_with(
        "from car_wash.core.service import GenericCRUDService\nfrom car_wash.domains.cars.repository import UserCarRepository\nfrom car_wash.domains.washes.bans.repository import CarWashBanRepository\nfrom car_wash.domains.washes.bookings.repository import BookingRepository\n\nclass BookingService(GenericCRUDService[BookingRead, ListResponse]):\n    repository = BookingRepository\n\n    def __init__(self, session):\n        super().__init__(session)\n        self.ban_repo = CarWashBanRepository(session)\n        self.user_car_repo = UserCarRepository(session)\n        self.tasks: list | None = None\n        self.bank_commissions = {}\n\n    async def create_booking(self):\n        self.",
    );
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "paginate_entities".to_string(),
            kind: SymbolKind::Function,
            module: Some("UserCarRepository".to_string()),
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "tasks".to_string(),
            kind: SymbolKind::Class,
            module: Some("UserCarRepository".to_string()),
            detail: Some("list | None".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "ban_repo".to_string(),
            kind: SymbolKind::Class,
            module: Some("CarWashBanRepository".to_string()),
            detail: Some("CarWashBanRepository".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "repository".to_string(),
            kind: SymbolKind::Class,
            module: Some("type[BookingRepository]".to_string()),
            detail: Some(
                "<class 'BookingRepository'> | type[AsyncpgRepository[Unknown, Unknown]]"
                    .to_string(),
            ),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "user_car_repo".to_string(),
            kind: SymbolKind::Class,
            module: Some("UserCarRepository".to_string()),
            detail: Some("UserCarRepository".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "bank_commissions".to_string(),
            kind: SymbolKind::Class,
            module: Some("dict".to_string()),
            detail: Some("dict".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    for word in [
        "paginate_entities",
        "tasks",
        "ban_repo",
        "repository",
        "user_car_repo",
        "bank_commissions",
    ] {
        let item = app
            .autocomplete_options
            .iter()
            .find(|(item, _)| item.word == word)
            .map(|(item, _)| item)
            .unwrap();
        assert_eq!(item.module.as_deref(), Some("BookingService"));
    }
    for word in [
        "tasks",
        "ban_repo",
        "repository",
        "user_car_repo",
        "bank_commissions",
    ] {
        let item = app
            .autocomplete_options
            .iter()
            .find(|(item, _)| item.word == word)
            .map(|(item, _)| item)
            .unwrap();
        assert_eq!(item.kind, SymbolKind::Variable);
    }
    let repository = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "repository")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(
        repository.module_path.as_deref(),
        Some("car_wash.domains.washes.bookings.repository.BookingRepository")
    );
    let ban_repo_idx = app
        .autocomplete_options
        .iter()
        .position(|(item, _)| item.word == "ban_repo")
        .unwrap();
    let ban_repo = &app.autocomplete_options[ban_repo_idx].0;
    assert_eq!(
        ban_repo.module_path.as_deref(),
        Some("car_wash.domains.washes.bans.repository.CarWashBanRepository")
    );
    app.autocomplete_selected_idx = ban_repo_idx;
    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.starts_with(
        "[[MODULE]] car_wash.domains.washes.bans.repository\nclass CarWashBanRepository"
    ));
    assert!(!popup.spans.is_empty());
}

#[test]
fn ty_context_completion_uses_declaring_owner_not_field_type() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_owner_test_{stamp}"));
    let package_dir = root.join("car_wash/domains/washes/boxes");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
            package_dir.join("output.py"),
            "from car_wash.utils.schemas.struct import BasedStruct\n\nclass BoxReadPublic(BasedStruct, kw_only=True):\n    id: int\n    name: str\n    car_wash_id: int\n    car_wash: CarWashRead\n    created_at: dt.datetime\n\nclass BoxRead(BoxReadPublic, kw_only=True):\n    percentage: int | None\n    user_id: int | None = None\n    employee: UserRead | None = None\n    car_wash: CarWashRead\n",
        )
        .unwrap();
    let struct_dir = root.join("car_wash/utils/schemas");
    std::fs::create_dir_all(&struct_dir).unwrap();
    std::fs::write(
            struct_dir.join("struct.py"),
            "from typing import TYPE_CHECKING, ClassVar\n\nclass BasedStruct(msgspec.Struct):\n    if TYPE_CHECKING:\n        _registered_field_validators: ClassVar[dict[str, list[Callable]]]\n        _registered_model_validators: ClassVar[list[Callable]]\n        _registered_properties: ClassVar[list[str]]\n",
        )
        .unwrap();
    let current = root.join("car_wash/domains/washes/bookings/service.py");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();

    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(current);
    app.file_extension = "py".to_string();
    app.editor = editor_with("from car_wash.domains.washes.boxes.output import BoxRead\nbox.");
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "active".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("bool".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "model_dump".to_string(),
            kind: SymbolKind::Function,
            module: Some("BoxRead".to_string()),
            detail: Some("def BoxRead.model_dump(self) -> dict".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "id".to_string(),
            kind: SymbolKind::Variable,
            module: Some("int".to_string()),
            detail: Some("(variable) id: int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "created_at".to_string(),
            kind: SymbolKind::Class,
            module: Some("datetime".to_string()),
            detail: Some("datetime".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "_registered_properties".to_string(),
            kind: SymbolKind::Class,
            module: Some("BoxRead".to_string()),
            detail: Some("(variable) _registered_properties: list[str]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "car_wash".to_string(),
            kind: SymbolKind::Class,
            module: Some("CarWashRead".to_string()),
            detail: Some("CarWashRead".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let id_owner = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "id")
        .and_then(|(item, _)| item.module.as_deref());
    let created_at_owner = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "created_at")
        .and_then(|(item, _)| item.module.as_deref());
    let car_wash_owner = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "car_wash")
        .and_then(|(item, _)| item.module.as_deref());
    let registered_owner = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "_registered_properties")
        .and_then(|(item, _)| item.module.as_deref());
    let id_module_path = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "id")
        .and_then(|(item, _)| item.module_path.as_deref());
    assert_eq!(id_owner, Some("BoxReadPublic"));
    assert_eq!(created_at_owner, Some("BoxReadPublic"));
    assert_eq!(car_wash_owner, Some("BoxRead"));
    assert_eq!(registered_owner, Some("BasedStruct"));
    assert_eq!(
        id_module_path,
        Some("car_wash.domains.washes.boxes.output.BoxReadPublic")
    );
}

#[test]
fn ty_context_completion_formats_source_overloads_and_pep695_attr_owners() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_repo_base_detail_test_{stamp}"));
    let package_dir = root.join("car_wash/core/db");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
        package_dir.join("repo_base.py"),
        "class RepoBase[TModel: Base, TReadStruct: BasedStruct]:\n    model: ClassVar[type[SQLAlchemyModel | Base]]\n    read_struct: ClassVar[type[Struct]]\n    table_verbose_name: str\n\n    @typing.overload\n    async def execute_sql(\n        self,\n        sql: str,\n        args: list[SQLType],\n        *,\n        fetch_many: Literal[True],\n        modify: bool = False,\n    ) -> list[Record]: ...\n\n    @typing.overload\n    async def execute_sql(\n        self, sql: str, args: list[SQLType], *, modify: bool = False\n    ) -> str: ...\n",
    )
    .unwrap();
    let current = root.join("car_wash/app.py");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();

    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(current);
    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root];
    app.editor = editor_with("from car_wash.core.db.repo_base import RepoBase\nRepoBase.");
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "execute_sql".to_string(),
            kind: SymbolKind::Function,
            module: None,
            detail: Some(
                "Overload[(self, sql: str, args: list[SQLType]) -> CoroutineType[Any, Any, str]]"
                    .to_string(),
            ),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "model".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("type[SQLAlchemyModel | Base]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "read_struct".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("type[Struct]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "table_verbose_name".to_string(),
            kind: SymbolKind::Class,
            module: None,
            detail: Some("str".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    for name in ["model", "read_struct", "table_verbose_name"] {
        let owner = app
            .autocomplete_options
            .iter()
            .find(|(item, _)| item.word == name)
            .and_then(|(item, _)| item.module.as_deref());
        assert_eq!(owner, Some("RepoBase"));
    }
    let execute_detail = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "execute_sql")
        .and_then(|(item, _)| item.detail.as_deref())
        .unwrap();
    assert!(execute_detail.contains("@overload"));
    assert!(execute_detail.contains("async def execute_sql("));
    assert!(!execute_detail.contains("Overload["));
}

#[test]
fn ty_context_member_completion_uses_inherited_method_owner() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_inherited_method_owner_test_{stamp}"));
    let package_dir = root.join("car_wash/core/db");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
        package_dir.join("crud.py"),
        "class GenericCRUDService:\n    async def read_entity(self):\n        pass\n    async def create_entity(self):\n        pass\n",
    )
    .unwrap();
    let current = root.join("car_wash/bookings/service.py");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();

    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(current);
    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root];
    app.editor = editor_with(
        "from car_wash.core.db.crud import GenericCRUDService\nclass BookingService(GenericCRUDService):\n    async def read_booking(self):\n        pass\n    async def create_booking(self):\n        self.rea",
    );
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "read_entity".to_string(),
            kind: SymbolKind::Function,
            module: Some("BookingService".to_string()),
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "read_booking".to_string(),
            kind: SymbolKind::Function,
            module: Some("BookingService".to_string()),
            detail: None,
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let read_entity_owner = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "read_entity")
        .and_then(|(item, _)| item.module.as_deref());
    let read_booking_owner = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "read_booking")
        .and_then(|(item, _)| item.module.as_deref());
    assert_eq!(read_entity_owner, Some("GenericCRUDService"));
    assert_eq!(read_booking_owner, Some("BookingService"));
}

#[test]
fn ty_context_member_completion_orders_owner_depth_and_private_names() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_owner_rank_test_{stamp}"));
    let package_dir = root.join("car_wash/domains/washes/boxes");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
        package_dir.join("output.py"),
        "class GrandBase:\n    grand_public: int\n\nclass BoxReadPublic(GrandBase):\n    base_public: int\n    _base_hidden: int\n\nclass BoxRead(BoxReadPublic):\n    current_public: int\n    _current_hidden: int\n",
    )
    .unwrap();
    let current = root.join("car_wash/domains/washes/bookings/service.py");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();

    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(current);
    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root];
    app.editor = editor_with("from car_wash.domains.washes.boxes.output import BoxRead\nbox.");
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![
        crate::lsp::LspCompletionItem {
            label: "base_public".to_string(),
            kind: SymbolKind::Class,
            module: Some("int".to_string()),
            detail: Some("int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "_base_hidden".to_string(),
            kind: SymbolKind::Class,
            module: Some("int".to_string()),
            detail: Some("int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "grand_public".to_string(),
            kind: SymbolKind::Class,
            module: Some("int".to_string()),
            detail: Some("int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "current_public".to_string(),
            kind: SymbolKind::Class,
            module: Some("int".to_string()),
            detail: Some("int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "_current_hidden".to_string(),
            kind: SymbolKind::Class,
            module: Some("int".to_string()),
            detail: Some("int".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "model_dump".to_string(),
            kind: SymbolKind::Function,
            module: Some("BoxRead".to_string()),
            detail: Some("def BoxRead.model_dump(self) -> dict".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "base_method".to_string(),
            kind: SymbolKind::Function,
            module: Some("BoxReadPublic".to_string()),
            detail: Some("def BoxReadPublic.base_method(self) -> dict".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        crate::lsp::LspCompletionItem {
            label: "mro".to_string(),
            kind: SymbolKind::Function,
            module: Some("BoxRead".to_string()),
            detail: Some("def BoxRead.mro(self) -> list[type]".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
    ]);

    let words = app
        .autocomplete_options
        .iter()
        .map(|(item, _)| item.word.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        words,
        vec![
            "current_public",
            "model_dump",
            "_current_hidden",
            "base_public",
            "base_method",
            "_base_hidden",
            "grand_public",
            "mro",
        ]
    );
}

#[path = "app_file_behavior_tests.rs"]
mod app_file_behavior_tests;
