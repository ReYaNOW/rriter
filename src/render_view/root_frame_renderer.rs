#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_fps_overlay(&mut self, minimap_w: f32) {
        let center_x = (self.width - minimap_w) / 2.0;
        self.push_rect(center_x - 45.0, 5.0, 90.0, 25.0, self.ui.pick(crate::theme::UiRole::BgPanelAlt, [0.1, 0.1, 0.1, 0.8]));

        let fps_text = std::mem::take(&mut self.fps_string);
        self.draw_string(&fps_text, center_x - 40.0, 24.0, self.ui.pick(crate::theme::UiRole::Info, [0.0, 1.0, 0.0, 1.0]));
        self.fps_string = fps_text;
    }

    fn resolve_markdown_edit_scroll_transition(
        &mut self,
        markdown: &mut crate::app::MarkdownTabState,
        markdown_media: &crate::markdown_media::MarkdownMedia,
        editor: &Editor,
        scroll: &mut crate::scroll::ScrollState,
        current_sticky_lines: &[(usize, usize)],
        editor_height: f32,
    ) -> bool {
        let Some(transition) = markdown.pending_transition_for(crate::app::MarkdownMode::Edit)
        else {
            return false;
        };
        if !markdown.pending_transition_is_valid(&transition, editor.version) {
            markdown.cancel_stale_scroll_transition();
            return false;
        }

        let sticky_inset = current_sticky_lines.len() as f32 * self.line_height;
        let deferred_applied = scroll.deferred_current_rebase_applied() == Some(true);
        let reproject_applied_current = deferred_applied
            && !markdown.deferred_current_geometry_matches_edit(
                editor.version,
                self.line_height,
                sticky_inset,
            );
        let applied_anchor = if reproject_applied_current {
            let Some((geometry, viewport_inset)) = markdown.deferred_edit_current_geometry() else {
                markdown.cancel_stale_scroll_transition();
                return false;
            };
            if geometry.version != editor.version {
                markdown.cancel_stale_scroll_transition();
                return false;
            }
            self.markdown_edit_viewport_anchor_with_line_height(
                editor,
                scroll.current + viewport_inset,
                geometry.line_height,
            )
        } else {
            None
        };

        let anchor = if reproject_applied_current {
            applied_anchor
        } else if let Some(anchor) = transition.anchor.clone() {
            Some(anchor)
        } else if transition.from == crate::app::MarkdownMode::Read {
            transition.origin_read_width.and_then(|width| {
                self.prepare_markdown_read_layout(markdown, markdown_media, editor.version, width)
                    .then(|| {
                        markdown
                            .read_layout
                            .viewport_source_anchor(transition.origin_scroll_y)
                    })
                    .flatten()
            })
        } else {
            None
        };
        let Some(anchor) = anchor else {
            markdown.cancel_stale_scroll_transition();
            return false;
        };
        let Some(line_y) = self.markdown_edit_source_y(editor, &anchor.source_range) else {
            markdown.cancel_stale_scroll_transition();
            return false;
        };
        let max_scroll = self.get_max_scroll(editor, editor_height);
        if let Some((target, relative_delta)) =
            markdown.pending_absolute_scroll_target(crate::app::MarkdownMode::Edit, scroll.target)
        {
            let target = match target {
                crate::app::MarkdownAbsoluteScrollTarget::Source {
                    source_range,
                    viewport_ratio,
                } => {
                    let Some(source_y) = self.markdown_edit_source_y(editor, &source_range) else {
                        markdown.cancel_stale_scroll_transition();
                        return false;
                    };
                    source_y - editor_height * viewport_ratio
                }
                crate::app::MarkdownAbsoluteScrollTarget::Start => 0.0,
                crate::app::MarkdownAbsoluteScrollTarget::End => max_scroll,
            };
            if !target.is_finite() {
                markdown.cancel_stale_scroll_transition();
                return false;
            }
            let max_scroll = max_scroll.max(0.0);
            let resolved_target = target.clamp(0.0, max_scroll).round();
            scroll.set_target((resolved_target + relative_delta).clamp(0.0, max_scroll));
            markdown.remember_pending_absolute_scroll_target_y(resolved_target);
        }
        markdown.apply_scroll_transition_with_reprojection(
            scroll,
            editor.version,
            anchor,
            line_y,
            sticky_inset,
            max_scroll,
            reproject_applied_current,
        )
    }

    /// Root frame composition. Each phase lives in a `draw_root_*` method (layout in
    /// `root_frame_layout_renderer.rs`, API/Database/Markdown Read frames in
    /// `root_frame_content_frames_renderer.rs`, editor phases in
    /// `root_frame_editor_text_renderer.rs` and `root_frame_editor_chrome_renderer.rs`);
    /// this method keeps their order, the telemetry locals and every early return.
    pub fn draw(
        &mut self,
        editor: &mut Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        scroll_x: f32,
        scroll_y_state: &mut crate::scroll::ScrollState,
        markdown: &mut crate::app::MarkdownTabState,
        blink_alpha: f32,
        show_fps: bool,
        spans: &[ColorSpan],
        dialog_window_open: bool,
        is_resizing: bool,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        show_search: bool,
        search_anim_y: f32,
        search_editor: &Editor,
        search_focused: bool,
        search_case_sensitive: bool,
        show_welcome: bool,
        recent_files: &[std::path::PathBuf],
        current_sticky_lines: &[(usize, usize)],
        sticky_anim_progress: f32,
        sticky_anim_is_adding: bool,
        is_ide_mode: bool,
        ide_panel: &crate::app::IdePanelState,
        show_settings: bool,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        // Owned by `App::hover`; the frame writes back only hit-test/layout outputs
        // (popup rects, hovered diagnostics, dwell timers), like `ui_registry`.
        hover: &mut crate::app::mouse::HoverState,
        tab_scroll_x: f32,
        _syntax_errors: &[(usize, usize)],
        ctrl_definition_range: Option<(usize, usize)>,
        python_inlay_hints: &[crate::app::PythonInlayHint],
        closing_hints: &[crate::languages::dart::ClosingHint],
        ide_workspaces: &[std::path::PathBuf],
        show_readonly_notice: bool,
        readonly_notice_text: &str,
        inline_git_popup: Option<&crate::app::InlineGitPopup>,
        inline_blame_dwell: &crate::app::git_blame::InlineBlameDwell,
        pdf_engine: &crate::app::pdf_tab::PdfEngineState,
        pdf_dark_pages: bool,
        markdown_media: &crate::markdown_media::MarkdownMedia,
        empty_ide_open_label: &str,
    ) -> (bool, Vec<(usize, usize)>) {
        let scroll_y = scroll_y_state.current;
        self.editor_hover_blocked = false;
        self.icon_rasterize_budget = 1;
        self.sync_current_python_inlay_hints(python_inlay_hints);

        let frame_now = Instant::now();
        let telemetry_frame_start = TELEMETRY_ENABLED.load(Ordering::Relaxed).then(Instant::now);
        let telemetry_was_typing = telemetry_frame_start
            .is_some()
            .then_some(self.last_editor_version_for_typing != editor.version);
        let telemetry_was_scrolling = telemetry_frame_start.is_some().then_some(
            (self.last_scroll_y - scroll_y).abs() > 0.1
                || (self.last_scroll_x - scroll_x).abs() > 0.1,
        );
        let mut telemetry_editor_time = 0.0;
        let mut telemetry_minimap_time = 0.0;
        let mut telemetry_side_panel_time = 0.0;
        let mut telemetry_root_phases = [0.0; 5];
        let mut telemetry_chrome_details = [0.0; 6];
        let markdown_read_active =
            crate::render_view::markdown_read::markdown_read_active(markdown.mode);

        let RootFrameDiagnostics {
            cursor_phys_line,
            instant_raw,
            has_lsp_diagnostics,
        } = self.draw_root_prepare_diagnostics(editor, editor_path, tabs, active_tab, lsp);
        let lsp_diagnostics = instant_raw.as_slice();

        if show_welcome && !is_ide_mode {
            return (self.draw_welcome(recent_files, ui_registry), Vec::new());
        }

        let mut wants_pointer = false;

        let layout = self.draw_root_panel_layout(
            editor,
            tabs,
            active_tab,
            ide_panel,
            scroll_x,
            scroll_y,
            cursor_phys_line,
            frame_now,
            markdown_read_active,
            show_settings,
            dialog_window_open,
            is_ide_mode,
        );
        let RootFramePanelLayout {
            s,
            mx,
            my,
            real_height,
            panel_left_w,
            panel_bottom_h,
            modal_overlay_open,
            ui_mx,
            ui_my,
            is_ui_disabled,
            ..
        } = layout;

        let viewport = self.draw_root_editor_viewport_layout(
            editor,
            tabs,
            active_tab,
            markdown,
            markdown_media,
            scroll_y_state,
            current_sticky_lines,
            layout,
            scroll_x,
            scroll_y,
            is_resizing,
            show_welcome,
            is_ide_mode,
            markdown_read_active,
            telemetry_frame_start,
            &mut telemetry_root_phases,
        );
        let RootFrameViewport { tab_bar_h, .. } = viewport;

        let active_api_route = self.draw_root_surface_and_side_panels(
            tabs,
            active_tab,
            ide_panel,
            lsp,
            ui_registry,
            layout,
            has_lsp_diagnostics,
            is_ide_mode,
            blink_alpha,
            telemetry_frame_start,
            &mut telemetry_root_phases,
            &mut telemetry_side_panel_time,
        );
        if is_ide_mode
            && !show_welcome
            && let Some(image) = tabs.get(active_tab).and_then(|tab| tab.image.as_deref())
        {
            let (x, y, w, h) = self.tab_body_rect(s, panel_left_w, tab_bar_h, viewport.editor_height);
            self.draw_root_image_frame(image, x, y, w, h, s, ui_mx, ui_my, ui_registry);
            return self.draw_root_tab_frame_chrome(editor, editor_title, editor_path, tabs, active_tab,
                markdown, None, ide_panel, lsp, ui_registry, ide_workspaces, layout, viewport,
                active_api_route, has_lsp_diagnostics, show_fps, blink_alpha, tab_scroll_x, wants_pointer);
        }
        if is_ide_mode
            && !show_welcome
            && let Some(tab) = tabs.get(active_tab).filter(|tab| tab.kind.is_pdf())
            && let Some(pdf) = tab.pdf.as_deref()
        {
            let (x, y, w, h) = self.tab_body_rect(s, panel_left_w, tab_bar_h, viewport.editor_height);
            self.draw_root_pdf_frame(pdf, pdf_engine, pdf_dark_pages, x, y, w, h, s, ui_mx, ui_my, ui_registry);
            wants_pointer |= self.draw_search_panel(show_search, search_anim_y, search_editor,
                search_focused, search_case_sensitive, pdf.search.matches.len(), !pdf.search.done,
                pdf.search.current, blink_alpha, crate::render_view::search::search_panel_scrollbar_x(
                    self.width, self.minimap_width, 10.0 * s, None), ui_registry);
            return self.draw_root_tab_frame_chrome(editor, editor_title, editor_path, tabs,
                active_tab, markdown,
                Some(crate::app::pdf_tab::PdfStatus { dark: pdf_dark_pages, page: pdf.status_page() }), ide_panel, lsp, ui_registry, ide_workspaces, layout,
                viewport, active_api_route, has_lsp_diagnostics, show_fps, blink_alpha,
                tab_scroll_x, wants_pointer);
        }
        let pre_editor_start = telemetry_frame_start.map(|_| Instant::now());
        if is_ide_mode
            && !show_welcome
            && let Some(crate::app::EditorTabKind::ApiClient(tab_meta, tab_state)) =
                tabs.get(active_tab).map(|tab| &tab.kind)
        {
            return self.draw_root_api_client_frame(
                tab_meta,
                tab_state,
                editor,
                editor_title,
                editor_path,
                tabs,
                active_tab,
                markdown,
                ide_panel,
                lsp,
                ui_registry,
                hover,
                ide_workspaces,
                layout,
                viewport,
                active_api_route,
                has_lsp_diagnostics,
                is_ide_mode,
                show_fps,
                blink_alpha,
                tab_scroll_x,
                wants_pointer,
            );
        }
        if is_ide_mode
            && !show_welcome
            && let Some(crate::app::EditorTabKind::DatabaseTable(tab_meta, tab_state)) =
                tabs.get(active_tab).map(|tab| &tab.kind)
        {
            return self.draw_root_database_table_frame(
                tab_meta,
                tab_state,
                editor,
                editor_title,
                editor_path,
                tabs,
                active_tab,
                markdown,
                ide_panel,
                lsp,
                ui_registry,
                ide_workspaces,
                layout,
                viewport,
                active_api_route,
                has_lsp_diagnostics,
                show_fps,
                blink_alpha,
                tab_scroll_x,
                wants_pointer,
            );
        }

        // IDE с пустыми вкладками — показываем cowsay экран вместо редактора.
        // Открытая нижняя панель завершает empty frame через штатный bottom chrome.
        let empty_ide_bottom_chrome =
            empty_ide_should_continue_bottom_chrome(is_ide_mode, tabs.is_empty(), panel_bottom_h);
        if is_ide_mode && self.startup_editor_hidden {
            return self.draw_empty_ide_frame(
                ide_panel,
                editor,
                lsp,
                ui_registry,
                has_lsp_diagnostics,
                mx,
                my,
                blink_alpha,
                panel_left_w,
                panel_bottom_h,
                true,
                modal_overlay_open,
                s,
                true,
                empty_ide_open_label,
            );
        }
        if is_ide_mode && tabs.is_empty() {
            return self.draw_empty_ide_frame(
                ide_panel,
                editor,
                lsp,
                ui_registry,
                has_lsp_diagnostics,
                mx,
                my,
                blink_alpha,
                panel_left_w,
                panel_bottom_h,
                empty_ide_bottom_chrome,
                modal_overlay_open,
                s,
                false,
                empty_ide_open_label,
            );
        } else {
            self.was_empty_ide = false;
        }

        if markdown_read_active {
            let RootFrameChrome {
                tab_tooltip,
                chrome_start,
                chrome_detail_start,
            } = self.draw_root_markdown_read_frame(
                markdown,
                markdown_media,
                editor,
                editor_title,
                editor_path,
                tabs,
                active_tab,
                scroll_y_state,
                spans,
                search_results,
                search_current_idx,
                search_editor,
                ide_panel,
                lsp,
                ui_registry,
                ide_workspaces,
                layout,
                viewport,
                active_api_route,
                has_lsp_diagnostics,
                show_welcome,
                is_ide_mode,
                show_fps,
                show_search,
                search_anim_y,
                search_focused,
                search_case_sensitive,
                dialog_window_open,
                blink_alpha,
                tab_scroll_x,
                &mut wants_pointer,
                pre_editor_start,
                telemetry_frame_start,
                &mut telemetry_root_phases,
                &mut telemetry_editor_time,
                &mut telemetry_chrome_details,
            );
            let wants_pointer = self.finish_root_overlays_and_telemetry(
                wants_pointer,
                tab_tooltip,
                ide_panel,
                editor,
                ui_registry,
                is_ide_mode,
                panel_left_w,
                panel_bottom_h,
                s,
                mx,
                my,
                ui_mx,
                ui_my,
                blink_alpha,
                show_readonly_notice,
                readonly_notice_text,
                tab_bar_h,
                is_ui_disabled,
                modal_overlay_open,
                real_height,
                chrome_detail_start,
                telemetry_frame_start,
                chrome_start,
                telemetry_was_typing,
                telemetry_was_scrolling,
                telemetry_editor_time,
                telemetry_minimap_time,
                telemetry_side_panel_time,
                telemetry_root_phases,
                telemetry_chrome_details,
            );
            return (wants_pointer, Vec::new());
        }

        let editor_text = self.draw_root_editor_text(
            editor,
            tabs,
            active_tab,
            spans,
            search_results,
            search_current_idx,
            ide_panel,
            ui_registry,
            ctrl_definition_range,
            python_inlay_hints,
            closing_hints,
            layout,
            viewport,
            blink_alpha,
            dialog_window_open,
            search_focused,
            show_settings,
            pre_editor_start,
            telemetry_frame_start,
            &mut telemetry_root_phases,
            &mut telemetry_editor_time,
            inline_git_popup.map(|popup| popup.anchor_line.saturating_sub(1)),
            inline_blame_dwell,
        );

        let overlays = self.draw_root_editor_overlays(
            editor,
            lsp_diagnostics,
            ide_panel,
            ui_registry,
            hover,
            active_tab,
            layout,
            viewport,
            editor_text,
            scroll_x,
            is_ide_mode,
            show_welcome,
            telemetry_frame_start,
            &mut telemetry_root_phases,
        );

        let (
            target_sticky_lines,
            RootFrameChrome {
                tab_tooltip,
                chrome_start,
                mut chrome_detail_start,
            },
        ) = self.draw_root_editor_chrome(
            editor,
            editor_title,
            editor_path,
            tabs,
            active_tab,
            spans,
            current_sticky_lines,
            lsp_diagnostics,
            ide_panel,
            lsp,
            ui_registry,
            ide_workspaces,
            layout,
            viewport,
            editor_text,
            overlays,
            tab_scroll_x,
            sticky_anim_progress,
            sticky_anim_is_adding,
            is_resizing,
            show_welcome,
            is_ide_mode,
            show_fps,
            dialog_window_open,
            telemetry_frame_start,
            &mut telemetry_minimap_time,
            &mut telemetry_chrome_details,
        );

        self.draw_root_editor_panels_and_hover(
            editor,
            editor_path,
            tabs,
            active_tab,
            markdown,
            search_results,
            search_current_idx,
            search_editor,
            lsp_diagnostics,
            ide_panel,
            lsp,
            ui_registry,
            hover,
            inline_git_popup,
            layout,
            viewport,
            editor_text,
            overlays,
            active_api_route,
            has_lsp_diagnostics,
            scroll_x,
            show_search,
            search_anim_y,
            search_focused,
            search_case_sensitive,
            show_welcome,
            is_ide_mode,
            dialog_window_open,
            blink_alpha,
            &mut wants_pointer,
            &mut chrome_detail_start,
            &mut telemetry_chrome_details,
        );

        let wants_pointer = self.finish_root_overlays_and_telemetry(
            wants_pointer,
            tab_tooltip,
            ide_panel,
            editor,
            ui_registry,
            is_ide_mode,
            panel_left_w,
            panel_bottom_h,
            s,
            mx,
            my,
            ui_mx,
            ui_my,
            blink_alpha,
            show_readonly_notice,
            readonly_notice_text,
            tab_bar_h,
            is_ui_disabled,
            modal_overlay_open,
            real_height,
            chrome_detail_start,
            telemetry_frame_start,
            chrome_start,
            telemetry_was_typing,
            telemetry_was_scrolling,
            telemetry_editor_time,
            telemetry_minimap_time,
            telemetry_side_panel_time,
            telemetry_root_phases,
            telemetry_chrome_details,
        );

        (wants_pointer, target_sticky_lines)
    }
}

include!("root_frame_layout_renderer.rs");
include!("root_frame_content_frames_renderer.rs");
include!("root_frame_editor_text_renderer.rs");
include!("root_frame_editor_chrome_renderer.rs");

#[cfg(test)]
include!("markdown_scroll_transition_review_tests.rs");
