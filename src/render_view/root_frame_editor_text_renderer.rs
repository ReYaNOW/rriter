/// Editor-area values produced by `Renderer::draw_root_editor_text` that the later
/// editor phases of the root frame read (`render_scroll_y` is clamped here).
#[derive(Clone, Copy)]
struct RootFrameEditorText<'a> {
    render_scroll_y: f32,
    minimap_w: f32,
    minimap_x: f32,
    scrollbar_x: f32,
    solid_minimap_bg: [f32; 4],
    skip_visual_lines: usize,
    end_visual_line: usize,
    active_git_diff_state: Option<&'a crate::app::git_diff::GitDiffState>,
    editor_clip_y: f32,
    editor_clip_h: f32,
}

/// Results of `Renderer::draw_root_editor_overlays` used by the editor chrome and hover phases.
#[derive(Clone, Copy)]
struct RootFrameEditorOverlays {
    hovered_diag_type_target: Option<usize>,
    gutter_x: f32,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_root_editor_text<'a>(
        &mut self,
        editor: &mut Editor,
        tabs: &'a [crate::app::EditorTab],
        active_tab: usize,
        spans: &[ColorSpan],
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        ctrl_definition_range: Option<(usize, usize)>,
        python_inlay_hints: &[crate::app::PythonInlayHint],
        closing_hints: &[crate::languages::dart::ClosingHint],
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        blink_alpha: f32,
        dialog_window_open: bool,
        search_focused: bool,
        show_settings: bool,
        pre_editor_start: Option<Instant>,
        telemetry_frame_start: Option<Instant>,
        telemetry_root_phases: &mut [f32; 5],
        telemetry_editor_time: &mut f32,
        inline_git_popup_line: Option<usize>,
        inline_blame_dwell: &crate::app::git_blame::InlineBlameDwell,
    ) -> RootFrameEditorText<'a> {
        let RootFramePanelLayout {
            visible_cursor_line,
            s,
            real_height,
            ui_mx,
            ui_my,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_h,
            editor_height,
            editor_scroll_height,
            max_scroll,
            scrollbar_width,
            render_scroll_x,
            render_scroll_y,
            ..
        } = viewport;
        editor.ensure_indent_cache_updated();
        let indent_levels = editor.get_cached_indent_levels();
        let (first, second) = editor.text_parts();
        let first_len = first.len();
        let len = first_len + second.len();

        // --- Подсветка скобок ---
        let bracket_pairs = if self.bracket_pair_cache_version == editor.version
            && self.bracket_pair_cache_cursor == editor.cursor
        {
            self.bracket_pair_cache
        } else {
            let mut bracket_pairs = None;
            let find_matching_bracket = |pos: usize, b: u8| -> Option<usize> {
                let (open, close, dir) = match b {
                    b'(' => (b'(', b')', 1isize),
                    b'[' => (b'[', b']', 1isize),
                    b'{' => (b'{', b'}', 1isize),
                    b')' => (b')', b'(', -1isize),
                    b']' => (b']', b'[', -1isize),
                    b'}' => (b'}', b'{', -1isize),
                    _ => return None,
                };
                let mut depth = 1;
                let mut curr = pos as isize + dir;
                while curr >= 0 && curr < len as isize {
                    let cb = editor.byte_at(curr as usize);
                    if cb == open {
                        depth += 1;
                    } else if cb == close {
                        depth -= 1;
                        if depth == 0 {
                            return Some(curr as usize);
                        }
                    }
                    curr += dir;
                }
                None
            };

            if editor.cursor < len {
                let b = editor.byte_at(editor.cursor);
                if let Some(matching) = find_matching_bracket(editor.cursor, b) {
                    bracket_pairs = Some((editor.cursor, matching));
                }
            }
            if bracket_pairs.is_none() && editor.cursor > 0 {
                let b = editor.byte_at(editor.cursor - 1);
                if let Some(matching) = find_matching_bracket(editor.cursor - 1, b) {
                    bracket_pairs = Some((editor.cursor - 1, matching));
                }
            }
            self.bracket_pair_cache = bracket_pairs;
            self.bracket_pair_cache_version = editor.version;
            self.bracket_pair_cache_cursor = editor.cursor;
            bracket_pairs
        };

        let sel_start = editor
            .selection_anchor
            .map(|a| a.min(editor.cursor))
            .unwrap_or(editor.cursor);
        let sel_end = editor
            .selection_anchor
            .map(|a| a.max(editor.cursor))
            .unwrap_or(editor.cursor);

        self.refresh_identical_words_cache(
            editor, first, second, first_len, len, sel_start, sel_end,
        );

        let render_scroll_y = render_scroll_y.min(max_scroll.max(0.0));

        let minimap_w = self.minimap_width;
        let editor_right =
            editor_vertical_overlay_bounds(self.width, minimap_w, scrollbar_width);
        let minimap_x = editor_right.visual_right;
        let scrollbar_x = editor_right.interaction_right;

        ui_registry.register_text_input(
            crate::ui_system::UiId::EditorTextBody,
            self.left_padding,
            tab_bar_h,
            editor_right.interaction_width_from(self.left_padding),
            editor_scroll_height,
            ui_mx,
            ui_my,
        );

        let solid_minimap_bg = [
            self.theme.minimap_bg[0],
            self.theme.minimap_bg[1],
            self.theme.minimap_bg[2],
            1.0,
        ];

        let cursor_line_y = self.baseline_offset - render_scroll_y
            + (visible_cursor_line as f32 * self.line_height);

        if cursor_line_y > -self.line_height * 2.0 && cursor_line_y < real_height + self.line_height
        {
            self.push_rect(
                self.left_padding,
                cursor_line_y - self.baseline_offset + 2.0,
                editor_right.visual_width_from(self.left_padding),
                self.line_height,
                self.theme.cursor_line,
            );
        }

        let skip_visual_lines = 0;
        let end_visual_line = self.visual_lines.len();
        let active_git_diff_state = tabs.get(active_tab).and_then(|tab| match &tab.kind {
            crate::app::EditorTabKind::GitDiff(_, state) => Some(state),
            crate::app::EditorTabKind::Normal
            | crate::app::EditorTabKind::ApiClient(_, _)
            | crate::app::EditorTabKind::DatabaseTable(_, _)
            | crate::app::EditorTabKind::DatabaseQuery(_, _)
            | crate::app::EditorTabKind::Pdf
            | crate::app::EditorTabKind::Image => None,
        });

        let editor_clip_x = self.left_padding.round().max(0.0);
        let editor_clip_y = tab_bar_h.round().max(0.0);
        let editor_clip_w = editor_right.visual_clip_width_from(editor_clip_x);
        let editor_clip_h = editor_height.round().max(0.0);
        if editor_clip_w > 0.0 && editor_clip_h > 0.0 {
            if let Some(pre_editor_start) = pre_editor_start {
                telemetry_root_phases[2] = pre_editor_start.elapsed().as_secs_f32();
            }
            let stage_start = telemetry_frame_start.map(|_| Instant::now());
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                self.gl.scissor(
                    editor_clip_x as i32,
                    (self.height - (editor_clip_y + editor_clip_h)).round() as i32,
                    editor_clip_w as i32,
                    editor_clip_h as i32,
                );
            }
            let editor_cursor_blocked = search_focused || ide_panel.git.message_focused;
            self.draw_editor_visible_text(
                editor,
                spans,
                search_results,
                search_current_idx,
                first,
                second,
                indent_levels,
                first_len,
                len,
                bracket_pairs,
                sel_start,
                sel_end,
                render_scroll_x,
                render_scroll_y,
                editor_right.interaction_right,
                editor_right.visual_right,
                blink_alpha,
                dialog_window_open,
                editor_cursor_blocked,
                show_settings,
                s,
                skip_visual_lines,
                end_visual_line,
                ui_registry,
                ctrl_definition_range,
                active_git_diff_state.map(|state| state.line_kinds.as_slice()),
                python_inlay_hints,
                closing_hints,
                inline_git_popup_line,
                active_tab,
                inline_blame_dwell,
            );
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
            if let Some(stage_start) = stage_start {
                *telemetry_editor_time = stage_start.elapsed().as_secs_f32();
            }
        }
        RootFrameEditorText {
            render_scroll_y,
            minimap_w,
            minimap_x,
            scrollbar_x,
            solid_minimap_bg,
            skip_visual_lines,
            end_visual_line,
            active_git_diff_state,
            editor_clip_y,
            editor_clip_h,
        }
    }

    fn draw_root_editor_overlays(
        &mut self,
        editor: &Editor,
        lsp_diagnostics: &[&crate::lsp::Diagnostic],
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover: &mut crate::app::mouse::HoverState,
        active_tab: usize,
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        editor_text: RootFrameEditorText<'_>,
        scroll_x: f32,
        is_ide_mode: bool,
        show_welcome: bool,
        telemetry_frame_start: Option<Instant>,
        telemetry_root_phases: &mut [f32; 5],
    ) -> RootFrameEditorOverlays {
        let RootFramePanelLayout {
            mx,
            my,
            panel_bottom_h,
            ui_mx,
            ui_my,
            is_ui_disabled,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_h,
            editor_height,
            ..
        } = viewport;
        let RootFrameEditorText {
            render_scroll_y,
            minimap_w,
            minimap_x,
            solid_minimap_bg,
            ..
        } = editor_text;
        let overlays_start = telemetry_frame_start.map(|_| Instant::now());
        self.flush();
        let mouse_in_popup = hover
            .popup_or_bridge_contains(mx, my, self.width, self.scale_factor)
            .0;

        let hovered_diag_type_target = self.draw_lsp_squiggles_and_collect_hovered_diag(
            hover,
            editor,
            lsp_diagnostics,
            scroll_x,
            render_scroll_y,
            panel_bottom_h,
            is_ide_mode,
            is_ui_disabled,
            ide_panel,
            ui_mx,
            ui_my,
            mouse_in_popup,
        );

        let gutter_x = self.draw_root_editor_gutter(
            editor,
            ui_registry,
            active_tab,
            layout,
            viewport,
            editor_text,
            is_ide_mode,
            show_welcome,
        );

        self.flush();

        if minimap_w > 0.0 {
            self.push_rect(
                minimap_x,
                tab_bar_h,
                minimap_w,
                editor_height,
                solid_minimap_bg,
            );
        }

        if let Some(overlays_start) = overlays_start {
            telemetry_root_phases[3] = overlays_start.elapsed().as_secs_f32();
        }
        RootFrameEditorOverlays {
            hovered_diag_type_target,
            gutter_x,
        }
    }

    fn draw_root_editor_gutter(
        &mut self,
        editor: &Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        active_tab: usize,
        layout: RootFramePanelLayout<'_>,
        viewport: RootFrameViewport,
        editor_text: RootFrameEditorText<'_>,
        is_ide_mode: bool,
        show_welcome: bool,
    ) -> f32 {
        let RootFramePanelLayout {
            s,
            real_height,
            panel_left_w,
            ..
        } = layout;
        let RootFrameViewport {
            tab_bar_h,
            editor_height,
            ..
        } = viewport;
        let RootFrameEditorText {
            render_scroll_y,
            solid_minimap_bg,
            skip_visual_lines,
            end_visual_line,
            active_git_diff_state,
            editor_clip_y,
            editor_clip_h,
            ..
        } = editor_text;
        let gutter_x = if is_ide_mode {
            48.0 * s + panel_left_w
        } else {
            0.0
        };
        // Гаттер рисуем только в зоне редактора (не заходим на нижнюю панель)
        self.push_rect(
            gutter_x.round() + 1.0,
            tab_bar_h,
            (self.left_padding - (gutter_x.round() + 1.0)).max(0.0),
            editor_height,
            solid_minimap_bg,
        );
        // Левая граница гаттера (отделяет IDE панель от зоны номеров строк)
        if is_ide_mode && panel_left_w > 0.0 {
            self.push_rect(
                gutter_x.round() + 1.0,
                tab_bar_h,
                1.0,
                editor_height,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
            );
        }
        // Правая граница гаттера (тонкая линия, как у Indent Guide)
        self.push_rect(
            self.left_padding - 1.0,
            tab_bar_h,
            1.0,
            editor_height,
            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.10],
        );

        // visual_lines carry an overscan margin above/below the viewport (update_cache),
        // so gutter hitboxes are clipped to the editor area: off-screen rows must not be clickable.
        let gutter_hit_clip =
            crate::ui_system::UiClipRect::new(0.0, editor_clip_y, self.width, editor_clip_h);
        let gutter_bottom = editor_clip_y + editor_clip_h;
        if !show_welcome && active_git_diff_state.is_none() {
            let line_numbers_x = gutter_x
                + crate::render_view::blame_column_width(
                    editor,
                    is_ide_mode,
                    show_welcome,
                    active_git_diff_state.is_some(),
                    s,
                );
            ui_registry.register_rect_clipped(
                crate::ui_system::UiId::EditorLineNumbers,
                line_numbers_x.round(),
                editor_clip_y,
                (self.left_padding - line_numbers_x).max(0.0),
                editor_clip_h,
                gutter_hit_clip,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }
        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let y = self.baseline_offset + v_line.y_offset - render_scroll_y;
            let phys_idx = v_line.physical_line - 1;
            let line_top = v_line.y_offset - render_scroll_y;
            let line_visible =
                line_top + self.line_height > editor_clip_y && line_top < gutter_bottom;

            if let Some(hunk_idx) = active_git_diff_state
                .filter(|_| line_visible)
                .and_then(|state| state.rollback_hunk_index_at_line(phys_idx))
            {
                let icon_size = 22.0 * s;
                let icon_x = self.left_padding - 22.0 * s;
                let icon_y = line_top + (self.line_height - icon_size) * 0.5;
                let hit_x = icon_x - 5.0 * s;
                let hit_w = icon_size + 10.0 * s;
                let hovered = ui_registry.register_rect_clipped(
                    crate::ui_system::UiId::GitDiffRollbackHunk(active_tab, hunk_idx),
                    hit_x,
                    line_top,
                    hit_w,
                    self.line_height,
                    gutter_hit_clip,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
                self.draw_atlas_icon(
                    crate::widgets::IconType::Rollback,
                    icon_x,
                    icon_y,
                    icon_size,
                    if hovered {
                        self.theme.rollback_hover
                    } else {
                        self.theme.fg
                    },
                );
            } else if active_git_diff_state.is_none()
                && editor.foldable_lines.contains_key(&phys_idx)
            {
                let arrow_x = self.left_padding - 20.0 * s;
                let is_folded = editor.folded_lines.contains(&phys_idx);
                let arrow_str = if is_folded { "▶" } else { "▼" };
                self.draw_string_scaled(arrow_str, arrow_x, y - 1.0 * s, self.theme.line_num, 1.0);
                ui_registry.register_rect_clipped(
                    crate::ui_system::UiId::EditorFoldArrow(phys_idx),
                    arrow_x - 5.0 * s,
                    y - self.line_height,
                    20.0 * s,
                    self.line_height + 5.0 * s,
                    gutter_hit_clip,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }

            let num_right_pad = if active_git_diff_state.is_some() {
                28.0 * s
            } else {
                24.0 * s
            };
            self.draw_editor_line_number(
                v_line.physical_line,
                self.left_padding,
                num_right_pad,
                y,
                1.0,
                self.theme.line_num,
            );
        }

        let blame_column_w = crate::render_view::blame_column_width(
            editor, is_ide_mode, show_welcome, active_git_diff_state.is_some(), s,
        );
        if blame_column_w > 0.0 {
            self.draw_git_blame_column(
                editor, ui_registry, layout, editor_text, gutter_x, blame_column_w,
            );
        }

        for i in 0..self.merged_intervals_cache.len() {
            let m = self.merged_intervals_cache[i];
            if m.bottom < 0.0 || m.top > real_height {
                continue;
            }
            let color = mod_interval_color(&self.theme, m);
            let draw_top = m.top + 2.0;
            let draw_bottom = m.bottom + 2.0;
            let draw_h = (draw_bottom - draw_top).max(4.0);
            self.push_rounded_rect(
                self.left_padding - 8.0 * s,
                draw_top,
                7.0 * s,
                draw_h,
                2.0 * s,
                color,
            );
        }

        if active_git_diff_state.is_none()
            && is_ide_mode
            && !editor.git_hunks.is_empty()
            && !show_welcome
        {
            for i in skip_visual_lines..end_visual_line {
                let v_line = self.visual_lines[i];
                let phys_idx = v_line.physical_line - 1;
                let Some(hunk_idx) = editor.git_hunk_index_at_line(phys_idx) else {
                    continue;
                };
                let y_top = v_line.y_offset - render_scroll_y;
                ui_registry.register_rect_clipped(
                    crate::ui_system::UiId::EditorGitHunk(hunk_idx, phys_idx),
                    self.left_padding - 14.0 * s,
                    y_top,
                    16.0 * s,
                    self.line_height,
                    gutter_hit_clip,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }
        }

        gutter_x
    }

    fn draw_git_blame_column(
        &mut self,
        editor: &Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        layout: RootFramePanelLayout<'_>,
        editor_text: RootFrameEditorText<'_>,
        gutter_x: f32,
        column_width: f32,
    ) {
        let Some(blame) = editor.git_blame.blame.as_ref() else { return };
        let s = layout.s;
        let column_x = gutter_x.round() + 1.0;
        let strip_color = self.ui.pick(crate::theme::UiRole::GitCommit, [0.42, 0.66, 1.0, 1.0]);
        let gutter_hit_clip = crate::ui_system::UiClipRect::new(
            0.0,
            editor_text.editor_clip_y,
            self.width,
            editor_text.editor_clip_h,
        );
        let gutter_bottom = editor_text.editor_clip_y + editor_text.editor_clip_h;
        let mut previous_phys_idx = None;
        let mut previous_buffer_commit = None;
        for i in editor_text.skip_visual_lines..editor_text.end_visual_line {
            let v_line = self.visual_lines[i];
            let phys_idx = v_line.physical_line.saturating_sub(1);
            let line_top = v_line.y_offset - editor_text.render_scroll_y;
            if line_top + self.line_height <= editor_text.editor_clip_y || line_top >= gutter_bottom {
                continue;
            }
            let first_visual_segment = i == 0
                || self.visual_lines[i - 1].physical_line != v_line.physical_line;
            let Some(head_line) = crate::editor::head_line_for(&editor.git_hunks, phys_idx) else {
                if first_visual_segment {
                    previous_phys_idx = Some(phys_idx);
                    previous_buffer_commit = None;
                }
                continue;
            };
            let Some(&commit_idx) = blame.line_commit.get(head_line) else {
                if first_visual_segment {
                    previous_phys_idx = Some(phys_idx);
                    previous_buffer_commit = None;
                }
                continue;
            };
            if blame.commits.get(commit_idx as usize).is_none_or(|commit| commit.uncommitted) {
                if first_visual_segment {
                    previous_phys_idx = Some(phys_idx);
                    previous_buffer_commit = Some(Some(commit_idx));
                }
                continue;
            }
            if first_visual_segment {
                if previous_phys_idx != Some(phys_idx.saturating_sub(1)) {
                    previous_buffer_commit = if phys_idx == 0 {
                        Some(None)
                    } else {
                        crate::editor::head_line_for(&editor.git_hunks, phys_idx - 1)
                            .and_then(|previous_head| blame.line_commit.get(previous_head).copied())
                            .map(Some)
                    };
                }
                previous_phys_idx = Some(phys_idx);
            }
            let rank = blame.age_ranks.get(commit_idx as usize).copied().unwrap_or(0.0);
            let mut color = strip_color;
            color[3] = 0.18 + rank.clamp(0.0, 1.0) * 0.72;
            self.push_rect(column_x, line_top.round(), (3.0 * s).round().max(1.0), self.line_height.round(), color);

            if first_visual_segment
                && previous_buffer_commit != Some(Some(commit_idx))
                && let Some(label) = blame.column_labels.get(commit_idx as usize)
            {
                let label_x = (column_x + 6.0 * s).round();
                let baseline = (self.baseline_offset + v_line.y_offset - editor_text.render_scroll_y).round();
                self.draw_string_scaled_stable(label, label_x, baseline, self.theme.line_num, 0.82);
            }
            if first_visual_segment {
                ui_registry.register_rect_clipped(
                    crate::ui_system::UiId::EditorBlameColumnRow(phys_idx),
                    column_x,
                    line_top,
                    column_width,
                    self.line_height,
                    gutter_hit_clip,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
                previous_buffer_commit = Some(Some(commit_idx));
            }
        }
    }

    fn prepare_git_blame_column_width(&mut self, editor: &mut Editor) {
        let generation = editor.git_blame.generation;
        if !editor.git_blame.column_open
            || editor.git_blame.column_width_generation == Some(generation)
        {
            return;
        }
        let Some(blame) = editor.git_blame.blame.as_ref() else { return };
        let mut text_width = 0.0f32;
        for label in &blame.column_labels {
            text_width = text_width.max(self.measure_ui_width_at_pixel_size(label, 18.0 * 0.82));
        }
        editor.git_blame.column_width = if text_width > 0.0 { text_width + 12.0 } else { 0.0 };
        editor.git_blame.column_width_generation = Some(generation);
    }
}
