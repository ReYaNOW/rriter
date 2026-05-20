#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    fn draw_git_panel(
        &mut self,
        panel_x: f32,
        title_h: f32,
        panel_w: f32,
        content_h: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        let pad = (10.0 * s).min((panel_w * 0.15).max(0.0));
        let inner_w = (panel_w - pad * 2.0).max(1.0);
        let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * s;
        let list_y = title_h + controls_h;
        let full_list_h = (content_h - controls_h).max(40.0 * s);
        let (list_h, graph_divider_h, graph_h) = if ide_panel.git.graph_open {
            crate::app::git_panel::git_graph_split_heights(
                full_list_h,
                ide_panel.git.graph_height_ratio,
                s,
            )
        } else {
            (full_list_h, 0.0, 0.0)
        };
        let graph_divider_y = list_y + list_h;
        let graph_y = graph_divider_y + graph_divider_h;
        let row_h = crate::render_view::tree_ui::TREE_ROW_H * s;
        let workspace_h = 30.0 * s;
        let scroll = ide_panel.git.scroll.current.round();
        let mut y = list_y - scroll;
        let text_scale = crate::render_view::tree_ui::TREE_TEXT_SCALE;
        let mut label_scratch = String::new();
        let mut git_file_tooltip: Option<(usize, usize, String, f32, f32)> = None;

        let input_x = panel_x + pad;
        let input_y = title_h + 8.0 * s;
        let input_w = inner_w;
        let input_h = 30.0 * s;
        let input_border = if ide_panel.git.message_focused {
            [0.60, 0.35, 0.85, 0.78]
        } else {
            [1.0, 1.0, 1.0, 0.10]
        };
        self.push_rounded_rect(
            input_x - 1.0,
            input_y - 1.0,
            input_w + 2.0,
            input_h + 2.0,
            4.0 * s,
            input_border,
        );
        self.push_rounded_rect(
            input_x,
            input_y,
            input_w,
            input_h,
            4.0 * s,
            if ide_panel.git.message_focused {
                [0.18, 0.19, 0.25, 1.0]
            } else {
                [0.11, 0.12, 0.16, 1.0]
            },
        );
        ui_registry.register_text_input(
            crate::ui_system::UiId::GitMessageInput,
            input_x,
            input_y,
            input_w,
            input_h,
            mx,
            my,
        );

        self.flush();
        unsafe {
            let text = ide_panel.git.message_editor.get_full_text();
            let text_y = input_y + input_h / 2.0 + 6.0 * s;
            let text_start_x = input_x + 5.0 * s;
            let visible_width = input_w - 10.0 * s;

            let mut cursor_total_x = 0.0;
            let mut total_text_width = 0.0;
            for (byte_idx, c) in text.char_indices() {
                let adv = self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0);
                if byte_idx < ide_panel.git.message_editor.cursor {
                    cursor_total_x += adv;
                }
                total_text_width += adv;
            }

            if ide_panel.git.message_focused {
                if cursor_total_x - self.search_scroll_x > visible_width {
                    self.search_scroll_x = cursor_total_x - visible_width;
                }
                if cursor_total_x - self.search_scroll_x < 0.0 {
                    self.search_scroll_x = cursor_total_x;
                }
                self.search_scroll_x = self
                    .search_scroll_x
                    .min(total_text_width - visible_width)
                    .max(0.0);
            }

            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (input_y + input_h);
            self.gl.scissor(
                input_x as i32,
                scissor_y as i32,
                input_w as i32,
                input_h as i32,
            );

            let sel_start = ide_panel
                .git
                .message_editor
                .selection_anchor
                .unwrap_or(ide_panel.git.message_editor.cursor)
                .min(ide_panel.git.message_editor.cursor);
            let sel_end = ide_panel
                .git
                .message_editor
                .selection_anchor
                .unwrap_or(ide_panel.git.message_editor.cursor)
                .max(ide_panel.git.message_editor.cursor);
            let mut cursor_draw_x = text_start_x - self.search_scroll_x;

            if text.is_empty() {
                self.draw_string_scaled(
                    "Message",
                    text_start_x,
                    text_y,
                    [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.34],
                    1.0,
                );
            } else {
                let mut current_x = text_start_x - self.search_scroll_x;
                let mut byte_idx = 0usize;

                for c in text.chars() {
                    if byte_idx == ide_panel.git.message_editor.cursor {
                        cursor_draw_x = current_x;
                    }
                    let adv = self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0);
                    if byte_idx >= sel_start && byte_idx < sel_end {
                        self.push_rect(
                            current_x,
                            input_y + 4.0 * s,
                            adv,
                            input_h - 8.0 * s,
                            self.theme.sel,
                        );
                    }
                    if let Some(g) = self.get_ui_glyph(c) {
                        self.push_quad(
                            current_x + g.offset_x,
                            text_y - g.offset_y,
                            g.width,
                            g.height,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            self.theme.fg,
                            g.is_emoji,
                        );
                    }
                    current_x += adv;
                    byte_idx += c.len_utf8();
                }
                if byte_idx == ide_panel.git.message_editor.cursor {
                    cursor_draw_x = current_x;
                }
            }
            if ide_panel.git.message_focused && sel_start == sel_end && blink_alpha > 0.5 {
                self.push_rect(
                    cursor_draw_x,
                    input_y + 4.0 * s,
                    2.0 * s,
                    input_h - 8.0 * s,
                    self.theme.fg,
                );
            }
            self.flush();
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let commit_y = title_h + 44.0 * s;
        let arrow_w = (38.0 * s).min((inner_w * 0.28).max(22.0 * s));
        let commit_gap = (4.0 * s).min((inner_w * 0.06).max(0.0));
        let commit_main_w = (inner_w - arrow_w - commit_gap).max(1.0);
        let commit_btn = Button {
            x: panel_x + pad,
            y: commit_y,
            w: commit_main_w,
            h: 28.0 * s,
            text: "Commit".to_string(),
            icon: Some(crate::widgets::IconType::Check),
            text_scale: 0.92,
            icon_size: 20.0 * s,
        };
        if ide_panel.git.pending {
            render_git_disabled_button(self, &commit_btn, s);
        } else {
            ui_registry.register_button(
                crate::ui_system::UiId::GitCommit,
                &commit_btn,
                self,
                mx,
                my,
                s,
                false,
            );
        }
        let menu_btn = Button {
            x: panel_x + pad + commit_main_w + commit_gap,
            y: commit_y,
            w: arrow_w,
            h: 28.0 * s,
            text: String::new(),
            icon: Some(crate::widgets::IconType::Down),
            text_scale: 0.0,
            icon_size: 24.0 * s,
        };
        if ide_panel.git.pending {
            render_git_disabled_button(self, &menu_btn, s);
        } else {
            ui_registry.register_button(
                crate::ui_system::UiId::GitCommitMenuToggle,
                &menu_btn,
                self,
                mx,
                my,
                s,
                false,
            );
        }

        let graph_btn_y = title_h + 75.0 * s;
        let graph_btn_w = (72.0 * s).min(inner_w.max(1.0));
        let graph_btn = Button {
            x: panel_x + pad,
            y: graph_btn_y,
            w: graph_btn_w,
            h: 22.0 * s,
            text: "Граф".to_string(),
            icon: Some(crate::widgets::IconType::Branch),
            text_scale: 0.78,
            icon_size: 21.0 * s,
        };
        let graph_hovered = ui_registry.register_rect(
            crate::ui_system::UiId::GitGraphToggle,
            graph_btn.x,
            graph_btn.y,
            graph_btn.w,
            graph_btn.h,
            mx,
            my,
        );
        render_git_graph_button(self, &graph_btn, s, graph_hovered, ide_panel.git.graph_open);
        if ide_panel.git.graph_open {
            self.push_rect(
                graph_btn.x,
                graph_btn.y + graph_btn.h - 2.0,
                graph_btn.w,
                2.0,
                [0.60, 0.35, 0.85, 0.9],
            );
        }

        let refresh_gap = 6.0 * s;
        let refresh_x = graph_btn.x + graph_btn.w + refresh_gap;
        let refresh_available_w = (panel_x + pad + inner_w - refresh_x).max(0.0);
        let refresh_label_w = self.measure_ui_width("Обновить", 0.78);
        let refresh_full_w = refresh_label_w + 22.0 * s + 18.0 * s;
        let mut notice_x = graph_btn.x + graph_btn.w + 8.0 * s;
        if refresh_available_w >= 30.0 * s {
            let refresh_icon_only = refresh_available_w < refresh_full_w;
            let refresh_btn = Button {
                x: refresh_x,
                y: graph_btn_y,
                w: if refresh_icon_only {
                    (34.0 * s).min(refresh_available_w)
                } else {
                    refresh_full_w.min(refresh_available_w)
                },
                h: 22.0 * s,
                text: if refresh_icon_only {
                    String::new()
                } else {
                    "Обновить".to_string()
                },
                icon: Some(crate::widgets::IconType::Reload),
                text_scale: 0.78,
                icon_size: 22.0 * s,
            };
            let refresh_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::GitRefresh,
                refresh_btn.x,
                refresh_btn.y,
                refresh_btn.w,
                refresh_btn.h,
                mx,
                my,
            );
            render_git_graph_button(self, &refresh_btn, s, refresh_hovered, false);
            notice_x = refresh_btn.x + refresh_btn.w + 8.0 * s;
        }

        if let Some(notice) = ide_panel
            .git
            .graph_notice
            .as_ref()
            .or(ide_panel.git.notice.as_ref())
        {
            self.draw_tree_label_clipped(
                notice,
                notice_x,
                graph_btn_y + 16.0 * s,
                (panel_x + pad + inner_w - notice_x).max(0.0),
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.68],
                0.78,
                &mut label_scratch,
            );
        }

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (list_y + list_h);
            self.gl.scissor(
                panel_x as i32,
                scissor_y.max(0.0) as i32,
                panel_w as i32,
                list_h as i32,
            );
        }

        let staged_workspace = ide_panel.git.staged_workspace_lock();
        let mut drew_any = false;

        for workspace in &ide_panel.git.snapshot.workspaces {
            let workspace_disabled =
                staged_workspace.is_some_and(|idx| idx != workspace.workspace_idx);
            let workspace_is_collapsed = ide_panel
                .git
                .collapsed_workspaces
                .contains(&workspace.workspace_idx);

            drew_any = true;
            let row_visible = y + workspace_h >= list_y && y <= list_y + list_h;
            if row_visible {
                let workspace_name_color =
                    git_disabled_color(self.theme.fg, workspace_disabled, 0.38);
                let show_stage_actions = !workspace.files.is_empty();
                let stage_interaction_disabled =
                    git_stage_controls_disabled(workspace_disabled, ide_panel.git.pending);
                let changed_count = workspace.files.len();
                let count_text_scale = 0.78;
                let count_badge_h = 19.0 * s;
                let (count_badge_w, count_text_w) = if changed_count > 0 {
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("{changed_count}"),
                    );
                    let text_w = self
                        .measure_ui_width(&label_scratch, count_text_scale)
                        .round();
                    ((text_w + 12.0 * s).max(count_badge_h), text_w)
                } else {
                    (0.0, 0.0)
                };
                let count_reserve = if changed_count > 0 {
                    count_badge_w + 6.0 * s
                } else {
                    0.0
                };
                let stage_btn_w = 26.0 * s;
                let stage_btn_gap = 4.0 * s;
                let stage_actions_w = if show_stage_actions {
                    stage_btn_w * 3.0 + stage_btn_gap * 2.0 + 8.0 * s
                } else {
                    0.0
                };
                let (ahead_text_w, push_w) = if workspace.ahead > 0 {
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("↑{}", workspace.ahead),
                    );
                    (
                        self.measure_ui_width(&label_scratch, 0.78).round(),
                        (46.0 * s).min((panel_w * 0.36).max(18.0 * s)),
                    )
                } else {
                    (0.0, 0.0)
                };
                let push_reserve = if workspace.ahead > 0 {
                    ahead_text_w + 8.0 * s + push_w + 6.0 * s
                } else {
                    0.0
                };
                self.push_rect(
                    panel_x,
                    y,
                    panel_w,
                    workspace_h,
                    [
                        self.theme.bg[0] + 0.035,
                        self.theme.bg[1] + 0.035,
                        self.theme.bg[2] + 0.045,
                        1.0,
                    ],
                );
                let name = workspace
                    .root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("workspace");
                let workspace_has_rows = workspace.has_collapsible_rows();
                let workspace_arrow_x = panel_x + pad;
                let workspace_label_x = if workspace_has_rows {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::GitWorkspaceToggle(workspace.workspace_idx),
                        workspace_arrow_x - 4.0 * s,
                        y + 4.0 * s,
                        18.0 * s,
                        workspace_h - 8.0 * s,
                        mx,
                        my,
                    );
                    self.draw_tree_disclosure_icon(
                        !workspace_is_collapsed,
                        workspace_arrow_x,
                        y + 2.0 * s,
                        workspace_h,
                        git_disabled_color([0.78, 0.80, 0.88, 0.75], workspace_disabled, 0.26),
                    );
                    workspace_arrow_x + 18.0 * s
                } else {
                    workspace_arrow_x
                };
                let right_reserve = 12.0 * s + count_reserve + push_reserve + stage_actions_w;
                let label_w = (panel_x + panel_w - workspace_label_x - right_reserve).max(0.0);
                let workspace_text_y = y + workspace_h / 2.0 + 4.5 * s;
                if let Some(branch_name) = &workspace.branch_name {
                    let branch_scale = 0.82;
                    let chip_pad_x = 6.0 * s;
                    let chip_h = 19.0 * s;
                    let chip_w =
                        self.measure_ui_width(branch_name, branch_scale) + chip_pad_x * 2.0;
                    let gap = 8.0 * s;
                    if label_w > chip_w + gap + 24.0 * s {
                        let name_w = self.measure_ui_width(name, 0.9).min(label_w - chip_w - gap);
                        self.draw_tree_label_clipped(
                            name,
                            workspace_label_x,
                            workspace_text_y,
                            name_w,
                            workspace_name_color,
                            0.9,
                            &mut label_scratch,
                        );
                        let chip_x = (workspace_label_x + name_w + gap).round();
                        let workspace_text_center_y =
                            self.ui_text_center_y(name, workspace_text_y, 0.9);
                        let branch_text_y = self.ui_text_baseline_for_center_y(
                            branch_name,
                            workspace_text_center_y,
                            branch_scale,
                        );
                        let branch_center_y =
                            self.ui_text_center_y(branch_name, branch_text_y, branch_scale);
                        let chip_y = branch_chip_y_from_text_center(branch_center_y, chip_h);
                        let chip_w = chip_w.round();
                        self.push_rounded_rect(
                            chip_x,
                            chip_y,
                            chip_w,
                            chip_h,
                            4.0 * s,
                            [0.20, 0.22, 0.30, 1.0],
                        );
                        self.draw_string_scaled(
                            branch_name,
                            (chip_x + chip_pad_x).round(),
                            branch_text_y,
                            [0.78, 0.82, 0.92, 1.0],
                            branch_scale,
                        );
                    } else {
                        self.draw_tree_label_clipped(
                            name,
                            workspace_label_x,
                            workspace_text_y,
                            label_w,
                            workspace_name_color,
                            0.9,
                            &mut label_scratch,
                        );
                    }
                } else {
                    self.draw_tree_label_clipped(
                        name,
                        workspace_label_x,
                        workspace_text_y,
                        label_w,
                        workspace_name_color,
                        0.9,
                        &mut label_scratch,
                    );
                }

                let right_x = panel_x + panel_w - pad;
                if show_stage_actions {
                    let stage_btn_h = 22.0 * s;
                    let unstage_x = right_x - stage_btn_w;
                    let stage_x = unstage_x - stage_btn_gap - stage_btn_w;
                    let rollback_x = stage_x - stage_btn_gap - stage_btn_w;
                    let push_x = if workspace.ahead > 0 {
                        rollback_x - 6.0 * s - push_w
                    } else {
                        rollback_x
                    };
                    let btn_y = y + ((workspace_h - stage_btn_h) / 2.0).round();
                    if changed_count > 0 {
                        label_scratch.clear();
                        let _ = std::fmt::Write::write_fmt(
                            &mut label_scratch,
                            format_args!("{changed_count}"),
                        );
                        let badge_x = if workspace.ahead > 0 {
                            push_x - 8.0 * s - ahead_text_w - 6.0 * s - count_badge_w
                        } else {
                            rollback_x - stage_btn_gap - count_badge_w
                        };
                        let badge_y = y + ((workspace_h - count_badge_h) / 2.0).round();
                        self.push_rounded_rect(
                            badge_x,
                            badge_y,
                            count_badge_w,
                            count_badge_h,
                            count_badge_h / 2.0,
                            git_disabled_color([0.24, 0.27, 0.34, 1.0], workspace_disabled, 0.34),
                        );
                        self.draw_string_scaled(
                            &label_scratch,
                            (badge_x + (count_badge_w - count_text_w) / 2.0).round(),
                            (badge_y + count_badge_h / 2.0 + 4.0 * s).round(),
                            git_disabled_color([0.86, 0.90, 1.0, 1.0], workspace_disabled, 0.38),
                            count_text_scale,
                        );
                    }
                    if workspace.ahead > 0 {
                        label_scratch.clear();
                        let _ = std::fmt::Write::write_fmt(
                            &mut label_scratch,
                            format_args!("↑{}", workspace.ahead),
                        );
                        self.draw_string_scaled(
                            &label_scratch,
                            (push_x - 8.0 * s - ahead_text_w).max(panel_x + pad),
                            y + workspace_h / 2.0 + 5.0 * s,
                            git_disabled_color([0.48, 0.74, 1.0, 1.0], workspace_disabled, 0.34),
                            0.78,
                        );
                        let push_btn = Button {
                            x: push_x,
                            y: y + 5.0 * s,
                            w: push_w,
                            h: 20.0 * s,
                            text: if push_w < 38.0 * s { "↑" } else { "Push" }.to_string(),
                            icon: None,
                            text_scale: 0.76,
                            icon_size: 0.0,
                        };
                        if workspace_disabled {
                            render_git_disabled_button(self, &push_btn, s);
                            register_git_locked_button_cursor(
                                ui_registry,
                                crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                                &push_btn,
                                mx,
                                my,
                            );
                        } else if ide_panel.git.pending {
                            push_btn.render(self, -1.0, -1.0, s, false);
                            register_git_locked_button_cursor(
                                ui_registry,
                                crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                                &push_btn,
                                mx,
                                my,
                            );
                        } else {
                            ui_registry.register_button(
                                crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                                &push_btn,
                                self,
                                mx,
                                my,
                                s,
                                false,
                            );
                        }
                    }
                    let rollback_btn = Button {
                        x: rollback_x,
                        y: btn_y,
                        w: stage_btn_w,
                        h: stage_btn_h,
                        text: String::new(),
                        icon: Some(crate::widgets::IconType::Rollback),
                        text_scale: 0.98,
                        icon_size: 21.0 * s,
                    };
                    let rollback_hovered = if workspace_disabled {
                        render_git_disabled_button(self, &rollback_btn, s);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitRollbackStaged(workspace.workspace_idx),
                            &rollback_btn,
                            mx,
                            my,
                        );
                        false
                    } else if stage_interaction_disabled {
                        rollback_btn.render(self, -1.0, -1.0, s, false);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitRollbackStaged(workspace.workspace_idx),
                            &rollback_btn,
                            mx,
                            my,
                        );
                        false
                    } else {
                        ui_registry.register_button(
                            crate::ui_system::UiId::GitRollbackStaged(workspace.workspace_idx),
                            &rollback_btn,
                            self,
                            mx,
                            my,
                            s,
                            false,
                        )
                    };
                    if rollback_hovered {
                        self.git_action_tooltip = Some((
                            GIT_TOOLTIP_ROLLBACK,
                            workspace.workspace_idx,
                            "Откатить staged".to_string(),
                            mx,
                            my,
                        ));
                    }

                    for (id, icon, bx, tooltip, kind) in [
                        (
                            crate::ui_system::UiId::GitStageAll(workspace.workspace_idx),
                            crate::widgets::IconType::GitPlus,
                            stage_x,
                            "Добавить все",
                            GIT_TOOLTIP_STAGE_ALL,
                        ),
                        (
                            crate::ui_system::UiId::GitUnstageAll(workspace.workspace_idx),
                            crate::widgets::IconType::GitMinus,
                            unstage_x,
                            "Убрать все",
                            GIT_TOOLTIP_UNSTAGE_ALL,
                        ),
                    ] {
                        let btn = Button {
                            x: bx,
                            y: btn_y,
                            w: stage_btn_w,
                            h: stage_btn_h,
                            text: String::new(),
                            icon: Some(icon),
                            text_scale: 0.0,
                            icon_size: 28.0 * s,
                        };
                        let hovered = if workspace_disabled {
                            render_git_disabled_button(self, &btn, s);
                            register_git_locked_button_cursor(ui_registry, id, &btn, mx, my);
                            false
                        } else if stage_interaction_disabled {
                            btn.render(self, -1.0, -1.0, s, false);
                            register_git_locked_button_cursor(ui_registry, id, &btn, mx, my);
                            false
                        } else {
                            ui_registry.register_button(id, &btn, self, mx, my, s, false)
                        };
                        if hovered {
                            self.git_action_tooltip =
                                Some((kind, workspace.workspace_idx, tooltip.to_string(), mx, my));
                        }
                    }
                } else if workspace.ahead > 0 {
                    let push_x = right_x - push_w;
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("↑{}", workspace.ahead),
                    );
                    self.draw_string_scaled(
                        &label_scratch,
                        (push_x - 8.0 * s - ahead_text_w).max(panel_x + pad),
                        y + workspace_h / 2.0 + 5.0 * s,
                        git_disabled_color([0.48, 0.74, 1.0, 1.0], workspace_disabled, 0.34),
                        0.78,
                    );
                    let push_btn = Button {
                        x: push_x,
                        y: y + 5.0 * s,
                        w: push_w,
                        h: 20.0 * s,
                        text: if push_w < 38.0 * s { "↑" } else { "Push" }.to_string(),
                        icon: None,
                        text_scale: 0.76,
                        icon_size: 0.0,
                    };
                    if workspace_disabled {
                        render_git_disabled_button(self, &push_btn, s);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                            &push_btn,
                            mx,
                            my,
                        );
                    } else if ide_panel.git.pending {
                        push_btn.render(self, -1.0, -1.0, s, false);
                        register_git_locked_button_cursor(
                            ui_registry,
                            crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                            &push_btn,
                            mx,
                            my,
                        );
                    } else {
                        ui_registry.register_button(
                            crate::ui_system::UiId::GitPush(workspace.workspace_idx),
                            &push_btn,
                            self,
                            mx,
                            my,
                            s,
                            false,
                        );
                    }
                } else if changed_count > 0 {
                    label_scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut label_scratch,
                        format_args!("{changed_count}"),
                    );
                    let badge_x = right_x - count_badge_w;
                    let badge_y = y + ((workspace_h - count_badge_h) / 2.0).round();
                    self.push_rounded_rect(
                        badge_x,
                        badge_y,
                        count_badge_w,
                        count_badge_h,
                        count_badge_h / 2.0,
                        git_disabled_color([0.24, 0.27, 0.34, 1.0], workspace_disabled, 0.34),
                    );
                    self.draw_string_scaled(
                        &label_scratch,
                        (badge_x + (count_badge_w - count_text_w) / 2.0).round(),
                        (badge_y + count_badge_h / 2.0 + 4.0 * s).round(),
                        git_disabled_color([0.86, 0.90, 1.0, 1.0], workspace_disabled, 0.38),
                        count_text_scale,
                    );
                }
            }
            y += workspace_h;

            if workspace_is_collapsed {
                continue;
            }

            if let Some(err) = &workspace.error {
                if y + row_h >= list_y && y <= list_y + list_h {
                    self.draw_tree_label_clipped(
                        err,
                        panel_x + pad,
                        y + row_h / 2.0 + 5.0 * s,
                        inner_w,
                        [0.95, 0.42, 0.46, 1.0],
                        0.82,
                        &mut label_scratch,
                    );
                }
                y += row_h;
                continue;
            }

            let mut collapsed_depth = None;
            let workspace_collapsed = ide_panel.git.collapsed_dirs.get(&workspace.workspace_idx);
            for (row_idx, row) in workspace.tree.iter().enumerate() {
                if let Some(depth) = collapsed_depth {
                    if row.depth > depth {
                        continue;
                    }
                    collapsed_depth = None;
                }
                let visible = y + row_h >= list_y && y <= list_y + list_h;
                let row_collapsed = row.file_idx.is_none()
                    && workspace_collapsed.is_some_and(|dirs| dirs.contains(row.path.as_str()));
                if visible {
                    let indent_x = panel_x
                        + pad
                        + row.depth as f32 * crate::render_view::tree_ui::TREE_INDENT_W * s;
                    if let Some(file_idx) = row.file_idx {
                        let Some(file) = workspace.files.get(file_idx) else {
                            y += row_h;
                            continue;
                        };
                        let file_layout = git_file_row_layout(indent_x, y, row_h, s);
                        let check_x = file_layout.check_x;
                        let check_y = file_layout.check_y;
                        let stage_interaction_disabled =
                            git_stage_controls_disabled(workspace_disabled, ide_panel.git.pending);
                        let hovered = git_row_visual_hovered(
                            mx,
                            my,
                            panel_x,
                            y,
                            panel_w,
                            row_h,
                            workspace_disabled,
                        );
                        let selected = ide_panel.git.selected_file
                            == Some((workspace.workspace_idx, file_idx));
                        if git_file_row_hitbox_enabled(stage_interaction_disabled) {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::GitFile(workspace.workspace_idx, file_idx),
                                check_x - 3.0 * s,
                                y + 4.0 * s,
                                file_layout.check_size + 6.0 * s,
                                row_h - 8.0 * s,
                                mx,
                                my,
                            );
                        }
                        if hovered {
                            self.push_rect(panel_x, y, panel_w, row_h, [1.0, 1.0, 1.0, 0.055]);
                        } else if selected {
                            self.push_rect(
                                panel_x,
                                y,
                                panel_w,
                                row_h,
                                [
                                    self.theme.sel[0],
                                    self.theme.sel[1],
                                    self.theme.sel[2],
                                    0.16,
                                ],
                            );
                        }
                        if git_file_tooltip_hovered(hovered, mx, check_x, file_layout.check_size) {
                            let home = std::env::var_os("HOME")
                                .or_else(|| std::env::var_os("USERPROFILE"))
                                .map(std::path::PathBuf::from);
                            git_file_tooltip = Some((
                                workspace.workspace_idx,
                                file_idx,
                                git_file_tooltip_text(file, home.as_deref()),
                                mx,
                                my,
                            ));
                        }

                        let (checkbox_color, check_color) =
                            git_checkbox_color(file.staged, false, workspace_disabled);
                        self.push_rounded_rect(
                            check_x,
                            check_y,
                            file_layout.check_size,
                            file_layout.check_size,
                            2.0 * s,
                            checkbox_color,
                        );
                        if file.staged {
                            self.draw_string_scaled(
                                "✓",
                                check_x + 2.0 * s,
                                y + 18.0 * s,
                                check_color,
                                0.78,
                            );
                        }

                        let status_w = 18.0 * s;
                        let status_x = panel_x + panel_w - pad - status_w;
                        if !workspace_disabled {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::GitFileDiff(
                                    workspace.workspace_idx,
                                    file_idx,
                                ),
                                file_layout.icon_x - 3.0 * s,
                                y,
                                (status_x - file_layout.icon_x - 8.0 * s).max(0.0),
                                row_h,
                                mx,
                                my,
                            );
                        }
                        self.draw_string_scaled(
                            file.status.label(),
                            status_x,
                            y + row_h / 2.0 + 5.0 * s,
                            if workspace_disabled {
                                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.28]
                            } else {
                                file.status.color()
                            },
                            0.82,
                        );

                        self.draw_file_icon(
                            row.icon_key,
                            false,
                            file_layout.icon_x,
                            file_layout.icon_y,
                            file_layout.icon_size,
                        );
                        self.draw_tree_leaf_label(
                            &row.name,
                            file_layout.text_x,
                            y,
                            row_h,
                            status_x - 8.0 * s,
                            if workspace_disabled {
                                [0.72, 0.76, 0.88, 0.38]
                            } else {
                                [0.72, 0.76, 0.88, 1.0]
                            },
                            s,
                            text_scale,
                            &mut label_scratch,
                        );
                    } else {
                        ui_registry.register_rect(
                            crate::ui_system::UiId::GitFolder(workspace.workspace_idx, row_idx),
                            panel_x,
                            y,
                            panel_w,
                            row_h,
                            mx,
                            my,
                        );
                        let folder_stage =
                            crate::app::git_panel::git_folder_stage_state(workspace, row_idx);
                        let folder_layout = git_folder_row_layout(indent_x, y, row_h, s);
                        let stage_interaction_disabled =
                            git_stage_controls_disabled(workspace_disabled, ide_panel.git.pending);
                        let check_size = folder_layout.check_size;
                        let check_x = folder_layout.check_x;
                        let check_y = folder_layout.check_y;
                        let (checkbox_color, check_color) = git_checkbox_color(
                            matches!(
                                folder_stage,
                                Some(crate::app::git_panel::GitFolderStageState::All)
                            ),
                            matches!(
                                folder_stage,
                                Some(crate::app::git_panel::GitFolderStageState::Partial)
                            ),
                            workspace_disabled,
                        );
                        self.push_rounded_rect(
                            check_x,
                            check_y,
                            check_size,
                            check_size,
                            2.0 * s,
                            checkbox_color,
                        );
                        match folder_stage {
                            Some(crate::app::git_panel::GitFolderStageState::All) => {
                                self.draw_string_scaled(
                                    "✓",
                                    check_x + 2.0 * s,
                                    y + 18.0 * s,
                                    check_color,
                                    0.78,
                                );
                            }
                            Some(crate::app::git_panel::GitFolderStageState::Partial) => {
                                let mark_w = 8.0 * s;
                                let mark_h = 2.0 * s;
                                self.push_rect(
                                    check_x + (check_size - mark_w) / 2.0,
                                    check_y + (check_size - mark_h) / 2.0,
                                    mark_w,
                                    mark_h,
                                    check_color,
                                );
                            }
                            _ => {}
                        }
                        if !stage_interaction_disabled
                            && git_folder_stage_hitbox_enabled(folder_stage)
                        {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::GitFolderStage(
                                    workspace.workspace_idx,
                                    row_idx,
                                ),
                                check_x - 3.0 * s,
                                y + 4.0 * s,
                                check_size + 6.0 * s,
                                row_h - 8.0 * s,
                                mx,
                                my,
                            );
                        }

                        let text_y = Self::tree_row_text_y(y, row_h, s);
                        let arrow_color =
                            git_disabled_color([0.78, 0.80, 0.88, 0.75], workspace_disabled, 0.26);
                        self.draw_tree_disclosure_icon(
                            !row_collapsed,
                            folder_layout.arrow_x,
                            y,
                            row_h,
                            arrow_color,
                        );
                        let icon_size = folder_layout.icon_size;
                        let icon_x = folder_layout.icon_x;
                        let icon_y = folder_layout.icon_y;
                        self.draw_file_icon(row.icon_key, true, icon_x, icon_y, icon_size);
                        let text_x = icon_x + icon_size + 4.0 * s;
                        self.draw_tree_label_clipped(
                            &row.name,
                            text_x,
                            text_y,
                            (panel_x + panel_w - pad - text_x).max(0.0),
                            git_disabled_color(self.theme.fg, workspace_disabled, 0.38),
                            text_scale,
                            &mut label_scratch,
                        );
                    }
                }
                y += row_h;
                if row_collapsed {
                    collapsed_depth = Some(row.depth);
                }
            }
        }

        if !drew_any {
            let hint = if ide_panel.git.pending {
                "Git scan..."
            } else {
                "No changes"
            };
            let tw = self.measure_ui_width(hint, text_scale);
            self.draw_string_scaled(
                hint,
                panel_x + (panel_w - tw) / 2.0,
                list_y + 30.0 * s,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.45],
                text_scale,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let total_h = (y + scroll - list_y).max(0.0);
        if total_h > list_h {
            let max_s = (total_h - list_h).max(1.0);
            let ratio = (scroll / max_s).clamp(0.0, 1.0);
            let thumb_h = (list_h / total_h * (list_h - 8.0 * s)).max(20.0 * s);
            let thumb_y = list_y + 4.0 * s + ratio * (list_h - 8.0 * s - thumb_h);
            self.push_rounded_rect(
                panel_x + panel_w - 5.0 * s,
                thumb_y,
                3.0 * s,
                thumb_h,
                1.5 * s,
                [1.0, 1.0, 1.0, 0.22],
            );
        }

        if ide_panel.git.graph_open {
            let divider_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::GitGraphResize,
                panel_x,
                graph_divider_y - 3.0 * s,
                panel_w,
                graph_divider_h + 6.0 * s,
                mx,
                my,
            );
            self.push_rect(
                panel_x,
                graph_divider_y,
                panel_w,
                1.0,
                [0.0, 0.0, 0.0, 0.22],
            );
            self.push_rect(
                panel_x,
                graph_divider_y,
                panel_w,
                if divider_hovered {
                    2.0
                } else {
                    graph_divider_h.max(1.0)
                },
                if divider_hovered {
                    [0.60, 0.35, 0.85, 0.4]
                } else {
                    [1.0, 1.0, 1.0, 0.10]
                },
            );
            self.draw_git_graph_panel(
                panel_x,
                panel_w,
                graph_y,
                graph_h,
                pad,
                s,
                ide_panel,
                ui_registry,
                mx,
                my,
                &mut label_scratch,
            );
        }

        if ide_panel.git.commit_menu_open && !ide_panel.git.pending {
            let menu_w = inner_w.min(230.0 * s).max(120.0 * s).min(panel_w);
            let menu_x = (panel_x + pad + inner_w - menu_w).max(panel_x + 2.0 * s);
            let item_h = 32.0 * s;
            let menu_items = ["Commit", "Commit (Amend)", "Commit & Push"];
            let menu_h = item_h * menu_items.len() as f32 + 8.0 * s;
            let menu_y = commit_y + 30.0 * s;
            self.push_rounded_rect(
                menu_x,
                menu_y,
                menu_w,
                menu_h,
                8.0 * s,
                [0.18, 0.19, 0.25, 0.98],
            );
            self.push_rect(
                menu_x,
                menu_y + item_h * 2.0 + 4.0 * s,
                menu_w,
                1.0,
                [1.0, 1.0, 1.0, 0.14],
            );
            for (idx, label) in menu_items.iter().enumerate() {
                let item_y = menu_y + 4.0 * s + idx as f32 * item_h;
                let hovered = ui_registry.register_rect(
                    crate::ui_system::UiId::GitCommitMenuItem(idx),
                    menu_x,
                    item_y,
                    menu_w,
                    item_h,
                    mx,
                    my,
                );
                if hovered {
                    self.push_rounded_rect(
                        menu_x + 5.0 * s,
                        item_y + 3.0 * s,
                        menu_w - 10.0 * s,
                        item_h - 6.0 * s,
                        5.0 * s,
                        [1.0, 1.0, 1.0, 0.07],
                    );
                }
                self.draw_tree_label_clipped(
                    label,
                    menu_x + 16.0 * s,
                    item_y + item_h / 2.0 + 5.5 * s,
                    menu_w - 32.0 * s,
                    self.theme.fg,
                    0.9,
                    &mut label_scratch,
                );
            }
        }

        self.git_file_tooltip = git_file_tooltip;
    }

}
