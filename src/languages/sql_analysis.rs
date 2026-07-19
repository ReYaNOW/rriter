use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SqlDiagnosticSeverity {
    #[default]
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlAnalysisDiagnostic {
    pub range: Range<usize>,
    pub severity: SqlDiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlRelationBinding {
    pub scope: Range<usize>,
    pub source_range: Range<usize>,
    pub schema: Option<String>,
    pub table_name: String,
    pub alias: String,
    pub is_cte: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlQualifiedReference {
    pub scope: Range<usize>,
    pub range: Range<usize>,
    pub qualifier: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlUnqualifiedReference {
    pub scope: Range<usize>,
    pub range: Range<usize>,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlAnalysis {
    pub diagnostics: Vec<SqlAnalysisDiagnostic>,
    pub relations: Vec<SqlRelationBinding>,
    pub qualified_references: Vec<SqlQualifiedReference>,
    pub unqualified_references: Vec<SqlUnqualifiedReference>,
    pub output_aliases: Vec<(Range<usize>, String)>,
    pub ctes: Vec<(Range<usize>, String)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SqlCompletionKind {
    #[default]
    None,
    Table,
    Column,
    QualifiedColumn,
    Operator,
    Value,
    Direction,
    NullOrdering,
    Keyword,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlCompletionContext {
    pub kind: SqlCompletionKind,
    pub prefix: String,
    pub qualifier: Option<String>,
    pub replace_range: Range<usize>,
    pub scope: Range<usize>,
    pub automatic: bool,
}

impl SqlCompletionContext {
    pub fn context_key(&self) -> String {
        format!(
            "sql:{:?}:{}:{}:{}",
            self.kind,
            self.scope.start,
            self.scope.end,
            self.qualifier.as_deref().unwrap_or("")
        )
    }
}

pub fn analyze_sql(sql: &str) -> SqlAnalysis {
    let Some(tree) = parse_sql(sql) else {
        return SqlAnalysis {
            diagnostics: vec![SqlAnalysisDiagnostic {
                range: 0..sql.len().max(1),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL000",
                message: "Не удалось запустить SQL-анализатор".to_string(),
            }],
            ..SqlAnalysis::default()
        };
    };
    let root = tree.root_node();
    let mut analysis = SqlAnalysis::default();
    collect_syntax_diagnostics(root, sql, &mut analysis.diagnostics);
    collect_statement_analysis(root, sql, &mut analysis);
    sort_and_deduplicate_diagnostics(&mut analysis.diagnostics);
    analysis
}

pub fn completion_context(sql: &str, cursor: usize) -> SqlCompletionContext {
    let cursor = super::sql::clamp_sql_offset(sql, cursor);
    let (prefix, qualifier, replace_range) = identifier_fragment(sql, cursor);
    let Some(tree) = parse_sql(sql) else {
        return SqlCompletionContext {
            kind: SqlCompletionKind::Keyword,
            prefix,
            qualifier,
            replace_range,
            scope: 0..sql.len(),
            automatic: false,
        };
    };
    let root = tree.root_node();
    let probe = previous_non_whitespace_byte(sql, cursor);
    let node = probe
        .and_then(|start| {
            let end = next_char_boundary(sql, start).unwrap_or(sql.len()).max(start + 1);
            root.descendant_for_byte_range(start, end.min(sql.len()))
        })
        .unwrap_or(root);
    if is_inside_string_or_comment(node, sql) {
        return SqlCompletionContext {
            kind: SqlCompletionKind::None,
            prefix,
            qualifier,
            replace_range,
            scope: 0..sql.len(),
            automatic: false,
        };
    }
    let scope_node = nearest_ancestor(node, "statement")
        .or_else(|| statement_near_cursor(root, sql, cursor))
        .unwrap_or(root);
    let scope = scope_node.start_byte()..scope_node.end_byte();
    let clause = nearest_clause(node).or_else(|| clause_near_cursor(scope_node, cursor));
    let tokens = leaf_tokens_before(scope_node, sql, cursor, 12);
    let last = tokens.last();
    let previous = tokens.get(tokens.len().saturating_sub(2));
    let after_dot = qualifier.is_some();
    let prefix_present = !prefix.is_empty();

    let kind = if after_dot {
        SqlCompletionKind::QualifiedColumn
    } else {
        match clause.as_deref() {
            Some("from") => {
                if last.is_some_and(|token| token.kind == "keyword_on") {
                    SqlCompletionKind::Column
                } else {
                    SqlCompletionKind::Table
                }
            }
            Some("join") => {
                if is_after_expression_operator(last) || is_after_boolean_connector(last) {
                    SqlCompletionKind::Column
                } else if last.is_some_and(|token| token.kind == "keyword_join") {
                    SqlCompletionKind::Table
                } else {
                    SqlCompletionKind::Column
                }
            }
            Some("where") => where_completion_kind(last, previous, prefix_present),
            Some("order_by") => order_completion_kind(last, previous, prefix_present),
            Some("group_by") | Some("returning") => SqlCompletionKind::Column,
            Some("select") | Some("select_expression") => {
                if last.is_some_and(|token| token.text.eq_ignore_ascii_case("FROM")) {
                    SqlCompletionKind::Table
                } else {
                    SqlCompletionKind::Column
                }
            }
            Some("update") => {
                if last.is_some_and(|token| token.kind == "keyword_update") {
                    SqlCompletionKind::Table
                } else if is_after_expression_operator(last) {
                    SqlCompletionKind::Value
                } else {
                    SqlCompletionKind::Column
                }
            }
            Some("insert") => {
                if last.is_some_and(|token| token.kind == "keyword_into") {
                    SqlCompletionKind::Table
                } else {
                    SqlCompletionKind::Column
                }
            }
            _ => {
                if last.is_some_and(|token| {
                    matches!(
                        token.kind.as_str(),
                        "keyword_from" | "keyword_join" | "keyword_into" | "keyword_update"
                    )
                }) {
                    SqlCompletionKind::Table
                } else if prefix_present {
                    SqlCompletionKind::Keyword
                } else {
                    SqlCompletionKind::None
                }
            }
        }
    };

    let automatic = match kind {
        SqlCompletionKind::None => false,
        SqlCompletionKind::Value => false,
        SqlCompletionKind::Operator => true,
        SqlCompletionKind::Direction | SqlCompletionKind::NullOrdering => true,
        SqlCompletionKind::Table
        | SqlCompletionKind::Column
        | SqlCompletionKind::QualifiedColumn
        | SqlCompletionKind::Keyword => prefix_present || after_dot || automatic_after_clause(last),
    };

    SqlCompletionContext {
        kind,
        prefix,
        qualifier,
        replace_range,
        scope,
        automatic,
    }
}

pub fn relation_for_qualifier<'a>(
    analysis: &'a SqlAnalysis,
    cursor: usize,
    qualifier: &str,
) -> Option<&'a SqlRelationBinding> {
    let scoped = analysis
        .relations
        .iter()
        .filter(|relation| relation.scope.start <= cursor && cursor <= relation.scope.end)
        .filter(|relation| {
            relation.alias.eq_ignore_ascii_case(qualifier)
                || relation.table_name.eq_ignore_ascii_case(qualifier)
        })
        .min_by_key(|relation| relation.scope.end.saturating_sub(relation.scope.start));
    if scoped.is_some() {
        return scoped;
    }

    // Incomplete expressions such as `SELECT b. FROM booking b` may temporarily
    // split the tree into recovery scopes. Falling back is safe only when the
    // qualifier resolves to exactly one relation in the whole parsed document.
    let mut matches = analysis.relations.iter().filter(|relation| {
        relation.alias.eq_ignore_ascii_case(qualifier)
            || relation.table_name.eq_ignore_ascii_case(qualifier)
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

pub fn output_aliases_at<'a>(
    analysis: &'a SqlAnalysis,
    cursor: usize,
) -> impl Iterator<Item = &'a str> + 'a {
    analysis
        .output_aliases
        .iter()
        .filter(move |(scope, _)| scope.start <= cursor && cursor <= scope.end)
        .map(|(_, alias)| alias.as_str())
}

fn parse_sql(sql: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_sequel::LANGUAGE.into();
    parser.set_language(&language).ok()?;
    parser.parse(sql, None)
}

fn collect_syntax_diagnostics(
    node: tree_sitter::Node<'_>,
    sql: &str,
    diagnostics: &mut Vec<SqlAnalysisDiagnostic>,
) {
    if node.is_error() {
        diagnostics.push(SqlAnalysisDiagnostic {
            range: stable_node_range(node, sql),
            severity: SqlDiagnosticSeverity::Error,
            code: "SQL001",
            message: syntax_message(node.parent().map(|parent| parent.kind())).to_string(),
        });
    } else if node.is_missing() {
        diagnostics.push(SqlAnalysisDiagnostic {
            range: stable_node_range(node, sql),
            severity: SqlDiagnosticSeverity::Error,
            code: "SQL002",
            message: format!("Ожидался элемент SQL: {}", readable_kind(node.kind())),
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_syntax_diagnostics(child, sql, diagnostics);
    }
}

fn collect_statement_analysis(
    root: tree_sitter::Node<'_>,
    sql: &str,
    analysis: &mut SqlAnalysis,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "statement" {
            analyze_statement(node, sql, analysis);
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn analyze_statement(
    statement: tree_sitter::Node<'_>,
    sql: &str,
    analysis: &mut SqlAnalysis,
) {
    let scope = statement.start_byte()..statement.end_byte();
    collect_structure_diagnostics(statement, sql, analysis);
    collect_ctes(statement, sql, scope.clone(), analysis);
    collect_relations(statement, sql, scope.clone(), analysis);
    collect_output_aliases(statement, sql, scope.clone(), analysis);
    collect_qualified_references(statement, sql, scope.clone(), analysis);
    collect_statement_lints(statement, sql, scope, analysis);
}

fn collect_structure_diagnostics(
    statement: tree_sitter::Node<'_>,
    sql: &str,
    analysis: &mut SqlAnalysis,
) {
    walk_scope_nodes(statement, statement, |node| {
        if node.kind() != "select" {
            return;
        }
        let leaves = leaf_nodes(node, node);
        let select_index = leaves
            .iter()
            .position(|leaf| leaf.kind() == "keyword_select");
        let Some(select_index) = select_index else { return; };
        let Some(first_expression_token) = leaves.get(select_index + 1).copied() else {
            analysis.diagnostics.push(SqlAnalysisDiagnostic {
                range: stable_node_range(node, sql),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL003",
                message: "После SELECT ожидается выражение или список столбцов".to_string(),
            });
            return;
        };
        if node_text(first_expression_token, sql)
            .is_some_and(|text| text.eq_ignore_ascii_case("FROM"))
        {
            analysis.diagnostics.push(SqlAnalysisDiagnostic {
                range: stable_node_range(first_expression_token, sql),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL003",
                message: "Перед FROM отсутствует выражение SELECT".to_string(),
            });
        }
    });
}

fn collect_ctes(
    statement: tree_sitter::Node<'_>,
    sql: &str,
    scope: Range<usize>,
    analysis: &mut SqlAnalysis,
) {
    walk_scope_nodes(statement, statement, |node| {
        if node.kind() != "cte" {
            return;
        }
        let mut cursor = node.walk();
        if let Some(name) = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "identifier")
            .and_then(|child| node_text(child, sql))
        {
            analysis.ctes.push((scope.clone(), normalize_identifier(name)));
        }
    });
}

fn collect_relations(
    statement: tree_sitter::Node<'_>,
    sql: &str,
    scope: Range<usize>,
    analysis: &mut SqlAnalysis,
) {
    let cte_names = analysis
        .ctes
        .iter()
        .filter(|(cte_scope, _)| *cte_scope == scope)
        .map(|(_, name)| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut aliases = BTreeMap::<String, Range<usize>>::new();
    walk_scope_nodes(statement, statement, |node| {
        if node.kind() != "relation" {
            return;
        }
        let mut cursor = node.walk();
        let object = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "object_reference");
        let Some(object) = object else { return; };
        let Some(table_name) = object
            .child_by_field_name("name")
            .and_then(|name| node_text(name, sql))
            .map(normalize_identifier)
        else {
            return;
        };
        let schema = object
            .child_by_field_name("schema")
            .and_then(|name| node_text(name, sql))
            .map(normalize_identifier);
        let alias = node
            .child_by_field_name("alias")
            .and_then(|alias| node_text(alias, sql))
            .map(normalize_identifier)
            .unwrap_or_else(|| table_name.clone());
        let alias_key = alias.to_ascii_lowercase();
        if let Some(previous) = aliases.insert(alias_key, node.start_byte()..node.end_byte()) {
            analysis.diagnostics.push(SqlAnalysisDiagnostic {
                range: node.start_byte()..node.end_byte(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL201",
                message: format!(
                    "Псевдоним «{alias}» уже используется в этом SQL-блоке (первое объявление: байты {}..{})",
                    previous.start, previous.end
                ),
            });
        }
        analysis.relations.push(SqlRelationBinding {
            scope: scope.clone(),
            source_range: node.start_byte()..node.end_byte(),
            schema,
            is_cte: cte_names.contains(&table_name.to_ascii_lowercase()),
            table_name,
            alias,
        });
    });
}

fn collect_output_aliases(
    statement: tree_sitter::Node<'_>,
    sql: &str,
    scope: Range<usize>,
    analysis: &mut SqlAnalysis,
) {
    walk_scope_nodes(statement, statement, |node| {
        if node.kind() != "term" {
            return;
        }
        if let Some(alias) = node
            .child_by_field_name("alias")
            .and_then(|alias| node_text(alias, sql))
            .map(normalize_identifier)
        {
            analysis.output_aliases.push((scope.clone(), alias));
        }
    });
}

fn collect_qualified_references(
    statement: tree_sitter::Node<'_>,
    sql: &str,
    scope: Range<usize>,
    analysis: &mut SqlAnalysis,
) {
    walk_scope_nodes(statement, statement, |node| {
        if node.kind() != "field" {
            return;
        }
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Some(name) = node_text(name_node, sql).map(normalize_identifier) else {
            return;
        };
        let mut cursor = node.walk();
        let owner = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "object_reference");
        if let Some(owner) = owner {
            let Some(qualifier) = owner
                .child_by_field_name("name")
                .and_then(|name| node_text(name, sql))
                .map(normalize_identifier)
            else {
                return;
            };
            analysis.qualified_references.push(SqlQualifiedReference {
                scope: scope.clone(),
                range: node.start_byte()..node.end_byte(),
                qualifier,
                name,
            });
        } else {
            analysis.unqualified_references.push(SqlUnqualifiedReference {
                scope: scope.clone(),
                range: name_node.start_byte()..name_node.end_byte(),
                name,
            });
        }
    });
}

fn collect_statement_lints(
    statement: tree_sitter::Node<'_>,
    sql: &str,
    scope: Range<usize>,
    analysis: &mut SqlAnalysis,
) {
    let delete = find_in_scope(statement, statement, "delete");
    let update = find_in_scope(statement, statement, "update");
    let truncate = has_leaf_kind_in_scope(statement, statement, "keyword_truncate");
    let from = find_in_scope(statement, statement, "from");
    let top_where = if update.is_some() {
        find_in_scope(statement, statement, "where")
    } else {
        from.and_then(|from| direct_named_child(from, "where"))
    };

    if let Some(delete) = delete
        && top_where.is_none()
    {
        analysis.diagnostics.push(warning(
            delete.start_byte()..delete.end_byte(),
            "SQL101",
            "DELETE выполняется без WHERE и затронет все строки таблицы",
        ));
    }
    if let Some(update) = update
        && top_where.is_none()
    {
        analysis.diagnostics.push(warning(
            update.start_byte()..update.end_byte(),
            "SQL102",
            "UPDATE выполняется без WHERE и затронет все строки таблицы",
        ));
    }
    if truncate {
        analysis.diagnostics.push(warning(
            scope.clone(),
            "SQL103",
            "TRUNCATE безвозвратно удаляет все строки выбранных таблиц после подтверждения транзакции",
        ));
    }

    for (kind, code, message) in [
        ("drop_table", "SQL104", "DROP TABLE удаляет таблицу и её данные"),
        ("drop_database", "SQL105", "DROP DATABASE удаляет базу данных"),
        ("drop_schema", "SQL106", "DROP SCHEMA удаляет схему и может удалить связанные объекты"),
        ("drop_type", "SQL107", "DROP TYPE может нарушить зависящие таблицы и функции"),
        ("drop_view", "SQL108", "DROP VIEW удаляет представление"),
        ("drop_function", "SQL109", "DROP FUNCTION удаляет функцию"),
        ("drop_sequence", "SQL110", "DROP SEQUENCE может нарушить генерацию идентификаторов"),
    ] {
        if let Some(node) = find_in_scope(statement, statement, kind) {
            analysis
                .diagnostics
                .push(warning(node.start_byte()..node.end_byte(), code, message));
        }
    }

    if let Some(node) = find_in_scope(statement, statement, "drop_column") {
        analysis.diagnostics.push(warning(
            node.start_byte()..node.end_byte(),
            "SQL111",
            "ALTER TABLE DROP COLUMN удаляет данные столбца",
        ));
    }
    if let Some(node) = find_in_scope(statement, statement, "drop_constraint") {
        analysis.diagnostics.push(warning(
            node.start_byte()..node.end_byte(),
            "SQL112",
            "Удаление ограничения может нарушить целостность данных",
        ));
    }
    if has_leaf_kind_in_scope(statement, statement, "keyword_cascade") {
        analysis.diagnostics.push(warning(
            scope.clone(),
            "SQL113",
            "CASCADE применит операцию ко всем зависимым объектам",
        ));
    }
    if let Some(where_node) = top_where
        && predicate_is_constant_true(where_node, sql)
    {
        analysis.diagnostics.push(warning(
            where_node.start_byte()..where_node.end_byte(),
            "SQL114",
            "Условие WHERE всегда истинно и не ограничивает изменяемые строки",
        ));
    }
    if let Some(from_node) = from {
        let direct_relations = direct_named_children_count(from_node, "relation");
        if direct_relations > 1 {
            analysis.diagnostics.push(warning(
                from_node.start_byte()..from_node.end_byte(),
                "SQL115",
                "Несколько таблиц через запятую образуют неявное декартово произведение; используйте явный JOIN",
            ));
        }
        if find_in_scope(from_node, from_node, "cross_join").is_some() {
            analysis.diagnostics.push(warning(
                from_node.start_byte()..from_node.end_byte(),
                "SQL116",
                "CROSS JOIN создаёт декартово произведение и может резко увеличить результат",
            ));
        }
        let has_limit = direct_named_child(from_node, "limit").is_some();
        let has_order = direct_named_child(from_node, "order_by").is_some();
        if has_limit && !has_order {
            analysis.diagnostics.push(warning(
                from_node.start_byte()..from_node.end_byte(),
                "SQL117",
                "LIMIT без ORDER BY возвращает недетерминированный набор строк",
            ));
        }
        if direct_named_child(from_node, "offset").is_some() && !has_order {
            analysis.diagnostics.push(warning(
                from_node.start_byte()..from_node.end_byte(),
                "SQL118",
                "OFFSET без ORDER BY может пропускать разные строки между запусками",
            ));
        }
    }
    if select_uses_star(statement) {
        analysis.diagnostics.push(warning(
            scope.clone(),
            "SQL119",
            "SELECT * делает результат зависимым от структуры таблицы; перечислите нужные столбцы",
        ));
    }
    walk_scope_nodes(statement, statement, |node| {
        if node.kind() == "binary_expression" {
            lint_binary_expression(node, sql, analysis);
        }
        if node.kind() == "order_target"
            && direct_named_child(node, "literal").is_some_and(|literal| {
                node_text(literal, sql).is_some_and(|value| value.trim().parse::<u32>().is_ok())
            })
        {
            analysis.diagnostics.push(warning(
                node.start_byte()..node.end_byte(),
                "SQL120",
                "ORDER BY по номеру столбца хрупок; используйте имя столбца или alias",
            ));
        }
        if node.kind() == "join"
            && has_leaf_kind_in_scope(node, node, "keyword_natural")
        {
            analysis.diagnostics.push(warning(
                node.start_byte()..node.end_byte(),
                "SQL121",
                "NATURAL JOIN зависит от совпадения имён столбцов и может незаметно изменить результат",
            ));
        }
    });
}

fn lint_binary_expression(
    node: tree_sitter::Node<'_>,
    sql: &str,
    analysis: &mut SqlAnalysis,
) {
    let Some(left) = node.child_by_field_name("left") else { return; };
    let Some(operator) = node.child_by_field_name("operator") else { return; };
    let Some(right) = node.child_by_field_name("right") else { return; };
    let operator_text = node_text(operator, sql).unwrap_or_default().trim();
    let left_text = node_text(left, sql).unwrap_or_default().trim();
    let right_text = node_text(right, sql).unwrap_or_default().trim();
    if matches!(operator_text, "=" | "!=" | "<>")
        && (left_text.eq_ignore_ascii_case("NULL") || right_text.eq_ignore_ascii_case("NULL"))
    {
        analysis.diagnostics.push(warning(
            node.start_byte()..node.end_byte(),
            "SQL122",
            "Сравнение с NULL через =, != или <> всегда даёт UNKNOWN; используйте IS NULL или IS NOT NULL",
        ));
    }
    if operator.kind() == "not_in" {
        analysis.diagnostics.push(warning(
            node.start_byte()..node.end_byte(),
            "SQL123",
            "NOT IN может вернуть пустой результат при наличии NULL; рассмотрите NOT EXISTS",
        ));
    }
    if matches!(operator.kind(), "keyword_like" | "not_like")
        && right_text.starts_with("'%")
    {
        analysis.diagnostics.push(warning(
            node.start_byte()..node.end_byte(),
            "SQL124",
            "Шаблон LIKE начинается с %, поэтому обычный B-tree индекс обычно не используется",
        ));
    }
}

fn semantic_parent_kind(node: tree_sitter::Node<'_>) -> Option<&'static str> {
    let mut current = Some(node);
    while let Some(value) = current {
        match value.kind() {
            "from" | "join" | "where" | "order_by" | "group_by" | "returning"
            | "select" | "select_expression" | "update" | "insert" => {
                return Some(value.kind())
            }
            _ => current = value.parent(),
        }
    }
    None
}

fn nearest_clause(node: tree_sitter::Node<'_>) -> Option<String> {
    semantic_parent_kind(node).map(str::to_string)
}

fn previous_non_whitespace_byte(sql: &str, cursor: usize) -> Option<usize> {
    sql.get(..cursor.min(sql.len()))?
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index))
}

fn statement_near_cursor<'tree>(
    root: tree_sitter::Node<'tree>,
    sql: &str,
    cursor: usize,
) -> Option<tree_sitter::Node<'tree>> {
    let mut candidates = Vec::new();
    collect_nodes_of_kind(root, "statement", &mut candidates);
    candidates
        .into_iter()
        .filter(|statement| statement.start_byte() <= cursor)
        .filter(|statement| {
            cursor <= statement.end_byte()
                || sql
                    .get(statement.end_byte()..cursor.min(sql.len()))
                    .is_some_and(|tail| tail.trim().is_empty())
        })
        .min_by_key(|statement| statement.end_byte().saturating_sub(statement.start_byte()))
}

fn clause_near_cursor(scope: tree_sitter::Node<'_>, cursor: usize) -> Option<String> {
    let mut candidates = Vec::new();
    collect_scope_clauses(scope, scope, cursor, &mut candidates);
    candidates
        .into_iter()
        .max_by_key(|node| node.start_byte())
        .map(|node| node.kind().to_string())
}

fn collect_scope_clauses<'tree>(
    root_scope: tree_sitter::Node<'tree>,
    node: tree_sitter::Node<'tree>,
    cursor: usize,
    out: &mut Vec<tree_sitter::Node<'tree>>,
) {
    if node != root_scope && matches!(node.kind(), "statement" | "subquery") {
        return;
    }
    if node.start_byte() <= cursor
        && matches!(
            node.kind(),
            "from"
                | "join"
                | "where"
                | "order_by"
                | "group_by"
                | "returning"
                | "select"
                | "select_expression"
                | "update"
                | "insert"
        )
    {
        out.push(node);
    }
    let mut walk = node.walk();
    for child in node.named_children(&mut walk) {
        collect_scope_clauses(root_scope, child, cursor, out);
    }
}

fn collect_nodes_of_kind<'tree>(
    node: tree_sitter::Node<'tree>,
    kind: &str,
    out: &mut Vec<tree_sitter::Node<'tree>>,
) {
    if node.kind() == kind {
        out.push(node);
    }
    let mut walk = node.walk();
    for child in node.named_children(&mut walk) {
        collect_nodes_of_kind(child, kind, out);
    }
}

fn leaf_nodes<'tree>(
    root_scope: tree_sitter::Node<'tree>,
    node: tree_sitter::Node<'tree>,
) -> Vec<tree_sitter::Node<'tree>> {
    fn collect<'tree>(
        root_scope: tree_sitter::Node<'tree>,
        node: tree_sitter::Node<'tree>,
        out: &mut Vec<tree_sitter::Node<'tree>>,
    ) {
        if node != root_scope && matches!(node.kind(), "statement" | "subquery") {
            return;
        }
        if node.child_count() == 0 {
            out.push(node);
            return;
        }
        let mut walk = node.walk();
        for child in node.children(&mut walk) {
            collect(root_scope, child, out);
        }
    }

    let mut out = Vec::new();
    collect(root_scope, node, &mut out);
    out
}

fn where_completion_kind(
    last: Option<&LeafToken>,
    previous: Option<&LeafToken>,
    prefix_present: bool,
) -> SqlCompletionKind {
    if is_after_expression_operator(last) {
        return SqlCompletionKind::Value;
    }
    if last.is_some_and(|token| token.kind == "keyword_is") {
        return SqlCompletionKind::Value;
    }
    if is_after_boolean_connector(last)
        || last.is_some_and(|token| token.kind == "keyword_where")
    {
        return SqlCompletionKind::Column;
    }
    if last.is_some_and(token_is_identifier_like)
        && previous.is_some_and(|token| is_after_expression_operator(Some(token)))
    {
        return SqlCompletionKind::Value;
    }
    if last.is_some_and(token_is_identifier_like) && !prefix_present {
        return SqlCompletionKind::Operator;
    }
    SqlCompletionKind::Column
}

fn order_completion_kind(
    last: Option<&LeafToken>,
    _previous: Option<&LeafToken>,
    prefix_present: bool,
) -> SqlCompletionKind {
    if last.is_some_and(|token| token.kind == "keyword_nulls") {
        return SqlCompletionKind::NullOrdering;
    }
    if last.is_some_and(|token| matches!(token.kind.as_str(), "keyword_asc" | "keyword_desc")) {
        return SqlCompletionKind::NullOrdering;
    }
    if last.is_some_and(token_is_identifier_like) && !prefix_present {
        return SqlCompletionKind::Direction;
    }
    SqlCompletionKind::Column
}

#[derive(Clone, Debug)]
struct LeafToken {
    kind: String,
    text: String,
}

fn leaf_tokens_before(
    scope: tree_sitter::Node<'_>,
    sql: &str,
    cursor: usize,
    limit: usize,
) -> Vec<LeafToken> {
    let mut tokens = Vec::new();
    collect_leaf_tokens(scope, scope, sql, cursor, &mut tokens);
    if tokens.len() > limit {
        tokens.drain(0..tokens.len() - limit);
    }
    tokens
}

fn collect_leaf_tokens(
    root_scope: tree_sitter::Node<'_>,
    node: tree_sitter::Node<'_>,
    sql: &str,
    cursor: usize,
    out: &mut Vec<LeafToken>,
) {
    if node != root_scope && matches!(node.kind(), "statement" | "subquery") {
        return;
    }
    if node.child_count() == 0 {
        if node.end_byte() <= cursor
            && let Some(text) = node_text(node, sql)
            && !text.trim().is_empty()
        {
            out.push(LeafToken {
                kind: node.kind().to_string(),
                text: text.to_string(),
            });
        }
        return;
    }
    let mut walk = node.walk();
    for child in node.children(&mut walk) {
        collect_leaf_tokens(root_scope, child, sql, cursor, out);
    }
}

fn token_is_identifier_like(token: &LeafToken) -> bool {
    matches!(token.kind.as_str(), "identifier" | "field" | "object_reference")
        || token.kind == "literal"
}

fn is_after_expression_operator(token: Option<&LeafToken>) -> bool {
    token.is_some_and(|token| {
        matches!(
            token.text.as_str(),
            "=" | "<>" | "!=" | "<" | ">" | "<=" | ">=" | "+" | "-" | "*" | "/"
        ) || matches!(
            token.kind.as_str(),
            "keyword_like"
                | "keyword_ilike"
                | "keyword_in"
                | "keyword_between"
                | "keyword_is"
        )
    })
}

fn is_after_boolean_connector(token: Option<&LeafToken>) -> bool {
    token.is_some_and(|token| matches!(token.kind.as_str(), "keyword_and" | "keyword_or"))
}

fn automatic_after_clause(token: Option<&LeafToken>) -> bool {
    token.is_some_and(|token| {
        matches!(
            token.kind.as_str(),
            "keyword_from"
                | "keyword_join"
                | "keyword_where"
                | "keyword_and"
                | "keyword_or"
                | "keyword_by"
                | "keyword_set"
                | "keyword_into"
        )
    })
}

fn identifier_fragment(sql: &str, cursor: usize) -> (String, Option<String>, Range<usize>) {
    let cursor = cursor.min(sql.len());
    let mut start = cursor;
    for (idx, ch) in sql[..cursor].char_indices().rev() {
        if is_identifier_char(ch) {
            start = idx;
        } else {
            break;
        }
    }
    let prefix = sql.get(start..cursor).unwrap_or_default().to_string();
    let before = sql.get(..start).unwrap_or_default();
    let dot_index = before.char_indices().rev().find_map(|(idx, ch)| {
        if ch.is_whitespace() {
            None
        } else if ch == '.' {
            Some(idx)
        } else {
            Some(usize::MAX)
        }
    });
    let qualifier = match dot_index {
        Some(index) if index != usize::MAX => {
            let mut owner_start = index;
            for (idx, ch) in sql[..index].char_indices().rev() {
                if is_identifier_char(ch) {
                    owner_start = idx;
                } else {
                    break;
                }
            }
            sql.get(owner_start..index)
                .filter(|value| !value.is_empty())
                .map(normalize_identifier)
        }
        _ => None,
    };
    (prefix, qualifier, start..cursor)
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_alphanumeric() || !ch.is_ascii()
}

fn is_inside_string_or_comment(mut node: tree_sitter::Node<'_>, sql: &str) -> bool {
    loop {
        let kind = node.kind();
        if kind.contains("comment") {
            return true;
        }
        if kind == "literal"
            && node_text(node, sql).is_some_and(|text| {
                let trimmed = text.trim_start();
                trimmed.starts_with('\'') || trimmed.starts_with('$')
            })
        {
            return true;
        }
        let Some(parent) = node.parent() else { return false; };
        node = parent;
    }
}

fn nearest_ancestor<'tree>(
    mut node: tree_sitter::Node<'tree>,
    kind: &str,
) -> Option<tree_sitter::Node<'tree>> {
    loop {
        if node.kind() == kind {
            return Some(node);
        }
        node = node.parent()?;
    }
}

fn walk_scope_nodes(
    root_scope: tree_sitter::Node<'_>,
    node: tree_sitter::Node<'_>,
    mut callback: impl FnMut(tree_sitter::Node<'_>),
) {
    fn inner(
        root_scope: tree_sitter::Node<'_>,
        node: tree_sitter::Node<'_>,
        callback: &mut impl FnMut(tree_sitter::Node<'_>),
    ) {
        callback(node);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child != root_scope && matches!(child.kind(), "statement" | "subquery") {
                continue;
            }
            inner(root_scope, child, callback);
        }
    }
    inner(root_scope, node, &mut callback);
}

fn find_in_scope<'tree>(
    root_scope: tree_sitter::Node<'tree>,
    node: tree_sitter::Node<'tree>,
    kind: &str,
) -> Option<tree_sitter::Node<'tree>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child != root_scope && matches!(child.kind(), "statement" | "subquery") {
            continue;
        }
        if let Some(found) = find_in_scope(root_scope, child, kind) {
            return Some(found);
        }
    }
    None
}

fn has_leaf_kind_in_scope(
    root_scope: tree_sitter::Node<'_>,
    node: tree_sitter::Node<'_>,
    kind: &str,
) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| {
        if child != root_scope && matches!(child.kind(), "statement" | "subquery") {
            false
        } else {
            has_leaf_kind_in_scope(root_scope, child, kind)
        }
    })
}

fn direct_named_child<'tree>(
    node: tree_sitter::Node<'tree>,
    kind: &str,
) -> Option<tree_sitter::Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn direct_named_children_count(node: tree_sitter::Node<'_>, kind: &str) -> usize {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .count()
}

fn predicate_is_constant_true(where_node: tree_sitter::Node<'_>, sql: &str) -> bool {
    let Some(predicate) = where_node.child_by_field_name("predicate") else {
        return false;
    };
    if predicate.kind() == "keyword_true" {
        return true;
    }
    if predicate.kind() != "binary_expression" {
        return false;
    }
    let mut cursor = predicate.walk();
    let children = predicate.children(&mut cursor).collect::<Vec<_>>();
    if children.len() != 3 || node_text(children[1], sql) != Some("=") {
        return false;
    }
    let left = node_text(children[0], sql).map(str::trim);
    let right = node_text(children[2], sql).map(str::trim);
    matches!((left, right), (Some("1"), Some("1")))
        || left.is_some_and(|value| value.eq_ignore_ascii_case("TRUE"))
            && right.is_some_and(|value| value.eq_ignore_ascii_case("TRUE"))
}

fn select_uses_star(statement: tree_sitter::Node<'_>) -> bool {
    find_in_scope(statement, statement, "all_fields").is_some()
}

fn stable_node_range(node: tree_sitter::Node<'_>, sql: &str) -> Range<usize> {
    let start = node.start_byte().min(sql.len());
    let mut end = node.end_byte().min(sql.len());
    if end <= start {
        end = next_char_boundary(sql, start).unwrap_or(sql.len());
    }
    start..end
}

fn next_char_boundary(text: &str, start: usize) -> Option<usize> {
    text.get(start..)?
        .char_indices()
        .nth(1)
        .map(|(offset, _)| start + offset)
        .or(Some(text.len()))
}

fn node_text<'a>(node: tree_sitter::Node<'_>, sql: &'a str) -> Option<&'a str> {
    sql.get(node.start_byte()..node.end_byte())
}

fn normalize_identifier(identifier: &str) -> String {
    let value = identifier.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .replace("\"\"", "\"")
}

fn warning(range: Range<usize>, code: &'static str, message: &str) -> SqlAnalysisDiagnostic {
    SqlAnalysisDiagnostic {
        range,
        severity: SqlDiagnosticSeverity::Warning,
        code,
        message: message.to_string(),
    }
}

fn syntax_message(parent: Option<&str>) -> &'static str {
    match parent {
        Some("select" | "select_expression") => "Некорректное выражение SELECT",
        Some("from" | "relation") => "Некорректная секция FROM или имя таблицы",
        Some("join") => "Некорректное выражение JOIN",
        Some("where") => "Некорректное условие WHERE",
        Some("order_by") => "Некорректная секция ORDER BY",
        Some("group_by") => "Некорректная секция GROUP BY",
        Some("update") => "Некорректный UPDATE",
        Some("delete") => "Некорректный DELETE",
        Some("insert") => "Некорректный INSERT",
        Some("cte") => "Некорректное общее табличное выражение WITH",
        _ => "Синтаксическая ошибка SQL",
    }
}

fn readable_kind(kind: &str) -> String {
    kind.trim_start_matches("keyword_").replace('_', " ")
}

fn sort_and_deduplicate_diagnostics(diagnostics: &mut Vec<SqlAnalysisDiagnostic>) {
    diagnostics.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then(left.range.end.cmp(&right.range.end))
            .then(left.code.cmp(right.code))
    });
    diagnostics.dedup_by(|left, right| {
        left.range == right.range && left.code == right.code && left.message == right.message
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_alias_from_tree_sitter_relation() {
        let sql = "SELECT b.car_wash_id FROM booking AS b WHERE b.id = 1";
        let analysis = analyze_sql(sql);
        let relation = relation_for_qualifier(&analysis, 12, "b").unwrap();
        assert_eq!(relation.table_name, "booking");
        assert_eq!(relation.alias, "b");
        assert!(analysis
            .qualified_references
            .iter()
            .any(|reference| reference.qualifier == "b" && reference.name == "car_wash_id"));
    }

    #[test]
    fn completion_after_alias_dot_is_qualified_column_context() {
        let sql = "SELECT b.ca FROM booking b";
        let context = completion_context(sql, "SELECT b.ca".len());
        assert_eq!(context.kind, SqlCompletionKind::QualifiedColumn);
        assert_eq!(context.qualifier.as_deref(), Some("b"));
        assert_eq!(context.prefix, "ca");
        assert_eq!(&sql[context.replace_range], "ca");
    }

    #[test]
    fn completion_after_comparison_does_not_offer_columns_automatically() {
        let sql = "SELECT * FROM booking WHERE id = ";
        let context = completion_context(sql, sql.len());
        assert_eq!(context.kind, SqlCompletionKind::Value);
        assert!(!context.automatic);
    }

    #[test]
    fn completion_after_column_offers_operators_without_reopening_on_value() {
        let sql = "SELECT * FROM booking WHERE id ";
        let context = completion_context(sql, sql.len());
        assert_eq!(context.kind, SqlCompletionKind::Operator);
        assert!(context.automatic);

        let with_operator = "SELECT * FROM booking WHERE id =";
        let value = completion_context(with_operator, with_operator.len());
        assert_eq!(value.kind, SqlCompletionKind::Value);
        assert!(!value.automatic);
    }

    #[test]
    fn dangerous_mutations_are_reported_from_ast() {
        let delete = analyze_sql("DELETE FROM booking");
        assert!(delete.diagnostics.iter().any(|diagnostic| diagnostic.code == "SQL101"));
        let update = analyze_sql("UPDATE booking SET status = 'done'");
        assert!(update.diagnostics.iter().any(|diagnostic| diagnostic.code == "SQL102"));
        let safe = analyze_sql("DELETE FROM booking WHERE id = 1");
        assert!(!safe.diagnostics.iter().any(|diagnostic| diagnostic.code == "SQL101"));
    }

    #[test]
    fn nested_where_does_not_make_outer_delete_safe() {
        let sql = "DELETE FROM booking USING (SELECT id FROM old_booking WHERE archived = TRUE) old";
        let analysis = analyze_sql(sql);
        assert!(analysis.diagnostics.iter().any(|diagnostic| diagnostic.code == "SQL101"));
    }

    #[test]
    fn syntax_errors_have_stable_ranges() {
        let sql = "SELECT FROM booking";
        let analysis = analyze_sql(sql);
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == SqlDiagnosticSeverity::Error
                && diagnostic.range.start < diagnostic.range.end
        }));
    }

    #[test]
    fn completion_ignores_strings_and_comments() {
        let string_sql = "SELECT 'b.ca' FROM booking b";
        let string_cursor = string_sql.find("ca").unwrap() + 2;
        assert_eq!(completion_context(string_sql, string_cursor).kind, SqlCompletionKind::None);

        let comment_sql = "SELECT 1 -- b.ca\nFROM booking b";
        let comment_cursor = comment_sql.find("ca").unwrap() + 2;
        assert_eq!(completion_context(comment_sql, comment_cursor).kind, SqlCompletionKind::None);
    }

    #[test]
    fn cte_and_join_aliases_are_collected_from_ast_scopes() {
        let sql = "WITH recent AS (SELECT id FROM booking) SELECT r.id, cw.name FROM recent r JOIN car_wash cw ON cw.id = r.car_wash_id";
        let analysis = analyze_sql(sql);
        assert!(analysis.ctes.iter().any(|(_, name)| name == "recent"));
        assert!(analysis.relations.iter().any(|relation| {
            relation.alias == "r" && relation.table_name == "recent" && relation.is_cte
        }));
        assert!(analysis.relations.iter().any(|relation| {
            relation.alias == "cw" && relation.table_name == "car_wash"
        }));
    }

    #[test]
    fn quoted_and_schema_qualified_relations_preserve_semantic_names() {
        let sql = "SELECT b.\"Car Wash ID\" FROM public.\"Booking Entry\" AS b";
        let analysis = analyze_sql(sql);
        let relation = relation_for_qualifier(&analysis, sql.find("b.").unwrap(), "b").unwrap();
        assert_eq!(relation.schema.as_deref(), Some("public"));
        assert_eq!(relation.table_name, "Booking Entry");
        assert!(analysis.qualified_references.iter().any(|reference| {
            reference.qualifier == "b" && reference.name == "Car Wash ID"
        }));
    }

    #[test]
    fn ast_lints_cover_null_comparison_and_destructive_patterns() {
        let sql = "SELECT * FROM booking WHERE deleted_at = NULL ORDER BY 1 LIMIT 10; TRUNCATE booking; DROP TABLE old_booking CASCADE";
        let analysis = analyze_sql(sql);
        for code in ["SQL103", "SQL104", "SQL113", "SQL119", "SQL120", "SQL122"] {
            assert!(
                analysis.diagnostics.iter().any(|diagnostic| diagnostic.code == code),
                "missing {code}: {:?}",
                analysis.diagnostics
            );
        }
        let unstable_limit = analyze_sql("SELECT id FROM booking LIMIT 10");
        assert!(unstable_limit
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "SQL117"));
    }

    #[test]
    fn duplicate_alias_is_an_error_with_a_real_range() {
        let sql = "SELECT * FROM booking b JOIN car_wash b ON b.id = b.car_wash_id";
        let analysis = analyze_sql(sql);
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "SQL201"
                && diagnostic.severity == SqlDiagnosticSeverity::Error
                && diagnostic.range.start < diagnostic.range.end
        }));
    }

    #[test]
    fn a4_b010_completion_clamps_mid_utf8_cursor_without_panicking() {
        let context = completion_context("SELECT Ж FROM items", "SELECT ".len() + 1);
        assert!(context.replace_range.start <= context.replace_range.end);
        assert!(context.replace_range.end <= "SELECT Ж FROM items".len());
    }
}
