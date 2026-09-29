/// LSP diagnostics of the active document filtered for the current root frame
/// (`Renderer::draw_root_prepare_diagnostics`).
struct RootFrameDiagnostics<'a> {
    cursor_phys_line: usize,
    instant_raw: Vec<&'a crate::lsp::Diagnostic>,
    has_lsp_diagnostics: bool,
}

/// Pointer, panel and modal layout of one root frame, computed once by
/// `Renderer::draw_root_panel_layout` and passed by value to the later phases.
#[derive(Clone, Copy)]
struct RootFramePanelLayout<'a> {
    total_lines: usize,
    visible_cursor_line: usize,
    s: f32,
    mx: f32,
    my: f32,
    real_height: f32,
    panel_left_w: f32,
    panel_bottom_h: f32,
    active_database_query: Option<(
        &'a crate::app::database::DatabaseQueryTabMeta,
        &'a crate::app::database::DatabaseQueryTabState,
    )>,
    database_query_modal_open: bool,
    database_query_results_h: f32,
    modal_overlay_open: bool,
    ui_mx: f32,
    ui_my: f32,
    status_progress_label: Option<&'a str>,
    status_progress_elapsed: Option<f32>,
    status_progress_value: Option<f32>,
    editor_bottom_h: f32,
    is_ui_disabled: bool,
}

/// Editor viewport geometry and rounded scroll of one root frame
/// (`Renderer::draw_root_editor_viewport_layout`).
#[derive(Clone, Copy)]
struct RootFrameViewport {
    tab_bar_visual_h: f32,
    tab_bar_h: f32,
    editor_height: f32,
    editor_scroll_height: f32,
    max_scroll: f32,
    scrollbar_width: f32,
    render_scroll_x: f32,
    render_scroll_y: f32,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_root_prepare_diagnostics<'a>(
        &mut self,
        editor: &Editor,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &'a [crate::app::EditorTab],
        active_tab: usize,
        lsp: Option<&'a crate::lsp::LspManager>,
    ) -> RootFrameDiagnostics<'a> {
        let cursor_phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= editor.cursor)
            .saturating_sub(1);

        // The matching hover context is set on `App::hover` before the frame
        // (`App::render_main_frame`), so this phase only reads diagnostics.
        let query_diagnostics = tabs.get(active_tab).and_then(|tab| match &tab.kind {
            crate::app::EditorTabKind::DatabaseQuery(_, state) => {
                Some(state.editor_diagnostics.as_slice())
            }
            _ => None,
        });
        let (diag_version, instant_raw, stale_instant_diagnostics) =
            if let Some(diagnostics) = query_diagnostics {
                (
                    crate::editor::lsp_document_version(editor.version),
                    diagnostics.iter().collect::<Vec<_>>(),
                    false,
                )
            } else if let Some(l) = lsp {
                if let Some(p) = editor_path {
                    let (version, diagnostics) = l.instant_merged_diagnostics(p);
                    (
                        version,
                        diagnostics,
                        l.has_stale_instant_diagnostics(p, editor.version),
                    )
                } else {
                    (0, Vec::new(), false)
                }
            } else {
                (0, Vec::new(), false)
            };

        let get_byte_offset = |line: u32, utf16_col: u32| -> usize {
            let line = line as usize;
            if line >= editor.line_offsets.len() {
                return editor.len();
            }
            let start = editor.line_offsets[line];
            let end = editor
                .line_offsets
                .get(line + 1)
                .copied()
                .unwrap_or(editor.len());
            let mut current_utf16 = 0;
            let mut current_byte = start;
            let (first, second) = editor.text_parts();
            let first_len = first.len();

            while current_byte < end {
                if current_utf16 >= utf16_col {
                    break;
                }
                let ch = if current_byte < first_len {
                    first[current_byte..].chars().next().unwrap_or('\0')
                } else {
                    second[current_byte - first_len..]
                        .chars()
                        .next()
                        .unwrap_or('\0')
                };
                current_utf16 += ch.len_utf16() as u32;
                current_byte += ch.len_utf8();
            }
            current_byte
        };

        self.lsp_diagnostic_indices.clear();
        self.unused_spans_cache.clear();
        let transient_member_dot = transient_python_member_dot_byte(editor);
        for (idx, &d) in instant_raw.iter().enumerate() {
            let diag_line = d.start_line as usize;
            let mut suppress = false;

            if diag_line == cursor_phys_line {
                if should_suppress_active_line_useless_expression(d, cursor_phys_line) {
                    suppress = true;
                }

                if (diag_version as u64) < editor.version || stale_instant_diagnostics {
                    suppress = true;
                }

                let code = d.code.as_deref().unwrap_or("");
                if code == "W291" || code == "W293" {
                    suppress = true;
                }

                if let Some(dot_byte) = transient_member_dot {
                    let start = get_byte_offset(d.start_line, d.start_col);
                    let end = get_byte_offset(d.end_line, d.end_col);
                    if diagnostic_overlaps_transient_member_dot(
                        Some(dot_byte),
                        editor.cursor,
                        start,
                        end,
                    ) {
                        suppress = true;
                    }
                }
            }

            if !suppress {
                self.lsp_diagnostic_indices.push(idx);
                if d.tags.contains(&1) || d.tags.contains(&2) {
                    let start = get_byte_offset(d.start_line, d.start_col);
                    let end = get_byte_offset(d.end_line, d.end_col);
                    if start < end {
                        self.unused_spans_cache.push((start, end));
                    }
                }
            }
        }
        self.unused_spans_cache.sort_unstable_by_key(|&(s, _)| s);
        let has_lsp_diagnostics = !self.lsp_diagnostic_indices.is_empty();
        RootFrameDiagnostics {
            cursor_phys_line,
            instant_raw,
            has_lsp_diagnostics,
        }
    }

    fn draw_root_panel_layout<'a>(
        &mut self,
        editor: &Editor,
        tabs: &'a [crate::app::EditorTab],
        active_tab: usize,
        ide_panel: &'a crate::app::IdePanelState,
        scroll_x: f32,
        scroll_y: f32,
        cursor_phys_line: usize,
        frame_now: Instant,
        markdown_read_active: bool,
        show_settings: bool,
        dialog_window_open: bool,
        is_ide_mode: bool,
    ) -> RootFramePanelLayout<'a> {
        let (total_lines, visible_cursor_line) = if markdown_read_active {
            // Preview owns its own virtualized layout and scroll surface; avoid rebuilding
            // source-only fold mappings on a Read-mode frame.
            (1, 0)
        } else {
            let total_lines = self.ensure_editor_visual_line_map(editor);
            let visible_cursor_line = self
                .phys_to_visual
                .get(cursor_phys_line)
                .copied()
                .unwrap_or(cursor_phys_line);
            (total_lines, visible_cursor_line)
        };
        let s = self.scale_factor;
        let mx = if show_settings || dialog_window_open {
            -1.0
        } else {
            self.last_mouse_x
        };
        let my = if show_settings || dialog_window_open {
            -1.0
        } else {
            self.last_mouse_y
        };

        let real_height = self.height;
        let panel_left_w = if is_ide_mode {
            ide_panel.visible_left_width(s)
        } else {
            0.0
        };
        let panel_bottom_h = if is_ide_mode && ide_panel.any_bottom_open() {
            ide_panel.bottom_height * s
        } else {
            0.0
        };
        let active_database_query = tabs.get(active_tab).and_then(|tab| match &tab.kind {
            crate::app::EditorTabKind::DatabaseQuery(meta, state) => Some((meta, state)),
            _ => None,
        });
        let database_query_modal_open =
            active_database_query.is_some_and(|(_, state)| state.review.is_some());
        let database_query_results_open = active_database_query
            .is_some_and(|(_, state)| crate::app::database::database_query_results_visible(state));
        let database_query_results_h = if database_query_results_open {
            active_database_query.map_or(0.0, |(_, state)| {
                crate::app::database::database_query_results_height(
                    state.result_view.preferred_height,
                    real_height,
                    panel_bottom_h,
                    s,
                )
            })
        } else {
            0.0
        };
        let modal_overlay_open = is_ide_mode
            && (ide_panel.api.mock_python_runtime_open
                || ide_panel.api.mock_guide_open
                || ide_panel.api.mock_server_detail_open
                || ide_panel.project_search.help_open
                || ide_panel.database.modal_open()
                || database_query_modal_open
                || crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel));
        let (ui_mx, ui_my) = if modal_overlay_open {
            (-1.0, -1.0)
        } else {
            (mx, my)
        };
        // Only real background work uses the progress channel; the PDF page label has its own slot.
        let (status_progress_label, status_progress_elapsed, status_progress_value) =
            match ide_panel.api.mock.server_status {
                crate::app::api_mock::types::ApiMockServerStatus::Starting => {
                    (Some("Мок-сервер"), None, Some(0.55))
                }
                _ => (
                    ide_panel.git.pending_label.as_deref(),
                    ide_panel.git.pending_elapsed_secs(frame_now),
                    None,
                ),
            };
        let editor_bottom_h = if is_ide_mode {
            ide_panel.editor_reserved_bottom_height(s) + database_query_results_h
        } else {
            0.0
        };
        let is_ui_disabled = is_ide_mode && ide_panel.terminal_focused;

        self.update_popup_mouse_move_gate();
        let popup_scroll_changed = self.update_popup_scroll_snapshot(scroll_x, scroll_y);
        if self.last_editor_version_for_typing != editor.version
            || self.last_cursor_for_popups != editor.cursor
            || popup_scroll_changed
        {
            self.suppress_popups_until_next_mouse_move();
            self.last_editor_version_for_typing = editor.version;
            self.last_cursor_for_popups = editor.cursor;
        }
        RootFramePanelLayout {
            total_lines,
            visible_cursor_line,
            s,
            mx,
            my,
            real_height,
            panel_left_w,
            panel_bottom_h,
            active_database_query,
            database_query_modal_open,
            database_query_results_h,
            modal_overlay_open,
            ui_mx,
            ui_my,
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            editor_bottom_h,
            is_ui_disabled,
        }
    }

    fn draw_root_editor_viewport_layout(
        &mut self,
        editor: &Editor,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        markdown: &mut crate::app::MarkdownTabState,
        scroll_y_state: &mut crate::scroll::ScrollState,
        current_sticky_lines: &[(usize, usize)],
        layout: RootFramePanelLayout<'_>,
        scroll_x: f32,
        mut scroll_y: f32,
        is_resizing: bool,
        show_welcome: bool,
        is_ide_mode: bool,
        markdown_read_active: bool,
        telemetry_frame_start: Option<Instant>,
        telemetry_root_phases: &mut [f32; 5],
    ) -> RootFrameViewport {
        let RootFramePanelLayout {
            total_lines,
            s,
            real_height,
            panel_left_w,
            active_database_query,
            editor_bottom_h,
            ..
        } = layout;
        let tab_bar_visual_h = crate::render_view::ide_tab_bar_height(show_welcome, is_ide_mode, s);
        let tab_bar_h = crate::render_view::editor_content_top_inset(
            show_welcome,
            is_ide_mode,
            active_database_query.is_some(),
            s,
        );
        let editor_height =
            editor_view_height(real_height, tab_bar_h, editor_bottom_h, is_ide_mode, s);
        let editor_scroll_height = editor_height;

        if !markdown_read_active
            && self.resolve_markdown_edit_scroll_transition(
                markdown,
                editor,
                scroll_y_state,
                current_sticky_lines,
                editor_height,
            )
        {
            scroll_y = scroll_y_state.current;
        }

        let active_tab_is_git_diff_for_layout = tabs
            .get(active_tab)
            .is_some_and(|tab| tab.kind.is_git_diff());
        let target_padding = editor_left_padding_for(
            editor.line_offsets.len(),
            active_tab_is_git_diff_for_layout,
            is_ide_mode,
            panel_left_w,
            s,
        );
        if (self.left_padding - target_padding).abs() > 0.5 {
            self.left_padding = target_padding;
            self.visual_lines.clear();
        }

        let max_scroll =
            editor_max_scroll_for_lines(total_lines, self.line_height, editor_scroll_height);
        let scrollbar_width = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };
        let target_minimap_w = 119.0 * s;
        let minimap_w =
            if self.width - self.left_padding - scrollbar_width - target_minimap_w
                < 2.0 * target_minimap_w
            {
                0.0
            } else {
                target_minimap_w
            };
        if (self.minimap_width - minimap_w).abs() > 0.5 {
            self.minimap_width = minimap_w;
            self.visual_lines.clear();
        }

        // self.height = real_height — текст рендерится на полную высоту окна,
        // включая зону нижней панели (нужно для работы прозрачности панели).
        let cache_start = telemetry_frame_start.map(|_| Instant::now());
        if !markdown_read_active {
            self.update_cache(editor, scroll_x, scroll_y, is_resizing);
            markdown.remember_displayed_edit_geometry(editor.version, self.line_height);
        }
        if let Some(cache_start) = cache_start {
            telemetry_root_phases[1] = cache_start.elapsed().as_secs_f32();
        }

        let render_scroll_x = scroll_x.round();
        let render_scroll_y = scroll_y.round() - tab_bar_h;

        if !markdown_read_active {
            self.update_max_scroll_x(editor);
        }
        RootFrameViewport {
            tab_bar_visual_h,
            tab_bar_h,
            editor_height,
            editor_scroll_height,
            max_scroll,
            scrollbar_width,
            render_scroll_x,
            render_scroll_y,
        }
    }

    fn draw_root_surface_and_side_panels(
        &mut self,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        layout: RootFramePanelLayout<'_>,
        has_lsp_diagnostics: bool,
        is_ide_mode: bool,
        blink_alpha: f32,
        telemetry_frame_start: Option<Instant>,
        telemetry_root_phases: &mut [f32; 5],
        telemetry_side_panel_time: &mut f32,
    ) -> Option<(crate::app::api_client::ApiSpecId, usize)> {
        let RootFramePanelLayout {
            s,
            real_height,
            panel_left_w,
            ui_mx,
            ui_my,
            is_ui_disabled,
            ..
        } = layout;
        // С этого момента self.height = real_height на всём протяжении кадра.
        // Матрица проекции в flush() всегда корректна.
        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.active_texture(glow::TEXTURE1);
            self.gl.bind_texture(glow::TEXTURE_2D, self.color_texture);
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.clear_color(
                crate::renderer::EDITOR_SURFACE_BG[0],
                crate::renderer::EDITOR_SURFACE_BG[1],
                crate::renderer::EDITOR_SURFACE_BG[2],
                crate::renderer::EDITOR_SURFACE_BG[3],
            );
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        let active_api_route = tabs.get(active_tab).and_then(|tab| match &tab.kind {
            crate::app::EditorTabKind::ApiClient(meta, state) if !state.auth_view => {
                Some((meta.spec_id, state.route_idx.unwrap_or(0)))
            }
            _ => None,
        });
        if let Some(frame_start) = telemetry_frame_start {
            telemetry_root_phases[0] =
                (frame_start.elapsed().as_secs_f32() - telemetry_root_phases[1]).max(0.0);
        }
        if is_ide_mode {
            let stage_start = telemetry_frame_start.map(|_| Instant::now());
            self.draw_ide_side_panels(
                ide_panel,
                lsp,
                ui_registry,
                has_lsp_diagnostics,
                s,
                ui_mx,
                ui_my,
                real_height,
                panel_left_w,
                is_ui_disabled,
                blink_alpha,
                active_api_route,
            );
            if let Some(stage_start) = stage_start {
                *telemetry_side_panel_time = stage_start.elapsed().as_secs_f32();
            }
        }
        active_api_route
    }
}
