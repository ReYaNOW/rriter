use super::*;

pub(super) fn about_to_wait(app: &mut App, event_loop: &ActiveEventLoop) {
    if app.run_ide_on_startup {
        app.run_ide_on_startup = false;
        app.enter_ide_mode();
        return; // Пропускаем один кадр, чтобы избежать гонок состояний
    }

    let now = Instant::now();
    let raw_dt = (now - app.last_frame).as_secs_f32();
    let dt = raw_dt.min(0.016);
    app.last_frame = now;

    let mut needs_redraw = false;
    let mut hover_wake_at: Option<Instant> = None;
    let mut hover_poll_pending = false;

    if app.current_sticky_lines != app.target_sticky_lines {
        let old_len = app.current_sticky_lines.len();
        let new_len = app.target_sticky_lines.len();

        if new_len > old_len {
            app.sticky_anim_progress = 0.0;
            app.sticky_anim_is_adding = true;
            app.current_sticky_lines = app.target_sticky_lines.clone();
        } else if new_len < old_len {
            if app.sticky_anim_is_adding || app.sticky_anim_progress >= 1.0 {
                app.sticky_anim_progress = 0.0;
                app.sticky_anim_is_adding = false;
            }
        } else {
            app.sticky_anim_progress = 1.0;
            app.current_sticky_lines = app.target_sticky_lines.clone();
        }
        needs_redraw = true;
    }

    if app.sticky_anim_progress < 1.0 {
        app.sticky_anim_progress += dt * 6.0;
        if app.sticky_anim_progress >= 0.99 {
            app.sticky_anim_progress = 1.0;
            if !app.sticky_anim_is_adding {
                app.current_sticky_lines = app.target_sticky_lines.clone();
            }
        }
        needs_redraw = true;
    }

    if app.autocomplete_active && app.autocomplete_scroll.update(dt) {
        needs_redraw = true;
    }

    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(popup) = &mut state.popup {
            if popup.scroll.update(dt) {
                needs_redraw = true;
            }
        }
        if let Some(byte_offset) = state.byte_offset {
            let popup_matches_byte = state
                .popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == byte_offset);
            let pending_popup_matches_byte = state
                .pending_popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == byte_offset);

            if !popup_matches_byte
                && !pending_popup_matches_byte
                && state.request_id.is_none()
                && state.definition_request_id.is_none()
            {
                state.timer += raw_dt;
                if state.timer >= crate::app::mouse::HOVER_REQUEST_DELAY_SEC {
                    state.timer = 0.0;
                    if app.is_ide_mode {
                        if let Some(lsp) = &mut app.lsp {
                            if let Some(path) = app.file_path.clone() {
                                let (line, col) = crate::lsp::offset_to_lsp_pos(
                                    &app.editor.get_full_text(),
                                    byte_offset,
                                    &app.editor.line_offsets,
                                );
                                state.request_id =
                                    lsp.request_hover(&path, &app.file_extension, line, col);
                                println!(
                                    "[HOVER DEBUG] 0.34s expired. Sent hover request. id: {:?}",
                                    state.request_id
                                );
                                if state.request_id.is_some() {
                                    hover_poll_pending = true;
                                }
                            }
                        }
                    }
                } else {
                    hover_wake_at = Some(
                        now + std::time::Duration::from_secs_f32(
                            crate::app::mouse::HOVER_REQUEST_DELAY_SEC - state.timer,
                        ),
                    );
                }
            } else if state.request_id.is_some() || state.definition_request_id.is_some() {
                hover_poll_pending = true;
            }
        } else if state.popup.is_some() || state.pending_popup.is_some() {
            state.timer += raw_dt;
            if state.timer >= 0.25 {
                println!("[HOVER DEBUG] 0.25s hide timer expired. Clearing popup.");
                state.popup = None;
                state.pending_popup = None;
                state.rect = None;
                state.clear_type_popup_transition_markers();
                needs_redraw = true;
            } else {
                hover_wake_at =
                    Some(now + std::time::Duration::from_secs_f32((0.25 - state.timer).max(0.0)));
            }
        }
    });

    if app.show_settings && app.settings_tab == 0 && app.settings_ide_scroll.update(dt) {
        app.window.as_ref().unwrap().request_redraw();
    }
    if app.show_settings && app.settings_tab == 4 && app.settings_scroll.update(dt) {
        needs_redraw = true;
    }

    if app.scroll_y.update(dt) {
        needs_redraw = true;
    }

    if app.scroll_x.update(dt) {
        needs_redraw = true;
    }

    if app.tab_scroll.update(dt) {
        needs_redraw = true;
    }

    if app.ide_panel.tab_drag.is_some() {
        needs_redraw = true;
    }

    if app.poll_file_tree() {
        needs_redraw = true;
    }

    // Watcher сигнализирует об изменениях на диске — обновляем дерево
    {
        let mut fs_changed = false;
        if let Some(rx) = &app.file_tree_notify_rx {
            while rx.try_recv().is_ok() {
                fs_changed = true;
            }
        }
        if fs_changed {
            app.refresh_file_tree();
            app.check_external_changes();
            needs_redraw = true;
        }
    }
    if app.ide_panel.explorer_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.problems_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.lsp_scroll_y.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.lsp_scroll_x.update(dt) {
        needs_redraw = true;
    }
    for scroll in app.ide_panel.lsp_logs_scroll_y.values_mut() {
        if scroll.update(dt) {
            needs_redraw = true;
        }
    }
    for scroll in app.ide_panel.lsp_logs_scroll_x.values_mut() {
        if scroll.update(dt) {
            needs_redraw = true;
        }
    }

    if app.is_ide_mode && app.ide_panel.is_open(crate::app::PanelId::Terminal) {
        let mut closed_terminals = Vec::new();
        for (i, term) in app.ide_panel.terminals.iter().enumerate() {
            if let Ok(mut child) = term.child.lock() {
                if let Ok(Some(_)) = child.try_wait() {
                    closed_terminals.push(i);
                }
            }
        }

        for idx in closed_terminals.into_iter().rev() {
            app.ide_panel.terminals.remove(idx);
            needs_redraw = true;
            if app.ide_panel.terminals.is_empty() {
                app.ide_panel
                    .terminals
                    .push(crate::app::terminal::Terminal::spawn(app.window.clone()));
                app.ide_panel.active_terminal = 0;
            } else if app.ide_panel.active_terminal >= app.ide_panel.terminals.len() {
                app.ide_panel.active_terminal = app.ide_panel.terminals.len().saturating_sub(1);
            }
        }

        if app.ide_panel.terminals.is_empty() {
            app.ide_panel
                .terminals
                .push(crate::app::terminal::Terminal::spawn(app.window.clone()));
            app.ide_panel.active_terminal = 0;
            app.ide_panel.terminal_focused = true;
        }
        let active = app.ide_panel.active_terminal;
        if let Some(t) = app.ide_panel.terminals.get_mut(active) {
            if t.scroll_y.update(dt) {
                needs_redraw = true;
            }
            if t.grid.lock().unwrap().dirty {
                needs_redraw = true;
            }
        }
    }

    if app.autocomplete_active && app.autocomplete_anim_progress < 1.0 {
        app.autocomplete_anim_progress += (1.0 - app.autocomplete_anim_progress) * 20.0 * dt;
        if app.autocomplete_anim_progress > 0.99 {
            app.autocomplete_anim_progress = 1.0;
        }
        needs_redraw = true;
    }

    let s = app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
    let window_height = app.window.as_ref().unwrap().inner_size().height as f32;
    let h = (700.0_f32 * s).min(window_height - 40.0 * s);
    let start_y = window_height + 100.0 * s;
    let open_y = (window_height - h) / 2.0;
    let target_y = if app.show_settings { open_y } else { start_y };

    let diff = target_y - app.settings_y;
    if diff.abs() > 1.5 {
        app.settings_y += diff * 10.0 * dt;
        let total_distance = (start_y - open_y).max(1.0);
        app.settings_anim_progress = ((start_y - app.settings_y) / total_distance).clamp(0.0, 1.0);
        needs_redraw = true;
    } else if !app.show_settings && app.settings_anim_progress > 0.0 {
        app.settings_y = start_y;
        app.settings_anim_progress = 0.0;
        needs_redraw = true;
    }

    let s = app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
    let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
        0.0
    } else {
        38.0 * s
    };
    let target_search_y = if app.show_search {
        tab_bar_h + 10.0 * s
    } else {
        -120.0 * s
    };
    let search_diff = target_search_y - app.search_anim_y;
    if search_diff.abs() > 1.5 {
        let speed = if app.show_search { 20.0 } else { 7.0 };
        app.search_anim_y += search_diff * speed * dt;
        needs_redraw = true;
    }

    if app.is_dragging && !app.ide_panel.is_dragging_terminal && !app.scroll_y.is_dragging {
        if let Some(w) = app.window.as_ref() {
            let wh = w.inner_size().height as f32;
            let ww = w.inner_size().width as f32;
            let my = app.renderer.as_ref().unwrap().last_mouse_y;
            let mx = app.renderer.as_ref().unwrap().last_mouse_x;
            let minimap_w = app.renderer.as_ref().unwrap().minimap_width;
            let padding = app.renderer.as_ref().unwrap().left_padding;

            let mut drag_scroll_delta_y = 0.0;
            if my < 0.0 {
                drag_scroll_delta_y = my;
            } else if my > wh {
                drag_scroll_delta_y = my - wh;
            }

            let mut drag_scroll_delta_x = 0.0;
            let view_right_edge = ww - minimap_w;
            if mx < padding {
                drag_scroll_delta_x = mx - padding;
            } else if mx > view_right_edge {
                drag_scroll_delta_x = mx - view_right_edge;
            }

            if drag_scroll_delta_y != 0.0 || drag_scroll_delta_x != 0.0 {
                if drag_scroll_delta_y != 0.0 {
                    let drag_amount = drag_scroll_delta_y.abs();
                    let speed = (drag_amount.powi(2) * 0.15).clamp(70.0, 4500.0);
                    app.scroll_y.target += drag_scroll_delta_y.signum() * speed * dt;
                }

                if drag_scroll_delta_x != 0.0 {
                    let drag_amount = drag_scroll_delta_x.abs();
                    let speed = (drag_amount.powi(2) * 0.15).clamp(70.0, 4500.0);
                    app.scroll_x.target += drag_scroll_delta_x.signum() * speed * dt;
                }

                let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
                    0.0
                } else {
                    38.0 * app.renderer.as_ref().unwrap().scale_factor
                };
                app.editor.set_cursor_at_pos(
                    mx,
                    my - tab_bar_h + app.scroll_y.current,
                    app.renderer.as_mut().unwrap(),
                    false,
                );
                needs_redraw = true;
            }
        }
    }

    if let Some(w) = app.window.as_ref() {
        let s = app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
        let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };
        let max_scroll_y = app
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&app.editor, w.inner_size().height as f32 - tab_bar_h);
        app.scroll_y.clamp_target(0.0, max_scroll_y);
        app.scroll_y.clamp_current(0.0, max_scroll_y);

        let max_scroll_x = app.renderer.as_ref().unwrap().max_scroll_x;
        app.scroll_x.clamp_target(0.0, max_scroll_x);
        app.scroll_x.clamp_current(0.0, max_scroll_x);
    }

    if let Some(rx) = &app.open_folder_rx {
        if let Ok(result) = rx.try_recv() {
            app.open_folder_rx = None;
            if let Some(path) = result {
                app.ide_workspaces.push(path.clone());
                app.ide_panel.file_tree_expanded.insert(path.clone());
                app.refresh_file_tree();
                app.start_file_watcher();
                let w = app.window.as_ref().unwrap();
                let maximized = w.is_maximized();
                let (width, height) = if maximized {
                    (app.window_width, app.window_height)
                } else {
                    let scale = w.scale_factor();
                    let size = w.inner_size().to_logical::<f64>(scale);
                    (size.width, size.height)
                };
                crate::save_config(&crate::Config {
                    window_width: width,
                    window_height: height,
                    maximized,
                    ide_workspaces: app.ide_workspaces.clone(),
                    ide_ignore_patterns: app.ide_ignore_patterns.clone(),
                    enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                        .load(std::sync::atomic::Ordering::Relaxed),
                });
            }
        }
    }

    if let Some(rx) = &app.open_file_rx {
        if let Ok(result) = rx.try_recv() {
            app.open_file_rx = None;
            if let Some(path) = result {
                app.open_file_in_tab(path, true);
            }
        }
    }

    if let Some(rx) = &app.save_file_rx {
        if let Ok(result) = rx.try_recv() {
            app.save_file_rx = None;
            if let Some(path) = result {
                app.file_path = Some(path.clone());
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                app.base_title = file_name.into_owned();

                if let Some(e) = path.extension() {
                    app.file_extension = e.to_string_lossy().to_string();
                } else {
                    app.file_extension = String::new();
                }

                app.add_recent_file(path);

                if app.save_current_file() {
                    if let Some(w) = app.window.as_ref() {
                        App::update_window_title(w, &app.base_title, app.editor.is_dirty());
                    }
                    app.highlighter.reset(
                        app.editor.version,
                        app.editor.get_full_text(),
                        app.file_extension.clone(),
                    );
                }
            }
        }
    }

    if let Some(last_resize) = app.last_resize_time {
        if now.duration_since(last_resize).as_millis() > 150 {
            app.last_resize_time = None;
            needs_redraw = true;
        } else {
            needs_redraw = true;
        }
    }

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
            crate::lsp::LspEvent::CodeActions {
                request_id,
                actions,
            } => {
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
                        let mut combined = new_items;
                        combined.extend(menu.items.drain(..));
                        menu.items = combined;
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
            crate::lsp::LspEvent::ServerReady => {}
            crate::lsp::LspEvent::StatusChanged { .. } => {}
            crate::lsp::LspEvent::Log { .. } => {} // Fix All ответ
            crate::lsp::LspEvent::HoverResponse { request_id, text } => {
                if let Some(ref t) = text {
                    if crate::render_view::TELEMETRY_ENABLED
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        println!("--- HOVER TEXT ---\n{}\n------------------", t);
                    }
                }
                crate::app::mouse::HOVER_STATE.with(|state| {
                    let mut state = state.borrow_mut();
                    if state.request_id == Some(request_id) {
                        println!("[HOVER DEBUG] Received response for req id: {}. Has text: {}", request_id, text.is_some());
                        state.request_id = None;
                        let Some(t) = text else {
                            println!("[HOVER DEBUG] Text is empty. Clearing popup.");
                            state.popup = None;
                            state.pending_popup = None;
                            state.rect = None;
                            if let Some(w) = app.window.as_ref() {
                                w.request_redraw();
                            }
                            return;
                        };
                        if let Some(bo) = state.byte_offset {
                            let (clean_msg, spans, line_kinds, inline_code_ranges) =
                                crate::lsp::highlight_hover_text(&t);
                            let hovered_symbol = symbol_at_offset(&app.editor, bo);
                            if clean_msg.trim() == "None"
                                && hovered_symbol.as_deref() == Some("await")
                            {
                                state.popup = None;
                                state.pending_popup = None;
                                state.rect = None;
                                if let Some(w) = app.window.as_ref() {
                                    w.request_redraw();
                                }
                                return;
                            }
                            let is_simple_type = should_replace_simple_type_hover(&clean_msg);
                            let (clean_msg, spans, line_kinds, inline_code_ranges) =
                                if should_replace_hover_with_source_signature(&clean_msg) {
                                    let lsp_ty = if is_simple_type {
                                        Some(clean_msg.as_str())
                                    } else {
                                        None
                                    };
                                    let current_mod = app.file_path.as_ref().and_then(|p| {
                                        module_path_from_definition_path(p, &app.ide_workspaces)
                                    });
                                    if let Some(sig) = source_signature_for_hover(
                                        &app.editor,
                                        bo,
                                        !is_simple_type,
                                        lsp_ty,
                                        current_mod.as_deref(),
                                    ) {
                                        crate::lsp::highlight_hover_text(&sig)
                                    } else {
                                        (clean_msg, spans, line_kinds, inline_code_ranges)
                                    }
                                } else {
                                    (clean_msg, spans, line_kinds, inline_code_ranges)
                                };
                            let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
                                0.0
                            } else {
                                38.0 * app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0)
                            };
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

                            let popup = crate::app::mouse::HoverPopup {
                                text: clean_msg,
                                spans,
                                line_kinds,
                                inline_code_ranges,
                                byte_offset: bo,
                                anchor_x,
                                anchor_y,
                                offset_x: None,
                                offset_y: None,
                                scroll: crate::scroll::ScrollState::new(15.0),
                                layout_cache: None,
                            };
                            state.pending_popup = None;
                            state.definition_request_id = None;
                            state.hide_diagnostic_popup_until_ready();
                            if let Some(path) = app.file_path.clone() {
                                let (line, col) = crate::lsp::offset_to_lsp_pos(
                                    &app.editor.get_full_text(),
                                    bo,
                                    &app.editor.line_offsets,
                                );
                                state.definition_request_id = app.lsp.as_mut().and_then(|lsp| {
                                    lsp.request_definition(&path, &app.file_extension, line, col)
                                });
                            } else {
                                state.definition_request_id = None;
                            }
                            if state.definition_request_id.is_some() {
                                println!("[HOVER DEBUG] Hover processed, waiting for definition req id: {:?}", state.definition_request_id);
                                state.pending_popup = Some(popup);
                            } else {
                                println!("[HOVER DEBUG] Hover processed, showing popup instantly.");
                                state.finish_stale_combined_transition();
                                state.popup = Some(popup);
                            }
                            state.selection_anchor = None;
                            state.selection_cursor = None;
                            state.selecting = false;
                            if let Some(w) = app.window.as_ref() {
                                w.request_redraw();
                            }
                        }
                    }
                });
            }
            crate::lsp::LspEvent::DefinitionResponse { request_id, path } => {
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
        }
    }

    if app.is_ide_mode {
        if let Some(lsp) = &mut app.lsp {
            // Умная синхронизация без аллокаций каждый кадр.
            // Обновляем UI только если статус или логи реально изменились.
            let lsp_logs_len = lsp.server_logs.get("ruff").map(|l| l.len()).unwrap_or(0);
            let needs_update = app.ide_panel.lsp_servers.is_empty()
                || app.ide_panel.lsp_servers[0].status != lsp.python_status
                || app.ide_panel.lsp_servers[0].logs.len() != lsp_logs_len;

            if needs_update {
                app.ide_panel.lsp_servers = lsp.servers_info();
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
                        let mut byte_offset = 0;
                        for log in &info.logs {
                            for &(s, e) in &log.folds {
                                entry.foldable_ranges_bytes.push((
                                    byte_offset + s,
                                    byte_offset + e,
                                    false,
                                ));
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
                                entry.folded_lines.insert(sl);
                                entry.folded_start_bytes.insert(entry.line_offsets[sl]);
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

    if app.highlighter.poll(app.editor.version) {
        app.apply_highlight_results();
        if app.autocomplete_active {
            app.update_autocomplete();
        }
        needs_redraw = true;
    }

    if app.is_focused {
        let blink_state = (now.duration_since(app.last_action).as_millis() / 500) % 2 == 0;
        if blink_state != app.last_blink_state {
            app.last_blink_state = blink_state;
            needs_redraw = true;
        }
    }

    let is_highlighting = !app.is_highlighted_once;
    let hover_poll_wake_at = if hover_poll_pending {
        Some(now + std::time::Duration::from_millis(16))
    } else {
        None
    };

    if needs_redraw || (app.show_welcome && app.is_ide_mode) {
        if let Some(w) = app.window.as_ref() {
            w.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    } else if is_highlighting {
        let mut wake_at = now + std::time::Duration::from_millis(5);
        if let Some(t) = hover_wake_at {
            if t < wake_at {
                wake_at = t;
            }
        }
        if let Some(t) = hover_poll_wake_at {
            if t < wake_at {
                wake_at = t;
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake_at));
    } else {
        let next_blink = app.last_action
            + std::time::Duration::from_millis(
                (now.duration_since(app.last_action).as_millis() / 500 + 1) as u64 * 500,
            );
        let mut wake_at = next_blink;
        if let Some(t) = hover_wake_at {
            if t < wake_at {
                wake_at = t;
            }
        }
        if let Some(t) = hover_poll_wake_at {
            if t < wake_at {
                wake_at = t;
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake_at));
    }
}
