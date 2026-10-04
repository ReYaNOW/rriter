impl LspManager {
    /// Запрашивает code actions для позиции/диагностики
    pub fn request_code_actions(
        &mut self,
        path: &PathBuf,
        ext: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        relevant_diags: &[Diagnostic],
        only: Option<Vec<String>>,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let proc = self.action_process_for_document(&abs_path, ext)?;
        proc.request_code_actions(
            &abs_path,
            start_line,
            start_col,
            end_line,
            end_col,
            relevant_diags,
            only,
        )
    }

    fn request_source_action(
        &mut self,
        path: &PathBuf,
        ext: &str,
        action_kind: &str,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(workspace) = self.workspaces.first() {
            workspace.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.action_process_for_document(&abs_path, ext)?
            .request_code_actions(
                &abs_path,
                0,
                0,
                u32::MAX,
                0,
                &[],
                Some(vec![action_kind.to_string()]),
            )
    }
    /// Запрос на глобальный fix-all (source.fixAll) для текущего файла
    pub fn request_fix_all(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        self.request_source_action(path, ext, "source.fixAll")
    }

    pub fn request_organize_imports(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        self.request_source_action(path, ext, "source.organizeImports")
    }

    fn stop_processes(&mut self) {
        self.python_disabled = true;
        self.dart.set_enabled(false);
        for state in self.dart_jobs.values_mut() { state.cancel_job(); }
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
    }
    #[allow(dead_code)]
    pub fn shutdown(mut self) {
        self.stop_processes();
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        self.stop_processes();
    }
}

/// Конвертирует LSP {line, character} → байтовый offset в тексте.
/// Нужно для применения TextChange к буферу редактора.
pub fn lsp_pos_to_offset(text: &str, line: u32, col: u32) -> usize {
    let mut cur_line = 0u32;
    let mut cur_col = 0u32; // UTF-16 единицы

    for (i, ch) in text.char_indices() {
        if cur_line == line && cur_col >= col {
            return i;
        }
        if ch == '\n' {
            if cur_line == line {
                return i;
            }
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += ch.len_utf16() as u32;
        }
    }

    if cur_line == line && cur_col >= col {
        return text.len();
    }

    text.len()
}

/// Применяет WorkspaceEdit к строке текста (для текущего файла).
/// Правки должны быть отсортированы с конца файла к началу, чтобы offset'ы не съехали.

#[allow(dead_code)]
pub fn format_and_highlight_json(
    raw_text: &str,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<(usize, usize, usize)>,
) {
    let (prefix, content) = if raw_text.starts_with("[LSP RECV] ") {
        ("[LSP RECV]\n", &raw_text[11..])
    } else if raw_text.starts_with("[LSP SEND] ") {
        ("[LSP SEND]\n", &raw_text[11..])
    } else {
        ("", raw_text)
    };

    let is_json = content.trim().starts_with('{') || content.trim().starts_with('[');
    let pretty = if is_json {
        match serde_json::from_str::<serde_json::Value>(content) {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| content.to_string()),
            Err(_) => content.to_string(),
        }
    } else {
        content.to_string()
    };

    let mut parser = tree_sitter::Parser::new();
    let lang = if is_json {
        tree_sitter_json::LANGUAGE.into()
    } else {
        tree_sitter_bash::LANGUAGE.into()
    };
    let _ = parser.set_language(&lang);

    let mut final_string = String::from(prefix);
    let mut spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: prefix.len(),
        role: if prefix.contains("RECV") {
            crate::theme::SyntaxRole::Function
        } else {
            crate::theme::SyntaxRole::Keyword
        },
    }];
    let mut folds = Vec::new();

    if is_json {
        let tree = parser.parse(&pretty, None).unwrap();

        if let Some(fold_q) = crate::queries::get_folding_query("json") {
            if let Ok(query) = tree_sitter::Query::new(&lang, fold_q) {
                let mut cursor = tree_sitter::QueryCursor::new();
                let mut matches = cursor.matches(&query, tree.root_node(), pretty.as_bytes());
                while let Some(m) = matches.next() {
                    for cap in m.captures {
                        let node = cap.node;
                        if node.end_position().row > node.start_position().row + 1 {
                            folds.push((
                                node.start_byte() + prefix.len(),
                                node.end_byte() + prefix.len(),
                                json_container_depth(node),
                            ));
                        }
                    }
                }
            }
        }

        if let Some((_, queries)) = crate::queries::get_ts_config("json") {
            for q in queries {
                if let Ok(query) = tree_sitter::Query::new(&lang, q) {
                    let mut cursor = tree_sitter::QueryCursor::new();
                    let mut matches = cursor.matches(&query, tree.root_node(), pretty.as_bytes());
                    while let Some(m) = matches.next() {
                        for cap in m.captures {
                            let name = query.capture_names()[cap.index as usize];
                            let role = match name {
                                "property" => crate::theme::SyntaxRole::Keyword,
                                "string" => crate::theme::SyntaxRole::String,
                                "number" => crate::theme::SyntaxRole::Constant,
                                "boolean" | "keyword.control" => crate::theme::SyntaxRole::KeywordControl,
                                "comment" => crate::theme::SyntaxRole::Comment,
                                _ => continue,
                            };
                            spans.push(crate::highlighter::ColorSpan {
                                start: cap.node.start_byte() + prefix.len(),
                                end: cap.node.end_byte() + prefix.len(),
                                role,
                            });
                        }
                    }
                }
            }
        }
        final_string.push_str(&pretty);
    } else {
        final_string.push_str(&pretty);
        spans.push(crate::highlighter::ColorSpan {
            start: prefix.len(),
            end: final_string.len(),
            role: crate::theme::SyntaxRole::LogText,
        });
    }

    spans.sort_by_key(|s| s.start);
    (final_string, spans, folds)
}
#[allow(dead_code)]
fn json_container_depth(node: tree_sitter::Node<'_>) -> usize {
    let mut depth = 1;
    let mut parent = node.parent();
    while let Some(p) = parent {
        if matches!(p.kind(), "object" | "array") {
            depth += 1;
        }
        parent = p.parent();
    }
    depth
}
