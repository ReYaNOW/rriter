use super::*;
use crate::platform::Clipboard;
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
        file_key: path.map(PathBuf::from).as_deref().map(crate::platform::PathKey::new),
        text_file_format: crate::platform::TextFileFormat {
            encoding: crate::platform::TextEncoding::Utf8,
            line_ending: crate::platform::LineEnding::Lf,
        },
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
        is_highlight_complete: false,
        icon_key: "default_file",
        syntax_errors: Vec::new(),
        kind: EditorTabKind::Normal,
    }
}

fn test_app() -> Option<App> {
    let now = Instant::now();
    Some(App {
        automation: None,
        scroll_render_bench: None,
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
        file_key: None,
        text_file_format: crate::platform::TextFileFormat {
            encoding: crate::platform::TextEncoding::Utf8,
            line_ending: crate::platform::LineEnding::Lf,
        },
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
        is_editor_drag_pending: false,
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
        api_import_file_rx: None,
        api_body_file_rx: None,
        api_load_rx: Vec::new(),
        api_request_rx: Vec::new(),
        api_mock_ty_rx: None,
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
        tool_paths: crate::platform::ToolPaths::default(),
        settings_tool_picker_rx: None,
        tool_installer: crate::app::tool_installer::ToolInstaller::default(),
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
        is_highlight_complete: false,
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
        autocomplete_signature_request_id: None,
        autocomplete_signature_items: Vec::new(),
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
        file_tree_watcher_stop_tx: None,
        file_tree_watched_dirs: Vec::new(),
        external_changes_rx: None,
        git_diff_rx: Vec::new(),
        inline_git_diff_rx: None,
        inline_git_popup: None,
        readonly_notice_until: None,
        lsp: None,
        lsp_actions_menu: None,
        pending_fix_all_id: None,
        ctrl_definition: CtrlDefinitionState::default(),
        python_inlay_hints: Vec::new(),
        python_inlay_hint_path: None,
        python_inlay_hint_range: None,
        python_inlay_hint_version: 0,
        python_inlay_hint_pending_request_id: None,
        python_inlay_hint_pending_path: None,
        python_inlay_hint_pending_range: None,
        python_inlay_hint_pending_version: 0,
        python_inlay_hint_cache: rustc_hash::FxHashMap::default(),
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
fn ty_import_completion_inserts_word_and_appends_import_without_cursor_jump() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("import os\nimport sys\n\nPa");
    app.file_path = Some(PathBuf::from("main.py"));
    app.file_extension = "py".to_string();
    app.autocomplete_mode = AutocompleteMode::TyImports;

    let item = AutocompleteItem {
        word: "Path".to_string(),
        kind: SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("pathlib".to_string()),
        module_path: Some("pathlib".to_string()),
        detail: None,
        insert_text: Some("Path".to_string()),
        text_edit: None,
        additional_text_edits: vec![crate::lsp::TextChange {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            new_text: "from pathlib import Path\n".to_string(),
        }],
    };

    app.apply_lsp_completion_item(&item);

    assert_eq!(
        app.editor.get_full_text(),
        "import os\nimport sys\nfrom pathlib import Path\n\nPath"
    );
    assert_eq!(app.editor.cursor, app.editor.len());
}

#[test]
fn ty_import_completion_with_text_edit_appends_after_multiline_import_region() {
    let Some(mut app) = test_app() else {
        return;
    };
    let text = concat!(
        "import datetime as dt\n",
        "from decimal import Decimal\n",
        "\n",
        "from car_wash.core.service import (\n",
        "    GenericCRUDService,\n",
        ")\n",
        "from car_wash.utils.schemas.types import (\n",
        "    StateEnum,\n",
        ")\n",
        "\n",
        "class BookingService:\n",
        "    Repo",
    );
    app.editor = editor_with(text);
    app.file_path = Some(PathBuf::from("main.py"));
    app.file_extension = "py".to_string();
    app.autocomplete_mode = AutocompleteMode::TyImports;

    let item = AutocompleteItem {
        word: "RepoBase".to_string(),
        kind: SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("car_wash.core.db.repo_base".to_string()),
        module_path: Some("car_wash.core.db.repo_base".to_string()),
        detail: None,
        insert_text: None,
        text_edit: Some(crate::lsp::TextChange {
            start_line: 11,
            start_col: 4,
            end_line: 11,
            end_col: 8,
            new_text: "RepoBase".to_string(),
        }),
        additional_text_edits: vec![crate::lsp::TextChange {
            start_line: 3,
            start_col: 0,
            end_line: 3,
            end_col: 0,
            new_text: "from car_wash.core.db.repo_base import RepoBase\n".to_string(),
        }],
    };

    app.apply_lsp_completion_item(&item);

    assert_eq!(
        app.editor.get_full_text(),
        concat!(
            "import datetime as dt\n",
            "from decimal import Decimal\n",
            "\n",
            "from car_wash.core.service import (\n",
            "    GenericCRUDService,\n",
            ")\n",
            "from car_wash.utils.schemas.types import (\n",
            "    StateEnum,\n",
            ")\n",
            "from car_wash.core.db.repo_base import RepoBase\n",
            "\n",
            "class BookingService:\n",
            "    RepoBase",
        )
    );
    assert_eq!(app.editor.cursor, app.editor.len());
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

    app.request_active_autocomplete_detail_for_index(0);

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
fn ty_context_local_asynccontextmanager_completion_uses_source_signature_and_module() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_lifespan_detail_test_{stamp}"));
    let main_path = root.join("car_wash/main.py");
    std::fs::create_dir_all(main_path.parent().unwrap()).unwrap();

    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(main_path);
    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root];
    app.editor = editor_with(
        "from contextlib import asynccontextmanager\nfrom typing import Any, AsyncGenerator\nfrom litestar import Litestar\n\n@asynccontextmanager\nasync def lifespan(_: Litestar) -> AsyncGenerator[None, Any]:\n    yield\n\nlifespa",
    );
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
        label: "lifespan".to_string(),
        kind: SymbolKind::Function,
        module: None,
        detail: Some("(_: Litestar) -> _AsyncGeneratorContextManager[None, None]".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }]);

    let item = &app.autocomplete_options[0].0;
    assert_eq!(item.module.as_deref(), Some("car_wash.main"));
    assert_eq!(item.module_path.as_deref(), Some("car_wash.main"));

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(
        popup.text,
        "[[MODULE]] car_wash.main\n@asynccontextmanager\nasync def lifespan(_: Litestar) -> AsyncGenerator[None, Any]"
    );
    assert!(!popup.text.contains("_AsyncGeneratorContextManager"));
}

#[test]
fn autocomplete_detail_popup_expands_class_repr_from_source() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_repo_base_repr_detail_test_{stamp}"));
    let package_dir = root.join("car_wash/core/db");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
        package_dir.join("repo_base.py"),
        "class RepoBase[TModel: Base, TReadStruct: BasedStruct]:\n    \"\"\"Repo docs.\"\"\"\n",
    )
    .unwrap();

    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root];
    app.editor = editor_with("Rep");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "RepoBase".to_string(),
            kind: SymbolKind::Class,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("car_wash.core.db.repo_base".to_string()),
            module_path: Some("car_wash.core.db.repo_base".to_string()),
            detail: Some("<class 'RepoBase'>".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(
        popup.text,
        "[[MODULE]] car_wash.core.db.repo_base\nclass RepoBase[TModel: Base, TReadStruct: BasedStruct]\n---\nRepo docs."
    );
}

#[test]
fn autocomplete_detail_popup_follows_reexported_class_to_definition_module() {
    let Some(mut app) = test_app() else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_litestar_reexport_detail_test_{stamp}"));
    let package_dir = root.join("litestar");
    std::fs::create_dir_all(&package_dir).unwrap();
    std::fs::write(
        package_dir.join("__init__.py"),
        "from .app import Litestar\n",
    )
    .unwrap();
    std::fs::write(
        package_dir.join("app.py"),
        "class Router:\n    pass\n\nclass Litestar(Router):\n    \"\"\"The Litestar application.\n\n    Root level docs.\n    \"\"\"\n",
    )
    .unwrap();

    app.file_extension = "py".to_string();
    app.ide_workspaces = vec![root];
    app.editor = editor_with("Litesta");
    app.autocomplete_active = true;
    app.autocomplete_mode = AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        AutocompleteItem {
            word: "Litestar".to_string(),
            kind: SymbolKind::Class,
            scope_start: 0,
            scope_end: usize::MAX,
            module: Some("litestar".to_string()),
            module_path: Some("litestar".to_string()),
            detail: Some("class Litestar".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(
        popup.text,
        "[[MODULE]] litestar.app\nclass Litestar(Router)\n---\nThe Litestar application.\n\nRoot level docs."
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

    app.autocomplete_options[0].0 = AutocompleteItem {
        word: "max".to_string(),
        kind: SymbolKind::Function,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("builtins".to_string()),
        module_path: Some("builtins.max".to_string()),
        detail: Some(
            "Overload[[SupportsRichComparisonT](arg1: SupportsRichComparisonT, arg2: SupportsRichComparisonT, /, *_args: SupportsRichComparisonT, *, key: None = None) -> SupportsRichComparisonT, [_T](arg1: _T, arg2: _T, /, *_args: _T, *, key: (_T, /) -> SupportsDunderLT[Any] | SupportsDunderGT[Any]) -> _T, [SupportsRichComparisonT](iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None) -> SupportsRichComparisonT, [_T](iterable: Iterable[_T], /, *, key: (_T, /) -> SupportsDunderLT[Any] | SupportsDunderGT[Any]) -> _T, [SupportsRichComparisonT, _T](iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None, default: _T) -> SupportsRichComparisonT | _T, [_T1, _T2](iterable: Iterable[_T1], /, *, key: (_T1, /) -> SupportsDunderLT[Any] | SupportsDunderGT[Any], default: _T2) -> _T1 | _T2]\n---\nmax(iterable, *[, default=obj, key=func]) -> value\nmax(arg1, arg2, *args, *[, key=func]) -> value\n\nWith a single iterable argument, return its biggest item."
                .to_string(),
        ),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    };

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(
        popup
            .text
            .contains("def max(*args: Any, key: Any = None, default: Any = ...) -> Any")
    );
    assert!(
        popup
            .text
            .contains("max(iterable, *[, default=obj, key=func])")
    );
    assert!(popup.text.contains("return its biggest item"));
    assert_eq!(popup.text.matches("def max").count(), 1);
    assert!(!popup.text.contains("Overload["));

    app.autocomplete_options[0].0.detail = Some(
        "Overload[[SupportsRichComparisonT](arg1: SupportsRichComparisonT, arg2: SupportsRichComparisonT, /, *_args: SupportsRichComparisonT, *, key: None = None) -> SupportsRichComparisonT, [_T](arg1: _T, arg2: _T, /, *_args: _T, *, key: (_T, /) -> SupportsDunderLT[Any] | SupportsDunderGT[Any]) -> _T, [SupportsRichComparisonT](iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None) -> SupportsRichComparisonT, [_T](iterable: Iterable[_T], /, *, key: (_T, /) -> SupportsDunderLT[Any] | SupportsDunderGT[Any]) -> _T]"
            .to_string(),
    );
    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(
        popup.text,
        "[[MODULE]] builtins\ndef max(*args: Any, key: Any = None, default: Any = ...) -> Any"
    );

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rriter_builtin_map_detail_test_{stamp}"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("builtins.pyi"),
        "class map(Generic[_S]):\n    pass\n",
    )
    .unwrap();
    app.ide_workspaces = vec![root];

    app.autocomplete_options[0].0 = AutocompleteItem {
        word: "map".to_string(),
        kind: SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("builtins".to_string()),
        module_path: Some("builtins.map".to_string()),
        detail: Some("class map".to_string()),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    };

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.contains("[[MODULE]] builtins"));
    assert!(popup.text.contains("class map(Generic[_S])"));
    assert!(
        popup
            .text
            .contains("Stops when the shortest iterable is exhausted")
    );

    app.autocomplete_options[0].0 = AutocompleteItem {
        word: "map".to_string(),
        kind: SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
        module: Some("builtins".to_string()),
        module_path: Some("builtins.map".to_string()),
        detail: Some(
            "class map\n---\nMake an iterator that computes the function using arguments from\neach of the iterables."
                .to_string(),
        ),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    };

    app.refresh_autocomplete_detail_popup();
    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert!(popup.text.contains("[[MODULE]] builtins"));
    assert_eq!(popup.text.matches("[[MODULE]] builtins").count(), 1);
    assert!(popup.text.contains("class map(Generic[_S])"));
    assert!(popup.text.contains("Make an iterator"));
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
