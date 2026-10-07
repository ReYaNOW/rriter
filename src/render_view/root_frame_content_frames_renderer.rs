/// Tab tooltip and chrome telemetry clocks handed from the Markdown Read and editor
/// paths of `Renderer::draw` to `finish_root_overlays_and_telemetry`.
struct RootFrameChrome {
    tab_tooltip: Option<(String, f32, f32)>,
    chrome_start: Option<Instant>,
    chrome_detail_start: Option<Instant>,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn tab_body_rect(
        &self,
        scale: f32,
        panel_left_w: f32,
        tab_bar_h: f32,
        editor_height: f32,
    ) -> (f32, f32, f32, f32) {
        let x = (48.0 * scale + panel_left_w).round() + 1.0;
        (x, tab_bar_h, (self.width - x).max(0.0), editor_height.max(0.0))
    }
}

impl Renderer {
    fn draw_root_api_client_frame(
        &mut self,
        tab_meta: &crate::app::api_client::ApiClientTabMeta,
        tab_state: &crate::app::api_client::ApiClientTabState,
        editor: &Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        markdown: &crate::app::MarkdownTabState,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover: &mut crate::app::mouse::HoverState,
        ide_workspaces: &[std::path::PathBuf],
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
        has_lsp_diagnostics: bool,
        is_ide_mode: bool,
        show_fps: bool,
        blink_alpha: f32,
        tab_scroll_x: f32,
        mut wants_pointer: bool,
    ) -> (bool, Vec<(usize, usize)>) {
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
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            git_blame_inline,
            is_ui_disabled,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_visual_h,
            tab_bar_h,
            editor_height,
            ..
        } = viewport;
        let gutter_x = 48.0 * s + panel_left_w;
        let tab_x = gutter_x.round() + 1.0;
        let tab_w = self.width - tab_x;
        self.draw_api_client_tab(
            tab_x,
            tab_bar_h,
            tab_w,
            editor_height,
            s,
            editor,
            ide_panel,
            tab_meta,
            tab_state,
            ui_registry,
            hover,
            ui_mx,
            ui_my,
            blink_alpha,
        );
        let tab_tooltip = self.draw_tab_bar(
            tabs,
            active_tab,
            editor,
            editor_title,
            editor_path,
            tab_x,
            0.0,
            tab_w,
            tab_bar_visual_h,
            s,
            ui_mx,
            ui_my,
            ui_registry,
            tab_scroll_x,
            ide_panel.tab_drag.as_ref(),
            lsp,
            &ide_panel.api,
            ide_workspaces,
        );
        if panel_bottom_h > 0.0 {
            self.draw_ide_bottom_panel(
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
            );
        }
        if is_ide_mode {
            self.draw_status_bar(
                editor,
                None,
                markdown.mode,
                None,
                lsp,
                ui_registry,
                s,
                ui_mx,
                ui_my,
                panel_bottom_h,
                status_progress_label,
                status_progress_elapsed,
                status_progress_value,
                git_blame_inline,
            );
        }
        if let Some((path, tx, ty)) = tab_tooltip {
            self.draw_tab_tooltip(&path, tx, ty, s);
        }
        if show_fps {
            self.draw_fps_overlay(self.minimap_width);
        }
        wants_pointer |= self.draw_root_ide_final_overlays(
            ide_panel,
            editor,
            ui_registry,
            true,
            panel_left_w,
            panel_bottom_h,
            s,
            mx,
            my,
            ui_mx,
            ui_my,
            blink_alpha,
            modal_overlay_open,
        );
        self.flush();
        self.register_root_resize_blockers(
            ide_panel,
            ui_registry,
            s,
            mx,
            my,
            panel_left_w,
            panel_bottom_h,
            is_ui_disabled,
            modal_overlay_open,
            real_height,
        );
        (wants_pointer | ui_registry.wants_pointer(), Vec::new())
    }

    fn draw_root_database_table_frame(
        &mut self,
        tab_meta: &crate::app::database::DatabaseTableTabMeta,
        tab_state: &crate::app::database::DatabaseTableTabState,
        editor: &Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        markdown: &crate::app::MarkdownTabState,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        ide_workspaces: &[std::path::PathBuf],
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
        has_lsp_diagnostics: bool,
        show_fps: bool,
        blink_alpha: f32,
        tab_scroll_x: f32,
        wants_pointer: bool,
    ) -> (bool, Vec<(usize, usize)>) {
        let s = layout.s;
        let (tab_x, tab_y, tab_w, tab_h) =
            self.tab_body_rect(s, layout.panel_left_w, viewport.tab_bar_h, viewport.editor_height);
        self.draw_database_table_tab(
            tab_x,
            tab_y,
            tab_w,
            tab_h,
            s,
            tab_meta,
            tab_state,
            ui_registry,
            layout.ui_mx,
            layout.ui_my,
            blink_alpha,
        );
        self.draw_root_tab_frame_chrome(
            editor,
            editor_title,
            editor_path,
            tabs,
            active_tab,
            markdown,
            None,
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
        )
    }

    /// Tab bar, bottom panel, status bar and the frame tail (tab tooltip, FPS, final
    /// overlays, flush, resize blockers) around a non-editor tab body. Shared by the
    /// Database table and PDF frames, which draw only their body before calling it.
    fn draw_root_tab_frame_chrome(
        &mut self,
        editor: &Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        markdown: &crate::app::MarkdownTabState,
        pdf_status: Option<crate::app::pdf_tab::PdfStatus>,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        ide_workspaces: &[std::path::PathBuf],
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
        has_lsp_diagnostics: bool,
        show_fps: bool,
        blink_alpha: f32,
        tab_scroll_x: f32,
        mut wants_pointer: bool,
    ) -> (bool, Vec<(usize, usize)>) {
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
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            git_blame_inline,
            is_ui_disabled,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_visual_h,
            tab_bar_h,
            editor_height,
            ..
        } = viewport;
        let (tab_x, _, tab_w, _) = self.tab_body_rect(s, panel_left_w, tab_bar_h, editor_height);
        let tab_tooltip = self.draw_tab_bar(
            tabs,
            active_tab,
            editor,
            editor_title,
            editor_path,
            tab_x,
            0.0,
            tab_w,
            tab_bar_visual_h,
            s,
            ui_mx,
            ui_my,
            ui_registry,
            tab_scroll_x,
            ide_panel.tab_drag.as_ref(),
            lsp,
            &ide_panel.api,
            ide_workspaces,
        );
        if panel_bottom_h > 0.0 {
            self.draw_ide_bottom_panel(
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
            );
        }
        self.draw_status_bar_with_cursor_position(
            editor,
            None,
            markdown.mode,
            pdf_status,
            !tabs.get(active_tab).is_some_and(|tab| tab.kind.is_image()),
            lsp,
            ui_registry,
            s,
            ui_mx,
            ui_my,
            panel_bottom_h,
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            git_blame_inline,
        );
        if let Some(image) = tabs.get(active_tab).and_then(|tab| tab.image.as_deref()) {
            let (_, _, body_w, body_h) = self.tab_body_rect(s, panel_left_w, tab_bar_h, editor_height);
            let zoom = if image.zoom <= 0.0 { image.fit_scale(body_w, body_h) } else { image.zoom };
            let mut label = std::mem::take(&mut self.scratch_buffer);
            label.clear();
            let _ = std::fmt::Write::write_fmt(&mut label, format_args!("{:.0}×{:.0} · {:.0}%", image.natural.0, image.natural.1, zoom * 100.0));
            let label_w = self.measure_ui_width(&label, 0.95).round();
            let bar_h = crate::render_view::ide_status_bar_height(s).round();
            let bar_y = crate::render_view::ide_status_bar_y(self.height, panel_bottom_h, s).round();
            let label_x = (self.width - 10.0 * s - label_w).round();
            let overlay_x = (label_x - 6.0 * s).round();
            self.push_rect(overlay_x, bar_y, (self.width - overlay_x).max(0.0), bar_h, self.ui.pick(crate::theme::UiRole::BgPanelAlt, [0.118, 0.125, 0.165, 1.0]));
            self.draw_string_scaled_stable(&label, label_x, (bar_y + bar_h * 0.5 + (5.0 * s).round()).round(), self.ui.pick(crate::theme::UiRole::TextPrimary, self.ui_theme.fg), 0.95);
            self.scratch_buffer = label;
        }
        if let Some((path, tx, ty)) = tab_tooltip {
            self.draw_tab_tooltip(&path, tx, ty, s);
        }
        if show_fps {
            self.draw_fps_overlay(self.minimap_width);
        }
        wants_pointer |= self.draw_root_ide_final_overlays(
            ide_panel,
            editor,
            ui_registry,
            true,
            panel_left_w,
            panel_bottom_h,
            s,
            mx,
            my,
            ui_mx,
            ui_my,
            blink_alpha,
            modal_overlay_open,
        );
        self.flush();
        self.register_root_resize_blockers(
            ide_panel,
            ui_registry,
            s,
            mx,
            my,
            panel_left_w,
            panel_bottom_h,
            is_ui_disabled,
            modal_overlay_open,
            real_height,
        );
        (wants_pointer | ui_registry.wants_pointer(), Vec::new())
    }

    fn draw_root_markdown_read_frame(
        &mut self,
        markdown: &mut crate::app::MarkdownTabState,
        markdown_media: &crate::markdown_media::MarkdownMedia,
        editor: &Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        scroll_y_state: &mut crate::scroll::ScrollState,
        spans: &[ColorSpan],
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        search_editor: &Editor,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        ide_workspaces: &[std::path::PathBuf],
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
        has_lsp_diagnostics: bool,
        show_welcome: bool,
        is_ide_mode: bool,
        show_fps: bool,
        show_search: bool,
        search_anim_y: f32,
        search_focused: bool,
        search_case_sensitive: bool,
        dialog_window_open: bool,
        blink_alpha: f32,
        tab_scroll_x: f32,
        wants_pointer: &mut bool,
        pre_editor_start: Option<Instant>,
        telemetry_frame_start: Option<Instant>,
        telemetry_root_phases: &mut [f32; 5],
        telemetry_editor_time: &mut f32,
        telemetry_chrome_details: &mut [f32; 6],
    ) -> RootFrameChrome {
        let RootFramePanelLayout {
            s,
            mx,
            my,
            panel_left_w,
            panel_bottom_h,
            ui_mx,
            ui_my,
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            git_blame_inline,
            is_ui_disabled,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_visual_h,
            tab_bar_h,
            editor_height,
            ..
        } = viewport;
        if let Some(pre_editor_start) = pre_editor_start {
            telemetry_root_phases[2] = pre_editor_start.elapsed().as_secs_f32();
        }
        let gutter_x = if is_ide_mode {
            48.0 * s + panel_left_w
        } else {
            0.0
        };
        let content_x = markdown_read::markdown_read_frame_x_for_editor_text(
            self.left_padding,
            s,
        );
        let stage_start = telemetry_frame_start.map(|_| Instant::now());
        self.draw_markdown_read(
            markdown,
            markdown_media,
            editor,
            scroll_y_state,
            spans,
            search_results,
            search_current_idx,
            content_x,
            tab_bar_h,
            (self.width - content_x).max(0.0),
            editor_height,
            ui_registry,
        );
        if let Some(stage_start) = stage_start {
            *telemetry_editor_time = stage_start.elapsed().as_secs_f32();
        }

        let chrome_start = telemetry_frame_start.map(|_| Instant::now());
        let mut chrome_detail_start = chrome_start;
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
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[0] = start.elapsed().as_secs_f32();
        }
        self.draw_root_fps_if_visible(show_fps, 0.0);
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[1] = start.elapsed().as_secs_f32();
        }
        let read_scrollbar_w = markdown_read::markdown_read_scrollbar_width(
            markdown.read_scroll_bounds().unwrap_or(0.0),
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
            crate::render_view::search::search_panel_scrollbar_x(
                self.width,
                self.minimap_width,
                0.0,
                Some(read_scrollbar_w),
            ),
            ui_registry,
        );
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[2] = start.elapsed().as_secs_f32();
        }
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
            git_blame_inline,
            is_ide_mode,
            status_progress_label,
            status_progress_elapsed,
            status_progress_value,
            dialog_window_open,
        );
        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[3] = start.elapsed().as_secs_f32();
        }
        RootFrameChrome {
            tab_tooltip,
            chrome_start,
            chrome_detail_start,
        }
    }
}
