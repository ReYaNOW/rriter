mod database_catalog;
mod database_grid;
mod database_panel;
mod database_postgres;
mod database_query;
mod database_runtime;
mod database_secrets;
mod database_ssh;
mod database_ssh_builtin;
mod database_table;

#[allow(unused_imports)]
pub use database_catalog::{
    DatabaseColumnInfo, DatabaseDdlResult, DatabaseTableMetadata, DatabaseTypeKind,
    load_public_table_metadata, quote_pg_identifier, reconstruct_public_table_ddl,
};
pub use database_panel::*;
#[allow(unused_imports)]
pub use database_grid::{
    DATABASE_GRID_DEFAULT_COLUMN_WIDTH, DATABASE_GRID_HEADER_HEIGHT,
    DATABASE_GRID_MAX_COLUMN_WIDTH, DATABASE_GRID_MIN_COLUMN_WIDTH, DATABASE_GRID_ROW_HEIGHT,
    DATABASE_TABLE_INPUT_TEXT_SCALE, DatabaseByteaPreview, DatabaseCellEditorKind,
    DatabaseCellEditorState, DatabaseCellPosition, DatabaseCellValue, DatabaseGridCell,
    DatabaseGridLayout, DatabaseGridRect, DatabaseGridRow, DatabaseGridSelection,
    DatabaseGridViewport, DatabaseRowState, DatabaseTableChunk, DatabaseTableGridState,
    DatabaseTableInputTarget, DatabaseTableRefreshPrompt, DatabaseTableReloadAction,
    DatabaseTableReviewState, DatabaseTableReviewSummary, civil_date_from_unix_days,
    database_column_width, database_columns_content_width, database_grid_layout,
    database_grid_max_scroll, database_grid_viewport, database_grid_visible_row_range,
    parse_bytea_preview, parse_editor_value, set_database_column_width,
};
#[allow(unused_imports)]
pub use database_query::*;
#[allow(unused_imports)]
pub use database_postgres::{
    DatabaseBackendError, DatabaseBackendNotice, DatabaseConnectionTestResult, DatabaseInfo,
    DatabaseListResult, DatabaseTableInfo, DatabaseTableListResult, PostgresSession,
    connect_postgres, list_databases, list_public_tables, test_database_connection,
};
#[allow(unused_imports)]
pub use database_runtime::{DatabaseCommand, DatabaseEvent, DatabaseRuntime, host_key_options};
#[allow(unused_imports)]
pub use database_secrets::{
    DatabaseSecretKind, database_secret_purpose, delete_all_database_secrets,
    delete_database_secret, load_database_secret, load_database_secret_bundle,
    save_remembered_database_secrets, store_database_secret,
};
#[allow(unused_imports)]
pub use database_ssh::{
    ResolvedSshEndpoint, SshBackendKind, SshBackendSelection, SshConnectOptions, SshHostKeyPolicy,
    SystemSshTunnel, resolve_builtin_endpoint, resolve_system_ssh, select_ssh_backend,
    system_ssh_args,
};
#[allow(unused_imports)]
pub use database_ssh_builtin::{BuiltinSshStream, DatabaseSshError};
#[allow(unused_imports)]
pub use database_table::{
    DatabaseChangeKind, DatabaseChangeParameter, DatabaseChangePlan,
    DatabaseChangePlanOperation, DatabaseChangeStatement, DatabasePreparedTableTransaction,
    DatabaseTableChunkResult, DatabaseTableCountResult, DatabaseTableModal,
    DatabaseTableTabMeta, DatabaseTableTabState, begin_table_transaction,
    DATABASE_SQL_PREVIEW_LINE_HEIGHT, DATABASE_TABLE_DISCONNECTED_MESSAGE,
    database_calendar_weekday_monday, database_calendar_year_month,
    database_days_in_month, database_shift_calendar_month,
    build_table_change_plan, count_public_table_rows,
    database_table_effective_order_by, load_public_table_chunk, validate_table_fragment,
};

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const DATABASE_STATE_VERSION: u32 = 2;
pub const MAX_DATABASE_CONNECTIONS: usize = 64;
pub const MAX_DATABASES_PER_CONNECTION: usize = 512;
pub const MAX_PUBLIC_TABLES_PER_DATABASE: usize = 10_000;
pub const MAX_COLUMNS_PER_RESULT: usize = 512;
pub const DATABASE_CHUNK_SIZE: usize = 100;
pub const MAX_CUSTOM_TABLE_LIMIT: usize = 10_000;
pub const DEFAULT_TABLE_LIMIT: usize = 100;
pub const MAX_CACHED_CHUNKS_PER_TAB: usize = 8;
pub const MAX_TABLE_CACHE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DISPLAY_CELL_BYTES: usize = 1024 * 1024;
pub const MAX_EDITABLE_MULTILINE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SQL_CONSOLE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_BYTEA_PREVIEW_BYTES: usize = 64 * 1024;
pub const MAX_RESULT_ROWS: usize = 50_000;
pub const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RESULT_SETS: usize = 32;
pub const MAX_SQL_HISTORY_ENTRIES: usize = 500;
pub const MAX_SQL_HISTORY_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_REVIEW_DETAIL_ROWS: usize = 500;
pub const MAX_REVIEW_CELL_DIFFS: usize = 2_000;
pub const MAX_PERSISTED_SELECTED_PRIMARY_KEYS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DatabaseConnectionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DatabaseTabId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SqlConsoleId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DatabaseJobId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DatabaseTransactionId(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct DatabaseGeneration(pub u64);

impl DatabaseGeneration {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseConnectionColor {
    #[default]
    Blue,
    Green,
    Yellow,
    Orange,
    Red,
    Purple,
    Cyan,
    Gray,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostgresTlsMode {
    Disable,
    #[default]
    Prefer,
    Require,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseExecutionPolicy {
    InternalReadAutocommit,
    TableMutationReview,
    UserSqlReview,
}

impl DatabaseExecutionPolicy {
    pub fn requires_explicit_transaction(self) -> bool {
        !matches!(self, Self::InternalReadAutocommit)
    }

    pub fn requires_global_review(self) -> bool {
        matches!(self, Self::TableMutationReview | Self::UserSqlReview)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SshJumpHostConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub config_alias: Option<String>,
    pub private_key_path: Option<PathBuf>,
    pub remember_password: bool,
    pub remember_key_passphrase: bool,
}

impl Default for SshJumpHostConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            config_alias: None,
            private_key_path: None,
            remember_password: false,
            remember_key_passphrase: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SshConnectionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub config_alias: Option<String>,
    pub private_key_path: Option<PathBuf>,
    pub remember_password: bool,
    pub remember_key_passphrase: bool,
    pub jump_host: Option<SshJumpHostConfig>,
}

impl Default for SshConnectionConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            config_alias: None,
            private_key_path: None,
            remember_password: false,
            remember_key_passphrase: false,
            jump_host: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConnectionConfig {
    pub id: DatabaseConnectionId,
    pub display_name: String,
    pub color: DatabaseConnectionColor,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub maintenance_database: String,
    pub tls_mode: PostgresTlsMode,
    pub remember_postgres_password: bool,
    pub ssh: Option<SshConnectionConfig>,
}

impl Default for DatabaseConnectionConfig {
    fn default() -> Self {
        Self {
            id: DatabaseConnectionId(0),
            display_name: "PostgreSQL".to_string(),
            color: DatabaseConnectionColor::default(),
            host: "localhost".to_string(),
            port: 5432,
            username: String::new(),
            maintenance_database: "postgres".to_string(),
            tls_mode: PostgresTlsMode::default(),
            remember_postgres_password: false,
            ssh: None,
        }
    }
}

impl DatabaseConnectionConfig {
    pub fn validate(&self) -> Result<(), DatabaseConfigError> {
        validate_required_text("connection name", &self.display_name, 128)?;
        validate_host("PostgreSQL host", &self.host)?;
        validate_port("PostgreSQL port", self.port)?;
        validate_required_text("PostgreSQL username", &self.username, 128)?;
        validate_database_name(&self.maintenance_database)?;
        if let Some(ssh) = &self.ssh {
            ssh.validate()?;
        }
        Ok(())
    }
}

impl SshConnectionConfig {
    pub fn validate(&self) -> Result<(), DatabaseConfigError> {
        if self.config_alias.as_deref().is_none_or(str::is_empty) {
            validate_host("SSH host", &self.host)?;
            validate_required_text("SSH username", &self.username, 128)?;
        } else if let Some(alias) = self.config_alias.as_deref() {
            validate_required_text("SSH config alias", alias, 255)?;
        }
        validate_port("SSH port", self.port)?;
        if let Some(jump_host) = &self.jump_host {
            jump_host.validate()?;
        }
        Ok(())
    }
}

impl SshJumpHostConfig {
    pub fn validate(&self) -> Result<(), DatabaseConfigError> {
        if self.config_alias.as_deref().is_none_or(str::is_empty) {
            validate_host("SSH jump host", &self.host)?;
            validate_required_text("SSH jump username", &self.username, 128)?;
        } else if let Some(alias) = self.config_alias.as_deref() {
            validate_required_text("SSH jump config alias", alias, 255)?;
        }
        validate_port("SSH jump port", self.port)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseConfigError {
    pub field: &'static str,
    pub message: &'static str,
}

impl fmt::Display for DatabaseConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for DatabaseConfigError {}

fn validate_required_text(
    field: &'static str,
    value: &str,
    max_chars: usize,
) -> Result<(), DatabaseConfigError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DatabaseConfigError {
            field,
            message: "value is required",
        });
    }
    if value.chars().count() > max_chars {
        return Err(DatabaseConfigError {
            field,
            message: "value is too long",
        });
    }
    if value.chars().any(char::is_control) {
        return Err(DatabaseConfigError {
            field,
            message: "control characters are not allowed",
        });
    }
    Ok(())
}

fn validate_host(field: &'static str, host: &str) -> Result<(), DatabaseConfigError> {
    validate_required_text(field, host, 255)?;
    if host.contains(':') {
        return Err(DatabaseConfigError {
            field,
            message: "IPv6 addresses are not supported yet",
        });
    }
    if host.chars().any(char::is_whitespace) {
        return Err(DatabaseConfigError {
            field,
            message: "whitespace is not allowed",
        });
    }
    Ok(())
}

fn validate_database_name(database: &str) -> Result<(), DatabaseConfigError> {
    validate_required_text("maintenance database", database, 128)
}

fn validate_port(field: &'static str, port: u16) -> Result<(), DatabaseConfigError> {
    if port == 0 {
        return Err(DatabaseConfigError {
            field,
            message: "port must be greater than zero",
        });
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub transaction_review_timeout_seconds: u64,
    pub statement_timeout_seconds: u64,
    pub lock_timeout_seconds: u64,
    pub connect_timeout_seconds: u64,
    pub ssh_startup_timeout_seconds: u64,
    pub default_table_limit: usize,
    pub result_row_limit: usize,
    pub result_memory_limit_bytes: usize,
    pub sql_history_limit: usize,
    pub default_connection_color: DatabaseConnectionColor,
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            transaction_review_timeout_seconds: 300,
            statement_timeout_seconds: 30,
            lock_timeout_seconds: 5,
            connect_timeout_seconds: 10,
            ssh_startup_timeout_seconds: 15,
            default_table_limit: DEFAULT_TABLE_LIMIT,
            result_row_limit: MAX_RESULT_ROWS,
            result_memory_limit_bytes: MAX_RESULT_BYTES,
            sql_history_limit: MAX_SQL_HISTORY_ENTRIES,
            default_connection_color: DatabaseConnectionColor::default(),
        }
    }
}

impl DatabaseSettings {
    pub fn normalize(&mut self) {
        self.transaction_review_timeout_seconds =
            self.transaction_review_timeout_seconds.clamp(30, 1_800);
        self.statement_timeout_seconds = self.statement_timeout_seconds.clamp(1, 3_600);
        self.lock_timeout_seconds = self.lock_timeout_seconds.clamp(1, 300);
        self.connect_timeout_seconds = self.connect_timeout_seconds.clamp(1, 120);
        self.ssh_startup_timeout_seconds = self.ssh_startup_timeout_seconds.clamp(1, 120);
        self.default_table_limit = self.default_table_limit.clamp(1, MAX_CUSTOM_TABLE_LIMIT);
        self.result_row_limit = self.result_row_limit.clamp(1, MAX_RESULT_ROWS);
        self.result_memory_limit_bytes = self
            .result_memory_limit_bytes
            .clamp(1024 * 1024, MAX_RESULT_BYTES);
        self.sql_history_limit = self.sql_history_limit.clamp(1, MAX_SQL_HISTORY_ENTRIES);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseSortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseColumnWidth {
    pub column_name: String,
    pub width_px: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatabaseTableViewKey {
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub table_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseTableViewState {
    pub key: DatabaseTableViewKey,
    pub where_clause: String,
    pub order_by: String,
    pub sorted_column: Option<String>,
    pub sort_direction: Option<DatabaseSortDirection>,
    pub limit: usize,
    pub current_page: usize,
    pub column_widths: Vec<DatabaseColumnWidth>,
    pub selected_primary_keys: Vec<Vec<String>>,
}

impl Default for DatabaseTableViewState {
    fn default() -> Self {
        Self {
            key: DatabaseTableViewKey {
                connection_id: DatabaseConnectionId(0),
                database_name: String::new(),
                table_name: String::new(),
            },
            where_clause: String::new(),
            order_by: String::new(),
            sorted_column: None,
            sort_direction: None,
            limit: DEFAULT_TABLE_LIMIT,
            current_page: 0,
            column_widths: Vec::new(),
            selected_primary_keys: Vec::new(),
        }
    }
}

impl DatabaseTableViewState {
    pub fn normalize(&mut self) {
        self.limit = self.limit.clamp(1, MAX_CUSTOM_TABLE_LIMIT);
        self.where_clause.truncate(64 * 1024);
        self.order_by.truncate(64 * 1024);
        self.column_widths.truncate(MAX_COLUMNS_PER_RESULT);
        for width in &mut self.column_widths {
            width.width_px = width.width_px.clamp(60, 4_096);
            width.column_name.truncate(256);
        }
        self.selected_primary_keys
            .truncate(MAX_PERSISTED_SELECTED_PRIMARY_KEYS);
        for primary_key in &mut self.selected_primary_keys {
            primary_key.truncate(32);
            for value in primary_key {
                value.truncate(4_096);
            }
        }
        if self.sorted_column.is_none() {
            self.sort_direction = None;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConsoleState {
    pub id: SqlConsoleId,
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub title: String,
    pub open: bool,
}

impl Default for DatabaseConsoleState {
    fn default() -> Self {
        Self {
            id: SqlConsoleId(0),
            connection_id: DatabaseConnectionId(0),
            database_name: String::new(),
            title: String::new(),
            open: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabasePersistedState {
    pub version: u32,
    pub settings: DatabaseSettings,
    pub connections: Vec<DatabaseConnectionConfig>,
    pub table_views: Vec<DatabaseTableViewState>,
    pub consoles: Vec<DatabaseConsoleState>,
    pub query_history: Vec<DatabaseQueryHistoryEntry>,
    pub selected_connection: Option<DatabaseConnectionId>,
    pub selected_database: Option<(DatabaseConnectionId, String)>,
    pub expanded_connections: Vec<DatabaseConnectionId>,
    pub expanded_databases: Vec<(DatabaseConnectionId, String)> ,
}

impl Default for DatabasePersistedState {
    fn default() -> Self {
        Self {
            version: DATABASE_STATE_VERSION,
            settings: DatabaseSettings::default(),
            connections: Vec::new(),
            table_views: Vec::new(),
            consoles: Vec::new(),
            query_history: Vec::new(),
            selected_connection: None,
            selected_database: None,
            expanded_connections: Vec::new(),
            expanded_databases: Vec::new(),
        }
    }
}

impl DatabasePersistedState {
    pub fn normalize_and_validate(&mut self) -> Result<(), DatabaseConfigError> {
        if self.version > DATABASE_STATE_VERSION {
            return Err(DatabaseConfigError {
                field: "state version",
                message: "database state was created by a newer RRiter version",
            });
        }
        self.version = DATABASE_STATE_VERSION;
        self.settings.normalize();
        if self.connections.len() > MAX_DATABASE_CONNECTIONS {
            return Err(DatabaseConfigError {
                field: "connections",
                message: "too many database connections",
            });
        }

        let mut ids = HashSet::with_capacity(self.connections.len());
        for connection in &self.connections {
            connection.validate()?;
            if !ids.insert(connection.id) {
                return Err(DatabaseConfigError {
                    field: "connection id",
                    message: "duplicate connection id",
                });
            }
        }

        self.table_views.retain(|view| {
            ids.contains(&view.key.connection_id)
                && !view.key.database_name.is_empty()
                && !view.key.table_name.is_empty()
        });
        for view in &mut self.table_views {
            view.normalize();
        }
        self.consoles.retain(|console| {
            ids.contains(&console.connection_id) && !console.database_name.is_empty()
        });
        self.query_history.retain(|entry| {
            ids.contains(&entry.connection_id) && !entry.database_name.is_empty()
        });
        for entry in &mut self.query_history {
            entry.normalize();
        }
        database_query::trim_database_query_history(
            &mut self.query_history,
            self.settings.sql_history_limit.min(MAX_SQL_HISTORY_ENTRIES),
            MAX_SQL_HISTORY_BYTES,
        );
        self.selected_connection = self.selected_connection.filter(|id| ids.contains(id));
        self.selected_database = self.selected_database.take().filter(|(id, name)| {
            ids.contains(id) && !name.is_empty()
        });
        self.expanded_connections.retain(|id| ids.contains(id));
        self.expanded_connections.sort_by_key(|id| id.0);
        self.expanded_connections.dedup();
        self.expanded_databases.retain(|(id, name)| ids.contains(id) && !name.is_empty());
        self.expanded_databases.sort_by(|a, b| (a.0.0, &a.1).cmp(&(b.0.0, &b.1)));
        self.expanded_databases.dedup();
        Ok(())
    }
}

pub struct DatabaseSecretBundle {
    pub postgres_password: Option<Zeroizing<String>>,
    pub ssh_password: Option<Zeroizing<String>>,
    pub ssh_key_passphrase: Option<Zeroizing<String>>,
    pub jump_password: Option<Zeroizing<String>>,
    pub jump_key_passphrase: Option<Zeroizing<String>>,
}

impl Clone for DatabaseSecretBundle {
    fn clone(&self) -> Self {
        self.clone_for_job()
    }
}

impl DatabaseSecretBundle {
    pub fn empty() -> Self {
        Self {
            postgres_password: None,
            ssh_password: None,
            ssh_key_passphrase: None,
            jump_password: None,
            jump_key_passphrase: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.postgres_password.is_none()
            && self.ssh_password.is_none()
            && self.ssh_key_passphrase.is_none()
            && self.jump_password.is_none()
            && self.jump_key_passphrase.is_none()
    }

    pub fn clone_for_job(&self) -> Self {
        Self {
            postgres_password: self.postgres_password.as_ref().map(|value| Zeroizing::new(value.to_string())),
            ssh_password: self.ssh_password.as_ref().map(|value| Zeroizing::new(value.to_string())),
            ssh_key_passphrase: self.ssh_key_passphrase.as_ref().map(|value| Zeroizing::new(value.to_string())),
            jump_password: self.jump_password.as_ref().map(|value| Zeroizing::new(value.to_string())),
            jump_key_passphrase: self.jump_key_passphrase.as_ref().map(|value| Zeroizing::new(value.to_string())),
        }
    }
}

pub fn database_config_root() -> PathBuf {
    crate::platform::config_dir().join("database")
}

pub fn database_state_path() -> PathBuf {
    database_config_root().join("state.json")
}

pub fn database_sql_root() -> PathBuf {
    crate::platform::state_dir().join("database").join("sql")
}

pub fn database_console_path(
    connection_id: DatabaseConnectionId,
    database_name: &str,
    console_id: SqlConsoleId,
) -> PathBuf {
    database_sql_root()
        .join(connection_id.0.to_string())
        .join(format!(
            "{}-{}.sql",
            hex_database_name(database_name),
            console_id.0
        ))
}

pub fn load_database_state(path: &Path) -> io::Result<DatabasePersistedState> {
    let content = match crate::platform::read_text_file(path) {
        Ok(content) => content.text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(DatabasePersistedState::default());
        }
        Err(error) => return Err(error),
    };
    let mut state: DatabasePersistedState = serde_json::from_str(&content)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    state
        .normalize_and_validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(state)
}

pub fn save_database_state(path: &Path, state: &DatabasePersistedState) -> io::Result<()> {
    let mut normalized = state.clone();
    normalized
        .normalize_and_validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let bytes = serde_json::to_vec_pretty(&normalized)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    crate::platform::atomic_write(path, &bytes)
}

pub fn load_database_console(path: &Path) -> io::Result<String> {
    match crate::platform::read_text_file(path) {
        Ok(content) => Ok(content.text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error),
    }
}

pub fn save_database_console(path: &Path, sql: &str) -> io::Result<()> {
    if sql.len() > MAX_SQL_CONSOLE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SQL console exceeds the 8 MiB limit",
        ));
    }
    crate::platform::atomic_write(path, sql.as_bytes())
}

fn hex_database_name(database_name: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = database_name.as_bytes();
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for &byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn valid_connection(id: u64) -> DatabaseConnectionConfig {
        DatabaseConnectionConfig {
            id: DatabaseConnectionId(id),
            display_name: format!("Local {id}"),
            host: "localhost".to_string(),
            username: "rriter".to_string(),
            ..DatabaseConnectionConfig::default()
        }
    }

    fn temp_test_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rriter-database-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn foundation_limits_ids_and_paths_match_database_plan() {
        assert_eq!(MAX_DATABASES_PER_CONNECTION, 512);
        assert_eq!(MAX_PUBLIC_TABLES_PER_DATABASE, 10_000);
        assert_eq!(DATABASE_CHUNK_SIZE, 100);
        assert_eq!(MAX_CACHED_CHUNKS_PER_TAB, 8);
        assert_eq!(MAX_TABLE_CACHE_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_DISPLAY_CELL_BYTES, 1024 * 1024);
        assert_eq!(MAX_EDITABLE_MULTILINE_BYTES, 4 * 1024 * 1024);
        assert_eq!(MAX_BYTEA_PREVIEW_BYTES, 64 * 1024);
        assert_eq!(MAX_RESULT_SETS, 32);
        assert_eq!(MAX_REVIEW_DETAIL_ROWS, 500);
        assert_eq!(MAX_REVIEW_CELL_DIFFS, 2_000);

        let _tab_id = DatabaseTabId(3);
        let _job_id = DatabaseJobId(4);
        let _transaction_id = DatabaseTransactionId(5);
        assert_eq!(DatabaseGeneration::default().next(), DatabaseGeneration(1));
        assert!(database_config_root().ends_with("database"));
        assert!(database_state_path().ends_with("state.json"));
    }

    #[test]
    fn execution_policies_keep_internal_reads_out_of_review_transactions() {
        assert!(!DatabaseExecutionPolicy::InternalReadAutocommit.requires_explicit_transaction());
        assert!(!DatabaseExecutionPolicy::InternalReadAutocommit.requires_global_review());
        assert!(DatabaseExecutionPolicy::TableMutationReview.requires_explicit_transaction());
        assert!(DatabaseExecutionPolicy::TableMutationReview.requires_global_review());
        assert!(DatabaseExecutionPolicy::UserSqlReview.requires_explicit_transaction());
        assert!(DatabaseExecutionPolicy::UserSqlReview.requires_global_review());
    }

    #[test]
    fn connection_validation_rejects_missing_fields_ipv6_and_zero_ports() {
        let mut connection = valid_connection(1);
        assert!(connection.validate().is_ok());

        connection.host = "::1".to_string();
        assert_eq!(connection.validate().unwrap_err().field, "PostgreSQL host");
        connection.host = "localhost".to_string();
        connection.port = 0;
        assert_eq!(connection.validate().unwrap_err().field, "PostgreSQL port");
        connection.port = 5432;
        connection.username.clear();
        assert_eq!(
            connection.validate().unwrap_err().field,
            "PostgreSQL username"
        );
    }

    #[test]
    fn ssh_validation_accepts_config_alias_without_duplicate_host_fields() {
        let mut ssh = SshConnectionConfig {
            config_alias: Some("production-bastion".to_string()),
            ..SshConnectionConfig::default()
        };
        assert!(ssh.validate().is_ok());

        ssh.config_alias = None;
        assert_eq!(ssh.validate().unwrap_err().field, "SSH host");
    }

    #[test]
    fn settings_and_table_view_state_are_bounded() {
        let mut settings = DatabaseSettings {
            transaction_review_timeout_seconds: 1,
            statement_timeout_seconds: 0,
            default_table_limit: usize::MAX,
            result_memory_limit_bytes: 1,
            ..DatabaseSettings::default()
        };
        settings.normalize();
        assert_eq!(settings.transaction_review_timeout_seconds, 30);
        assert_eq!(settings.statement_timeout_seconds, 1);
        assert_eq!(settings.default_table_limit, MAX_CUSTOM_TABLE_LIMIT);
        assert_eq!(settings.result_memory_limit_bytes, 1024 * 1024);

        let mut view = DatabaseTableViewState {
            limit: 0,
            selected_primary_keys: vec![vec!["id".to_string()]; 300],
            column_widths: vec![DatabaseColumnWidth {
                column_name: "name".to_string(),
                width_px: 1,
            }],
            ..DatabaseTableViewState::default()
        };
        view.normalize();
        assert_eq!(view.limit, 1);
        assert_eq!(view.selected_primary_keys.len(), 256);
        assert_eq!(view.column_widths[0].width_px, 60);
    }

    #[test]
    fn persisted_state_roundtrip_contains_no_secret_values() {
        let mut state = DatabasePersistedState::default();
        let mut connection = valid_connection(7);
        connection.remember_postgres_password = true;
        connection.ssh = Some(SshConnectionConfig {
            host: "bastion.example.com".to_string(),
            username: "deploy".to_string(),
            remember_password: true,
            remember_key_passphrase: true,
            ..SshConnectionConfig::default()
        });
        state.connections.push(connection);

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(!json.contains("postgres-secret-value"));
        assert!(!json.contains("ssh-secret-value"));
        assert!(!json.contains("key-passphrase-value"));
        assert!(!json.contains("\"postgres_password\":"));
        assert!(!json.contains("\"ssh_password\":"));
        assert!(!json.contains("\"key_passphrase\":"));

        let mut decoded: DatabasePersistedState = serde_json::from_str(&json).unwrap();
        decoded.normalize_and_validate().unwrap();
        assert_eq!(decoded, state);
    }

    #[test]
    fn persisted_state_roundtrip_keeps_database_tree_and_session_selection() {
        let mut state = DatabasePersistedState::default();
        state.connections.push(valid_connection(7));
        state.selected_connection = Some(DatabaseConnectionId(7));
        state.selected_database = Some((DatabaseConnectionId(7), "analytics".to_string()));
        state.expanded_connections.push(DatabaseConnectionId(7));
        state
            .expanded_databases
            .push((DatabaseConnectionId(7), "analytics".to_string()));

        let json = serde_json::to_string(&state).unwrap();
        let mut decoded: DatabasePersistedState = serde_json::from_str(&json).unwrap();
        decoded.normalize_and_validate().unwrap();

        assert_eq!(decoded, state);
    }

    #[test]
    fn persisted_query_history_is_bounded_and_connection_scoped() {
        let mut state = DatabasePersistedState::default();
        state.connections.push(valid_connection(7));
        state.settings.sql_history_limit = 3;
        for index in 0..8 {
            state.query_history.push(DatabaseQueryHistoryEntry {
                connection_id: if index == 0 {
                    DatabaseConnectionId(999)
                } else {
                    DatabaseConnectionId(7)
                },
                database_name: "analytics".to_string(),
                console_id: SqlConsoleId(1),
                sql: format!("SELECT {index}"),
                ..DatabaseQueryHistoryEntry::default()
            });
        }
        state.normalize_and_validate().unwrap();
        assert_eq!(state.query_history.len(), 3);
        assert!(state
            .query_history
            .iter()
            .all(|entry| entry.connection_id == DatabaseConnectionId(7)));
        assert_eq!(state.query_history.last().unwrap().sql, "SELECT 7");
    }

    #[test]
    fn persisted_state_rejects_unknown_future_versions() {
        let mut state = DatabasePersistedState {
            version: DATABASE_STATE_VERSION + 1,
            ..DatabasePersistedState::default()
        };
        let error = state.normalize_and_validate().unwrap_err();
        assert_eq!(error.field, "state version");
    }

    #[test]
    fn persisted_state_rejects_duplicate_connection_ids() {
        let mut state = DatabasePersistedState::default();
        state.connections.push(valid_connection(9));
        state.connections.push(valid_connection(9));
        let error = state.normalize_and_validate().unwrap_err();
        assert_eq!(error.field, "connection id");
    }

    #[test]
    fn state_and_console_files_use_atomic_platform_writes() {
        let root = temp_test_path("persist");
        let state_path = root.join("state.json");
        let console_path = root.join("console.sql");
        let mut state = DatabasePersistedState::default();
        state.connections.push(valid_connection(3));

        save_database_state(&state_path, &state).unwrap();
        assert_eq!(load_database_state(&state_path).unwrap(), state);

        save_database_console(&console_path, "SELECT 1;\n").unwrap();
        assert_eq!(load_database_console(&console_path).unwrap(), "SELECT 1;\n");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn console_paths_are_cross_platform_safe_and_stable() {
        let path = database_console_path(
            DatabaseConnectionId(42),
            "данные/production",
            SqlConsoleId(5),
        );
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        assert!(file_name.ends_with("-5.sql"));
        assert!(!file_name.contains('/'));
        assert!(!file_name.contains("данные"));
    }

    #[test]
    fn oversized_console_is_rejected_before_filesystem_access() {
        let path = temp_test_path("oversized.sql");
        let sql = "x".repeat(MAX_SQL_CONSOLE_BYTES + 1);
        let error = save_database_console(&path, &sql).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!path.exists());
    }

    #[test]
    fn secret_bundle_starts_empty_without_debug_exposure() {
        let secrets = DatabaseSecretBundle::empty();
        assert!(secrets.is_empty());
    }
}
