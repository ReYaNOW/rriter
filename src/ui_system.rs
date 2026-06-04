/// Декларативная система UI для автоматической обработки кнопок и виджетов
/// Устраняет необходимость вручную прописывать координаты и биндинги для каждой кнопки
use crate::renderer::Renderer;
use crate::widgets::{Button, IconButton};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiMockContractFieldGroup {
    Path,
    Query,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiMockContractFieldProp {
    Required,
    Nullable,
    Default,
    Enum,
    MinLength,
    MaxLength,
    Pattern,
    Minimum,
    Maximum,
    MinItems,
    MaxItems,
}

/// Уникальный идентификатор UI элемента
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiId {
    // Welcome screen
    WelcomeNewFile,
    WelcomeOpenFile,
    WelcomeIdeMode,
    WelcomeRecentFile(usize),

    // Dialog
    DialogSave,
    DialogDiscard,
    DialogCancel,

    // Settings tabs
    SettingsTab(usize),

    // Settings IDE
    SettingsIdeAddWorkspace,
    SettingsIdeRemoveWorkspace(usize),
    SettingsIdeAddIgnore,
    SettingsIdeRemoveIgnore(usize),
    SettingsIdeIgnoreInput,

    // LSP panel
    LspServerRestart(usize),
    LspServerToggle(usize),
    LspServerStop(usize),
    LspServerLogs(usize),
    LspServerClearLogs(usize),
    LspServerFixAll(usize),
    LspLogFoldToggle(usize, usize), // (server_idx, line_idx)
    LspLogsFilterInput,
    LspLogsFilterClear,
    LspLogsFilterCase,
    LspLogsFilterSend,
    LspLogsFilterRecv,
    LspLogsFilterOther,

    // API client
    ApiImportAdd,
    ApiImportFile,
    ApiImportUrl,
    ApiImportUrlInput,
    ApiImportUrlConfirm,
    ApiSpecSelect(usize),
    ApiSpecOpen(usize),
    ApiSpecRefresh(usize),
    ApiSpecRemove(usize),
    ApiSpecRemoveConfirm,
    ApiSpecRemoveCancel,
    ApiAuthRoot,
    ApiRoutesRoot,
    ApiRouteTag(usize),
    ApiRouteRow(usize),
    ApiServerSelect(usize),
    ApiAuthValue(usize),
    ApiAuthRefreshToken(usize),
    ApiAuthUsername(usize),
    ApiAuthPassword(usize),
    ApiAuthAccessSave(usize),
    ApiAuthAccessClear(usize),
    ApiAuthRefreshSave(usize),
    ApiAuthRefreshClear(usize),
    ApiAuthSave(usize),
    ApiAuthClear(usize),
    ApiTryRequest,
    ApiPathParamInput(usize, usize),
    ApiQueryParamInput(usize, usize),
    ApiPathParamAllowedValue(usize, usize, usize),
    ApiQueryParamAllowedValue(usize, usize, usize),
    ApiBodyInput(usize),
    ApiInputExampleTab(usize),
    ApiInputSchemaTab(usize),
    ApiInputSchemaMenu(usize),
    ApiInputSchemaMenuItem(usize, usize),
    ApiInputSchemaBody(usize),
    ApiInputSchemaFold(usize, usize),
    ApiBodyScrollX(usize),
    ApiBodyFieldInput(usize, usize),
    ApiBodyAllowedValue(usize, usize, usize),
    ApiBodyFilePick(usize, usize),
    ApiOutputExampleTab(usize),
    ApiOutputSchemaTab(usize),
    ApiOutputStatusTab(usize, usize),
    ApiOutputSchemaMenu(usize),
    ApiOutputSchemaMenuItem(usize, usize),
    ApiOutputSchemaBody(usize),
    ApiOutputSchemaFold(usize, usize),
    ApiResponseBodyTab(usize),
    ApiResponseHeadersTab(usize),
    ApiResponseCurlTab(usize),
    ApiResponseBody(usize),
    ApiResponseScrollX(usize),
    ApiResponseUseAccessToken(usize, usize),
    ApiResponseSaveRefreshToken(usize, usize),
    ApiTabBody,
    ApiMockServerToggle,
    ApiMockServerDetails,
    ApiMockServerCopyUrl,
    ApiMockServerDetailsClose,
    ApiMockServerLogArea,
    ApiMockServerLogScrollY,
    ApiMockModeSelect,
    ApiMockProxyBaseInput,
    ApiMockGuideOpen,
    ApiMockGuideClose,
    ApiMockGuideBody,
    ApiMockGuideScrollY,
    ApiMockPythonManage,
    ApiMockPythonManageClose,
    ApiMockPythonModeToggle,
    ApiMockPythonCheckRuntime,
    ApiMockPythonPrepareVersion,
    ApiMockPythonPickUvPath,
    ApiMockPythonPickCustomPath,
    ApiMockPythonVersionOption(usize),
    ApiMockPythonUvPathInput,
    ApiMockPythonVersionInput,
    ApiMockPythonCustomPathInput,
    ApiMockExportOpenApi,
    ApiMockRouteEnable(usize),
    ApiMockRouteDetailsToggle(usize),
    ApiMockRoutePythonToggle(usize),
    ApiMockRouteReset(usize),
    ApiMockRouteResetConfirm,
    ApiMockRouteResetCancel,
    ApiMockContractPathToggle(usize),
    ApiMockContractQueryToggle(usize),
    ApiMockContractBodyToggle(usize),
    ApiMockContractPathFieldToggle(usize, usize),
    ApiMockContractQueryFieldToggle(usize, usize),
    ApiMockContractBodyFieldToggle(usize, usize),
    ApiMockContractFieldRequired(usize, ApiMockContractFieldGroup, usize),
    ApiMockContractFieldNullable(usize, ApiMockContractFieldGroup, usize),
    ApiMockContractFieldRemove(usize, ApiMockContractFieldGroup, usize),
    ApiMockContractFieldRemoveConfirm,
    ApiMockContractFieldRemoveCancel,
    ApiMockContractFieldPropInput(
        usize,
        ApiMockContractFieldGroup,
        usize,
        ApiMockContractFieldProp,
    ),
    ApiMockContractFieldAddConstraint(usize, ApiMockContractFieldGroup, usize),
    ApiMockContractFieldAddConstraintOption(
        usize,
        ApiMockContractFieldGroup,
        usize,
        ApiMockContractFieldProp,
    ),
    ApiMockStaticResponseInput(usize),
    ApiMockCombinedPython(usize),
    ApiMockContractInput(usize),
    ApiMockSignatureInput(usize),
    ApiMockPreludeInput(usize),
    ApiMockBodyInput(usize),
    ApiMockContractReset(usize),
    ApiMockPreludeReset(usize),
    ApiMockBodyReset(usize),
    ApiMockAddInputField(usize),
    ApiMockAddOutputField(usize),
    ApiMockAddManualRoute,
    ApiMockManualRouteOpen(usize),
    ApiMockManualRouteMethod(usize),
    ApiMockManualRoutePath(usize),
    ApiMockManualRouteRemove(usize),

    // Git panel
    GitWorkspaceToggle(usize),
    GitFile(usize, usize),
    GitFileDiff(usize, usize),
    GitFolder(usize, usize),
    GitFolderStage(usize, usize),
    GitCommit,
    GitCommitMenuToggle,
    GitCommitMenuItem(usize),
    GitPush(usize),
    GitRollbackStaged(usize),
    GitStageAll(usize),
    GitUnstageAll(usize),
    GitConfirmAction,
    GitConfirmCancel,
    GitMessageInput,
    GitRefresh,
    GitGraphToggle,
    GitGraphWorkspace(usize),
    GitGraphResize,
    GitGraphScroll,
    GitGraphCommit(usize, usize),
    GitGraphCopyCommit(usize, usize),
    GitGraphOpenCommit(usize, usize),

    // Sidebar
    SidebarSlot(crate::app::PanelId),

    // File tree
    FileTreeNode(usize),
    FileTreeArrow(usize),
    FileTreeMenuItem(usize),
    FileTreeCreateInput,
    FileTreeCreateConfirm,
    FileTreeCreateCancel,
    FileTreeRenameInput,
    FileTreeRenameConfirm,
    FileTreeRenameCancel,
    FileTreeMoveConfirm,
    FileTreeMoveCancel,
    FileTreeDeleteConfirm,
    FileTreeDeleteCancel,

    // Search
    SearchClose,
    SearchNext,
    SearchPrev,
    SearchCaseToggle,
    SearchInput,
    SearchPanelBody,

    // Project search
    ProjectSearchQueryInput,
    ProjectSearchIncludeInput,
    ProjectSearchExcludeInput,
    ProjectSearchRun,
    ProjectSearchCaseToggle,
    ProjectSearchHelp,
    ProjectSearchHelpPopup,
    ProjectSearchFileToggle(usize),
    ProjectSearchMatchJump(usize, usize),
    ProjectSearchPanelBody,

    // Tabs
    EditorTab(usize),
    EditorTabClose(usize),

    // Editor
    EditorFoldArrow(usize),
    EditorFoldDots(usize),
    EditorGitHunk(usize, usize),
    InlineGitPrevHunk,
    InlineGitNextHunk,
    InlineGitRollbackHunk,
    InlineGitPanelBody,
    GitDiffRollbackHunk(usize, usize),
    GitDiffPrevHunk,
    GitDiffNextHunk,
    GitDiffPanelBody,
    StickyLine(usize, usize),
    EditorScrollbarY,
    EditorScrollbarX,
    EditorTextBody,
    EditorMinimap,

    // Panels
    ResizeLeft,
    ResizeBottom,
    BottomPanelBody,
    LspLogArea(usize),
    LspScrollY,
    LspScrollX,
    LspLogScrollY(usize),
    LspLogScrollX(usize),
    CopyDiagnostic(usize),
    PopupCopyDiagnostic(usize),
    OpenDiagUrl(usize),
    PopupOpenDiagUrl(usize),
    ProblemJump(usize),
    ProblemUrl(usize),
    ProblemsTab(usize),
    ProblemFileToggle(usize),
    StatusBar,
    StatusDiagnostics,
    TerminalBody,
    TerminalScrollY,
    TerminalTab(usize),
    TerminalTabClose(usize),
    TerminalAdd,
    TerminalSearchClose,
    TerminalSearchNext,
    TerminalSearchPrev,
    TerminalSearchCaseToggle,
    TerminalSearchInput,
    HoverPopupScroll,
}

/// Тип UI элемента с его геометрией
#[derive(Debug, Clone)]
pub enum UiElement {
    Button {
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    IconButton {
        id: UiId,
        x: f32,
        y: f32,
        size: f32,
        active_square_width: Option<f32>,
        #[allow(dead_code)]
        custom_color: Option<[f32; 4]>,
    },
    TextInput {
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Rect {
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

impl UiElement {
    /// Проверяет, находится ли точка (mx, my) внутри элемента
    pub fn contains(&self, mx: f32, my: f32) -> bool {
        match self {
            UiElement::Button { x, y, w, h, .. } | UiElement::Rect { x, y, w, h, .. } => {
                mx >= *x && mx <= x + w && my >= *y && my <= y + h
            }
            UiElement::IconButton {
                x,
                y,
                size,
                active_square_width,
                custom_color: _,
                id: _,
            } => {
                if let Some(sq_w) = active_square_width {
                    let icon_center = y + size / 2.0;
                    let sq_y = (icon_center - sq_w / 2.0).round();
                    mx >= 0.0 && mx <= *sq_w && my >= sq_y && my <= sq_y + sq_w
                } else {
                    mx >= *x && mx <= x + size && my >= *y && my <= y + size
                }
            }
            UiElement::TextInput { x, y, w, h, .. } => {
                mx >= *x && mx <= x + w && my >= *y && my <= y + h
            }
        }
    }

    pub fn id(&self) -> UiId {
        match self {
            UiElement::Button { id, .. }
            | UiElement::IconButton { id, .. }
            | UiElement::TextInput { id, .. }
            | UiElement::Rect { id, .. } => *id,
        }
    }
}

/// Реестр всех UI элементов на текущем кадре
pub struct UiRegistry {
    elements: Vec<UiElement>,
    hovered: Option<UiId>,
    wants_pointer: bool,
    wants_text: bool,
    /// Индекс-метка: элементы начиная с этой позиции считаются
    /// "оверлейными" (настройки, диалоги). Используется для поиска
    /// только среди элементов оверлея, чтобы не активировать
    /// фоновые элементы редактора кликами сквозь модальные окна.
    overlay_mark: usize,
}

impl UiRegistry {
    pub fn new() -> Self {
        Self {
            elements: Vec::with_capacity(128),
            hovered: None,
            wants_pointer: false,
            wants_text: false,
            overlay_mark: 0,
        }
    }

    /// Очищает реестр перед новым кадром
    pub fn clear(&mut self) {
        self.elements.clear();
        self.hovered = None;
        self.wants_pointer = false;
        self.wants_text = false;
        self.overlay_mark = 0;
    }

    /// Ставит метку: все элементы, зарегистрированные ПОСЛЕ этого вызова,
    /// считаются оверлейными. `find_overlay_at` ищет только среди них.
    pub fn mark_overlay_start(&mut self) {
        self.overlay_mark = self.elements.len();
    }

    /// Ищет элемент под курсором только среди оверлейных элементов
    /// (зарегистрированных после последнего `mark_overlay_start`).
    pub fn find_overlay_at(&self, mx: f32, my: f32) -> Option<UiId> {
        self.elements[self.overlay_mark..]
            .iter()
            .rev()
            .find(|el| el.contains(mx, my))
            .map(|el| el.id())
    }

    /// Регистрирует кнопку и возвращает, наведена ли на неё мышь
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn register_button(
        &mut self,
        id: UiId,
        button: &Button,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        let hovered = button.render(renderer, mx, my, scale, pressed);

        self.elements.push(UiElement::Button {
            id,
            x: button.x,
            y: button.y,
            w: button.w,
            h: button.h,
        });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
        }
        hovered
    }

    /// Регистрирует иконочную кнопку
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn register_icon_button(
        &mut self,
        id: UiId,
        icon_button: &IconButton,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        let hovered = icon_button.render(renderer, mx, my, scale, pressed);

        self.elements.push(UiElement::IconButton {
            id,
            x: icon_button.x,
            y: icon_button.y,
            size: icon_button.size,
            active_square_width: icon_button.active_square_width,
            custom_color: icon_button.custom_color,
        });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
        }
        hovered
    }

    /// Регистрирует текстовое поле
    pub fn register_text_input(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        mx: f32,
        my: f32,
    ) -> bool {
        let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;

        self.elements.push(UiElement::TextInput { id, x, y, w, h });

        if hovered {
            self.hovered = Some(id);
            self.wants_text = true;
        }
        hovered
    }

    /// Регистрирует область-блокировщик: поглощает клики, но не меняет курсор.
    /// Используется для непрозрачных панелей, перекрывающих редактор.
    pub fn register_blocker(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        mx: f32,
        my: f32,
    ) -> bool {
        let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;
        self.elements.push(UiElement::Rect { id, x, y, w, h });
        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = false;
            self.wants_text = false;
        }
        hovered
    }

    /// Регистрирует кликабельную область (для элементов дерева файлов, чипов и т.д.)
    pub fn register_rect(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        mx: f32,
        my: f32,
    ) -> bool {
        let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;

        self.elements.push(UiElement::Rect { id, x, y, w, h });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
        }
        hovered
    }

    /// Находит элемент под курсором мыши
    pub fn find_at(&self, mx: f32, my: f32) -> Option<UiId> {
        // Ищем с конца (последние элементы рисуются поверх)
        self.elements
            .iter()
            .rev()
            .find(|el| el.contains(mx, my))
            .map(|el| el.id())
    }

    pub fn rect_for(&self, id: UiId) -> Option<(f32, f32, f32, f32)> {
        self.elements.iter().rev().find_map(|el| match el {
            UiElement::Button {
                id: el_id,
                x,
                y,
                w,
                h,
            }
            | UiElement::TextInput {
                id: el_id,
                x,
                y,
                w,
                h,
            }
            | UiElement::Rect {
                id: el_id,
                x,
                y,
                w,
                h,
            } if *el_id == id => Some((*x, *y, *w, *h)),
            UiElement::IconButton {
                id: el_id,
                x,
                y,
                size,
                ..
            } if *el_id == id => Some((*x, *y, *size, *size)),
            _ => None,
        })
    }

    /// Возвращает текущий наведённый элемент
    pub fn hovered(&self) -> Option<UiId> {
        self.hovered
    }

    /// Сбрасывает только флаги курсора (без очистки элементов).
    /// Используется при переходе к рендеру оверлеев (настройки, диалоги),
    /// чтобы элементы под оверлеем не влияли на тип курсора.
    pub fn reset_cursor_state(&mut self) {
        self.wants_pointer = false;
        self.wants_text = false;
    }

    /// Нужен ли курсор-указатель
    pub fn wants_pointer(&self) -> bool {
        self.wants_pointer
    }

    /// Нужен ли текстовый курсор
    pub fn wants_text(&self) -> bool {
        self.wants_text
    }

    /// Возвращает код курсора (0 = стрелка, 1 = указатель, 2 = текст)
    pub fn cursor_code(&self) -> u8 {
        if self.wants_text {
            2
        } else if self.wants_pointer {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_registry_hit_testing_overlay_and_cursor_state_end_to_end() {
        let mut registry = UiRegistry::new();

        assert!(registry.register_rect(UiId::EditorTextBody, 0.0, 0.0, 100.0, 40.0, 10.0, 10.0));
        assert_eq!(registry.hovered(), Some(UiId::EditorTextBody));
        assert!(registry.wants_pointer());
        assert_eq!(registry.cursor_code(), 1);

        assert!(registry.register_text_input(UiId::SearchInput, 5.0, 5.0, 50.0, 20.0, 10.0, 10.0));
        assert_eq!(registry.find_at(10.0, 10.0), Some(UiId::SearchInput));
        assert!(registry.wants_text());
        assert_eq!(registry.cursor_code(), 2);

        registry.mark_overlay_start();
        registry.register_blocker(UiId::BottomPanelBody, 0.0, 0.0, 200.0, 200.0, 10.0, 10.0);
        assert_eq!(
            registry.find_overlay_at(10.0, 10.0),
            Some(UiId::BottomPanelBody)
        );

        registry.reset_cursor_state();
        assert_eq!(registry.cursor_code(), 0);

        registry.clear();
        assert_eq!(registry.find_at(10.0, 10.0), None);
        assert_eq!(registry.find_overlay_at(10.0, 10.0), None);
    }

    #[test]
    fn ui_element_icon_active_square_uses_fitts_law_hitbox() {
        let icon = UiElement::IconButton {
            id: UiId::SidebarSlot(crate::app::PanelId::Explorer),
            x: 12.0,
            y: 100.0,
            size: 24.0,
            active_square_width: Some(48.0),
            custom_color: None,
        };

        assert!(icon.contains(1.0, 88.0));
        assert!(icon.contains(47.0, 135.0));
        assert!(!icon.contains(49.0, 112.0));
    }

    #[test]
    fn ui_element_ids_and_regular_icon_hitboxes_are_stable() {
        let button = UiElement::Button {
            id: UiId::WelcomeOpenFile,
            x: 10.0,
            y: 20.0,
            w: 80.0,
            h: 24.0,
        };
        let icon = UiElement::IconButton {
            id: UiId::SearchClose,
            x: 30.0,
            y: 40.0,
            size: 18.0,
            active_square_width: None,
            custom_color: Some([1.0, 0.0, 0.0, 1.0]),
        };
        let text = UiElement::TextInput {
            id: UiId::TerminalSearchInput,
            x: 0.0,
            y: 0.0,
            w: 120.0,
            h: 22.0,
        };
        let rect = UiElement::Rect {
            id: UiId::HoverPopupScroll,
            x: 5.0,
            y: 6.0,
            w: 7.0,
            h: 8.0,
        };

        assert_eq!(button.id(), UiId::WelcomeOpenFile);
        assert_eq!(icon.id(), UiId::SearchClose);
        assert_eq!(text.id(), UiId::TerminalSearchInput);
        assert_eq!(rect.id(), UiId::HoverPopupScroll);
        assert!(button.contains(90.0, 44.0));
        assert!(icon.contains(48.0, 58.0));
        assert!(!icon.contains(49.0, 58.0));
        assert!(text.contains(120.0, 22.0));
        assert!(rect.contains(12.0, 14.0));
    }
}
