use super::database_postgres::{DatabaseBackendError, DatabaseBackendNotice, connect_postgres};
use super::database_ssh::SshConnectOptions;
use super::{
    DatabaseConnectionConfig, DatabaseSecretBundle, DatabaseSettings, MAX_COLUMNS_PER_RESULT,
};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio_postgres::Row;

const TABLE_METADATA_SQL: &str = r#"
SELECT
    a.attnum::int4,
    a.attname,
    format_type(a.atttypid, a.atttypmod),
    a.atttypid::int8,
    NOT a.attnotnull,
    pg_get_expr(ad.adbin, ad.adrelid),
    a.attidentity::text,
    a.attgenerated::text,
    EXISTS (
        SELECT 1
        FROM pg_index i
        WHERE i.indrelid = c.oid
          AND i.indisprimary
          AND a.attnum = ANY(i.indkey)
    ) AS is_primary_key,
    t.typtype::text
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_attribute a ON a.attrelid = c.oid
JOIN pg_type t ON t.oid = a.atttypid
LEFT JOIN pg_attrdef ad ON ad.adrelid = c.oid AND ad.adnum = a.attnum
WHERE n.nspname = 'public'
  AND c.relname = $1
  AND c.relkind IN ('r', 'p')
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY a.attnum
"#;

const ENUM_VALUES_SQL: &str = r#"
SELECT e.enumtypid::int8, e.enumlabel
FROM pg_enum e
WHERE e.enumtypid = ANY($1::oid[])
ORDER BY e.enumtypid, e.enumsortorder
"#;

const CONSTRAINTS_SQL: &str = r#"
SELECT con.conname, pg_get_constraintdef(con.oid, true)
FROM pg_constraint con
JOIN pg_class c ON c.oid = con.conrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'public'
  AND c.relname = $1
ORDER BY con.contype, con.conname
"#;

const INDEXES_SQL: &str = r#"
SELECT i.relname, pg_get_indexdef(i.oid)
FROM pg_index x
JOIN pg_class t ON t.oid = x.indrelid
JOIN pg_namespace n ON n.oid = t.relnamespace
JOIN pg_class i ON i.oid = x.indexrelid
WHERE n.nspname = 'public'
  AND t.relname = $1
  AND NOT x.indisprimary
  AND NOT EXISTS (
      SELECT 1 FROM pg_constraint con WHERE con.conindid = i.oid
  )
ORDER BY i.relname
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseTypeKind {
    Boolean,
    Enum,
    Date,
    Time,
    Timestamp,
    TimestampTz,
    Json,
    Jsonb,
    Bytea,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseColumnInfo {
    pub ordinal: usize,
    pub name: String,
    pub type_name: String,
    pub type_oid: u32,
    pub type_kind: DatabaseTypeKind,
    pub nullable: bool,
    pub default_expression: Option<String>,
    pub identity: bool,
    pub generated: bool,
    pub primary_key: bool,
    pub enum_values: Vec<String>,
}

impl DatabaseColumnInfo {
    pub fn editable(&self) -> bool {
        !self.primary_key
            && !self.identity
            && !self.generated
            && self.type_kind != DatabaseTypeKind::Bytea
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableMetadata {
    pub database_name: String,
    pub table_name: String,
    pub columns: Vec<DatabaseColumnInfo>,
    pub primary_key_columns: Vec<String>,
    pub editable: bool,
    pub read_only_reason: Option<String>,
    pub notices: Vec<DatabaseBackendNotice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseDdlResult {
    pub database_name: String,
    pub table_name: String,
    pub ddl: String,
    pub notices: Vec<DatabaseBackendNotice>,
}

pub async fn load_public_table_metadata(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    table_name: &str,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseTableMetadata, DatabaseBackendError> {
    validate_table_name(table_name)?;
    let session = connect_postgres(connection, secrets, database_name, settings, ssh_options).await?;
    let timeout = Duration::from_secs(settings.statement_timeout_seconds);
    // Internal read: intentionally autocommit, without BEGIN/review.
    let rows = tokio::time::timeout(timeout, session.client.query(TABLE_METADATA_SQL, &[&table_name]))
        .await
        .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL table metadata"))??;
    if rows.is_empty() {
        return Err(DatabaseBackendError::InvalidConfiguration(format!(
            "public table {} does not exist or is not a regular table",
            quote_pg_identifier(table_name)
        )));
    }
    if rows.len() > MAX_COLUMNS_PER_RESULT {
        return Err(DatabaseBackendError::LimitExceeded(
            "table contains more than 512 columns",
        ));
    }

    let enum_oids: Vec<u32> = rows
        .iter()
        .filter_map(|row| {
            let typtype: String = row.get(9);
            (typtype == "e").then(|| row.get::<_, i64>(3) as u32)
        })
        .collect();
    let enum_values = load_enum_values(&session.client, &enum_oids, timeout).await?;
    let mut columns = Vec::with_capacity(rows.len());
    for row in rows {
        let type_oid = row.get::<_, i64>(3) as u32;
        let type_name: String = row.get(2);
        let typtype: String = row.get(9);
        columns.push(DatabaseColumnInfo {
            ordinal: row.get::<_, i32>(0) as usize,
            name: row.get(1),
            type_kind: classify_type(&type_name, &typtype),
            type_name,
            type_oid,
            nullable: row.get(4),
            default_expression: row.get(5),
            identity: !row.get::<_, String>(6).is_empty(),
            generated: !row.get::<_, String>(7).is_empty(),
            primary_key: row.get(8),
            enum_values: enum_values.get(&type_oid).cloned().unwrap_or_default(),
        });
    }
    let primary_key_columns: Vec<String> = columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.clone())
        .collect();
    let editable = !primary_key_columns.is_empty();
    Ok(DatabaseTableMetadata {
        database_name: database_name.to_string(),
        table_name: table_name.to_string(),
        columns,
        primary_key_columns,
        editable,
        read_only_reason: (!editable).then(|| {
            "Изменение запрещено: таблица не имеет primary key, поэтому RRiter не может безопасно адресовать строку."
                .to_string()
        }),
        notices: session.notices.clone(),
    })
}

pub async fn reconstruct_public_table_ddl(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    table_name: &str,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseDdlResult, DatabaseBackendError> {
    let metadata = load_public_table_metadata(
        connection,
        secrets,
        database_name,
        table_name,
        settings,
        ssh_options,
    )
    .await?;
    let mut session = connect_postgres(connection, secrets, database_name, settings, ssh_options).await?;
    let timeout = Duration::from_secs(settings.statement_timeout_seconds);
    // Internal catalog reads: intentionally autocommit.
    let constraints = tokio::time::timeout(
        timeout,
        session.client.query(CONSTRAINTS_SQL, &[&table_name]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL table constraints"))??;
    let indexes = tokio::time::timeout(timeout, session.client.query(INDEXES_SQL, &[&table_name]))
        .await
        .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL table indexes"))??;
    let ddl = build_ddl(&metadata, &constraints, &indexes);
    let mut notices = metadata.notices;
    notices.extend(std::mem::take(&mut session.notices));
    Ok(DatabaseDdlResult {
        database_name: database_name.to_string(),
        table_name: table_name.to_string(),
        ddl,
        notices,
    })
}

async fn load_enum_values(
    client: &tokio_postgres::Client,
    enum_oids: &[u32],
    timeout: Duration,
) -> Result<BTreeMap<u32, Vec<String>>, DatabaseBackendError> {
    if enum_oids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let rows = tokio::time::timeout(timeout, client.query(ENUM_VALUES_SQL, &[&enum_oids]))
        .await
        .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL enum metadata"))??;
    let mut output = BTreeMap::new();
    for row in rows {
        output
            .entry(row.get::<_, i64>(0) as u32)
            .or_insert_with(Vec::new)
            .push(row.get(1));
    }
    Ok(output)
}

fn validate_table_name(table_name: &str) -> Result<(), DatabaseBackendError> {
    let trimmed = table_name.trim();
    if trimmed.is_empty() || trimmed.len() > 256 || trimmed.chars().any(char::is_control) {
        return Err(DatabaseBackendError::InvalidConfiguration(
            "table name is empty, too long, or contains control characters".to_string(),
        ));
    }
    Ok(())
}

pub fn quote_pg_identifier(identifier: &str) -> String {
    let mut output = String::with_capacity(identifier.len() + 2);
    output.push('"');
    for ch in identifier.chars() {
        if ch == '"' {
            output.push('"');
        }
        output.push(ch);
    }
    output.push('"');
    output
}

fn classify_type(type_name: &str, typtype: &str) -> DatabaseTypeKind {
    if typtype == "e" {
        return DatabaseTypeKind::Enum;
    }
    match type_name.trim().to_ascii_lowercase().as_str() {
        "boolean" | "bool" => DatabaseTypeKind::Boolean,
        "date" => DatabaseTypeKind::Date,
        "time without time zone" | "time with time zone" | "time" | "timetz" => {
            DatabaseTypeKind::Time
        }
        "timestamp without time zone" | "timestamp" => DatabaseTypeKind::Timestamp,
        "timestamp with time zone" | "timestamptz" => DatabaseTypeKind::TimestampTz,
        "json" => DatabaseTypeKind::Json,
        "jsonb" => DatabaseTypeKind::Jsonb,
        "bytea" => DatabaseTypeKind::Bytea,
        _ => DatabaseTypeKind::Other,
    }
}

fn build_ddl(metadata: &DatabaseTableMetadata, constraints: &[Row], indexes: &[Row]) -> String {
    let mut lines = Vec::with_capacity(metadata.columns.len() + constraints.len());
    for column in &metadata.columns {
        let mut line = format!(
            "    {} {}",
            quote_pg_identifier(&column.name),
            column.type_name
        );
        if column.identity {
            line.push_str(" GENERATED BY DEFAULT AS IDENTITY");
        } else if column.generated {
            if let Some(expression) = &column.default_expression {
                line.push_str(" GENERATED ALWAYS AS (");
                line.push_str(expression);
                line.push_str(") STORED");
            }
        } else if let Some(default_expression) = &column.default_expression {
            line.push_str(" DEFAULT ");
            line.push_str(default_expression);
        }
        if !column.nullable {
            line.push_str(" NOT NULL");
        }
        lines.push(line);
    }
    for row in constraints {
        let name: String = row.get(0);
        let definition: String = row.get(1);
        lines.push(format!(
            "    CONSTRAINT {} {}",
            quote_pg_identifier(&name),
            definition
        ));
    }
    let mut ddl = format!(
        "CREATE TABLE {}.{} (\n{}\n);",
        quote_pg_identifier("public"),
        quote_pg_identifier(&metadata.table_name),
        lines.join(",\n")
    );
    for row in indexes {
        let definition: String = row.get(1);
        ddl.push_str("\n\n");
        ddl.push_str(&definition);
        if !definition.trim_end().ends_with(';') {
            ddl.push(';');
        }
    }
    ddl
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(columns: Vec<DatabaseColumnInfo>) -> DatabaseTableMetadata {
        DatabaseTableMetadata {
            database_name: "app".to_string(),
            table_name: "weird\"table".to_string(),
            primary_key_columns: vec!["id".to_string()],
            editable: true,
            read_only_reason: None,
            notices: Vec::new(),
            columns,
        }
    }

    #[test]
    fn identifiers_are_quoted_for_postgresql() {
        assert_eq!(quote_pg_identifier("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn type_classification_covers_special_editors() {
        assert_eq!(classify_type("boolean", "b"), DatabaseTypeKind::Boolean);
        assert_eq!(classify_type("jsonb", "b"), DatabaseTypeKind::Jsonb);
        assert_eq!(classify_type("custom", "e"), DatabaseTypeKind::Enum);
        assert_eq!(classify_type("bytea", "b"), DatabaseTypeKind::Bytea);
    }

    #[test]
    fn primary_key_identity_generated_and_bytea_columns_are_locked() {
        let mut value = DatabaseColumnInfo {
            ordinal: 1,
            name: "value".to_string(),
            type_name: "text".to_string(),
            type_oid: 25,
            type_kind: DatabaseTypeKind::Other,
            nullable: true,
            default_expression: None,
            identity: false,
            generated: false,
            primary_key: false,
            enum_values: Vec::new(),
        };
        assert!(value.editable());
        value.primary_key = true;
        assert!(!value.editable());
        value.primary_key = false;
        value.identity = true;
        assert!(!value.editable());
        value.identity = false;
        value.generated = true;
        assert!(!value.editable());
        value.generated = false;
        value.type_kind = DatabaseTypeKind::Bytea;
        assert!(!value.editable());
    }

    #[test]
    fn metadata_without_primary_key_is_read_only() {
        let columns = vec![DatabaseColumnInfo {
            ordinal: 1,
            name: "value".to_string(),
            type_name: "text".to_string(),
            type_oid: 25,
            type_kind: DatabaseTypeKind::Other,
            nullable: true,
            default_expression: None,
            identity: false,
            generated: false,
            primary_key: false,
            enum_values: Vec::new(),
        }];
        let primary_key_columns: Vec<String> = columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect();
        assert!(primary_key_columns.is_empty());
    }

    #[test]
    fn ddl_builder_preserves_defaults_and_identifier_quoting() {
        let meta = metadata(vec![DatabaseColumnInfo {
            ordinal: 1,
            name: "id".to_string(),
            type_name: "bigint".to_string(),
            type_oid: 20,
            type_kind: DatabaseTypeKind::Other,
            nullable: false,
            default_expression: Some("nextval('seq'::regclass)".to_string()),
            identity: false,
            generated: false,
            primary_key: true,
            enum_values: Vec::new(),
        }]);
        let ddl = build_ddl(&meta, &[], &[]);
        assert!(ddl.contains("CREATE TABLE \"public\".\"weird\"\"table\""));
        assert!(ddl.contains("DEFAULT nextval('seq'::regclass) NOT NULL"));
    }

    #[test]
    fn internal_catalog_queries_do_not_open_transactions() {
        for sql in [TABLE_METADATA_SQL, ENUM_VALUES_SQL, CONSTRAINTS_SQL, INDEXES_SQL] {
            let lower = sql.to_ascii_lowercase();
            assert!(!lower.contains("begin"));
            assert!(!lower.contains("commit"));
            assert!(!lower.contains("rollback"));
        }
    }
}
