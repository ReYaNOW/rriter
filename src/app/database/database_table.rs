use super::database_postgres::{
    DatabaseBackendError, DatabaseBackendNotice, PostgresSession, connect_postgres,
};
use super::database_ssh::SshConnectOptions;
use super::{
    DATABASE_CHUNK_SIZE, DatabaseCellValue,
    DatabaseColumnInfo, DatabaseConnectionConfig, DatabaseConnectionId, DatabaseDialogInput, DatabaseGeneration, DatabaseGridCell,
    DatabaseGridRow, DatabaseRowState, DatabaseSecretBundle, DatabaseSettings,
    DatabaseTableChunk, DatabaseTableMetadata, DatabaseTableReviewSummary,
    MAX_REVIEW_DETAIL_ROWS, MAX_REVIEW_CELL_DIFFS, parse_bytea_preview, quote_pg_identifier,
};
use std::time::Duration;
use tokio_postgres::types::ToSql;

pub const DATABASE_TABLE_DISCONNECTED_MESSAGE: &str =
    "Подключение к базе данных не установлено. Откройте панель «Базы данных» и обновите подключение.";
pub const DATABASE_SQL_PREVIEW_LINE_HEIGHT: f32 = 26.0;

#[derive(Clone, Debug)]
pub enum DatabaseTableModal {
    SqlPreview {
        tab_id: super::DatabaseTabId,
        text: String,
        cursor: usize,
        selection_anchor: Option<usize>,
        spans: Vec<crate::highlighter::ColorSpan>,
        scroll_x: crate::scroll::ScrollState,
        scroll_y: crate::scroll::ScrollState,
    },
    RefreshPrompt {
        tab_id: super::DatabaseTabId,
        close_after_save: bool,
    },
    CustomLimit {
        tab_id: super::DatabaseTabId,
        input: DatabaseDialogInput,
        error: Option<String>,
    },
    MultilineEditor {
        tab_id: super::DatabaseTabId,
        position: super::DatabaseCellPosition,
        input: DatabaseDialogInput,
        scroll: crate::scroll::ScrollState,
        error: Option<String>,
    },
    Review {
        tab_id: super::DatabaseTabId,
        state: super::DatabaseTableReviewState,
        scroll: crate::scroll::ScrollState,
    },
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableTabMeta {
    pub tab_id: super::DatabaseTabId,
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub table_name: String,
}

impl DatabaseTableTabMeta {
    pub fn title(&self) -> String {
        format!("{} — {}", self.table_name, self.database_name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableTabState {
    pub generation: DatabaseGeneration,
    pub metadata: Option<super::DatabaseTableMetadata>,
    pub grid: super::DatabaseTableGridState,
    pub loading: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub notice_until: Option<std::time::Instant>,
    pub unavailable_text: DatabaseDialogInput,
    pub unavailable_text_focused: bool,
    pub unavailable_text_dragging: bool,
}

impl DatabaseTableTabState {
    pub fn new(view: super::DatabaseTableViewState) -> Self {
        let mut unavailable_text = DatabaseDialogInput::new(DATABASE_TABLE_DISCONNECTED_MESSAGE);
        unavailable_text.cursor = 0;
        Self {
            generation: DatabaseGeneration::default(),
            metadata: None,
            grid: super::DatabaseTableGridState::new(view),
            loading: true,
            error: None,
            notice: None,
            notice_until: None,
            unavailable_text,
            unavailable_text_focused: false,
            unavailable_text_dragging: false,
        }
    }

    pub fn set_unavailable_text(&mut self, text: impl Into<String>) {
        self.unavailable_text.set_text(text);
        self.unavailable_text.cursor = 0;
        self.unavailable_text_focused = false;
        self.unavailable_text_dragging = false;
    }

    pub fn clear_unavailable_selection(&mut self) {
        self.unavailable_text.cursor = 0;
        self.unavailable_text.clear_selection();
        self.unavailable_text_focused = false;
        self.unavailable_text_dragging = false;
    }

    pub fn show_timed_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.notice_until = Some(
            std::time::Instant::now() + std::time::Duration::from_millis(2800),
        );
    }

    pub fn active_notice(&self) -> Option<&str> {
        self.notice.as_deref().filter(|_| {
            self.notice_until
                .is_some_and(|until| std::time::Instant::now() < until)
        })
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
        self.notice_until = None;
    }
}

impl Default for DatabaseTableTabState {
    fn default() -> Self {
        Self::new(crate::app::database::DatabaseTableViewState::default())
    }
}

pub fn database_calendar_year_month(text: &str) -> Option<(i32, u32)> {
    let date = text.get(..10)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || day == 0 || day > database_days_in_month(year, month) {
        return None;
    }
    Some((year, month))
}

pub fn database_days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 0,
    }
}

pub fn database_shift_calendar_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = year.saturating_mul(12).saturating_add(month as i32 - 1).saturating_add(delta);
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

pub fn database_calendar_weekday_monday(year: i32, month: u32, day: u32) -> u32 {
    let mut y = year;
    let mut m = month as i32;
    if m < 3 {
        y -= 1;
        m += 12;
    }
    let k = y.rem_euclid(100);
    let j = y.div_euclid(100);
    let h = (day as i32 + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j)
        .rem_euclid(7);
    ((h + 5).rem_euclid(7)) as u32
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableCountResult {
    pub database_name: String,
    pub table_name: String,
    pub generation: DatabaseGeneration,
    pub count: u64,
    pub notices: Vec<DatabaseBackendNotice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableChunkResult {
    pub database_name: String,
    pub table_name: String,
    pub generation: DatabaseGeneration,
    pub chunk: DatabaseTableChunk,
    pub notices: Vec<DatabaseBackendNotice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseChangeParameter {
    pub value: Option<String>,
    pub type_name: String,
    pub preview: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseChangeKind {
    Insert,
    Update,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseChangeStatement {
    pub kind: DatabaseChangeKind,
    pub sql: String,
    pub parameters: Vec<DatabaseChangeParameter>,
    pub changed_cells: usize,
    pub row_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseChangePlan {
    pub database_name: String,
    pub table_name: String,
    pub statements: Vec<DatabaseChangeStatement>,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseChangePlanOperation {
    Insert(DatabaseGridRow),
    Update(DatabaseGridRow),
    Delete(DatabaseGridRow),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabasePreparedTableTransaction {
    pub summary: DatabaseTableReviewSummary,
    pub notices: Vec<DatabaseBackendNotice>,
}

pub fn validate_table_fragment(fragment: &str, label: &str) -> Result<(), String> {
    let trimmed = fragment.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed.len() > 64 * 1024 {
        return Err(format!("{label} превышает 64 KiB"));
    }
    if crate::languages::sql::contains_top_level_semicolon(trimmed) {
        return Err(format!("{label} не должен содержать верхнеуровневую точку с запятой"));
    }
    if label == "WHERE" && contains_unquoted_double_equals(trimmed) {
        return Err("WHERE содержит недопустимый оператор ==; используйте =".to_string());
    }
    let wrapped = if label == "WHERE" {
        format!("SELECT 1 FROM public.__rriter_validation WHERE {trimmed}")
    } else {
        format!("SELECT 1 FROM public.__rriter_validation ORDER BY {trimmed}")
    };
    if crate::languages::sql::scan_statements(&wrapped).len() != 1 {
        return Err(format!("{label} должен быть одним SQL-фрагментом"));
    }
    if crate::languages::sql::has_syntax_error(&wrapped) {
        return Err(format!("{label} содержит синтаксическую ошибку"));
    }
    Ok(())
}

fn contains_unquoted_double_equals(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut quote = None;
    while index + 1 < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                if index + 1 < bytes.len() && bytes[index + 1] == active {
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'=' && bytes[index + 1] == b'=' {
            return true;
        }
        index += 1;
    }
    false
}

pub fn database_table_effective_order_by(view: &super::DatabaseTableViewState) -> String {
    match (view.sorted_column.as_deref(), view.sort_direction) {
        (Some(column), Some(direction)) => format!(
            "__rriter_source.{} {}",
            quote_pg_identifier(column),
            match direction {
                super::DatabaseSortDirection::Asc => "ASC",
                super::DatabaseSortDirection::Desc => "DESC",
            }
        ),
        _ => view.order_by.trim().to_string(),
    }
}

pub fn build_table_change_plan(
    metadata: &DatabaseTableMetadata,
    database_name: &str,
    table_name: &str,
    operations: Vec<DatabaseChangePlanOperation>,
) -> Result<DatabaseChangePlan, String> {
    if !metadata.editable {
        return Err(metadata
            .read_only_reason
            .clone()
            .unwrap_or_else(|| "Таблица доступна только для чтения".to_string()));
    }
    let mut statements = Vec::with_capacity(operations.len());
    for operation in operations {
        let statement = match operation {
            DatabaseChangePlanOperation::Insert(row) => build_insert(metadata, table_name, row)?,
            DatabaseChangePlanOperation::Update(row) => build_update(metadata, table_name, row)?,
            DatabaseChangePlanOperation::Delete(row) => build_delete(metadata, table_name, row)?,
        };
        statements.push(statement);
    }
    if statements.is_empty() {
        return Err("Нет изменений для сохранения".to_string());
    }
    let mut preview = String::from("BEGIN;\n\n");
    for statement in &statements {
        preview.push_str(&statement.sql);
        preview.push_str(";\n");
        for (index, parameter) in statement.parameters.iter().enumerate() {
            preview.push_str(&format!(
                "-- ${}: {} = {}\n",
                index + 1,
                parameter.type_name,
                parameter.preview
            ));
        }
        preview.push('\n');
    }
    preview.push_str("-- «Применить» выполнит COMMIT; «Отмена» выполнит ROLLBACK.\n");
    Ok(DatabaseChangePlan {
        database_name: database_name.to_string(),
        table_name: table_name.to_string(),
        statements,
        preview,
    })
}

fn build_insert(
    metadata: &DatabaseTableMetadata,
    table_name: &str,
    row: DatabaseGridRow,
) -> Result<DatabaseChangeStatement, String> {
    if row.cells.len() != metadata.columns.len() {
        return Err("Структура новой строки не совпадает со структурой таблицы".to_string());
    }
    let mut columns = Vec::new();
    let mut values = Vec::new();
    let mut parameters = Vec::new();
    let mut parameter_index = 1usize;
    for (column, cell) in metadata.columns.iter().zip(row.cells.iter()) {
        if column.generated || column.identity && matches!(cell.value, DatabaseCellValue::Default) {
            continue;
        }
        columns.push(quote_pg_identifier(&column.name));
        match &cell.value {
            DatabaseCellValue::Default => values.push("DEFAULT".to_string()),
            value => {
                values.push(parameter_expression(parameter_index, &column.type_name));
                parameters.push(change_parameter(value, &column.type_name));
                parameter_index += 1;
            }
        }
    }
    let returning = returning_clause(metadata);
    let sql = if columns.is_empty() {
        format!(
            "INSERT INTO public.{} DEFAULT VALUES{}",
            quote_pg_identifier(table_name),
            returning
        )
    } else {
        format!(
            "INSERT INTO public.{} ({}) VALUES ({}){}",
            quote_pg_identifier(table_name),
            columns.join(", "),
            values.join(", "),
            returning
        )
    };
    Ok(DatabaseChangeStatement {
        kind: DatabaseChangeKind::Insert,
        sql,
        parameters,
        changed_cells: row.cells.iter().filter(|cell| !matches!(cell.value, DatabaseCellValue::Default)).count(),
        row_label: format!("new row {}", row.absolute_index + 1),
    })
}

fn build_update(
    metadata: &DatabaseTableMetadata,
    table_name: &str,
    row: DatabaseGridRow,
) -> Result<DatabaseChangeStatement, String> {
    let mut assignments = Vec::new();
    let mut parameters = Vec::new();
    let mut parameter_index = 1usize;
    let mut changed_cells = 0usize;
    for (column, cell) in metadata.columns.iter().zip(row.cells.iter()) {
        if !cell.dirty || !column.editable() {
            continue;
        }
        let identifier = quote_pg_identifier(&column.name);
        match &cell.value {
            DatabaseCellValue::Default => assignments.push(format!("{identifier} = DEFAULT")),
            value => {
                assignments.push(format!(
                    "{identifier} = {}",
                    parameter_expression(parameter_index, &column.type_name)
                ));
                parameters.push(change_parameter(value, &column.type_name));
                parameter_index += 1;
            }
        }
        changed_cells += 1;
    }
    if assignments.is_empty() {
        return Err("Строка не содержит изменённых редактируемых ячеек".to_string());
    }
    let where_sql = primary_key_predicate(metadata, &row, &mut parameters, &mut parameter_index)?;
    let xmin = row.xmin.clone().ok_or_else(|| "У строки отсутствует xmin".to_string())?;
    let xmin_index = parameter_index;
    parameters.push(DatabaseChangeParameter {
        value: Some(xmin.clone()),
        type_name: "xid".to_string(),
        preview: xmin,
    });
    let sql = format!(
        "UPDATE public.{} SET {} WHERE {} AND xmin = ${}::text::xid{}",
        quote_pg_identifier(table_name),
        assignments.join(", "),
        where_sql,
        xmin_index,
        returning_clause(metadata)
    );
    Ok(DatabaseChangeStatement {
        kind: DatabaseChangeKind::Update,
        sql,
        parameters,
        changed_cells,
        row_label: format!("row {}", row.absolute_index + 1),
    })
}

fn build_delete(
    metadata: &DatabaseTableMetadata,
    table_name: &str,
    row: DatabaseGridRow,
) -> Result<DatabaseChangeStatement, String> {
    let mut parameters = Vec::new();
    let mut parameter_index = 1usize;
    let where_sql = primary_key_predicate(metadata, &row, &mut parameters, &mut parameter_index)?;
    let xmin = row.xmin.clone().ok_or_else(|| "У строки отсутствует xmin".to_string())?;
    let xmin_index = parameter_index;
    parameters.push(DatabaseChangeParameter {
        value: Some(xmin.clone()),
        type_name: "xid".to_string(),
        preview: xmin,
    });
    let sql = format!(
        "DELETE FROM public.{} WHERE {} AND xmin = ${}::text::xid{}",
        quote_pg_identifier(table_name),
        where_sql,
        xmin_index,
        returning_clause(metadata)
    );
    Ok(DatabaseChangeStatement {
        kind: DatabaseChangeKind::Delete,
        sql,
        parameters,
        changed_cells: 0,
        row_label: format!("row {}", row.absolute_index + 1),
    })
}

fn primary_key_predicate(
    metadata: &DatabaseTableMetadata,
    row: &DatabaseGridRow,
    parameters: &mut Vec<DatabaseChangeParameter>,
    parameter_index: &mut usize,
) -> Result<String, String> {
    let mut predicates = Vec::new();
    for key in &metadata.primary_key_columns {
        let column_index = metadata
            .columns
            .iter()
            .position(|column| &column.name == key)
            .ok_or_else(|| format!("Primary key column {key} отсутствует в metadata"))?;
        let column = &metadata.columns[column_index];
        let cell = row
            .cells
            .get(column_index)
            .ok_or_else(|| "Строка не соответствует metadata".to_string())?;
        let value = &cell.original;
        let identifier = quote_pg_identifier(&column.name);
        if matches!(value, DatabaseCellValue::Null) {
            predicates.push(format!("{identifier} IS NULL"));
        } else {
            predicates.push(format!(
                "{identifier} = {}",
                parameter_expression(*parameter_index, &column.type_name)
            ));
            parameters.push(change_parameter(value, &column.type_name));
            *parameter_index += 1;
        }
    }
    if predicates.is_empty() {
        return Err("Таблица не имеет primary key".to_string());
    }
    Ok(predicates.join(" AND "))
}

fn returning_clause(metadata: &DatabaseTableMetadata) -> String {
    let values = metadata
        .columns
        .iter()
        .map(|column| {
            let id = quote_pg_identifier(&column.name);
            if column.type_kind == super::DatabaseTypeKind::Bytea {
                format!("CASE WHEN {id} IS NULL THEN NULL ELSE octet_length({id})::text END AS {id}")
            } else {
                format!("{id}::text AS {id}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(" RETURNING {values}, xmin::text AS __rriter_xmin")
}

fn parameter_expression(index: usize, type_name: &str) -> String {
    format!("${index}::text::{type_name}")
}

fn change_parameter(value: &DatabaseCellValue, type_name: &str) -> DatabaseChangeParameter {
    match value {
        DatabaseCellValue::Null => DatabaseChangeParameter {
            value: None,
            type_name: type_name.to_string(),
            preview: "<NULL>".to_string(),
        },
        DatabaseCellValue::Default => DatabaseChangeParameter {
            value: None,
            type_name: type_name.to_string(),
            preview: "<default>".to_string(),
        },
        DatabaseCellValue::Text(value)
        | DatabaseCellValue::Enum(value)
        | DatabaseCellValue::DateTime(value) => DatabaseChangeParameter {
            value: Some(value.clone()),
            type_name: type_name.to_string(),
            preview: bounded_preview(value),
        },
        DatabaseCellValue::Boolean(value) => DatabaseChangeParameter {
            value: Some(value.to_string()),
            type_name: type_name.to_string(),
            preview: value.to_string(),
        },
        DatabaseCellValue::ByteaPreview(value) => DatabaseChangeParameter {
            value: None,
            type_name: type_name.to_string(),
            preview: format!("<bytea {} bytes>", value.total_bytes),
        },
    }
}

fn bounded_preview(value: &str) -> String {
    const MAX: usize = 512;
    if value.len() <= MAX {
        return value.replace('\n', "\\n");
    }
    let mut end = MAX;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… <{} bytes>", value[..end].replace('\n', "\\n"), value.len())
}

pub async fn count_public_table_rows(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    metadata: &DatabaseTableMetadata,
    where_clause: &str,
    generation: DatabaseGeneration,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseTableCountResult, DatabaseBackendError> {
    validate_table_fragment(where_clause, "WHERE")
        .map_err(DatabaseBackendError::InvalidConfiguration)?;
    let session = connect_postgres(connection, secrets, database_name, settings, ssh_options).await?;
    let mut sql = format!(
        "SELECT COUNT(*)::int8 FROM public.{}",
        quote_pg_identifier(&metadata.table_name)
    );
    if !where_clause.trim().is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(where_clause.trim());
    }
    let timeout = Duration::from_secs(settings.statement_timeout_seconds);
    let row = tokio::time::timeout(timeout, session.client.query_one(&sql, &[]))
        .await
        .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL table COUNT"))??;
    let count = row.get::<_, i64>(0).max(0) as u64;
    Ok(DatabaseTableCountResult {
        database_name: database_name.to_string(),
        table_name: metadata.table_name.clone(),
        generation,
        count,
        notices: session.notices.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn load_public_table_chunk(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    metadata: &DatabaseTableMetadata,
    where_clause: &str,
    order_by: &str,
    page: usize,
    limit: usize,
    chunk_index: usize,
    generation: DatabaseGeneration,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseTableChunkResult, DatabaseBackendError> {
    validate_table_fragment(where_clause, "WHERE")
        .map_err(DatabaseBackendError::InvalidConfiguration)?;
    validate_table_fragment(order_by, "ORDER BY")
        .map_err(DatabaseBackendError::InvalidConfiguration)?;
    let session = connect_postgres(connection, secrets, database_name, settings, ssh_options).await?;
    let page_offset = page.saturating_mul(limit);
    let chunk_offset = chunk_index.saturating_mul(DATABASE_CHUNK_SIZE);
    if chunk_offset >= limit {
        return Err(DatabaseBackendError::InvalidConfiguration(
            "requested chunk is outside the current page".to_string(),
        ));
    }
    let chunk_size = DATABASE_CHUNK_SIZE.min(limit - chunk_offset);
    let absolute_offset = page_offset.saturating_add(chunk_offset);
    let select_columns = metadata
        .columns
        .iter()
        .map(|column| select_expression(column, "__rriter_source"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "SELECT {select_columns}, __rriter_source.xmin::text AS __rriter_xmin FROM public.{} AS __rriter_source",
        quote_pg_identifier(&metadata.table_name)
    );
    if !where_clause.trim().is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(where_clause.trim());
    }
    let effective_order = if order_by.trim().is_empty() {
        metadata
            .primary_key_columns
            .iter()
            .map(|column| format!("__rriter_source.{} ASC", quote_pg_identifier(column)))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        order_by.trim().to_string()
    };
    if !effective_order.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&effective_order);
    }
    sql.push_str(&format!(" OFFSET {absolute_offset} LIMIT {chunk_size}"));
    let timeout = Duration::from_secs(settings.statement_timeout_seconds);
    let rows = tokio::time::timeout(timeout, session.client.query(&sql, &[]))
        .await
        .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL table chunk"))??;
    let mut grid_rows = Vec::with_capacity(rows.len());
    let mut estimated_bytes = 0usize;
    for (row_index, row) in rows.into_iter().enumerate() {
        let mut cells = Vec::with_capacity(metadata.columns.len());
        for (column_index, column) in metadata.columns.iter().enumerate() {
            let value: Option<String> = row.get(column_index);
            let value = decode_cell_value(column, value);
            cells.push(DatabaseGridCell::new(value));
        }
        let xmin: String = row.get(metadata.columns.len());
        let grid_row = DatabaseGridRow {
            absolute_index: absolute_offset + row_index,
            cells,
            xmin: Some(xmin),
            state: DatabaseRowState::Clean,
        };
        estimated_bytes = estimated_bytes.saturating_add(grid_row.estimated_bytes());
        grid_rows.push(grid_row);
    }
    Ok(DatabaseTableChunkResult {
        database_name: database_name.to_string(),
        table_name: metadata.table_name.clone(),
        generation,
        chunk: DatabaseTableChunk {
            generation,
            chunk_index,
            rows: grid_rows,
            estimated_bytes,
        },
        notices: session.notices.clone(),
    })
}

fn select_expression(column: &DatabaseColumnInfo, source_alias: &str) -> String {
    let id = quote_pg_identifier(&column.name);
    let source = format!("{source_alias}.{id}");
    if column.type_kind == super::DatabaseTypeKind::Bytea {
        format!(
            "CASE WHEN {source} IS NULL THEN NULL ELSE octet_length({source})::text || ':' || encode(substring({source} FROM 1 FOR {}), 'hex') END AS {id}",
            super::MAX_BYTEA_PREVIEW_BYTES
        )
    } else {
        format!("{source}::text AS {id}")
    }
}

fn decode_cell_value(column: &DatabaseColumnInfo, value: Option<String>) -> DatabaseCellValue {
    let Some(value) = value else {
        return DatabaseCellValue::Null;
    };
    match column.type_kind {
        super::DatabaseTypeKind::Boolean => DatabaseCellValue::Boolean(value == "true" || value == "t"),
        super::DatabaseTypeKind::Enum => DatabaseCellValue::Enum(value),
        super::DatabaseTypeKind::Date
        | super::DatabaseTypeKind::Time
        | super::DatabaseTypeKind::Timestamp
        | super::DatabaseTypeKind::TimestampTz => DatabaseCellValue::DateTime(value),
        super::DatabaseTypeKind::Bytea => DatabaseCellValue::ByteaPreview(parse_bytea_preview(&value)),
        _ => DatabaseCellValue::Text(value),
    }
}

pub async fn begin_table_transaction(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    plan: &DatabaseChangePlan,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<(PostgresSession, DatabasePreparedTableTransaction), DatabaseBackendError> {
    let session = connect_postgres(connection, secrets, &plan.database_name, settings, ssh_options).await?;
    session.client.batch_execute("BEGIN").await?;
    let set_local = format!(
        "SET LOCAL statement_timeout = '{}s'; SET LOCAL lock_timeout = '{}s'; SET LOCAL idle_in_transaction_session_timeout = '{}s'",
        settings.statement_timeout_seconds,
        settings.lock_timeout_seconds,
        settings.transaction_review_timeout_seconds.saturating_add(15)
    );
    if let Err(error) = session.client.batch_execute(&set_local).await {
        let _ = session.client.batch_execute("ROLLBACK").await;
        return Err(error.into());
    }

    let mut summary = DatabaseTableReviewSummary {
        inserted_rows: 0,
        updated_rows: 0,
        changed_cells: 0,
        deleted_rows: 0,
        detail_rows: Vec::new(),
        notices: Vec::new(),
        truncated_details: false,
    };
    let mut detail_cells = 0usize;
    for statement in &plan.statements {
        let values: Vec<Option<String>> = statement
            .parameters
            .iter()
            .map(|parameter| parameter.value.clone())
            .collect();
        let refs: Vec<&(dyn ToSql + Sync)> = values
            .iter()
            .map(|value| value as &(dyn ToSql + Sync))
            .collect();
        let rows = match session.client.query(&statement.sql, &refs).await {
            Ok(rows) => rows,
            Err(error) => {
                let _ = session.client.batch_execute("ROLLBACK").await;
                return Err(error.into());
            }
        };
        if matches!(statement.kind, DatabaseChangeKind::Update | DatabaseChangeKind::Delete)
            && rows.len() != 1
        {
            let _ = session.client.batch_execute("ROLLBACK").await;
            return Err(DatabaseBackendError::InvalidConfiguration(
                "Строка была изменена или удалена другим клиентом".to_string(),
            ));
        }
        match statement.kind {
            DatabaseChangeKind::Insert => summary.inserted_rows += rows.len(),
            DatabaseChangeKind::Update => {
                summary.updated_rows += rows.len();
                summary.changed_cells += statement.changed_cells;
            }
            DatabaseChangeKind::Delete => summary.deleted_rows += rows.len(),
        }
        for row in rows {
            if summary.detail_rows.len() >= MAX_REVIEW_DETAIL_ROWS {
                summary.truncated_details = true;
                break;
            }
            let mut values = Vec::new();
            for index in 0..row.len().saturating_sub(1) {
                if detail_cells >= MAX_REVIEW_CELL_DIFFS {
                    summary.truncated_details = true;
                    break;
                }
                let value: Option<String> = row.get(index);
                values.push(value.map_or_else(
                    || "<NULL>".to_string(),
                    |value| bounded_preview(&value),
                ));
                detail_cells += 1;
            }
            if !values.is_empty() {
                summary
                    .detail_rows
                    .push(format!("{}: {}", statement.row_label, values.join(" | ")));
            }
        }
    }
    let notices = session.notices.clone();
    summary.notices = notices.iter().map(database_backend_notice_text).collect();
    Ok((session, DatabasePreparedTableTransaction { summary, notices }))
}

fn database_backend_notice_text(notice: &DatabaseBackendNotice) -> String {
    match notice {
        DatabaseBackendNotice::BuiltinSshFallback { reason } => {
            format!("Используется встроенный SSH: {reason}")
        }
        DatabaseBackendNotice::NativeCertificateWarnings { count } => {
            format!("Системное хранилище сертификатов вернуло предупреждений: {count}")
        }
    }
}

pub async fn finish_table_transaction(
    session: &PostgresSession,
    commit: bool,
) -> Result<(), DatabaseBackendError> {
    session
        .client
        .batch_execute(if commit { "COMMIT" } else { "ROLLBACK" })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::database::{DatabaseColumnInfo, DatabaseTypeKind};

    fn metadata() -> DatabaseTableMetadata {
        DatabaseTableMetadata {
            database_name: "db".to_string(),
            table_name: "items".to_string(),
            columns: vec![
                DatabaseColumnInfo {
                    ordinal: 1,
                    name: "id".to_string(),
                    type_name: "integer".to_string(),
                    type_oid: 23,
                    type_kind: DatabaseTypeKind::Other,
                    nullable: false,
                    default_expression: Some("nextval('items_id_seq')".to_string()),
                    identity: true,
                    generated: false,
                    primary_key: true,
                    enum_values: Vec::new(),
                },
                DatabaseColumnInfo {
                    ordinal: 2,
                    name: "name".to_string(),
                    type_name: "text".to_string(),
                    type_oid: 25,
                    type_kind: DatabaseTypeKind::Other,
                    nullable: true,
                    default_expression: None,
                    identity: false,
                    generated: false,
                    primary_key: false,
                    enum_values: Vec::new(),
                },
            ],
            primary_key_columns: vec!["id".to_string()],
            editable: true,
            read_only_reason: None,
            notices: Vec::new(),
        }
    }

    fn row(state: DatabaseRowState) -> DatabaseGridRow {
        DatabaseGridRow {
            absolute_index: 0,
            cells: vec![
                DatabaseGridCell::new(DatabaseCellValue::Text("1".to_string())),
                DatabaseGridCell::new(DatabaseCellValue::Text("old".to_string())),
            ],
            xmin: Some("7".to_string()),
            state,
        }
    }

    #[test]
    fn calendar_helpers_handle_leap_years_and_month_edges() {
        assert_eq!(database_days_in_month(2024, 2), 29);
        assert_eq!(database_days_in_month(2025, 2), 28);
        assert_eq!(database_shift_calendar_month(2025, 1, -1), (2024, 12));
        assert_eq!(database_shift_calendar_month(2025, 12, 1), (2026, 1));
        assert_eq!(database_calendar_weekday_monday(2026, 7, 1), 2);
    }

    #[test]
    fn unavailable_table_text_reuses_selectable_input_state() {
        let mut state = DatabaseTableTabState::default();
        state.loading = false;
        assert_eq!(
            state.unavailable_text.text(),
            DATABASE_TABLE_DISCONNECTED_MESSAGE
        );
        assert_eq!(state.unavailable_text.cursor, 0);
        state.unavailable_text_focused = true;
        state.unavailable_text_dragging = true;
        state.set_unavailable_text("connection refused");
        assert_eq!(state.unavailable_text.text(), "connection refused");
        assert_eq!(state.unavailable_text.cursor, 0);
        assert!(!state.unavailable_text_focused);
        assert!(!state.unavailable_text_dragging);
        state.unavailable_text.select_all();
        assert_eq!(
            state.unavailable_text.selected_text(),
            Some("connection refused")
        );
        assert!(!state.unavailable_text_focused);
        assert!(!state.unavailable_text_dragging);
    }

    #[test]
    fn calendar_parser_accepts_valid_date_prefix_only() {
        assert_eq!(database_calendar_year_month("2026-07-16 12:30:00"), Some((2026, 7)));
        assert_eq!(database_calendar_year_month("2025-02-29"), None);
        assert_eq!(database_calendar_year_month("not-a-date"), None);
    }

    #[test]
    fn fragments_reject_only_top_level_statement_separator() {
        assert!(validate_table_fragment("id=10", "WHERE").is_ok());
        assert!(validate_table_fragment("name = 'a;b'", "WHERE").is_ok());
        assert!(validate_table_fragment("name = 'a'; DROP TABLE x", "WHERE").is_err());
        assert!(validate_table_fragment("AND broken", "WHERE").is_err());
        assert_eq!(
            validate_table_fragment("id == 1", "WHERE").unwrap_err(),
            "WHERE содержит недопустимый оператор ==; используйте ="
        );
        assert!(validate_table_fragment("name = 'a==b'", "WHERE").is_ok());
    }

    #[test]
    fn timed_table_notice_expires_without_becoming_a_persistent_error() {
        let mut state = DatabaseTableTabState::default();
        state.show_timed_notice("busy");
        assert_eq!(state.active_notice(), Some("busy"));
        state.notice_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
        assert_eq!(state.active_notice(), None);
        assert!(state.error.is_none());
    }

    #[test]
    fn sql_preview_formatter_breaks_generated_statements_into_readable_lines() {
        let formatted = crate::app::database::format_database_sql(
            "UPDATE public.\"items\" SET \"name\" = 'new' WHERE \"id\" = 10 RETURNING \"id\";",
        )
        .unwrap();
        assert!(formatted.contains("\nWHERE "));
        assert!(formatted.contains("\nRETURNING "));
    }

    #[test]
    fn generated_sort_targets_typed_source_column() {
        let mut view = crate::app::database::DatabaseTableViewState::default();
        view.sorted_column = Some("User ID".to_string());
        view.sort_direction = Some(crate::app::database::DatabaseSortDirection::Asc);
        view.order_by = "\"User ID\" ASC".to_string();
        assert_eq!(
            database_table_effective_order_by(&view),
            "__rriter_source.\"User ID\" ASC"
        );
        view.sorted_column = None;
        view.sort_direction = None;
        assert_eq!(database_table_effective_order_by(&view), "\"User ID\" ASC");
    }

    #[test]
    fn selected_columns_keep_display_alias_but_qualify_source() {
        let column = &metadata().columns[0];
        assert_eq!(
            select_expression(column, "__rriter_source"),
            "__rriter_source.\"id\"::text AS \"id\""
        );
    }

    #[test]
    fn update_uses_primary_key_xmin_and_parameterized_values() {
        let mut row = row(DatabaseRowState::Clean);
        row.cells[1].set(DatabaseCellValue::Text("new".to_string()));
        let plan = build_table_change_plan(
            &metadata(),
            "db",
            "items",
            vec![DatabaseChangePlanOperation::Update(row)],
        )
        .unwrap();
        let sql = &plan.statements[0].sql;
        assert!(sql.contains("UPDATE public.\"items\""));
        assert!(sql.contains("\"id\" = $2::text::integer"));
        assert!(sql.contains("xmin = $3::text::xid"));
        assert!(!sql.contains("new"));
        assert_eq!(plan.statements[0].parameters.len(), 3);
        assert!(plan.preview.contains("«Применить» выполнит COMMIT"));
    }

    #[test]
    fn insert_keeps_identity_as_default_and_returns_actual_values() {
        let mut row = row(DatabaseRowState::Added);
        row.cells[0].set(DatabaseCellValue::Default);
        row.cells[1].set(DatabaseCellValue::Text("new".to_string()));
        let plan = build_table_change_plan(
            &metadata(),
            "db",
            "items",
            vec![DatabaseChangePlanOperation::Insert(row)],
        )
        .unwrap();
        assert!(plan.statements[0]
            .sql
            .starts_with("INSERT INTO public.\"items\" (\"name\") VALUES"));
        assert!(plan.statements[0].sql.contains("RETURNING \"id\"::text"));
    }

    #[test]
    fn delete_requires_primary_key_and_xmin() {
        let plan = build_table_change_plan(
            &metadata(),
            "db",
            "items",
            vec![DatabaseChangePlanOperation::Delete(row(DatabaseRowState::Deleted))],
        )
        .unwrap();
        assert!(plan.statements[0].sql.starts_with("DELETE FROM public.\"items\""));
        assert!(plan.statements[0].sql.contains("xmin"));
    }
}
