use super::*;

pub(crate) struct FrameOutcome {
    autocomplete_frame_start: Option<Instant>,
    autocomplete_prev_frame: Option<Instant>,
    autocomplete_swap_start: Option<Instant>,
    autocomplete_frame_metrics: Option<(usize, usize, f32, usize, usize, usize, usize)>,
}

impl App {
    /// Body of `WindowEvent::Resized` minus the native GL surface resize, which the window
    /// branch does first; headless `resize` resizes its pbuffer before calling this.
    pub(crate) fn handle_main_resized(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if self.markdown_mode() == crate::app::MarkdownMode::Read {
            self.finish_markdown_read_selection_gesture();
            self.scroll_y.end_drag();
        }
        if size.width == 0 || size.height == 0 {
            self.render_suspended = true;
            self.last_frame = Instant::now();
        } else {
            self.render_suspended = false;
            self.renderer
                .as_mut()
                .unwrap()
                .resize(size.width, size.height);
            self.renderer
                .as_mut()
                .unwrap()
                .last_editor_version_for_scroll_x = u64::MAX;
            self.last_resize_time = Some(Instant::now());
            self.window.as_ref().unwrap().request_redraw();
        }
    }

    /// Body of `WindowEvent::ScaleFactorChanged` minus the native GL surface resize.
    pub(crate) fn handle_main_scale_factor_changed(&mut self, scale_factor: f64) {
        if self.markdown_mode() == crate::app::MarkdownMode::Read {
            self.finish_markdown_read_selection_gesture();
            self.scroll_y.end_drag();
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_scale_factor(scale_factor as f32);
            renderer.last_editor_version_for_scroll_x = u64::MAX;
        }
        if let Some(window) = self.window.as_ref() {
            let size = window.inner_size();
            if size.width > 0 && size.height > 0 {
                self.renderer
                    .as_mut()
                    .unwrap()
                    .resize(size.width, size.height);
            }
            window.request_redraw();
        }
    }

    pub(crate) fn render_main_frame(&mut self) -> FrameOutcome {
                // The first content frame after `enter_ide_mode_deferred` is being drawn; a
                // frame with the editor area held back is not it (the code is).
                if self.ide_deferred == crate::app::IdeDeferred::AwaitFrame
                    && self.startup_editor_pending.is_none()
                {
                    self.ide_deferred = crate::app::IdeDeferred::Ready;
                }
                // `--ide` startup: tab bar and editor area stay blank until the first highlight.
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.startup_editor_hidden = self.startup_editor_pending.is_some();
                }
                let (autocomplete_frame_start, autocomplete_prev_frame) =
                    autocomplete_frame_start(self.autocomplete_active);

                let blink_alpha = if !self.is_focused || self.modal_dialog_open() {
                    1.0
                } else if self.last_blink_state {
                    1.0
                } else {
                    0.0
                };

                let is_resizing = self.last_resize_time.is_some();

                // Очищаем UI registry перед новым кадром```
                self.ui_registry.clear();
                // Apply API mock hover releases queued by `ApiClientState` before the
                // frame reads `self.hover`.
                self.release_api_mock_hover();
                // Database query consoles keep their own hover context; switching it
                // resets the hover (`HoverState::set_database_query_hover_context`).
                let query_hover_context =
                    self.tabs.get(self.active_tab).and_then(|tab| match &tab.kind {
                        crate::app::EditorTabKind::DatabaseQuery(meta, _) => {
                            Some(meta.console_id.0)
                        }
                        _ => None,
                    });
                self.hover.set_database_query_hover_context(query_hover_context);

                let query_problem = self
                    .tabs
                    .get(self.active_tab)
                    .and_then(|tab| match &tab.kind {
                        crate::app::EditorTabKind::DatabaseQuery(meta, state) => Some((
                            meta.database_name.as_str(),
                            state.editor_diagnostics.as_slice(),
                        )),
                        _ => None,
                    });
                if self.ide_panel.refresh_flat_diagnostics_if_needed(
                    self.active_tab,
                    self.file_path.as_deref(),
                    query_problem,
                    self.lsp.as_ref(),
                ) {
                    crate::app::mouse::clamp_problems_scroll(self);
                }

                if let Some(log) = &mut self.pending_key_log
                    && log.t_render.is_none()
                {
                    log.t_render = Some(std::time::Instant::now());
                }

                if let Some(mut renderer) = self.renderer.take() {
                    self.pdf_prepare_frame(&mut renderer);
                    self.renderer = Some(renderer);
                }
                self.markdown_media_prepare_frame();
                let ctrl_definition_range = self.ctrl_definition_highlight_range();
                let python_inlay_hints = if self.python_inlay_hint_path.as_ref()
                    == self.file_path.as_ref()
                    && self.python_inlay_hint_version == self.editor.version
                {
                    self.python_inlay_hints.as_slice()
                } else {
                    &[]
                };
                let closing_hints = if self.file_extension == "dart"
                    && self.closing_hint_state.revision() == self.editor.version
                {
                    self.closing_hint_state.hints()
                } else {
                    &[]
                };
                let (mut wants_pointer, target_sticky) = self.renderer.as_mut().unwrap().draw(
                    &mut self.editor,
                    &self.base_title,
                    self.file_path.as_ref(),
                    &self.tabs,
                    self.active_tab,
                    self.scroll_x.current,
                    &mut self.scroll_y,
                    &mut self.markdown,
                    blink_alpha,
                    self.show_fps,
                    &self.highlighter.spans,
                    // `modal_dialog_open()` on the field: `self.renderer` is borrowed here.
                    self.confirm_dialog.is_open(),
                    is_resizing,
                    &self.search_results,
                    self.search_current_idx,
                    self.show_search,
                    self.search_anim_y,
                    &self.search_editor,
                    self.search_focused,
                    self.search_case_sensitive,
                    self.show_welcome,
                    &self.recent_files,
                    &self.current_sticky_lines,
                    self.sticky_anim_progress,
                    self.sticky_anim_is_adding,
                    self.is_ide_mode,
                    &self.ide_panel,
                    self.show_settings,
                    self.lsp.as_ref(),
                    &mut self.ui_registry,
                    &mut self.hover,
                    self.tab_scroll.current.round(),
                    &self.highlighter.syntax_errors,
                    ctrl_definition_range,
                    python_inlay_hints,
                    closing_hints,
                    &self.ide_workspaces,
                    self.readonly_notice_until
                        .is_some_and(|until| std::time::Instant::now() < until),
                    self.readonly_notice_text,
                    self.inline_git_popup.as_ref(),
                    &self.pdf_engine,
                    self.pdf_dark_pages,
                    &self.markdown_media,
                    &self.empty_ide_open_label,
                );

                self.target_sticky_lines = target_sticky;
                if self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.editor_hover_blocked)
                {
                    crate::app::mouse::clear_hover_popup(&mut self.hover);
                }

                // Продолжаем рендерить пока tooltip ещё не показан
                let diag_timer_active =
                    self.hover.diag_hover_timer > 0.0 && self.hover.diag_hover_timer < 0.2;
                let git_tooltip_waiting = self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.git_tooltip_waiting);
                let link_tooltip_waiting = self.markdown_mode() == crate::app::MarkdownMode::Read
                    && self
                        .renderer
                        .as_ref()
                        .is_some_and(|renderer| renderer.markdown_link_tooltip_waiting);
                if diag_timer_active || git_tooltip_waiting || link_tooltip_waiting {
                    self.window.as_ref().unwrap().request_redraw();
                }

                // Сбрасываем иконку копирования когда popup диагностики закрывается
                if self.hover.hovered_diags.is_empty() {
                    self.ide_panel.diag_copied_idx = None;
                }

                let (mx, my, s, minimap_w) = {
                    let r = self.renderer.as_ref().unwrap();
                    (
                        r.last_mouse_x,
                        r.last_mouse_y,
                        r.scale_factor,
                        r.minimap_width,
                    )
                };

                let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
                let window_height = self.window.as_ref().unwrap().inner_size().height as f32;
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_max_scroll(&self.editor, window_height);
                let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };
                let scrollbar_x =
                    self.search_panel_scrollbar_x_for_mode(window_width, minimap_w, scrollbar_w, s);

                let mut over_search = false;
                if self.show_search && self.search_anim_y > -10.0 {
                    let geometry =
                        crate::render_view::search::search_panel_geometry(scrollbar_x, s);
                    let search_h = crate::render_view::search::SEARCH_PANEL_H * s;
                    if mx >= geometry.x
                        && mx <= geometry.x + geometry.w
                        && my >= self.search_anim_y
                        && my <= self.search_anim_y + search_h
                    {
                        over_search = true;
                    }
                }

                if !over_search {
                    let r = self.renderer.as_ref().unwrap();
                    for &(rx, ry, rw, rh, _) in &r.sticky_scroll_rects {
                        if mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh {
                            wants_pointer = true;
                            break;
                        }
                    }
                }

                // LSP actions menu — рисуем поверх всего
                if let Some(menu) = self.lsp_actions_menu.clone() {
                    let wants = self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .draw_lsp_actions_menu(&menu, blink_alpha);
                    if wants {
                        wants_pointer = true;
                    }
                }

                if self.autocomplete_active {
                    let perf_enabled = autocomplete_log_enabled();
                    let perf_total_start = perf_enabled.then(Instant::now);
                    let mut perf_list_ms = 0.0;
                    let mut perf_refresh_ms = 0.0;
                    let mut perf_layout_ms = 0.0;
                    let mut perf_detail_draw_ms = 0.0;
                    let mut perf_detail_len = 0usize;
                    let mut perf_detail_lines = 0usize;
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        self.active_tab_is_database_query(),
                        s,
                    );
                    let render_scroll_y = self.scroll_y.current.round() - tab_bar_h;
                    let (cx, cy) = self.renderer.as_mut().unwrap().get_cursor_xy(&self.editor);
                    let (anchor_x, anchor_y) = *self
                        .autocomplete_anchor
                        .get_or_insert((cx, cy - render_scroll_y));
                    let perf_list_start = perf_enabled.then(Instant::now);
                    let rect = self.renderer.as_mut().unwrap().draw_autocomplete(
                        anchor_x,
                        anchor_y,
                        &self.autocomplete_options,
                        self.autocomplete_mode,
                        self.autocomplete_selected_idx,
                        self.autocomplete_anim_progress,
                        self.autocomplete_scroll.current,
                        self.autocomplete_hovered_idx,
                        self.autocomplete_min_width,
                    );
                    if let Some(start) = perf_list_start {
                        perf_list_ms = start.elapsed().as_secs_f64() * 1000.0;
                    }
                    self.autocomplete_min_width = self.autocomplete_min_width.max(rect.2);
                    self.autocomplete_rect = Some(rect);
                    if rect.2 > 0.0
                        && rect.3 > 0.0
                        && mx >= rect.0
                        && mx <= rect.0 + rect.2
                        && my >= rect.1
                        && my <= rect.1 + rect.3
                    {
                        crate::app::mouse::clear_hover_popup(&mut self.hover);
                    }
                    let perf_refresh_start = perf_enabled.then(Instant::now);
                    if self.autocomplete_detail_popup.is_none()
                        && self
                            .autocomplete_options
                            .get(self.autocomplete_selected_idx)
                            .and_then(|(item, _)| item.detail.as_deref())
                            .is_some_and(|detail| !detail.trim().is_empty())
                    {
                        self.refresh_autocomplete_detail_popup();
                    }
                    if let Some(start) = perf_refresh_start {
                        perf_refresh_ms = start.elapsed().as_secs_f64() * 1000.0;
                    }
                    let detail_anim_progress = self.autocomplete_anim_progress.clamp(0.0, 1.0);
                    let detail_opacity_p = ((detail_anim_progress - 0.55) / 0.30).clamp(0.0, 1.0);
                    let detail_opacity =
                        detail_opacity_p * detail_opacity_p * (3.0 - 2.0 * detail_opacity_p);
                    if detail_anim_progress > 0.0 && rect.3 > 0.0 {
                        if let Some(mut popup) = self.autocomplete_detail_popup.take() {
                            if perf_enabled {
                                perf_detail_len = popup.text.len();
                                perf_detail_lines = popup.text.lines().count();
                            }
                            let (rx, ry, rw, rh) = rect;
                            let perf_layout_start = perf_enabled.then(Instant::now);
                            let (natural_w, natural_h, max_h) = {
                                let r = self.renderer.as_mut().unwrap();
                                let pad = 12.0 * r.scale_factor;
                                let line_h = 22.0 * r.scale_factor;
                                let gap = 16.0 * r.scale_factor;
                                let margin = 4.0 * r.scale_factor;
                                let min_h = line_h + pad * 2.0;
                                let available_below = r.height - (ry + rh + gap) - margin;
                                let detail_cap_h = line_h * 6.0 + pad * 2.0;
                                let max_h = (r.height * 0.28)
                                    .min(detail_cap_h)
                                    .min(available_below)
                                    .max(min_h);
                                let max_text_w = (r.width - 80.0 * r.scale_factor)
                                    .min(820.0 * r.scale_factor)
                                    .max(320.0 * r.scale_factor);
                                let cache_valid =
                                    popup.layout_cache.as_ref().is_some_and(|cache| {
                                        cache.scale_factor == r.scale_factor
                                            && cache.max_text_w == max_text_w
                                            && cache.span_count == popup.spans.len()
                                            && cache.text_len == popup.text.len()
                                    });
                                if !cache_valid {
                                    popup.layout_cache = Some(
                                        r.build_hover_popup_layout(&popup, max_text_w, line_h),
                                    );
                                }
                                let box_w = popup
                                    .layout_cache
                                    .as_ref()
                                    .map(|layout| layout.max_line_w + pad * 2.0)
                                    .unwrap_or(320.0 * r.scale_factor);
                                let box_h = popup
                                    .layout_cache
                                    .as_ref()
                                    .map(|layout| layout.total_text_h + pad * 2.0)
                                    .unwrap_or(120.0 * r.scale_factor);
                                (box_w, box_h, max_h)
                            };
                            let (box_w, box_h) =
                                self.stable_autocomplete_detail_size(natural_w, natural_h, max_h);
                            let detail_byte_offset = self.active_autocomplete_detail_byte_offset();
                            let detail_phys_line = self
                                .active_autocomplete_detail_editor()
                                .line_offsets
                                .partition_point(|&o| o <= detail_byte_offset)
                                .saturating_sub(1);
                            let (popup_x, popup_y, line_top_y) = {
                                let r = self.renderer.as_mut().unwrap();
                                let gap = 16.0 * r.scale_factor;
                                let margin = 4.0 * r.scale_factor;
                                let placement =
                                    *self.autocomplete_detail_placement.get_or_insert_with(|| {
                                        autocomplete_detail_placement(
                                            (rx, ry, rw, rh),
                                            box_w,
                                            box_h,
                                            r.width,
                                            r.height,
                                            gap,
                                            margin,
                                        )
                                    });
                                let clamp_x = |value: f32| {
                                    value
                                        .max(margin)
                                        .min((r.width - box_w - margin).max(margin))
                                };
                                let (popup_x, popup_y) = match placement {
                                    1 => (clamp_x(rx + rw + gap), ry),
                                    -1 => (clamp_x(rx - gap - box_w), ry),
                                    2 => (clamp_x(rx), ry + rh + gap),
                                    _ => (clamp_x(rx), (ry - gap - box_h).max(margin)),
                                };
                                let vis_line_idx =
                                    r.phys_to_visual.get(detail_phys_line).copied().unwrap_or(0)
                                        as f32;
                                (
                                    popup_x,
                                    popup_y,
                                    vis_line_idx * r.line_height - render_scroll_y,
                                )
                            };
                            if let Some(start) = perf_layout_start {
                                perf_layout_ms = start.elapsed().as_secs_f64() * 1000.0;
                            }
                            popup.byte_offset = detail_byte_offset;
                            popup.anchor_x = popup_x;
                            popup.anchor_y = popup_y;
                            popup.offset_x = Some(0.0);
                            popup.offset_y = Some(popup_y - line_top_y);
                            popup.anim_progress = detail_anim_progress;
                            let selection = self.autocomplete_detail_selection();
                            let use_api_detail_editor =
                                self.ide_panel.api.api_mock_completion_focus().is_some();
                            let detail_editor = if use_api_detail_editor {
                                &self.ide_panel.api.input_editor
                            } else {
                                &self.editor
                            };
                            let renderer = self.renderer.as_mut().unwrap();
                            let ui_registry = &mut self.ui_registry;
                            let perf_detail_draw_start = perf_enabled.then(Instant::now);
                            let (bx, by, bw, bh, max_scroll) = renderer.draw_hover_popup(
                                &mut popup,
                                None,
                                selection,
                                detail_editor,
                                ui_registry,
                                Some(&mut self.hover),
                                mx,
                                my,
                                render_scroll_y,
                                &mut wants_pointer,
                                detail_opacity,
                                Some((box_w, box_h)),
                                None,
                            );
                            if let Some(start) = perf_detail_draw_start {
                                perf_detail_draw_ms = start.elapsed().as_secs_f64() * 1000.0;
                            }
                            self.autocomplete_detail_rect = Some((bx, by, bw, bh));
                            if mx >= bx && mx <= bx + bw && my >= by && my <= by + bh {
                                crate::app::mouse::clear_hover_popup(&mut self.hover);
                            }
                            self.autocomplete_detail_max_scroll = max_scroll;
                            self.autocomplete_detail_popup = Some(popup);
                        } else {
                            self.autocomplete_detail_rect = None;
                            self.autocomplete_detail_max_scroll = 0.0;
                        }
                    } else {
                        self.autocomplete_detail_rect = None;
                        self.autocomplete_detail_max_scroll = 0.0;
                    }
                    if let Some(start) = perf_total_start {
                        record_autocomplete_popup_perf(
                            start.elapsed().as_secs_f64() * 1000.0,
                            perf_list_ms,
                            perf_refresh_ms,
                            perf_layout_ms,
                            perf_detail_draw_ms,
                            self.autocomplete_options.len(),
                            perf_detail_len,
                            perf_detail_lines,
                        );
                    }
                    if self.autocomplete_hovered_idx.is_some() {
                        wants_pointer = true;
                    }
                } else {
                    self.autocomplete_rect = None;
                    self.autocomplete_detail_rect = None;
                }

                let popup_blocks_background = self.popup_blocks_background_at(mx, my);
                if popup_blocks_background {
                    self.ui_registry.reset_cursor_state();
                    if self.autocomplete_window_contains(mx, my) {
                        wants_pointer = self.autocomplete_hovered_idx.is_some();
                    } else {
                        wants_pointer = false;
                    }
                }

                let mut settings_cursor_mode = 0;
                if self.show_settings || self.settings_anim_progress > 0.0 {
                    // Помечаем границу: элементы оверлея регистрируются ниже.
                    // find_overlay_at() будет искать только среди них.
                    self.ui_registry.mark_overlay_start();
                    let rust_row = self.lsp.as_ref().map(crate::lsp::LspManager::rust_row_info);
                    let theme_selection = self.theme_selection();
                    settings_cursor_mode =self.renderer.as_mut().unwrap().draw_settings(
                        self.settings_anim_progress,
                        self.settings_tab,
                        theme_selection,
                        &self.faq_editor,
                        self.settings_scroll.current,
                        self.settings_general_scroll.current,
                        self.settings_database_scroll.current,
                        &mut self.settings_general_max_scroll,
                        &mut self.settings_database_max_scroll,
                        &self.ide_workspaces,
                        &self.ide_ignore_patterns,
                        &self.settings_ignore_editor,
                        self.settings_ignore_focused,
                        &mut self.settings_ignore_scroll_x,
                        self.settings_ide_scroll.current,
                        blink_alpha,
                        &self.tool_paths,
                        &self.tool_installer,
                        &self.dart_settings,
                        &self.rust_settings,
                        &self.dart_tool_state,
                        self.ide_panel
                            .lsp_servers
                            .iter()
                            .find(|server| server.name == "dart")
                            .map(|server| server.status),
                        rust_row.as_ref(),
                        self.ide_panel.database.settings(),
                        self.ctrl_wheel_multiplier,
                        &mut self.keymap_settings,
                        &mut self.ui_registry,
                    );
                    // Window resizes shrink the max; keep the scroll target inside it.
                    self.settings_general_scroll
                        .clamp_target(0.0, self.settings_general_max_scroll);
                    self.settings_database_scroll
                        .clamp_target(0.0, self.settings_database_max_scroll);
                    if settings_cursor_mode == 1 {
                        wants_pointer = true;
                    }
                }

                // Проверяем hover на зонах resize IDE-панелей — они требуют специальный курсор
                let mut ide_resize_cursor: Option<winit::window::CursorIcon> = None;
                let blocking_modal_open = self.database_blocking_modal_open();
                if crate::app::mouse::ide_root_resize_hover_enabled(
                    self.is_ide_mode,
                    self.show_settings,
                    popup_blocks_background,
                    blocking_modal_open,
                ) {
                    let r = self.renderer.as_ref().unwrap();
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let s = r.scale_factor;

                    let panel_left_w = self.ide_panel.visible_left_width(s);
                    let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    ide_resize_cursor = crate::app::mouse::ide_root_resize_cursor(
                        mx,
                        my,
                        s,
                        wh,
                        panel_left_w,
                        panel_bottom_h,
                        self.ide_panel.bottom_terminal_is_transparent(),
                    );

                    if !self.show_welcome {
                        if ide_resize_cursor.is_none()
                            && self.ui_registry.find_at(mx, my)
                                == Some(crate::ui_system::UiId::GitGraphResize)
                        {
                            ide_resize_cursor = Some(winit::window::CursorIcon::NsResize);
                        }
                        let query_results_resizing =
                            self.tabs.get(self.active_tab).is_some_and(|tab| {
                                matches!(
                                    &tab.kind,
                                    crate::app::EditorTabKind::DatabaseQuery(_, state)
                                        if state.result_view.is_resizing_height
                                )
                            });
                        if ide_resize_cursor.is_none()
                            && (query_results_resizing
                                || self.ui_registry.find_at(mx, my)
                                    == Some(crate::ui_system::UiId::DatabaseQueryResultResize))
                        {
                            ide_resize_cursor = Some(winit::window::CursorIcon::NsResize);
                        }
                    }
                }

                if self.markdown_toc.open
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    let (mx, my) = (renderer.last_mouse_x, renderer.last_mouse_y);
                    wants_pointer |= renderer.draw_markdown_toc(
                        &mut self.markdown_toc,
                        &mut self.ui_registry,
                        mx,
                        my,
                    );
                }

                let cursor_icon = if blocking_modal_open {
                    let (mx, my) = self.renderer.as_ref().map_or((-1.0, -1.0), |renderer| {
                        (renderer.last_mouse_x, renderer.last_mouse_y)
                    });
                    match self.ui_registry.find_overlay_at(mx, my) {
                        Some(crate::ui_system::UiId::DatabaseDialogField(_))
                        | Some(crate::ui_system::UiId::DatabaseTableCellEditor)
                        | Some(crate::ui_system::UiId::DatabaseTableModalInput) => {
                            winit::window::CursorIcon::Text
                        }
                        Some(
                            crate::ui_system::UiId::DatabaseDialogBackdrop
                            | crate::ui_system::UiId::DatabaseDialogBody
                            | crate::ui_system::UiId::DatabaseTableModalBody
                            | crate::ui_system::UiId::DatabaseQueryReviewBackdrop
                            | crate::ui_system::UiId::DatabaseQueryReviewBody
                            | crate::ui_system::UiId::DatabaseQueryResultBody
                            | crate::ui_system::UiId::DatabaseQueryReviewMessagesBody,
                        )
                        | None => winit::window::CursorIcon::Default,
                        Some(_) => winit::window::CursorIcon::Pointer,
                    }
                } else if let Some(rc) = ide_resize_cursor {
                    rc
                } else if self.ide_panel.is_resizing_left {
                    winit::window::CursorIcon::EwResize
                } else if self.ide_panel.is_resizing_bottom || self.ide_panel.git.graph_resizing {
                    winit::window::CursorIcon::NsResize
                } else if self
                    .ui_registry
                    .find_at(
                        self.renderer.as_ref().unwrap().last_mouse_x,
                        self.renderer.as_ref().unwrap().last_mouse_y,
                    )
                    .is_some_and(|id| {
                        matches!(
                            id,
                            crate::ui_system::UiId::DatabaseTableColumnResize(_)
                                | crate::ui_system::UiId::DatabaseQueryColumnResize(_)
                        )
                    })
                {
                    winit::window::CursorIcon::EwResize
                } else if self.ide_panel.api.api_python_runtime_overlay_active() {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    match self
                        .ui_registry
                        .find_overlay_at(mx, my)
                        .filter(|id| {
                            crate::app::api_client::ApiClientState::ui_id_is_api_python_runtime_overlay(
                                *id,
                            )
                        })
                    {
                        Some(crate::ui_system::UiId::ApiMockPythonUvPathInput)
                        | Some(crate::ui_system::UiId::ApiMockPythonCustomPathInput) => {
                            winit::window::CursorIcon::Text
                        }
                        Some(_) => winit::window::CursorIcon::Pointer,
                        None => winit::window::CursorIcon::Default,
                    }
                } else if self.ide_panel.project_search.help_open {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    match self.ui_registry.find_overlay_at(mx, my) {
                        Some(crate::ui_system::UiId::ProjectSearchHelp) => {
                            winit::window::CursorIcon::Pointer
                        }
                        _ => winit::window::CursorIcon::Default,
                    }
                } else if self.ide_panel.database.context_menu.is_some() {
                    let (mx, my) = {
                        let renderer = self.renderer.as_ref().unwrap();
                        (renderer.last_mouse_x, renderer.last_mouse_y)
                    };
                    crate::app::context_menu::context_menu_cursor(
                        self.ui_registry.find_overlay_at(mx, my),
                    )
                } else if self.ide_panel.file_tree_context_menu.is_some() {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    crate::app::file_tree::file_tree_context_menu_cursor(
                        self.ui_registry.find_overlay_at(mx, my),
                    )
                } else if self.file_tree_modal_overlay_active() {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    match self
                        .ui_registry
                        .find_at(mx, my)
                        .filter(|id| crate::app::App::ui_id_is_file_tree_overlay(*id))
                    {
                        Some(crate::ui_system::UiId::FileTreeCreateInput)
                        | Some(crate::ui_system::UiId::FileTreeRenameInput) => {
                            winit::window::CursorIcon::Text
                        }
                        Some(_) => winit::window::CursorIcon::Pointer,
                        None => winit::window::CursorIcon::Default,
                    }
                } else if let Some(id) = {
                    let r = self.renderer.as_ref().unwrap();
                    self.ui_registry
                        .find_at(r.last_mouse_x, r.last_mouse_y)
                        .filter(|id| {
                            matches!(
                                id,
                                crate::ui_system::UiId::InlineGitPanelBody
                                    | crate::ui_system::UiId::InlineGitPrevHunk
                                    | crate::ui_system::UiId::InlineGitNextHunk
                                    | crate::ui_system::UiId::InlineGitRollbackHunk
                            )
                        })
                } {
                    match id {
                        crate::ui_system::UiId::InlineGitPrevHunk
                        | crate::ui_system::UiId::InlineGitNextHunk
                        | crate::ui_system::UiId::InlineGitRollbackHunk => {
                            winit::window::CursorIcon::Pointer
                        }
                        _ => winit::window::CursorIcon::Default,
                    }
                } else if self.active_pdf_tab().is_some() {
                    // Links register as pointer areas and text lines as text regions; the registry decides.
                    if wants_pointer {
                        winit::window::CursorIcon::Pointer
                    } else if self.ui_registry.wants_text() {
                        winit::window::CursorIcon::Text
                    } else {
                        winit::window::CursorIcon::Default
                    }
                } else if self.active_tab_is_api_client() {
                    if self.ui_registry.wants_text() {
                        winit::window::CursorIcon::Text
                    } else if wants_pointer {
                        winit::window::CursorIcon::Pointer
                    } else {
                        winit::window::CursorIcon::Default
                    }
                } else if self.active_tab_is_database_table() {
                    if self.ui_registry.wants_text() {
                        winit::window::CursorIcon::Text
                    } else if wants_pointer {
                        winit::window::CursorIcon::Pointer
                    } else {
                        winit::window::CursorIcon::Default
                    }
                } else if self.markdown_mode() == crate::app::MarkdownMode::Read {
                    markdown_read_cursor_icon(
                        wants_pointer
                            || (self.markdown.hovered_link.is_some()
                                && self.ui_registry.hovered()
                                    == Some(crate::ui_system::UiId::MarkdownReadBody)),
                        popup_blocks_background,
                        &self.ui_registry,
                    )
                } else if wants_pointer {
                    winit::window::CursorIcon::Pointer
                } else if popup_blocks_background {
                    winit::window::CursorIcon::Default
                } else if !self.show_welcome {
                    let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
                    let window_height = self.window.as_ref().unwrap().inner_size().height as f32;
                    let max_scroll = self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .get_max_scroll(&self.editor, window_height);

                    let r = self.renderer.as_ref().unwrap();
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let padding = r.left_padding;
                    let minimap_w = r.minimap_width;
                    let s = r.scale_factor;
                    let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };

                    let diag_popup_hovered = self
                        .hover
                        .diag_rect
                        .map(|(rx, ry, rw, rh, _, _, _)| {
                            mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh
                        })
                        .unwrap_or(false);
                    let mut is_text = !diag_popup_hovered
                        && mx > padding
                        && mx < (window_width - minimap_w - scrollbar_w);

                    if r.max_scroll_x > 0.0 {
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        if my > wh - 14.0 * s {
                            is_text = false;
                        }
                    }

                    if self.scroll_x.is_dragging || self.scroll_y.is_dragging {
                        is_text = false;
                    }

                    if let Some((rx, ry, rw, rh)) = self.autocomplete_rect {
                        if mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh {
                            is_text = false;
                        }
                    }

                    let panel_bottom_h = if self.is_ide_mode && self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };

                    let is_transparent_terminal =
                        self.is_ide_mode && self.ide_panel.bottom_terminal_is_transparent();

                    if panel_bottom_h > 0.0 && my >= window_height - panel_bottom_h {
                        if !is_transparent_terminal {
                            is_text = false;
                        }
                    }

                    if self.show_settings
                        || self.modal_dialog_open()
                        || self.settings_anim_progress >= 1.5
                    {
                        is_text = settings_cursor_mode == 2;
                    }

                    if self.show_search && self.search_anim_y > -10.0 {
                        let scrollbar_x = self.search_panel_scrollbar_x_for_mode(
                            window_width,
                            minimap_w,
                            scrollbar_w,
                            s,
                        );
                        let geometry =
                            crate::render_view::search::search_panel_geometry(scrollbar_x, s);
                        let search_h = crate::render_view::search::SEARCH_PANEL_H * s;
                        let input_x = geometry.x + 10.0 * s;
                        let input_y = self.search_anim_y + 11.0 * s;
                        let input_w = geometry.input_w;
                        let input_h = 30.0 * s;

                        if mx >= geometry.x
                            && mx <= geometry.x + geometry.w
                            && my >= self.search_anim_y
                            && my <= self.search_anim_y + search_h
                        {
                            if mx >= input_x
                                && mx <= input_x + input_w
                                && my >= input_y
                                && my <= input_y + input_h
                            {
                                is_text = true;
                            } else {
                                is_text = false;
                            }
                        }
                    }

                    let hover_popup_hovered = if let Some((x, y, w, h)) = self.hover.rect {
                        mx >= x && mx <= x + w && my >= y && my <= y + h
                    } else {
                        false
                    };
                    if hover_popup_hovered {
                        winit::window::CursorIcon::Default
                    } else if self.modifiers.control_key() && self.ctrl_definition.target.is_some()
                    {
                        winit::window::CursorIcon::Pointer
                    } else if is_text || self.ui_registry.wants_text() {
                        winit::window::CursorIcon::Text
                    } else {
                        winit::window::CursorIcon::Default
                    }
                } else {
                    winit::window::CursorIcon::Default
                };

                if self.current_cursor != cursor_icon {
                    self.current_cursor = cursor_icon;
                    self.window.as_ref().unwrap().set_cursor(cursor_icon);
                }

                self.renderer.as_mut().unwrap().flush();

                let autocomplete_swap_start = autocomplete_frame_start.map(|_| Instant::now());
                let autocomplete_frame_metrics = if autocomplete_swap_start.is_some() {
                    let detail_len = self
                        .autocomplete_detail_popup
                        .as_ref()
                        .map(|popup| popup.text.len())
                        .unwrap_or(0);
                    let r = self.renderer.as_ref().unwrap();
                    Some((
                        self.autocomplete_options.len(),
                        detail_len,
                        self.autocomplete_anim_progress,
                        r.vertices.len(),
                        r.vertices.capacity(),
                        r.glyphs.len(),
                        r.ui_glyphs.len(),
                    ))
                } else {
                    None
                };

                FrameOutcome {
                    autocomplete_frame_start,
                    autocomplete_prev_frame,
                    autocomplete_swap_start,
                    autocomplete_frame_metrics,
                }
    }

    pub(crate) fn finish_main_frame(&mut self, outcome: FrameOutcome) {
                let FrameOutcome {
                    autocomplete_frame_start,
                    autocomplete_prev_frame,
                    autocomplete_swap_start,
                    autocomplete_frame_metrics,
                } = outcome;
                if let (Some(frame_start), Some(swap_start), Some(metrics)) = (
                    autocomplete_frame_start,
                    autocomplete_swap_start,
                    autocomplete_frame_metrics,
                ) {
                    record_autocomplete_frame_perf(
                        frame_start,
                        autocomplete_prev_frame,
                        swap_start,
                        swap_start.elapsed().as_secs_f64() * 1000.0,
                        metrics.0,
                        metrics.1,
                        metrics.2,
                        metrics.3,
                        metrics.4,
                        metrics.5,
                        metrics.6,
                    );
                }

                if self.autocomplete_active && self.autocomplete_anim_progress < 1.0 {
                    self.window.as_ref().unwrap().request_redraw();
                }

                if let Some(log) = self.pending_key_log.take() {
                    let now = std::time::Instant::now();
                    let t_total = now.duration_since(log.t0).as_secs_f64() * 1000.0;

                    let t_highlight = log.t_highlight.unwrap_or(log.t0);
                    let input_to_hl = t_highlight.duration_since(log.t0).as_secs_f64() * 1000.0;

                    let t_render = log.t_render.unwrap_or(t_highlight);
                    let hl_to_render = t_render.duration_since(t_highlight).as_secs_f64() * 1000.0;

                    let render_to_swap = now.duration_since(t_render).as_secs_f64() * 1000.0;

                    if crate::render_view::TELEMETRY_ENABLED
                        .load(std::sync::atomic::Ordering::Relaxed)
                        && !crate::platform::is_headless()
                    {
                        println!(
                            "Key: {:?} | Total: {:.2}ms (Input->HL: {:.2}ms, HL->RenderPrep: {:.2}ms, Render+Swap: {:.2}ms)",
                            log.key, t_total, input_to_hl, hl_to_render, render_to_swap
                        );
                    }
                }
    }
}
