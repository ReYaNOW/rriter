#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_ide_context_overlays(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
        panel_left_w: f32,
        s: f32,
    ) -> bool {
        let mut wants_pointer =
            self.draw_file_tree_overlays(ide_panel, ui_registry, mx, my, blink_alpha);
        wants_pointer |=
            self.draw_git_dropdown_overlays(ide_panel, ui_registry, mx, my, panel_left_w, s);
        wants_pointer
    }

    fn draw_ide_modal_overlays(
        &mut self,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) -> bool {
        let mut drew = false;
        let api = &ide_panel.api;
        if api.mock_python_runtime_open {
            self.draw_api_mock_python_overlay(s, api, ui_registry, mx, my, blink_alpha);
            drew = true;
        }
        if api.mock_guide_open {
            self.draw_api_mock_guide_overlay(0.0, 0.0, self.width, self.height, s, api, ui_registry, mx, my);
            drew = true;
        }
        if api.mock_server_detail_open {
            self.draw_api_mock_server_detail_overlay(
                0.0,
                0.0,
                self.width,
                self.height,
                s,
                api,
                ui_registry,
                mx,
                my,
            );
            drew = true;
        }
        if ide_panel.project_search.help_open {
            self.draw_project_search_help_overlay(ide_panel, ui_registry, mx, my, s);
            drew = true;
        }
        if self.draw_database_overlays(s, ide_panel, editor, ui_registry, mx, my, blink_alpha) {
            drew = true;
        }
        drew
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_empty_ide_frame(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        editor: &crate::editor::Editor,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        lsp_has_diagnostics: bool,
        mx: f32,
        my: f32,
        blink_alpha: f32,
        panel_left_w: f32,
        panel_bottom_h: f32,
        continue_bottom_chrome: bool,
        modal_overlay_open: bool,
        s: f32,
    ) -> (bool, Vec<(usize, usize)>) {
        self.draw_empty_ide(panel_left_w);

        if continue_bottom_chrome {
            let (ui_mx, ui_my) = if modal_overlay_open {
                (-1.0, -1.0)
            } else {
                (mx, my)
            };
            let is_ui_disabled = ide_panel.terminal_focused;
            self.draw_ide_bottom_panel(
                ide_panel,
                lsp,
                ui_registry,
                lsp_has_diagnostics,
                s,
                ui_mx,
                ui_my,
                panel_bottom_h,
                is_ui_disabled,
                blink_alpha,
                None,
            );
            self.draw_status_bar(
                editor,
                None,
                crate::app::MarkdownMode::Edit,
                lsp,
                ui_registry,
                s,
                ui_mx,
                ui_my,
                panel_bottom_h,
                None,
                None,
                None,
            );
        }

        if self.draw_ide_modal_overlays(s, ide_panel, editor, ui_registry, mx, my, blink_alpha) {
            self.flush();
            return (ui_registry.wants_pointer(), Vec::new());
        }
        if should_draw_empty_ide_file_tree_overlay(
            true,
            true,
            crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel),
        ) || crate::render_view::ide_panels::git_dropdown_overlay_active_for_panel(ide_panel)
        {
            let wants_pointer = self.draw_ide_context_overlays(
                ide_panel,
                ui_registry,
                mx,
                my,
                blink_alpha,
                panel_left_w,
                s,
            );
            self.flush();
            return (wants_pointer | ui_registry.wants_pointer(), Vec::new());
        }
        if continue_bottom_chrome {
            self.flush();
            self.register_root_resize_blockers(
                ide_panel,
                ui_registry,
                s,
                mx,
                my,
                panel_left_w,
                panel_bottom_h,
                ide_panel.terminal_focused,
                modal_overlay_open,
                self.height,
            );
            return (ui_registry.wants_pointer(), Vec::new());
        }
        (false, Vec::new())
    }

    fn draw_ide_welcome_bounce(
        &mut self,
        gutter_x: f32,
        panel_bottom_h: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let anim_w = self.width - gutter_x;
        let anim_h = self.height - panel_bottom_h;
        self.push_rect(gutter_x, 0.0, anim_w, anim_h, [0.173, 0.180, 0.224, 1.0]);

        ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            gutter_x,
            0.0,
            anim_w,
            anim_h,
            mx,
            my,
        );

        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f32();
        let text = "RRiter";
        let sub_text = "Нет открытых файлов";
        let scale_t = 3.0;
        let scale_sub = 1.2;
        let text_w = self
            .measure_ui_width(text, scale_t)
            .max(self.measure_ui_width(sub_text, scale_sub));
        let text_h = 70.0 * s;

        let eff_w = (anim_w - text_w).max(1.0);
        let eff_h = (anim_h - text_h).max(1.0);
        let px = (t * 100.0 * s) % (eff_w * 2.0);
        let rx = if px < eff_w { px } else { eff_w * 2.0 - px };
        let py = (t * 75.0 * s) % (eff_h * 2.0);
        let ry = if py < eff_h { py } else { eff_h * 2.0 - py };

        let r = (t * 2.0).sin() * 0.2 + 0.6;
        let g = (t * 3.0).sin() * 0.2 + 0.6;
        let b = (t * 5.0).sin() * 0.2 + 0.8;

        let draw_x = gutter_x + rx;
        let draw_y = ry + 40.0 * s;
        self.draw_string_scaled(text, draw_x, draw_y, [r, g, b, 1.0], scale_t);
        self.draw_string_scaled(
            sub_text,
            draw_x,
            draw_y + 30.0 * s,
            [0.5, 0.5, 0.6, 1.0],
            scale_sub,
        );
        self.flush();
    }

    fn draw_editor_horizontal_scrollbar(
        &mut self,
        render_scroll_x: f32,
        scrollbar_x: f32,
        editor_bottom_h: f32,
        is_ide_mode: bool,
        real_height: f32,
        s: f32,
    ) {
        if self.max_scroll_x <= 0.0 {
            return;
        }
        let track_w = scrollbar_x - self.left_padding;
        let track_h_bg = 14.0 * s;
        let status_bar_h = if is_ide_mode {
            ide_status_bar_height(s)
        } else {
            0.0
        };
        let track_y_bg = real_height - editor_bottom_h - status_bar_h - track_h_bg;

        self.push_rect(
            self.left_padding,
            track_y_bg,
            track_w,
            track_h_bg,
            [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
        );

        let thumb_w = (track_w / (self.max_scroll_x + track_w).max(1.0) * track_w).max(40.0 * s).min(track_w.max(0.0));
        let scroll_ratio_x = (render_scroll_x / self.max_scroll_x).clamp(0.0, 1.0);
        let thumb_x = self.left_padding + scroll_ratio_x * (track_w - thumb_w);

        let thumb_h = 6.0 * s;
        let thumb_y = track_y_bg + (track_h_bg - thumb_h) / 2.0;

        self.push_rounded_rect(
            thumb_x,
            thumb_y,
            thumb_w,
            thumb_h,
            3.0 * s,
            [0.7, 0.33, 0.54, 1.0],
        );
    }

    fn register_root_resize_blockers(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        s: f32,
        mx: f32,
        my: f32,
        panel_left_w: f32,
        panel_bottom_h: f32,
        is_ui_disabled: bool,
        modal_overlay_open: bool,
        real_height: f32,
    ) {
        if panel_left_w > 0.0 && !is_ui_disabled && !modal_overlay_open {
            let resize_x = 48.0 * s + panel_left_w;
            let resize_hit = 3.0 * s;
            let resize_h = if panel_bottom_h > 0.0 && ide_panel.bottom_panel_blocks_editor_hover()
            {
                ide_bottom_panel_y(self.height, panel_bottom_h, s)
            } else {
                real_height
            };
            ui_registry.register_blocker(
                crate::ui_system::UiId::ResizeLeft,
                resize_x - resize_hit,
                0.0,
                resize_hit * 2.0,
                resize_h,
                mx,
                my,
            );
        }
        if panel_bottom_h > 0.0 && !is_ui_disabled && !modal_overlay_open {
            let panel_y = ide_bottom_panel_y(self.height, panel_bottom_h, s);
            ui_registry.register_blocker(
                crate::ui_system::UiId::ResizeBottom,
                48.0 * s,
                panel_y - 8.0 * s,
                self.width - 48.0 * s,
                16.0 * s,
                mx,
                my,
            );
        }
    }

    fn draw_readonly_notice(&mut self, tab_bar_h: f32, s: f32) {
        let text = "Файл открыт в режиме только чтение";
        let text_w = self.measure_ui_width(text, 1.0);
        let pad_x = 16.0 * s;
        let w = text_w + pad_x * 2.0;
        let h = 32.0 * s;
        let x = ((self.width - w) * 0.5).max(8.0 * s).round();
        let y = (tab_bar_h + 10.0 * s).round();
        self.push_rounded_rect(x, y, w, h, 6.0 * s, [0.10, 0.11, 0.14, 0.94]);
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            6.0 * s,
            (1.0 * s).max(1.0),
            [1.0, 1.0, 1.0, 0.16],
            [0.10, 0.11, 0.14, 0.94],
        );
        self.draw_string_scaled(text, x + pad_x, y + h * 0.5 + 5.0 * s, self.theme.fg, 1.0);
    }

    fn register_editor_horizontal_scrollbar(
        &mut self,
        ui_registry: &mut crate::ui_system::UiRegistry,
        scrollbar_x: f32,
        editor_bottom_h: f32,
        is_ide_mode: bool,
        real_height: f32,
        s: f32,
    ) {
        if self.max_scroll_x <= 0.0 {
            return;
        }
        let track_w = scrollbar_x - self.left_padding;
        let status_bar_h = if is_ide_mode {
            ide_status_bar_height(s)
        } else {
            0.0
        };
        ui_registry.register_rect(
            crate::ui_system::UiId::EditorScrollbarX,
            self.left_padding,
            real_height - editor_bottom_h - status_bar_h - 14.0 * s,
            track_w,
            14.0 * s,
            self.last_mouse_x,
            self.last_mouse_y,
        );
    }

    fn draw_search_panel_if_visible(
        &mut self,
        show_search: bool,
        search_anim_y: f32,
        search_editor: &Editor,
        search_focused: bool,
        search_case_sensitive: bool,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        blink_alpha: f32,
        scrollbar_width: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> bool {
        if search_anim_y <= -100.0 * self.scale_factor {
            return false;
        }
        self.draw_search_panel(
            show_search,
            search_anim_y,
            search_editor,
            search_focused,
            search_case_sensitive,
            search_results,
            search_current_idx,
            blink_alpha,
            scrollbar_width,
            ui_registry,
        )
    }

    fn draw_dialog_dim_if_open(&mut self, dialog_window_open: bool) {
        if dialog_window_open {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.6]);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_root_tab_chrome(
        &mut self,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        editor: &crate::editor::Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        show_welcome: bool,
        is_ide_mode: bool,
        gutter_x: f32,
        panel_bottom_h: f32,
        tab_bar_visual_h: f32,
        s: f32,
        ui_mx: f32,
        ui_my: f32,
        mx: f32,
        my: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        tab_scroll_x: f32,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ide_workspaces: &[std::path::PathBuf],
    ) -> Option<(String, f32, f32)> {
        if show_welcome && is_ide_mode {
            self.draw_ide_welcome_bounce(gutter_x, panel_bottom_h, ui_registry, mx, my, s);
            return None;
        }
        if !show_welcome && is_ide_mode {
            let tab_x = gutter_x.round() + 1.0;
            let tab_w = self.width - tab_x;
            let tooltip = self.draw_tab_bar(
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
            self.flush();
            return tooltip;
        }
        None
    }

    fn draw_root_fps_if_visible(&mut self, show_fps: bool, minimap_w: f32) {
        if show_fps {
            self.draw_fps_overlay(minimap_w);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_root_bottom_status_and_dim(
        &mut self,
        editor: &crate::editor::Editor,
        editor_path: Option<&std::path::PathBuf>,
        markdown_mode: crate::app::MarkdownMode,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        has_lsp_diagnostics: bool,
        s: f32,
        ui_mx: f32,
        ui_my: f32,
        panel_bottom_h: f32,
        is_ui_disabled: bool,
        blink_alpha: f32,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
        is_ide_mode: bool,
        status_progress_label: Option<&str>,
        status_progress_elapsed: Option<f32>,
        status_progress_value: Option<f32>,
        dialog_window_open: bool,
    ) {
        if is_ide_mode && panel_bottom_h > 0.0 {
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
                editor_path.zip(tabs.get(active_tab).map(|tab| tab.text_file_format.encoding)),
                markdown_mode,
                lsp,
                ui_registry,
                s,
                ui_mx,
                ui_my,
                panel_bottom_h,
                status_progress_label,
                status_progress_elapsed,
                status_progress_value,
            );
        }
        self.draw_dialog_dim_if_open(dialog_window_open);
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_root_overlays_and_telemetry(
        &mut self,
        mut wants_pointer: bool,
        tab_tooltip: Option<(String, f32, f32)>,
        ide_panel: &crate::app::IdePanelState,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        is_ide_mode: bool,
        panel_left_w: f32,
        panel_bottom_h: f32,
        s: f32,
        mx: f32,
        my: f32,
        ui_mx: f32,
        ui_my: f32,
        blink_alpha: f32,
        show_readonly_notice: bool,
        tab_bar_h: f32,
        is_ui_disabled: bool,
        modal_overlay_open: bool,
        real_height: f32,
        mut chrome_detail_start: Option<Instant>,
        telemetry_frame_start: Option<Instant>,
        chrome_start: Option<Instant>,
        telemetry_was_typing: Option<bool>,
        telemetry_was_scrolling: Option<bool>,
        telemetry_editor_time: f32,
        telemetry_minimap_time: f32,
        telemetry_side_panel_time: f32,
        mut telemetry_root_phases: [f32; 5],
        mut telemetry_chrome_details: [f32; 6],
    ) -> bool {
        if let Some((path, tx, ty)) = tab_tooltip {
            self.draw_tab_tooltip(&path, tx, ty, s);
        }

        let mouse_in_blocking_bottom_panel = is_ide_mode
            && panel_bottom_h > 0.0
            && ide_panel.bottom_panel_blocks_editor_hover()
            && my >= ide_bottom_panel_y(self.height, panel_bottom_h, s)
            && my <= ide_bottom_panel_y(self.height, panel_bottom_h, s) + panel_bottom_h;

        if is_ide_mode {
            let overlay_mx = if mouse_in_blocking_bottom_panel { -1.0 } else { mx };
            let overlay_my = if mouse_in_blocking_bottom_panel { -1.0 } else { my };
            wants_pointer |= self.draw_ide_context_overlays(
                ide_panel,
                ui_registry,
                overlay_mx,
                overlay_my,
                blink_alpha,
                panel_left_w,
                s,
            );
            self.draw_ide_modal_overlays(s, ide_panel, editor, ui_registry, mx, my, blink_alpha);
        }

        if is_ide_mode {
            self.draw_git_file_tooltip_overlay(s, ide_panel, ui_registry, ui_mx, ui_my);
        } else {
            self.reset_git_file_tooltip_overlay();
        }

        if show_readonly_notice {
            self.draw_readonly_notice(tab_bar_h, s);
        }

        if let Some(start) = chrome_detail_start.replace(Instant::now()) {
            telemetry_chrome_details[4] = start.elapsed().as_secs_f32();
        }
        self.flush();
        if let Some(start) = chrome_detail_start {
            telemetry_chrome_details[5] = start.elapsed().as_secs_f32();
        }

        if is_ide_mode {
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
        }

        self.finalize_root_frame_telemetry(
            telemetry_frame_start,
            chrome_start,
            telemetry_was_typing,
            telemetry_was_scrolling,
            telemetry_editor_time,
            telemetry_minimap_time,
            telemetry_side_panel_time,
            &mut telemetry_root_phases,
            telemetry_chrome_details,
        );
        wants_pointer | ui_registry.wants_pointer()
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize_root_frame_telemetry(
        &mut self,
        telemetry_frame_start: Option<Instant>,
        chrome_start: Option<Instant>,
        telemetry_was_typing: Option<bool>,
        telemetry_was_scrolling: Option<bool>,
        telemetry_editor_time: f32,
        telemetry_minimap_time: f32,
        telemetry_side_panel_time: f32,
        telemetry_root_phases: &mut [f32; 5],
        telemetry_chrome_details: [f32; 6],
    ) {
        if let Some(frame_start_time) = telemetry_frame_start {
            let elapsed = frame_start_time.elapsed().as_secs_f32();
            if let Some(chrome_start) = chrome_start {
                telemetry_root_phases[4] = chrome_start.elapsed().as_secs_f32();
            }
            TELEMETRY.with(|t| {
                let mut t = t.borrow_mut();
                if telemetry_was_typing.unwrap_or(false) {
                    t.type_time += elapsed;
                    t.type_count += 1;
                } else if telemetry_was_scrolling.unwrap_or(false) {
                    t.scroll_time += elapsed;
                    t.scroll_count += 1;
                } else {
                    t.render_time += elapsed;
                    t.render_count += 1;
                }
                t.editor_time += telemetry_editor_time;
                t.editor_count += u32::from(telemetry_editor_time > 0.0);
                t.minimap_time += telemetry_minimap_time;
                t.minimap_count += u32::from(telemetry_minimap_time > 0.0);
                t.side_panel_time += telemetry_side_panel_time;
                t.side_panel_count += u32::from(telemetry_side_panel_time > 0.0);
                t.root_other_time += (elapsed
                    - telemetry_editor_time
                    - telemetry_minimap_time
                    - telemetry_side_panel_time)
                    .max(0.0);
                t.root_other_count += 1;
                for (index, elapsed) in telemetry_root_phases.iter().copied().enumerate() {
                    t.root_phase_time[index] += elapsed;
                    t.root_phase_count[index] += 1;
                }
                for (index, elapsed) in telemetry_chrome_details.into_iter().enumerate() {
                    t.chrome_detail_time[index] += elapsed;
                    t.chrome_detail_count[index] += 1;
                }

                if t.last_print.elapsed().as_secs() >= 10 {
                    let r_avg = if t.render_count > 0 {
                        (t.render_time / t.render_count as f32) * 1000.0
                    } else {
                        0.0
                    };
                    let s_avg = if t.scroll_count > 0 {
                        (t.scroll_time / t.scroll_count as f32) * 1000.0
                    } else {
                        0.0
                    };
                    let ty_avg = if t.type_count > 0 {
                        (t.type_time / t.type_count as f32) * 1000.0
                    } else {
                        0.0
                    };
                    println!(
                        "📊 Telemetry (10s): Idle Render {:.2}ms | Scroll {:.2}ms | Type {:.2}ms",
                        r_avg, s_avg, ty_avg
                    );
                    let stage_avg = |time: f32, count: u32| {
                        if count > 0 { time / count as f32 * 1000.0 } else { 0.0 }
                    };
                    println!(
                        "📊 Frame split: Editor {:.2}ms | Minimap {:.2}ms | Side {:.2}ms | Swap {:.2}ms",
                        stage_avg(t.editor_time, t.editor_count),
                        stage_avg(t.minimap_time, t.minimap_count),
                        stage_avg(t.side_panel_time, t.side_panel_count),
                        stage_avg(t.swap_time, t.swap_count),
                    );
                    println!(
                        "📊 Root other: {:.2}ms",
                        stage_avg(t.root_other_time, t.root_other_count),
                    );
                    println!(
                        "📊 Root phases: Prep {:.2}ms | Cache {:.2}ms | Pre-editor {:.2}ms | Overlays {:.2}ms | Chrome {:.2}ms",
                        stage_avg(t.root_phase_time[0], t.root_phase_count[0]),
                        stage_avg(t.root_phase_time[1], t.root_phase_count[1]),
                        stage_avg(t.root_phase_time[2], t.root_phase_count[2]),
                        stage_avg(t.root_phase_time[3], t.root_phase_count[3]),
                        stage_avg(t.root_phase_time[4], t.root_phase_count[4]),
                    );
                    let measured_frames = t.render_count + t.scroll_count + t.type_count;
                    println!(
                        "📊 Flush: Avg {:.3}ms | Max {:.2}ms | Calls/frame {:.1} | Vertices/frame {:.0}",
                        stage_avg(t.flush_time, t.flush_count),
                        t.flush_max_time * 1000.0,
                        t.flush_count as f32 / measured_frames.max(1) as f32,
                        t.flush_vertices as f32 / measured_frames.max(1) as f32,
                    );
                    println!(
                        "📊 Chrome detail: Tabs {:.2}ms | Sticky-scroll {:.2}ms | Popups {:.2}ms | Bottom-status {:.2}ms | Overlays {:.2}ms | Final-flush {:.2}ms",
                        stage_avg(t.chrome_detail_time[0], t.chrome_detail_count[0]),
                        stage_avg(t.chrome_detail_time[1], t.chrome_detail_count[1]),
                        stage_avg(t.chrome_detail_time[2], t.chrome_detail_count[2]),
                        stage_avg(t.chrome_detail_time[3], t.chrome_detail_count[3]),
                        stage_avg(t.chrome_detail_time[4], t.chrome_detail_count[4]),
                        stage_avg(t.chrome_detail_time[5], t.chrome_detail_count[5]),
                    );
                    let present_fps = if t.scroll_present_interval_time > 0.0 {
                        t.scroll_present_interval_count as f32 / t.scroll_present_interval_time
                    } else {
                        0.0
                    };
                    println!(
                        "📊 Scroll present: {:.0} FPS | Avg gap {:.2}ms | Max gap {:.2}ms | Frames {}",
                        present_fps,
                        stage_avg(
                            t.scroll_present_interval_time,
                            t.scroll_present_interval_count,
                        ),
                        t.max_scroll_present_interval * 1000.0,
                        t.scroll_present_interval_count,
                    );

                    t.render_time = 0.0;
                    t.render_count = 0;
                    t.scroll_time = 0.0;
                    t.scroll_count = 0;
                    t.type_time = 0.0;
                    t.type_count = 0;
                    t.editor_time = 0.0;
                    t.editor_count = 0;
                    t.minimap_time = 0.0;
                    t.minimap_count = 0;
                    t.side_panel_time = 0.0;
                    t.side_panel_count = 0;
                    t.swap_time = 0.0;
                    t.swap_count = 0;
                    t.scroll_present_interval_time = 0.0;
                    t.scroll_present_interval_count = 0;
                    t.max_scroll_present_interval = 0.0;
                    t.root_other_time = 0.0;
                    t.root_other_count = 0;
                    t.root_phase_time = [0.0; 5];
                    t.root_phase_count = [0; 5];
                    t.flush_time = 0.0;
                    t.flush_count = 0;
                    t.flush_max_time = 0.0;
                    t.flush_vertices = 0;
                    t.chrome_detail_time = [0.0; 6];
                    t.chrome_detail_count = [0; 6];
                    t.last_print = Instant::now();
                }
            });
        }
    }
}
