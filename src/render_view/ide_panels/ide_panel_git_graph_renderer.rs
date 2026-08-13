#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_panel(
        &mut self,
        panel_x: f32,
        panel_w: f32,
        graph_y: f32,
        graph_h: f32,
        pad: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        scratch: &mut String,
    ) {
        if graph_h <= 20.0 * s {
            return;
        }
        self.push_rect(
            panel_x,
            graph_y,
            panel_w,
            graph_h,
            [
                self.theme.bg[0] + 0.018,
                self.theme.bg[1] + 0.020,
                self.theme.bg[2] + 0.026,
                1.0,
            ],
        );

        let header_h = 34.0 * s;
        self.push_rect(
            panel_x,
            graph_y,
            panel_w,
            header_h,
            [
                self.theme.bg[0] + 0.005,
                self.theme.bg[1] + 0.006,
                self.theme.bg[2] + 0.010,
                1.0,
            ],
        );
        let tab_clip_x = panel_x + pad;
        let tab_clip_w = (panel_w - pad * 2.0).max(0.0);
        let mut tab_x = tab_clip_x - ide_panel.git.graph_workspace_scroll_x.round();
        let tab_y = graph_y + 6.0 * s;
        let tab_h = 23.0 * s;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (graph_y + header_h);
            self.gl.scissor(
                tab_clip_x.round() as i32,
                scissor_y.max(0.0) as i32,
                tab_clip_w.round() as i32,
                header_h.round() as i32,
            );
        }
        for workspace in ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .filter(|workspace| workspace.repo_root.is_some())
        {
            let name = workspace
                .root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace");
            let tab_w = (self.measure_ui_width(name, 0.76) + 18.0 * s).max(48.0 * s);
            let active = ide_panel.git.graph_workspace_idx == Some(workspace.workspace_idx);
            let visible = tab_x + tab_w >= tab_clip_x && tab_x <= tab_clip_x + tab_clip_w;
            if visible {
                let hit_x = tab_x.max(tab_clip_x);
                let hit_w = (tab_x + tab_w).min(tab_clip_x + tab_clip_w) - hit_x;
                let hovered = ui_registry.register_rect(
                    crate::ui_system::UiId::GitGraphWorkspace(workspace.workspace_idx),
                    hit_x,
                    tab_y,
                    hit_w,
                    tab_h,
                    mx,
                    my,
                );
                if active || hovered {
                    self.push_rounded_rect(
                        tab_x,
                        tab_y,
                        tab_w,
                        tab_h,
                        4.0 * s,
                        if active {
                            [0.60, 0.35, 0.85, 0.28]
                        } else {
                            [1.0, 1.0, 1.0, 0.075]
                        },
                    );
                }
                self.draw_string_scaled(
                    name,
                    tab_x + 9.0 * s,
                    tab_y + tab_h / 2.0 + 4.5 * s,
                    if active {
                        self.theme.fg
                    } else {
                        [0.72, 0.76, 0.88, 0.72]
                    },
                    0.76,
                );
            }
            tab_x += tab_w + 6.0 * s;
        }
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let rows_y = graph_y + header_h;
        let rows_h = (graph_h - header_h).max(0.0);
        let commits = &ide_panel.git.graph_snapshot;
        if rows_h <= 0.0 {
            return;
        }
        if commits.is_empty() {
            let hint = if ide_panel.git.graph_pending {
                "Graph scan..."
            } else {
                ide_panel
                    .git
                    .graph_notice
                    .as_deref()
                    .unwrap_or("No commits")
            };
            let tw = self.measure_ui_width(hint, 0.82);
            self.draw_string_scaled(
                hint,
                panel_x + (panel_w - tw) / 2.0,
                rows_y + 28.0 * s,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.48],
                0.82,
            );
            return;
        }

        let row_h = crate::app::git_panel::GIT_GRAPH_ROW_H * s;
        let scroll = ide_panel.git.graph_scroll.current.round();
        let hover_settled = ide_panel.git.graph_scroll.is_settled();
        let rows_clip = crate::ui_system::UiClipRect::new(panel_x, rows_y, panel_w, rows_h);
        let first = (scroll / row_h).floor().max(0.0) as usize;
        let last = (((scroll + rows_h) / row_h).ceil() as usize + 1).min(commits.len());
        let active_workspace = ide_panel.git.graph_workspace_idx.unwrap_or(0);
        let mut row_hover_target = None;

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (rows_y + rows_h);
            self.gl.scissor(
                panel_x as i32,
                scissor_y.max(0.0) as i32,
                panel_w as i32,
                rows_h as i32,
            );
        }

        for idx in first..last {
            let commit = &commits[idx];
            let row_y = rows_y + idx as f32 * row_h - scroll;
            let hovered = hover_settled
                && ui_registry.register_rect_clipped(
                    crate::ui_system::UiId::GitGraphCommit(active_workspace, idx),
                    panel_x,
                    row_y,
                    panel_w,
                    row_h,
                    rows_clip,
                    mx,
                    my,
                );
            if hovered {
                row_hover_target = Some((
                    GitGraphTooltipTarget {
                        workspace_idx: active_workspace,
                        commit_idx: idx,
                    },
                    panel_x + panel_w,
                    my,
                ));
                self.push_rect(panel_x, row_y, panel_w, row_h, [1.0, 1.0, 1.0, 0.055]);
            }

            let circle_y = row_y + row_h / 2.0;
            let graph_layout = git_graph_row_layout(panel_x, pad, s, commit.column, &commit.lanes);
            let gutter_w = graph_layout.gutter_w;
            let lane_step = graph_layout.lane_step;
            let lane_start_x = graph_layout.lane_start_x;
            let text_x = graph_layout.text_x;
            let commit_x = lane_start_x + commit.column as f32 * lane_step;
            let graph_clip_right = panel_x + panel_w - 8.0 * s;
            for vertical_pass in [false, true] {
                for lane in &commit.lanes {
                    let is_vertical = matches!(
                        lane.kind,
                        crate::app::git_panel::GitGraphLaneKind::Vertical
                            | crate::app::git_panel::GitGraphLaneKind::VerticalTop
                            | crate::app::git_panel::GitGraphLaneKind::VerticalBottom
                    );
                    if is_vertical != vertical_pass {
                        continue;
                    }
                    let lane_x = lane_start_x + lane.column as f32 * lane_step;
                    let target_x = lane_start_x + lane.target_column as f32 * lane_step;
                    if lane_x > panel_x + pad + gutter_w {
                        continue;
                    }
                    if lane_x > graph_clip_right && target_x > graph_clip_right {
                        continue;
                    }
                    let color =
                        git_graph_lane_color(usize::from(lane.color_idx), 0.62, self.theme.sel);
                    match lane.kind {
                        crate::app::git_panel::GitGraphLaneKind::Vertical => {
                            let mut top = row_y;
                            let mut bottom = row_y + row_h;
                            if usize::from(lane.column) == commit.column {
                                if idx == 0 {
                                    top = circle_y;
                                }
                                if idx + 1 == commits.len() {
                                    bottom = circle_y;
                                }
                            }
                            self.push_git_graph_vertical_segment(lane_x, top, bottom, s, color);
                        }
                        crate::app::git_panel::GitGraphLaneKind::VerticalTop => {
                            let bottom = if usize::from(lane.column) == commit.column {
                                circle_y - 5.0 * s
                            } else {
                                circle_y
                            };
                            self.push_git_graph_vertical_segment(lane_x, row_y, bottom, s, color);
                        }
                        crate::app::git_panel::GitGraphLaneKind::VerticalBottom => {
                            let top = if usize::from(lane.column) == commit.column {
                                circle_y + 5.0 * s
                            } else {
                                circle_y
                            };
                            self.push_git_graph_vertical_segment(
                                lane_x,
                                top,
                                row_y + row_h,
                                s,
                                color,
                            );
                        }
                        crate::app::git_panel::GitGraphLaneKind::Shift => {
                            self.push_git_graph_shift_segment(
                                lane_x, target_x, row_y, row_h, s, color,
                            );
                        }
                        crate::app::git_panel::GitGraphLaneKind::ShiftToCommit => {
                            self.push_git_graph_shift_to_commit_segment(
                                lane_x, target_x, row_y, row_h, s, color,
                            );
                        }
                        crate::app::git_panel::GitGraphLaneKind::Parent => {
                            self.push_git_graph_parent_segment(
                                commit_x, target_x, row_y, row_h, s, color,
                            );
                        }
                    }
                }
            }
            let circle_color = git_graph_lane_color(commit.color_idx, 1.0, self.theme.sel);
            if commit.is_head {
                self.push_rounded_rect(
                    commit_x - 6.0 * s,
                    circle_y - 6.0 * s,
                    12.0 * s,
                    12.0 * s,
                    6.0 * s,
                    circle_color,
                );
                self.push_rounded_rect(
                    commit_x - 3.0 * s,
                    circle_y - 3.0 * s,
                    6.0 * s,
                    6.0 * s,
                    3.0 * s,
                    [
                        self.theme.bg[0] + 0.018,
                        self.theme.bg[1] + 0.020,
                        self.theme.bg[2] + 0.026,
                        1.0,
                    ],
                );
            } else {
                self.push_rounded_rect(
                    commit_x - 5.0 * s,
                    circle_y - 5.0 * s,
                    10.0 * s,
                    10.0 * s,
                    5.0 * s,
                    circle_color,
                );
            }

            let has_last_name = commit.author_name.split_whitespace().nth(1).is_some();
            let author_text_w = self.measure_ui_width(&commit.author_name, 0.78);
            let author_reserve_w = if has_last_name {
                118.0 * s
            } else {
                (author_text_w + 6.0 * s).clamp(48.0 * s, 92.0 * s)
            };
            let author_right_x = panel_x + panel_w - 30.0 * s;
            let author_draw_w = author_text_w.min(author_reserve_w);
            let author_x = (author_right_x - author_draw_w).max(text_x);
            let row_text_y = Self::tree_row_text_y(row_y, row_h, s);
            let local_ref_name = commit
                .local_refs
                .first()
                .map(|git_ref| git_ref.name.as_str());
            let remote_ref_name = commit
                .remote_refs
                .first()
                .map(|git_ref| git_ref.name.as_str());
            let chip_scale = 0.82;
            let chip_pad_x = 5.0 * s;
            let chip_gap = 5.0 * s;
            let chip_max_w = 140.0 * s;
            let local_chip_w = local_ref_name.map(|name| {
                branch_chip_width(
                    self.measure_ui_width(name, chip_scale),
                    chip_pad_x,
                    chip_max_w,
                )
            });
            let remote_chip_w = remote_ref_name.map(|name| {
                branch_chip_width(
                    self.measure_ui_width(name, chip_scale),
                    chip_pad_x,
                    chip_max_w,
                )
            });
            let mut chips_w = local_chip_w.unwrap_or(0.0) + remote_chip_w.unwrap_or(0.0);
            if local_chip_w.is_some() && remote_chip_w.is_some() {
                chips_w += chip_gap;
            }
            let row_available_w = (author_x - text_x - 12.0 * s).max(20.0 * s);
            let chips_visible = chips_w > 0.0 && row_available_w >= chips_w + 36.0 * s;
            let summary_max_w = if chips_visible {
                row_available_w - chips_w - 8.0 * s
            } else {
                row_available_w
            };
            let summary_w = self.draw_git_graph_label_clipped(
                &commit.summary,
                text_x,
                row_text_y,
                summary_max_w,
                self.theme.fg,
                0.82,
                scratch,
            );
            let row_text_center_y = self.ui_text_center_y(&commit.summary, row_text_y, 0.82);
            if chips_visible {
                let chip_h = 18.0 * s;
                let mut chip_x = (text_x + summary_w + 8.0 * s).round();
                if let (Some(name), Some(chip_w)) = (local_ref_name, local_chip_w) {
                    self.draw_git_graph_branch_chip(
                        name,
                        chip_x,
                        row_text_center_y,
                        chip_w,
                        chip_h,
                        4.0 * s,
                        [0.28, 0.24, 0.40, 1.0],
                        [0.86, 0.90, 1.0, 1.0],
                        chip_scale,
                        chip_pad_x,
                        false,
                        scratch,
                    );
                    chip_x += chip_w + chip_gap;
                }
                if let (Some(name), Some(chip_w)) = (remote_ref_name, remote_chip_w) {
                    self.draw_git_graph_branch_chip(
                        name,
                        chip_x,
                        row_text_center_y,
                        chip_w,
                        chip_h,
                        4.0 * s,
                        [0.24, 0.32, 0.42, 1.0],
                        [0.86, 0.90, 1.0, 1.0],
                        chip_scale,
                        chip_pad_x,
                        false,
                        scratch,
                    );
                }
            }
            self.draw_git_graph_label_clipped(
                &commit.author_name,
                author_x,
                row_text_y,
                author_draw_w,
                [0.72, 0.76, 0.88, 0.72],
                0.78,
                scratch,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let max_scroll = crate::app::git_panel::git_graph_max_scroll(commits.len(), rows_h, s);
        if max_scroll > 0.0 {
            let ratio = (scroll / max_scroll).clamp(0.0, 1.0);
            let thumb_h = crate::app::git_panel::git_graph_scroll_thumb_h(commits.len(), rows_h, s);
            let thumb_y = rows_y + 4.0 * s + ratio * (rows_h - 8.0 * s - thumb_h);
            let track_w = 10.0 * s;
            let track_x = panel_x + panel_w - track_w - 9.0 * s;
            ui_registry.register_rect(
                crate::ui_system::UiId::GitGraphScroll,
                track_x,
                rows_y,
                track_w,
                rows_h,
                mx,
                my,
            );
            self.push_rounded_rect(
                track_x + 2.0 * s,
                thumb_y,
                6.0 * s,
                thumb_h,
                3.0 * s,
                [1.0, 1.0, 1.0, 0.22],
            );
        }

        let mouse_in_commit_area =
            mx >= panel_x && mx <= panel_x + panel_w && my >= rows_y && my <= rows_y + rows_h;
        if let Some((target, anchor_x, anchor_y)) = row_hover_target {
            self.git_graph_tooltip =
                Some((target.workspace_idx, target.commit_idx, anchor_x, anchor_y));
        } else if let Some(hover) = self.git_graph_tooltip_hover
            && hover.workspace_idx == active_workspace
            && (hover.contains(mx, my) || self.git_graph_tooltip_selecting)
        {
            self.git_graph_tooltip = Some((
                hover.workspace_idx,
                hover.commit_idx,
                hover.anchor_x,
                hover.anchor_y,
            ));
        } else if !mouse_in_commit_area {
            self.git_graph_tooltip = None;
            self.git_graph_tooltip_hover = None;
            self.git_graph_tooltip_stable_w = 0.0;
        }
    }

}

#[cfg(test)]
mod git_graph_scroll_regression_tests {
    #[test]
    fn bug_32_git_graph_disables_row_hover_while_scroll_is_moving() {
        let source = include_str!("ide_panel_git_graph_renderer.rs");
        assert!(source.contains("let hover_settled = ide_panel.git.graph_scroll.is_settled();"));
        assert!(source.contains("let hovered = hover_settled"));
    }
}


fn git_log_semantic_color(
    theme: &crate::renderer::Theme,
    kind: crate::app::git_panel::GitLogKind,
) -> [f32; 4] {
    match kind {
        crate::app::git_panel::GitLogKind::Header => [0.72, 0.76, 0.90, 0.92],
        crate::app::git_panel::GitLogKind::Stdout => {
            [theme.fg[0], theme.fg[1], theme.fg[2], 0.86]
        }
        crate::app::git_panel::GitLogKind::Stderr => [0.96, 0.72, 0.40, 0.95],
        crate::app::git_panel::GitLogKind::Hook => [0.60, 0.72, 1.00, 0.96],
        crate::app::git_panel::GitLogKind::Success => [0.42, 0.84, 0.50, 0.96],
        crate::app::git_panel::GitLogKind::Failure => [0.96, 0.42, 0.46, 0.98],
        crate::app::git_panel::GitLogKind::Info => [0.60, 0.62, 0.70, 0.88],
    }
}

fn git_log_prefix(kind: crate::app::git_panel::GitLogKind) -> &'static str {
    match kind {
        crate::app::git_panel::GitLogKind::Stdout => "> ",
        crate::app::git_panel::GitLogKind::Stderr => "! ",
        _ => "",
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    fn draw_git_logs_panel(
        &mut self,
        panel_x: f32,
        panel_w: f32,
        logs_y: f32,
        logs_h: f32,
        pad: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        if logs_h <= 20.0 * s {
            return;
        }
        self.push_rect(
            panel_x,
            logs_y,
            panel_w,
            logs_h,
            [
                self.theme.bg[0] + 0.018,
                self.theme.bg[1] + 0.020,
                self.theme.bg[2] + 0.026,
                1.0,
            ],
        );

        let toolbar_h = crate::app::git_panel::GIT_LOG_TOOLBAR_H * s;
        self.push_rect(
            panel_x,
            logs_y,
            panel_w,
            toolbar_h,
            [
                self.theme.bg[0] + 0.005,
                self.theme.bg[1] + 0.006,
                self.theme.bg[2] + 0.010,
                1.0,
            ],
        );
        self.draw_string_scaled(
            "VCS Console",
            panel_x + pad,
            logs_y + toolbar_h / 2.0 + 5.0 * s,
            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.82],
            0.80,
        );

        let clear_w = (self.measure_ui_width("Очистить", 0.76) + 18.0 * s).max(58.0 * s);
        let clear_h = 22.0 * s;
        let clear_x = panel_x + panel_w - pad - clear_w;
        let clear_y = logs_y + (toolbar_h - clear_h) / 2.0;
        let clear_hovered = ui_registry.register_rect(
            crate::ui_system::UiId::GitLogsClear,
            clear_x,
            clear_y,
            clear_w,
            clear_h,
            mx,
            my,
        );
        if clear_hovered {
            self.push_rounded_rect(
                clear_x,
                clear_y,
                clear_w,
                clear_h,
                4.0 * s,
                [1.0, 1.0, 1.0, 0.07],
            );
        }
        self.draw_string_scaled(
            "Очистить",
            clear_x + 9.0 * s,
            clear_y + clear_h / 2.0 + 4.5 * s,
            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.78],
            0.76,
        );

        let rows_y = logs_y + toolbar_h;
        let rows_h = (logs_h - toolbar_h).max(0.0);
        ui_registry.register_blocker(
            crate::ui_system::UiId::GitLogsBody,
            panel_x,
            rows_y,
            panel_w,
            rows_h,
            mx,
            my,
        );
        if rows_h <= 1.0 {
            return;
        }

        let row_h = crate::app::git_panel::GIT_LOG_ROW_H * s;
        let line_count = ide_panel.git.git_logs.line_count();
        let total_h = line_count as f32 * row_h;
        let max_scroll = (total_h - rows_h).max(0.0);
        let scroll = ide_panel.git.logs_scroll.current.clamp(0.0, max_scroll);
        let render_scroll = scroll.round();
        let first = (render_scroll / row_h).floor().max(0.0) as usize;
        let visible_count = (rows_h / row_h).ceil() as usize + 2;
        let last = (first + visible_count).min(line_count);
        let text_x = panel_x + pad;
        let text_scale = 0.78;

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (rows_y + rows_h);
            self.gl.scissor(
                panel_x.round() as i32,
                scissor_y.max(0.0).round() as i32,
                panel_w.max(0.0).round() as i32,
                rows_h.max(0.0).round() as i32,
            );
        }

        for line_idx in first..last {
            let Some(line) = ide_panel.git.git_logs.line_at(line_idx) else {
                continue;
            };
            let baseline = rows_y.round()
                + (line_idx as f32 * row_h).round()
                - render_scroll
                + (row_h * 0.70).round();
            let kind = line.kind();
            let semantic = git_log_semantic_color(&self.theme, kind);
            let prefix = git_log_prefix(kind);
            let mut x = text_x;
            if !prefix.is_empty() {
                self.draw_string_scaled(prefix, x, baseline, semantic, text_scale);
                x += self.measure_ui_width(prefix, text_scale);
            }
            match line.spans() {
                crate::app::git_panel::GitLogSpansRef::TruncationMarker => {
                    self.draw_string_scaled(
                        crate::app::git_panel::GIT_LOG_TRUNCATION_MARKER,
                        x,
                        baseline,
                        semantic,
                        text_scale,
                    );
                }
                crate::app::git_panel::GitLogSpansRef::Line(spans) => {
                    for span in spans {
                        if x >= panel_x + panel_w - pad {
                            break;
                        }
                        let color = span
                            .ansi_fg
                            .and_then(|index| {
                                crate::app::terminal::ANSI_16_COLORS.get(index as usize).copied()
                            })
                            .unwrap_or(semantic);
                        self.draw_string_scaled(&span.text, x, baseline, color, text_scale);
                        x += self.measure_ui_width(&span.text, text_scale);
                    }
                }
            }
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        if let Some(thumb) = crate::scroll::scrollbar_thumb(
            rows_y + 4.0 * s,
            (rows_h - 8.0 * s).max(1.0),
            rows_h,
            total_h,
            scroll,
            10.0 * s,
        ) {
            self.push_rounded_rect(
                panel_x + panel_w - 5.0 * s,
                thumb.start,
                3.0 * s,
                thumb.len,
                1.5 * s,
                [1.0, 1.0, 1.0, 0.22],
            );
        }

        if ide_panel.git.git_logs.is_empty() {
            let message = "Здесь появятся логи commit-операций";
            let width = self.measure_ui_width(message, 0.80);
            self.draw_string_scaled(
                message,
                panel_x + ((panel_w - width) / 2.0).max(pad),
                rows_y + 30.0 * s,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.44],
                0.80,
            );
        }
    }
}

#[cfg(test)]
mod git_logs_renderer_tests {
    #[test]
    fn semantic_prefixes_do_not_infer_errors_from_output_text() {
        use crate::app::git_panel::GitLogKind;
        assert_eq!(super::git_log_prefix(GitLogKind::Stdout), "> ");
        assert_eq!(super::git_log_prefix(GitLogKind::Stderr), "! ");
        assert_eq!(super::git_log_prefix(GitLogKind::Failure), "");
        assert_eq!(super::git_log_prefix(GitLogKind::Success), "");
    }
}
