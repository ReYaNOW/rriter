pub struct LspServerSummary<'a> {
    pub name: &'static str,
    pub status: &'a LspServerStatus,
    pub log_count: usize,
}

pub struct LspManager {
    python: Option<LspProcess>,
    ty_process: Option<LspProcess>,
    workspaces: Vec<PathBuf>,
    /// Актуальные диагностики для каждого открытого файла
    pub diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
    pub instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    pub merged_instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    pub ty_instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    ty_diag_result_ids: HashMap<PathBuf, String>,
    ty_workspace_diag_pending: Option<i32>,
    ty_workspace_diag_dirty: bool,
    pub dirty_diagnostics: bool,
    pub last_change: Option<std::time::Instant>,
    current_path: Option<PathBuf>,
    current_python_file: Option<(PathBuf, String, i32)>,
    /// Статус ruff сервера
    pub python_status: LspServerStatus,
    pub ty_status: LspServerStatus,
    /// Отключён ли ruff вручную
    pub python_disabled: bool,
    pub server_logs: HashMap<&'static str, Vec<LogEntry>>,
    pub suppress_diagnostics: bool,
}

impl LspManager {
    fn is_python_ext(ext: &str) -> bool {
        matches!(ext, "py" | "pyi")
    }

    pub fn new(workspaces: Vec<PathBuf>) -> Self {
        LspManager {
            python: None,
            ty_process: None,
            workspaces,
            diagnostics: HashMap::new(),
            instant_diagnostics: HashMap::new(),
            ty_instant_diagnostics: HashMap::new(),
            merged_instant_diagnostics: HashMap::new(),
            ty_diag_result_ids: HashMap::new(),
            ty_workspace_diag_pending: None,
            ty_workspace_diag_dirty: false,
            dirty_diagnostics: false,
            last_change: None,
            current_path: None,
            current_python_file: None,
            python_status: LspServerStatus::Disabled,
            ty_status: LspServerStatus::Disabled,
            python_disabled: false,
            server_logs: HashMap::new(),
            suppress_diagnostics: false,
        }
    }

    /// Запускает нужный LSP-сервер если ещё не запущен (lazy)
    fn ensure_python(&mut self) {
        if self.python.is_none() && !self.python_disabled {
            self.python_status = LspServerStatus::Starting;
            self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspaces.clone()));
        }
        if self.ty_process.is_none() && !self.python_disabled {
            self.ty_status = LspServerStatus::Starting;
            self.ty_process = Some(LspProcess::start(&TY_SERVER, self.workspaces.clone()));
            self.ty_diag_result_ids.clear();
            self.ty_workspace_diag_pending = None;
            self.ty_workspace_diag_dirty = true;
        }
    }

    /// Перезапустить ruff сервер
    pub fn restart_python(&mut self) {
        if let Some(proc) = &mut self.python {
            proc.restart();
            self.python_status = LspServerStatus::Starting;
        } else if !self.python_disabled {
            self.python_status = LspServerStatus::Starting;
            self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspaces.clone()));
        }
        if let Some(proc) = &mut self.ty_process {
            proc.restart();
            self.ty_status = LspServerStatus::Starting;
            self.ty_diag_result_ids.clear();
            self.ty_workspace_diag_pending = None;
            self.ty_workspace_diag_dirty = true;
        } else if !self.python_disabled {
            self.ty_status = LspServerStatus::Starting;
            self.ty_process = Some(LspProcess::start(&TY_SERVER, self.workspaces.clone()));
            self.ty_diag_result_ids.clear();
            self.ty_workspace_diag_pending = None;
            self.ty_workspace_diag_dirty = true;
        }
    }

    /// Отключить ruff (остановить и не перезапускать)
    pub fn disable_python(&mut self) {
        self.python_disabled = true;
        self.python_status = LspServerStatus::Disabled;
        self.ty_status = LspServerStatus::Disabled;
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
        self.diagnostics.clear();
        self.instant_diagnostics.clear();
        self.ty_instant_diagnostics.clear();
        self.merged_instant_diagnostics.clear();
        self.ty_diag_result_ids.clear();
        self.ty_workspace_diag_pending = None;
        self.ty_workspace_diag_dirty = false;
        self.dirty_diagnostics = false;
        self.server_logs.clear();
    }

    /// Включить ruff обратно
    pub fn enable_python(&mut self) {
        self.python_disabled = false;
        self.python_status = LspServerStatus::Starting;
        self.ty_status = LspServerStatus::Starting;
        self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspaces.clone()));
        self.ty_process = Some(LspProcess::start(&TY_SERVER, self.workspaces.clone()));
        self.ty_diag_result_ids.clear();
        self.ty_workspace_diag_pending = None;
        self.ty_workspace_diag_dirty = true;
        self.reopen_current_python_file();
    }

    fn reopen_current_python_file(&mut self) {
        let Some((path, text, version)) = self.current_python_file.clone() else {
            return;
        };
        let ws = self.workspaces.first().cloned();
        if let Some(proc) = &mut self.python {
            proc.notify_open(&path, &text, version, ws.as_ref());
        }
        if let Some(proc) = &mut self.ty_process {
            proc.notify_open(&path, &text, version, ws.as_ref());
        }
    }

    /// Лёгкая информация о серверах без клонирования логов.
    pub fn server_summaries(&self) -> [LspServerSummary<'_>; 2] {
        [
            LspServerSummary {
                name: RUFF_SERVER.program,
                status: &self.python_status,
                log_count: self
                    .server_logs
                    .get(RUFF_SERVER.program)
                    .map_or(0, Vec::len),
            },
            LspServerSummary {
                name: TY_SERVER.program,
                status: &self.ty_status,
                log_count: self.server_logs.get(TY_SERVER.program).map_or(0, Vec::len),
            },
        ]
    }

    /// Информация о серверах для UI
    pub fn servers_info(&self) -> Vec<LspServerInfo> {
        let logs = self
            .server_logs
            .get(RUFF_SERVER.program)
            .cloned()
            .unwrap_or_default();
        let ty_logs = self
            .server_logs
            .get(TY_SERVER.program)
            .cloned()
            .unwrap_or_default();
        vec![
            LspServerInfo {
                name: RUFF_SERVER.program,
                status: self.python_status.clone(),
                logs,
            },
            LspServerInfo {
                name: TY_SERVER.program,
                status: self.ty_status.clone(),
                logs: ty_logs,
            },
        ]
    }

    pub fn clear_server_logs(&mut self, name: &str) {
        self.server_logs.remove(name);
    }

    /// Возвращает процесс для нужного расширения, запустив при необходимости
    fn process_for_ext(&mut self, ext: &str) -> Option<&mut LspProcess> {
        match ext {
            "py" | "pyi" => {
                self.ensure_python();
                self.python.as_mut()
            }
            _ => None,
        }
    }

    /// Уведомляет LSP об открытии файла
    pub fn notify_open(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32) {
        self.suppress_diagnostics = false;
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.current_path = Some(abs_path.clone());
        let ws = self.workspaces.first().cloned();
        if Self::is_python_ext(ext) {
            self.current_python_file = Some((abs_path.clone(), text.to_string(), version));
            self.ensure_python();
            if let Some(proc) = &mut self.python {
                proc.notify_open(&abs_path, text, version, ws.as_ref());
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_open(&abs_path, text, version, ws.as_ref());
            }
            self.ty_workspace_diag_dirty = true;
        } else {
            self.current_python_file = None;
        }
    }

    /// Уведомляет LSP об изменении файла (когда sync_edits непуст)
    pub fn notify_change(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32) {
        self.suppress_diagnostics = false;
        self.last_change = Some(std::time::Instant::now());
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if Self::is_python_ext(ext) {
            self.current_path = Some(abs_path.clone());
            self.current_python_file = Some((abs_path.clone(), text.to_string(), version));
            self.ensure_python();
            if let Some(proc) = &mut self.python {
                proc.notify_change(&abs_path, text, version);
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_change(&abs_path, text, version);
            }
            self.ty_workspace_diag_dirty = true;
        }
    }

    pub fn request_hover(
        &mut self,
        path: &PathBuf,
        _ext: &str,
        line: u32,
        col: u32,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if let Some(proc) = &mut self.ty_process {
            Some(proc.request_hover(&abs_path, line, col))
        } else {
            None
        }
    }

    pub fn request_definition(
        &mut self,
        path: &PathBuf,
        _ext: &str,
        line: u32,
        col: u32,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if let Some(proc) = &mut self.ty_process {
            Some(proc.request_definition(&abs_path, line, col))
        } else {
            None
        }
    }

    pub fn request_ty_completion(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        if !Self::is_python_ext(ext) {
            return None;
        }
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.ensure_python();
        self.ty_process
            .as_mut()
            .map(|proc| proc.request_completion(&abs_path, line, col, trigger))
    }

    pub fn request_ty_signature_help(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        if !Self::is_python_ext(ext) {
            return None;
        }
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.ensure_python();
        self.ty_process
            .as_mut()
            .map(|proc| proc.request_signature_help(&abs_path, line, col, trigger))
    }

    pub fn request_ty_inlay_hints(
        &mut self,
        path: &PathBuf,
        ext: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Option<i32> {
        if !Self::is_python_ext(ext) {
            return None;
        }
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.ensure_python();
        self.ty_process.as_mut().map(|proc| {
            proc.request_inlay_hints(&abs_path, start_line, start_col, end_line, end_col)
        })
    }

    /// Уведомляет LSP о закрытии файла
    pub fn notify_close(&mut self, path: &PathBuf, ext: &str) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if Self::is_python_ext(ext) {
            if self.current_path.as_ref() == Some(&abs_path) {
                self.current_path = None;
            }
            if matches!(self.current_python_file.as_ref(), Some((path, _, _)) if path == &abs_path)
            {
                self.current_python_file = None;
            }
            if let Some(proc) = &mut self.python {
                proc.notify_close(&abs_path);
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_close(&abs_path);
            }
            self.ty_workspace_diag_dirty = true;
        }
    }

    fn ty_workspace_result_ids_json(&self) -> String {
        if self.ty_diag_result_ids.is_empty() {
            return String::from("[]");
        }

        let mut items = Vec::with_capacity(self.ty_diag_result_ids.len());
        for (path, value) in &self.ty_diag_result_ids {
            let uri = path_to_uri(&path.to_string_lossy());
            items.push(format!(
                r#"{{"uri":"{}","value":"{}"}}"#,
                json_escape(&uri),
                json_escape(value)
            ));
        }
        format!("[{}]", items.join(","))
    }

    fn request_ty_workspace_diagnostics_if_ready(&mut self) {
        if !self.ty_workspace_diag_dirty
            || self.ty_workspace_diag_pending.is_some()
            || self.python_disabled
            || self.suppress_diagnostics
            || self.ty_status != LspServerStatus::Running
        {
            return;
        }
        if self
            .last_change
            .is_some_and(|last| last.elapsed().as_secs_f32() < 3.0)
        {
            return;
        }

        let previous_result_ids_json = self.ty_workspace_result_ids_json();
        if let Some(proc) = &mut self.ty_process {
            let id = proc.request_workspace_diagnostics(previous_result_ids_json);
            self.ty_workspace_diag_pending = Some(id);
            self.ty_workspace_diag_dirty = false;
        }
    }

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
        let proc = self.process_for_ext(ext)?;
        Some(proc.request_code_actions(
            &abs_path,
            start_line,
            start_col,
            end_line,
            end_col,
            relevant_diags,
            only,
        ))
    }

    /// Опрашивает события от всех серверов. Вызывать раз в кадр.
    /// Обновляет self.diagnostics при получении новых диагностик.
    pub fn poll(&mut self) -> Vec<LspEvent> {
        let mut all = Vec::new();

        if let Some(proc) = &self.python {
            proc.poll(&mut all);
        }
        if let Some(proc) = &mut self.ty_process {
            proc.poll(&mut all);
        }

        // Обновляем кешированные диагностики и статусы
        for ev in &mut all {
            match ev {
                LspEvent::Diagnostics {
                    server_name,
                    path,
                    version,
                    items,
                    result_id,
                    ..
                } => {
                    if !self.suppress_diagnostics {
                        let v = version.unwrap_or(0);

                        if *server_name == TY_SERVER.program {
                            if let Some(result_id) = result_id.as_ref() {
                                self.ty_diag_result_ids
                                    .insert(path.clone(), result_id.clone());
                            }
                            self.ty_instant_diagnostics
                                .insert(path.clone(), (v, items.clone()));
                        } else {
                            self.instant_diagnostics
                                .insert(path.clone(), (v, items.clone()));
                        }

                        let (max_v, merged) = merged_diagnostics_for_path(
                            path,
                            &self.instant_diagnostics,
                            &self.ty_instant_diagnostics,
                        );
                        self.merged_instant_diagnostics
                            .insert(path.clone(), (max_v, merged));

                        self.dirty_diagnostics = true;
                        self.last_change = None;
                    }
                }
                LspEvent::StatusChanged { name, status } => {
                    if *name == TY_SERVER.program {
                        self.ty_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.ty_workspace_diag_dirty = true;
                        } else if *status == LspServerStatus::Starting
                            || *status == LspServerStatus::Crashed
                            || *status == LspServerStatus::Disabled
                        {
                            self.ty_workspace_diag_pending = None;
                        }
                    } else {
                        self.python_status = status.clone();
                    }
                }
                LspEvent::ConfigurationServed { name } => {
                    if *name == TY_SERVER.program {
                        self.ty_workspace_diag_dirty = true;
                    }
                }
                LspEvent::WorkspaceDiagnosticsDone { request_id } => {
                    if self.ty_workspace_diag_pending == Some(*request_id) {
                        self.ty_workspace_diag_pending = None;
                    }
                }
                LspEvent::Log { name, message } => {
                    let (final_text, spans, folds) = format_lsp_log_entry(message);
                    *message = final_text.clone();
                    let logs = self.server_logs.entry(*name).or_insert_with(Vec::new);
                    let now = Instant::now();
                    logs.push(LogEntry {
                        text: final_text,
                        spans,
                        folds,
                        created_at: now,
                    });
                    trim_lsp_logs(logs, now);
                }
                _ => {}
            }
        }

        if let Some(t) = self.last_change {
            if t.elapsed().as_secs_f32() >= 3.0 {
                if self.dirty_diagnostics {
                    for (path, merged) in merged_diagnostics_for_all_paths(
                        &self.instant_diagnostics,
                        &self.ty_instant_diagnostics,
                    ) {
                        self.diagnostics.insert(path, merged);
                    }
                    self.dirty_diagnostics = false;
                }
                self.last_change = None;
            }
        } else {
            if self.dirty_diagnostics {
                for (path, merged) in merged_diagnostics_for_all_paths(
                    &self.instant_diagnostics,
                    &self.ty_instant_diagnostics,
                ) {
                    self.diagnostics.insert(path, merged);
                }
                self.dirty_diagnostics = false;
            }
        }

        self.request_ty_workspace_diagnostics_if_ready();

        all
    }

    pub fn get_diagnostics(&self, path: &PathBuf) -> &[Diagnostic] {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.diagnostics
            .get(&abs_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn clear_diagnostics_for_path(&mut self, path: &PathBuf) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.diagnostics.remove(&abs_path);
        self.instant_diagnostics.remove(&abs_path);
        self.ty_instant_diagnostics.remove(&abs_path);
        self.merged_instant_diagnostics.remove(&abs_path);
        self.ty_diag_result_ids.remove(&abs_path);
        self.dirty_diagnostics = false;
    }

    pub fn get_instant_diagnostics_with_version(&self, path: &PathBuf) -> (i32, &[Diagnostic]) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.merged_instant_diagnostics
            .get(&abs_path)
            .map(|(v, d)| (*v, d.as_slice()))
            .unwrap_or((0, &[]))
    }

    pub fn has_stale_instant_diagnostics(&self, path: &PathBuf, editor_version: u64) -> bool {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let is_stale = |diags: &HashMap<PathBuf, (i32, Vec<Diagnostic>)>| {
            diags
                .get(&abs_path)
                .is_some_and(|(version, _)| (*version as u64) < editor_version)
        };
        is_stale(&self.instant_diagnostics) || is_stale(&self.ty_instant_diagnostics)
    }

    /// Диагностики для текущего файла, отфильтрованные по строке
    pub fn diagnostics_for_line(&self, path: &PathBuf, line: u32) -> Vec<&Diagnostic> {
        self.get_diagnostics(path)
            .iter()
            .filter(move |d| d.start_line == line)
            .collect()
    }

    /// Запрос на глобальный fix-all (source.fixAll) для текущего файла
    pub fn request_fix_all(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let proc = self.process_for_ext(ext)?;
        let id = next_id();
        let uri = path_to_uri(&abs_path.to_string_lossy());
        let _ = proc.cmd_tx.send(Cmd::CodeAction {
            id,
            uri,
            start_line: 0,
            start_col: 0,
            end_line: u32::MAX,
            end_col: 0,
            diagnostics_json: String::from("[]"),
            only: Some(vec!["source.fixAll".to_string()]),
        });
        Some(id)
    }

    pub fn request_organize_imports(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let proc = self.process_for_ext(ext)?;
        let id = next_id();
        let uri = path_to_uri(&abs_path.to_string_lossy());
        let _ = proc.cmd_tx.send(Cmd::CodeAction {
            id,
            uri,
            start_line: 0,
            start_col: 0,
            end_line: u32::MAX,
            end_col: 0,
            diagnostics_json: String::from("[]"),
            only: Some(vec!["source.organizeImports".to_string()]),
        });
        Some(id)
    }

    #[allow(dead_code)]
    pub fn shutdown(mut self) {
        self.python_disabled = true;
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
    }
}

// ── Утилита: конвертация LSP-позиции обратно в байт-offset ───────────────────

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
        color: if prefix.contains("RECV") {
            [0.313, 0.980, 0.482, 1.0]
        } else {
            [0.545, 0.913, 0.992, 1.0]
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
                            let color = match name {
                                "property" => [0.545, 0.913, 0.992, 1.0],
                                "string" => [0.945, 0.980, 0.549, 1.0],
                                "number" => [0.741, 0.576, 0.976, 1.0],
                                "boolean" => [1.0, 0.474, 0.776, 1.0],
                                "keyword.control" => [1.0, 0.474, 0.776, 1.0],
                                "comment" => [0.384, 0.447, 0.643, 1.0],
                                _ => continue,
                            };
                            spans.push(crate::highlighter::ColorSpan {
                                start: cap.node.start_byte() + prefix.len(),
                                end: cap.node.end_byte() + prefix.len(),
                                color,
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
            color: [0.875, 0.882, 0.902, 1.0],
        });
    }

    spans.sort_by_key(|s| s.start);
    (final_string, spans, folds)
}

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

#[cfg(test)]
mod lsp_manager_allocation_tests {
    use super::*;

    #[test]
    fn server_summaries_report_counts_without_cloning_logs() {
        let mut manager = LspManager::new(Vec::new());
        manager.python_status = LspServerStatus::Running;
        manager.server_logs.insert(
            RUFF_SERVER.program,
            vec![LogEntry {
                text: "ruff log".to_string(),
                spans: Vec::new(),
                folds: Vec::new(),
                created_at: Instant::now(),
            }],
        );

        let summaries = manager.server_summaries();

        assert_eq!(summaries[0].name, RUFF_SERVER.program);
        assert_eq!(summaries[0].log_count, 1);
        assert!(std::ptr::eq(summaries[0].status, &manager.python_status));
        assert_eq!(summaries[1].name, TY_SERVER.program);
        assert_eq!(summaries[1].log_count, 0);
        assert!(std::ptr::eq(summaries[1].status, &manager.ty_status));
    }
}

#[cfg(test)]
mod lsp_tests;
