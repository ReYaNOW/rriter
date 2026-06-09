use crate::editor::Editor;
use crate::renderer::Renderer;
use std::sync::atomic::Ordering;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_lsp_squiggles_and_collect_hovered_diag(
        &mut self,
        editor: &Editor,
        lsp_diagnostics: &[&crate::lsp::Diagnostic],
        scroll_x: f32,
        render_scroll_y: f32,
        panel_bottom_h: f32,
        is_ide_mode: bool,
        is_ui_disabled: bool,
        ide_panel: &crate::app::IdePanelState,
        mx: f32,
        my: f32,
        mouse_in_popup: bool,
    ) -> Option<usize> {
        // LSP squiggles — волнистые подчёркивания диагностик
        crate::app::mouse::HOVER_STATE.with(|s| {
            let mut state = s.borrow_mut();
            if !state.stale_combined_popup && !state.has_active_combined_type_popup() {
                state.hovered_diags_cache.clear();
            }
        });
        let mut hovered_diag_type_target = None;
        if !self.lsp_diagnostic_indices.is_empty() {
            let render_scroll_x = scroll_x.round();
            let hover_content_y = crate::app::mouse::hover_screen_y_to_content_y(
                my,
                render_scroll_y,
                self.line_height,
                self.baseline_offset,
            )
            .unwrap_or(-1.0);
            let mut effective_bottom_h = panel_bottom_h;
            if is_ide_mode && ide_panel.is_open(crate::app::PanelId::Terminal) && !is_ui_disabled {
                effective_bottom_h = 0.0;
            }
            let blocking_bottom_y = if effective_bottom_h > 0.0 {
                Some(crate::render_view::ide_bottom_panel_y(
                    self.height,
                    effective_bottom_h,
                    self.scale_factor,
                ))
            } else {
                None
            };
            for i in 0..self.lsp_diagnostic_indices.len() {
                let idx = self.lsp_diagnostic_indices[i];
                let diag = lsp_diagnostics[idx];
                // Цвет по severity
                let color: [f32; 4] = match diag.severity {
                    crate::lsp::DiagSeverity::Error => [0.96, 0.26, 0.21, 0.90],
                    crate::lsp::DiagSeverity::Warning => [0.95, 0.9, 0.3, 0.90],
                    crate::lsp::DiagSeverity::Info => [0.26, 0.73, 0.90, 0.80],
                    crate::lsp::DiagSeverity::Hint => [0.50, 0.50, 0.50, 0.70],
                };
                let line = diag.start_line as usize;
                if line >= editor.line_offsets.len() {
                    continue;
                }

                let mut v_line_opt = None;
                for vl in &self.visual_lines {
                    if vl.physical_line == line + 1 {
                        v_line_opt = Some(*vl);
                        break;
                    }
                }
                let v_line = match v_line_opt {
                    Some(vl) => vl,
                    None => continue,
                };

                let line_y = self.baseline_offset + v_line.y_offset - render_scroll_y;
                let squiggle_y = line_y + 2.0 * self.scale_factor;

                // Точный расчёт X-позиции: идём по символам строки, считая UTF-16 единицы
                let avg_adv = self.char_advance('a');
                let display_end_col = if diag.end_line == diag.start_line {
                    diag.end_col
                } else {
                    u32::MAX
                };
                let diag_hover_range = crate::app::mouse::diagnostic_hover_byte_range_on_line(
                    editor,
                    line,
                    diag.start_col,
                    display_end_col,
                );
                let hit_type_target = diag_hover_range.map(|range| range.2);

                let (x_start_px, start_byte) =
                    self.visual_x_for_utf16_col(editor, line, diag.start_col, true);
                let (mut x_end_px, end_byte) =
                    self.visual_x_for_utf16_col(editor, line, display_end_col, false);
                if diag.end_line != diag.start_line {
                    x_end_px = x_end_px.max(x_start_px + avg_adv * 4.0);
                }

                let x_start = self.left_padding + x_start_px - render_scroll_x;
                let x_end = self.left_padding + x_end_px - render_scroll_x;
                let top_y = v_line.y_offset - render_scroll_y;

                let mut in_hitbox = false;
                let is_under_panel = blocking_bottom_y
                    .map(|panel_y| my >= panel_y && my <= panel_y + effective_bottom_h)
                    .unwrap_or(false);

                if !self.hide_popups_until_mouse_move && !is_under_panel {
                    let squiggle_hit_y_top = v_line.y_offset;

                    if mouse_in_popup {
                        if crate::app::mouse::HOVER_STATE
                            .with(|s| s.borrow().hovered_diags.contains(&idx))
                        {
                            in_hitbox = true;
                        }
                    } else if crate::app::mouse::hover_content_y_in_line_hitbox(
                        hover_content_y,
                        squiggle_hit_y_top,
                        self.line_height,
                    ) {
                        let line_x = mx - self.left_padding + render_scroll_x;
                        in_hitbox = self.visual_text_range_contains_x(
                            editor,
                            editor.line_offsets[line],
                            start_byte,
                            end_byte,
                            line_x,
                            avg_adv / 2.0,
                        );
                    }
                }

                if in_hitbox {
                    let target_to_record = if mouse_in_popup {
                        crate::app::mouse::HOVER_STATE
                            .with(|s| s.borrow().combined_type_target())
                            .or(hit_type_target)
                    } else {
                        let visual_line_x = mx - self.left_padding + render_scroll_x;
                        let line_x = self.text_x_for_visual_line_x(editor, line, visual_line_x);
                        let target_under_cursor =
                            crate::app::mouse::diagnostic_hover_type_target_at_x(
                                editor,
                                line,
                                line_x,
                                hit_type_target,
                                |ch| self.char_advance(ch),
                            );
                        crate::app::mouse::HOVER_STATE
                            .with(|s| s.borrow().byte_offset)
                            .or(target_under_cursor)
                    };
                    if hovered_diag_type_target.is_none() {
                        hovered_diag_type_target = crate::app::mouse::HOVER_STATE.with(|s| {
                            s.borrow_mut().record_hovered_diagnostic(
                                (idx, x_start, top_y, top_y + self.line_height, x_end),
                                target_to_record,
                            )
                        });
                    } else {
                        crate::app::mouse::HOVER_STATE.with(|s| {
                            s.borrow_mut().record_hovered_diagnostic(
                                (idx, x_start, top_y, top_y + self.line_height, x_end),
                                target_to_record,
                            );
                        });
                    }
                }

                if x_end < self.left_padding || x_start > self.width {
                    continue;
                }

                let line_start = editor.line_offsets[line];
                let mut segment_start_x_px = x_start_px;
                let mut hint_idx = self
                    .current_python_inlay_hints
                    .partition_point(|hint| hint.byte_offset < start_byte);
                let mut pushed_segment = false;
                while hint_idx < self.current_python_inlay_hints.len() {
                    let hint_offset = self.current_python_inlay_hints[hint_idx].byte_offset;
                    if hint_offset >= end_byte {
                        break;
                    }
                    let before_hint_x_px =
                        self.visual_x_for_byte_offset(editor, line_start, hint_offset, false);
                    let segment_w = before_hint_x_px - segment_start_x_px;
                    if segment_w > avg_adv * 0.25 {
                        self.push_squiggle(
                            (self.left_padding + segment_start_x_px - render_scroll_x)
                                .max(self.left_padding),
                            squiggle_y,
                            segment_w,
                            color,
                        );
                        pushed_segment = true;
                    }
                    while hint_idx < self.current_python_inlay_hints.len()
                        && self.current_python_inlay_hints[hint_idx].byte_offset == hint_offset
                    {
                        hint_idx += 1;
                    }
                    segment_start_x_px =
                        self.visual_x_for_byte_offset(editor, line_start, hint_offset, true);
                }

                let min_final_w = if pushed_segment { 0.0 } else { avg_adv / 2.0 };
                let final_w = (x_end_px - segment_start_x_px).max(min_final_w);
                if final_w > 0.5 {
                    self.push_squiggle(
                        (self.left_padding + segment_start_x_px - render_scroll_x)
                            .max(self.left_padding),
                        squiggle_y,
                        final_w,
                        color,
                    );
                }
            }
            self.flush();
        }
        hovered_diag_type_target
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_hover_overlays(
        &mut self,
        editor: &Editor,
        lsp_diagnostics: &[&crate::lsp::Diagnostic],
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        scroll_x: f32,
        render_scroll_y: f32,
        mut hovered_diag_type_target: Option<usize>,
        wants_pointer: &mut bool,
        clip_rect: Option<(f32, f32, f32, f32)>,
    ) {
        // --- LSP Diagnostic Tooltip ---
        let hovered_diags_cache_empty =
            crate::app::mouse::HOVER_STATE.with(|s| s.borrow().diagnostic_popup_cache_is_empty());
        if hovered_diags_cache_empty {
            crate::app::mouse::HOVER_STATE.with(|s| {
                let mut state = s.borrow_mut();
                state.diag_rect = None;
                state.hovered_diags.clear();
            });
        }

        let now = std::time::Instant::now();
        let popup_dt = self
            .last_draw_instant
            .map(|t| now.duration_since(t).as_secs_f32().min(0.1))
            .unwrap_or(0.0);
        self.last_draw_instant = Some(now);

        let (
            has_type_popup,
            is_hover_pending,
            hover_timer,
            has_byte_offset,
            hover_byte_offset,
            type_popup_byte,
        ) = crate::app::mouse::HOVER_STATE.with(|s| {
            let state = s.borrow();
            (
                state.popup.is_some(),
                state.request_id.is_some() || state.definition_request_id.is_some(),
                state.timer,
                state.byte_offset.is_some(),
                state
                    .byte_offset
                    .or_else(|| state.popup.as_ref().map(|p| p.byte_offset)),
                state.popup.as_ref().map(|p| p.byte_offset),
            )
        });

        if crate::app::mouse::HOVER_STATE.with(|s| s.borrow().hovered_diags_cache.is_empty()) {
            if let Some(byte_offset) = hover_byte_offset {
                let hover_line = editor
                    .line_offsets
                    .partition_point(|&o| o <= byte_offset)
                    .saturating_sub(1);

                if hover_line < editor.line_offsets.len() {
                    let mut found = None;

                    for i in 0..self.lsp_diagnostic_indices.len() {
                        let idx = self.lsp_diagnostic_indices[i];
                        let diag = lsp_diagnostics[idx];
                        if hover_line < diag.start_line as usize
                            || hover_line > diag.end_line as usize
                        {
                            continue;
                        }

                        let display_start_col = if hover_line == diag.start_line as usize {
                            diag.start_col
                        } else {
                            0
                        };
                        let display_end_col = if hover_line == diag.end_line as usize {
                            diag.end_col
                        } else {
                            u32::MAX
                        };

                        if let Some((hit_start_byte, hit_end_byte, type_target)) =
                            crate::app::mouse::diagnostic_hover_byte_range_on_line(
                                editor,
                                hover_line,
                                display_start_col,
                                display_end_col,
                            )
                        {
                            if byte_offset >= hit_start_byte && byte_offset < hit_end_byte {
                                found = Some((
                                    idx,
                                    diag,
                                    display_start_col,
                                    display_end_col,
                                    type_target,
                                ));
                                break;
                            }
                        }
                    }

                    if let Some((idx, _diag, display_start_col, display_end_col, type_target)) =
                        found
                    {
                        let mut v_line_opt = None;
                        for vl in &self.visual_lines {
                            if vl.physical_line == hover_line + 1 {
                                v_line_opt = Some(*vl);
                                break;
                            }
                        }

                        if let Some(v_line) = v_line_opt {
                            let render_scroll_x = scroll_x.round();
                            let top_y = v_line.y_offset - render_scroll_y;
                            let avg_adv = self.char_advance('a');
                            let mut x_start_px = 0.0f32;
                            let mut x_end_px = 0.0f32;
                            let mut cur_x = 0.0f32;
                            let mut start_found = false;
                            let mut end_found = false;

                            editor.utf16_col_to_byte_advance(
                                hover_line,
                                |ch, utf16_before, _pos| {
                                    if !start_found && utf16_before >= display_start_col {
                                        x_start_px = cur_x;
                                        start_found = true;
                                    }
                                    if !end_found && utf16_before >= display_end_col {
                                        x_end_px = cur_x;
                                        end_found = true;
                                    }
                                    cur_x += if ch == '\t' {
                                        self.char_advance(' ') * 4.0
                                    } else {
                                        self.char_advance(ch)
                                    };
                                },
                            );

                            if !start_found {
                                x_start_px = cur_x;
                            }
                            if !end_found {
                                x_end_px = cur_x.max(x_start_px + avg_adv * 4.0);
                            }

                            hovered_diag_type_target = crate::app::mouse::HOVER_STATE.with(|s| {
                                s.borrow_mut().record_hovered_diagnostic(
                                    (
                                        idx,
                                        self.left_padding + x_start_px - render_scroll_x,
                                        top_y,
                                        top_y + self.line_height,
                                        self.left_padding + x_end_px - render_scroll_x,
                                    ),
                                    Some(type_target),
                                )
                            });
                        }
                    }
                }
            }
        }

        let first_idx = crate::app::mouse::HOVER_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.update_hovered_diag_type_target_for_frame(hovered_diag_type_target);
            state.hovered_diags.clear();
            if state.stale_combined_popup && !state.stale_hovered_diags_cache.is_empty() {
                for i in 0..state.stale_hovered_diags_cache.len() {
                    let idx = state.stale_hovered_diags_cache[i].0;
                    state.hovered_diags.push(idx);
                }
            } else {
                for i in 0..state.hovered_diags_cache.len() {
                    let idx = state.hovered_diags_cache[i].0;
                    state.hovered_diags.push(idx);
                }
            }
            state.hovered_diags.first().copied()
        });

        let type_in_progress = has_byte_offset
            && !has_type_popup
            && (hover_timer < crate::app::mouse::HOVER_REQUEST_DELAY_SEC || is_hover_pending);
        let is_error_hovered =
            crate::app::mouse::HOVER_STATE.with(|s| !s.borrow().diagnostic_popup_cache_is_empty());
        let effective_hovered_diag_type_target = crate::app::mouse::HOVER_STATE.with(|s| {
            s.borrow()
                .effective_hovered_diag_type_target(hovered_diag_type_target)
        });
        let diagnostic_needs_type =
            is_error_hovered && effective_hovered_diag_type_target.is_some();
        let type_matches_diag = crate::app::mouse::hover_bytes_share_token(
            editor,
            effective_hovered_diag_type_target,
            type_popup_byte,
        );
        let hover_matches_diag = crate::app::mouse::hover_bytes_share_token(
            editor,
            effective_hovered_diag_type_target,
            hover_byte_offset,
        );
        let type_matches_hover =
            crate::app::mouse::hover_bytes_share_token(editor, type_popup_byte, hover_byte_offset);

        let error_timer_ready = crate::app::mouse::HOVER_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.advance_diagnostic_hover_timer(
                first_idx,
                has_type_popup,
                type_in_progress,
                popup_dt,
            )
        });

        let (show_error, show_type, show_combined) =
            crate::app::mouse::compute_hover_visibility_from_matches(
                is_error_hovered,
                error_timer_ready,
                has_type_popup,
                diagnostic_needs_type,
                type_matches_diag,
                hover_matches_diag,
                type_matches_hover,
                crate::app::mouse::HOVER_STATE.with(|s| s.borrow().stale_combined_popup),
            );

        let show_placeholder_type = crate::app::mouse::HOVER_STATE.with(|s| {
            s.borrow()
                .should_show_stale_popup_while_target_loads(show_type)
        });

        if crate::render_view::hover_trace_enabled() {
            static LAST_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let last_log = LAST_LOG.load(Ordering::Relaxed);
            if (is_error_hovered || has_type_popup || hover_byte_offset.is_some())
                && now_ms - last_log > 500
            {
                let (stale, popup_diag) = crate::app::mouse::HOVER_STATE.with(|s| {
                    let state = s.borrow();
                    (state.stale_combined_popup, state.popup_diag_type_target)
                });
                println!(
                    "[HOVER VIS LOG] is_error: {}, timer_ready: {}, has_type: {}, d_type_target: {:?}, type_byte: {:?}, hover_byte: {:?}, stale: {}, popup_diag: {:?}, SHOW_ERR: {}, SHOW_TYPE: {}, SHOW_COMB: {}, SHOW_PLACEHOLDER: {}",
                    is_error_hovered,
                    error_timer_ready,
                    has_type_popup,
                    effective_hovered_diag_type_target,
                    type_popup_byte,
                    hover_byte_offset,
                    stale,
                    popup_diag,
                    show_error,
                    show_type,
                    show_combined,
                    show_placeholder_type
                );
                LAST_LOG.store(now_ms, Ordering::Relaxed);
            }
        }

        let (attached_hover_w, attached_hover_h) = if show_combined {
            crate::app::mouse::HOVER_STATE.with(|s| {
                let mut state = s.borrow_mut();
                if let Some(popup) = state.popup.as_mut() {
                    let scale = self.scale_factor;
                    let pad = 12.0 * scale;
                    let line_h = 22.0 * scale;
                    let max_text_w = (self.width - 80.0 * scale)
                        .min(820.0 * scale)
                        .max(320.0 * scale);
                    let cache_valid = popup.layout_cache.as_ref().is_some_and(|cache| {
                        cache.scale_factor == self.scale_factor
                            && cache.max_text_w == max_text_w
                            && cache.span_count == popup.spans.len()
                            && cache.text_len == popup.text.len()
                    });
                    if !cache_valid {
                        popup.layout_cache =
                            Some(self.build_hover_popup_layout(popup, max_text_w, line_h));
                    }
                    if let Some(layout) = popup.layout_cache.as_ref() {
                        (
                            layout.max_line_w + pad * 2.0,
                            (self.height * 0.35).min(layout.total_text_h + pad * 2.0),
                        )
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                }
            })
        } else {
            (0.0, 0.0)
        };

        if !show_type && !show_placeholder_type {
            crate::app::mouse::HOVER_STATE.with(|s| {
                s.borrow_mut().rect = None;
            });
        }

        if show_error {
            self.draw_diagnostic_popup(
                lsp_diagnostics,
                ide_panel,
                ui_registry,
                attached_hover_w,
                attached_hover_h,
                clip_rect,
                mx,
                my,
                wants_pointer,
            );
        } else if is_error_hovered {
            crate::app::mouse::HOVER_STATE.with(|s| {
                s.borrow_mut().hide_diagnostic_popup_until_ready();
            });
        } else {
            crate::app::mouse::HOVER_STATE.with(|s| {
                s.borrow_mut().reset_diagnostic_popup();
            });
        }

        if show_type || show_placeholder_type {
            let (mut popup, selection, attached_diag) = crate::app::mouse::HOVER_STATE
                .with(|s| s.borrow_mut().take_type_popup_for_draw(show_combined));
            if let Some(popup_ref) = popup.as_mut() {
                let (bx, by, bw, bh, ms) = self.draw_hover_popup(
                    popup_ref,
                    attached_diag,
                    selection,
                    editor,
                    ui_registry,
                    mx,
                    my,
                    render_scroll_y,
                    wants_pointer,
                    1.0,
                    None,
                    clip_rect,
                );
                crate::app::mouse::HOVER_STATE.with(|s| {
                    let mut state = s.borrow_mut();
                    state.put_type_popup_after_draw(popup, Some((bx, by, bw, bh)), ms);
                    state.mark_type_popup_drawn(show_combined, effective_hovered_diag_type_target);
                });
            } else {
                crate::app::mouse::HOVER_STATE.with(|s| {
                    let mut state = s.borrow_mut();
                    state.put_type_popup_after_draw(None, None, 0.0);
                    state.mark_type_popup_drawn(false, None);
                });
            }
        } else {
            crate::app::mouse::HOVER_STATE.with(|s| {
                let mut state = s.borrow_mut();
                state.rect = None;
                state.mark_type_popup_drawn(false, None);
            });
        }
    }
}
