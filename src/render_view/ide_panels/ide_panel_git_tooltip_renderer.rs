struct CommitDetailsTextRow {
    x: f32,
    top: f32,
    line_h: f32,
    scale: f32,
    color: [f32; 4],
    mono: bool,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_commit_details_text_row(
        &mut self,
        text: &str,
        row: CommitDetailsTextRow,
        graph_selectable: bool,
    ) {
        if graph_selectable {
            let start = self.push_git_graph_tooltip_text_row(text, row.x, row.top, row.line_h, row.scale, row.mono);
            self.draw_git_graph_selectable_text(
                text,
                row.x,
                row.top + row.line_h * 0.62,
                row.color,
                row.scale,
                start,
                row.top,
                row.line_h,
                row.mono,
            );
        } else if row.mono {
            self.draw_string_mono_scaled(text, row.x.round(), (row.top + row.line_h * 0.62).round(), row.color, row.scale);
        } else {
            self.draw_string_scaled_stable(text, row.x.round(), (row.top + row.line_h * 0.62).round(), row.color, row.scale);
        }
    }

    pub(crate) fn draw_git_blame_commit_popup(
        &mut self,
        editor: &crate::editor::Editor,
        dwell: &crate::app::git_blame::InlineBlameDwell,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let annotation = ui_registry.rect_for(crate::ui_system::UiId::EditorBlameInline);
        let Some(current_key) = dwell.key else {
            self.git_blame_popup_hover = None;
            self.git_blame_popup_copied = None;
            self.git_blame_popup_details = None;
            return;
        };
        let hovered = self.git_blame_popup_hover.is_some_and(|hover| {
            if hover.key != current_key { return false; }
            let in_rect = |rect: (f32, f32, f32, f32)| {
                mx >= rect.0 && mx <= rect.0 + rect.2 && my >= rect.1 && my <= rect.1 + rect.3
            };
            in_rect(hover.popup) || in_rect(hover.annotation) || annotation.is_some_and(in_rect)
        });
        let Some(annotation) = annotation.filter(|_| {
            hovered || ui_registry.hovered() == Some(crate::ui_system::UiId::EditorBlameInline)
        }) else {
            self.git_blame_popup_hover = None;
            self.git_blame_popup_copied = None;
            self.git_blame_popup_details = None;
            return;
        };
        let active_commit = dwell.line.and_then(|line| {
            if current_key.generation != editor.git_blame.generation {
                return None;
            }
            let head_line = crate::editor::head_line_for(&editor.git_hunks, line)?;
            let blame = editor.git_blame.blame.as_ref()?;
            let commit_idx = *blame.line_commit.get(head_line)? as usize;
            blame.commits.get(commit_idx).filter(|commit| !commit.uncommitted)
        });
        let Some(commit) = active_commit else {
            self.git_blame_popup_hover = None;
            self.git_blame_popup_copied = None;
            self.git_blame_popup_details = None;
            return;
        };
        self.draw_git_blame_commit_popup_at(editor, commit, annotation, current_key, ui_registry, mx, my, s);
    }

    pub(crate) fn draw_git_blame_column_popup(
        &mut self,
        editor: &crate::editor::Editor,
        active_tab: usize,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let hovered_line = match ui_registry.hovered() {
            Some(crate::ui_system::UiId::EditorBlameColumnRow(line)) => Some(line),
            _ => None,
        };
        let existing = self.git_blame_popup_hover;
        let pointer_in_popup = existing.is_some_and(|hover| {
            mx >= hover.popup.0 && mx <= hover.popup.0 + hover.popup.2
                && my >= hover.popup.1 && my <= hover.popup.1 + hover.popup.3
        });
        let line = hovered_line.or_else(|| {
            existing
                .filter(|hover| hover.key.tab == active_tab
                    && hover.key.version == editor.version
                    && hover.key.generation == editor.git_blame.generation
                    && pointer_in_popup)
                .map(|hover| hover.key.line)
        });
        let Some(line) = line else {
            self.git_blame_popup_hover = None;
            self.git_blame_popup_copied = None;
            self.git_blame_popup_details = None;
            return;
        };
        let anchor = if hovered_line == Some(line) {
            ui_registry.rect_for(crate::ui_system::UiId::EditorBlameColumnRow(line))
        } else {
            existing.map(|hover| hover.annotation)
        };
        let Some(anchor) = anchor else { return };
        let Some(head_line) = crate::editor::head_line_for(&editor.git_hunks, line) else { return };
        let Some(blame) = editor.git_blame.blame.as_ref() else { return };
        let Some(commit_idx) = blame.line_commit.get(head_line).copied() else { return };
        let Some(commit) = blame.commits.get(commit_idx as usize).filter(|commit| !commit.uncommitted) else { return };
        let key = crate::app::git_blame::InlineDwellKey {
            tab: active_tab,
            line,
            version: editor.version,
            generation: editor.git_blame.generation,
        };
        self.draw_git_blame_commit_popup_at(editor, commit, anchor, key, ui_registry, mx, my, s);
    }

    fn draw_git_blame_commit_popup_at(
        &mut self,
        editor: &crate::editor::Editor,
        commit: &crate::editor::BlameCommit,
        annotation: (f32, f32, f32, f32),
        current_key: crate::app::git_blame::InlineDwellKey,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let oid = commit.oid;
        let cached_message = editor.git_blame.messages.iter().find(|(cached, _)| *cached == oid).map(|(_, message)| message);
        let details_stale = self.git_blame_popup_details.as_ref().is_none_or(|details| {
            self.git_blame_popup_hover.is_none_or(|hover| hover.oid != oid)
                || details.author != commit.author
                || details.author_mail.as_deref() != Some(commit.author_mail.as_str())
                || details.summary != commit.summary
                || details.message.as_deref() != cached_message.map(String::as_str)
        });
        if details_stale {
            let full_oid = oid.to_string();
            let author_display = if commit.author_mail.is_empty() {
                commit.author.clone()
            } else {
                format!("{} <{}>", commit.author, commit.author_mail)
            };
            self.git_blame_popup_details = Some(crate::renderer::CommitDetails {
                short_oid: commit.short_oid.clone(),
                oid: full_oid,
                author: commit.author.clone(),
                author_display,
                author_mail: Some(commit.author_mail.clone()),
                time: crate::app::git_panel::format_git_absolute_time(commit.author_time, 0),
                summary: commit.summary.clone(),
                message: cached_message.cloned(),
            });
            if self.git_blame_popup_copied.is_some_and(|copied| copied != oid) {
                self.git_blame_popup_copied = None;
            }
        }
        let Some(details) = self.git_blame_popup_details.take() else { return };
        let message = details.message.as_deref().unwrap_or(&details.summary).trim_end_matches('\n');
        let scale = 0.92;
        let pad_x = 10.0 * s;
        let pad_y = 7.0 * s;
        let margin = 6.0 * s;
        let copy_button_w = self
            .measure_ui_width("Копировать хэш", 0.86)
            .max(self.measure_ui_width("Хэш скопирован", 0.86))
            + 24.0 * s;
        let reveal_button_w = self.measure_ui_width("Показать в графе", 0.86) + 24.0 * s;
        let buttons_w = pad_x * 2.0 + copy_button_w + 8.0 * s + reveal_button_w;
        let popup_w = (440.0 * s).min((self.width - margin * 2.0).max(buttons_w));
        let inner_w = (popup_w - pad_x * 2.0).max(1.0);
        let mut message_lines = 0usize;
        for line in message.split('\n') {
            message_lines += self.git_graph_tooltip_wrapped_line_count(line, inner_w, scale);
        }
        message_lines = message_lines.max(1);
        let title_h = 18.0 * s;
        let line_h = 19.0 * s;
        let popup_h = pad_y + title_h * 3.0 + 5.0 * s + message_lines as f32 * line_h + 8.0 * s + 24.0 * s + pad_y;
        let mut x = annotation.0 + annotation.2;
        if x + popup_w > self.width - margin { x = annotation.0 - popup_w; }
        x = x.clamp(margin, (self.width - popup_w - margin).max(margin)).round();
        let y = (annotation.1 + annotation.3 * 0.5 - popup_h * 0.5)
            .clamp(margin, (self.height - popup_h - margin).max(margin)).round();
        self.git_blame_popup_hover = Some(crate::renderer::GitBlamePopupHover {
            oid,
            key: current_key,
            annotation,
            popup: (x, y, popup_w, popup_h),
        });
        ui_registry.register_blocker(crate::ui_system::UiId::GitBlamePopupBody, x, y, popup_w, popup_h, mx, my);
        self.push_rounded_rect_border(
            x, y, popup_w, popup_h, 7.0 * s, (1.0 * s).round().max(1.0),
            self.ui.pick(UiRole::Selection, self.ui_theme.sel),
            self.ui.pick(UiRole::BgTooltip, [0.11, 0.12, 0.16, 0.98]),
        );
        let content_x = (x + pad_x).round();
        let mut top = y + pad_y;
        self.draw_commit_details_text_row(&details.author_display, CommitDetailsTextRow {
            x: content_x, top, line_h: title_h, scale: 0.86,
            color: self.ui.pick(UiRole::TextPrimary, [1.0, 1.0, 1.0, 1.0]), mono: false,
        }, false);
        top += title_h;
        self.draw_commit_details_text_row(&details.time, CommitDetailsTextRow {
            x: content_x, top, line_h: title_h, scale: 0.86,
            color: self.ui.pick(UiRole::TextSecondary, [0.78, 0.82, 0.92, 1.0]), mono: false,
        }, false);
        top += title_h;
        self.draw_commit_details_text_row(&details.oid, CommitDetailsTextRow {
            x: content_x, top, line_h: title_h, scale: 0.82,
            color: self.ui.pick(UiRole::TextPrimary, [1.0, 1.0, 1.0, 1.0]), mono: true,
        }, false);
        top += title_h + 5.0 * s;
        for message_line in message.split('\n') {
            let mut remaining = message_line;
            while !remaining.is_empty() {
                let end = self.git_graph_tooltip_wrap_end(remaining, inner_w, scale);
                let end = if end == 0 { remaining.char_indices().nth(1).map_or(remaining.len(), |(idx, _)| idx) } else { end };
                let text = remaining[..end].trim_end();
                self.draw_commit_details_text_row(text, CommitDetailsTextRow {
                    x: content_x, top, line_h, scale,
                    color: self.ui.pick(UiRole::GitCommit, [0.86, 0.90, 1.0, 1.0]), mono: false,
                }, false);
                top += line_h;
                if end >= remaining.len() { break; }
                remaining = remaining[end..].trim_start();
            }
            if message_line.is_empty() { top += line_h; }
        }
        top += 5.0 * s;
        let copied = self.git_blame_popup_copied == Some(oid);
        let label = if copied { "Хэш скопирован" } else { "Копировать хэш" };
        let button_w = copy_button_w;
        let button_y = top.round();
        let hovered = ui_registry.register_rect(crate::ui_system::UiId::GitBlameCopyHash, content_x, button_y, button_w, 22.0 * s, mx, my);
        self.push_rounded_rect(content_x.round(), button_y, button_w, 22.0 * s, 4.0 * s, if hovered { self.ui.pick(UiRole::RowHover, [0.20, 0.23, 0.31, 1.0]) } else { self.ui.ink(0.12) });
        self.draw_string_scaled(label, (content_x + 12.0 * s).round(), (button_y + 15.0 * s).round(), self.ui.pick(UiRole::Link, [0.38, 0.62, 1.0, 1.0]), 0.86);
        let reveal_label = "Показать в графе";
        let reveal_x = (content_x + button_w + 8.0 * s).round();
        let reveal_w = reveal_button_w;
        let reveal_hovered = ui_registry.register_rect(crate::ui_system::UiId::GitBlameShowInGraph, reveal_x, button_y, reveal_w, 22.0 * s, mx, my);
        self.push_rounded_rect(reveal_x, button_y, reveal_w, 22.0 * s, 4.0 * s, if reveal_hovered { self.ui.pick(UiRole::RowHover, [0.20, 0.23, 0.31, 1.0]) } else { self.ui.ink(0.12) });
        self.draw_string_scaled(reveal_label, (reveal_x + 12.0 * s).round(), (button_y + 15.0 * s).round(), self.ui.pick(UiRole::Link, [0.38, 0.62, 1.0, 1.0]), 0.86);
        self.git_blame_popup_details = Some(details);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_file_tooltip(&mut self, text: &str, mouse_x: f32, mouse_y: f32, s: f32) {
        let tooltip_scale = 0.88;
        let pad_x = 12.0 * s;
        let measured_text_w = self.measure_ui_width(text, tooltip_scale);
        let Some(layout) = git_simple_tooltip_layout(
            self.width,
            self.height,
            mouse_x,
            mouse_y,
            measured_text_w,
            s,
        ) else {
            return;
        };

        self.push_rounded_rect(
            layout.x,
            layout.y,
            layout.w,
            layout.h,
            6.0 * s,
            self.ui.pick(UiRole::Selection, self.ui_theme.sel),
        );
        self.push_rounded_rect(
            layout.x + 1.0,
            layout.y + 1.0,
            (layout.w - 2.0).max(0.0),
            (layout.h - 2.0).max(0.0),
            5.0 * s,
            self.ui.pick(UiRole::BgTooltip, [
                self.ui_theme.minimap_bg[0],
                self.ui_theme.minimap_bg[1],
                self.ui_theme.minimap_bg[2],
                0.98,
            ]),
        );
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        self.draw_tree_label_clipped(
            text,
            layout.x + pad_x,
            (layout.y + layout.h * 0.5 + 5.0 * s).round(),
            layout.text_w,
            self.ui.pick(UiRole::TextPrimary, self.ui_theme.fg),
            tooltip_scale,
            &mut scratch,
        );
        self.scratch_buffer = scratch;
    }

    pub(crate) fn draw_git_file_tooltip_overlay(
        &mut self,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        if ide_panel.is_resizing_left
            || ide_panel.is_resizing_bottom
            || ide_panel.git.graph_resizing
        {
            self.reset_git_file_tooltip_overlay();
            return;
        }
        self.git_tooltip_waiting = false;
        let file_tooltip = self.git_file_tooltip.take().map(
            |(workspace_idx, file_idx, tooltip, mouse_x, mouse_y)| {
                (
                    false,
                    GitTooltipTarget {
                        kind: GIT_TOOLTIP_FILE,
                        workspace_idx,
                        item_idx: file_idx,
                    },
                    tooltip,
                    mouse_x,
                    mouse_y,
                )
            },
        );
        let action_tooltip = self.git_action_tooltip.take().map(
            |(kind, workspace_idx, tooltip, mouse_x, mouse_y)| {
                (
                    true,
                    GitTooltipTarget {
                        kind,
                        workspace_idx,
                        item_idx: 0,
                    },
                    tooltip,
                    mouse_x,
                    mouse_y,
                )
            },
        );
        let graph_tooltip = self.git_graph_tooltip.take();
        let Some((is_action_tooltip, target, tooltip, mouse_x, mouse_y)) =
            action_tooltip.or(file_tooltip)
        else {
            if let Some((workspace_idx, commit_idx, mouse_x, mouse_y)) = graph_tooltip
                && let Some(commit) = ide_panel.git.graph_snapshot.get(commit_idx)
            {
                let target = GitTooltipTarget {
                    kind: GIT_TOOLTIP_GRAPH_COMMIT,
                    workspace_idx,
                    item_idx: commit_idx,
                };
                if self.hide_popups_until_mouse_move {
                    return;
                }
                if let Some((anchor_x, anchor_y)) =
                    git_graph_tooltip_anchor(
                        &mut self.git_tooltip_timer,
                        target,
                        mouse_x,
                        mouse_y,
                        std::time::Instant::now(),
                    )
                {
                    let mut scratch = std::mem::take(&mut self.scratch_buffer);
                    self.draw_git_graph_tooltip(
                        commit,
                        GitGraphTooltipTarget {
                            workspace_idx,
                            commit_idx,
                        },
                        &ide_panel.git.graph_snapshot,
                        anchor_x,
                        anchor_y,
                        s,
                        ui_registry,
                        mx,
                        my,
                        ide_panel.git.graph_copied_commit,
                        &mut scratch,
                    );
                    self.scratch_buffer = scratch;
                } else {
                    self.git_tooltip_waiting = true;
                }
                return;
            }

            git_tooltip_reset(&mut self.git_tooltip_timer);
            self.git_graph_tooltip_hover = None;
            self.git_graph_tooltip_text.clear();
            self.git_graph_tooltip_text_rows.clear();
            self.git_graph_tooltip_stable_w = 0.0;
            self.clear_git_graph_tooltip_selection();
            return;
        };

        if self.hide_popups_until_mouse_move {
            return;
        }

        if let Some((anchor_x, anchor_y)) =
            git_tooltip_anchor(
                &mut self.git_tooltip_timer,
                target,
                mouse_x,
                mouse_y,
                std::time::Instant::now(),
            )
        {
            self.draw_git_file_tooltip(&tooltip, anchor_x, anchor_y, s);
        } else {
            self.git_tooltip_waiting = is_action_tooltip;
        }
    }

    pub(crate) fn reset_git_file_tooltip_overlay(&mut self) {
        self.git_file_tooltip = None;
        self.git_action_tooltip = None;
        self.git_graph_tooltip = None;
        self.git_graph_tooltip_hover = None;
        self.git_graph_tooltip_text.clear();
        self.git_graph_tooltip_text_rows.clear();
        self.git_graph_tooltip_stable_w = 0.0;
        self.clear_git_graph_tooltip_selection();
        self.git_blame_popup_hover = None;
        self.git_blame_popup_copied = None;
        self.git_blame_popup_details = None;
        self.git_tooltip_waiting = false;
        git_tooltip_reset(&mut self.git_tooltip_timer);
        self.reset_delayed_tooltip_anchor();
    }

    fn push_git_graph_vertical_segment(
        &mut self,
        x: f32,
        top: f32,
        bottom: f32,
        s: f32,
        color: [f32; 4],
    ) {
        if bottom - top > 0.5 * s {
            self.push_git_graph_sdf_segment(x, top, x, bottom, 2.0 * s, color);
        }
    }

    fn push_git_graph_soft_vertical_segment(
        &mut self,
        x: f32,
        top: f32,
        bottom: f32,
        width: f32,
        color: [f32; 4],
    ) {
        self.push_git_graph_sdf_segment(x, top, x, bottom, width, color);
    }

    fn push_git_graph_horizontal_segment(
        &mut self,
        x0: f32,
        x1: f32,
        y: f32,
        width: f32,
        color: [f32; 4],
    ) {
        self.push_git_graph_sdf_segment(x0, y, x1, y, width, color);
    }

    fn push_git_graph_parent_segment(
        &mut self,
        from_x: f32,
        to_x: f32,
        row_y: f32,
        row_h: f32,
        s: f32,
        color: [f32; 4],
    ) {
        let w = (to_x - from_x).abs();
        if w <= 0.5 * s {
            return;
        }
        let line_w = 2.0 * s;
        let start_y = row_y + row_h / 2.0;
        let end_y = row_y + row_h;
        let r = (8.0 * s).min(w * 0.5).max(2.0 * s);

        if to_x > from_x {
            let turn_x = (to_x - r).max(from_x);
            if turn_x - from_x > 0.5 * s {
                self.push_git_graph_horizontal_segment(from_x, turn_x, start_y, line_w, color);
            }
            let turn_y = (start_y + r).min(end_y);
            self.push_git_graph_quadratic_curve(
                turn_x, start_y, to_x, start_y, to_x, turn_y, line_w, color,
            );
            self.push_git_graph_soft_vertical_segment(to_x, turn_y, end_y, line_w, color);
        } else {
            let turn_y = (end_y - r).max(start_y);
            self.push_git_graph_soft_vertical_segment(from_x, start_y, turn_y, line_w, color);
            let turn_x = (from_x - r).max(to_x);
            self.push_git_graph_quadratic_curve(
                from_x, turn_y, from_x, end_y, turn_x, end_y, line_w, color,
            );
            if turn_x - to_x > 0.5 * s {
                self.push_git_graph_horizontal_segment(turn_x, to_x, end_y, line_w, color);
            }
        }
    }

    fn push_git_graph_shift_segment(
        &mut self,
        from_x: f32,
        to_x: f32,
        row_y: f32,
        row_h: f32,
        s: f32,
        color: [f32; 4],
    ) {
        let w = (to_x - from_x).abs();
        if w <= 0.5 * s {
            self.push_git_graph_vertical_segment(from_x, row_y, row_y + row_h, s, color);
            return;
        }
        let line_w = 2.0 * s;
        let mid_y = row_y + row_h / 2.0;
        let radius = (8.0 * s).min(w * 0.5).max(2.0 * s);
        let turn_in_y = (mid_y - radius).max(row_y);
        let turn_out_y = (mid_y + radius).min(row_y + row_h);
        let dir = if to_x > from_x { 1.0 } else { -1.0 };
        let from_mid_x = from_x + dir * radius;
        let to_mid_x = to_x - dir * radius;
        self.push_git_graph_soft_vertical_segment(from_x, row_y, turn_in_y, line_w, color);
        self.push_git_graph_quadratic_curve(
            from_x, turn_in_y, from_x, mid_y, from_mid_x, mid_y, line_w, color,
        );
        if (to_mid_x - from_mid_x).abs() > 0.5 * s {
            self.push_git_graph_horizontal_segment(from_mid_x, to_mid_x, mid_y, line_w, color);
        }
        self.push_git_graph_quadratic_curve(
            to_mid_x, mid_y, to_x, mid_y, to_x, turn_out_y, line_w, color,
        );
        self.push_git_graph_soft_vertical_segment(to_x, turn_out_y, row_y + row_h, line_w, color);
    }

    fn push_git_graph_shift_to_commit_segment(
        &mut self,
        from_x: f32,
        to_x: f32,
        row_y: f32,
        row_h: f32,
        s: f32,
        color: [f32; 4],
    ) {
        let w = (to_x - from_x).abs();
        let line_w = 2.0 * s;
        let mid_y = row_y + row_h / 2.0;
        if w <= 0.5 * s {
            self.push_git_graph_vertical_segment(from_x, row_y, mid_y, s, color);
            return;
        }
        let radius = (8.0 * s).min(w * 0.5).max(2.0 * s);
        let turn_in_y = (mid_y - radius).max(row_y);
        let dir = if to_x > from_x { 1.0 } else { -1.0 };
        let mid_x = to_x - dir * radius;
        self.push_git_graph_soft_vertical_segment(from_x, row_y, turn_in_y, line_w, color);
        self.push_git_graph_quadratic_curve(
            from_x,
            turn_in_y,
            from_x,
            mid_y,
            from_x + dir * radius,
            mid_y,
            line_w,
            color,
        );
        if (mid_x - (from_x + dir * radius)).abs() > 0.5 * s {
            self.push_git_graph_horizontal_segment(
                from_x + dir * radius,
                mid_x,
                mid_y,
                line_w,
                color,
            );
        }
        self.push_git_graph_horizontal_segment(mid_x, to_x, mid_y, line_w, color);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_git_graph_quadratic_curve(
        &mut self,
        x0: f32,
        y0: f32,
        cx: f32,
        cy: f32,
        x1: f32,
        y1: f32,
        width: f32,
        color: [f32; 4],
    ) {
        let approx_len = ((x1 - x0).abs() + (y1 - y0).abs()).max(width);
        let steps = (approx_len / (width * 0.75)).ceil().clamp(18.0, 64.0) as usize;
        let radius = width * 0.5;
        let extent = radius + 1.25;
        let sdf_params = [approx_len + width * 4.0, radius, 0.0];
        let mut prev_x = x0;
        let mut prev_y = y0;
        let mut prev_left = [x0, y0];
        let mut prev_right = [x0, y0];
        let mut prev_u = 0.0f32;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let inv = 1.0 - t;
            let x = inv * inv * x0 + 2.0 * inv * t * cx + t * t * x1;
            let y = inv * inv * y0 + 2.0 * inv * t * cy + t * t * y1;
            let tx = 2.0 * inv * (cx - x0) + 2.0 * t * (x1 - cx);
            let ty = 2.0 * inv * (cy - y0) + 2.0 * t * (y1 - cy);
            let tangent_len = (tx * tx + ty * ty).sqrt();
            if tangent_len <= 0.01 {
                continue;
            }
            let nx = -ty / tangent_len * extent;
            let ny = tx / tangent_len * extent;
            let left = [x + nx, y + ny];
            let right = [x - nx, y - ny];
            if step > 0 {
                let dx = x - prev_x;
                let dy = y - prev_y;
                let u = prev_u + (dx * dx + dy * dy).sqrt();
                let v0 = crate::renderer::Vertex {
                    pos: prev_left,
                    uv: [prev_u, extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                let v1 = crate::renderer::Vertex {
                    pos: left,
                    uv: [u, extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                let v2 = crate::renderer::Vertex {
                    pos: right,
                    uv: [u, -extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                let v3 = crate::renderer::Vertex {
                    pos: prev_right,
                    uv: [prev_u, -extent],
                    color,
                    mode: 8.0,
                    sdf_params,
                };
                self.ensure_vertex_capacity(6);
                self.vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
                prev_u = u;
            }
            prev_x = x;
            prev_y = y;
            prev_left = left;
            prev_right = right;
        }
    }

    fn push_git_graph_sdf_segment(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        width: f32,
        color: [f32; 4],
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.01 {
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        let radius = width * 0.5;
        let extent = radius + 1.25;
        let nx = -uy * extent;
        let ny = ux * extent;
        let segment_len = len;
        let sdf_params = [segment_len, radius, 0.0];
        let v0 = crate::renderer::Vertex {
            pos: [x0 + nx, y0 + ny],
            uv: [0.0, extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        let v1 = crate::renderer::Vertex {
            pos: [x1 + nx, y1 + ny],
            uv: [segment_len, extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        let v2 = crate::renderer::Vertex {
            pos: [x1 - nx, y1 - ny],
            uv: [segment_len, -extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        let v3 = crate::renderer::Vertex {
            pos: [x0 - nx, y0 - ny],
            uv: [0.0, -extent],
            color,
            mode: 8.0,
            sdf_params,
        };
        self.ensure_vertex_capacity(6);
        self.vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
    }

    fn clear_git_graph_tooltip_selection(&mut self) {
        self.git_graph_tooltip_selection_anchor = None;
        self.git_graph_tooltip_selection_cursor = None;
        self.git_graph_tooltip_selecting = false;
    }

    fn git_graph_tooltip_char_advance(&mut self, c: char, scale: f32, mono: bool) -> f32 {
        if mono {
            self.get_glyph(c)
                .map(|g| Self::snapped_text_advance(g.advance, scale))
                .unwrap_or(0.0)
        } else {
            self.get_ui_glyph(c)
                .map(|g| Self::snapped_text_advance(g.advance, scale))
                .unwrap_or(0.0)
        }
    }

    fn measure_git_graph_tooltip_text_width(&mut self, text: &str, scale: f32) -> f32 {
        text.chars()
            .filter(|&c| c != '\n' && c != '\r' && c != '\u{FE0F}' && c != '\u{200D}')
            .map(|c| self.git_graph_tooltip_char_advance(c, scale, false))
            .sum()
    }

    fn measure_git_graph_tooltip_mono_width(&mut self, text: &str, scale: f32) -> f32 {
        text.chars()
            .filter(|&c| c != '\n' && c != '\r' && c != '\u{FE0F}' && c != '\u{200D}')
            .map(|c| self.git_graph_tooltip_char_advance(c, scale, true))
            .sum()
    }

    fn git_graph_tooltip_wrap_end(&mut self, text: &str, max_w: f32, scale: f32) -> usize {
        let mut used = 0.0;
        let mut last_break = None;
        let mut end = 0usize;
        for (idx, ch) in text.char_indices() {
            if ch == '\n' || ch == '\r' {
                return idx;
            }
            let adv = self.git_graph_tooltip_char_advance(ch, scale, false);
            if end > 0 && used + adv > max_w {
                return last_break.filter(|&break_at| break_at > 0).unwrap_or(end);
            }
            used += adv;
            end = idx + ch.len_utf8();
            if ch.is_whitespace() {
                last_break = Some(end);
            }
        }
        end
    }

    fn git_graph_tooltip_wrapped_line_count(
        &mut self,
        mut text: &str,
        max_w: f32,
        scale: f32,
    ) -> usize {
        let mut lines = 0usize;
        while !text.is_empty() {
            let end = self.git_graph_tooltip_wrap_end(text, max_w, scale);
            lines += 1;
            if end >= text.len() {
                break;
            }
            text = text[end..].trim_start();
        }
        lines.max(1)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_wrapped_selectable_text(
        &mut self,
        mut text: &str,
        x: f32,
        mut line_top: f32,
        line_h: f32,
        color: [f32; 4],
        scale: f32,
        max_w: f32,
    ) -> f32 {
        if text.is_empty() {
            let _ = self.push_git_graph_tooltip_text_row(text, x, line_top, line_h, scale, false);
            return line_top + line_h;
        }
        while !text.is_empty() {
            let end = self.git_graph_tooltip_wrap_end(text, max_w, scale);
            let row_text = text[..end].trim_end();
            let row_start =
                self.push_git_graph_tooltip_text_row(row_text, x, line_top, line_h, scale, false);
            self.draw_git_graph_selectable_text(
                row_text,
                x,
                line_top + line_h * 0.62,
                color,
                scale,
                row_start,
                line_top,
                line_h,
                false,
            );
            if end >= text.len() {
                break;
            }
            text = text[end..].trim_start();
            line_top += line_h;
        }
        line_top + line_h
    }

    fn push_git_graph_tooltip_text_row(
        &mut self,
        text: &str,
        x: f32,
        top: f32,
        line_h: f32,
        scale: f32,
        mono: bool,
    ) -> usize {
        if !self.git_graph_tooltip_text.is_empty() {
            self.git_graph_tooltip_text.push('\n');
        }
        let start = self.git_graph_tooltip_text.len();
        self.git_graph_tooltip_text.push_str(text);
        let end = self.git_graph_tooltip_text.len();
        self.git_graph_tooltip_text_rows
            .push(crate::renderer::GitGraphTooltipTextRow {
                x,
                top,
                line_h,
                scale,
                mono,
                start,
                end,
            });
        start
    }

    fn draw_git_graph_selectable_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
        byte_start: usize,
        line_top: f32,
        line_h: f32,
        mono: bool,
    ) {
        let selected = git_graph_selection_range(self);
        let mut draw_x = x.round();
        let y = y.round();
        for (idx, c) in text.char_indices() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            let glyph = if mono {
                self.get_glyph(c)
            } else {
                self.get_ui_glyph(c)
            };
            if let Some(g) = glyph {
                let adv = Self::snapped_text_advance(g.advance, scale);
                let glyph_x = draw_x;
                if let Some((sel_start, sel_end)) = selected {
                    let offset = byte_start + idx;
                    if offset >= sel_start && offset < sel_end {
                        self.push_rect(
                            glyph_x,
                            line_top.round(),
                            adv.ceil() + 1.0,
                            line_h.ceil(),
                            self.ui.pick(UiRole::Selection, self.ui_theme.sel),
                        );
                    }
                }
                if mono {
                    self.push_mono_glyph_at_scale(c, glyph_x, y, scale, color, false);
                } else {
                    self.push_ui_glyph_at_scale(c, glyph_x, y, scale, color, false);
                }
                draw_x += adv;
            }
        }
    }

    pub(crate) fn git_graph_tooltip_byte_at(&mut self, mx: f32, my: f32) -> usize {
        let Some(row) = self
            .git_graph_tooltip_text_rows
            .iter()
            .min_by(|a, b| {
                let da = if my < a.top {
                    a.top - my
                } else if my > a.top + a.line_h {
                    my - (a.top + a.line_h)
                } else {
                    0.0
                };
                let db = if my < b.top {
                    b.top - my
                } else if my > b.top + b.line_h {
                    my - (b.top + b.line_h)
                } else {
                    0.0
                };
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
        else {
            return 0;
        };
        if mx <= row.x {
            return row.start;
        }
        let line = self
            .git_graph_tooltip_text
            .get(row.start..row.end)
            .unwrap_or("")
            .to_string();
        let mut x = row.x;
        for (idx, ch) in line.char_indices() {
            let adv = self.git_graph_tooltip_char_advance(ch, row.scale, row.mono);
            if mx <= x + adv * 0.5 {
                return row.start + idx;
            }
            x += adv;
        }
        row.end
    }

    pub(crate) fn selected_git_graph_tooltip_text(&self) -> Option<String> {
        let (start, end) = git_graph_selection_range(self)?;
        if end <= self.git_graph_tooltip_text.len()
            && self.git_graph_tooltip_text.is_char_boundary(start)
            && self.git_graph_tooltip_text.is_char_boundary(end)
        {
            Some(self.git_graph_tooltip_text[start..end].to_string())
        } else {
            None
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_git_graph_tooltip(
        &mut self,
        commit: &crate::app::git_panel::GitGraphCommit,
        target: GitGraphTooltipTarget,
        commits: &[crate::app::git_panel::GitGraphCommit],
        anchor_x: f32,
        anchor_y: f32,
        s: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        copied_commit: Option<(usize, usize)>,
        scratch: &mut String,
    ) {
        let pad_x = 10.0 * s;
        let pad_y = 6.0 * s;
        let target_key = (target.workspace_idx, target.commit_idx);
        let target_changed = self.git_graph_tooltip_hover.is_none_or(|hover| {
            hover.workspace_idx != target.workspace_idx || hover.commit_idx != target.commit_idx
        });
        if target_changed {
            self.clear_git_graph_tooltip_selection();
            self.git_graph_tooltip_visible_copied = None;
        }
        let details_stale = self.git_graph_tooltip_details.as_ref().is_none_or(|details| {
            details.oid.as_str() != commit.oid.as_ref()
                || details.author != commit.author_name
                || details.author_mail.as_deref() != Some(commit.author_email.as_str())
                || details.time != commit.absolute_time
                || details.summary != commit.summary
        });
        if details_stale {
            self.git_graph_tooltip_details = Some(crate::renderer::CommitDetails {
                oid: commit.oid.to_string(),
                short_oid: commit.short_oid.clone(),
                author: commit.author_name.clone(),
                author_display: commit.author_name.clone(),
                author_mail: Some(commit.author_email.clone()),
                time: commit.absolute_time.clone(),
                summary: commit.summary.clone(),
                message: None,
            });
        }
        let Some(details) = self.git_graph_tooltip_details.take() else { return };
        if self.git_graph_tooltip_seen_copied != copied_commit {
            self.git_graph_tooltip_seen_copied = copied_commit;
            self.git_graph_tooltip_visible_copied =
                (copied_commit == Some(target_key)).then_some(target_key);
        }
        let copied = self.git_graph_tooltip_visible_copied == Some(target_key);
        self.git_graph_tooltip_text.clear();
        self.git_graph_tooltip_text_rows.clear();

        let margin = 6.0 * s;
        let tooltip_w = (440.0 * s).min((self.width - margin * 2.0).max(260.0 * s));
        let inner_w = (tooltip_w - pad_x * 2.0).max(1.0);
        let title_scale = 0.84;
        let title_line_h = 22.0 * s;
        let title_icon_size = 18.0 * s;
        let title_icon_gap = 5.0 * s;
        let title_lines = 2;
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(
            scratch,
            format_args!("{} ({})", commit.relative_time, commit.absolute_time),
        );
        let summary_lines = self.git_graph_tooltip_wrapped_line_count(&details.summary, inner_w, 0.9);
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ =
                std::fmt::Write::write_fmt(scratch, format_args!("{} files", stats.files_changed));
        } else {
            scratch.push_str("stats deferred");
        }
        let files_w = self.measure_git_graph_tooltip_mono_width(scratch, 0.82);
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ = std::fmt::Write::write_fmt(scratch, format_args!("+{}", stats.insertions));
        } else {
            scratch.push_str("+?");
        }
        let insertions_w = self.measure_git_graph_tooltip_text_width(scratch, 0.82);
        let branch_chip_w = commit.branch_name.as_ref().map(|branch_name| {
            branch_chip_width(self.measure_ui_width(branch_name, 0.82), 6.0 * s, f32::MAX)
        });
        let title_h = title_lines as f32 * title_line_h;
        let summary_h = summary_lines as f32 * 20.0 * s;
        let branch_section_h = if branch_chip_w.is_some() {
            10.0 * s + 18.0 * s
        } else {
            0.0
        };
        let tooltip_h = pad_y
            + title_h
            + 6.0 * s
            + summary_h
            + 6.0 * s
            + 18.0 * s
            + branch_section_h
            + 6.0 * s
            + 18.0 * s
            + pad_y;
        let mut tooltip_x = anchor_x + 6.0 * s;
        if tooltip_x + tooltip_w > self.width - margin {
            tooltip_x = anchor_x - tooltip_w - 6.0 * s;
        }
        tooltip_x = tooltip_x.clamp(margin, (self.width - tooltip_w - margin).max(margin));
        let content_x = tooltip_x + pad_x;
        let mut tooltip_y = anchor_y - tooltip_h / 2.0;
        tooltip_y = tooltip_y.clamp(margin, (self.height - tooltip_h - margin).max(margin));
        let hover_x = anchor_x.min(tooltip_x);
        let hover_y = anchor_y.min(tooltip_y);
        self.git_graph_tooltip_hover = Some(crate::renderer::GitGraphTooltipHover {
            workspace_idx: target.workspace_idx,
            commit_idx: target.commit_idx,
            anchor_x,
            anchor_y,
            x: hover_x,
            y: hover_y,
            w: (tooltip_x + tooltip_w - hover_x).max(tooltip_w),
            h: (tooltip_y + tooltip_h - hover_y).max(tooltip_h),
        });
        if mx >= tooltip_x
            && mx <= tooltip_x + tooltip_w
            && my >= tooltip_y
            && my <= tooltip_y + tooltip_h
        {
            ui_registry.reset_cursor_state();
            ui_registry.register_blocker(
                crate::ui_system::UiId::GitGraphCommit(target.workspace_idx, target.commit_idx),
                tooltip_x,
                tooltip_y,
                tooltip_w,
                tooltip_h,
                mx,
                my,
            );
        }
        self.push_rounded_rect_border(
            tooltip_x,
            tooltip_y,
            tooltip_w,
            tooltip_h,
            7.0 * s,
            (1.0 * s).round().max(1.0),
            self.ui.pick(UiRole::Selection, self.ui_theme.sel),
            self.ui.pick(UiRole::BgTooltip, [0.11, 0.12, 0.16, 0.98]),
        );

        let mut line_top = tooltip_y + pad_y;
        let author_row_top = line_top;
        let date_row_top = line_top + title_line_h;
        let author_text_y = (author_row_top + title_line_h * 0.62).round();
        let date_text_y = (date_row_top + title_line_h * 0.62).round();
        let title_icon_raise = 2.0 * s;
        let author_icon_y =
            (author_row_top + (title_line_h - title_icon_size) * 0.38 - title_icon_raise).round();
        let date_icon_extra_drop = 1.0 * s;
        let date_icon_y = (date_row_top + (title_line_h - title_icon_size) * 0.38
            - title_icon_raise
            + date_icon_extra_drop)
            .round();
        let author_x = (content_x + title_icon_size + title_icon_gap).round();
        let title_count_color = self.ui.pick(UiRole::Icon, self.ui_theme.sel);
        let title_count_text_color = self.ui.pick(UiRole::TextPrimary, [1.0, 1.0, 1.0, 1.0]);
        let (newest_count, oldest_count) =
            git_graph_tooltip_branch_counts(commits, target.commit_idx);
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{newest_count}"));
        let newest_w = self.measure_git_graph_tooltip_mono_width(scratch, title_scale);
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{oldest_count}"));
        let oldest_w = self.measure_git_graph_tooltip_mono_width(scratch, title_scale);
        let count_text_w = newest_w.max(oldest_w);
        let count_x = (content_x + inner_w - count_text_w).round();
        let count_icon_size = title_icon_size;
        let count_icon_x = (count_x - title_icon_gap - count_icon_size).round();
        self.draw_atlas_icon(
            crate::widgets::IconType::Person,
            content_x.round(),
            author_icon_y,
            title_icon_size,
            self.ui.pick(UiRole::Icon, self.ui_theme.sel),
        );
        self.draw_commit_details_text_row(
            &details.author,
            CommitDetailsTextRow {
                x: author_x,
                top: author_row_top,
                line_h: title_line_h,
                scale: title_scale,
                color: self.ui.pick(UiRole::TextPrimary, [1.0, 1.0, 1.0, 1.0]),
                mono: false,
            },
            true,
        );
        self.draw_atlas_icon(
            crate::widgets::IconType::NumberCount,
            count_icon_x,
            author_icon_y,
            count_icon_size,
            title_count_color,
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{newest_count}"));
        self.draw_string_mono_scaled(
            scratch,
            count_x,
            author_text_y,
            title_count_text_color,
            title_scale,
        );

        self.draw_atlas_icon(
            crate::widgets::IconType::Time,
            content_x.round(),
            date_icon_y,
            title_icon_size,
            self.ui.pick(UiRole::Icon, self.ui_theme.sel),
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(
            scratch,
            format_args!("{} ({})", commit.relative_time, details.time),
        );
        let date_x = (content_x + title_icon_size + title_icon_gap).round();
        self.draw_commit_details_text_row(
            scratch,
            CommitDetailsTextRow {
                x: date_x,
                top: date_row_top,
                line_h: title_line_h,
                scale: title_scale,
                color: self.ui.pick(UiRole::TextPrimary, [1.0, 1.0, 1.0, 1.0]),
                mono: false,
            },
            true,
        );
        self.draw_atlas_icon(
            crate::widgets::IconType::NumberCount,
            count_icon_x,
            date_icon_y,
            count_icon_size,
            title_count_color,
        );
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(scratch, format_args!("{oldest_count}"));
        self.draw_string_mono_scaled(
            scratch,
            count_x,
            date_text_y,
            title_count_text_color,
            title_scale,
        );
        line_top += title_line_h * 2.0;

        line_top += 6.0 * s;
        line_top = self.draw_git_graph_wrapped_selectable_text(
            &details.summary,
            content_x,
            line_top,
            20.0 * s,
            self.ui.pick(UiRole::GitCommit, [0.86, 0.90, 1.0, 1.0]),
            0.9,
            inner_w,
        );

        line_top += 1.0 * s;
        self.push_rect(tooltip_x, line_top, tooltip_w, 1.0, self.ui.ink(0.12));
        line_top += 5.0 * s;
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ =
                std::fmt::Write::write_fmt(scratch, format_args!("{} files", stats.files_changed));
        } else {
            scratch.push_str("stats deferred");
        }
        let stats_start = self.push_git_graph_tooltip_text_row(
            scratch,
            content_x,
            line_top,
            18.0 * s,
            0.82,
            true,
        );
        self.draw_git_graph_selectable_text(
            scratch,
            content_x,
            line_top + 18.0 * s * 0.68,
            self.ui.pick(UiRole::TextSecondary, [0.78, 0.82, 0.92, 1.0]),
            0.82,
            stats_start,
            line_top,
            18.0 * s,
            true,
        );
        let mut stat_x = content_x + files_w + 12.0 * s;
        let mut stats_end = stats_start + scratch.len();
        self.git_graph_tooltip_text.push(' ');
        stats_end += 1;
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ = std::fmt::Write::write_fmt(scratch, format_args!("+{}", stats.insertions));
        } else {
            scratch.push_str("+?");
        }
        self.git_graph_tooltip_text.push_str(scratch);
        if let Some(row) = self.git_graph_tooltip_text_rows.last_mut() {
            row.end = self.git_graph_tooltip_text.len();
        }
        self.draw_git_graph_selectable_text(
            scratch,
            stat_x,
            line_top + 18.0 * s * 0.68,
            self.ui.pick(UiRole::GitHunkAdded, [0.52, 0.82, 0.58, 1.0]),
            0.82,
            stats_end,
            line_top,
            18.0 * s,
            false,
        );
        stat_x += insertions_w + 10.0 * s;
        let mut stats_end = stats_end + scratch.len();
        self.git_graph_tooltip_text.push(' ');
        stats_end += 1;
        scratch.clear();
        if let Some(stats) = commit.stats {
            let _ = std::fmt::Write::write_fmt(scratch, format_args!("-{}", stats.deletions));
        } else {
            scratch.push_str("-?");
        }
        self.git_graph_tooltip_text.push_str(scratch);
        if let Some(row) = self.git_graph_tooltip_text_rows.last_mut() {
            row.end = self.git_graph_tooltip_text.len();
        }
        self.draw_git_graph_selectable_text(
            scratch,
            stat_x,
            line_top + 18.0 * s * 0.68,
            self.ui.pick(UiRole::GitHunkDeleted, [0.95, 0.42, 0.46, 1.0]),
            0.82,
            stats_end,
            line_top,
            18.0 * s,
            false,
        );
        line_top += 18.0 * s;

        if let Some(branch_name) = &commit.branch_name {
            line_top += 1.0 * s;
            self.push_rect(tooltip_x, line_top, tooltip_w, 1.0, self.ui.ink(0.12));
            line_top += 5.0 * s;
            let pill_h = 18.0 * s;
            let scale = 0.82;
            let pill_w = branch_chip_w.unwrap_or_else(|| {
                branch_chip_width(self.measure_ui_width(branch_name, scale), 6.0 * s, f32::MAX)
            });
            let desired_center_y = line_top + pill_h * 0.5;
            self.draw_git_graph_branch_chip(
                branch_name,
                content_x,
                desired_center_y,
                pill_w,
                pill_h,
                4.0 * s,
                self.ui.pick(UiRole::Accent, [0.28, 0.24, 0.40, 1.0]),
                self.ui.pick(UiRole::TextOnAccent, [0.86, 0.90, 1.0, 1.0]),
                scale,
                6.0 * s,
                true,
                scratch,
            );
            line_top += pill_h + 4.0 * s;
        }

        line_top += 1.0 * s;
        self.push_rect(tooltip_x, line_top, tooltip_w, 1.0, self.ui.ink(0.12));
        line_top += 5.0 * s;
        let hash_w = self.measure_git_graph_tooltip_mono_width(&commit.short_oid, 0.86);
        let hash_x = content_x;
        self.draw_commit_details_text_row(
            &details.short_oid,
            CommitDetailsTextRow {
                x: hash_x,
                top: line_top,
                line_h: 18.0 * s,
                scale: 0.86,
                color: self.ui.pick(UiRole::TextPrimary, [1.0, 1.0, 1.0, 1.0]),
                mono: true,
            },
            true,
        );
        let copy_size = 16.0 * s;
        let copy_x = hash_x + hash_w + 7.0 * s;
        let copy_y = line_top + (18.0 * s - copy_size) * 0.5 - 2.0 * s;
        let copy_hovered = ui_registry.register_rect(
            crate::ui_system::UiId::GitGraphCopyCommit(target.workspace_idx, target.commit_idx),
            copy_x - 3.0 * s,
            copy_y - 3.0 * s,
            copy_size + 6.0 * s,
            copy_size + 6.0 * s,
            mx,
            my,
        );
        self.draw_atlas_icon(
            if copied {
                crate::widgets::IconType::Check
            } else {
                crate::widgets::IconType::Copy
            },
            copy_x,
            copy_y,
            copy_size,
            if copied {
                self.ui.pick(UiRole::Success, [0.3, 0.9, 0.4, 1.0])
            } else if copy_hovered {
                self.ui.pick(UiRole::Icon, [1.0, 1.0, 1.0, 1.0])
            } else {
                self.ui.pick(UiRole::Link, [0.38, 0.62, 1.0, 0.86])
            },
        );
        let sep_x = copy_x + copy_size + 12.0 * s;
        self.push_rect(
            sep_x,
            line_top - 1.0 * s,
            1.0,
            18.0 * s,
            self.ui.ink(0.28),
        );
        let open_icon_size = 14.0 * s;
        let open_icon_x = sep_x + 14.0 * s;
        let open_icon_y = line_top + (18.0 * s - open_icon_size) * 0.5 - 3.0 * s;
        let open_x = open_icon_x + open_icon_size + 5.0 * s;
        let open_text = "Open on GitHub";
        let open_w = self.measure_ui_width(open_text, 0.86);
        ui_registry.register_rect(
            crate::ui_system::UiId::GitGraphOpenCommit(target.workspace_idx, target.commit_idx),
            open_icon_x,
            line_top,
            open_icon_size + 5.0 * s + open_w,
            18.0 * s,
            mx,
            my,
        );
        self.draw_atlas_icon(
            crate::widgets::IconType::GithubDark,
            open_icon_x,
            open_icon_y,
            open_icon_size,
            self.ui.pick(UiRole::Link, [0.38, 0.62, 1.0, 1.0]),
        );
        self.draw_string_scaled(
            open_text,
            open_x,
            line_top + 18.0 * s * 0.62,
            self.ui.pick(UiRole::Link, [0.38, 0.62, 1.0, 1.0]),
            0.86,
        );
        self.git_graph_tooltip_details = Some(details);
    }

}
