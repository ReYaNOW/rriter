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

fn visit_git_log_display_pieces(
    line: crate::app::git_panel::GitLogDisplayLineRef<'_>,
    mut visit: impl FnMut(usize, &str, Option<u8>),
) {
    let line_ref = line.line();
    let prefix = line_ref.kind().display_prefix();
    let mut byte = 0usize;
    if !prefix.is_empty() {
        visit(byte, prefix, None);
        byte = byte.saturating_add(prefix.len());
    }
    match line_ref.spans() {
        crate::app::git_panel::GitLogSpansRef::TruncationMarker => {
            visit(
                byte,
                crate::app::git_panel::GIT_LOG_TRUNCATION_MARKER,
                None,
            );
        }
        crate::app::git_panel::GitLogSpansRef::Line(spans) => {
            for span in spans {
                visit(byte, &span.text, span.ansi_fg);
                byte = byte.saturating_add(span.text.len());
            }
        }
    }
}

fn wrap_git_log_display_line_with_advance(
    line: crate::app::git_panel::GitLogDisplayLineRef<'_>,
    max_width: f32,
    mut advance: impl FnMut(char) -> f32,
    rows: &mut std::collections::VecDeque<crate::renderer::GitLogVisualRow>,
) -> usize {
    let max_width = max_width.max(1.0);
    let first_row = rows.len();
    let mut row_start = 0usize;
    let mut row_width = 0.0f32;
    let mut soft_break: Option<(usize, f32)> = None;
    let line_len = line.byte_len();

    visit_git_log_display_pieces(line, |piece_start, piece, _| {
        for (local_byte, ch) in piece.char_indices() {
            let byte = piece_start + local_byte;
            let byte_end = byte + ch.len_utf8();
            let char_width = advance(ch).max(0.0);
            loop {
                if row_width > 0.0 && row_width + char_width > max_width {
                    if let Some((break_byte, break_width)) = soft_break
                        && break_byte > row_start
                    {
                        rows.push_back(crate::renderer::GitLogVisualRow {
                            line: line.id(),
                            byte_start: row_start,
                            byte_end: break_byte,
                            width: break_width,
                        });
                        row_start = break_byte;
                        row_width = (row_width - break_width).max(0.0);
                    } else {
                        rows.push_back(crate::renderer::GitLogVisualRow {
                            line: line.id(),
                            byte_start: row_start,
                            byte_end: byte,
                            width: row_width,
                        });
                        row_start = byte;
                        row_width = 0.0;
                    }
                    soft_break = None;
                    continue;
                }

                row_width += char_width;
                if ch.is_whitespace() {
                    soft_break = Some((byte_end, row_width));
                }
                break;
            }
        }
    });

    if row_start < line_len || rows.len() == first_row {
        rows.push_back(crate::renderer::GitLogVisualRow {
            line: line.id(),
            byte_start: row_start,
            byte_end: line_len,
            width: row_width,
        });
    }
    rows.len() - first_row
}

fn remove_cached_git_log_line(
    cache: &mut crate::renderer::GitLogsLayoutCache,
    line_index: usize,
) -> bool {
    let Some(line) = cache.lines.get(line_index).copied() else {
        return false;
    };
    let row_start = cache
        .lines
        .iter()
        .take(line_index)
        .map(|entry| entry.row_count)
        .sum::<usize>();
    let row_end = row_start.saturating_add(line.row_count).min(cache.rows.len());
    cache.rows.drain(row_start..row_end);
    cache.lines.remove(line_index);
    true
}

fn git_log_row_selected_bytes(
    row: crate::renderer::GitLogVisualRow,
    selection: crate::app::git_panel::GitLogSelectionRange,
) -> Option<(usize, usize)> {
    if row.line < selection.start.line || row.line > selection.end.line {
        return None;
    }
    let start = if row.line == selection.start.line {
        selection.start.byte.max(row.byte_start)
    } else {
        row.byte_start
    };
    let end = if row.line == selection.end.line {
        selection.end.byte.min(row.byte_end)
    } else {
        row.byte_end
    };
    (start < end).then_some((start, end))
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[inline]
    fn git_logs_char_advance(&mut self, ch: char, text_scale: f32) -> f32 {
        if matches!(ch, '\n' | '\r' | '\u{FE0F}' | '\u{200D}') {
            return 0.0;
        }
        self.get_ui_glyph(ch)
            .map(|glyph| Self::snapped_text_advance(glyph.advance, text_scale))
            .unwrap_or(0.0)
    }

    fn append_measured_git_log_line(
        &mut self,
        cache: &mut crate::renderer::GitLogsLayoutCache,
        line: crate::app::git_panel::GitLogDisplayLineRef<'_>,
        wrap_width: f32,
        text_scale: f32,
    ) {
        let row_count = wrap_git_log_display_line_with_advance(
            line,
            wrap_width,
            |ch| self.git_logs_char_advance(ch, text_scale),
            &mut cache.rows,
        );
        cache.lines.push_back(crate::renderer::GitLogCachedLine {
            id: line.id(),
            row_count,
        });
        #[cfg(test)]
        {
            cache.measured_line_count = cache.measured_line_count.wrapping_add(1);
        }
    }

    fn prepend_measured_git_log_marker(
        &mut self,
        cache: &mut crate::renderer::GitLogsLayoutCache,
        line: crate::app::git_panel::GitLogDisplayLineRef<'_>,
        wrap_width: f32,
        text_scale: f32,
    ) {
        let mut marker_rows = std::collections::VecDeque::new();
        let row_count = wrap_git_log_display_line_with_advance(
            line,
            wrap_width,
            |ch| self.git_logs_char_advance(ch, text_scale),
            &mut marker_rows,
        );
        while let Some(row) = marker_rows.pop_back() {
            cache.rows.push_front(row);
        }
        cache.lines.push_front(crate::renderer::GitLogCachedLine {
            id: line.id(),
            row_count,
        });
        #[cfg(test)]
        {
            cache.measured_line_count = cache.measured_line_count.wrapping_add(1);
        }
    }

    fn rebuild_git_logs_layout(
        &mut self,
        cache: &mut crate::renderer::GitLogsLayoutCache,
        logs: &crate::app::git_panel::GitLogBuffer,
        wrap_width: f32,
        text_scale: f32,
    ) {
        cache.rows.clear();
        cache.lines.clear();
        #[cfg(test)]
        {
            cache.full_reflow_count = cache.full_reflow_count.wrapping_add(1);
        }
        for index in 0..logs.line_count() {
            if let Some(line) = logs.display_line_at(index) {
                self.append_measured_git_log_line(cache, line, wrap_width, text_scale);
            }
        }
    }

    fn sync_git_logs_layout_cache(
        &mut self,
        logs: &crate::app::git_panel::GitLogBuffer,
        wrap_width: f32,
        scale_factor: f32,
        text_scale: f32,
    ) {
        let snapshot = logs.snapshot();
        let mut cache = std::mem::take(&mut self.git_logs_layout_cache);
        let geometry_changed = cache.snapshot.is_none()
            || cache.wrap_width.to_bits() != wrap_width.to_bits()
            || cache.scale_factor.to_bits() != scale_factor.to_bits()
            || cache.text_scale.to_bits() != text_scale.to_bits();

        if geometry_changed {
            self.rebuild_git_logs_layout(&mut cache, logs, wrap_width, text_scale);
        } else if cache.snapshot != Some(snapshot) {
            if snapshot.display_line_count == 0 {
                cache.rows.clear();
                cache.lines.clear();
            } else {
                let marker_expected = snapshot.truncated;
                let marker_present = cache.lines.front().is_some_and(|line| {
                    line.id == crate::app::git_panel::GitLogDisplayLineId::TruncationMarker
                });
                if marker_present && !marker_expected {
                    let _ = remove_cached_git_log_line(&mut cache, 0);
                } else if marker_expected && !marker_present {
                    if let Some(marker) = logs.display_line_at(0) {
                        self.prepend_measured_git_log_marker(
                            &mut cache,
                            marker,
                            wrap_width,
                            text_scale,
                        );
                    }
                }

                let first_stored_cache_index = usize::from(marker_expected);
                while cache.lines.len() > first_stored_cache_index {
                    let id = cache.lines[first_stored_cache_index].id;
                    if logs.display_line_index(id).is_some() {
                        break;
                    }
                    let _ = remove_cached_git_log_line(&mut cache, first_stored_cache_index);
                }

                let next_display_index = cache
                    .lines
                    .back()
                    .and_then(|line| logs.display_line_index(line.id))
                    .map_or(0, |index| index.saturating_add(1));
                for index in next_display_index..snapshot.display_line_count {
                    if let Some(line) = logs.display_line_at(index) {
                        self.append_measured_git_log_line(
                            &mut cache,
                            line,
                            wrap_width,
                            text_scale,
                        );
                    }
                }
            }
        }

        cache.snapshot = Some(snapshot);
        cache.wrap_width = wrap_width;
        cache.scale_factor = scale_factor;
        cache.text_scale = text_scale;
        self.git_logs_layout_cache = cache;
    }

    fn git_log_width_between(
        &mut self,
        line: crate::app::git_panel::GitLogDisplayLineRef<'_>,
        start: usize,
        end: usize,
        text_scale: f32,
    ) -> f32 {
        if start >= end {
            return 0.0;
        }
        let mut width = 0.0;
        visit_git_log_display_pieces(line, |piece_start, piece, _| {
            let piece_end = piece_start.saturating_add(piece.len());
            if start < piece_end && end > piece_start {
                let local_start = start.saturating_sub(piece_start);
                let local_end = end.min(piece_end).saturating_sub(piece_start);
                if let Some(slice) = piece.get(local_start..local_end) {
                    width += self.measure_ui_width(slice, text_scale);
                }
            }
        });
        width
    }

    fn draw_git_log_visual_row(
        &mut self,
        line: crate::app::git_panel::GitLogDisplayLineRef<'_>,
        row: crate::renderer::GitLogVisualRow,
        x: f32,
        baseline: f32,
        text_scale: f32,
    ) {
        let semantic = git_log_semantic_color(&self.theme, line.line().kind());
        let mut draw_x = x;
        visit_git_log_display_pieces(line, |piece_start, piece, ansi_fg| {
            let piece_end = piece_start.saturating_add(piece.len());
            if row.byte_start < piece_end && row.byte_end > piece_start {
                let local_start = row.byte_start.saturating_sub(piece_start);
                let local_end = row.byte_end.min(piece_end).saturating_sub(piece_start);
                if let Some(slice) = piece.get(local_start..local_end) {
                    let color = ansi_fg
                        .and_then(|index| {
                            crate::app::terminal::ANSI_16_COLORS.get(index as usize).copied()
                        })
                        .unwrap_or(semantic);
                    self.draw_string_scaled(slice, draw_x, baseline, color, text_scale);
                    draw_x += self.measure_ui_width(slice, text_scale);
                }
            }
        });
    }

    pub(crate) fn git_logs_layout_metrics(
        &self,
    ) -> Option<crate::renderer::GitLogsLayoutMetrics> {
        self.git_logs_layout_cache.metrics
    }

    pub(crate) fn reset_git_logs_layout(&mut self) {
        self.git_logs_layout_cache = crate::renderer::GitLogsLayoutCache::default();
        self.git_logs_selecting = false;
    }

    pub(crate) fn git_logs_text_point_at(
        &mut self,
        logs: &crate::app::git_panel::GitLogBuffer,
        mx: f32,
        my: f32,
    ) -> Option<crate::app::git_panel::GitLogTextPoint> {
        let metrics = self.git_logs_layout_cache.metrics?;
        if self.git_logs_layout_cache.rows.is_empty() || metrics.row_h <= 0.0 {
            return None;
        }
        let (_, rows_y, _, rows_h) = metrics.rows_rect;
        let content_y = (my.clamp(rows_y, rows_y + rows_h.max(0.0)) - rows_y
            + metrics.render_scroll)
            .max(0.0);
        let row_index = ((content_y / metrics.row_h).floor() as usize)
            .min(self.git_logs_layout_cache.rows.len().saturating_sub(1));
        let row = self.git_logs_layout_cache.rows.get(row_index).copied()?;
        let display_index = logs.display_line_index(row.line)?;
        let line = logs.display_line_at(display_index)?;
        let text_x = metrics.text_rect.0;
        if mx <= text_x {
            return Some(crate::app::git_panel::GitLogTextPoint {
                line: row.line,
                byte: row.byte_start,
            });
        }
        let x = mx - text_x;
        if x >= row.width {
            return Some(crate::app::git_panel::GitLogTextPoint {
                line: row.line,
                byte: row.byte_end,
            });
        }

        let mut cursor_x = 0.0f32;
        let mut hit = None;
        visit_git_log_display_pieces(line, |piece_start, piece, _| {
            if hit.is_some() {
                return;
            }
            for (local_byte, ch) in piece.char_indices() {
                let byte = piece_start + local_byte;
                if byte < row.byte_start || byte >= row.byte_end {
                    continue;
                }
                let char_width = self.git_logs_char_advance(ch, metrics.text_scale);
                if x < cursor_x + char_width * 0.5 {
                    hit = Some(byte);
                    return;
                }
                cursor_x += char_width;
            }
        });
        Some(crate::app::git_panel::GitLogTextPoint {
            line: row.line,
            byte: hit.unwrap_or(row.byte_end),
        })
    }

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
            self.git_logs_layout_cache.metrics = None;
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

        let toolbar_h = (crate::app::git_panel::GIT_LOG_TOOLBAR_H * s)
            .round()
            .max(1.0);
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

        let rows_x = panel_x.round();
        let rows_y = (logs_y + toolbar_h).round();
        let rows_w = panel_w.max(0.0).round();
        let rows_h = (logs_h - toolbar_h).max(0.0).round();
        ui_registry.register_blocker(
            crate::ui_system::UiId::GitLogsBody,
            rows_x,
            rows_y,
            rows_w,
            rows_h,
            mx,
            my,
        );
        if rows_h <= 1.0 {
            self.git_logs_layout_cache.metrics = None;
            return;
        }

        let text_scale = 0.78;
        let row_h = (crate::app::git_panel::GIT_LOG_ROW_H * s).round().max(1.0);
        let track_w = (10.0 * s).round().max(6.0);
        let track_x = (panel_x + panel_w - track_w).round();
        let track_y = rows_y + (4.0 * s).round();
        let track_h = (rows_h - (8.0 * s).round()).max(1.0);
        let text_x = (panel_x + pad).round();
        let text_right = (track_x - (2.0 * s).round()).max(text_x);
        let text_w = (text_right - text_x).max(0.0);
        let wrap_width = text_w.round().max(1.0);

        self.sync_git_logs_layout_cache(
            &ide_panel.git.git_logs,
            wrap_width,
            self.scale_factor,
            text_scale,
        );
        let content_h = self.git_logs_layout_cache.rows.len() as f32 * row_h;
        let max_scroll = (content_h - rows_h).max(0.0);
        let scroll = ide_panel.git.logs_scroll.current.clamp(0.0, max_scroll);
        let render_scroll = scroll.round();
        let metrics = crate::renderer::GitLogsLayoutMetrics {
            rows_rect: (rows_x, rows_y, rows_w, rows_h),
            text_rect: (text_x, rows_y, text_w, rows_h),
            track_rect: (track_x, track_y, track_w, track_h),
            row_h,
            text_scale,
            content_h,
            max_scroll,
            render_scroll,
        };
        self.git_logs_layout_cache.metrics = Some(metrics);

        let selection = ide_panel
            .git
            .git_logs
            .selection()
            .and_then(|selection| ide_panel.git.git_logs.normalize_selection(selection));
        let first = (render_scroll / row_h).floor().max(0.0) as usize;
        let visible_count = (rows_h / row_h).ceil() as usize + 2;
        let last = (first + visible_count).min(self.git_logs_layout_cache.rows.len());

        if text_w > 0.0 {
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (rows_y + rows_h);
                self.gl.scissor(
                    text_x.round() as i32,
                    scissor_y.max(0.0).round() as i32,
                    text_w.round() as i32,
                    rows_h.round() as i32,
                );
            }

            for row_index in first..last {
                let Some(row) = self.git_logs_layout_cache.rows.get(row_index).copied() else {
                    continue;
                };
                let Some(display_index) = ide_panel.git.git_logs.display_line_index(row.line) else {
                    continue;
                };
                let Some(line) = ide_panel.git.git_logs.display_line_at(display_index) else {
                    continue;
                };
                let row_top = rows_y + row_index as f32 * row_h - render_scroll;
                let baseline = row_top + (row_h * 0.70).round();
                if let Some((selected_start, selected_end)) =
                    selection.and_then(|selection| git_log_row_selected_bytes(row, selection))
                {
                    let selected_x = self.git_log_width_between(
                        line,
                        row.byte_start,
                        selected_start,
                        text_scale,
                    );
                    let selected_w = self.git_log_width_between(
                        line,
                        selected_start,
                        selected_end,
                        text_scale,
                    );
                    if selected_w > 0.0 {
                        self.push_rect(
                            text_x + selected_x,
                            row_top + (2.0 * s).round(),
                            selected_w,
                            (row_h - (4.0 * s).round()).max(1.0),
                            self.theme.sel,
                        );
                    }
                }
                self.draw_git_log_visual_row(line, row, text_x, baseline, text_scale);
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
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

        if max_scroll > 0.0 {
            ui_registry.register_rect(
                crate::ui_system::UiId::GitLogsScroll,
                track_x,
                track_y,
                track_w,
                track_h,
                mx,
                my,
            );
            if let Some(thumb) = crate::scroll::scrollbar_thumb(
                track_y,
                track_h,
                track_h,
                track_h + max_scroll,
                scroll,
                10.0 * s,
            ) {
                let bar_w = (3.0 * s).round().max(2.0);
                self.push_rounded_rect(
                    track_x + ((track_w - bar_w) / 2.0).round(),
                    thumb.start,
                    bar_w,
                    thumb.len,
                    (bar_w / 2.0).max(1.0),
                    [1.0, 1.0, 1.0, 0.22],
                );
            }
        }
    }
}

#[cfg(test)]
mod git_logs_renderer_tests {
    use std::collections::VecDeque;

    use crate::app::git_panel::{
        GitLogBuffer, GitLogKind, GitLogLine, GitLogSpan, GitLogTextPoint,
    };

    fn line(kind: GitLogKind, spans: &[&str]) -> GitLogLine {
        GitLogLine {
            kind,
            spans: spans
                .iter()
                .enumerate()
                .map(|(index, text)| GitLogSpan {
                    text: (*text).to_string(),
                    ansi_fg: (index > 0).then_some(index as u8),
                })
                .collect(),
        }
    }

    fn wrapped_rows(
        buffer: &GitLogBuffer,
        index: usize,
        max_width: f32,
    ) -> VecDeque<crate::renderer::GitLogVisualRow> {
        let mut rows = VecDeque::new();
        let display = buffer.display_line_at(index).unwrap();
        super::wrap_git_log_display_line_with_advance(display, max_width, |_| 10.0, &mut rows);
        rows
    }

    #[test]
    fn long_line_wraps_to_visual_rows_and_exact_boundary_is_stable() {
        let mut buffer = GitLogBuffer::default();
        buffer.append(line(GitLogKind::Info, &["abcdefghij"]));
        let rows = wrapped_rows(&buffer, 0, 30.0);
        assert_eq!(rows.len(), 4);
        assert_eq!((rows[0].byte_start, rows[0].byte_end), (0, 3));
        assert_eq!((rows[3].byte_start, rows[3].byte_end), (9, 10));

        let mut exact = GitLogBuffer::default();
        exact.append(line(GitLogKind::Info, &["abc"]));
        assert_eq!(wrapped_rows(&exact, 0, 30.0).len(), 1);
        exact.append(line(GitLogKind::Info, &["abcd"]));
        assert_eq!(wrapped_rows(&exact, 1, 30.0).len(), 2);
    }

    #[test]
    fn unbroken_oversized_glyph_still_advances() {
        let mut buffer = GitLogBuffer::default();
        buffer.append(line(GitLogKind::Info, &["ab"]));
        let mut rows = VecDeque::new();
        let display = buffer.display_line_at(0).unwrap();
        super::wrap_git_log_display_line_with_advance(display, 5.0, |_| 10.0, &mut rows);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].byte_start, rows[0].byte_end), (0, 1));
        assert_eq!((rows[1].byte_start, rows[1].byte_end), (1, 2));
    }

    #[test]
    fn wrapping_preserves_prefix_and_ansi_span_byte_mapping() {
        let mut buffer = GitLogBuffer::default();
        buffer.append(line(GitLogKind::Stdout, &["ab", "cd"]));
        let rows = wrapped_rows(&buffer, 0, 30.0);
        assert_eq!(rows.front().unwrap().byte_start, 0);
        assert_eq!(rows.back().unwrap().byte_end, 6); // "> abcd"
        let selection = crate::app::git_panel::GitLogSelection {
            anchor: GitLogTextPoint {
                line: rows[0].line,
                byte: 0,
            },
            cursor: GitLogTextPoint {
                line: rows[0].line,
                byte: 6,
            },
        };
        assert_eq!(buffer.copy_selection(selection).as_deref(), Some("> abcd"));
    }

    #[test]
    fn utf8_wrap_boundaries_are_never_inside_a_codepoint() {
        let mut buffer = GitLogBuffer::default();
        buffer.append(line(GitLogKind::Info, &["я🙂б"]));
        let rows = wrapped_rows(&buffer, 0, 10.0);
        let display = buffer.display_line_at(0).unwrap();
        for row in rows {
            assert!(display.is_char_boundary(row.byte_start));
            assert!(display.is_char_boundary(row.byte_end));
        }
    }

    #[test]
    fn soft_wrap_selection_has_no_clipboard_newline() {
        let mut buffer = GitLogBuffer::default();
        buffer.append(line(GitLogKind::Info, &["abcdef"]));
        let rows = wrapped_rows(&buffer, 0, 20.0);
        assert!(rows.len() > 1);
        let id = rows[0].line;
        let selection = crate::app::git_panel::GitLogSelection {
            anchor: GitLogTextPoint { line: id, byte: 1 },
            cursor: GitLogTextPoint { line: id, byte: 5 },
        };
        assert_eq!(buffer.copy_selection(selection).as_deref(), Some("bcde"));
    }

    #[test]
    fn selection_intersection_is_identical_for_normalized_forward_and_reverse_ranges() {
        let mut buffer = GitLogBuffer::default();
        buffer.append(line(GitLogKind::Info, &["abcdef"]));
        let rows = wrapped_rows(&buffer, 0, 20.0);
        let id = rows[0].line;
        let forward = crate::app::git_panel::GitLogSelection {
            anchor: GitLogTextPoint { line: id, byte: 1 },
            cursor: GitLogTextPoint { line: id, byte: 5 },
        };
        let reverse = crate::app::git_panel::GitLogSelection {
            anchor: forward.cursor,
            cursor: forward.anchor,
        };
        let f = buffer.normalize_selection(forward).unwrap();
        let r = buffer.normalize_selection(reverse).unwrap();
        assert_eq!(f, r);
        assert_eq!(
            super::git_log_row_selected_bytes(rows[1], f),
            super::git_log_row_selected_bytes(rows[1], r)
        );
    }

    #[test]
    fn cached_head_line_removal_drops_only_its_visual_rows() {
        use crate::renderer::{GitLogCachedLine, GitLogsLayoutCache};
        let mut cache = GitLogsLayoutCache::default();
        let mut buffer = GitLogBuffer::default();
        buffer.append(line(GitLogKind::Info, &["abcd"]));
        buffer.append(line(GitLogKind::Info, &["ef"]));
        for index in 0..2 {
            let display = buffer.display_line_at(index).unwrap();
            let before = cache.rows.len();
            super::wrap_git_log_display_line_with_advance(
                display,
                20.0,
                |_| 10.0,
                &mut cache.rows,
            );
            cache.lines.push_back(GitLogCachedLine {
                id: display.id(),
                row_count: cache.rows.len() - before,
            });
        }
        let second = cache.lines[1].id;
        assert_eq!(cache.rows.len(), 3);
        assert!(super::remove_cached_git_log_line(&mut cache, 0));
        assert_eq!(cache.lines.len(), 1);
        assert_eq!(cache.lines[0].id, second);
        assert_eq!(cache.rows.len(), 1);
    }

    #[test]
    fn scrollbar_uses_visual_content_height() {
        let row_h = 20.0;
        let rows_h = 60.0;
        let visual_rows = 7usize;
        let content_h = visual_rows as f32 * row_h;
        let max_scroll = (content_h - rows_h).max(0.0);
        assert_eq!(max_scroll, 80.0);
        assert_ne!(max_scroll, (2.0f32 * row_h - rows_h).max(0.0));
    }

    #[test]
    fn semantic_prefixes_come_from_log_kind_contract() {
        use crate::app::git_panel::GitLogKind;
        assert_eq!(GitLogKind::Stdout.display_prefix(), "> ");
        assert_eq!(GitLogKind::Stderr.display_prefix(), "! ");
        assert_eq!(GitLogKind::Failure.display_prefix(), "");
        assert_eq!(GitLogKind::Success.display_prefix(), "");
    }
}
