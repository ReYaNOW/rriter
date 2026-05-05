
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
        autocomplete_detail_request_id: None,
        autocomplete_detail_word: None,
        autocomplete_detail_request_path: None,
        autocomplete_detail_context_key: None,
        autocomplete_detail_popup: None,
        autocomplete_detail_rect: None,
        autocomplete_detail_placement: None,
        autocomplete_detail_max_scroll: 0.0,
        autocomplete_min_width: 0.0,
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
            .starts_with("[[MODULE]] car_wash.core.db.repo_base.RepoBase\n")
    );
    assert_eq!(
        popup.line_kinds.first().copied(),
        Some(crate::lsp::HoverLineKindPublic::Text)
    );
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

#[path = "app_file_behavior_tests.rs"]
mod app_file_behavior_tests;
