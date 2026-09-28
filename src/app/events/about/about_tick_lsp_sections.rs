// Per-frame `about_to_wait` sections, included into `events/about.rs`.
// Each section runs in the same order as before and reports whether it needs a redraw.

/// Drains LSP events once per frame; heavy responses go to dedicated handlers.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_lsp_events(app: &mut App) {
    // LSP: опрашиваем события (диагностика, code actions) — раз в кадр, не блокирует
    let mut lsp_events = Vec::new();
    if app.is_ide_mode {
        if let Some(lsp) = &mut app.lsp {
            lsp_events = lsp.poll();
        }
    }

    for event in lsp_events {
        match event {
            crate::lsp::LspEvent::Diagnostics { .. } => {
                // lsp.diagnostics обновлены внутри poll() автоматически
                if let Some(w) = app.window.as_ref() {
                    w.request_redraw();
                }
            }
            crate::lsp::LspEvent::ClosingLabels {
                server: crate::lsp::LspServerKind::Dart,
                path,
                labels,
            } => {
                if app.apply_server_closing_labels(&path, &labels)
                    && let Some(w) = app.window.as_ref()
                {
                    w.request_redraw();
                }
            }
            crate::lsp::LspEvent::CodeActions {
                request_id,
                actions,
            } => {
                about_to_wait_lsp_code_actions(app, request_id, actions);
            }
            crate::lsp::LspEvent::CompletionResponse {
                request_id,
                items,
                is_incomplete,
            } => {
                if app.autocomplete_response_matches_current(request_id) {
                    let mode = app.autocomplete_pending_request_mode;
                    app.autocomplete_pending_request_id = None;
                    if mode == Some(crate::app::AutocompleteMode::LspContext) {
                        app.remember_lsp_autocomplete_cache(items.clone(), is_incomplete);
                        app.update_lsp_autocomplete(items);
                    } else {
                        app.remember_ty_autocomplete_cache(items.clone());
                        app.update_ty_autocomplete(items);
                    }
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                } else if app.autocomplete_pending_request_id == Some(request_id) {
                    app.autocomplete_pending_request_id = None;
                    app.autocomplete_pending_request_mode = None;
                    app.autocomplete_pending_request_path = None;
                    app.autocomplete_pending_context_key = None;
                } else if app.autocomplete_detail_request_id == Some(request_id) {
                    if app.autocomplete_active {
                        app.remember_autocomplete_detail_cache(&items);
                        app.merge_autocomplete_details(items);
                    } else {
                        app.remember_autocomplete_detail_cache(&items);
                        app.finish_autocomplete_detail_request();
                    }
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }
            crate::lsp::LspEvent::SignatureHelpResponse { request_id, help } => {
                if app.autocomplete_signature_request_id == Some(request_id) {
                    app.autocomplete_signature_request_id = None;
                    if app.autocomplete_mode == crate::app::AutocompleteMode::LspContext {
                        app.update_lsp_signature_help_autocomplete(help);
                    } else {
                        let parameters = help
                            .signatures
                            .get(help.active_signature)
                            .or_else(|| help.signatures.first())
                            .map(|signature| {
                                signature
                                    .parameters
                                    .iter()
                                    .map(|parameter| parameter.label.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        app.update_ty_signature_help_autocomplete(parameters);
                    }
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }
            crate::lsp::LspEvent::InlayHintsResponse { request_id, hints } => {
                if app.python_inlay_hint_pending_request_id == Some(request_id) {
                    app.python_inlay_hint_pending_request_id = None;
                    let Some(path) = app.python_inlay_hint_pending_path.take() else {
                        app.python_inlay_hint_pending_range = None;
                        continue;
                    };
                    let Some(range) = app.python_inlay_hint_pending_range.take() else {
                        continue;
                    };
                    let version = app.python_inlay_hint_pending_version;
                    if app.file_path.as_ref() == Some(&path) && app.editor.version == version {
                        let text = app.editor.get_full_text();
                        let parsed = crate::app::lsp_inlay_hints_from_lsp_with_offsets(
                            &app.file_extension,
                            &text,
                            &app.editor.line_offsets,
                            &hints,
                        );
                        app.python_inlay_hint_cache.insert(
                            (path.clone(), app.file_extension.clone()),
                            (version, range, parsed.clone()),
                        );
                        app.python_inlay_hints = parsed;
                        app.python_inlay_hint_path = Some(path);
                        app.python_inlay_hint_range = Some(range);
                        app.python_inlay_hint_version = version;
                        if let Some(w) = app.window.as_ref() {
                            w.request_redraw();
                        }
                    }
                }
            }
            crate::lsp::LspEvent::ServerReady { .. } => {}
            crate::lsp::LspEvent::StatusChanged { .. } => {}
            crate::lsp::LspEvent::ConfigurationServed { .. } => {}
            crate::lsp::LspEvent::ClosingLabels { .. } => {}
            crate::lsp::LspEvent::WorkspaceDiagnosticsDone { .. } => {}
            crate::lsp::LspEvent::ReferencesResponse { .. } => {}
            crate::lsp::LspEvent::PrepareRenameResponse { .. } => {}
            crate::lsp::LspEvent::RenameResponse { .. } => {}
            crate::lsp::LspEvent::FormattingResponse { .. } => {}
            crate::lsp::LspEvent::Log { .. } => {} // Fix All ответ
            crate::lsp::LspEvent::HoverResponse { request_id, text } => {
                about_to_wait_lsp_hover_response(app, request_id, text);
            }
            crate::lsp::LspEvent::DefinitionResponse { request_id, target } => {
                about_to_wait_lsp_definition_response(app, request_id, target);
            }
        }
    }
}

/// Code actions response: Alt+Enter menu items or the LSP panel Fix All merge.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_lsp_code_actions(
    app: &mut App,
    request_id: i32,
    actions: Vec<crate::lsp::CodeAction>,
) {
    // Проверяем: это ответ на Alt+Enter меню?
    let is_for_menu = app
        .lsp_actions_menu
        .as_ref()
        .and_then(|m| m.pending_request_id)
        .map(|id| id == request_id)
        .unwrap_or(false);

    if is_for_menu {
        if let Some(menu) = &mut app.lsp_actions_menu {
            let new_items: Vec<crate::app::LspActionItem> = actions
                .into_iter()
                .filter(|a| {
                    a.edit.is_some()
                        && !a.title.to_lowercase().contains("fix all")
                        && !a.title.to_lowercase().contains("organize imports")
                })
                .map(crate::app::LspActionItem::CodeAction)
                .collect();
            let selected = lsp_action_selection_after_prepend(
                menu.items.len(),
                menu.selected,
                new_items.len(),
            );
            let mut combined = new_items;
            combined.extend(menu.items.drain(..));
            menu.items = combined;
            menu.selected = selected.min(menu.items.len().saturating_sub(1));
            menu.pending_request_id = None;
        }
        if let Some(w) = app.window.as_ref() {
            w.request_redraw();
        }
    } else if app.pending_fix_all_id == Some(request_id) {
        // Fix All из панели LSP серверов
        app.pending_fix_all_id = None;
        let mut merged_edit = crate::lsp::WorkspaceEdit::default();
        for action in actions {
            if let Some(edit) = action.edit {
                for (path, changes) in edit.changes {
                    merged_edit.changes.entry(path).or_default().extend(changes);
                }
            }
        }
        if !merged_edit.changes.is_empty() {
            app.apply_workspace_edit(&merged_edit, true);
        }
        if let Some(w) = app.window.as_ref() {
            w.request_redraw();
        }
    }
}

/// Hover response: API mock hover first, then the source hover popup.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_lsp_hover_response(app: &mut App, request_id: i32, text: Option<String>) {
    if let Some(ref t) = text {
        if crate::render_view::hover_trace_enabled() {
            println!("--- HOVER TEXT ---\n{}\n------------------", t);
        }
    }
    if app.apply_api_mock_hover_response(request_id, text.clone()) {
        return;
    }
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.request_id == Some(request_id) {
            if crate::render_view::hover_trace_enabled() {
                println!(
                    "[HOVER DEBUG] Received response for req id: {}. Has text: {}",
                    request_id,
                    text.is_some()
                );
            }
            if let Some(bo) = state.byte_offset {
                let current_mod = app.file_path.as_ref().and_then(|p| {
                    module_path_from_definition_path(p, &app.ide_workspaces)
                });
                let tab_bar_h = app.editor_top_inset(
                    app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0),
                );
                let render_scroll_y = app.scroll_y.current.round() - tab_bar_h;
                let (anchor_x, anchor_y) = if let Some(renderer) = app.renderer.as_mut()
                {
                    crate::app::mouse::hover_anchor_for_byte(
                        renderer,
                        &app.editor,
                        bo,
                        render_scroll_y,
                    )
                } else {
                    (0.0, 0.0)
                };

                let definition_request = || {
                    let path = app.file_path.clone()?;
                    let (line, col) = crate::lsp::offset_to_lsp_pos(
                        &app.editor.get_full_text(),
                        bo,
                        &app.editor.line_offsets,
                    );
                    app.lsp.as_mut().and_then(|lsp| {
                        lsp.request_definition(&path, &app.file_extension, line, col)
                    })
                };
                if apply_source_hover_response_to_state(
                    &mut state,
                    request_id,
                    &app.editor,
                    bo,
                    bo,
                    text,
                    current_mod.as_deref(),
                    (anchor_x, anchor_y),
                    definition_request,
                ) {
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }
        }
    });
}

/// Definition response: Ctrl+click target or hover popup enrichment.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_lsp_definition_response(
    app: &mut App,
    request_id: i32,
    target: Option<crate::lsp::DefinitionTarget>,
) {
    let path = target.as_ref().map(|target| target.path.clone());
    if app.ctrl_definition.request_id == Some(request_id) {
        app.ctrl_definition.request_id = None;
        app.ctrl_definition.target =
            app.ctrl_definition_target_from_lsp(target.map(|target| {
                crate::app::DefinitionJumpTarget {
                    path: target.path,
                    line: target.line,
                    col: target.col,
                }
            }));
        if let Some(w) = app.window.as_ref() {
            w.request_redraw();
        }
        return;
    }
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.definition_request_id == Some(request_id) {
            state.definition_request_id = None;
            let mut popup = state.pending_popup.take();
            if popup.is_none() {
                popup = state.popup.take();
            }
            if let Some(path) = path {
                if let Some(module_path) =
                    module_path_from_definition_path(&path, &app.ide_workspaces)
                {
                    if let Some(popup) = &mut popup {
                        let mut text_changed_by_class = false;
                        if let Some(symbol) =
                            symbol_at_offset(&app.editor, popup.byte_offset)
                        {
                            if let Some(class_sig) =
                                source_class_signature_from_definition_file(
                                    &path, &symbol,
                                )
                            {
                                let mut new_text = popup.text.clone();
                                let class_prefix = format!("class {}", symbol);

                                if new_text.starts_with(&class_prefix) {
                                    new_text =
                                        new_text.replacen(&class_prefix, &class_sig, 1);
                                } else if new_text
                                    .contains(&format!("\n{class_prefix}"))
                                {
                                    new_text =
                                        new_text.replace(&class_prefix, &class_sig);
                                } else if new_text == symbol {
                                    new_text = class_sig.clone();
                                } else if new_text.starts_with(&format!("{symbol}\n")) {
                                    new_text =
                                        new_text.replacen(&symbol, &class_sig, 1);
                                }

                                if new_text != popup.text {
                                    let (clean, spans, kinds, inline) =
                                        crate::lsp::highlight_hover_text(&new_text);
                                    popup.text = clean;
                                    popup.spans = spans;
                                    popup.line_kinds = kinds;
                                    popup.inline_code_ranges = inline;
                                    text_changed_by_class = true;
                                }
                            }
                        }

                        if !text_changed_by_class
                            && should_replace_simple_type_hover(&popup.text)
                        {
                            if let Some(symbol) =
                                symbol_at_offset(&app.editor, popup.byte_offset)
                            {
                                if let Some(attr_hover) =
                                    source_attribute_hover_from_definition_file(
                                        &path,
                                        &symbol,
                                        &module_path,
                                        Some(&popup.text),
                                    )
                                {
                                    let (clean, spans, kinds, inline) =
                                        crate::lsp::highlight_hover_text(&attr_hover);
                                    popup.text = clean;
                                    popup.spans = spans;
                                    popup.line_kinds = kinds;
                                    popup.inline_code_ranges = inline;
                                }
                            }
                        }

                        if popup.text.starts_with("class ")
                            && !popup.text.starts_with(&module_path)
                            && !popup.text.starts_with(HOVER_MODULE_PREFIX)
                        {
                            prepend_hover_module_path(popup, &module_path);
                        }
                    }
                }
            }
            if let Some(popup) = popup {
                state.finish_stale_combined_transition();
                state.popup = Some(popup);
                if let Some(w) = app.window.as_ref() {
                    w.request_redraw();
                }
            }
        }
    });
}

/// Syncs LSP server status and log editors into the IDE panel.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_lsp_server_logs(app: &mut App) {
    if app.is_ide_mode {
        if let Some(lsp) = &mut app.lsp {
            // Умная синхронизация без аллокаций каждый кадр.
            // Обновляем UI только если статус или логи реально изменились.
            let filter = app.ide_panel.current_lsp_log_filter();
            let needs_update = {
                let summaries = lsp.server_summaries();
                app.ide_panel.lsp_log_filter_dirty
                    || app.ide_panel.lsp_log_filter_applied.as_ref() != Some(&filter)
                    || app.ide_panel.lsp_servers.len() != summaries.len()
                    || summaries.iter().any(|info| {
                        app.ide_panel
                            .lsp_servers
                            .iter()
                            .find(|ui| ui.name == info.name)
                            .is_none_or(|ui| &ui.status != info.status)
                            || app
                                .ide_panel
                                .lsp_log_source_counts
                                .get(info.name)
                                .copied()
                                .unwrap_or(0)
                                != info.log_count
                    })
            };

            if needs_update {
                let raw_servers = lsp.servers_info();
                app.ide_panel.lsp_log_source_counts.clear();
                let mut ui_servers = raw_servers;
                for info in &mut ui_servers {
                    app.ide_panel
                        .lsp_log_source_counts
                        .insert(info.name.to_string(), info.logs.len());
                    info.logs.retain(|log| filter.matches(log));
                }
                app.ide_panel.lsp_servers = ui_servers;
                app.ide_panel.lsp_log_filter_applied = Some(filter);
                app.ide_panel.lsp_log_filter_dirty = false;
                // Синхронизируем Editor для логов (для выделения и копирования)
                for info in &app.ide_panel.lsp_servers {
                    let new_text = info
                        .logs
                        .iter()
                        .map(|l| l.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let focused = app.ide_panel.lsp_logs_focused.as_deref() == Some(info.name);
                    let entry = app
                        .ide_panel
                        .lsp_log_editors
                        .entry(info.name.to_string())
                        .or_insert_with(|| crate::editor::Editor::new(new_text.len().max(512)));
                    // Пересоздаём только если текст изменился (иначе сбросим выделение)
                    if entry.get_full_text() != new_text {
                        let saved_cursor = if focused { Some(entry.cursor) } else { None };
                        let saved_anchor = if focused {
                            entry.selection_anchor
                        } else {
                            None
                        };
                        *entry = crate::editor::Editor::new(new_text.len().max(512));
                        let _ = entry.insert_str(&new_text);

                        entry.foldable_ranges_bytes.clear();
                        let mut autofold_starts = Vec::new();
                        let mut byte_offset = 0;
                        for log in &info.logs {
                            for &(s, e, depth) in &log.folds {
                                let start = byte_offset + s;
                                entry
                                    .foldable_ranges_bytes
                                    .push((start, byte_offset + e, false));
                                if depth == 2 {
                                    autofold_starts.push(start);
                                }
                            }
                            byte_offset += log.text.len() + 1;
                        }
                        entry.rebuild_line_offsets();

                        for &(s, e, _) in &entry.foldable_ranges_bytes {
                            let sl = entry
                                .line_offsets
                                .partition_point(|&x| x <= s)
                                .saturating_sub(1);
                            let el = entry
                                .line_offsets
                                .partition_point(|&x| x <= e)
                                .saturating_sub(1);
                            if el > sl {
                                entry.foldable_lines.insert(sl, el);
                                if autofold_starts.contains(&s) {
                                    entry.folded_lines.insert(sl);
                                    entry.folded_start_bytes.insert(entry.line_offsets[sl]);
                                }
                            }
                        }

                        if let Some(c) = saved_cursor {
                            entry.cursor = c.min(new_text.len());
                            entry.selection_anchor = saved_anchor.map(|a| a.min(new_text.len()));
                        } else {
                            // По умолчанию курсор в конце (хвост лога)
                            entry.cursor = new_text.len();
                            entry.selection_anchor = None;
                        }
                    }
                }
                if let Some(w) = app.window.as_ref() {
                    w.request_redraw();
                }
            }
        }
    }
}
