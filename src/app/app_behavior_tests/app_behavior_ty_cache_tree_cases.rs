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
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Argument);
}

#[test]
fn ty_context_keeps_empty_call_argument_completions() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with("repo.find_one(");
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
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Argument);
    assert!(app.autocomplete_active);
}

#[test]
fn ty_signature_help_arguments_feed_call_completion() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.editor = editor_with(
        "asyncpg.create_pool(config.database_url, max_size=20, command_timeout=66, ma",
    );
    app.autocomplete_mode = AutocompleteMode::TyContext;

    app.update_ty_signature_help_autocomplete(vec![
        "dsn".to_string(),
        "max_size".to_string(),
        "command_timeout".to_string(),
        "max_queries".to_string(),
        "max_inactive_connection_lifetime".to_string(),
    ]);

    assert_eq!(app.autocomplete_options[0].0.word, "max_queries");
    assert_eq!(app.autocomplete_options[0].0.kind, SymbolKind::Argument);
    assert_eq!(
        app.autocomplete_options[0].0.insert_text.as_deref(),
        Some("max_queries=")
    );
    assert!(
        !app.autocomplete_options
            .iter()
            .any(|(item, _)| item.word == "max_size")
    );
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
