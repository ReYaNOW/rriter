#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_root_editor_chrome(
        &mut self,
        editor: &Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        spans: &[ColorSpan],
        current_sticky_lines: &[(usize, usize)],
        lsp_diagnostics: &[&crate::lsp::Diagnostic],
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        ide_workspaces: &[std::path::PathBuf],
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        editor_text: RootFrameEditorText<'_>,
        overlays: RootFrameEditorOverlays,
        tab_scroll_x: f32,
        sticky_anim_progress: f32,
        sticky_anim_is_adding: bool,
        is_resizing: bool,
        show_welcome: bool,
        is_ide_mode: bool,
        show_fps: bool,
        dialog_window_open: bool,
        telemetry_frame_start: Option<Instant>,
        telemetry_minimap_time: &mut f32,
        telemetry_chrome_details: &mut [f32; 6],
    ) -> (Vec<(usize, usize)>, RootFrameChrome) {
        let RootFramePanelLayout {
            total_lines,
            visible_cursor_line,
            s,
            mx,
            my,
            real_height,
            panel_bottom_h,
            active_database_query,
            database_query_results_h,
            ui_mx,
            ui_my,
            editor_bottom_h,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_visual_h,
            tab_bar_h,
            editor_height,
            editor_scroll_height,
            max_scroll,
            scrollbar_width,
            render_scroll_x,
            ..
        } = viewport;
        let RootFrameEditorText {
            render_scroll_y,
            minimap_w,
            minimap_x,
            scrollbar_x,
            ..
        } = editor_text;
        let RootFrameEditorOverlays { gutter_x, .. } = overlays;
        let stage_start = telemetry_frame_start.map(|_| Instant::now());
        if minimap_w > 0.0 {
            self.draw_minimap(
                editor,
                spans,
                render_scroll_y,
                max_scroll,
                total_lines,
                visible_cursor_line,
                editor_scroll_height,
                tab_bar_h,
            );
        }
        if let Some(stage_start) = stage_start {
            *telemetry_minimap_time = stage_start.elapsed().as_secs_f32();
        }
        let chrome_start = telemetry_frame_start.map(|_| Instant::now());
        let mut chrome_detail_start = chrome_start;

        if minimap_w > 0.0 {
            ui_registry.register_rect(
                crate::ui_system::UiId::EditorMinimap,
                minimap_x,
                tab_bar_h,
                minimap_w,
                editor_scroll_height,
                ui_mx,
                ui_my,
            );
        }

        self.draw_editor_horizontal_scrollbar(
            render_scroll_x,
            scrollbar_x,
            editor_bottom_h,
            is_ide_mode,
            real_height,
            s,
        );

        let tab_tooltip = self.draw_root_tab_chrome(
            tabs,
            active_tab,
            editor,
            editor_title,
            editor_path,
            show_welcome,
            is_ide_mode,
            gutter_x,
            panel_bottom_h,
            tab_bar_visual_h,
            s,
            ui_mx,
            ui_my,
            mx,
            my,
            ui_registry,
            tab_scroll_x,
            ide_panel,
            lsp,
            ide_workspaces,
        );
        if !show_welcome
            && is_ide_mode
            && let Some((query_meta, query_state)) = active_database_query
        {
            let tab_x = gutter_x.round() + 1.0;
            let tab_w = self.width - tab_x;
            self.draw_database_query_chrome(
                tab_x,
                tab_bar_visual_h,
                tab_w,
                tab_bar_h + editor_height,
                database_query_results_h,
                s,
                query_meta,
                query_state,
                &ide_panel.database.persisted.query_history,
                ui_registry,
                ui_mx,
                ui_my,
                mx,
                my,
            );
            self.flush();
        }
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[0] = start.elapsed().as_secs_f32();
        }

        let target_sticky_lines = if show_welcome {
            Vec::new()
        } else {
            self.draw_sticky_lines(
                editor,
                spans,
                current_sticky_lines,
                render_scroll_y,
                render_scroll_x,
                sticky_anim_progress,
                sticky_anim_is_adding,
                gutter_x.round() + 1.0,
                ui_registry,
                tab_bar_h,
            )
        };

        // --- 8.5. Линейка диагностики рядом со скроллбаром ---
        if !is_resizing && is_ide_mode && !dialog_window_open {
            self.draw_diagnostics_ruler(
                editor,
                lsp_diagnostics,
                tab_bar_h,
                editor_scroll_height,
                scrollbar_width,
            );
        }

        self.draw_root_editor_vertical_scrollbar(editor, ui_registry, layout, viewport, editor_text);

        self.register_editor_horizontal_scrollbar(
            ui_registry,
            scrollbar_x,
            editor_bottom_h,
            is_ide_mode,
            real_height,
            s,
        );

        self.draw_root_fps_if_visible(show_fps, minimap_w);
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[1] = start.elapsed().as_secs_f32();
        }

        (
            target_sticky_lines,
            RootFrameChrome {
                tab_tooltip,
                chrome_start,
                chrome_detail_start,
            },
        )
    }

    fn draw_root_editor_vertical_scrollbar(
        &mut self,
        editor: &Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        editor_text: RootFrameEditorText<'_>,
    ) {
        let RootFramePanelLayout { total_lines, s, .. } = layout;
        let RootFrameViewport {
            tab_bar_h,
            editor_scroll_height,
            max_scroll,
            scrollbar_width,
            ..
        } = viewport;
        let RootFrameEditorText {
            render_scroll_y,
            scrollbar_x,
            active_git_diff_state,
            ..
        } = editor_text;
        if scrollbar_width > 0.0 {
            // Git diff marks sit under the thumb, so they are pushed first.
            if let Some(state) = active_git_diff_state {
                let total = total_lines.max(1) as f32;
                for hunk in &state.hunks {
                    let start_ratio = hunk.display_start_line as f32 / total;
                    let display_end_line = editor
                        .line_offsets
                        .partition_point(|&offset| offset < hunk.display_end)
                        .max(hunk.display_start_line + 1);
                    let line_count = display_end_line
                        .saturating_sub(hunk.display_start_line)
                        .max(1);
                    let mark_y = tab_bar_h + start_ratio * editor_scroll_height;
                    let mark_h = ((line_count as f32 / total) * editor_scroll_height)
                        .max(2.0 * s)
                        .min(18.0 * s);
                    let mut has_old = false;
                    let mut has_new = false;
                    let start = hunk.display_start_line.min(state.line_kinds.len());
                    let end = (start + line_count).min(state.line_kinds.len());
                    for kind in &state.line_kinds[start..end] {
                        match kind {
                            crate::app::git_diff::DiffLineKind::Deleted
                            | crate::app::git_diff::DiffLineKind::ModifiedOld => has_old = true,
                            crate::app::git_diff::DiffLineKind::Added
                            | crate::app::git_diff::DiffLineKind::ModifiedNew => has_new = true,
                            crate::app::git_diff::DiffLineKind::Context => {}
                        }
                    }
                    if has_old {
                        self.push_rounded_rect(
                            scrollbar_x + 1.0 * s,
                            mark_y,
                            3.0 * s,
                            mark_h,
                            1.5 * s,
                            [0.76, 0.78, 0.84, 0.90],
                        );
                    }
                    if has_new {
                        self.push_rounded_rect(
                            scrollbar_x + scrollbar_width - 4.0 * s,
                            mark_y,
                            3.0 * s,
                            mark_h,
                            1.5 * s,
                            [0.18, 0.82, 0.34, 0.95],
                        );
                    }
                }
            }
            let bar = editor_vertical_scrollbar(
                (scrollbar_x, tab_bar_h, scrollbar_width, editor_scroll_height),
                editor_scroll_content_height(total_lines, self.line_height, editor_scroll_height),
                max_scroll,
                render_scroll_y,
            );
            let hit = crate::render_view::scrollbar_widget::ScrollbarHit {
                ui: ui_registry,
                id: crate::ui_system::UiId::EditorScrollbarY,
                mx: self.last_mouse_x,
                my: self.last_mouse_y,
                blocker: false,
            };
            let _ = self.draw_scrollbar(&bar, s, 1.0, Some(hit));
        }
    }

    fn draw_root_editor_panels_and_hover(
        &mut self,
        editor: &Editor,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        markdown: &crate::app::MarkdownTabState,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        search_editor: &Editor,
        lsp_diagnostics: &[&crate::lsp::Diagnostic],
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover: &mut crate::app::mouse::HoverState,
        inline_git_popup: Option<&crate::app::InlineGitPopup>,
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        editor_text: RootFrameEditorText<'_>,
        overlays: RootFrameEditorOverlays,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
        has_lsp_diagnostics: bool,
        scroll_x: f32,
        show_search: bool,
        search_anim_y: f32,
        search_focused: bool,
        search_case_sensitive: bool,
        show_welcome: bool,
        is_ide_mode: bool,
        dialog_window_open: bool,
        blink_alpha: f32,
        wants_pointer: &mut bool,
        chrome_detail_start: &mut Option<Instant>,
        telemetry_chrome_details: &mut [f32; 6],
    ) {
        let RootFramePanelLayout {
            s,
            mx,
            my,
            panel_bottom_h,
            active_database_query,
            database_query_modal_open,
            ui_mx,
            ui_my,
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            is_ui_disabled,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_h,
            editor_height,
            editor_scroll_height,
            scrollbar_width,
            render_scroll_x,
            ..
        } = viewport;
        let RootFrameEditorText {
            render_scroll_y,
            minimap_w,
            scrollbar_x,
            active_git_diff_state,
            ..
        } = editor_text;
        let RootFrameEditorOverlays {
            hovered_diag_type_target,
            gutter_x,
        } = overlays;
        self.draw_inline_git_popup_panel(
            editor,
            inline_git_popup,
            active_git_diff_state.is_some(),
            show_welcome,
            render_scroll_x,
            render_scroll_y,
            editor_height,
            tab_bar_h,
            scrollbar_x,
            ui_registry,
            ui_mx,
            ui_my,
            s,
        );

        self.draw_git_diff_hunk_panel(
            active_git_diff_state,
            show_welcome,
            minimap_w,
            scrollbar_width,
            gutter_x,
            tab_bar_h,
            render_scroll_y,
            editor_scroll_height,
            ui_registry,
            ui_mx,
            ui_my,
            s,
        );

        *wants_pointer |= self.draw_search_panel_if_visible(
            show_search,
            search_anim_y,
            search_editor,
            search_focused,
            search_case_sensitive,
            search_results,
            search_current_idx,
            blink_alpha,
            search::search_panel_scrollbar_x(self.width, self.minimap_width, scrollbar_width, None),
            ui_registry,
        );
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[2] = start.elapsed().as_secs_f32();
        }

        // self.height уже = real_height на всём протяжении, ничего восстанавливать не нужно

        self.draw_root_bottom_status_and_dim(
            editor,
            editor_path,
            markdown.mode,
            tabs,
            active_tab,
            ide_panel,
            lsp,
            ui_registry,
            has_lsp_diagnostics,
            s,
            ui_mx,
            ui_my,
            panel_bottom_h,
            is_ui_disabled,
            blink_alpha,
            active_api_route,
            is_ide_mode,
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            dialog_window_open,
        );
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[3] = start.elapsed().as_secs_f32();
        }

        let status_bar_y = if is_ide_mode {
            ide_status_bar_y(self.height, panel_bottom_h, s)
        } else {
            self.height
        };
        let hover_blocked_by_status_bar =
            is_ide_mode && my >= status_bar_y && my <= status_bar_y + ide_status_bar_height(s);
        let hover_blocked_by_bottom_panel = is_ide_mode
            && panel_bottom_h > 0.0
            && ide_panel.bottom_panel_blocks_editor_hover()
            && my >= ide_bottom_panel_y(self.height, panel_bottom_h, s)
            && my <= ide_bottom_panel_y(self.height, panel_bottom_h, s) + panel_bottom_h;
        let hover_blocked_by_inline_git = inline_git_popup.is_some()
            && matches!(
                ui_registry.find_at(mx, my),
                Some(
                    crate::ui_system::UiId::InlineGitPanelBody
                        | crate::ui_system::UiId::InlineGitPrevHunk
                        | crate::ui_system::UiId::InlineGitNextHunk
                        | crate::ui_system::UiId::InlineGitRollbackHunk
                )
            );
        let hover_blocked_by_database_query_results = active_database_query.is_some()
            && crate::app::mouse::HoverState::database_query_results_block_hover_at(
                ui_registry,
                mx,
                my,
            );
        let file_tree_overlay_open =
            crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel);
        // A blocked hover is cleared by `App::render_main_frame` after the frame
        // (`editor_hover_blocked`), so this phase does not reset hover state itself.
        let hover_blocked = hover_blocked_by_status_bar
            || hover_blocked_by_bottom_panel
            || hover_blocked_by_inline_git
            || hover_blocked_by_database_query_results
            || file_tree_overlay_open
            || ide_panel.database.modal_open()
            || database_query_modal_open
            || ide_panel.project_search.help_open
            || ide_panel.api.mock_guide_open
            || ide_panel.api.mock_server_detail_open
            || ide_panel.api.mock_python_runtime_open;
        self.editor_hover_blocked = hover_blocked;
        if !hover_blocked && !is_ui_disabled {
            self.draw_hover_overlays(
                hover,
                editor,
                lsp_diagnostics,
                ide_panel,
                ui_registry,
                mx,
                my,
                scroll_x,
                render_scroll_y,
                hovered_diag_type_target,
                wants_pointer,
                None,
            );
        }
    }
}

/// Editor vertical scrollbar shared by `draw_root_editor_vertical_scrollbar` and the
/// `EditorScrollbarY` press/drag handlers. `max_scroll` is the line-rounded editor range
/// (`get_max_scroll`), which may exceed `content_h - viewport`.
pub(crate) fn editor_vertical_scrollbar(
    lane: crate::render_view::scrollbar_widget::ScrollbarRect,
    content_h: f32,
    max_scroll: f32,
    scroll: f32,
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    Scrollbar {
        style: ScrollbarStyle::EDITOR_Y,
        axis: ScrollbarAxis::Vertical,
        lane,
        extent: ScrollbarExtent {
            max_scroll,
            ..ScrollbarExtent::new(lane.3, content_h.max(lane.3), scroll)
        },
    }
}
