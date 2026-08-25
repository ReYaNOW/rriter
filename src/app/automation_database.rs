use std::ffi::OsString;
use std::time::Duration;

use crate::app::database::{
    DATABASE_GRID_ROW_HEIGHT, DatabaseCellPosition, DatabaseConnectionConfig,
    DatabaseConnectionId, DatabaseConnectionNode, DatabaseConnectionStatus, DatabasePendingJobKind,
    DatabaseQueryMode, DatabaseQueryTabState, DatabaseSecretBundle, DatabaseTableModal,
    DatabaseTableTabState, PostgresTlsMode, SshHostKeyPolicy, database_grid_max_scroll,
    database_query_scroll_limits,
};
use crate::app::{App, EditorTabKind};
use crate::ui_system::UiId;

const PGO_CONNECTION_ID: DatabaseConnectionId = DatabaseConnectionId(9_001_001);
const PGO_DISPLAY_NAME: &str = "PGO PostgreSQL";
const PGO_DATABASE_NAME: &str = "rriter_pgo";
const PGO_DATABASE_USER: &str = "rriter_pgo";
const PGO_TABLE_NAME: &str = "pgo_items";
const PGO_EDITED_NAME: &str = "pgo-item-001-pgo-edited";
const PGO_QUERY_MARKER: &str = "pgo-item-001";
const PGO_EXPLAIN_MARKER: &str = "Index Scan using pgo_items_pkey";
const PGO_QUERY: &str =
    "SELECT id, name, active\nFROM public.pgo_items\nORDER BY id\nLIMIT 64;";

const ENV_HOST: &str = "RRITER_PGO_DATABASE_HOST";
const ENV_PORT: &str = "RRITER_PGO_DATABASE_PORT";
const ENV_NAME: &str = "RRITER_PGO_DATABASE_NAME";
const ENV_USER: &str = "RRITER_PGO_DATABASE_USER";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DatabaseAutomationStep {
    SetupConnection,
    LoadCatalog,
    WaitCatalog,
    LoadTables,
    WaitTables,
    LoadDdl,
    WaitDdl,
    DismissDdl,
    OpenTable,
    WaitTable,
    ScrollTableTimed { duration_secs: u16 },
    SortTable,
    WaitTableReload,
    EditTableCell,
    SaveTableChanges,
    WaitTableReview,
    RollbackTableTransaction,
    WaitTableTransactionFinished,
    OpenQuery,
    WaitQueryCompletion,
    SetQueryText,
    RunQuery,
    WaitQueryResult,
    ScrollQueryResultTimed { duration_secs: u16 },
    RunExplain,
    WaitExplain,
    AssertIdle,
}

impl DatabaseAutomationStep {
    pub(super) fn name(self) -> String {
        match self {
            Self::SetupConnection => "database-setup-pgo-connection".to_string(),
            Self::LoadCatalog => "database-load-catalog".to_string(),
            Self::WaitCatalog => "database-wait-catalog".to_string(),
            Self::LoadTables => "database-load-public-tables".to_string(),
            Self::WaitTables => "database-wait-public-tables".to_string(),
            Self::LoadDdl => "database-load-ddl".to_string(),
            Self::WaitDdl => "database-wait-ddl".to_string(),
            Self::DismissDdl => "database-dismiss-ddl".to_string(),
            Self::OpenTable => "database-open-table".to_string(),
            Self::WaitTable => "database-wait-table-data".to_string(),
            Self::ScrollTableTimed { duration_secs } => {
                format!("database-scroll-table-timed:{duration_secs}s")
            }
            Self::SortTable => "database-sort-table".to_string(),
            Self::WaitTableReload => "database-wait-table-reload".to_string(),
            Self::EditTableCell => "database-edit-table-cell".to_string(),
            Self::SaveTableChanges => "database-begin-table-save".to_string(),
            Self::WaitTableReview => "database-wait-table-review".to_string(),
            Self::RollbackTableTransaction => "database-rollback-table-transaction".to_string(),
            Self::WaitTableTransactionFinished => {
                "database-wait-table-transaction-finished".to_string()
            }
            Self::OpenQuery => "database-open-query".to_string(),
            Self::WaitQueryCompletion => "database-wait-query-completion".to_string(),
            Self::SetQueryText => "database-set-query-text".to_string(),
            Self::RunQuery => "database-run-user-query".to_string(),
            Self::WaitQueryResult => "database-wait-user-query-result".to_string(),
            Self::ScrollQueryResultTimed { duration_secs } => {
                format!("database-scroll-query-result-timed:{duration_secs}s")
            }
            Self::RunExplain => "database-run-explain".to_string(),
            Self::WaitExplain => "database-wait-explain".to_string(),
            Self::AssertIdle => "database-assert-idle".to_string(),
        }
    }

    pub(super) fn timeout(self) -> Duration {
        match self {
            Self::WaitCatalog
            | Self::WaitTables
            | Self::WaitDdl
            | Self::WaitTable
            | Self::WaitTableReload
            | Self::WaitTableReview
            | Self::WaitTableTransactionFinished
            | Self::WaitQueryCompletion
            | Self::WaitQueryResult
            | Self::WaitExplain => Duration::from_secs(45),
            Self::ScrollTableTimed { duration_secs }
            | Self::ScrollQueryResultTimed { duration_secs } => {
                Duration::from_secs(u64::from(duration_secs) + 5)
            }
            _ => Duration::from_secs(12),
        }
    }

    pub(super) fn is_timed_scroll(self) -> bool {
        matches!(
            self,
            Self::ScrollTableTimed { .. } | Self::ScrollQueryResultTimed { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DatabaseStepResult {
    Pending,
    Done,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PgoDatabaseEndpoint {
    host: String,
    port: u16,
    database_name: String,
    username: String,
}

fn environment_value(
    lookup: &mut impl FnMut(&str) -> Option<OsString>,
    name: &str,
) -> Result<String, String> {
    let value = lookup(name).ok_or_else(|| format!("missing PGO database environment variable {name}"))?;
    value
        .into_string()
        .map_err(|_| format!("PGO database environment variable {name} is not valid UTF-8"))
        .and_then(|value| {
            if value.trim().is_empty() {
                Err(format!("PGO database environment variable {name} is empty"))
            } else {
                Ok(value)
            }
        })
}

fn endpoint_from_lookup(
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) -> Result<PgoDatabaseEndpoint, String> {
    let host = environment_value(&mut lookup, ENV_HOST)?;
    if host != "127.0.0.1" {
        return Err(format!(
            "PGO database host must be loopback 127.0.0.1, got {host:?}"
        ));
    }
    let port_text = environment_value(&mut lookup, ENV_PORT)?;
    let port = port_text.parse::<u16>().map_err(|_| {
        format!("invalid PGO database port {port_text:?} in {ENV_PORT}; expected 1..=65535")
    })?;
    if port == 0 {
        return Err(format!(
            "invalid PGO database port {port_text:?} in {ENV_PORT}; expected 1..=65535"
        ));
    }
    let database_name = environment_value(&mut lookup, ENV_NAME)?;
    let username = environment_value(&mut lookup, ENV_USER)?;
    if database_name != PGO_DATABASE_NAME {
        return Err(format!(
            "PGO database name mismatch: expected {PGO_DATABASE_NAME:?}, got {database_name:?}"
        ));
    }
    if username != PGO_DATABASE_USER {
        return Err(format!(
            "PGO database user mismatch: expected {PGO_DATABASE_USER:?}, got {username:?}"
        ));
    }
    Ok(PgoDatabaseEndpoint {
        host,
        port,
        database_name,
        username,
    })
}

fn pgo_connection_config(endpoint: &PgoDatabaseEndpoint) -> DatabaseConnectionConfig {
    DatabaseConnectionConfig {
        id: PGO_CONNECTION_ID,
        display_name: PGO_DISPLAY_NAME.to_string(),
        color: Default::default(),
        host: endpoint.host.clone(),
        port: endpoint.port,
        username: endpoint.username.clone(),
        maintenance_database: endpoint.database_name.clone(),
        tls_mode: PostgresTlsMode::Disable,
        remember_postgres_password: false,
        ssh: None,
    }
}

fn global_failure(app: &App) -> Option<String> {
    app.ide_panel
        .database
        .global_error
        .as_ref()
        .map(|error| format!("database runtime error: {error}; {}", diagnostics(app)))
}

fn pgo_connection(app: &App) -> Result<&DatabaseConnectionNode, String> {
    app.ide_panel
        .database
        .connection(PGO_CONNECTION_ID)
        .ok_or_else(|| format!("PGO database connection is missing; {}", diagnostics(app)))
}

fn pgo_database_index(app: &App) -> Result<usize, String> {
    pgo_connection(app)?
        .databases
        .iter()
        .position(|database| database.name == PGO_DATABASE_NAME)
        .ok_or_else(|| format!("PGO database catalog is missing {PGO_DATABASE_NAME}; {}", diagnostics(app)))
}

fn active_pgo_table_id(app: &App) -> Result<crate::app::database::DatabaseTabId, String> {
    let tab_id = app
        .active_database_table_tab_id()
        .ok_or_else(|| format!("PGO database table tab is not active; {}", diagnostics(app)))?;
    let Some((meta, _)) = app.database_table_meta_state(tab_id) else {
        return Err(format!("active PGO database table state is missing; {}", diagnostics(app)));
    };
    if meta.connection_id != PGO_CONNECTION_ID
        || meta.database_name != PGO_DATABASE_NAME
        || meta.table_name != PGO_TABLE_NAME
    {
        return Err(format!("unexpected active database table; {}", diagnostics(app)));
    }
    Ok(tab_id)
}

fn query_wait_state(state: &DatabaseQueryTabState, marker: &str) -> Result<bool, String> {
    if let Some(error) = state.error.as_ref() {
        return Err(format!("database query failed: {error}"));
    }
    if state.running {
        return Ok(false);
    }
    if state.review.is_some() {
        return Err("read-only PGO database query unexpectedly requires transaction review".to_string());
    }
    if state.results.is_empty() {
        return Ok(false);
    }
    let marker_found = state.results.iter().flat_map(|result| &result.rows).any(|row| {
        row.iter()
            .any(|cell| cell.value.as_deref().is_some_and(|value| value.contains(marker)))
    });
    if !marker_found {
        return Err(format!(
            "database query completed without expected marker {marker:?}; results={} rows={}",
            state.results.len(),
            state.results.iter().map(|result| result.rows.len()).sum::<usize>()
        ));
    }
    Ok(true)
}

fn table_wait_state(state: &DatabaseTableTabState, require_sorted: bool) -> Result<bool, String> {
    if let Some(error) = state.error.as_ref() {
        return Err(format!("database table failed: {error}"));
    }
    if let Some(error) = state.grid.count_error.as_ref() {
        return Err(format!("database table count failed: {error}"));
    }
    if state.loading || state.grid.loading_count || state.grid.loading_chunk {
        return Ok(false);
    }
    let Some(metadata) = state.metadata.as_ref() else {
        return Ok(false);
    };
    if metadata.columns.iter().map(|column| column.name.as_str()).collect::<Vec<_>>()
        != ["id", "name", "active"]
    {
        return Err(format!(
            "unexpected PGO table columns: {:?}",
            metadata.columns.iter().map(|column| &column.name).collect::<Vec<_>>()
        ));
    }
    if !metadata.editable {
        return Err(format!(
            "PGO table unexpectedly read-only: {:?}",
            metadata.read_only_reason
        ));
    }
    if state.grid.count.unwrap_or(0) == 0 || state.grid.row(0).is_none() {
        return Ok(false);
    }
    if require_sorted
        && (state.grid.view.sorted_column.as_deref() != Some("id")
            || state.grid.view.order_by != "\"id\" ASC")
    {
        return Ok(false);
    }
    Ok(true)
}

pub(super) fn run_step(app: &mut App, step: DatabaseAutomationStep) -> DatabaseStepResult {
    use DatabaseAutomationStep as S;
    if !matches!(step, S::DismissDdl | S::AssertIdle) {
        if let Some(error) = global_failure(app) {
            return DatabaseStepResult::Failed(error);
        }
    }
    match step {
        S::SetupConnection => setup_connection(app),
        S::LoadCatalog => load_catalog(app),
        S::WaitCatalog => wait_catalog(app),
        S::LoadTables => load_tables(app),
        S::WaitTables => wait_tables(app),
        S::LoadDdl => load_ddl(app),
        S::WaitDdl => wait_ddl(app),
        S::DismissDdl => dismiss_ddl(app),
        S::OpenTable => open_table(app),
        S::WaitTable => wait_table(app, false),
        S::SortTable => sort_table(app),
        S::WaitTableReload => wait_table(app, true),
        S::EditTableCell => edit_table_cell(app),
        S::SaveTableChanges => save_table_changes(app),
        S::WaitTableReview => wait_table_review(app),
        S::RollbackTableTransaction => rollback_table_transaction(app),
        S::WaitTableTransactionFinished => wait_table_transaction_finished(app),
        S::OpenQuery => open_query(app),
        S::WaitQueryCompletion => wait_query_completion(app),
        S::SetQueryText => set_query_text(app),
        S::RunQuery => run_query(app, DatabaseQueryMode::Run),
        S::WaitQueryResult => wait_query_result(app, PGO_QUERY_MARKER),
        S::RunExplain => run_query(app, DatabaseQueryMode::Explain),
        S::WaitExplain => wait_query_result(app, PGO_EXPLAIN_MARKER),
        S::AssertIdle => assert_idle(app),
        S::ScrollTableTimed { .. } | S::ScrollQueryResultTimed { .. } => {
            DatabaseStepResult::Failed("timed database scroll dispatched through wrong path".to_string())
        }
    }
}

fn setup_connection(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    let endpoint = match endpoint_from_lookup(|name| std::env::var_os(name)) {
        Ok(endpoint) => endpoint,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    let config = pgo_connection_config(&endpoint);
    if let Err(error) = config.validate() {
        return DatabaseStepResult::Failed(format!("invalid PGO database connection: {error}"));
    }
    if let Some(existing) = app.ide_panel.database.connection(PGO_CONNECTION_ID) {
        if existing.config != config {
            return DatabaseStepResult::Failed(format!(
                "PGO database connection id collision for {:?}",
                existing.config.display_name
            ));
        }
    } else {
        app.ide_panel
            .database
            .connections
            .push(DatabaseConnectionNode::new(config));
    }
    app.ide_panel.database.next_connection_id = app
        .ide_panel
        .database
        .next_connection_id
        .max(PGO_CONNECTION_ID.0.saturating_add(1));
    app.ide_panel.database.selected_connection = Some(PGO_CONNECTION_ID);
    app.ide_panel.database.selected_database = None;
    app.ide_panel.database.selected_table = None;
    app.ide_panel.database.session_secrets.insert(
        PGO_CONNECTION_ID,
        DatabaseSecretBundle::empty(),
    );
    app.ide_panel.database.global_error = None;
    app.ide_panel.database.notice = None;
    app.ide_panel.database.sync_persisted_connections();
    DatabaseStepResult::Done
}

fn load_catalog(app: &mut App) -> DatabaseStepResult {
    let connection = match pgo_connection(app) {
        Ok(connection) => connection,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    if connection.databases_loaded {
        return DatabaseStepResult::Done;
    }
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    app.toggle_database_connection(PGO_CONNECTION_ID);
    if app.ide_panel.database.pending_job.as_ref().is_some_and(|job| {
        job.connection_id == PGO_CONNECTION_ID && job.kind == DatabasePendingJobKind::LoadDatabases
    }) {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "loading PGO database catalog did not start; {}",
            diagnostics(app)
        ))
    }
}

fn wait_catalog(app: &App) -> DatabaseStepResult {
    let connection = match pgo_connection(app) {
        Ok(connection) => connection,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    if connection.status == DatabaseConnectionStatus::Error {
        return DatabaseStepResult::Failed(format!(
            "PGO database connection failed: {:?}; {}",
            connection.status_message,
            diagnostics(app)
        ));
    }
    if connection.loading || !connection.databases_loaded {
        return DatabaseStepResult::Pending;
    }
    if connection
        .databases
        .iter()
        .any(|database| database.name == PGO_DATABASE_NAME)
    {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "catalog loaded without database {PGO_DATABASE_NAME}; {}",
            diagnostics(app)
        ))
    }
}

fn load_tables(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    let index = match pgo_database_index(app) {
        Ok(index) => index,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    if pgo_connection(app)
        .ok()
        .and_then(|connection| connection.databases.get(index))
        .is_some_and(|database| database.tables_loaded)
    {
        return DatabaseStepResult::Done;
    }
    app.toggle_database_node(PGO_CONNECTION_ID, index);
    if app.ide_panel.database.pending_job.as_ref().is_some_and(|job| {
        job.connection_id == PGO_CONNECTION_ID && job.kind == DatabasePendingJobKind::LoadTables
    }) {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "loading PGO public tables did not start; {}",
            diagnostics(app)
        ))
    }
}

fn wait_tables(app: &App) -> DatabaseStepResult {
    let index = match pgo_database_index(app) {
        Ok(index) => index,
        Err(_) => return DatabaseStepResult::Pending,
    };
    let Some(database) = pgo_connection(app)
        .ok()
        .and_then(|connection| connection.databases.get(index))
    else {
        return DatabaseStepResult::Pending;
    };
    if let Some(error) = database.error.as_ref() {
        return DatabaseStepResult::Failed(format!(
            "loading PGO public tables failed: {error}; {}",
            diagnostics(app)
        ));
    }
    if database.loading || !database.tables_loaded {
        return DatabaseStepResult::Pending;
    }
    if database.tables.iter().any(|table| table.name == PGO_TABLE_NAME) {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "public tables loaded without {PGO_TABLE_NAME}; {}",
            diagnostics(app)
        ))
    }
}

fn load_ddl(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    *app.ide_panel.database.ddl_hover.borrow_mut() = None;
    app.load_database_ddl(
        PGO_CONNECTION_ID,
        PGO_DATABASE_NAME,
        PGO_TABLE_NAME,
        SshHostKeyPolicy::Strict,
    );
    if app.ide_panel.database.pending_job.as_ref().is_some_and(|job| {
        job.connection_id == PGO_CONNECTION_ID && job.kind == DatabasePendingJobKind::LoadDdl
    }) {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "loading PGO table DDL did not start; {}",
            diagnostics(app)
        ))
    }
}

fn wait_ddl(app: &App) -> DatabaseStepResult {
    let hover = app.ide_panel.database.ddl_hover.borrow();
    if let Some(ddl) = hover.as_ref()
        && ddl.connection_id == PGO_CONNECTION_ID
        && ddl.database_name == PGO_DATABASE_NAME
        && ddl.table_name == PGO_TABLE_NAME
        && ddl.popup.text.contains("CREATE TABLE")
        && ddl.popup.text.contains(PGO_TABLE_NAME)
    {
        return DatabaseStepResult::Done;
    }
    if app.ide_panel.database.pending_job.as_ref().is_some_and(|job| {
        job.connection_id == PGO_CONNECTION_ID && job.kind == DatabasePendingJobKind::LoadDdl
    }) {
        DatabaseStepResult::Pending
    } else {
        DatabaseStepResult::Failed(format!(
            "PGO DDL job finished without reconstructed DDL; {}",
            diagnostics(app)
        ))
    }
}

fn dismiss_pgo_ddl_state(
    panel: &crate::app::database::DatabasePanelState,
) -> Result<(), String> {
    {
        let hover = panel.ddl_hover.borrow();
        let Some(ddl) = hover.as_ref() else {
            return Err("expected PGO DDL popup is missing".to_string());
        };
        if ddl.connection_id != PGO_CONNECTION_ID
            || ddl.database_name != PGO_DATABASE_NAME
            || ddl.table_name != PGO_TABLE_NAME
        {
            return Err(format!(
                "refusing to dismiss unexpected DDL popup: connection={} database={:?} table={:?}",
                ddl.connection_id.0, ddl.database_name, ddl.table_name
            ));
        }
    }
    panel.ddl_hover.borrow_mut().take();
    Ok(())
}

fn dismiss_ddl(app: &mut App) -> DatabaseStepResult {
    if let Err(error) = dismiss_pgo_ddl_state(&app.ide_panel.database) {
        return DatabaseStepResult::Failed(format!("{error}; {}", diagnostics(app)));
    }
    if let Some(window) = app.window.as_ref() {
        window.request_redraw();
    }
    DatabaseStepResult::Done
}

fn database_query_review_present(app: &App) -> bool {
    app.tabs.iter().any(|tab| {
        matches!(
            &tab.kind,
            EditorTabKind::DatabaseQuery(_, state) if state.review.is_some()
        )
    })
}

fn database_transient_issues(
    panel: &crate::app::database::DatabasePanelState,
    query_review_present: bool,
) -> Vec<&'static str> {
    let mut issues = Vec::new();
    if panel.pending_job.is_some() {
        issues.push("pending_job");
    }
    if panel.active_command.is_some() {
        issues.push("active_command");
    }
    if !panel.queued_commands.is_empty() {
        issues.push("queued_commands");
    }
    if panel.host_key_retry.is_some() {
        issues.push("host_key_retry");
    }
    if panel.table_modal.is_some() {
        issues.push("table_modal");
    }
    if panel.ddl_hover.borrow().is_some() {
        issues.push("ddl_hover");
    }
    if panel.context_menu.is_some() {
        issues.push("context_menu");
    }
    if panel.dialog.is_some() {
        issues.push("dialog");
    }
    if panel.delete_prompt.is_some() {
        issues.push("delete_prompt");
    }
    if panel.host_key_prompt.is_some() {
        issues.push("host_key_prompt");
    }
    if query_review_present {
        issues.push("query_review");
    }
    if panel.global_error.is_some() {
        issues.push("global_error");
    }
    issues
}

fn database_idle_check(
    panel: &crate::app::database::DatabasePanelState,
    query_review_present: bool,
) -> Result<(), String> {
    let issues = database_transient_issues(panel, query_review_present);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "database workload left transient state: {}",
            issues.join(",")
        ))
    }
}

fn assert_idle(app: &App) -> DatabaseStepResult {
    match database_idle_check(
        &app.ide_panel.database,
        database_query_review_present(app),
    ) {
        Ok(()) => DatabaseStepResult::Done,
        Err(error) => DatabaseStepResult::Failed(format!("{error}; {}", diagnostics(app))),
    }
}

fn open_table(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    app.open_database_table_tab(PGO_CONNECTION_ID, PGO_DATABASE_NAME, PGO_TABLE_NAME);
    match active_pgo_table_id(app) {
        Ok(_) => DatabaseStepResult::Done,
        Err(error) => DatabaseStepResult::Failed(error),
    }
}

fn wait_table(app: &App, require_sorted: bool) -> DatabaseStepResult {
    let tab_id = match active_pgo_table_id(app) {
        Ok(tab_id) => tab_id,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    let Some((_, state)) = app.database_table_meta_state(tab_id) else {
        return DatabaseStepResult::Pending;
    };
    match table_wait_state(state, require_sorted) {
        Ok(true) => DatabaseStepResult::Done,
        Ok(false) => DatabaseStepResult::Pending,
        Err(error) => DatabaseStepResult::Failed(format!("{error}; {}", diagnostics(app))),
    }
}

fn sort_table(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    let tab_id = match active_pgo_table_id(app) {
        Ok(tab_id) => tab_id,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    app.cycle_database_table_sort(tab_id, 0);
    if app.ide_panel.database.pending_job.is_some() {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "sorting PGO table did not trigger a reload; {}",
            diagnostics(app)
        ))
    }
}

fn edit_table_cell(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    let tab_id = match active_pgo_table_id(app) {
        Ok(tab_id) => tab_id,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    let column_index = app
        .database_table_meta_state(tab_id)
        .and_then(|(_, state)| state.metadata.as_ref())
        .and_then(|metadata| metadata.columns.iter().position(|column| column.name == "name"));
    let Some(column_index) = column_index else {
        return DatabaseStepResult::Failed(format!(
            "PGO table does not expose editable name column; {}",
            diagnostics(app)
        ));
    };
    app.start_database_table_cell_edit(
        tab_id,
        DatabaseCellPosition {
            row: 0,
            column: column_index,
        },
    );
    let editor_ready = app
        .database_table_meta_state_mut(tab_id)
        .and_then(|(_, state)| state.grid.cell_editor.as_mut())
        .map(|editor| {
            editor.input.select_all();
            editor.error = None;
        })
        .is_some();
    if !editor_ready {
        return DatabaseStepResult::Failed(format!(
            "PGO table cell editor did not open; {}",
            diagnostics(app)
        ));
    }
    app.handle_main_ime_commit(PGO_EDITED_NAME);
    app.commit_database_table_cell_editor(tab_id, false);
    let updated = app.database_table_meta_state(tab_id).is_some_and(|(_, state)| {
        state.grid.dirty()
            && state.grid.row(0).and_then(|row| row.cells.get(column_index)).is_some_and(|cell| {
                cell.value.copy_text() == PGO_EDITED_NAME
            })
    });
    if updated {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "PGO table edit did not produce a dirty updated cell; {}",
            diagnostics(app)
        ))
    }
}

fn save_table_changes(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    let tab_id = match active_pgo_table_id(app) {
        Ok(tab_id) => tab_id,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    app.save_database_table_changes(tab_id, false);
    if app.ide_panel.database.pending_job.as_ref().is_some_and(|job| {
        job.kind == DatabasePendingJobKind::BeginTableSave && job.connection_id == PGO_CONNECTION_ID
    }) {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "PGO table save did not start a transaction; {}",
            diagnostics(app)
        ))
    }
}

fn wait_table_review(app: &App) -> DatabaseStepResult {
    let tab_id = match active_pgo_table_id(app) {
        Ok(tab_id) => tab_id,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    if let Some((_, state)) = app.database_table_meta_state(tab_id)
        && let Some(error) = state.error.as_ref()
    {
        return DatabaseStepResult::Failed(format!(
            "PGO table transaction failed: {error}; {}",
            diagnostics(app)
        ));
    }
    match app.ide_panel.database.table_modal.as_ref() {
        Some(DatabaseTableModal::Review {
            tab_id: review_tab,
            state,
            ..
        }) if *review_tab == tab_id => {
            if state.summary.updated_rows != 1 || state.summary.changed_cells == 0 {
                DatabaseStepResult::Failed(format!(
                    "unexpected PGO table review summary: updated={} changed_cells={}; {}",
                    state.summary.updated_rows,
                    state.summary.changed_cells,
                    diagnostics(app)
                ))
            } else {
                DatabaseStepResult::Done
            }
        }
        _ if app.ide_panel.database.pending_job.is_some() => DatabaseStepResult::Pending,
        _ => DatabaseStepResult::Pending,
    }
}

fn rollback_table_transaction(app: &mut App) -> DatabaseStepResult {
    let tab_id = match active_pgo_table_id(app) {
        Ok(tab_id) => tab_id,
        Err(error) => return DatabaseStepResult::Failed(error),
    };
    if !matches!(
        app.ide_panel.database.table_modal.as_ref(),
        Some(DatabaseTableModal::Review { tab_id: review_tab, .. }) if *review_tab == tab_id
    ) {
        return DatabaseStepResult::Failed(format!(
            "PGO table transaction review is not active; {}",
            diagnostics(app)
        ));
    }
    app.rollback_database_table_transaction();
    if app.ide_panel.database.pending_job.as_ref().is_some_and(|job| {
        job.kind == DatabasePendingJobKind::RollbackTransaction
            && job.connection_id == PGO_CONNECTION_ID
    }) {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "PGO table rollback did not start; {}",
            diagnostics(app)
        ))
    }
}

fn wait_table_transaction_finished(app: &App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    if app.ide_panel.database.table_modal.is_some() {
        return DatabaseStepResult::Pending;
    }
    if app
        .ide_panel
        .database
        .notice
        .as_deref()
        .is_some_and(|notice| notice.contains("Транзакция отменена"))
    {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "PGO table rollback finished without rollback notice; {}",
            diagnostics(app)
        ))
    }
}

fn open_query(app: &mut App) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    app.open_database_query_tab(PGO_CONNECTION_ID, PGO_DATABASE_NAME, true, None);
    match app.active_database_query_meta_state() {
        Some((meta, _))
            if meta.connection_id == PGO_CONNECTION_ID && meta.database_name == PGO_DATABASE_NAME =>
        {
            DatabaseStepResult::Done
        }
        _ => DatabaseStepResult::Failed(format!(
            "PGO database query tab did not open; {}",
            diagnostics(app)
        )),
    }
}

fn wait_query_completion(app: &App) -> DatabaseStepResult {
    let Some((meta, state)) = app.active_database_query_meta_state() else {
        return DatabaseStepResult::Failed(format!(
            "PGO database query tab is not active; {}",
            diagnostics(app)
        ));
    };
    if meta.connection_id != PGO_CONNECTION_ID || meta.database_name != PGO_DATABASE_NAME {
        return DatabaseStepResult::Failed(format!(
            "unexpected active database query; {}",
            diagnostics(app)
        ));
    }
    if let Some(error) = state.error.as_ref() {
        return DatabaseStepResult::Failed(format!("query completion failed: {error}; {}", diagnostics(app)));
    }
    if !state.completion_loaded {
        return DatabaseStepResult::Pending;
    }
    let expected_columns = ["id", "name", "active"];
    if !expected_columns.iter().all(|expected| {
        state
            .completion
            .columns
            .iter()
            .any(|column| column.column_name == *expected)
    }) {
        return DatabaseStepResult::Failed(format!(
            "PGO query completion metadata is missing fixture columns; {}",
            diagnostics(app)
        ));
    }
    DatabaseStepResult::Done
}

fn set_query_text(app: &mut App) -> DatabaseStepResult {
    let Some((meta, _)) = app.active_database_query_meta_state() else {
        return DatabaseStepResult::Failed(format!(
            "PGO query editor is not active; {}",
            diagnostics(app)
        ));
    };
    if meta.connection_id != PGO_CONNECTION_ID {
        return DatabaseStepResult::Failed(format!("unexpected database query editor; {}", diagnostics(app)));
    }
    app.editor.select_all();
    app.handle_main_ime_commit(PGO_QUERY);
    if app.editor.get_full_text() == PGO_QUERY {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "PGO query text was not applied through editor input; {}",
            diagnostics(app)
        ))
    }
}

fn run_query(app: &mut App, mode: DatabaseQueryMode) -> DatabaseStepResult {
    if app.ide_panel.database.pending_job.is_some() {
        return DatabaseStepResult::Pending;
    }
    app.run_active_database_query(mode);
    if app.ide_panel.database.pending_job.as_ref().is_some_and(|job| {
        job.kind == DatabasePendingJobKind::RunUserSql && job.connection_id == PGO_CONNECTION_ID
    }) {
        DatabaseStepResult::Done
    } else {
        DatabaseStepResult::Failed(format!(
            "PGO database query did not enter runtime; mode={mode:?}; {}",
            diagnostics(app)
        ))
    }
}

fn wait_query_result(app: &App, marker: &str) -> DatabaseStepResult {
    let Some((meta, state)) = app.active_database_query_meta_state() else {
        return DatabaseStepResult::Failed(format!(
            "PGO query result tab is not active; {}",
            diagnostics(app)
        ));
    };
    if meta.connection_id != PGO_CONNECTION_ID {
        return DatabaseStepResult::Failed(format!("unexpected database query result; {}", diagnostics(app)));
    }
    match query_wait_state(state, marker) {
        Ok(true) => DatabaseStepResult::Done,
        Ok(false) => DatabaseStepResult::Pending,
        Err(error) => DatabaseStepResult::Failed(format!("{error}; {}", diagnostics(app))),
    }
}

pub(super) fn scroll_table(app: &mut App, direction: f32) -> Result<(), String> {
    let tab_id = active_pgo_table_id(app)?;
    let Some((_, state)) = app.database_table_meta_state_mut(tab_id) else {
        return Err(format!("PGO table state disappeared; {}", diagnostics(app)));
    };
    let row_count = state.grid.logical_row_count();
    let max_scroll = database_grid_max_scroll(
        row_count,
        DATABASE_GRID_ROW_HEIGHT,
        state.grid.viewport_height,
    );
    if max_scroll <= 0.0 {
        return Err(format!(
            "PGO table has no vertical scroll range; rows={row_count} viewport_height={}",
            state.grid.viewport_height
        ));
    }
    state.grid.scroll_y.anim_speed = 7.0;
    state.grid.scroll_y.scroll_by(72.0 * direction);
    state.grid.scroll_y.clamp_target(0.0, max_scroll);
    app.request_database_table_chunk_for_scroll(tab_id);
    if let Some(window) = app.window.as_ref() {
        window.request_redraw();
    }
    Ok(())
}

pub(super) fn scroll_query_result(app: &mut App, direction: f32) -> Result<(), String> {
    let viewport = app
        .ui_registry
        .rect_for(UiId::DatabaseQueryResultBody)
        .ok_or_else(|| format!("PGO query result viewport is not rendered; {}", diagnostics(app)))?;
    let scale = app.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
    let history = app.ide_panel.database.persisted.query_history.clone();
    let Some(active_tab) = app.tabs.get_mut(app.active_tab) else {
        return Err(format!("PGO query tab disappeared; {}", diagnostics(app)));
    };
    let EditorTabKind::DatabaseQuery(meta, state) = &mut active_tab.kind else {
        return Err(format!("PGO query result tab is not active; {}", diagnostics(app)));
    };
    if meta.connection_id != PGO_CONNECTION_ID {
        return Err("unexpected database query result connection".to_string());
    }
    let (_, max_y) = database_query_scroll_limits(
        meta,
        state,
        &history,
        viewport.2.max(1.0),
        viewport.3.max(1.0),
        scale,
    );
    if max_y <= 0.0 {
        return Err(format!(
            "PGO query result has no vertical scroll range; rows={} viewport_height={}",
            state
                .results
                .get(state.result_view.active_result)
                .map_or(0, |result| result.rows.len()),
            viewport.3
        ));
    }
    state.result_view.scroll_y.anim_speed = 7.0;
    state.result_view.scroll_y.scroll_by(72.0 * direction);
    state.result_view.scroll_y.clamp_target(0.0, max_y);
    if let Some(window) = app.window.as_ref() {
        window.request_redraw();
    }
    Ok(())
}

pub(super) fn diagnostics(app: &App) -> String {
    let pending = app.ide_panel.database.pending_job.as_ref().map_or_else(
        || "none".to_string(),
        |job| {
            format!(
                "id={} kind={:?} owner={:?} connection={} database={:?} table={:?}",
                job.id.0,
                job.kind,
                job.owner,
                job.connection_id.0,
                job.database_name,
                job.table_name
            )
        },
    );
    let connection = app
        .ide_panel
        .database
        .connection(PGO_CONNECTION_ID)
        .map_or_else(
            || "missing".to_string(),
            |node| {
                format!(
                    "status={:?} expanded={} loading={} databases_loaded={} databases={} status_message={:?}",
                    node.status,
                    node.expanded,
                    node.loading,
                    node.databases_loaded,
                    node.databases.len(),
                    node.status_message
                )
            },
        );
    let active = app.tabs.get(app.active_tab).map_or_else(
        || "none".to_string(),
        |tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, state) => format!(
                "table:{}.{}, loading={}, metadata={}, count={:?}, chunks={}, rows={}, count_loading={}, chunk_loading={}, error={:?}, count_error={:?}, dirty={}",
                meta.database_name,
                meta.table_name,
                state.loading,
                state.metadata.is_some(),
                state.grid.count,
                state.grid.chunks.len(),
                state.grid.logical_row_count(),
                state.grid.loading_count,
                state.grid.loading_chunk,
                state.error,
                state.grid.count_error,
                state.grid.dirty(),
            ),
            EditorTabKind::DatabaseQuery(meta, state) => format!(
                "query:{} console={} running={} completion_loaded={} results={} rows={} review={} error={:?}",
                meta.database_name,
                meta.console_id.0,
                state.running,
                state.completion_loaded,
                state.results.len(),
                state.results.iter().map(|result| result.rows.len()).sum::<usize>(),
                state.review.is_some(),
                state.error,
            ),
            _ => "non-database".to_string(),
        },
    );
    let transient = transient_diagnostics(app);
    format!(
        "database_state pending=[{pending}] global_error={:?} connection=[{connection}] active=[{active}] {transient}",
        app.ide_panel.database.global_error
    )
}

pub(super) fn transient_diagnostics(app: &App) -> String {
    let panel = &app.ide_panel.database;
    let pending = panel.pending_job.as_ref().map_or_else(
        || "none".to_string(),
        |job| {
            format!(
                "id={} kind={:?} owner={:?} connection={} database={:?} table={:?}",
                job.id.0,
                job.kind,
                job.owner,
                job.connection_id.0,
                job.database_name,
                job.table_name
            )
        },
    );
    format!(
        "database_ddl_hover_present={} database_context_menu_present={} database_table_modal_present={} database_dialog_present={} database_delete_prompt_present={} database_host_key_prompt_present={} database_pending_job=[{}] database_active_command_present={} database_queue_len={} database_host_key_retry_present={} database_query_review_present={} database_global_error_present={}",
        panel.ddl_hover.borrow().is_some(),
        panel.context_menu.is_some(),
        panel.table_modal.is_some(),
        panel.dialog.is_some(),
        panel.delete_prompt.is_some(),
        panel.host_key_prompt.is_some(),
        pending,
        panel.active_command.is_some(),
        panel.queued_commands.len(),
        panel.host_key_retry.is_some(),
        database_query_review_present(app),
        panel.global_error.is_some(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::database::{DatabaseQueryCell, DatabaseQueryResultSet};
    use std::collections::HashMap;

    fn endpoint(values: &[(&str, &str)]) -> Result<PgoDatabaseEndpoint, String> {
        let values = values
            .iter()
            .map(|(key, value)| ((*key).to_string(), OsString::from(value)))
            .collect::<HashMap<_, _>>();
        endpoint_from_lookup(|key| values.get(key).cloned())
    }

    #[test]
    fn pgo_database_endpoint_requires_explicit_valid_ephemeral_port() {
        let base = [
            (ENV_HOST, "127.0.0.1"),
            (ENV_NAME, PGO_DATABASE_NAME),
            (ENV_USER, PGO_DATABASE_USER),
        ];
        assert!(endpoint(&base).unwrap_err().contains(ENV_PORT));

        let mut malformed = base.to_vec();
        malformed.push((ENV_PORT, "not-a-port"));
        assert!(endpoint(&malformed).unwrap_err().contains("invalid PGO database port"));

        let mut zero = base.to_vec();
        zero.push((ENV_PORT, "0"));
        assert!(endpoint(&zero).unwrap_err().contains("1..=65535"));
    }

    #[test]
    fn pgo_database_connection_is_loopback_tls_disabled_and_secretless() {
        let parsed = endpoint(&[
            (ENV_HOST, "127.0.0.1"),
            (ENV_PORT, "15432"),
            (ENV_NAME, PGO_DATABASE_NAME),
            (ENV_USER, PGO_DATABASE_USER),
        ])
        .unwrap();
        let config = pgo_connection_config(&parsed);
        assert_eq!(config.id, PGO_CONNECTION_ID);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 15432);
        assert_eq!(config.tls_mode, PostgresTlsMode::Disable);
        assert!(!config.remember_postgres_password);
        assert!(config.ssh.is_none());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn pgo_query_wait_requires_real_nonempty_marker_result_and_surfaces_error() {
        let mut state = DatabaseQueryTabState::default();
        assert_eq!(query_wait_state(&state, PGO_QUERY_MARKER), Ok(false));

        state.error = Some("fixture failure".to_string());
        assert!(query_wait_state(&state, PGO_QUERY_MARKER)
            .unwrap_err()
            .contains("fixture failure"));

        state.error = None;
        state.results.push(DatabaseQueryResultSet {
            title: "Result 1".to_string(),
            columns: vec!["name".to_string()],
            rows: vec![vec![DatabaseQueryCell {
                value: Some(PGO_QUERY_MARKER.to_string()),
            }]],
            ..DatabaseQueryResultSet::default()
        });
        assert_eq!(query_wait_state(&state, PGO_QUERY_MARKER), Ok(true));
    }

    fn ddl_hover(
        connection_id: DatabaseConnectionId,
        database_name: &str,
        table_name: &str,
    ) -> crate::app::database::DatabaseDdlHoverState {
        crate::app::database::DatabaseDdlHoverState {
            connection_id,
            database_name: database_name.to_string(),
            table_name: table_name.to_string(),
            popup: crate::app::mouse::HoverPopup {
                text: format!("CREATE TABLE {table_name} (id integer);"),
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
            },
            rect: None,
            max_scroll: 0.0,
            selection_anchor: None,
            selection_cursor: None,
            selecting: false,
        }
    }

    fn pending_job(id: u64) -> crate::app::database::DatabasePendingJob {
        crate::app::database::DatabasePendingJob {
            id: crate::app::database::DatabaseJobId(id),
            kind: DatabasePendingJobKind::LoadDdl,
            owner: crate::app::database::DatabaseJobOwner::Connection(PGO_CONNECTION_ID),
            connection_id: PGO_CONNECTION_ID,
            database_name: Some(PGO_DATABASE_NAME.to_string()),
            table_name: Some(PGO_TABLE_NAME.to_string()),
        }
    }

    #[test]
    fn dismiss_ddl_removes_only_expected_pgo_popup_and_preserves_global_error() {
        let mut panel = crate::app::database::DatabasePanelState::default();
        panel.global_error = Some("keep-me".to_string());
        *panel.ddl_hover.borrow_mut() = Some(ddl_hover(
            PGO_CONNECTION_ID,
            PGO_DATABASE_NAME,
            PGO_TABLE_NAME,
        ));

        assert_eq!(dismiss_pgo_ddl_state(&panel), Ok(()));
        assert!(panel.ddl_hover.borrow().is_none());
        assert_eq!(panel.global_error.as_deref(), Some("keep-me"));
    }

    #[test]
    fn dismiss_ddl_rejects_wrong_identity_without_destroying_popup() {
        let panel = crate::app::database::DatabasePanelState::default();
        *panel.ddl_hover.borrow_mut() = Some(ddl_hover(
            DatabaseConnectionId(PGO_CONNECTION_ID.0 + 1),
            PGO_DATABASE_NAME,
            PGO_TABLE_NAME,
        ));

        let error = dismiss_pgo_ddl_state(&panel).unwrap_err();
        assert!(error.contains("refusing to dismiss unexpected DDL popup"));
        assert!(panel.ddl_hover.borrow().is_some());
    }

    #[test]
    fn database_idle_check_accepts_clean_state_and_names_transient_leaks() {
        let mut panel = crate::app::database::DatabasePanelState::default();
        assert_eq!(database_idle_check(&panel, false), Ok(()));

        panel.pending_job = Some(pending_job(1));
        panel.active_command = Some(crate::app::database::DatabaseCommand::CancelJob {
            job_id: crate::app::database::DatabaseJobId(1),
        });
        panel.queued_commands.push_back((
            crate::app::database::DatabaseCommand::CancelJob {
                job_id: crate::app::database::DatabaseJobId(2),
            },
            pending_job(2),
        ));
        panel.table_modal = Some(DatabaseTableModal::CustomLimit {
            tab_id: crate::app::database::DatabaseTabId(1),
            input: Default::default(),
            error: None,
        });
        *panel.ddl_hover.borrow_mut() = Some(ddl_hover(
            PGO_CONNECTION_ID,
            PGO_DATABASE_NAME,
            PGO_TABLE_NAME,
        ));
        panel.global_error = Some("fixture error".to_string());

        let error = database_idle_check(&panel, true).unwrap_err();
        for marker in [
            "pending_job",
            "active_command",
            "queued_commands",
            "table_modal",
            "ddl_hover",
            "query_review",
            "global_error",
        ] {
            assert!(error.contains(marker), "missing {marker}: {error}");
        }
    }

    #[test]
    fn database_wait_steps_have_state_based_timeout_budget_and_scroll_steps_are_identified() {
        assert_eq!(DatabaseAutomationStep::WaitCatalog.timeout(), Duration::from_secs(45));
        assert!(DatabaseAutomationStep::ScrollTableTimed { duration_secs: 8 }.is_timed_scroll());
        assert!(DatabaseAutomationStep::ScrollQueryResultTimed { duration_secs: 8 }.is_timed_scroll());
        assert!(!DatabaseAutomationStep::WaitQueryResult.is_timed_scroll());
    }
}
