/// Декларативная система UI для автоматической обработки кнопок и виджетов
/// Устраняет необходимость вручную прописывать координаты и биндинги для каждой кнопки
use crate::renderer::Renderer;
use crate::widgets::{Button, ButtonView, IconButton};

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
    SettingsDatabaseAdjust(usize, i8),

    // Settings IDE
    SettingsIdeAddWorkspace,
    SettingsIdeRemoveWorkspace(usize),
    SettingsIdeAddIgnore,
    SettingsIdeRemoveIgnore(usize),
    SettingsIdeIgnoreInput,

    // Settings platform/tooling
    SettingsToolPick(usize),
    SettingsToolClear(usize),
    SettingsToolInstall(usize),
    SettingsOpenToolInstallLog,
    SettingsCloseToolInstallLog,
    SettingsCancelToolInstall,
    SettingsCopyToolInstallLog,
    SettingsToolInstallLogBackdrop,
    SettingsToolInstallLogBody,
    SettingsOpenDirectory(usize),
    SettingsCopyGraphicsDiagnostics,
    SettingsRefreshTools,

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
    ApiRouteFilterInput,
    ApiRouteFilterClear,
    ApiRouteTag(usize),
    ApiRouteRow(usize),
    ApiRoutePathText(usize),
    ApiRouteSummaryText(usize),
    ApiRouteDescriptionText(usize),
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

    // Database tools
    DatabasePanelBody,
    DatabaseAdd,
    DatabaseDelete,
    DatabaseRefresh,
    DatabaseConnectionRow(usize),
    DatabaseConnectionArrow(usize),
    DatabaseRow(usize, usize),
    DatabaseArrow(usize, usize),
    DatabaseTableRow(usize, usize, usize),
    DatabaseContextItem(usize),
    DatabaseDialogBackdrop,
    DatabaseDialogBody,
    DatabaseDialogField(crate::app::database::DatabaseFormField),
    DatabaseDialogSecretEye(crate::app::database::DatabaseFormField),
    DatabaseDialogTls,
    DatabaseDialogColor,
    DatabaseDialogSshToggle,
    DatabaseDialogJumpToggle,
    DatabaseDialogRememberPostgres,
    DatabaseDialogRememberSshPassword,
    DatabaseDialogRememberSshPassphrase,
    DatabaseDialogRememberJumpPassword,
    DatabaseDialogRememberJumpPassphrase,
    DatabaseDialogTest,
    DatabaseDialogSave,
    DatabaseDialogCancel,
    DatabaseDeleteConfirm,
    DatabaseDeleteCancel,
    DatabaseHostKeyTrustOnce,
    DatabaseHostKeyTrustStore,
    DatabaseHostKeyCancel,
    DatabaseDdlBody,
    DatabaseDdlScroll,
    DatabaseTableBody,
    DatabaseTableUnavailableText,
    DatabaseTableAddRow,
    DatabaseTableDeleteRows,
    DatabaseTableUndo,
    DatabaseTableSave,
    DatabaseTablePreview,
    DatabaseTableRefresh,
    DatabaseTablePageFirst,
    DatabaseTablePagePrevious,
    DatabaseTablePageNext,
    DatabaseTablePageLast,
    DatabaseTableLimit,
    DatabaseTableWhereInput,
    DatabaseTableOrderInput,
    DatabaseTableHeader(usize),
    DatabaseTableColumnResize(usize),
    DatabaseGridRow(usize),
    DatabaseTableCell(usize, usize),
    DatabaseTableCellEditor,
    DatabaseTableEnumOption(usize),
    DatabaseTableEnumPreviousPage,
    DatabaseTableEnumNextPage,
    DatabaseTableDatePreviousMonth,
    DatabaseTableDateNextMonth,
    DatabaseTableDateDay(u8),
    DatabaseTableDateToday,
    DatabaseTableDateNow,
    DatabaseTableGridBody,
    DatabaseTableScrollY,
    DatabaseTableScrollX,
    DatabaseTableModalBackdrop,
    DatabaseTableModalBody,
    DatabaseTableModalInput,
    DatabaseTableModalPrimary,
    DatabaseTableModalSecondary,
    DatabaseTableModalTertiary,
    DatabaseTableModalScroll,
    DatabaseTableModalScrollX,
    DatabaseQueryRun,
    DatabaseQueryCancel,
    DatabaseQueryExplain,
    DatabaseQueryExplainAnalyze,
    DatabaseQueryFormat,
    DatabaseQueryHistory,
    DatabaseQueryNextDiagnostic,
    DatabaseQueryResultTab(usize),
    DatabaseQueryHistoryEntry(usize),
    DatabaseQueryResultBody,
    DatabaseQueryResultResize,
    DatabaseQueryColumnResize(usize),
    DatabaseQueryScrollY,
    DatabaseQueryScrollX,
    DatabaseQueryReviewBackdrop,
    DatabaseQueryReviewBody,
    DatabaseQueryReviewMessagesBody,
    DatabaseQueryReviewMessagesScrollY,
    DatabaseQueryCommit,
    DatabaseQueryRollback,

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
    GitRepoActionMenu(usize),
    GitFetch(usize),
    GitPull(usize),
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
    ProjectSearchFilterInput,
    ProjectSearchRun,
    ProjectSearchCaseToggle,
    ProjectSearchHelp,
    ProjectSearchHelpPopup,
    ProjectSearchFileToggle(usize),
    ProjectSearchMatchJump(usize, usize),
    ProjectSearchQueryScrollbarY,
    ProjectSearchQueryScrollbarX,
    ProjectSearchScrollbar,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiClipRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub(crate) fn fit_centered_rect(
    window_w: f32,
    window_h: f32,
    desired_w: f32,
    desired_h: f32,
    margin: f32,
) -> UiClipRect {
    let window_w = window_w.max(0.0);
    let window_h = window_h.max(0.0);
    let margin = margin.max(0.0);
    let w = desired_w.max(0.0).min((window_w - margin * 2.0).max(0.0));
    let h = desired_h.max(0.0).min((window_h - margin * 2.0).max(0.0));
    UiClipRect::new(
        ((window_w - w) * 0.5).max(0.0).round(),
        ((window_h - h) * 0.5).max(0.0).round(),
        w.round(),
        h.round(),
    )
}

impl UiClipRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn intersect(self, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || !self.w.is_finite()
            || !self.h.is_finite()
            || !x.is_finite()
            || !y.is_finite()
            || !w.is_finite()
            || !h.is_finite()
            || self.w <= 0.0
            || self.h <= 0.0
            || w <= 0.0
            || h <= 0.0
        {
            return None;
        }
        let left = self.x.max(x);
        let top = self.y.max(y);
        let right = (self.x + self.w).min(x + w);
        let bottom = (self.y + self.h).min(y + h);
        (right > left && bottom > top).then_some(Self {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        })
    }

    pub fn intersect_rect(self, other: Self) -> Option<Self> {
        self.intersect(other.x, other.y, other.w, other.h)
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        self.w > 0.0
            && self.h > 0.0
            && x >= self.x
            && x <= self.x + self.w
            && y >= self.y
            && y <= self.y + self.h
    }
}

pub(crate) fn point_in_rect(
    mx: f32,
    my: f32,
    rect: (f32, f32, f32, f32),
) -> bool {
    rect.0.is_finite()
        && rect.1.is_finite()
        && rect.2.is_finite()
        && rect.3.is_finite()
        && rect.2 > 0.0
        && rect.3 > 0.0
        && mx >= rect.0
        && mx <= rect.0 + rect.2
        && my >= rect.1
        && my <= rect.1 + rect.3
}

fn icon_hit_rect(
    x: f32,
    y: f32,
    size: f32,
    active_square_width: Option<f32>,
) -> Option<UiClipRect> {
    let (hit_x, hit_y, hit_w, hit_h) = if let Some(square_width) = active_square_width {
        let square_y = (y + size * 0.5 - square_width * 0.5).round();
        (0.0, square_y, square_width, square_width)
    } else {
        (x, y, size, size)
    };
    UiClipRect::new(hit_x, hit_y, hit_w, hit_h)
        .intersect(hit_x, hit_y, hit_w, hit_h)
}

impl UiElement {
    fn valid_rect(x: f32, y: f32, w: f32, h: f32) -> bool {
        x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0
    }

    /// Проверяет, находится ли точка (mx, my) внутри элемента.
    pub fn contains(&self, mx: f32, my: f32) -> bool {
        if !mx.is_finite() || !my.is_finite() {
            return false;
        }
        match self {
            UiElement::Button { x, y, w, h, .. }
            | UiElement::Rect { x, y, w, h, .. }
            | UiElement::TextInput { x, y, w, h, .. } => {
                Self::valid_rect(*x, *y, *w, *h)
                    && mx >= *x
                    && mx <= *x + *w
                    && my >= *y
                    && my <= *y + *h
            }
            UiElement::IconButton { x, y, size, active_square_width, .. } => {
                icon_hit_rect(*x, *y, *size, *active_square_width)
                    .is_some_and(|rect| rect.contains(mx, my))
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
    clip_stack: Vec<UiClipRect>,
    interaction_stack: Vec<bool>,
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
            clip_stack: Vec::with_capacity(4),
            interaction_stack: Vec::with_capacity(4),
            hovered: None,
            wants_pointer: false,
            wants_text: false,
            overlay_mark: 0,
        }
    }

    /// Очищает реестр перед новым кадром
    pub fn clear(&mut self) {
        self.elements.clear();
        self.clip_stack.clear();
        self.interaction_stack.clear();
        self.hovered = None;
        self.wants_pointer = false;
        self.wants_text = false;
        self.overlay_mark = 0;
    }

    pub fn push_clip(&mut self, clip: UiClipRect) {
        let clip = match self.clip_stack.last().copied() {
            Some(parent) => parent.intersect_rect(clip),
            None => Self::valid_rect(clip.x, clip.y, clip.w, clip.h),
        }
        .unwrap_or(UiClipRect::new(clip.x, clip.y, 0.0, 0.0));
        self.clip_stack.push(clip);
    }

    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn active_clip(&self) -> Option<UiClipRect> {
        self.clip_stack.last().copied()
    }

    fn clipped_rect(
        &self,
        explicit_clip: UiClipRect,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> Option<UiClipRect> {
        let clip = match self.active_clip() {
            Some(parent) => parent.intersect_rect(explicit_clip)?,
            None => explicit_clip,
        };
        clip.intersect(x, y, w, h)
    }

    fn valid_rect(x: f32, y: f32, w: f32, h: f32) -> Option<UiClipRect> {
        UiClipRect::new(x, y, w, h).intersect(x, y, w, h)
    }

    pub fn push_interactions_enabled(&mut self, enabled: bool) {
        let enabled = self
            .interaction_stack
            .last()
            .copied()
            .unwrap_or(true)
            && enabled;
        self.interaction_stack.push(enabled);
    }

    pub fn pop_interactions_enabled(&mut self) {
        self.interaction_stack.pop();
    }

    fn interactions_enabled(&self) -> bool {
        self.interaction_stack.last().copied().unwrap_or(true)
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
        if !self.interactions_enabled() {
            button.render(
                renderer,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                scale,
                false,
            );
            return false;
        }
        self.register_button_view(id, button.as_view(), renderer, mx, my, scale, pressed)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn register_button_view(
        &mut self,
        id: UiId,
        button: ButtonView<'_>,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        if !self.interactions_enabled() {
            button.render(
                renderer,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                scale,
                false,
            );
            return false;
        }
        if let Some(clip) = self.active_clip() {
            return self.register_button_view_clipped(
                id, button, clip, renderer, mx, my, scale, pressed,
            );
        }
        let Some(_) = Self::valid_rect(button.x, button.y, button.w, button.h) else {
            button.render(renderer, f32::NEG_INFINITY, f32::NEG_INFINITY, scale, false);
            return false;
        };
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
            self.wants_text = false;
        }
        hovered
    }

    /// Регистрирует иконочную кнопку
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn register_button_view_clipped(
        &mut self,
        id: UiId,
        button: ButtonView<'_>,
        clip: UiClipRect,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        if !self.interactions_enabled() {
            button.render(
                renderer,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                scale,
                false,
            );
            return false;
        }
        let Some(rect) = self.clipped_rect(clip, button.x, button.y, button.w, button.h)
        else {
            return false;
        };
        let hovered = rect.contains(mx, my);
        let render_mx = if hovered { mx } else { f32::NEG_INFINITY };
        let render_my = if hovered { my } else { f32::NEG_INFINITY };
        button.render(renderer, render_mx, render_my, scale, pressed && hovered);
        self.elements.push(UiElement::Button {
            id,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        });
        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
            self.wants_text = false;
        }
        hovered
    }

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
        if !self.interactions_enabled() {
            icon_button.render(
                renderer,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                scale,
                false,
            );
            return false;
        }
        if let Some(clip) = self.active_clip() {
            return self.register_icon_button_clipped(
                id,
                icon_button,
                clip,
                renderer,
                mx,
                my,
                scale,
                pressed,
            );
        }
        let Some(_) = icon_hit_rect(
            icon_button.x,
            icon_button.y,
            icon_button.size,
            icon_button.active_square_width,
        ) else {
            icon_button.render(renderer, f32::NEG_INFINITY, f32::NEG_INFINITY, scale, false);
            return false;
        };
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
            self.wants_text = false;
        }
        hovered
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn register_icon_button_clipped(
        &mut self,
        id: UiId,
        icon_button: &IconButton,
        clip: UiClipRect,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        if !self.interactions_enabled() {
            icon_button.render(
                renderer,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                scale,
                false,
            );
            return false;
        }
        let Some(hit_rect) = icon_hit_rect(
            icon_button.x,
            icon_button.y,
            icon_button.size,
            icon_button.active_square_width,
        ) else {
            return false;
        };
        let Some(rect) = self.clipped_rect(
            clip,
            hit_rect.x,
            hit_rect.y,
            hit_rect.w,
            hit_rect.h,
        ) else {
            return false;
        };
        let hovered = rect.contains(mx, my);
        let render_mx = if hovered { mx } else { f32::NEG_INFINITY };
        let render_my = if hovered { my } else { f32::NEG_INFINITY };
        icon_button.render(renderer, render_mx, render_my, scale, pressed && hovered);
        self.elements.push(UiElement::Rect {
            id,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        });
        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
            self.wants_text = false;
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
        if !self.interactions_enabled() {
            return false;
        }
        if let Some(clip) = self.active_clip() {
            return self.register_text_input_clipped(id, x, y, w, h, clip, mx, my);
        }
        let Some(rect) = Self::valid_rect(x, y, w, h) else { return false; };
        let hovered = rect.contains(mx, my);

        self.elements.push(UiElement::TextInput { id, x, y, w, h });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = false;
            self.wants_text = true;
        }
        hovered
    }

    pub fn register_text_input_clipped(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        clip: UiClipRect,
        mx: f32,
        my: f32,
    ) -> bool {
        if !self.interactions_enabled() {
            return false;
        }
        let Some(rect) = self.clipped_rect(clip, x, y, w, h) else {
            return false;
        };
        let hovered = rect.contains(mx, my);
        self.elements.push(UiElement::TextInput {
            id,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        });
        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = false;
            self.wants_text = true;
        }
        hovered
    }

    /// Регистрирует выделяемую текстовую область.
    pub fn register_text_region(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        mx: f32,
        my: f32,
    ) -> bool {
        if !self.interactions_enabled() {
            return false;
        }
        if let Some(clip) = self.active_clip() {
            let Some(rect) = self.clipped_rect(clip, x, y, w, h) else {
                return false;
            };
            let hovered = rect.contains(mx, my);
            self.elements.push(UiElement::Rect {
                id,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
            });
            if hovered {
                self.hovered = Some(id);
                self.wants_pointer = false;
                self.wants_text = true;
            }
            return hovered;
        }
        let Some(rect) = Self::valid_rect(x, y, w, h) else { return false; };
        let hovered = rect.contains(mx, my);

        self.elements.push(UiElement::Rect { id, x, y, w, h });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = false;
            self.wants_text = true;
        }
        hovered
    }

    /// Регистрирует выделяемую текстовую область, сохраняя стандартный курсор.
    pub fn register_default_cursor_text_region(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        mx: f32,
        my: f32,
    ) -> bool {
        self.register_blocker(id, x, y, w, h, mx, my)
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
        if !self.interactions_enabled() {
            return false;
        }
        if let Some(clip) = self.active_clip() {
            let Some(rect) = self.clipped_rect(clip, x, y, w, h) else {
                return false;
            };
            let hovered = rect.contains(mx, my);
            self.elements.push(UiElement::Rect {
                id,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
            });
            if hovered {
                self.hovered = Some(id);
                self.wants_pointer = false;
                self.wants_text = false;
            }
            return hovered;
        }
        let Some(rect) = Self::valid_rect(x, y, w, h) else { return false; };
        let hovered = rect.contains(mx, my);

        self.elements.push(UiElement::Rect { id, x, y, w, h });
        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = false;
            self.wants_text = false;
        }
        hovered
    }

    pub fn register_blocker_clipped(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        clip: UiClipRect,
        mx: f32,
        my: f32,
    ) -> bool {
        if !self.interactions_enabled() {
            return false;
        }
        let Some(rect) = self.clipped_rect(clip, x, y, w, h) else {
            return false;
        };
        let hovered = rect.contains(mx, my);
        self.elements.push(UiElement::Rect {
            id,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        });
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
        if !self.interactions_enabled() {
            return false;
        }
        if let Some(clip) = self.active_clip() {
            return self.register_rect_clipped(id, x, y, w, h, clip, mx, my);
        }
        let Some(rect) = Self::valid_rect(x, y, w, h) else { return false; };
        let hovered = rect.contains(mx, my);

        self.elements.push(UiElement::Rect { id, x, y, w, h });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
            self.wants_text = false;
        }
        hovered
    }

    pub fn register_rect_clipped(
        &mut self,
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        clip: UiClipRect,
        mx: f32,
        my: f32,
    ) -> bool {
        if !self.interactions_enabled() {
            return false;
        }
        let Some(rect) = self.clipped_rect(clip, x, y, w, h) else {
            return false;
        };
        let hovered = rect.contains(mx, my);
        self.elements.push(UiElement::Rect {
            id,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        });
        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
            self.wants_text = false;
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

        registry.reset_cursor_state();
        assert!(registry.register_text_region(
            UiId::ApiRouteDescriptionText(3),
            0.0,
            0.0,
            100.0,
            40.0,
            10.0,
            10.0,
        ));
        assert_eq!(
            registry.find_at(10.0, 10.0),
            Some(UiId::ApiRouteDescriptionText(3))
        );
        assert!(registry.wants_text());
        assert!(!registry.wants_pointer());
        assert_eq!(registry.cursor_code(), 2);

        registry.mark_overlay_start();
        registry.register_blocker(UiId::BottomPanelBody, 0.0, 0.0, 200.0, 200.0, 10.0, 10.0);
        assert_eq!(
            registry.find_overlay_at(10.0, 10.0),
            Some(UiId::BottomPanelBody)
        );

        registry.reset_cursor_state();
        assert_eq!(registry.cursor_code(), 0);

        assert!(registry.register_default_cursor_text_region(
            UiId::DatabaseTableUnavailableText,
            0.0,
            0.0,
            100.0,
            40.0,
            10.0,
            10.0,
        ));
        assert_eq!(
            registry.find_at(10.0, 10.0),
            Some(UiId::DatabaseTableUnavailableText)
        );
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

    #[test]
    fn clipped_hitboxes_are_limited_to_visible_region() {
        let mut registry = UiRegistry::new();
        let clip = UiClipRect::new(20.0, 20.0, 40.0, 30.0);

        assert!(!registry.register_rect_clipped(
            UiId::DatabaseQueryHistoryEntry(0),
            0.0,
            0.0,
            10.0,
            10.0,
            clip,
            5.0,
            5.0,
        ));
        assert!(registry.register_rect_clipped(
            UiId::DatabaseTableCell(1, 2),
            0.0,
            10.0,
            80.0,
            60.0,
            clip,
            30.0,
            30.0,
        ));
        assert_eq!(
            registry.rect_for(UiId::DatabaseTableCell(1, 2)),
            Some((20.0, 20.0, 40.0, 30.0))
        );
        assert_eq!(registry.find_at(10.0, 30.0), None);
        assert_eq!(
            registry.find_at(30.0, 30.0),
            Some(UiId::DatabaseTableCell(1, 2))
        );
    }

    fn assert_partial_hitbox_clips(id: UiId) {
        let mut registry = UiRegistry::new();
        let clip = UiClipRect::new(20.0, 20.0, 40.0, 30.0);
        assert!(registry.register_rect_clipped(
            id,
            0.0,
            10.0,
            80.0,
            60.0,
            clip,
            30.0,
            30.0,
        ));
        assert_eq!(registry.rect_for(id), Some((20.0, 20.0, 40.0, 30.0)));
        assert_eq!(registry.find_at(10.0, 30.0), None);
    }

    #[test]
    fn bug_18_table_header_hitbox_is_clipped() {
        assert_partial_hitbox_clips(UiId::DatabaseTableHeader(0));
    }

    #[test]
    fn bug_19_table_column_resize_hitbox_is_clipped() {
        assert_partial_hitbox_clips(UiId::DatabaseTableColumnResize(0));
    }

    #[test]
    fn bug_20_table_cell_hitbox_is_clipped() {
        assert_partial_hitbox_clips(UiId::DatabaseTableCell(2, 3));
    }

    #[test]
    fn bug_21_table_row_gutter_hitbox_is_clipped() {
        assert_partial_hitbox_clips(UiId::DatabaseTableRow(0, 0, 2));
    }

    #[test]
    fn bug_26_connection_form_controls_are_clipped() {
        assert_partial_hitbox_clips(UiId::DatabaseDialogField(
            crate::app::database::DatabaseFormField::Host,
        ));
    }

    #[test]
    fn bug_31_database_tree_rows_are_clipped() {
        assert_partial_hitbox_clips(UiId::DatabaseConnectionRow(1));
    }

    #[test]
    fn bug_33_git_graph_rows_are_clipped() {
        assert_partial_hitbox_clips(UiId::GitGraphCommit(0, 1));
    }

    #[test]
    fn bug_35_problem_rows_are_clipped() {
        assert_partial_hitbox_clips(UiId::ProblemJump(1));
    }

    #[test]
    fn bug_36_problem_url_hitbox_is_clipped() {
        assert_partial_hitbox_clips(UiId::ProblemUrl(1));
    }

    #[test]
    fn bug_41_api_toolbar_hitboxes_are_clipped() {
        assert_partial_hitbox_clips(UiId::ApiSpecOpen(0));
    }

    #[test]
    fn clipped_text_inputs_and_blockers_never_register_zero_area_rectangles() {
        let mut registry = UiRegistry::new();
        let clip = UiClipRect::new(10.0, 10.0, 20.0, 20.0);
        assert!(!registry.register_text_input_clipped(
            UiId::DatabaseDialogField(crate::app::database::DatabaseFormField::Host),
            40.0,
            40.0,
            20.0,
            20.0,
            clip,
            45.0,
            45.0,
        ));
        assert!(!registry.register_blocker_clipped(
            UiId::DatabaseTableGridBody,
            0.0,
            0.0,
            0.0,
            10.0,
            clip,
            0.0,
            0.0,
        ));
        assert_eq!(registry.find_at(45.0, 45.0), None);
    }
    #[test]
    fn r2_011_clipped_icon_has_no_left_edge_phantom_hitbox() {
        let mut registry = UiRegistry::new();
        let icon = IconButton {
            x: 300.0,
            y: 40.0,
            size: 20.0,
            icon: None,
            is_active: false,
            icon_size: None,
            active_square_width: None,
            custom_color: None,
        };
        let clip = UiClipRect::new(290.0, 30.0, 40.0, 40.0);
        let rect = clip.intersect(icon.x, icon.y, icon.size, icon.size).unwrap();
        registry.elements.push(UiElement::Rect { id: UiId::SearchClose, x: rect.x, y: rect.y, w: rect.w, h: rect.h });
        assert_eq!(registry.find_at(1.0, 50.0), None);
        assert_eq!(registry.find_at(305.0, 50.0), Some(UiId::SearchClose));
    }

    #[test]
    fn r2_012_clipped_icon_never_reexpands_vertically() {
        let mut registry = UiRegistry::new();
        registry.elements.push(UiElement::Rect {
            id: UiId::SearchClose,
            x: 20.0,
            y: 20.0,
            w: 18.0,
            h: 4.0,
        });
        assert_eq!(registry.find_at(25.0, 23.0), Some(UiId::SearchClose));
        assert_eq!(registry.find_at(25.0, 28.0), None);
    }

    #[test]
    fn r2_013_clipped_icon_hover_uses_visible_rect_only() {
        let clip = UiClipRect::new(20.0, 20.0, 20.0, 10.0);
        let visible = clip.intersect(20.0, 15.0, 20.0, 20.0).unwrap();
        assert!(!visible.contains(30.0, 16.0));
        assert!(visible.contains(30.0, 22.0));
    }

    #[test]
    fn r2_014_clipped_button_hover_uses_visible_rect_only() {
        let clip = UiClipRect::new(20.0, 20.0, 30.0, 10.0);
        let visible = clip.intersect(10.0, 10.0, 60.0, 30.0).unwrap();
        assert!(!visible.contains(15.0, 25.0));
        assert!(visible.contains(25.0, 25.0));
    }


    #[test]
    fn r3_001_explicit_clip_is_intersected_with_parent_clip() {
        let mut registry = UiRegistry::new();
        registry.push_clip(UiClipRect::new(10.0, 10.0, 20.0, 20.0));
        assert!(registry.register_rect_clipped(
            UiId::SearchClose,
            0.0,
            0.0,
            100.0,
            100.0,
            UiClipRect::new(0.0, 0.0, 100.0, 100.0),
            15.0,
            15.0,
        ));
        assert_eq!(registry.rect_for(UiId::SearchClose), Some((10.0, 10.0, 20.0, 20.0)));
        assert_eq!(registry.find_at(5.0, 5.0), None);
    }

    #[test]
    fn r3_002_topmost_element_controls_cursor_shape() {
        let mut registry = UiRegistry::new();
        assert!(registry.register_text_input(
            UiId::SearchInput, 0.0, 0.0, 20.0, 20.0, 10.0, 10.0,
        ));
        assert_eq!(registry.cursor_code(), 2);
        assert!(registry.register_rect(
            UiId::SearchClose, 0.0, 0.0, 20.0, 20.0, 10.0, 10.0,
        ));
        assert_eq!(registry.find_at(10.0, 10.0), Some(UiId::SearchClose));
        assert_eq!(registry.cursor_code(), 1);
    }

    #[test]
    fn r3_003_zero_sized_elements_are_not_registered() {
        let mut registry = UiRegistry::new();
        assert!(!registry.register_rect(
            UiId::SearchClose, 1.0, 1.0, 0.0, 20.0, 1.0, 10.0,
        ));
        assert!(!registry.register_text_input(
            UiId::SearchInput, 1.0, 1.0, 20.0, 0.0, 10.0, 1.0,
        ));
        assert_eq!(registry.find_at(1.0, 1.0), None);
    }

    #[test]
    fn r3_004_negative_and_non_finite_rectangles_are_rejected_consistently() {
        let mut registry = UiRegistry::new();
        for (w, h) in [(-1.0, 10.0), (10.0, -1.0), (f32::NAN, 10.0), (10.0, f32::INFINITY)] {
            assert!(!registry.register_rect(
                UiId::SearchClose, 0.0, 0.0, w, h, 0.0, 0.0,
            ));
            assert!(!registry.register_rect_clipped(
                UiId::SearchClose,
                0.0,
                0.0,
                w,
                h,
                UiClipRect::new(0.0, 0.0, 100.0, 100.0),
                0.0,
                0.0,
            ));
        }
        assert_eq!(registry.find_at(0.0, 0.0), None);
    }

    #[test]
    fn r3_005_active_square_geometry_is_shared_by_clipped_and_unclipped_paths() {
        let rect = icon_hit_rect(300.0, 20.0, 16.0, Some(40.0)).unwrap();
        assert_eq!(rect, UiClipRect::new(0.0, 8.0, 40.0, 40.0));
        let clipped = UiClipRect::new(0.0, 0.0, 25.0, 100.0)
            .intersect_rect(rect)
            .unwrap();
        assert_eq!(clipped, UiClipRect::new(0.0, 8.0, 25.0, 40.0));
    }

}
