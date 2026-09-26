/// Обработчики UI событий - централизованная логика для кликов по кнопкам
/// Устраняет дублирование кода между input.rs и ui.rs
use crate::app::App;
use crate::ui_system::UiId;

mod ui_database;
mod ui_editor;
mod ui_git;
mod ui_lsp;
mod ui_panels;
mod ui_settings;

pub(crate) use ui_editor::repeated_ui_click;

impl App {
    /// Обрабатывает клик по UI элементу
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_ui_click(&mut self, id: UiId) {
        let same_click_target = self.last_click_ui_id == Some(id);
        self.last_click_ui_id = Some(id);
        match id {
            UiId::DatabasePanelBody
            | UiId::DatabaseGlobalErrorCopy
            | UiId::DatabaseAdd
            | UiId::DatabaseDelete
            | UiId::DatabaseRefresh
            | UiId::DatabaseConnectionRow(_)
            | UiId::DatabaseConnectionArrow(_)
            | UiId::DatabaseRow(_, _)
            | UiId::DatabaseArrow(_, _)
            | UiId::DatabaseTableRow(_, _, _)
            | UiId::DatabaseContextItem(_)
            | UiId::DatabaseDialogBackdrop
            | UiId::DatabaseDialogBody
            | UiId::DatabaseDialogField(_)
            | UiId::DatabaseDialogSecretEye(_)
            | UiId::DatabaseDialogTls
            | UiId::DatabaseDialogColor
            | UiId::DatabaseDialogSshToggle
            | UiId::DatabaseDialogJumpToggle
            | UiId::DatabaseDialogRememberPostgres
            | UiId::DatabaseDialogRememberSshPassword
            | UiId::DatabaseDialogRememberSshPassphrase
            | UiId::DatabaseDialogRememberJumpPassword
            | UiId::DatabaseDialogRememberJumpPassphrase
            | UiId::DatabaseDialogTest
            | UiId::DatabaseDialogSave
            | UiId::DatabaseDialogCancel
            | UiId::DatabaseDeleteConfirm
            | UiId::DatabaseDeleteCancel
            | UiId::DatabaseHostKeyTrustOnce
            | UiId::DatabaseHostKeyTrustStore
            | UiId::DatabaseHostKeyCancel
            | UiId::DatabaseDdlBody
            | UiId::DatabaseDdlScroll
            | UiId::DatabaseTableBody
            | UiId::DatabaseTableUnavailableText
            | UiId::DatabaseTableAddRow
            | UiId::DatabaseTableDeleteRows
            | UiId::DatabaseTableUndo
            | UiId::DatabaseTableSave
            | UiId::DatabaseTablePreview
            | UiId::DatabaseTableRefresh
            | UiId::DatabaseTablePageFirst
            | UiId::DatabaseTablePagePrevious
            | UiId::DatabaseTablePageNext
            | UiId::DatabaseTablePageLast
            | UiId::DatabaseTableLimit
            | UiId::DatabaseTableWhereInput
            | UiId::DatabaseTableOrderInput
            | UiId::DatabaseTableHeader(_)
            | UiId::DatabaseTableColumnResize(_)
            | UiId::DatabaseGridRow(_)
            | UiId::DatabaseTableCell(_, _)
            | UiId::DatabaseTableCellEditor
            | UiId::DatabaseTableEnumOption(_)
            | UiId::DatabaseTableEnumPreviousPage
            | UiId::DatabaseTableEnumNextPage
            | UiId::DatabaseTableDatePreviousMonth
            | UiId::DatabaseTableDateNextMonth
            | UiId::DatabaseTableDateDay(_)
            | UiId::DatabaseTableDateToday
            | UiId::DatabaseTableDateNow
            | UiId::DatabaseTableGridBody
            | UiId::DatabaseTableScrollY
            | UiId::DatabaseTableScrollX
            | UiId::DatabaseQueryScrollY
            | UiId::DatabaseQueryScrollX
            | UiId::DatabaseTableModalBackdrop
            | UiId::DatabaseTableModalBody
            | UiId::DatabaseTableModalInput
            | UiId::DatabaseTableModalPrimary
            | UiId::DatabaseTableModalSecondary
            | UiId::DatabaseTableModalTertiary
            | UiId::DatabaseTableModalScroll
            | UiId::DatabaseTableModalScrollX
            | UiId::DatabaseQueryRun
            | UiId::DatabaseQueryCancel
            | UiId::DatabaseQueryExplain
            | UiId::DatabaseQueryExplainAnalyze
            | UiId::DatabaseQueryFormat
            | UiId::DatabaseQueryHistory
            | UiId::DatabaseQueryNextDiagnostic
            | UiId::DatabaseQueryResultTab(_)
            | UiId::DatabaseQueryHistoryEntry(_)
            | UiId::DatabaseQueryResultBody
            | UiId::DatabaseQueryResultResize
            | UiId::DatabaseQueryColumnResize(_)
            | UiId::DatabaseQueryReviewBackdrop
            | UiId::DatabaseQueryReviewBody
            | UiId::DatabaseQueryReviewMessagesBody
            | UiId::DatabaseQueryReviewMessagesScrollY
            | UiId::DatabaseQueryCommit
            | UiId::DatabaseQueryRollback => {
                self.handle_database_ui_click(id, same_click_target);
            }
            UiId::ApiImportAdd
            | UiId::ApiImportFile
            | UiId::ApiImportUrl
            | UiId::ApiImportUrlInput
            | UiId::ApiImportUrlConfirm
            | UiId::ApiSpecSelect(_)
            | UiId::ApiSpecOpen(_)
            | UiId::ApiSpecRefresh(_)
            | UiId::ApiSpecRemove(_)
            | UiId::ApiSpecRemoveConfirm
            | UiId::ApiSpecRemoveCancel
            | UiId::ApiAuthRoot
            | UiId::ApiRoutesRoot
            | UiId::ApiRouteFilterInput
            | UiId::ApiRouteFilterClear
            | UiId::ApiRouteTag(_)
            | UiId::ApiRouteRow(_)
            | UiId::ApiRoutePathText(_)
            | UiId::ApiRouteSummaryText(_)
            | UiId::ApiRouteDescriptionText(_)
            | UiId::ApiServerSelect(_)
            | UiId::ApiAuthValue(_)
            | UiId::ApiAuthRefreshToken(_)
            | UiId::ApiAuthUsername(_)
            | UiId::ApiAuthPassword(_)
            | UiId::ApiAuthAccessSave(_)
            | UiId::ApiAuthAccessClear(_)
            | UiId::ApiAuthRefreshSave(_)
            | UiId::ApiAuthRefreshClear(_)
            | UiId::ApiAuthSave(_)
            | UiId::ApiAuthClear(_)
            | UiId::ApiTryRequest
            | UiId::ApiPathParamInput(_, _)
            | UiId::ApiQueryParamInput(_, _)
            | UiId::ApiPathParamAllowedValue(_, _, _)
            | UiId::ApiQueryParamAllowedValue(_, _, _)
            | UiId::ApiBodyInput(_)
            | UiId::ApiInputExampleTab(_)
            | UiId::ApiInputSchemaTab(_)
            | UiId::ApiInputSchemaMenu(_)
            | UiId::ApiInputSchemaMenuItem(_, _)
            | UiId::ApiInputSchemaBody(_)
            | UiId::ApiInputSchemaFold(_, _)
            | UiId::ApiBodyScrollY(_)
            | UiId::ApiBodyScrollX(_)
            | UiId::ApiBodyFieldInput(_, _)
            | UiId::ApiBodyAllowedValue(_, _, _)
            | UiId::ApiBodyFilePick(_, _)
            | UiId::ApiOutputExampleTab(_)
            | UiId::ApiOutputSchemaTab(_)
            | UiId::ApiOutputStatusTab(_, _)
            | UiId::ApiOutputSchemaMenu(_)
            | UiId::ApiOutputSchemaMenuScrollY(_)
            | UiId::ApiOutputSchemaMenuItem(_, _)
            | UiId::ApiOutputSchemaBody(_)
            | UiId::ApiOutputSchemaFold(_, _)
            | UiId::ApiOutputScrollY(_)
            | UiId::ApiOutputScrollX(_)
            | UiId::ApiResponseBodyTab(_)
            | UiId::ApiResponseHeadersTab(_)
            | UiId::ApiResponseCurlTab(_)
            | UiId::ApiResponseBody(_)
            | UiId::ApiResponseScrollY(_)
            | UiId::ApiResponseScrollX(_)
            | UiId::ApiResponseUseAccessToken(_, _)
            | UiId::ApiResponseSaveRefreshToken(_, _)
            | UiId::ApiMockServerToggle
            | UiId::ApiMockServerDetails
            | UiId::ApiMockServerCopyUrl
            | UiId::ApiMockServerDetailsClose
            | UiId::ApiMockServerLogArea
            | UiId::ApiMockServerLogScrollY
            | UiId::ApiMockModeSelect
            | UiId::ApiMockProxyBaseInput
            | UiId::ApiMockGuideOpen
            | UiId::ApiMockGuideClose
            | UiId::ApiMockGuideBody
            | UiId::ApiMockGuideScrollY
            | UiId::ApiMockPythonManage
            | UiId::ApiMockPythonManageClose
            | UiId::ApiMockPythonModeToggle
            | UiId::ApiMockPythonCheckRuntime
            | UiId::ApiMockPythonPrepareVersion
            | UiId::ApiMockPythonPickUvPath
            | UiId::ApiMockPythonPickCustomPath
            | UiId::ApiMockPythonVersionOption(_)
            | UiId::ApiMockPythonVersionsScrollY
            | UiId::ApiMockPythonInstallLogScrollY
            | UiId::ApiMockPythonUvPathInput
            | UiId::ApiMockPythonVersionInput
            | UiId::ApiMockPythonCustomPathInput
            | UiId::ApiMockExportOpenApi
            | UiId::ApiMockRouteEnable(_)
            | UiId::ApiMockRouteDetailsToggle(_)
            | UiId::ApiMockRoutePythonToggle(_)
            | UiId::ApiMockRouteReset(_)
            | UiId::ApiMockRouteResetConfirm
            | UiId::ApiMockRouteResetCancel
            | UiId::ApiMockContractPathToggle(_)
            | UiId::ApiMockContractQueryToggle(_)
            | UiId::ApiMockContractBodyToggle(_)
            | UiId::ApiMockContractPathFieldToggle(_, _)
            | UiId::ApiMockContractQueryFieldToggle(_, _)
            | UiId::ApiMockContractBodyFieldToggle(_, _)
            | UiId::ApiMockContractFieldRequired(_, _, _)
            | UiId::ApiMockContractFieldNullable(_, _, _)
            | UiId::ApiMockContractFieldRemove(_, _, _)
            | UiId::ApiMockContractFieldRemoveConfirm
            | UiId::ApiMockContractFieldRemoveCancel
            | UiId::ApiMockContractFieldPropInput(_, _, _, _)
            | UiId::ApiMockContractFieldAddConstraint(_, _, _)
            | UiId::ApiMockContractFieldAddConstraintOption(_, _, _, _)
            | UiId::ApiMockStaticResponseInput(_)
            | UiId::ApiMockStaticResponseScrollY(_)
            | UiId::ApiMockStaticResponseScrollX(_)
            | UiId::ApiMockCombinedPython(_)
            | UiId::ApiMockContractInput(_)
            | UiId::ApiMockSignatureInput(_)
            | UiId::ApiMockPreludeInput(_)
            | UiId::ApiMockBodyInput(_)
            | UiId::ApiMockContractReset(_)
            | UiId::ApiMockPreludeReset(_)
            | UiId::ApiMockBodyReset(_)
            | UiId::ApiMockAddInputField(_)
            | UiId::ApiMockAddOutputField(_)
            | UiId::ApiMockAddManualRoute
            | UiId::ApiMockManualRouteOpen(_)
            | UiId::ApiMockManualRouteMethod(_)
            | UiId::ApiMockManualRoutePath(_)
            | UiId::ApiMockManualRouteRemove(_)
            | UiId::ApiTabBody => {
                self.handle_api_client_click(id, same_click_target);
            }
            UiId::HoverPopupScroll
            | UiId::StatusBar
            | UiId::SearchPanelBody
            | UiId::ProjectSearchPanelBody
            | UiId::ProjectSearchHelpPopup
            | UiId::InlineGitPanelBody
            | UiId::GitDiffPanelBody => {}
            _ => {
                // Domain handlers catch disjoint UiId sets, so the first one
                // that is not `NotMine` is the arm the single match would pick.
                let _ = self.handle_settings_ui_click(id, same_click_target) != UiClickFlow::NotMine
                    || self.handle_lsp_ui_click(id, same_click_target) != UiClickFlow::NotMine
                    || self.handle_git_ui_click(id, same_click_target) != UiClickFlow::NotMine
                    || self.handle_panels_ui_click(id, same_click_target) != UiClickFlow::NotMine
                    || self.handle_editor_ui_click(id, same_click_target) != UiClickFlow::NotMine;
            }
        }
    }
}

/// Result of a domain click handler split out of `App::handle_ui_click`.
/// `Return` mirrors an early `return` from the original single match; nothing
/// follows the dispatch, so it is treated like `Handled`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UiClickFlow {
    NotMine,
    Handled,
    Return,
}
