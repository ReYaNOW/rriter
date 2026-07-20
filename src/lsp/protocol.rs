use super::hover::PendingRequestKind;
use super::*;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use tree_sitter::StreamingIterator;

pub fn highlight_diagnostic_message(msg: &str) -> Vec<crate::highlighter::ColorSpan> {
    let mut spans = Vec::new();
    let mut backtick_ranges = Vec::new();
    let mut in_tick = false;
    let mut tick_start = 0;

    for (offset, c) in msg.char_indices() {
        if c == '`' {
            if in_tick {
                backtick_ranges.push((tick_start, offset));
                in_tick = false;
            } else {
                tick_start = offset + 1;
                in_tick = true;
            }
            spans.push(crate::highlighter::ColorSpan {
                start: offset,
                end: offset + c.len_utf8(),
                color: [0.6, 0.6, 0.65, 1.0],
            });
        } else if c == '├' || c == '─' || c == '│' || c == '└' {
            spans.push(crate::highlighter::ColorSpan {
                start: offset,
                end: offset + c.len_utf8(),
                color: [0.45, 0.45, 0.50, 1.0],
            });
        }
    }

    if !backtick_ranges.is_empty() {
        crate::languages::python::TS_DIAG_PARSER.with(|p_cell| {
            crate::languages::python::TS_DIAG_QUERY.with(|q_cell| {
                crate::languages::python::TS_DIAG_CURSOR.with(|c_cell| {
                    let mut parser = p_cell.borrow_mut();
                    let query_opt = q_cell.borrow();
                    let mut cursor = c_cell.borrow_mut();

                    if let Some(query) = query_opt.as_ref() {
                        for &(start, end) in &backtick_ranges {
                            if start >= end {
                                continue;
                            }
                            let code = &msg[start..end];
                            if let Some(tree) = parser.parse(code, None) {
                                let mut matches =
                                    cursor.matches(query, tree.root_node(), code.as_bytes());
                                while let Some(m) = matches.next() {
                                    for cap in m.captures {
                                        let name = query.capture_names()[cap.index as usize];
                                        let color = match name {
                                            "property" | "variable" => [0.972, 0.972, 0.949, 1.0],
                                            "string" => [0.945, 0.980, 0.549, 1.0],
                                            "type" | "class_name" => [0.545, 0.913, 0.992, 1.0],
                                            "keyword.control" | "keyword" | "operator" => {
                                                [1.0, 0.474, 0.776, 1.0]
                                            }
                                            "function" | "py_function" | "py_builtin_or_func" => {
                                                [0.313, 0.980, 0.482, 1.0]
                                            }
                                            "number" => [0.741, 0.576, 0.976, 1.0],
                                            "comment" => [0.384, 0.447, 0.643, 1.0],
                                            _ => [0.972, 0.972, 0.949, 1.0],
                                        };
                                        if color != [0.972, 0.972, 0.949, 1.0] {
                                            spans.push(crate::highlighter::ColorSpan {
                                                start: start + cap.node.start_byte(),
                                                end: start + cap.node.end_byte(),
                                                color,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            })
        });
    }

    spans.sort_unstable_by_key(|s| s.start);
    spans
}

/// Одна замена текста (из workspace/applyEdit или codeAction)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextChange {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub new_text: String,
}

/// Набор правок по файлам
#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub changes: HashMap<PathBuf, Vec<TextChange>>,
}

/// Виртуальная closing label от language server. Координаты используют UTF-16, как LSP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspClosingLabel {
    pub label: String,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// Событие от LSP-сервера → главный поток
#[derive(Debug)]
pub enum LspEvent {
    Log {
        name: &'static str,
        message: String,
    },
    /// Диагностика для файла от типизированного LSP-сервера
    Diagnostics {
        server: LspServerKind,
        path: PathBuf,
        #[allow(dead_code)]
        version: Option<i32>,
        items: Vec<Diagnostic>,
        result_id: Option<String>,
    },
    /// Виртуальные подписи закрывающих конструкций для одного документа.
    ClosingLabels {
        server: LspServerKind,
        path: PathBuf,
        labels: Vec<LspClosingLabel>,
    },
    ConfigurationServed {
        server: LspServerKind,
    },
    WorkspaceDiagnosticsDone {
        request_id: i32,
    },
    /// Ответ на запрос codeAction (исправления от ruff)
    CodeActions {
        request_id: i32,
        actions: Vec<CodeAction>,
    },
    /// Сервер готов принимать запросы
    ServerReady {
        server: LspServerKind,
    },
    /// Статус сервера изменился
    StatusChanged {
        server: LspServerKind,
        status: LspServerStatus,
    },
    HoverResponse {
        request_id: i32,
        text: Option<String>,
    },
    DefinitionResponse {
        request_id: i32,
        target: Option<DefinitionTarget>,
    },
    CompletionResponse {
        request_id: i32,
        items: Vec<LspCompletionItem>,
        is_incomplete: bool,
    },
    SignatureHelpResponse {
        request_id: i32,
        help: LspSignatureHelp,
    },
    InlayHintsResponse {
        request_id: i32,
        hints: Vec<LspInlayHint>,
    },
    ReferencesResponse {
        request_id: i32,
        targets: Vec<DefinitionTarget>,
    },
    PrepareRenameResponse {
        request_id: i32,
        range: Option<TextChange>,
    },
    RenameResponse {
        request_id: i32,
        edit: WorkspaceEdit,
    },
    FormattingResponse {
        request_id: i32,
        edits: Vec<TextChange>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LspServerKind {
    Ruff,
    Ty,
    Dart,
}

impl LspServerKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ruff => "ruff",
            Self::Ty => "ty",
            Self::Dart => "dart",
        }
    }

    pub const fn restart_attempt_limit(self) -> u8 {
        match self {
            Self::Ruff | Self::Ty | Self::Dart => 4,
        }
    }

    #[cfg(test)]
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "ruff" => Some(Self::Ruff),
            "ty" => Some(Self::Ty),
            "dart" => Some(Self::Dart),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionTarget {
    pub path: PathBuf,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LspCompletionItem {
    pub label: String,
    pub kind: crate::highlighter::SymbolKind,
    pub module: Option<String>,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub text_edit: Option<TextChange>,
    pub additional_text_edits: Vec<TextChange>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LspSignatureHelp {
    pub signatures: Vec<LspSignature>,
    pub active_signature: usize,
    pub active_parameter: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LspSignature {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<LspSignatureParameter>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LspSignatureParameter {
    pub label: String,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspInlayHint {
    pub line: u32,
    pub col: u32,
    pub label: String,
}

// ── Конфигурация LSP-серверов ─────────────────────────────────────────────────

pub(super) struct LspServerDef {
    pub(super) kind: LspServerKind,
    pub(super) program: &'static str,
    pub(super) override_env: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) language_id: &'static str,
    #[allow(dead_code)]
    pub(super) extensions: &'static [&'static str],
}

pub(super) const RUFF_SERVER: LspServerDef = LspServerDef {
    kind: LspServerKind::Ruff,
    program: "ruff",
    override_env: "RRITER_RUFF_PATH",
    args: &["server"],
    language_id: "python",
    extensions: &["py"],
};

pub(super) const TY_SERVER: LspServerDef = LspServerDef {
    kind: LspServerKind::Ty,
    program: "ty",
    override_env: "RRITER_TY_PATH",
    args: &["server"],
    language_id: "python",
    extensions: &["py"],
};

pub(super) const DART_SERVER: LspServerDef = LspServerDef {
    kind: LspServerKind::Dart,
    program: "dart",
    override_env: "RRITER_DART_PATH",
    args: &["language-server"],
    language_id: "dart",
    extensions: &["dart"],
};

// ── Внутренние команды main → supervisor ─────────────────────────────────────

pub(super) enum Cmd {
    /// Открыть файл (didOpen)
    Open {
        uri: String,
        lang: &'static str,
        version: i32,
        text: Arc<str>,
    },
    /// Изменить файл (didChange, полный текст)
    Change {
        uri: String,
        version: i32,
        text: Arc<str>,
    },
    /// Закрыть файл (didClose)
    Close {
        #[allow(dead_code)]
        uri: String,
    },
    /// Запросить codeActions для позиции
    CodeAction {
        id: i32,
        uri: String,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        /// JSON-encoded массив диагностик для контекста (для ruff это важно)
        diagnostics_json: String,
        only: Option<Vec<String>>,
    },
    Shutdown,
    Hover {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
    },
    Definition {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
    },
    References {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
        include_declaration: bool,
    },
    PrepareRename {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
    },
    Rename {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
        new_name: String,
    },
    Completion {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
        trigger: Option<String>,
    },
    SignatureHelp {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
        trigger: Option<String>,
    },
    InlayHint {
        id: i32,
        uri: String,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    },
    Formatting {
        id: i32,
        uri: String,
        tab_size: u32,
        insert_spaces: bool,
    },
    WorkspaceDiagnostic {
        id: i32,
        previous_result_ids_json: String,
    },
}

// ── Конвертация byte-offset → LSP Position ───────────────────────────────────

/// Конвертирует байтовый offset в LSP Position {line, character}.
/// `line_offsets[i]` = байтовый offset начала строки i.
/// LSP character = UTF-16 code units (для ASCII = байты).
#[inline]
#[allow(dead_code)]
pub fn offset_to_lsp_pos(text: &str, offset: usize, line_offsets: &[usize]) -> (u32, u32) {
    let offset = offset.min(text.len());
    // Бинарный поиск строки
    let line = line_offsets
        .partition_point(|&o| o <= offset)
        .saturating_sub(1);
    let line_start = line_offsets.get(line).copied().unwrap_or(0);
    let col_bytes = offset.saturating_sub(line_start);

    // Считаем UTF-16 единицы (для ASCII — тривиально; для Unicode — точно)
    let line_slice_end = (line_start + col_bytes).min(text.len());
    let line_slice = text.get(line_start..line_slice_end).unwrap_or("");
    let utf16_col: u32 = line_slice.chars().map(|c| c.len_utf16() as u32).sum();

    (line as u32, utf16_col)
}

include!("protocol/protocol_json_rpc_encoding.rs");
include!("protocol/protocol_value_parsers.rs");


#[cfg(test)]
#[path = "protocol_tests.rs"]
mod protocol_tests;
