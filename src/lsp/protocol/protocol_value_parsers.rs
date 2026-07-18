// ── Парсинг входящих JSON-RPC сообщений ──────────────────────────────────────

/// Минимальный value-tree для парсинга LSP ответов без полной serde-схемы.
/// Используем только базовый JSON-парсинг.

use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize)]
pub(super) struct RpcHeader<'a> {
    #[serde(borrow)]
    pub method: Option<&'a str>,
    #[serde(borrow)]
    pub id: Option<RpcId<'a>>,
    #[serde(default)]
    pub error: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RpcId<'a> {
    Number(i64),
    Text(&'a str),
}

impl RpcId<'_> {
    pub(super) fn as_i64(&self) -> Option<i64> {
        match self {
            RpcId::Number(id) => Some(*id),
            RpcId::Text(text) => text.parse().ok(),
        }
    }

    pub(super) fn log_json(&self) -> String {
        match self {
            RpcId::Number(id) => id.to_string(),
            RpcId::Text(text) => format!(r#""{}""#, json_escape(text)),
        }
    }
}

#[derive(Deserialize)]
struct PublishDiagnosticsFrame<'a> {
    #[serde(borrow)]
    params: PublishDiagnosticsParams<'a>,
}

#[derive(Deserialize)]
struct PublishDiagnosticsParams<'a> {
    uri: &'a str,
    version: Option<i32>,
    #[serde(default, borrow)]
    diagnostics: Vec<BorrowedDiagnostic<'a>>,
}

#[derive(Deserialize)]
struct WorkspaceDiagnosticFrame<'a> {
    #[serde(default, borrow)]
    result: Option<WorkspaceDiagnosticResult<'a>>,
}

#[derive(Deserialize)]
struct WorkspaceDiagnosticResult<'a> {
    #[serde(default, borrow)]
    items: Vec<WorkspaceDiagnosticReport<'a>>,
}

#[derive(Deserialize)]
struct WorkspaceDiagnosticReport<'a> {
    #[serde(default)]
    kind: Option<&'a str>,
    #[serde(default)]
    uri: Option<&'a str>,
    #[serde(default)]
    version: Option<i32>,
    #[serde(default, rename = "resultId")]
    result_id: Option<&'a str>,
    #[serde(default, borrow)]
    items: Vec<BorrowedDiagnostic<'a>>,
    #[serde(default, rename = "relatedDocuments", borrow)]
    related_documents:
        std::collections::HashMap<Cow<'a, str>, WorkspaceDiagnosticRelatedReport<'a>>,
}

#[derive(Deserialize)]
struct WorkspaceDiagnosticRelatedReport<'a> {
    #[serde(default)]
    kind: Option<&'a str>,
    #[serde(default)]
    version: Option<i32>,
    #[serde(default, rename = "resultId")]
    result_id: Option<&'a str>,
    #[serde(default, borrow)]
    items: Vec<BorrowedDiagnostic<'a>>,
}

#[derive(Deserialize)]
struct BorrowedDiagnostic<'a> {
    range: BorrowedRange,
    severity: Option<u64>,
    #[serde(default, borrow)]
    code: Option<BorrowedCode<'a>>,
    #[serde(default, rename = "codeDescription", borrow)]
    code_description: Option<BorrowedCodeDescription<'a>>,
    #[serde(default, borrow)]
    message: Cow<'a, str>,
    #[serde(default)]
    source: Option<&'a str>,
    #[serde(default)]
    tags: Vec<u32>,
    #[serde(default, borrow)]
    data: Option<BorrowedDiagnosticData<'a>>,
}

#[derive(Deserialize)]
struct BorrowedRange {
    start: BorrowedPosition,
    end: BorrowedPosition,
}

#[derive(Deserialize)]
struct BorrowedPosition {
    line: u32,
    character: u32,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BorrowedCode<'a> {
    Text(&'a str),
    Number(u64),
}

#[derive(Deserialize)]
struct BorrowedCodeDescription<'a> {
    href: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BorrowedDiagnosticData<'a> {
    QuickFix(#[serde(borrow)] BorrowedDiagnosticQuickFixData<'a>),
    Ignored(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
struct BorrowedDiagnosticQuickFixData<'a> {
    title: Option<&'a str>,
    #[serde(default, borrow)]
    edits: Vec<BorrowedTextEdit<'a>>,
}

#[derive(Deserialize)]
struct BorrowedTextEdit<'a> {
    range: BorrowedRange,
    #[serde(default, rename = "newText", borrow)]
    new_text: Cow<'a, str>,
}

fn clean_diagnostic_message(message: &str) -> Cow<'_, str> {
    let mut out = Cow::Borrowed(message);
    if out.contains("info: ") {
        let mut clean_msg = String::with_capacity(out.len());
        for line in out.lines() {
            let mut l = line;
            if l.starts_with("info: ") {
                l = &l[6..];
            }
            clean_msg.push_str(l);
            clean_msg.push('\n');
        }
        out = Cow::Owned(clean_msg.trim_end().to_string());
    }
    if out.contains("\\n") || out.contains("\\t") || out.contains('\r') {
        out = Cow::Owned(
            out.replace("\\n", "\n")
                .replace("\\t", "    ")
                .replace('\r', ""),
        );
    }
    out
}

fn parse_borrowed_text_edit_value(v: &BorrowedTextEdit<'_>) -> TextChange {
    TextChange {
        start_line: v.range.start.line,
        start_col: v.range.start.character,
        end_line: v.range.end.line,
        end_col: v.range.end.character,
        new_text: v.new_text.to_string(),
    }
}

fn parse_borrowed_diagnostic_value(v: &BorrowedDiagnostic<'_>) -> Diagnostic {
    let severity = match v.severity.unwrap_or(1) {
        1 => DiagSeverity::Error,
        2 => DiagSeverity::Warning,
        3 => DiagSeverity::Info,
        _ => DiagSeverity::Hint,
    };

    let code = v.code.as_ref().map(|code| match code {
        BorrowedCode::Text(text) => Arc::<str>::from(*text),
        BorrowedCode::Number(value) => Arc::<str>::from(value.to_string()),
    });

    let mut quickfixes = Vec::new();
    if let Some(BorrowedDiagnosticData::QuickFix(data)) = &v.data
        && let Some(title) = data.title
    {
        let edits = data
            .edits
            .iter()
            .map(parse_borrowed_text_edit_value)
            .collect::<Vec<_>>();
        if !edits.is_empty() {
            quickfixes.push(QuickFix {
                title: title.to_string(),
                edits,
            });
        }
    }

    let message = clean_diagnostic_message(&v.message);
    Diagnostic {
        start_line: v.range.start.line,
        start_col: v.range.start.character,
        end_line: v.range.end.line,
        end_col: v.range.end.character,
        severity,
        code,
        code_href: v
            .code_description
            .as_ref()
            .and_then(|code| code.href)
            .map(Arc::<str>::from),
        message: Arc::<str>::from(message.as_ref()),
        source: v.source.map(Arc::<str>::from),
        quickfixes: quickfixes.into_boxed_slice(),
        tags: v.tags.clone().into_boxed_slice(),
    }
}

pub(super) fn parse_publish_diagnostics_frame(
    body: &[u8],
    server_name: &'static str,
) -> Result<LspEvent, serde_json::Error> {
    let frame: PublishDiagnosticsFrame<'_> = serde_json::from_slice(body)?;
    let items = frame
        .params
        .diagnostics
        .iter()
        .map(parse_borrowed_diagnostic_value)
        .collect::<Vec<_>>();
    Ok(LspEvent::Diagnostics {
        server_name,
        path: uri_to_path(frame.params.uri),
        version: frame.params.version,
        items,
        result_id: None,
    })
}

pub(super) fn parse_workspace_diagnostics_frame(
    body: &[u8],
    server_name: &'static str,
) -> Vec<LspEvent> {
    let Ok(frame) = serde_json::from_slice::<WorkspaceDiagnosticFrame<'_>>(body) else {
        return Vec::new();
    };
    let Some(result) = frame.result else {
        return Vec::new();
    };

    let mut events = Vec::new();
    for item in result.items {
        if let Some(uri) = item.uri {
            push_workspace_diagnostic_event(
                &mut events,
                server_name,
                uri,
                item.kind,
                item.version,
                item.result_id,
                &item.items,
            );
        }
        for (uri, report) in item.related_documents {
            push_workspace_diagnostic_event(
                &mut events,
                server_name,
                uri.as_ref(),
                report.kind,
                report.version,
                report.result_id,
                &report.items,
            );
        }
    }
    events
}

fn push_workspace_diagnostic_event(
    events: &mut Vec<LspEvent>,
    server_name: &'static str,
    uri: &str,
    kind: Option<&str>,
    version: Option<i32>,
    result_id: Option<&str>,
    items: &[BorrowedDiagnostic<'_>],
) {
    if kind == Some("unchanged") {
        return;
    }
    events.push(LspEvent::Diagnostics {
        server_name,
        path: uri_to_path(uri),
        version,
        items: items.iter().map(parse_borrowed_diagnostic_value).collect(),
        result_id: result_id.map(str::to_string),
    });
}

pub(super) fn parse_diagnostic_value(v: &serde_json::Value) -> Option<Diagnostic> {
    let range = v.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;

    let sl = start.get("line")?.as_u64()? as u32;
    let sc = start.get("character")?.as_u64()? as u32;
    let el = end.get("line")?.as_u64()? as u32;
    let ec = end.get("character")?.as_u64()? as u32;

    let severity = match v.get("severity").and_then(|s| s.as_u64()).unwrap_or(1) {
        1 => DiagSeverity::Error,
        2 => DiagSeverity::Warning,
        3 => DiagSeverity::Info,
        _ => DiagSeverity::Hint,
    };

    let message = v
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("");

    let code = v.get("code").and_then(|c| {
        if let Some(s) = c.as_str() {
            Some(Arc::<str>::from(s))
        } else if let Some(n) = c.as_u64() {
            Some(Arc::<str>::from(n.to_string()))
        } else {
            None
        }
    });

    let source = v
        .get("source")
        .and_then(|s| s.as_str())
        .map(Arc::<str>::from);

    let code_href = v
        .get("codeDescription")
        .and_then(|cd| cd.get("href"))
        .and_then(|h| h.as_str())
        .map(Arc::<str>::from);

    let mut tags = Vec::new();
    if let Some(tags_arr) = v.get("tags").and_then(|t| t.as_array()) {
        for t in tags_arr {
            if let Some(tag_id) = t.as_u64() {
                tags.push(tag_id as u32);
            }
        }
    }

    let mut quickfixes = Vec::new();
    if let Some(data) = v.get("data") {
        if let Some(title) = data.get("title").and_then(|t| t.as_str()) {
            if let Some(edits_arr) = data.get("edits").and_then(|e| e.as_array()) {
                let mut edits = Vec::new();
                for e in edits_arr {
                    if let Some(tc) = parse_text_edit_value(e) {
                        edits.push(tc);
                    }
                }
                if !edits.is_empty() {
                    quickfixes.push(QuickFix {
                        title: title.to_string(),
                        edits,
                    });
                }
            }
        }
    }

    let message = clean_diagnostic_message(message);

    Some(Diagnostic {
        start_line: sl,
        start_col: sc,
        end_line: el,
        end_col: ec,
        severity,
        code,
        code_href,
        message: Arc::<str>::from(message.as_ref()),
        source,
        quickfixes: quickfixes.into_boxed_slice(),
        tags: tags.into_boxed_slice(),
    })
}

pub(super) fn parse_text_edit_value(v: &serde_json::Value) -> Option<TextChange> {
    let range = v.get("range")?;
    let start = range.get("start")?;
    let end_r = range.get("end")?;

    let sl = start.get("line")?.as_u64()? as u32;
    let sc = start.get("character")?.as_u64()? as u32;
    let el = end_r.get("line")?.as_u64()? as u32;
    let ec = end_r.get("character")?.as_u64()? as u32;

    let new_text = v
        .get("newText")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    Some(TextChange {
        start_line: sl,
        start_col: sc,
        end_line: el,
        end_col: ec,
        new_text,
    })
}

fn parse_completion_text_edit_value(v: &serde_json::Value) -> Option<TextChange> {
    if let Some(change) = parse_text_edit_value(v) {
        return Some(change);
    }
    let replace = v.get("replace").or_else(|| v.get("insert"))?;
    let start = replace.get("start")?;
    let end_r = replace.get("end")?;
    let new_text = v.get("newText").and_then(|t| t.as_str()).unwrap_or("");
    Some(TextChange {
        start_line: start.get("line")?.as_u64()? as u32,
        start_col: start.get("character")?.as_u64()? as u32,
        end_line: end_r.get("line")?.as_u64()? as u32,
        end_col: end_r.get("character")?.as_u64()? as u32,
        new_text: new_text.to_string(),
    })
}

pub(super) fn parse_workspace_edit_value(v: &serde_json::Value) -> WorkspaceEdit {
    let mut edit = WorkspaceEdit::default();

    if let Some(changes) = v.get("changes").and_then(|c| c.as_object()) {
        for (uri, edits) in changes {
            let path = uri_to_path(uri);
            if let Some(arr) = edits.as_array() {
                let parsed_edits: Vec<TextChange> =
                    arr.iter().filter_map(parse_text_edit_value).collect();
                if !parsed_edits.is_empty() {
                    edit.changes.entry(path).or_default().extend(parsed_edits);
                }
            }
        }
    }

    if let Some(doc_changes) = v.get("documentChanges").and_then(|d| d.as_array()) {
        for item in doc_changes {
            if let Some(td) = item.get("textDocument") {
                if let Some(uri) = td.get("uri").and_then(|u| u.as_str()) {
                    let path = uri_to_path(uri);
                    if let Some(edits) = item.get("edits").and_then(|e| e.as_array()) {
                        let parsed_edits: Vec<TextChange> =
                            edits.iter().filter_map(parse_text_edit_value).collect();
                        if !parsed_edits.is_empty() {
                            edit.changes.entry(path).or_default().extend(parsed_edits);
                        }
                    }
                }
            }
        }
    }

    edit
}

pub(super) fn parse_hover_value(v: &serde_json::Value) -> Option<String> {
    let contents = v.get("contents")?;
    if let Some(s) = contents.as_str() {
        return Some(s.to_string());
    }
    if let Some(obj) = contents.as_object() {
        if let Some(val) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(val.to_string());
        }
    }
    if let Some(arr) = contents.as_array() {
        let mut out = String::new();
        for item in arr {
            if let Some(s) = item.as_str() {
                out.push_str(s);
                out.push('\n');
            } else if let Some(obj) = item.as_object() {
                if let Some(val) = obj.get("value").and_then(|v| v.as_str()) {
                    out.push_str(val);
                    out.push('\n');
                }
            }
        }
        return Some(out.trim_end().to_string());
    }
    None
}

fn definition_position(v: &serde_json::Value) -> Option<(u32, u32)> {
    let line = v.get("line")?.as_u64()? as u32;
    let col = v.get("character")?.as_u64()? as u32;
    Some((line, col))
}

pub(super) fn parse_definition_target(v: &serde_json::Value) -> Option<DefinitionTarget> {
    if let Some(uri) = v.get("uri").and_then(|u| u.as_str()) {
        let (line, col) = v
            .pointer("/range/start")
            .and_then(definition_position)
            .unwrap_or((0, 0));
        return Some(DefinitionTarget {
            path: uri_to_path(uri),
            line,
            col,
        });
    }
    if let Some(uri) = v.get("targetUri").and_then(|u| u.as_str()) {
        let (line, col) = v
            .pointer("/targetSelectionRange/start")
            .or_else(|| v.pointer("/targetRange/start"))
            .and_then(definition_position)
            .unwrap_or((0, 0));
        return Some(DefinitionTarget {
            path: uri_to_path(uri),
            line,
            col,
        });
    }
    if let Some(arr) = v.as_array() {
        for item in arr {
            if let Some(target) = parse_definition_target(item) {
                return Some(target);
            }
        }
    }
    None
}

pub(super) fn parse_code_action_value(v: &serde_json::Value) -> Option<CodeAction> {
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let kind = v
        .get("kind")
        .and_then(|k| k.as_str())
        .map(|s| s.to_string());
    let edit = v.get("edit").map(parse_workspace_edit_value);

    let mut code = None;
    if let Some(diags) = v.get("diagnostics").and_then(|d| d.as_array()) {
        if let Some(first) = diags.first() {
            if let Some(c) = first.get("code") {
                if let Some(s) = c.as_str() {
                    code = Some(s.to_string());
                } else if let Some(n) = c.as_u64() {
                    code = Some(n.to_string());
                }
            }
        }
    }

    Some(CodeAction {
        title,
        kind,
        edit,
        code,
    })
}

fn completion_kind(kind: Option<u64>) -> crate::highlighter::SymbolKind {
    match kind {
        Some(2 | 3 | 4) => crate::highlighter::SymbolKind::Function,
        Some(10) => crate::highlighter::SymbolKind::Property,
        Some(5 | 6 | 12 | 13 | 21) => crate::highlighter::SymbolKind::Variable,
        Some(7 | 8 | 22 | 25) => crate::highlighter::SymbolKind::Class,
        Some(9) => crate::highlighter::SymbolKind::Module,
        Some(14) => crate::highlighter::SymbolKind::Keyword,
        _ => crate::highlighter::SymbolKind::Unknown,
    }
}

fn refine_completion_kind(
    kind: crate::highlighter::SymbolKind,
    label: &str,
    detail: Option<&str>,
    insert_text: Option<&str>,
) -> crate::highlighter::SymbolKind {
    let Some(detail) = detail else {
        return if label.ends_with('=') || insert_text.is_some_and(|text| text.contains('=')) {
            crate::highlighter::SymbolKind::Parameter
        } else {
            kind
        };
    };
    if detail.starts_with("(parameter)")
        || label.ends_with('=')
        || insert_text.is_some_and(|text| text.contains('='))
    {
        crate::highlighter::SymbolKind::Parameter
    } else if detail.starts_with("(variable)") {
        crate::highlighter::SymbolKind::Variable
    } else if detail.starts_with("(property)") || detail.starts_with("(field)") {
        crate::highlighter::SymbolKind::Property
    } else if detail.starts_with("(function)")
        || detail.starts_with("(method)")
        || detail.starts_with("Overload[")
        || detail.starts_with("def ")
        || detail.starts_with("async def ")
    {
        crate::highlighter::SymbolKind::Function
    } else if detail.starts_with("class ") || detail.starts_with("type[") {
        crate::highlighter::SymbolKind::Class
    } else {
        kind
    }
}

fn completion_module(
    v: &serde_json::Value,
    kind: &crate::highlighter::SymbolKind,
    detail: Option<&str>,
) -> Option<String> {
    let label = v
        .get("label")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if let Some(full_name) = v
        .pointer("/data/fullName")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(owner) = owner_from_completion_detail(label, full_name) {
            return Some(owner);
        }
        return Some(full_name.to_string());
    }
    if let Some(owner) = v
        .pointer("/data/owner")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(owner.to_string());
    }
    let detail_owner = detail.and_then(|detail| owner_from_completion_detail(label, detail));
    if let Some(module) = detail.and_then(completion_import_detail_source) {
        return Some(module);
    }
    let detail_is_field_type = detail.is_some_and(|detail| {
        detail.starts_with("(variable)")
            || detail.starts_with("(parameter)")
            || detail.starts_with("(property)")
            || detail.starts_with("(field)")
    });
    if detail_is_field_type {
        return detail_owner.or_else(|| {
            v.pointer("/labelDetails/description")
                .and_then(|value| value.as_str())
                .filter(|desc| looks_like_python_module_path(desc))
                .and_then(completion_description_source)
        });
    }
    if !matches!(
        kind,
        crate::highlighter::SymbolKind::Variable
            | crate::highlighter::SymbolKind::Parameter
            | crate::highlighter::SymbolKind::Property
    ) {
        if let Some(module) = v
            .pointer("/data/module")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(owner) = detail_owner.as_deref().filter(|owner| {
                looks_like_python_module_path(module) && !module.ends_with(&format!(".{owner}"))
            }) {
                return Some(format!("{module}.{owner}"));
            }
            return Some(module.to_string());
        }
    }
    if let Some(owner) = detail_owner {
        return Some(owner);
    }
    if let Some(desc) = v
        .pointer("/labelDetails/description")
        .and_then(|value| value.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        if matches!(
            kind,
            crate::highlighter::SymbolKind::Variable
                | crate::highlighter::SymbolKind::Parameter
                | crate::highlighter::SymbolKind::Property
        ) && !looks_like_python_module_path(desc)
        {
            return None;
        }
        return completion_description_source(desc);
    }
    None
}

fn looks_like_python_module_path(text: &str) -> bool {
    let s = text.trim();
    s.contains('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn completion_description_source(desc: &str) -> Option<String> {
    let desc = desc.trim();
    if desc.is_empty()
        || desc.contains('/')
        || desc.contains('\\')
        || desc.contains('|')
        || desc.contains('[')
        || desc.contains(']')
        || desc.contains("->")
        || desc.starts_with("def ")
        || desc.starts_with("async def ")
        || desc.starts_with("overload[")
        || desc.starts_with('(')
        || !desc
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        || matches!(
            desc,
            "Any"
                | "None"
                | "bool"
                | "bytes"
                | "dict"
                | "float"
                | "int"
                | "list"
                | "set"
                | "str"
                | "tuple"
                | "type"
        )
    {
        None
    } else {
        Some(desc.to_string())
    }
}

fn completion_import_detail_source(detail: &str) -> Option<String> {
    let source = detail
        .trim()
        .strip_prefix("(import ")?
        .strip_suffix(')')?
        .trim();
    completion_description_source(source)
}

fn completion_documentation(v: &serde_json::Value) -> Option<&str> {
    let doc = v.get("documentation")?;
    doc.as_str()
        .or_else(|| doc.get("value").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn first_non_empty_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
}

fn first_signature_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.chars().all(|c| c == '-'))
        .find(|line| {
            line.starts_with("class ") || line.starts_with("def ") || line.starts_with("async def ")
        })
        .unwrap_or_else(|| first_non_empty_line(text))
}

fn completion_doc_is_richer_signature(label: &str, detail: &str, doc: &str) -> bool {
    if label.is_empty() || doc.len() <= detail.len() {
        return false;
    }
    let first = first_signature_line(doc);
    if first == detail {
        return false;
    }
    let class_prefix = format!("class {label}");
    let def_prefix = format!("def {label}");
    let async_def_prefix = format!("async def {label}");
    ((detail == class_prefix || detail.starts_with(&format!("{class_prefix}(")))
        && first.starts_with(&class_prefix)
        && first[class_prefix.len()..]
            .chars()
            .next()
            .is_some_and(|c| matches!(c, '[' | '(' | ':')))
        || ((detail == def_prefix || detail.starts_with(&format!("{def_prefix}(")))
            && first.starts_with(&def_prefix)
            && first[def_prefix.len()..]
                .chars()
                .next()
                .is_some_and(|c| matches!(c, '[' | '(')))
        || ((detail == async_def_prefix || detail.starts_with(&format!("{async_def_prefix}(")))
            && first.starts_with(&async_def_prefix)
            && first[async_def_prefix.len()..]
                .chars()
                .next()
                .is_some_and(|c| matches!(c, '[' | '(')))
}

fn completion_detail_with_attached_doc(detail: &str, doc: &str) -> Option<String> {
    let doc = doc.trim();
    if doc.is_empty() || doc == detail || detail.contains('\n') {
        return None;
    }
    let attachable = detail.starts_with("class ")
        || detail.starts_with("def ")
        || detail.starts_with("async def ")
        || detail.starts_with("Overload[");
    attachable.then(|| format!("{detail}\n---\n{doc}"))
}

fn completion_detail(v: &serde_json::Value) -> Option<String> {
    let label = v.get("label").and_then(|value| value.as_str()).unwrap_or("");
    let documentation = completion_documentation(v);
    if let Some(detail) = v
        .get("detail")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|detail| completion_detail_is_more_specific_than_label_detail(detail))
    {
        if let Some(doc) = documentation {
            if completion_doc_is_richer_signature(label, detail, doc) {
                return Some(doc.to_string());
            }
            if let Some(detail) = completion_detail_with_attached_doc(detail, doc) {
                return Some(detail);
            }
        }
        return Some(detail.to_string());
    }
    if let Some(label_detail) = v
        .pointer("/labelDetails/detail")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(doc) = documentation {
            if completion_doc_is_richer_signature(label, label_detail, doc) {
                return Some(doc.to_string());
            }
            if let Some(detail) = completion_detail_with_attached_doc(label_detail, doc) {
                return Some(detail);
            }
        }
        return Some(label_detail.to_string());
    }
    if let Some(detail) = v
        .get("detail")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(detail.to_string());
    }
    if let Some(doc) = documentation {
        return Some(doc.to_string());
    }
    None
}

fn completion_detail_is_more_specific_than_label_detail(detail: &str) -> bool {
    detail.starts_with("(variable)")
        || detail.starts_with("(parameter)")
        || detail.starts_with("(property)")
        || detail.starts_with("(field)")
        || detail.starts_with("(function)")
        || detail.starts_with("(method)")
        || detail.starts_with("Overload[")
        || detail.starts_with("def ")
        || detail.starts_with("async def ")
        || detail.starts_with("class ")
        || detail.starts_with("type[")
}

fn owner_from_completion_detail(label: &str, detail: &str) -> Option<String> {
    if label.is_empty() {
        return None;
    }
    let needle = format!(".{label}");
    let idx = detail.find(&needle)?;
    let before = &detail[..idx];
    let owner_start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.'))
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    let owner = before[owner_start..].trim().trim_matches('`');
    (!owner.is_empty()).then(|| owner.to_string())
}

pub(super) fn parse_completion_item_value(v: &serde_json::Value) -> Option<LspCompletionItem> {
    let label = v.get("label")?.as_str()?.to_string();
    let insert_text = v
        .get("insertText")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| {
            v.pointer("/textEdit/newText")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        });
    let text_edit = v.get("textEdit").and_then(parse_completion_text_edit_value);
    let additional_text_edits = v
        .get("additionalTextEdits")
        .and_then(|value| value.as_array())
        .map(|items| items.iter().filter_map(parse_text_edit_value).collect())
        .unwrap_or_default();

    let detail = completion_detail(v);
    let kind = refine_completion_kind(
        completion_kind(v.get("kind").and_then(|value| value.as_u64())),
        &label,
        detail.as_deref(),
        insert_text.as_deref(),
    );
    Some(LspCompletionItem {
        label,
        module: completion_module(v, &kind, detail.as_deref()),
        kind,
        detail,
        insert_text,
        text_edit,
        additional_text_edits,
    })
}

pub(super) fn parse_completion_items(result: &serde_json::Value) -> Vec<LspCompletionItem> {
    let items = if let Some(arr) = result.as_array() {
        arr
    } else if let Some(arr) = result.get("items").and_then(|value| value.as_array()) {
        arr
    } else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(parse_completion_item_value)
        .collect()
}

fn signature_parameter_name(label: &str) -> Option<String> {
    let mut label = label.trim();
    while let Some(rest) = label
        .strip_prefix('*')
        .or_else(|| label.strip_prefix(','))
        .map(str::trim_start)
    {
        label = rest;
    }
    let name_end = label
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map(|(idx, _)| idx)
        .unwrap_or(label.len());
    let name = label.get(..name_end)?.trim();
    if name.is_empty()
        || matches!(name, "self" | "cls" | "args" | "kwargs")
        || name.as_bytes()[0].is_ascii_digit()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_signature_parameter_label(
    signature_label: &str,
    parameter: &serde_json::Value,
) -> Option<String> {
    let label = parameter.get("label")?;
    if let Some(text) = label.as_str() {
        return signature_parameter_name(text);
    }
    let range = label.as_array()?;
    let start = range.first()?.as_u64()? as usize;
    let end = range.get(1)?.as_u64()? as usize;
    signature_label
        .get(start..end)
        .and_then(signature_parameter_name)
}

pub(super) fn parse_signature_help_parameters(result: &serde_json::Value) -> Vec<String> {
    let signatures = match result.get("signatures").and_then(|value| value.as_array()) {
        Some(signatures) if !signatures.is_empty() => signatures,
        _ => return Vec::new(),
    };
    let active = result
        .get("activeSignature")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let signature = signatures.get(active).or_else(|| signatures.first());
    let Some(signature) = signature else {
        return Vec::new();
    };
    let signature_label = signature
        .get("label")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let Some(parameters) = signature.get("parameters").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        if let Some(name) = parse_signature_parameter_label(signature_label, parameter)
            && !out.contains(&name)
        {
            out.push(name);
        }
    }
    out
}

fn parse_inlay_hint_label(v: &serde_json::Value) -> Option<String> {
    if let Some(label) = v.as_str() {
        return Some(label.to_string());
    }
    let parts = v.as_array()?;
    let mut out = String::new();
    for part in parts {
        if let Some(value) = part.as_str() {
            out.push_str(value);
        } else if let Some(value) = part.get("value").and_then(|value| value.as_str()) {
            out.push_str(value);
        }
    }
    (!out.is_empty()).then_some(out)
}

pub(super) fn parse_inlay_hint_value(v: &serde_json::Value) -> Option<LspInlayHint> {
    let pos = v.get("position")?;
    let line = pos.get("line")?.as_u64()? as u32;
    let col = pos.get("character")?.as_u64()? as u32;
    let label = parse_inlay_hint_label(v.get("label")?)?;
    Some(LspInlayHint { line, col, label })
}

pub(super) fn parse_inlay_hints(result: &serde_json::Value) -> Vec<LspInlayHint> {
    let Some(items) = result.as_array() else {
        return Vec::new();
    };
    items.iter().filter_map(parse_inlay_hint_value).collect()
}
