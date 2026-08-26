pub(crate) fn intersect_scissor_boxes(first: [i32; 4], second: [i32; 4]) -> [i32; 4] {
    let left = first[0].max(second[0]);
    let bottom = first[1].max(second[1]);
    let right = first[0]
        .saturating_add(first[2].max(0))
        .min(second[0].saturating_add(second[2].max(0)));
    let top = first[1]
        .saturating_add(first[3].max(0))
        .min(second[1].saturating_add(second[3].max(0)));
    [left, bottom, (right - left).max(0), (top - bottom).max(0)]
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn one_line_cursor_from_x(
        &mut self,
        text: &str,
        x_offset: f32,
        text_scale: f32,
    ) -> usize {
        let mut current_x = 0.0;
        for (byte_idx, ch) in text.char_indices() {
            let advance = self
                .get_ui_glyph(ch)
                .map(|glyph| Self::snapped_text_advance(glyph.advance, text_scale))
                .unwrap_or_else(|| (10.0 * text_scale).round().max(1.0));
            if x_offset <= current_x + advance * 0.5 {
                return byte_idx;
            }
            current_x = (current_x + advance).round();
        }
        text.len()
    }

    pub(crate) fn one_line_scroll_for_cursor(
        &mut self,
        text: &str,
        cursor: usize,
        text_scale: f32,
        visible_width: f32,
        mut scroll_x: f32,
    ) -> f32 {
        let mut cursor_x = 0.0;
        let mut total_width = 0.0;
        for (byte_idx, ch) in text.char_indices() {
            let advance = self
                .get_ui_glyph(ch)
                .map(|glyph| Self::snapped_text_advance(glyph.advance, text_scale))
                .unwrap_or(10.0 * text_scale);
            if byte_idx < cursor {
                cursor_x += advance;
            }
            total_width += advance;
        }
        let visible_width = visible_width.max(1.0);
        if cursor_x - scroll_x > visible_width {
            scroll_x = cursor_x - visible_width;
        } else if cursor_x < scroll_x {
            scroll_x = cursor_x;
        }
        scroll_x
            .min((total_width - visible_width).max(0.0))
            .max(0.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_ide_bottom_panel(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        lsp_has_diagnostics: bool,
        s: f32,
        mx: f32,
        my: f32,
        panel_bottom_h: f32,
        is_ui_disabled: bool,
        blink_alpha: f32,
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
    ) {
        let sb_w = 48.0 * s;
        let panel_x = sb_w;
        let panel_y = ide_bottom_panel_y(self.height, panel_bottom_h, s);
        let panel_w = self.width - panel_x;

        let uses_translucent_bg = ide_panel.slots.iter().any(|sl| {
            sl.group == crate::app::PanelGroup::Bottom
                && sl.open
                && (sl.id == crate::app::PanelId::Terminal
                    || sl.id == crate::app::PanelId::Problems)
        });
        // Прозрачность терминала/ляпов (0.0 - полностью прозрачный, 1.0 - непрозрачный)
        let panel_alpha = if uses_translucent_bg { 0.80 } else { 1.0 };

        let panel_bg = [
            0.129, // #21
            0.133, // #22
            0.173, // #2c
            panel_alpha,
        ];
        // Ручка ресайза (1px линия вверху панели)self.push_rect(panel_x, panel_y, panel_w, 1.0,[1.0, 1.0, 1.0, 0.15]);
        self.push_rect(
            panel_x,
            panel_y + 1.0,
            panel_w,
            panel_bottom_h - 1.0,
            panel_bg,
        );

        let blocked = ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            panel_x,
            panel_y,
            panel_w,
            panel_bottom_h,
            mx,
            my,
        );
        if blocked {
            ui_registry.reset_cursor_state();
        }

        let tab_h = 32.0 * s;
        let tab_bar_bg = [
            (self.theme.bg[0] + 0.07).min(1.0),
            (self.theme.bg[1] + 0.07).min(1.0),
            (self.theme.bg[2] + 0.08).min(1.0),
            panel_alpha,
        ];
        self.push_rect(panel_x, panel_y + 1.0, panel_w, tab_h, tab_bar_bg);

        let mut tx = panel_x + 8.0 * s;
        for (i, slot) in ide_panel
            .slots
            .iter()
            .filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
            .enumerate()
        {
            let label = slot.id.label();
            let tw = self.measure_ui_width(label, 0.9) + 20.0 * s;
            if i == 0 {
                let act_bg = [
                    (self.theme.bg[0] + 0.12).min(1.0),
                    (self.theme.bg[1] + 0.12).min(1.0),
                    (self.theme.bg[2] + 0.13).min(1.0),
                    1.0,
                ];
                self.push_rect(tx, panel_y + 1.0, tw, tab_h, act_bg);
                self.push_rect(tx, panel_y + tab_h - 1.0, tw, 2.0, [0.60, 0.35, 0.85, 1.0]);
            }
            self.draw_string_scaled(
                label,
                tx + 10.0 * s,
                panel_y + 1.0 + tab_h / 2.0 + 5.5 * s,
                self.theme.fg,
                0.9,
            );
            tx += tw;
        }

        // Подсветка ручки ресайза при наведении (wants_pointer=false — курсор через NsResize)
        if my >= panel_y - 8.0 * s && my <= panel_y + 8.0 * s && mx >= panel_x {
            self.push_rect(panel_x, panel_y, panel_w, 2.0, [0.60, 0.35, 0.85, 0.4]);
        }

        let content_y = panel_y + 1.0 + tab_h;
        let content_h = panel_bottom_h - 1.0 - tab_h;
        if content_h > 8.0 * s {
            if let Some(slot) = ide_panel
                .slots
                .iter()
                .find(|slot| slot.group == crate::app::PanelGroup::Bottom && slot.open)
            {
                self.draw_ide_panel_content(
                    slot.id,
                    panel_x,
                    content_y,
                    panel_w,
                    content_h,
                    s,
                    ide_panel,
                    lsp,
                    ui_registry,
                    lsp_has_diagnostics,
                    mx,
                    my,
                    is_ui_disabled,
                    blink_alpha,
                    active_api_route,
                );
            }
        }
    }

    pub(crate) fn draw_status_bar(
        &mut self,
        editor: &crate::editor::Editor,
        editor_file: Option<(&std::path::PathBuf, crate::platform::TextEncoding)>,
        markdown_mode: crate::app::MarkdownMode,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        s: f32,
        mx: f32,
        my: f32,
        panel_bottom_h: f32,
        progress_label: Option<&str>,
        progress_elapsed_secs: Option<f32>,
        progress_value: Option<f32>,
    ) {
        let bar_h = ide_status_bar_height(s).round();
        let bar_y = ide_status_bar_y(self.height, panel_bottom_h, s).round();
        let bar_x = (48.0 * s).round();
        let bar_w = (self.width - bar_x).max(0.0);
        if bar_w <= 1.0 || bar_h <= 1.0 {
            return;
        }

        self.push_rect(bar_x, bar_y, bar_w, bar_h, [0.118, 0.125, 0.165, 1.0]);
        self.push_rect(
            bar_x,
            bar_y,
            bar_w,
            1.0,
            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.12],
        );
        ui_registry.register_blocker(
            crate::ui_system::UiId::StatusBar,
            bar_x,
            bar_y,
            bar_w,
            bar_h,
            mx,
            my,
        );

        let (error_count, warning_count) = lsp
            .map(crate::lsp::LspManager::total_diagnostic_counts)
            .unwrap_or((0, 0));

        let icon_sz = 20.0 * s;
        let text_scale = 0.95;
        let pad_x = 10.0 * s;
        let icon_gap = 5.0 * s;
        let item_gap = 16.0 * s;
        let diag_x = bar_x + pad_x;
        let icon_y = bar_y + (bar_h - icon_sz) / 2.0;
        let text_y = bar_y + bar_h / 2.0 + 5.0 * s;
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        let ext = editor_file
            .and_then(|(path, _)| path.extension())
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let language = language_display_name_for_ext(ext);
        let language_w = self.measure_ui_width(language, text_scale).round();
        let status_markdown_mode = markdown_status_mode_for_ext(ext, markdown_mode);
        let mode_text_w = status_markdown_mode
            .map(markdown_status_mode_label)
            .map(|label| self.measure_ui_width(label, 0.82).round());
        let bar_rect = crate::ui_system::UiClipRect::new(bar_x, bar_y, bar_w, bar_h);
        let language_layout = status_language_layout(bar_rect, language_w, mode_text_w, s);
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", error_count));
        let error_w = self.measure_ui_width(&scratch, text_scale).round();
        scratch.clear();
        let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", warning_count));
        let warning_w = self.measure_ui_width(&scratch, text_scale).round();
        let diagnostics_w =
            icon_sz + icon_gap + error_w + item_gap + icon_sz + icon_gap + warning_w + pad_x;
        let diagnostics_right = diag_x + diagnostics_w;
        let show_diagnostics =
            status_diagnostics_fit(language_layout, diagnostics_right, s);
        let diagnostics_hovered = if show_diagnostics {
            let hovered = ui_registry.register_rect(
                crate::ui_system::UiId::StatusDiagnostics,
                diag_x - 4.0 * s,
                bar_y,
                diagnostics_w,
                bar_h,
                mx,
                my,
            );
            if hovered {
                self.push_rect(
                    diag_x - 4.0 * s,
                    bar_y,
                    diagnostics_w,
                    bar_h,
                    [1.0, 1.0, 1.0, 0.07],
                );
            }
            self.draw_atlas_icon(
                crate::widgets::IconType::Error,
                diag_x,
                icon_y,
                icon_sz,
                [1.0, 1.0, 1.0, 1.0],
            );
            scratch.clear();
            let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", error_count));
            let error_text_x = diag_x + icon_sz + icon_gap;
            self.draw_string_scaled(&scratch, error_text_x, text_y, self.theme.fg, text_scale);
            let warn_icon_x = error_text_x + error_w + item_gap;
            self.draw_atlas_icon(
                crate::widgets::IconType::Warning,
                warn_icon_x,
                icon_y,
                icon_sz,
                [1.0, 1.0, 1.0, 1.0],
            );
            scratch.clear();
            let _ = std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", warning_count));
            self.draw_string_scaled(
                &scratch,
                warn_icon_x + icon_sz + icon_gap,
                text_y,
                self.theme.fg,
                text_scale,
            );
            hovered
        } else {
            false
        };
        let left_status_limit = if show_diagnostics {
            diagnostics_right
        } else {
            bar_x + pad_x
        };
        let position_group_right = self.draw_status_language_group(
            language,
            editor_file.map(|(_, encoding)| encoding),
            status_markdown_mode,
            language_layout,
            left_status_limit,
            ui_registry,
            bar_rect,
            s,
            mx,
            my,
            text_y,
            text_scale,
        );
        let (line, character) = cursor_line_and_character(editor);
        const ZERO_SAMPLE: &str = "00000000000000000000";
        let item_gap = 14.0 * s;
        let digit_gap = 4.0 * s;
        let line_digits = line.to_string();
        let char_digits = character.to_string();
        let line_digits_w = self
            .measure_mono_width(
                &ZERO_SAMPLE[..line_digits.len().max(2).min(ZERO_SAMPLE.len())],
                text_scale,
            )
            .round();
        let char_digits_w = self
            .measure_mono_width(
                &ZERO_SAMPLE[..char_digits.len().max(2).min(ZERO_SAMPLE.len())],
                text_scale,
            )
            .round();
        let line_label_w = self.measure_ui_width("Стр", text_scale).round();
        let char_label_w = self.measure_ui_width("Сим", text_scale).round();
        let line_block_w = line_label_w + digit_gap + line_digits_w;
        let char_block_w = char_label_w + digit_gap + char_digits_w;
        let selected_count = selected_char_count(editor);
        let selected_count_digits = selected_count.map(|count| count.to_string());
        let selected_block_w = selected_count_digits
            .as_ref()
            .map(|digits| {
                self.measure_ui_width("(", text_scale).round()
                    + self
                        .measure_mono_width(
                            &ZERO_SAMPLE[..digits.len().max(2).min(ZERO_SAMPLE.len())],
                            text_scale,
                        )
                        .round()
                    + self.measure_ui_width(" выделено)", text_scale).round()
            })
            .unwrap_or(0.0);
        let pos_color = self.theme.fg;
        let mut group_w = line_block_w + item_gap + char_block_w;
        if selected_block_w > 0.0 {
            group_w += item_gap + selected_block_w;
        }
        let line_x = position_group_right - 22.0 * s - group_w;
        if let Some(label) = progress_label {
            let label_w = self.measure_ui_width(label, 0.82).round();
            let progress_gap = 8.0 * s;
            let track_w = 74.0 * s;
            let track_h = 5.0 * s;
            scratch.clear();
            if let Some(elapsed) = progress_elapsed_secs {
                if elapsed >= 60.0 {
                    let minutes = (elapsed / 60.0).floor() as u64;
                    let seconds = (elapsed as u64) % 60;
                    let _ = std::fmt::Write::write_fmt(
                        &mut scratch,
                        format_args!("{minutes}:{seconds:02}"),
                    );
                } else {
                    let _ = std::fmt::Write::write_fmt(
                        &mut scratch,
                        format_args!("{elapsed:.1}s"),
                    );
                }
            }
            let elapsed_w = if scratch.is_empty() {
                0.0
            } else {
                self.measure_ui_width(&scratch, 0.76).round()
            };
            let elapsed_gap = if elapsed_w > 0.0 { 7.0 * s } else { 0.0 };
            let progress_w = label_w + elapsed_gap + elapsed_w + progress_gap + track_w;
            let progress_x = line_x - 18.0 * s - progress_w;
            if progress_x > left_status_limit + 8.0 * s {
                let elapsed_x = progress_x + label_w + elapsed_gap;
                let track_x = elapsed_x + elapsed_w + progress_gap;
                let track_y = bar_y + (bar_h - track_h) / 2.0;
                self.draw_string_scaled(
                    label,
                    progress_x,
                    text_y,
                    [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.72],
                    0.82,
                );
                if elapsed_w > 0.0 {
                    self.draw_string_scaled(
                        &scratch,
                        elapsed_x,
                        text_y,
                        [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.52],
                        0.76,
                    );
                }
                self.push_rounded_rect(
                    track_x,
                    track_y,
                    track_w,
                    track_h,
                    track_h / 2.0,
                    [1.0, 1.0, 1.0, 0.10],
                );
                if let Some(value) = progress_value {
                    self.push_rounded_rect(
                        track_x,
                        track_y,
                        (track_w * value.clamp(0.0, 1.0)).max(track_h),
                        track_h,
                        track_h / 2.0,
                        [0.60, 0.35, 0.85, 0.88],
                    );
                } else {
                    let thumb_w = (28.0 * s).min(track_w);
                    let phase = progress_elapsed_secs.map(git_progress_thumb_phase).unwrap_or(1.0);
                    self.push_rounded_rect(
                        track_x + (track_w - thumb_w) * phase,
                        track_y,
                        thumb_w,
                        track_h,
                        track_h / 2.0,
                        [0.60, 0.35, 0.85, 0.88],
                    );
                }
            }
        }
        if line_x > left_status_limit + 8.0 * s {
            self.draw_string_scaled("Стр", line_x, text_y, pos_color, text_scale);
            self.draw_string_mono_scaled(
                &line_digits,
                line_x + line_label_w + digit_gap,
                text_y,
                pos_color,
                text_scale,
            );
            let char_x = line_x + line_block_w + item_gap;
            self.draw_string_scaled("Сим", char_x, text_y, pos_color, text_scale);
            self.draw_string_mono_scaled(
                &char_digits,
                char_x + char_label_w + digit_gap,
                text_y,
                pos_color,
                text_scale,
            );
            if let Some(digits) = selected_count_digits.as_deref() {
                let selected_x = char_x + char_block_w + item_gap;
                self.draw_string_scaled("(", selected_x, text_y, pos_color, text_scale);
                let digit_x = selected_x + self.measure_ui_width("(", text_scale).round();
                self.draw_string_mono_scaled(digits, digit_x, text_y, pos_color, text_scale);
                let suffix_x = digit_x
                    + self
                        .measure_mono_width(
                            &ZERO_SAMPLE[..digits.len().max(2).min(ZERO_SAMPLE.len())],
                            text_scale,
                        )
                        .round();
                self.draw_string_scaled(" выделено)", suffix_x, text_y, pos_color, text_scale);
            }
        }

        if diagnostics_hovered {
            scratch.clear();
            let _ = std::fmt::Write::write_fmt(
                &mut scratch,
                format_args!(
                    "Ляпы: {} ошибок, {} предупреждений",
                    error_count, warning_count
                ),
            );
            let tip_w = self.measure_ui_width(&scratch, text_scale).round() + 16.0 * s;
            let tip_h = 24.0 * s;
            let tip_x = (diag_x - 4.0 * s)
                .min(self.width - tip_w - 6.0 * s)
                .max(6.0 * s);
            let tip_y = (bar_y - tip_h - 6.0 * s).max(6.0 * s);
            self.push_rounded_rect_border(
                tip_x,
                tip_y,
                tip_w,
                tip_h,
                5.0 * s,
                1.0,
                [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.18],
                [0.08, 0.085, 0.115, 0.96],
            );
            self.draw_string_scaled(
                &scratch,
                tip_x + 8.0 * s,
                tip_y + 18.0 * s,
                self.theme.fg,
                text_scale,
            );
        }

        self.scratch_buffer = scratch;
    }

    fn draw_file_tree_dialog_shell(&mut self, x: f32, y: f32, w: f32, h: f32, s: f32) {
        let border = 2.0 * s;
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            10.0 * s,
            border,
            self.theme.sel,
            [0.15, 0.16, 0.20, 1.0],
        );
    }

    fn draw_file_tree_dialog_input(
        &mut self,
        editor: &crate::editor::Editor,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        scroll_x: f32,
        blink_alpha: f32,
    ) {
        let text = editor.get_full_text();
        self.draw_one_line_dialog_input(
            &text,
            editor.cursor,
            editor.selection_anchor,
            false,
            true,
            input_x,
            input_y,
            input_w,
            input_h,
            scroll_x,
            blink_alpha,
            crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE,
            0.0,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_one_line_dialog_input(
        &mut self,
        text: &str,
        cursor: usize,
        selection_anchor: Option<usize>,
        masked: bool,
        focused: bool,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        scroll_x: f32,
        blink_alpha: f32,
        text_scale: f32,
        right_inset: f32,
    ) {
        let s = self.scale_factor;
        self.draw_one_line_input_with_chrome(
            text,
            cursor,
            selection_anchor,
            masked,
            focused,
            input_x,
            input_y,
            input_w,
            input_h,
            scroll_x,
            blink_alpha,
            text_scale,
            right_inset,
            (8.0 * s).round(),
            (5.0 * s).round(),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_one_line_input_with_chrome(
        &mut self,
        text: &str,
        cursor: usize,
        selection_anchor: Option<usize>,
        masked: bool,
        focused: bool,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        scroll_x: f32,
        blink_alpha: f32,
        text_scale: f32,
        right_inset: f32,
        horizontal_padding: f32,
        corner_radius: f32,
    ) {
        let s = self.scale_factor;
        let x = input_x.round();
        let y = input_y.round();
        let w = input_w.round().max(1.0);
        let h = input_h.round().max(1.0);
        let pad_x = horizontal_padding.round().clamp(0.0, w * 0.5);
        let right_inset = right_inset.round().clamp(0.0, w - 1.0);
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            corner_radius.round().max(0.0),
            (1.0 * s).round().max(1.0),
            if focused {
                [0.60, 0.35, 0.85, 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.14]
            },
            [0.08, 0.09, 0.12, 1.0],
        );

        self.draw_one_line_selectable_text(
            text,
            cursor,
            selection_anchor,
            masked,
            focused,
            x,
            y,
            w,
            h,
            scroll_x,
            blink_alpha,
            text_scale,
            self.theme.fg,
            right_inset,
            pad_x,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_one_line_selectable_text(
        &mut self,
        text: &str,
        cursor: usize,
        selection_anchor: Option<usize>,
        masked: bool,
        focused: bool,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        scroll_x: f32,
        blink_alpha: f32,
        text_scale: f32,
        color: [f32; 4],
        right_inset: f32,
        horizontal_padding: f32,
    ) {
        let s = self.scale_factor;
        let x = input_x.round();
        let y = input_y.round();
        let w = input_w.round().max(1.0);
        let h = input_h.round().max(1.0);
        let pad_x = horizontal_padding.round().clamp(0.0, w * 0.5);
        let right_inset = right_inset.round().clamp(0.0, w - 1.0);
        let content_w = (w - pad_x * 2.0 - right_inset).max(1.0);
        let text_start_x = x + pad_x;
        let text_y = Self::tree_row_text_y(y, h, s);

        self.flush();
        unsafe {
            let restore_scissor = self.gl.is_enabled(glow::SCISSOR_TEST);
            let mut previous_scissor = [0i32; 4];
            if restore_scissor {
                self.gl
                    .get_parameter_i32_slice(glow::SCISSOR_BOX, &mut previous_scissor);
            }
            self.gl.enable(glow::SCISSOR_TEST);
            let requested_scissor = [
                text_start_x as i32,
                (self.height - (y + h)).round().max(0.0) as i32,
                content_w.round().max(1.0) as i32,
                h.round().max(1.0) as i32,
            ];
            let active_scissor = if restore_scissor {
                intersect_scissor_boxes(previous_scissor, requested_scissor)
            } else {
                requested_scissor
            };
            self.gl.scissor(
                active_scissor[0],
                active_scissor[1],
                active_scissor[2],
                active_scissor[3],
            );

            let scroll_x = scroll_x.round();
            let sel_start = selection_anchor.unwrap_or(cursor).min(cursor);
            let sel_end = selection_anchor.unwrap_or(cursor).max(cursor);
            let mut current_x = (text_start_x - scroll_x).round();
            let mut byte_idx = 0usize;
            let mut cursor_draw_x = current_x;
            let selection_y = (y + 5.0 * s).round();
            let selection_h = (h - 10.0 * s).round().max(1.0);

            for c in text.chars() {
                if byte_idx == cursor {
                    cursor_draw_x = current_x;
                }
                let char_to_render = if masked { '•' } else if c == '\n' { '↵' } else { c };
                let adv = self
                    .get_ui_glyph(char_to_render)
                    .map(|glyph| Self::snapped_text_advance(glyph.advance, text_scale))
                    .unwrap_or_else(|| (10.0 * text_scale).round().max(1.0));

                if byte_idx >= sel_start && byte_idx < sel_end {
                    self.push_rect(current_x, selection_y, adv, selection_h, self.theme.sel);
                }

                if current_x + adv >= text_start_x
                    && current_x <= text_start_x + content_w
                {
                    let mut buf = [0u8; 4];
                    self.draw_string_scaled_stable(
                        char_to_render.encode_utf8(&mut buf),
                        current_x,
                        text_y,
                        color,
                        text_scale,
                    );
                }

                current_x = (current_x + adv).round();
                byte_idx += c.len_utf8();
            }
            if byte_idx == cursor {
                cursor_draw_x = current_x;
            }

            if focused && sel_start == sel_end && blink_alpha > 0.5 {
                self.push_rect(
                    cursor_draw_x.round(),
                    selection_y,
                    (1.5 * s).round().max(1.0),
                    selection_h,
                    color,
                );
            }

            self.flush();
            if restore_scissor {
                self.gl.scissor(
                    previous_scissor[0],
                    previous_scissor[1],
                    previous_scissor[2],
                    previous_scissor[3],
                );
            } else {
                self.gl.disable(glow::SCISSOR_TEST);
            }
        }
    }

    fn draw_file_tree_dialog_buttons<const N: usize>(
        &mut self,
        ui_registry: &mut crate::ui_system::UiRegistry,
        buttons: [(crate::ui_system::UiId, &str, f32); N],
        btn_y: f32,
        btn_w: f32,
        btn_h: f32,
        s: f32,
        mx: f32,
        my: f32,
    ) -> bool {
        let mut wants_pointer = false;
        for (id, label, bx) in buttons {
            let hovered = ui_registry.register_rect(id, bx, btn_y, btn_w, btn_h, mx, my);
            if hovered {
                wants_pointer = true;
            }
            let bg = if hovered {
                [0.30, 0.32, 0.38, 1.0]
            } else {
                [0.22, 0.23, 0.28, 1.0]
            };
            self.push_rounded_rect(bx, btn_y, btn_w, btn_h, 5.0 * s, bg);
            let tw = self.measure_ui_width(label, 0.86);
            self.draw_string_scaled_stable(
                label,
                (bx + (btn_w - tw) / 2.0).round(),
                dialog_button_text_baseline(btn_y, btn_h, s),
                self.theme.fg,
                0.86,
            );
        }
        wants_pointer
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_animated_context_menu<'a, Label, Id, Separator>(
        &mut self,
        menu_x: f32,
        menu_y: f32,
        opened_at: std::time::Instant,
        item_count: usize,
        mut label_at: Label,
        mut id_at: Id,
        mut separator_before: Separator,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> bool
    where
        Label: FnMut(usize) -> &'a str,
        Id: FnMut(usize) -> crate::ui_system::UiId,
        Separator: FnMut(usize) -> bool,
    {
        let s = self.scale_factor;
        let row_h = 28.0 * s;
        let pad_x = 12.0 * s;
        let border = 2.0 * s;
        let separator_h = 8.0 * s;
        let mut menu_w = 190.0 * s;
        let mut separator_count = 0usize;
        for idx in 0..item_count {
            menu_w = menu_w.max(self.measure_ui_width(label_at(idx), 0.88) + pad_x * 2.0);
            separator_count += usize::from(separator_before(idx));
        }
        let menu_h = item_count as f32 * row_h
            + separator_count as f32 * separator_h
            + border * 2.0;
        let x = menu_x.min((self.width - menu_w - 6.0 * s).max(6.0 * s));
        let y = menu_y.min((self.height - menu_h - 6.0 * s).max(6.0 * s));
        let anim_progress = crate::app::context_menu::context_menu_anim_progress(
            opened_at,
            std::time::Instant::now(),
        );
        let visible_h = crate::app::context_menu::context_menu_visible_height(
            menu_h,
            border * 2.0,
            anim_progress,
        );
        self.push_rounded_rect_border(
            x,
            y,
            menu_w,
            visible_h,
            6.0 * s,
            border,
            self.theme.sel,
            [0.09, 0.10, 0.14, 1.0],
        );

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (y + visible_h)).round() as i32;
            self.gl.scissor(
                x.round() as i32,
                sy,
                menu_w.round() as i32,
                visible_h.round() as i32,
            );
        }

        let mut wants_pointer = false;
        let mut row_y = y + border;
        let visible_bottom = y + visible_h;
        for idx in 0..item_count {
            if separator_before(idx) {
                let line_y = row_y + separator_h / 2.0;
                self.push_rect(
                    x + border + pad_x,
                    line_y.round(),
                    menu_w - border * 2.0 - pad_x * 2.0,
                    1.0,
                    [1.0, 1.0, 1.0, 0.16],
                );
                row_y += separator_h;
            }
            if row_y >= visible_bottom {
                break;
            }
            let visible_row_h = (visible_bottom - row_y).min(row_h).max(0.0);
            let hovered = ui_registry.register_rect(
                id_at(idx),
                x,
                row_y,
                menu_w,
                visible_row_h,
                mx,
                my,
            );
            if hovered {
                wants_pointer = true;
                self.push_rect(
                    x + border,
                    row_y,
                    menu_w - border * 2.0,
                    visible_row_h,
                    [1.0, 1.0, 1.0, 0.10],
                );
            }
            self.draw_string_scaled_stable(
                label_at(idx),
                x + pad_x,
                row_y + row_h / 2.0 + 5.0 * s,
                self.theme.fg,
                0.88,
            );
            row_y += row_h;
        }
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        wants_pointer
    }

    pub(crate) fn draw_file_tree_overlays(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) -> bool {
        let s = self.scale_factor;
        let mut wants_pointer = false;
        let mut label_scratch = String::new();
        if crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel) {
            ui_registry.mark_overlay_start();
            if crate::app::file_tree::file_tree_modal_overlay_active_for_panel(ide_panel) {
                ui_registry.reset_cursor_state();
            }
        }

        if let Some(menu) = &ide_panel.file_tree_context_menu {
            wants_pointer |= self.draw_animated_context_menu(
                menu.x,
                menu.y,
                menu.opened_at,
                menu.entries.len(),
                |idx| menu.entries[idx].label(),
                crate::ui_system::UiId::FileTreeMenuItem,
                |idx| file_tree_menu_separator_before(&menu.entries, idx),
                ui_registry,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_create_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s).min(self.width - 32.0 * s);
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                dialog.kind.title(),
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let (path_prefix, input_x, input_w) =
                crate::app::file_tree::file_tree_path_input_layout(
                    x,
                    w,
                    s,
                    &dialog.parent_dir,
                    |text| self.measure_ui_width(text, path_scale),
                );
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            self.draw_string_scaled(
                &path_prefix,
                x + side_pad,
                input_y + 23.0 * s,
                [0.55, 0.57, 0.64, 1.0],
                path_scale,
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeCreateInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            let create_text = dialog.editor.get_full_text();
            let create_scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                &create_text,
                dialog.editor.cursor,
                (input_w - 16.0 * s).max(0.0),
                |ch| {
                    let char_to_render = if ch == '\n' { '↵' } else { ch };
                    self.get_ui_glyph(char_to_render)
                        .map(|g| g.advance * path_scale)
                        .unwrap_or(10.0 * path_scale)
                },
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                create_scroll_x,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 112.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeCreateConfirm,
                    "Создать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeCreateCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_rename_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let base_w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s)
                .min(self.width - 32.0 * s);
            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let base_x = ((self.width - base_w) / 2.0).round();
            let base_input_w = if let Some(parent_dir) = dialog.path.parent() {
                let (_, _, input_w) =
                    crate::app::file_tree::file_tree_path_input_layout(
                        base_x,
                        base_w,
                        s,
                        parent_dir,
                        |text| self.measure_ui_width(text, path_scale),
                    );
                input_w
            } else {
                base_w - crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * 2.0 * s
            };
            let rename_text = dialog.editor.get_full_text();
            let rename_text_w = self.measure_ui_width(&rename_text, path_scale);
            let w = crate::app::file_tree::file_tree_rename_dialog_width(
                base_w,
                self.width - 32.0 * s,
                base_input_w,
                rename_text_w,
                s,
            );
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Переименовать",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let (path_prefix, input_x, input_w) = if let Some(parent_dir) = dialog.path.parent() {
                crate::app::file_tree::file_tree_rename_path_input_layout(
                    x,
                    w,
                    base_w,
                    s,
                    parent_dir,
                    |text| self.measure_ui_width(text, path_scale),
                )
            } else {
                (String::new(), x + side_pad, w - side_pad * 2.0)
            };
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            if !path_prefix.is_empty() {
                self.draw_string_scaled(
                    &path_prefix,
                    x + side_pad,
                    input_y + 23.0 * s,
                    [0.55, 0.57, 0.64, 1.0],
                    path_scale,
                );
            }
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeRenameInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                dialog.input_scroll_x.current,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 130.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeRenameConfirm,
                    "Переименовать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeRenameCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_move_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Подтвердить перемещение",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_move_dialog_message(
                &dialog.sources,
                &dialog.target_dir,
            );
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeMoveConfirm,
                    "Переместить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeMoveCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_delete_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить в корзину",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_delete_dialog_message(&dialog.paths);
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeDeleteConfirm,
                    "В корзину",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeDeleteCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.api.spec_remove_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 204.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить OpenAPI",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            self.draw_string_scaled(
                "Удалить импортированную спецификацию?",
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );
            let label = if dialog.title.is_empty() {
                dialog.source.as_str()
            } else {
                dialog.title.as_str()
            };
            self.draw_tree_label_clipped(
                label,
                x + side_pad,
                y + 94.0 * s,
                w - side_pad * 2.0,
                [0.72, 0.76, 0.88, 1.0],
                0.82,
                &mut label_scratch,
            );
            if !dialog.source.is_empty() && dialog.source != label {
                self.draw_tree_label_clipped(
                    dialog.source.as_str(),
                    x + side_pad,
                    y + 120.0 * s,
                    w - side_pad * 2.0,
                    [0.58, 0.61, 0.70, 1.0],
                    0.74,
                    &mut label_scratch,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 58.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::ApiSpecRemoveConfirm,
                    "Удалить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::ApiSpecRemoveCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.api.mock_contract_field_delete_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 158.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить переменную",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            self.draw_string_scaled(
                "Удалить переменную из контракта мока?",
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );
            self.draw_tree_label_clipped(
                dialog.field_label.as_str(),
                x + side_pad,
                y + 94.0 * s,
                w - side_pad * 2.0,
                [0.72, 0.76, 0.88, 1.0],
                0.82,
                &mut label_scratch,
            );

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::ApiMockContractFieldRemoveConfirm,
                    "Удалить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::ApiMockContractFieldRemoveCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.api.mock_route_reset_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 166.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled("Сбросить мок", x + side_pad, y + 38.0 * s, self.theme.fg, 1.0);
            self.draw_string_scaled(
                "Удалить все настройки мока для route?",
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );
            self.draw_tree_label_clipped(
                dialog.route_label.as_str(),
                x + side_pad,
                y + 94.0 * s,
                w - side_pad * 2.0,
                [0.72, 0.76, 0.88, 1.0],
                0.82,
                &mut label_scratch,
            );

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::ApiMockRouteResetConfirm,
                    "Сбросить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::ApiMockRouteResetCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.git.confirm_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 40.0) * s).min(self.width - 32.0 * s);
            let visible_files = dialog.files.len().min(7);
            let h = (172.0 * s + visible_files as f32 * 20.0 * s).min(self.height - 32.0 * s);
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);

            let (title, message, confirm_label) = match dialog.action {
                crate::app::git_panel::GitConfirmAction::RollbackStaged => (
                    "Откатить staged файлы",
                    "Отменить staged изменения в выбранных файлах?",
                    "Откатить",
                ),
            };
            self.draw_string_scaled(title, x + side_pad, y + 38.0 * s, self.theme.fg, 1.0);
            self.draw_string_scaled(
                message,
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );

            let list_x = x + side_pad;
            let list_y = y + 92.0 * s;
            let list_w = w - side_pad * 2.0;
            for (idx, file) in dialog.files.iter().take(visible_files).enumerate() {
                self.draw_tree_label_clipped(
                    file.display_path.as_str(),
                    list_x,
                    list_y + idx as f32 * 20.0 * s,
                    list_w,
                    [0.72, 0.76, 0.88, 1.0],
                    0.82,
                    &mut label_scratch,
                );
            }
            if dialog.files.len() > visible_files {
                let more = format!("+{} more", dialog.files.len() - visible_files);
                self.draw_string_scaled(
                    &more,
                    list_x,
                    list_y + visible_files as f32 * 20.0 * s,
                    [0.55, 0.57, 0.64, 1.0],
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::GitConfirmAction,
                    confirm_label,
                    ok_x,
                ),
                (crate::ui_system::UiId::GitConfirmCancel, "Отмена", cancel_x),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        wants_pointer
    }
}
