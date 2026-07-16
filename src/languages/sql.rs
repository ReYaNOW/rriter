use std::fmt;
use std::ops::Range;

pub const SQL_KEYWORDS: &[&str] = &[
    "ALL",
    "ALTER",
    "ANALYZE",
    "AND",
    "ANY",
    "AS",
    "ASC",
    "BEGIN",
    "BETWEEN",
    "BY",
    "CASE",
    "CHECK",
    "COMMIT",
    "CONFLICT",
    "CONSTRAINT",
    "CREATE",
    "CROSS",
    "CURRENT",
    "DATABASE",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "DO",
    "DROP",
    "ELSE",
    "END",
    "EXCEPT",
    "EXISTS",
    "EXPLAIN",
    "FALSE",
    "FILTER",
    "FOR",
    "FOREIGN",
    "FROM",
    "FULL",
    "FUNCTION",
    "GENERATED",
    "GRANT",
    "GROUP",
    "HAVING",
    "IF",
    "ILIKE",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTO",
    "IS",
    "JOIN",
    "LATERAL",
    "LEFT",
    "LIKE",
    "LIMIT",
    "LOCK",
    "MERGE",
    "NATURAL",
    "NOT",
    "NULL",
    "NULLS",
    "OFFSET",
    "ON",
    "ONLY",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PARTITION",
    "PRIMARY",
    "REFERENCES",
    "RETURNING",
    "REVOKE",
    "RIGHT",
    "ROLLBACK",
    "ROW",
    "ROWS",
    "SAVEPOINT",
    "SELECT",
    "SET",
    "TABLE",
    "THEN",
    "TO",
    "TRANSACTION",
    "TRUE",
    "TRUNCATE",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USING",
    "VALUES",
    "VIEW",
    "WHEN",
    "WHERE",
    "WINDOW",
    "WITH",
];

pub const SQL_BUILTIN_FUNCTIONS: &[&str] = &[
    "array_agg",
    "avg",
    "coalesce",
    "count",
    "current_date",
    "current_timestamp",
    "date_trunc",
    "greatest",
    "json_agg",
    "json_build_object",
    "jsonb_agg",
    "jsonb_build_object",
    "jsonb_set",
    "least",
    "lower",
    "max",
    "min",
    "now",
    "nullif",
    "row_number",
    "string_agg",
    "substring",
    "sum",
    "to_char",
    "to_date",
    "to_json",
    "to_jsonb",
    "upper",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlStatementKind {
    Query,
    Mutation,
    Definition,
    Explain,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlStatement {
    pub range: Range<usize>,
    pub kind: SqlStatementKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlValidationErrorKind {
    Empty,
    TooManyStatements,
    TransactionControl,
    UnsupportedInManagedTransaction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlValidationError {
    pub kind: SqlValidationErrorKind,
    pub statement_index: Option<usize>,
    pub range: Option<Range<usize>>,
    pub message: &'static str,
}

impl fmt::Display for SqlValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for SqlValidationError {}

#[derive(Debug)]
struct ScannedStatement {
    range: Range<usize>,
    words: Vec<String>,
}

#[derive(Debug)]
enum ScanState {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    LineComment,
    BlockComment { depth: usize },
    DollarQuoted { delimiter: Vec<u8> },
}

pub fn scan_statements(sql: &str) -> Vec<SqlStatement> {
    scanned_statements(sql)
        .into_iter()
        .map(|statement| SqlStatement {
            range: statement.range,
            kind: classify_words(&statement.words),
        })
        .collect()
}

pub fn has_syntax_error(sql: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_sequel::LANGUAGE.into();
    if parser.set_language(&language).is_err() {
        return true;
    }
    parser.parse(sql, None).is_none_or(|tree| tree.root_node().has_error())
}

pub fn contains_top_level_semicolon(sql: &str) -> bool {
    scanned_statements(sql).iter().any(|statement| {
        statement.range.end > statement.range.start
            && sql.as_bytes().get(statement.range.end - 1) == Some(&b';')
    })
}

pub fn statement_range_at(sql: &str, cursor: usize) -> Option<Range<usize>> {
    let cursor = cursor.min(sql.len());
    let statements = scanned_statements(sql);
    if let Some(statement) = statements
        .iter()
        .find(|statement| cursor >= statement.range.start && cursor <= statement.range.end)
    {
        return Some(statement.range.clone());
    }

    statements
        .iter()
        .find(|statement| statement.range.start >= cursor)
        .or_else(|| statements.last())
        .map(|statement| statement.range.clone())
}

pub fn validate_managed_user_sql(sql: &str) -> Result<Vec<SqlStatement>, SqlValidationError> {
    let statements = scanned_statements(sql);
    if statements.is_empty() {
        return Err(SqlValidationError {
            kind: SqlValidationErrorKind::Empty,
            statement_index: None,
            range: None,
            message: "SQL query is empty",
        });
    }
    if statements.len() > 32 {
        return Err(SqlValidationError {
            kind: SqlValidationErrorKind::TooManyStatements,
            statement_index: None,
            range: None,
            message: "SQL script contains more than 32 statements",
        });
    }

    let mut result = Vec::with_capacity(statements.len());
    for (index, statement) in statements.into_iter().enumerate() {
        if is_transaction_control(&statement.words) {
            return Err(SqlValidationError {
                kind: SqlValidationErrorKind::TransactionControl,
                statement_index: Some(index),
                range: Some(statement.range),
                message: "Transaction control commands are managed by RRiter",
            });
        }
        if is_unsupported_in_managed_transaction(&statement.words) {
            return Err(SqlValidationError {
                kind: SqlValidationErrorKind::UnsupportedInManagedTransaction,
                statement_index: Some(index),
                range: Some(statement.range),
                message: "This PostgreSQL command is not supported in managed transactions",
            });
        }
        result.push(SqlStatement {
            range: statement.range,
            kind: classify_words(&statement.words),
        });
    }
    Ok(result)
}

fn classify_words(words: &[String]) -> SqlStatementKind {
    match words.first().map(String::as_str) {
        Some("SELECT" | "SHOW" | "TABLE" | "VALUES" | "WITH") => SqlStatementKind::Query,
        Some("INSERT" | "UPDATE" | "DELETE" | "MERGE") => SqlStatementKind::Mutation,
        Some("CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "COMMENT" | "GRANT" | "REVOKE") => {
            SqlStatementKind::Definition
        }
        Some("EXPLAIN") => SqlStatementKind::Explain,
        _ => SqlStatementKind::Other,
    }
}

fn is_transaction_control(words: &[String]) -> bool {
    let first = words.first().map(String::as_str);
    let second = words.get(1).map(String::as_str);
    match first {
        Some("BEGIN" | "COMMIT" | "ROLLBACK" | "ABORT" | "END" | "SAVEPOINT") => true,
        Some("START") => second == Some("TRANSACTION"),
        Some("RELEASE") => second == Some("SAVEPOINT"),
        Some("PREPARE") => second == Some("TRANSACTION"),
        Some("SET") => {
            second == Some("TRANSACTION")
                || words
                    .windows(4)
                    .any(|window| words_eq(window, &["SESSION", "CHARACTERISTICS", "AS", "TRANSACTION"]))
        }
        _ => false,
    }
}

fn is_unsupported_in_managed_transaction(words: &[String]) -> bool {
    let first = words.first().map(String::as_str);
    let second = words.get(1).map(String::as_str);
    match first {
        Some("VACUUM" | "CHECKPOINT" | "DISCARD") => true,
        Some("CREATE") => {
            matches!(second, Some("DATABASE" | "TABLESPACE" | "SUBSCRIPTION"))
                || (second == Some("INDEX") && words.iter().any(|word| word == "CONCURRENTLY"))
        }
        Some("DROP") => {
            matches!(second, Some("DATABASE" | "TABLESPACE" | "SUBSCRIPTION"))
                || (second == Some("INDEX") && words.iter().any(|word| word == "CONCURRENTLY"))
        }
        Some("ALTER") => second == Some("SYSTEM"),
        Some("REINDEX") => words.iter().any(|word| word == "CONCURRENTLY"),
        Some("COPY") => true,
        _ => false,
    }
}

fn words_eq(words: &[String], expected: &[&str]) -> bool {
    words.len() == expected.len()
        && words
            .iter()
            .zip(expected.iter())
            .all(|(word, expected)| word == expected)
}

fn scanned_statements(sql: &str) -> Vec<ScannedStatement> {
    let bytes = sql.as_bytes();
    let mut state = ScanState::Normal;
    let mut statements = Vec::new();
    let mut segment_start = 0usize;
    let mut words = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        match &mut state {
            ScanState::Normal => match bytes[index] {
                b'\'' => {
                    state = ScanState::SingleQuoted;
                    index += 1;
                }
                b'"' => {
                    state = ScanState::DoubleQuoted;
                    index += 1;
                }
                b'-' if bytes.get(index + 1) == Some(&b'-') => {
                    state = ScanState::LineComment;
                    index += 2;
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    state = ScanState::BlockComment { depth: 1 };
                    index += 2;
                }
                b'$' => {
                    if let Some((delimiter, next_index)) = dollar_quote_delimiter(bytes, index) {
                        state = ScanState::DollarQuoted { delimiter };
                        index = next_index;
                    } else {
                        index += 1;
                    }
                }
                b';' => {
                    push_scanned_statement(
                        sql,
                        segment_start..index.saturating_add(1),
                        &mut words,
                        &mut statements,
                    );
                    segment_start = index.saturating_add(1);
                    index += 1;
                }
                byte if is_word_start(byte) => {
                    let start = index;
                    index += 1;
                    while index < bytes.len() && is_word_continue(bytes[index]) {
                        index += 1;
                    }
                    words.push(sql[start..index].to_ascii_uppercase());
                }
                _ => index += 1,
            },
            ScanState::SingleQuoted => {
                if bytes[index] == b'\'' {
                    if bytes.get(index + 1) == Some(&b'\'') {
                        index += 2;
                    } else {
                        state = ScanState::Normal;
                        index += 1;
                    }
                } else if bytes[index] == b'\\' && bytes.get(index + 1).is_some() {
                    index += 2;
                } else {
                    index += 1;
                }
            }
            ScanState::DoubleQuoted => {
                if bytes[index] == b'"' {
                    if bytes.get(index + 1) == Some(&b'"') {
                        index += 2;
                    } else {
                        state = ScanState::Normal;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            ScanState::LineComment => {
                if bytes[index] == b'\n' {
                    state = ScanState::Normal;
                }
                index += 1;
            }
            ScanState::BlockComment { depth } => {
                if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    *depth = depth.saturating_add(1);
                    index += 2;
                } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    *depth = depth.saturating_sub(1);
                    index += 2;
                    if *depth == 0 {
                        state = ScanState::Normal;
                    }
                } else {
                    index += 1;
                }
            }
            ScanState::DollarQuoted { delimiter } => {
                if bytes[index..].starts_with(delimiter) {
                    index = index.saturating_add(delimiter.len());
                    state = ScanState::Normal;
                } else {
                    index += 1;
                }
            }
        }
    }

    push_scanned_statement(
        sql,
        segment_start..bytes.len(),
        &mut words,
        &mut statements,
    );
    statements
}

fn push_scanned_statement(
    sql: &str,
    range: Range<usize>,
    words: &mut Vec<String>,
    statements: &mut Vec<ScannedStatement>,
) {
    let trimmed = trim_ascii_whitespace(sql, range);
    if !words.is_empty() && trimmed.start < trimmed.end {
        statements.push(ScannedStatement {
            range: trimmed,
            words: std::mem::take(words),
        });
    } else {
        words.clear();
    }
}

fn trim_ascii_whitespace(sql: &str, mut range: Range<usize>) -> Range<usize> {
    let bytes = sql.as_bytes();
    while range.start < range.end && bytes[range.start].is_ascii_whitespace() {
        range.start += 1;
    }
    while range.end > range.start && bytes[range.end - 1].is_ascii_whitespace() {
        range.end -= 1;
    }
    range
}

fn dollar_quote_delimiter(bytes: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    if bytes.get(start) != Some(&b'$') {
        return None;
    }
    let mut index = start.saturating_add(1);
    while let Some(&byte) = bytes.get(index) {
        if byte == b'$' {
            let tag = &bytes[start.saturating_add(1)..index];
            if tag.is_empty()
                || (is_word_start(tag[0]) && tag.iter().copied().all(is_word_continue))
            {
                let end = index.saturating_add(1);
                return Some((bytes[start..end].to_vec(), end));
            }
            return None;
        }
        if !is_word_continue(byte) {
            return None;
        }
        index += 1;
    }
    None
}

fn is_word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_word_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub fn format_sql_conservative(sql: &str) -> Result<String, String> {
    if sql.trim().is_empty() {
        return Ok(String::new());
    }
    if has_syntax_error(sql) || has_obviously_incomplete_statement(sql) {
        return Err("SQL contains syntax errors; formatting was not applied".to_string());
    }

    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len().saturating_add(sql.len() / 8));
    let mut state = ScanState::Normal;
    let mut index = 0usize;
    let mut pending_space = false;
    let mut line_start = true;

    while index < bytes.len() {
        match &mut state {
            ScanState::Normal => match bytes[index] {
                b'\'' => {
                    write_pending_space(&mut out, &mut pending_space, line_start);
                    out.push('\'');
                    state = ScanState::SingleQuoted;
                    line_start = false;
                    index += 1;
                }
                b'"' => {
                    write_pending_space(&mut out, &mut pending_space, line_start);
                    out.push('"');
                    state = ScanState::DoubleQuoted;
                    line_start = false;
                    index += 1;
                }
                b'-' if bytes.get(index + 1) == Some(&b'-') => {
                    if !line_start && !out.ends_with(' ') {
                        out.push(' ');
                    }
                    out.push_str("--");
                    state = ScanState::LineComment;
                    pending_space = false;
                    line_start = false;
                    index += 2;
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    write_pending_space(&mut out, &mut pending_space, line_start);
                    out.push_str("/*");
                    state = ScanState::BlockComment { depth: 1 };
                    line_start = false;
                    index += 2;
                }
                b'$' => {
                    if let Some((delimiter, next_index)) = dollar_quote_delimiter(bytes, index) {
                        write_pending_space(&mut out, &mut pending_space, line_start);
                        out.push_str(std::str::from_utf8(&delimiter).unwrap_or("$$"));
                        state = ScanState::DollarQuoted { delimiter };
                        line_start = false;
                        index = next_index;
                    } else {
                        write_pending_space(&mut out, &mut pending_space, line_start);
                        out.push('$');
                        line_start = false;
                        index += 1;
                    }
                }
                b';' => {
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push(';');
                    out.push('\n');
                    pending_space = false;
                    line_start = true;
                    index += 1;
                }
                b',' => {
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push(',');
                    pending_space = true;
                    line_start = false;
                    index += 1;
                }
                byte if byte.is_ascii_whitespace() => {
                    pending_space = !line_start;
                    index += 1;
                }
                byte if is_word_start(byte) => {
                    let start = index;
                    index += 1;
                    while index < bytes.len() && is_word_continue(bytes[index]) {
                        index += 1;
                    }
                    let word = &sql[start..index];
                    if is_format_break_keyword(word) && !line_start && !out.trim_end().is_empty() {
                        while out.ends_with(' ') {
                            out.pop();
                        }
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                        pending_space = false;
                    } else {
                        write_pending_space(&mut out, &mut pending_space, line_start);
                    }
                    out.push_str(word);
                    line_start = false;
                }
                _ => {
                    write_pending_space(&mut out, &mut pending_space, line_start);
                    out.push(bytes[index] as char);
                    line_start = false;
                    index += 1;
                }
            },
            ScanState::SingleQuoted => {
                out.push(bytes[index] as char);
                if bytes[index] == b'\'' {
                    if bytes.get(index + 1) == Some(&b'\'') {
                        out.push('\'');
                        index += 2;
                    } else {
                        state = ScanState::Normal;
                        index += 1;
                    }
                } else if bytes[index] == b'\\' && bytes.get(index + 1).is_some() {
                    out.push(bytes[index + 1] as char);
                    index += 2;
                } else {
                    index += 1;
                }
            }
            ScanState::DoubleQuoted => {
                out.push(bytes[index] as char);
                if bytes[index] == b'"' {
                    if bytes.get(index + 1) == Some(&b'"') {
                        out.push('"');
                        index += 2;
                    } else {
                        state = ScanState::Normal;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            ScanState::LineComment => {
                let ch = bytes[index] as char;
                out.push(ch);
                index += 1;
                if ch == '\n' {
                    state = ScanState::Normal;
                    line_start = true;
                }
            }
            ScanState::BlockComment { depth } => {
                if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    out.push_str("/*");
                    *depth = depth.saturating_add(1);
                    index += 2;
                } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    out.push_str("*/");
                    *depth = depth.saturating_sub(1);
                    index += 2;
                    if *depth == 0 {
                        state = ScanState::Normal;
                    }
                } else {
                    let ch = bytes[index] as char;
                    out.push(ch);
                    if ch == '\n' {
                        line_start = true;
                    }
                    index += 1;
                }
            }
            ScanState::DollarQuoted { delimiter } => {
                if bytes[index..].starts_with(delimiter) {
                    out.push_str(std::str::from_utf8(delimiter).unwrap_or("$$"));
                    index += delimiter.len();
                    state = ScanState::Normal;
                } else {
                    let ch = bytes[index] as char;
                    out.push(ch);
                    if ch == '\n' {
                        line_start = true;
                    }
                    index += 1;
                }
            }
        }
    }

    Ok(out.trim().to_string())
}

fn has_obviously_incomplete_statement(sql: &str) -> bool {
    scanned_statements(sql).iter().any(|statement| {
        let words = statement.words.as_slice();
        matches!(words, [only] if only == "SELECT")
            || statement_text_starts_with_keywords(sql, &statement.range, &["SELECT", "FROM"])
            || matches!(words, [only] if matches!(only.as_str(), "INSERT" | "UPDATE" | "DELETE"))
            || matches!(words, [first, second] if first == "INSERT" && second == "INTO")
    })
}

fn statement_text_starts_with_keywords(
    sql: &str,
    range: &Range<usize>,
    keywords: &[&str],
) -> bool {
    let mut remaining = sql.get(range.clone()).unwrap_or_default();
    for keyword in keywords {
        let Some(rest) = strip_ascii_keyword(remaining, keyword) else {
            return false;
        };
        remaining = rest;
    }
    true
}

fn strip_ascii_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let trimmed = text.trim_start_matches(|ch: char| ch.is_ascii_whitespace());
    let prefix = trimmed.get(..keyword.len())?;
    if !prefix.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let rest = trimmed.get(keyword.len()..)?;
    if rest
        .as_bytes()
        .first()
        .is_some_and(|byte| is_word_continue(*byte))
    {
        return None;
    }
    Some(rest)
}

fn write_pending_space(out: &mut String, pending_space: &mut bool, line_start: bool) {
    if *pending_space
        && !line_start
        && !out.ends_with(' ')
        && !out.ends_with('\n')
        && !out.ends_with('(')
    {
        out.push(' ');
    }
    *pending_space = false;
}

fn is_format_break_keyword(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "GROUP"
            | "HAVING"
            | "ORDER"
            | "LIMIT"
            | "OFFSET"
            | "RETURNING"
            | "UNION"
            | "INTERSECT"
            | "EXCEPT"
            | "INSERT"
            | "UPDATE"
            | "DELETE"
            | "VALUES"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_ignores_semicolons_in_literals_comments_and_dollar_quotes() {
        let sql = "SELECT ';'; -- ;\nSELECT $$a;b$$; /* ; /* nested ; */ */ SELECT 3;";
        let statements = scan_statements(sql);
        assert_eq!(statements.len(), 3);
        assert_eq!(&sql[statements[0].range.clone()], "SELECT ';';");
        assert!(sql[statements[1].range.clone()].contains("SELECT $$a;b$$;"));
        assert!(sql[statements[2].range.clone()].ends_with("SELECT 3;"));
    }

    #[test]
    fn scanner_handles_quoted_identifiers_and_escaped_strings() {
        let sql = "SELECT 'it''s;ok', E'\\';still', \"semi;column\"; UPDATE t SET v = 1;";
        let statements = scan_statements(sql);
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].kind, SqlStatementKind::Query);
        assert_eq!(statements[1].kind, SqlStatementKind::Mutation);
    }

    #[test]
    fn statement_range_at_prefers_statement_under_cursor() {
        let sql = "SELECT 1;\n\nUPDATE items SET value = 2;";
        let update_offset = sql.find("UPDATE").unwrap();
        let range = statement_range_at(sql, update_offset + 4).unwrap();
        assert_eq!(&sql[range], "UPDATE items SET value = 2;");
    }

    #[test]
    fn managed_sql_rejects_transaction_control_outside_literals() {
        let allowed = validate_managed_user_sql("SELECT 'COMMIT'; -- ROLLBACK\nSELECT 2;");
        assert!(allowed.is_ok());

        let error = validate_managed_user_sql("SELECT 1; COMMIT;").unwrap_err();
        assert_eq!(error.kind, SqlValidationErrorKind::TransactionControl);
        assert_eq!(error.statement_index, Some(1));
    }

    #[test]
    fn managed_sql_rejects_transaction_synonyms_and_session_characteristics() {
        for sql in [
            "ABORT;",
            "END;",
            "START TRANSACTION;",
            "RELEASE SAVEPOINT point_a;",
            "PREPARE TRANSACTION 'tx';",
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE;",
            "SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY;",
        ] {
            let error = validate_managed_user_sql(sql).unwrap_err();
            assert_eq!(error.kind, SqlValidationErrorKind::TransactionControl, "{sql}");
        }
    }

    #[test]
    fn managed_sql_rejects_commands_that_require_autocommit_or_copy_streams() {
        for sql in [
            "CREATE DATABASE demo;",
            "DROP TABLESPACE fast_space;",
            "ALTER SYSTEM SET work_mem = '64MB';",
            "VACUUM items;",
            "CREATE INDEX CONCURRENTLY idx_items ON items(id);",
            "REINDEX INDEX CONCURRENTLY idx_items;",
            "COPY items TO STDOUT;",
        ] {
            let error = validate_managed_user_sql(sql).unwrap_err();
            assert_eq!(
                error.kind,
                SqlValidationErrorKind::UnsupportedInManagedTransaction,
                "{sql}"
            );
        }
    }

    #[test]
    fn managed_sql_classifies_common_postgresql_statements() {
        let statements = validate_managed_user_sql(
            "SELECT 1; INSERT INTO t VALUES (1); CREATE TABLE x(id int); EXPLAIN SELECT 1;",
        )
        .unwrap();
        assert_eq!(
            statements.iter().map(|item| item.kind).collect::<Vec<_>>(),
            vec![
                SqlStatementKind::Query,
                SqlStatementKind::Mutation,
                SqlStatementKind::Definition,
                SqlStatementKind::Explain,
            ]
        );
    }

    #[test]
    fn managed_sql_rejects_empty_and_excessive_scripts() {
        let empty = validate_managed_user_sql(" -- comment only\n").unwrap_err();
        assert_eq!(empty.kind, SqlValidationErrorKind::Empty);

        let sql = "SELECT 1;".repeat(33);
        let excessive = validate_managed_user_sql(&sql).unwrap_err();
        assert_eq!(excessive.kind, SqlValidationErrorKind::TooManyStatements);
    }

    #[test]
    fn dollar_quote_parser_rejects_positional_parameters() {
        let sql = "SELECT $1, $tag$body;still body$tag$; SELECT 2;";
        let statements = scan_statements(sql);
        assert_eq!(statements.len(), 2);
    }

    #[test]
    fn conservative_formatter_preserves_literals_comments_and_dollar_quotes() {
        let sql = "select  'a;  b'  from  t where x=$tag$keep  spaces$tag$; -- keep  comment";
        let formatted = format_sql_conservative(sql).unwrap();
        assert!(formatted.contains("'a;  b'"));
        assert!(formatted.contains("$tag$keep  spaces$tag$"));
        assert!(formatted.contains("-- keep  comment"));
        assert!(formatted.contains("\nfrom ") || formatted.contains("\nfrom\n"));
        assert!(formatted.contains("\nwhere ") || formatted.contains("\nwhere\n"));
    }

    #[test]
    fn conservative_formatter_refuses_invalid_sql() {
        assert!(format_sql_conservative("SELECT FROM").is_err());
    }
}
