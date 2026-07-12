#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_tree_label_clipped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        color: [f32; 4],
        scale: f32,
        scratch: &mut String,
    ) -> f32 {
        if max_w <= 0.0 {
            return 0.0;
        }
        let full_w = self.measure_ui_width(text, scale);
        if full_w <= max_w {
            self.draw_string_scaled_stable(text, x, y, color, scale);
            return full_w;
        }

        let ellipsis = "…";
        let ellipsis_w = self.measure_ui_width(ellipsis, scale);
        if ellipsis_w > max_w {
            return 0.0;
        }
        let prefix_len = clipped_label_prefix_len(text, max_w, ellipsis_w, |ch| {
            self.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(0.0)
        });
        scratch.clear();
        scratch.push_str(&text[..prefix_len]);
        scratch.push_str(ellipsis);
        self.draw_string_scaled_stable(scratch, x, y, color, scale);
        self.measure_ui_width(scratch, scale).min(max_w)
    }

    fn draw_git_graph_row_text(&mut self, text: &str, x: f32, y: f32, color: [f32; 4], scale: f32) {
        let mut draw_x = x.round();
        let y = y.round();
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let (q_x, q_y, q_w, q_h) =
                    crate::renderer::glyph_quad_rect(draw_x, y, g, scale);
                self.push_quad(q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji);
                draw_x += Self::snapped_text_advance(g.advance, scale);
            }
        }
    }

    fn ui_text_visual_mid_y(&mut self, text: &str, scale: f32) -> f32 {
        let mut top = 0.0f32;
        let mut bottom = 0.0f32;
        let mut seen = false;
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let glyph_top = -g.offset_y * scale;
                let glyph_bottom = (g.height - g.offset_y) * scale;
                if seen {
                    top = top.min(glyph_top);
                    bottom = bottom.max(glyph_bottom);
                } else {
                    top = glyph_top;
                    bottom = glyph_bottom;
                    seen = true;
                }
            }
        }
        if seen { (top + bottom) * 0.5 } else { 0.0 }
    }

    fn ui_text_center_y(&mut self, text: &str, baseline_y: f32, scale: f32) -> f32 {
        baseline_y.round() + self.ui_text_visual_mid_y(text, scale)
    }

    fn ui_text_baseline_for_center_y(&mut self, text: &str, center_y: f32, scale: f32) -> f32 {
        (center_y - self.ui_text_visual_mid_y(text, scale)).round()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_branch_chip(
        &mut self,
        text: &str,
        chip_x: f32,
        text_center_y: f32,
        chip_w: f32,
        chip_h: f32,
        radius: f32,
        bg_color: [f32; 4],
        text_color: [f32; 4],
        scale: f32,
        pad_x: f32,
        register_tooltip_row: bool,
        scratch: &mut String,
    ) {
        let text_y = self.ui_text_baseline_for_center_y(text, text_center_y, scale);
        let actual_center_y = self.ui_text_center_y(text, text_y, scale);
        let chip_y = branch_chip_y_from_text_center(actual_center_y, chip_h);
        self.push_rounded_rect(chip_x, chip_y, chip_w, chip_h, radius, bg_color);

        let max_text_w = (chip_w - pad_x * 2.0).max(1.0);
        let full_w = self.measure_ui_width(text, scale);
        let (draw_text, draw_w) = if full_w <= max_text_w {
            (text, full_w)
        } else {
            let ellipsis = "…";
            let ellipsis_w = self.measure_ui_width(ellipsis, scale);
            if ellipsis_w > max_text_w {
                ("", 0.0)
            } else {
                let prefix_len = clipped_label_prefix_len(text, max_text_w, ellipsis_w, |ch| {
                    self.get_ui_glyph(ch)
                        .map(|g| g.advance * scale)
                        .unwrap_or(0.0)
                });
                scratch.clear();
                scratch.push_str(&text[..prefix_len]);
                scratch.push_str(ellipsis);
                let draw_w = self.measure_ui_width(scratch, scale).min(max_text_w);
                (scratch.as_str(), draw_w)
            }
        };
        if draw_text.is_empty() {
            return;
        }

        let text_x = (chip_x + (chip_w - draw_w) * 0.5).round();
        if register_tooltip_row {
            let row_start = self
                .push_git_graph_tooltip_text_row(draw_text, text_x, chip_y, chip_h, scale, false);
            self.draw_git_graph_selectable_text(
                draw_text, text_x, text_y, text_color, scale, row_start, chip_y, chip_h, false,
            );
        } else {
            self.draw_git_graph_row_text(draw_text, text_x, text_y, text_color, scale);
        }
    }

    fn draw_git_graph_label_clipped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        color: [f32; 4],
        scale: f32,
        scratch: &mut String,
    ) -> f32 {
        if max_w <= 0.0 {
            return 0.0;
        }
        let full_w = self.measure_ui_width(text, scale);
        if full_w <= max_w {
            self.draw_git_graph_row_text(text, x, y, color, scale);
            return full_w;
        }

        let ellipsis = "…";
        let ellipsis_w = self.measure_ui_width(ellipsis, scale);
        if ellipsis_w > max_w {
            return 0.0;
        }
        let prefix_len = clipped_label_prefix_len(text, max_w, ellipsis_w, |ch| {
            self.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(0.0)
        });
        scratch.clear();
        scratch.push_str(&text[..prefix_len]);
        scratch.push_str(ellipsis);
        self.draw_git_graph_row_text(scratch, x, y, color, scale);
        self.measure_ui_width(scratch, scale).min(max_w)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_ide_side_panels(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        lsp_has_diagnostics: bool,
        s: f32,
        mx: f32,
        my: f32,
        real_height: f32,
        panel_left_w: f32,
        is_ui_disabled: bool,
        blink_alpha: f32,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
    ) {
        self.git_file_tooltip = None;

        let sb_w = 48.0 * s;
        let blocking_bottom_y =
            if ide_panel.any_bottom_open() && ide_panel.bottom_panel_blocks_editor_hover() {
                Some(ide_bottom_panel_y(
                    real_height,
                    ide_panel.bottom_height * s,
                    s,
                ))
            } else {
                None
            };
        let mouse_in_blocking_bottom = blocking_bottom_y
            .map(|panel_y| my >= panel_y && my <= panel_y + ide_panel.bottom_height * s)
            .unwrap_or(false);
        let hit_mx = if mouse_in_blocking_bottom { -1.0 } else { mx };
        let hit_my = if mouse_in_blocking_bottom { -1.0 } else { my };

        // Сайдбар рисуется на полную высоту окна (real_height)self.push_rect(0.0, 0.0, sb_w, real_height, sidebar_bg);
        self.push_rect(sb_w - 1.0, 0.0, 1.0, real_height, [1.0, 1.0, 1.0, 0.12]);

        let btn_size = sb_w;
        let btn_gap = 0.0;
        let btn_x = 0.0;
        let top_start_y = 0.0;

        let mut top_idx = 0usize;
        let mut bottom_idx = 0usize;

        let lsp_has_issues = lsp.map_or(false, |l| {
            l.diagnostics.values().any(|diags| {
                diags.iter().any(|d| {
                    d.severity == crate::lsp::DiagSeverity::Error
                        || d.severity == crate::lsp::DiagSeverity::Warning
                })
            })
        });

        for slot in &ide_panel.slots {
            let is_dragging_this = ide_panel
                .drag
                .as_ref()
                .map(|d| d.panel_id == slot.id && d.threshold_passed)
                .unwrap_or(false);
            if is_dragging_this {
                if slot.group == crate::app::PanelGroup::Top {
                    top_idx += 1;
                } else {
                    bottom_idx += 1;
                }
                continue;
            }

            let btn_y = if slot.group == crate::app::PanelGroup::Top {
                let y = top_start_y + top_idx as f32 * (btn_size + btn_gap);
                top_idx += 1;
                y
            } else {
                // Кнопки нижней группы фиксированы у дна окна, независимо от панели
                let y = real_height - btn_size - bottom_idx as f32 * btn_size;
                bottom_idx += 1;
                y
            };

            let custom_color = if slot.id == crate::app::PanelId::Problems {
                if lsp_has_issues {
                    Some([1.0, 0.8, 0.1, 1.0])
                } else {
                    Some([0.69, 0.745, 0.773, 1.0])
                }
            } else {
                None
            };

            let btn = IconButton {
                x: btn_x,
                y: btn_y,
                size: btn_size,
                icon: Some(slot.id.icon()),
                is_active: slot.open,
                icon_size: Some(36.0 * s),
                active_square_width: Some(sb_w),
                custom_color,
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::SidebarSlot(slot.id),
                &btn,
                self,
                mx,
                my,
                s,
                false,
            );
        }

        // Призрак перетаскиваемой кнопки + разделитель
        if let Some(drag) = &ide_panel.drag {
            if drag.threshold_passed {
                if let Some(slot) = ide_panel.slots.iter().find(|sl| sl.id == drag.panel_id) {
                    let ghost_y =
                        (drag.current_y - btn_size / 2.0).clamp(0.0, real_height - btn_size);
                    let ghost_color = if slot.id == crate::app::PanelId::Problems {
                        if lsp_has_issues {
                            Some([1.0, 0.8, 0.1, 1.0])
                        } else {
                            Some([0.69, 0.745, 0.773, 1.0])
                        }
                    } else {
                        None
                    };
                    let ghost = IconButton {
                        x: btn_x,
                        y: ghost_y,
                        size: btn_size,
                        icon: Some(slot.id.icon()),
                        is_active: false,
                        icon_size: Some(36.0 * s),
                        active_square_width: None,
                        custom_color: ghost_color,
                    };
                    ghost.render(self, -1.0, -1.0, s, false);
                }
                // Горизонтальный разделитель посередине сайдбара
                let sep_y = (real_height / 2.0).round();
                self.push_rect(
                    2.0 * s,
                    sep_y - 1.0,
                    sb_w - 4.0 * s,
                    2.0,
                    [0.60, 0.35, 0.85, 0.9],
                );
            }
        }

        // Левая панель (для групп Top)
        if panel_left_w > 0.0 {
            let panel_x = sb_w;
            let panel_bg = [
                0.129, // #21
                0.133, // #22
                0.173, // #2c
                1.0,
            ];
            self.push_rect(panel_x, 0.0, panel_left_w, real_height, panel_bg);
            self.push_rect(
                panel_x + panel_left_w - 1.0,
                0.0,
                1.0,
                real_height,
                [1.0, 1.0, 1.0, 0.12],
            );
            // Тонкая линия-разделитель между левой панелью и зоной номеров строк (аналог Indent Guide)
            let sep_x = (panel_x + panel_left_w).round();
            self.push_rect(
                sep_x,
                0.0,
                1.0,
                real_height,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
            );

            let title_h = 32.0 * s;
            let title_bg = [
                (self.theme.bg[0] + 0.07).min(1.0),
                (self.theme.bg[1] + 0.07).min(1.0),
                (self.theme.bg[2] + 0.08).min(1.0),
                1.0,
            ];
            self.push_rect(panel_x, 0.0, panel_left_w, title_h, title_bg);

            let open_top_count = ide_panel
                .slots
                .iter()
                .filter(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                .count();

            if open_top_count == 1 {
                let slot = ide_panel
                    .slots
                    .iter()
                    .find(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                    .unwrap();
                let label = slot.id.label();
                self.draw_string_scaled(
                    label,
                    panel_x + 12.0 * s,
                    title_h / 2.0 + 6.0 * s,
                    self.theme.fg,
                    0.9,
                );
            } else {
                let mut tx = panel_x + 6.0 * s;
                for (i, slot) in ide_panel
                    .slots
                    .iter()
                    .filter(|sl| sl.group == crate::app::PanelGroup::Top && sl.open)
                    .enumerate()
                {
                    let label = slot.id.label();
                    let tw = self.measure_ui_width(label, 0.85) + 20.0 * s;
                    if i == 0 {
                        let act_bg = [
                            (self.theme.bg[0] + 0.12).min(1.0),
                            (self.theme.bg[1] + 0.12).min(1.0),
                            (self.theme.bg[2] + 0.13).min(1.0),
                            1.0,
                        ];
                        self.push_rect(tx, 0.0, tw, title_h, act_bg);
                        self.push_rect(tx, title_h - 2.0, tw, 2.0, [0.60, 0.35, 0.85, 1.0]);
                    }
                    self.draw_string_scaled(
                        label,
                        tx + 10.0 * s,
                        title_h / 2.0 + 6.0 * s,
                        self.theme.fg,
                        0.85,
                    );
                    tx += tw;
                }
            }

            // (Ручка ресайза была здесь, перенесена в конец блока левой панели)

            // --- Project search ---
            if ide_panel.is_open(crate::app::PanelId::Search) {
                let is_top = ide_panel.slots.iter().any(|s| {
                    s.id == crate::app::PanelId::Search && s.group == crate::app::PanelGroup::Top
                });
                if is_top {
                    let panel_bottom_h = if ide_panel.any_bottom_open() {
                        ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let content_bottom = ide_bottom_panel_y(real_height, panel_bottom_h, s);
                    self.draw_project_search_panel(
                        panel_x,
                        title_h,
                        panel_left_w,
                        (content_bottom - title_h).max(0.0),
                        s,
                        ide_panel,
                        ui_registry,
                        blink_alpha,
                    );
                }
            }

            // --- LSP серверы ---
            if ide_panel.is_open(crate::app::PanelId::LspServers) {
                let is_top = ide_panel.slots.iter().any(|s| {
                    s.id == crate::app::PanelId::LspServers
                        && s.group == crate::app::PanelGroup::Top
                });
                if is_top {
                    self.draw_lsp_servers_panel(
                        panel_x,
                        title_h,
                        panel_left_w,
                        real_height - title_h,
                        s,
                        ide_panel,
                        lsp_has_diagnostics,
                        ui_registry,
                    );
                }
            }

            // --- API клиент ---
            if ide_panel.is_open(crate::app::PanelId::ApiClient) {
                let is_top = ide_panel.slots.iter().any(|s| {
                    s.id == crate::app::PanelId::ApiClient
                        && s.group == crate::app::PanelGroup::Top
                });
                if is_top {
                    let panel_bottom_h = if ide_panel.any_bottom_open() {
                        ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let content_bottom = ide_bottom_panel_y(real_height, panel_bottom_h, s);
                    self.draw_api_client_panel(
                        panel_x,
                        title_h,
                        panel_left_w,
                        (content_bottom - title_h).max(0.0),
                        s,
                        ide_panel,
                        ui_registry,
                        hit_mx,
                        hit_my,
                        blink_alpha,
                        active_api_route,
                    );
                }
            }

            // --- Git ---
            if ide_panel.is_open(crate::app::PanelId::Git) {
                let is_top = ide_panel.slots.iter().any(|s| {
                    s.id == crate::app::PanelId::Git && s.group == crate::app::PanelGroup::Top
                });
                if is_top {
                    let panel_bottom_h = if ide_panel.any_bottom_open() {
                        ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let content_bottom = ide_bottom_panel_y(real_height, panel_bottom_h, s);
                    self.draw_git_panel(
                        panel_x,
                        title_h,
                        panel_left_w,
                        (content_bottom - title_h).max(0.0),
                        s,
                        ide_panel,
                        ui_registry,
                        hit_mx,
                        hit_my,
                        blink_alpha,
                    );
                }
            }

            // --- Дерево файлов проводника ---
            if ide_panel.is_open(crate::app::PanelId::Explorer) {
                let file_tree_overlay_open =
                    crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel);
                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    self.gl.scissor(
                        panel_x as i32,
                        0,
                        panel_left_w as i32,
                        (real_height - title_h) as i32,
                    );
                }

                let row_h = crate::render_view::tree_ui::TREE_ROW_H * s;
                let indent_w = crate::render_view::tree_ui::TREE_INDENT_W * s;
                let scroll = ide_panel.explorer_scroll.current.round();
                let hover_settled =
                    (ide_panel.explorer_scroll.current - ide_panel.explorer_scroll.target).abs()
                        < 0.5;
                let content_h = real_height - title_h;
                let total_nodes = ide_panel.file_tree_nodes.len();

                let tree_text_scale = crate::render_view::tree_ui::TREE_TEXT_SCALE;
                if total_nodes == 0 {
                    let hint = "Нет папок в проекте";
                    let tw = self.measure_ui_width(hint, tree_text_scale);
                    let tx = panel_x + (panel_left_w - tw) / 2.0;
                    self.draw_string_scaled(
                        hint,
                        tx,
                        title_h + 30.0 * s,
                        [0.45, 0.45, 0.45, 1.0],
                        tree_text_scale,
                    );
                } else {
                    let first_vis = (scroll / row_h).floor() as usize;
                    let last_vis =
                        (((scroll + content_h) / row_h).ceil() as usize + 1).min(total_nodes);
                    let mut label_scratch = String::new();

                    for i in first_vis..last_vis {
                        let node = &ide_panel.file_tree_nodes[i];
                        let row_y = title_h + i as f32 * row_h - scroll;

                        if !file_tree_overlay_open {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::FileTreeNode(i),
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                hit_mx,
                                hit_my,
                            );
                        }

                        let is_hovered = hover_settled
                            && !file_tree_overlay_open
                            && ui_registry.hovered()
                                == Some(crate::ui_system::UiId::FileTreeNode(i));
                        let is_selected = ide_panel
                            .file_tree_selection
                            .iter()
                            .any(|path| crate::platform::paths_equal(path, &node.path));

                        if is_selected {
                            self.push_rect(
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                [0.60, 0.35, 0.85, 0.24],
                            );
                        }

                        if is_hovered && !is_ui_disabled {
                            self.push_rect(
                                panel_x,
                                row_y,
                                panel_left_w,
                                row_h,
                                [1.0, 1.0, 1.0, 0.06],
                            );
                        }

                        let indent_x = panel_x + 8.0 * s + node.depth as f32 * indent_w;
                        let mut has_error = false;
                        let mut has_warn = false;
                        if !node.is_ignored {
                            let severity = lsp.and_then(|l| {
                                if node.is_dir {
                                    l.diagnostic_severity_under_path(&node.path)
                                } else {
                                    l.diagnostic_severity_for_path(&node.path)
                                }
                            });
                            if let Some(severity) = severity {
                                has_error = severity == crate::lsp::DiagSeverity::Error;
                                has_warn = severity == crate::lsp::DiagSeverity::Warning;
                            }
                        }

                        let color: [f32; 4] = if node.is_ignored {
                            [0.973, 0.584, 0.502, 0.8]
                        } else if node.is_dir {
                            [0.78, 0.68, 1.0, 1.0]
                        } else {
                            [0.651, 0.686, 0.918, 1.0]
                        };

                        let icon_size = 20.0 * s;
                        let icon_y = row_y + (row_h - icon_size) / 2.0;

                        if node.is_dir {
                            let arrow_x = indent_x - 2.0 * s;
                            if !file_tree_overlay_open {
                                ui_registry.register_rect(
                                    crate::ui_system::UiId::FileTreeArrow(i),
                                    arrow_x - 4.0 * s,
                                    row_y,
                                    18.0 * s,
                                    row_h,
                                    hit_mx,
                                    hit_my,
                                );
                            }
                            let arrow_color = if node.is_ignored {
                                [0.973, 0.584, 0.502, 0.6]
                            } else {
                                [0.78, 0.68, 1.0, 0.7]
                            };
                            let label = self.draw_tree_dir_entry(
                                &node.name,
                                node.icon_key,
                                indent_x,
                                row_y,
                                row_h,
                                panel_x + panel_left_w - 10.0 * s,
                                node.is_expanded,
                                color,
                                arrow_color,
                                s,
                                tree_text_scale,
                                &mut label_scratch,
                            );
                            if has_error || has_warn {
                                let sq_color = if has_error {
                                    self.theme.diag_error
                                } else {
                                    self.theme.diag_warn
                                };
                                self.push_squiggle(label.x, label.y + 2.0 * s, label.w, sq_color);
                            }
                        } else {
                            let file_icon_x = crate::render_view::tree_ui::tree_icon_x(indent_x, s);
                            self.draw_file_icon(
                                node.icon_key,
                                false,
                                file_icon_x,
                                icon_y,
                                icon_size,
                            );
                            let text_x = file_icon_x + icon_size + 4.0 * s;
                            let label = self.draw_tree_leaf_label(
                                &node.name,
                                text_x,
                                row_y,
                                row_h,
                                panel_x + panel_left_w - 10.0 * s,
                                color,
                                s,
                                tree_text_scale,
                                &mut label_scratch,
                            );
                            if has_error || has_warn {
                                let sq_color = if has_error {
                                    self.theme.diag_error
                                } else {
                                    self.theme.diag_warn
                                };
                                self.push_squiggle(label.x, label.y + 2.0 * s, label.w, sq_color);
                            }
                        }
                    }

                    if let Some(drag) = &ide_panel.file_tree_drag {
                        if drag.threshold_passed {
                            if let Some(target_idx) = drag.target_idx {
                                if target_idx < total_nodes {
                                    let row_y = title_h + target_idx as f32 * row_h - scroll;
                                    self.push_rect(
                                        panel_x,
                                        row_y,
                                        panel_left_w,
                                        row_h,
                                        [0.52, 0.78, 0.58, 0.22],
                                    );
                                    self.push_rect(
                                        panel_x,
                                        row_y + row_h - 2.0,
                                        panel_left_w,
                                        2.0,
                                        [0.52, 0.78, 0.58, 0.85],
                                    );
                                }
                            }
                            let label = if drag.paths.len() == 1 {
                                drag.paths[0]
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("1 элемент")
                                    .to_string()
                            } else {
                                format!("{} элементов", drag.paths.len())
                            };
                            let ghost_w = self.measure_ui_width(&label, tree_text_scale) + 18.0 * s;
                            let ghost_x = drag.current_x + 12.0 * s;
                            let ghost_y = drag.current_y + 10.0 * s;
                            self.push_rounded_rect(
                                ghost_x,
                                ghost_y,
                                ghost_w,
                                26.0 * s,
                                5.0 * s,
                                [0.12, 0.13, 0.18, 0.92],
                            );
                            self.draw_string_scaled(
                                &label,
                                ghost_x + 9.0 * s,
                                ghost_y + 18.0 * s,
                                self.theme.fg,
                                tree_text_scale,
                            );
                        }
                    }

                    // Тонкий скроллбар
                    let total_h = total_nodes as f32 * row_h;
                    if total_h > content_h {
                        let max_s = (total_h - content_h).max(1.0);
                        let ratio = (scroll / max_s).clamp(0.0, 1.0);
                        let thumb_h = (content_h / total_h * (content_h - 8.0 * s)).max(20.0 * s);
                        let thumb_y = title_h + 4.0 * s + ratio * (content_h - 8.0 * s - thumb_h);
                        self.push_rounded_rect(
                            panel_x + panel_left_w - 5.0 * s,
                            thumb_y,
                            3.0 * s,
                            thumb_h,
                            1.5 * s,
                            [1.0, 1.0, 1.0, 0.22],
                        );
                    }
                }

                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }
            }

            // Подсветка ручки ресайза (wants_pointer=false — курсор управляется в events.rs через EwResize)
            // Не подсвечиваем, когда терминал в фокусе
            let resize_x = panel_x + panel_left_w;
            let resize_hit = 3.0 * s;
            let resize_max_y = blocking_bottom_y.unwrap_or(real_height);
            if !is_ui_disabled
                && mx >= resize_x - resize_hit
                && mx <= resize_x + resize_hit
                && my >= 0.0
                && my <= resize_max_y
            {
                self.push_rect(
                    resize_x - 1.0,
                    0.0,
                    1.0,
                    resize_max_y,
                    [0.60, 0.35, 0.85, 0.4],
                );
            }
        }
    }

}
