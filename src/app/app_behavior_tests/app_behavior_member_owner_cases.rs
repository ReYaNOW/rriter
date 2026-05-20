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

#[path = "../app_file_behavior_tests.rs"]
mod app_file_behavior_tests;
