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
