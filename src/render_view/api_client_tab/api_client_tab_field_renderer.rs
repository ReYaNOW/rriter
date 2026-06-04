impl Renderer {
    fn draw_api_table_row_separator(&mut self, x: f32, y: f32, w: f32, row_h: f32, _s: f32) {
        let line_h = 1.0;
        let x = x.round();
        let y = (y + row_h - line_h).round();
        let w = w.round().max(line_h * 2.0);
        self.push_rect(
            x + line_h,
            y,
            (w - line_h * 2.0).max(0.0),
            line_h,
            [1.0, 1.0, 1.0, 0.13],
        );
    }

    fn draw_api_meta_inline(&mut self, label: &str, value: &str, x: f32, y: f32, s: f32) {
        self.draw_string_scaled_stable(label, x, y, [0.68, 0.70, 0.78, 1.0], API_FIELD_META_SCALE);
        let label_w = self.measure_ui_width(label, API_FIELD_META_SCALE);
        self.draw_string_scaled_stable(
            value,
            x + label_w + 4.0 * s,
            y,
            [0.82, 0.83, 0.88, 1.0],
            API_FIELD_META_SCALE,
        );
    }

    fn api_body_prop_row_layout(
        &mut self,
        w: f32,
        s: f32,
        schema: &ApiSchema,
        model: &crate::app::api_client::ApiSpecModel,
        value: &str,
    ) -> ApiFieldRowLayout {
        let allowed = api_schema_allowed_values(schema, model);
        let (choice_label, choices) = if !allowed.is_empty() {
            ("Допустимо:", allowed)
        } else if !schema.examples.is_empty() {
            ("Примеры:", schema.examples.as_slice())
        } else {
            ("", &[][..])
        };
        self.api_field_row_layout(
            w,
            s,
            value,
            api_schema_is_array_input(schema),
            if api_schema_is_file_input(schema, model) {
                64.0 * s
            } else {
                0.0
            },
            choice_label,
            choices,
            usize::from(schema.max_chars.is_some()) + usize::from(schema.default_value.is_some()),
            if allowed.is_empty() {
                0
            } else {
                schema.examples.len().min(3)
            },
        )
    }

    fn api_param_row_layout(
        &mut self,
        w: f32,
        s: f32,
        param: &ApiParam,
        value: &str,
    ) -> ApiFieldRowLayout {
        let (choice_label, choices) = if !param.enum_values.is_empty() {
            ("Допустимо:", param.enum_values.as_slice())
        } else if !param.examples.is_empty() {
            ("Примеры:", param.examples.as_slice())
        } else {
            ("", &[][..])
        };
        self.api_field_row_layout(
            w,
            s,
            value,
            matches!(
                param.primitive_type,
                crate::app::api_client::ApiPrimitiveType::Array
            ),
            0.0,
            choice_label,
            choices,
            usize::from(param.default_value.is_some()),
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn api_field_row_layout(
        &mut self,
        w: f32,
        s: f32,
        value: &str,
        is_array: bool,
        pick_w: f32,
        choice_label: &str,
        choices: &[String],
        pre_choice_lines: usize,
        bottom_lines: usize,
    ) -> ApiFieldRowLayout {
        let left_w = (w * 0.30).clamp(126.0 * s, 230.0 * s);
        let gap = 20.0 * s;
        let min_input_w = 118.0 * s;
        let base_input_w = (w * 0.60).max(120.0 * s);
        let array_content_input_w = self.api_array_content_width(value, s) + 24.0 * s + pick_w;
        let desired_input_w = if is_array {
            array_content_input_w.max(base_input_w)
        } else {
            base_input_w
        };
        let has_choices = !choices.is_empty();
        let choice_full_w = if has_choices {
            self.api_choice_one_line_width(choice_label, choices, s)
        } else {
            0.0
        };
        let max_right_w = (w - left_w - min_input_w - gap).max(0.0);
        let compact_right_w = if has_choices || pre_choice_lines > 0 || bottom_lines > 0 {
            (w * 0.28).clamp(120.0 * s, 260.0 * s).min(max_right_w)
        } else {
            0.0
        };
        let one_line_right_w = if has_choices {
            choice_full_w.max(compact_right_w).min(max_right_w)
        } else {
            compact_right_w
        };
        let mut right_w = one_line_right_w;
        let mut choice_rows = if has_choices {
            self.api_choice_rows_for_width(choice_label, choices, right_w, s)
        } else {
            0
        };
        let max_input_one_line = (w - left_w - right_w - gap).max(min_input_w);
        if has_choices && is_array && array_content_input_w > max_input_one_line + 1.0 {
            right_w =
                (w - left_w - gap - array_content_input_w).clamp(compact_right_w, one_line_right_w);
            choice_rows = self.api_choice_rows_for_width(choice_label, choices, right_w, s);
        }
        let max_input_w = (w - left_w - right_w - gap).max(min_input_w);
        let input_w = desired_input_w.min(max_input_w).max(min_input_w);
        let field_w = (input_w - 16.0 * s - pick_w).max(24.0 * s);
        let array_rows = if is_array {
            self.api_array_rows_for_width(value, field_w, s)
        } else {
            1
        };
        let input_h = array_rows as f32 * 32.0 * s;
        let meta_lines = pre_choice_lines
            + choice_rows
            + bottom_lines
            + usize::from(
                !has_choices && pre_choice_lines == 0 && bottom_lines == 0 && right_w > 0.0,
            );
        let meta_h = if meta_lines == 0 {
            0.0
        } else {
            (32.0 + meta_lines.saturating_sub(1) as f32 * 20.0) * s
        };
        let row_h = (input_h + 14.0 * s).max(meta_h + 14.0 * s);
        let input_x = left_w;
        let right_x = input_x + input_w + 12.0 * s;
        ApiFieldRowLayout {
            row_h,
            input_x,
            input_w,
            input_h,
            right_x,
            right_w: (w - right_x - 8.0 * s).max(24.0 * s),
        }
    }

    fn api_choice_one_line_width(&mut self, label: &str, values: &[String], s: f32) -> f32 {
        if values.is_empty() {
            return 0.0;
        }
        let sep_w = self.measure_ui_width("┃", API_FIELD_META_SCALE) + 6.0 * s;
        let values_w = values
            .iter()
            .map(|value| self.measure_ui_width(value, API_FIELD_META_SCALE))
            .sum::<f32>();
        self.measure_ui_width(label, API_FIELD_META_SCALE)
            + 5.0 * s
            + values_w
            + values.len().saturating_sub(1) as f32 * sep_w
            + values.len() as f32 * 6.0 * s
    }

    fn api_choice_rows_for_width(
        &mut self,
        label: &str,
        values: &[String],
        w: f32,
        s: f32,
    ) -> usize {
        if values.is_empty() {
            return 0;
        }
        let label_w = self.measure_ui_width(label, API_FIELD_META_SCALE) + 5.0 * s;
        let sep_w = self.measure_ui_width("┃", API_FIELD_META_SCALE) + 6.0 * s;
        let max_x = w.max(24.0 * s);
        let mut rows = 1usize;
        let mut cx = label_w;
        for (idx, value) in values.iter().enumerate() {
            let value_w = self.measure_ui_width(value, API_FIELD_META_SCALE);
            let needs_sep = idx > 0 && cx > 1.0;
            let full_w = value_w + if needs_sep { sep_w } else { 0.0 };
            if cx + full_w > max_x {
                rows += 1;
                cx = 0.0;
                cx += value_w + 6.0 * s;
            } else {
                cx += full_w + 6.0 * s;
            }
        }
        rows
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_allowed_values<F>(
        &mut self,
        label: &str,
        values: &[String],
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        id_for: F,
    ) where
        F: Fn(usize) -> crate::ui_system::UiId,
    {
        self.draw_string_scaled_stable(label, x, y, [0.68, 0.70, 0.78, 1.0], API_FIELD_META_SCALE);
        let label_w = self.measure_ui_width(label, API_FIELD_META_SCALE) + 5.0 * s;
        let sep = "┃";
        let sep_w = self.measure_ui_width(sep, API_FIELD_META_SCALE) + 6.0 * s;
        let max_x = x + w;
        let line_h = 20.0 * s;
        let mut row = 0usize;
        let mut cx = x + label_w;
        for (idx, value) in values.iter().enumerate() {
            let value_w = self.measure_ui_width(value, API_FIELD_META_SCALE);
            let mut needs_sep = idx > 0 && cx > x + 1.0;
            let full_w = value_w + if needs_sep { sep_w } else { 0.0 };
            if cx + full_w > max_x {
                row += 1;
                cx = x;
                needs_sep = false;
            }
            let draw_y = y + row as f32 * line_h;
            if needs_sep {
                self.draw_string_scaled_stable(
                    sep,
                    cx,
                    draw_y,
                    [0.50, 0.54, 0.62, 1.0],
                    API_FIELD_META_SCALE,
                );
                cx += sep_w;
            }
            let hit_w = value_w.max(16.0 * s);
            if ui_registry.register_rect(
                id_for(idx),
                cx - 2.0 * s,
                draw_y - 12.0 * s,
                hit_w + 4.0 * s,
                18.0 * s,
                mx,
                my,
            ) {
                self.push_rect(
                    cx - 2.0 * s,
                    draw_y - 12.0 * s,
                    hit_w + 4.0 * s,
                    18.0 * s,
                    [1.0, 1.0, 1.0, 0.08],
                );
            }
            self.draw_string_scaled_stable(
                value,
                cx,
                draw_y,
                [0.35, 0.75, 1.0, 1.0],
                API_FIELD_META_SCALE,
            );
            cx += value_w + 6.0 * s;
        }
    }

    fn draw_api_array_value_chips(
        &mut self,
        value: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        focused: bool,
    ) {
        let mut cx = x;
        let max_x = x + w;
        let line_h = 32.0 * s;
        let focused_parts;
        let (items, draft) = if focused {
            focused_parts = api_array_edit_parts(value);
            (focused_parts.0.as_slice(), focused_parts.1)
        } else {
            focused_parts = (api_array_value_parts(value).collect::<Vec<_>>(), "");
            (focused_parts.0.as_slice(), "")
        };
        let mut row = 0usize;
        for item in items {
            let text_w = self.measure_ui_width(item, API_FIELD_META_SCALE);
            let chip_w = (text_w + 16.0 * s).max(24.0 * s);
            if cx > x && cx + chip_w > max_x {
                row += 1;
                cx = x;
            }
            let chip_y = y + row as f32 * line_h;
            if chip_y >= y + h {
                break;
            }
            let chip_h = (y + h - chip_y).min(line_h);
            self.push_rounded_rect_border(
                cx,
                chip_y,
                chip_w.min(w),
                chip_h,
                4.0 * s,
                1.0,
                [0.35, 0.75, 1.0, 0.42],
                [0.16, 0.22, 0.28, 1.0],
            );
            self.draw_string_scaled_stable(
                item,
                cx + 8.0 * s,
                api_centered_text_y(chip_y, chip_h, s),
                [0.70, 0.88, 1.0, 1.0],
                API_FIELD_META_SCALE,
            );
            cx += chip_w + 5.0 * s;
        }
        if focused && !draft.is_empty() {
            let draft_w = self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
            if cx > x && cx + draft_w > max_x {
                row += 1;
                cx = x;
            }
            let draft_y = y + row as f32 * line_h;
            if draft_y >= y + h {
                return;
            }
            self.draw_string_scaled_stable(
                draft,
                cx,
                api_centered_text_y(draft_y, line_h.min(y + h - draft_y), s),
                self.theme.fg,
                API_FIELD_VALUE_SCALE,
            );
        }
    }

    fn api_array_visual_cursor(&mut self, value: &str, max_w: f32, s: f32) -> (f32, usize) {
        let (items, draft) = api_array_edit_parts(value);
        let mut width = 0.0;
        let mut row = 0usize;
        for item in items {
            let text_w = self.measure_ui_width(item, API_FIELD_META_SCALE);
            let chip_w = (text_w + 16.0 * s).max(24.0 * s);
            if width > 0.0 && width + chip_w > max_w {
                row += 1;
                width = 0.0;
            }
            width += chip_w + 5.0 * s;
        }
        if !draft.is_empty() {
            let draft_w = self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
            if width > 0.0 && width + draft_w > max_w {
                row += 1;
                width = 0.0;
            }
            width += draft_w;
        }
        (width.min(max_w), row)
    }

    fn api_array_rows_for_width(&mut self, value: &str, max_w: f32, s: f32) -> usize {
        let (items, draft) = api_array_edit_parts(value);
        let mut rows = 1usize;
        let mut width = 0.0;
        for item in items {
            let text_w = self.measure_ui_width(item, API_FIELD_META_SCALE);
            let chip_w = (text_w + 16.0 * s).max(24.0 * s);
            if width > 0.0 && width + chip_w > max_w {
                rows += 1;
                width = 0.0;
            }
            width += chip_w + 5.0 * s;
        }
        if !draft.is_empty() {
            let draft_w = self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
            if width > 0.0 && width + draft_w > max_w {
                rows += 1;
            }
        }
        rows
    }

    fn api_array_content_width(&mut self, value: &str, s: f32) -> f32 {
        let (items, draft) = api_array_edit_parts(value);
        let mut width = 0.0;
        for item in items {
            width += (self.measure_ui_width(item, API_FIELD_META_SCALE) + 16.0 * s).max(24.0 * s)
                + 5.0 * s;
        }
        if !draft.is_empty() {
            width += self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
        }
        width
    }

    pub(crate) fn draw_api_editor_selection_one_line(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text_scale: f32,
        scroll_x: f32,
    ) {
        let Some(anchor) = editor.selection_anchor else {
            return;
        };
        if anchor == editor.cursor {
            return;
        }
        let text = editor.get_full_text();
        let start = anchor.min(editor.cursor).min(text.len());
        let end = anchor.max(editor.cursor).min(text.len());
        let sel_x = self.measure_ui_width(&text[..start], text_scale) - scroll_x;
        let sel_w = self.measure_ui_width(&text[start..end], text_scale);
        let x1 = (x + sel_x).max(x);
        let x2 = (x + sel_x + sel_w).min(x + w);
        if x2 > x1 {
            self.push_rect(x1, y, x2 - x1, h, [0.55, 0.36, 0.90, 0.36]);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_api_one_line_input(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        shown: &str,
        color: [f32; 4],
        focused: bool,
        input_scroll_x: f32,
        editor: &crate::editor::Editor,
        blink_alpha: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        text_scale: f32,
    ) {
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            5.0 * s,
            (1.0 * s).max(1.0),
            if focused {
                [0.60, 0.35, 0.85, 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.12]
            },
            [0.13, 0.14, 0.18, 1.0],
        );
        ui_registry.register_text_input(id, x, y, w, h, mx, my);
        let text_w = (w - 16.0 * s).max(1.0);
        let scroll_x = if focused { input_scroll_x.round() } else { 0.0 };
        if focused {
            let sel_h = (h - 10.0 * s).max(16.0 * s);
            self.draw_api_editor_selection_one_line(
                editor,
                x + 8.0 * s,
                y + (h - sel_h) * 0.5,
                text_w,
                sel_h,
                text_scale,
                scroll_x,
            );
        }
        self.draw_api_one_line_clipped(
            shown,
            x + 8.0 * s,
            api_centered_text_y(y, h, s),
            text_w,
            scroll_x,
            color,
            text_scale,
        );
        if focused && blink_alpha > 0.5 {
            let cursor_w = self.api_editor_cursor_x_one_line(editor, text_scale) - scroll_x;
            let cursor_h = (h - 12.0 * s).max(16.0 * s);
            self.push_rect(
                x + 8.0 * s + cursor_w.clamp(0.0, text_w),
                y + (h - cursor_h) * 0.5,
                1.5 * s,
                cursor_h,
                self.theme.fg,
            );
        }
    }

    fn draw_api_one_line_clipped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        scroll_x: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        let mut draw_x = x - scroll_x;
        let max_x = x + w;
        for ch in text.chars() {
            let adv = self
                .get_ui_glyph(ch)
                .map(|g| Self::snapped_text_advance(g.advance, scale))
                .unwrap_or(8.0);
            if draw_x >= x && draw_x + adv <= max_x {
                self.draw_string_scaled_stable(&ch.to_string(), draw_x, y, color, scale);
            }
            draw_x += adv;
            if draw_x > max_x {
                break;
            }
        }
    }

    fn api_body_snapped_width(&mut self, text: &str) -> f32 {
        text.chars()
            .filter(|&ch| ch != '\u{FE0F}' && ch != '\u{200D}' && ch != '\n' && ch != '\r')
            .map(|ch| {
                self.get_ui_glyph(ch)
                    .map(|g| Self::snapped_text_advance(g.advance, API_BODY_TEXT_SCALE))
                    .unwrap_or(8.0)
            })
            .sum()
    }

    pub(crate) fn api_editor_cursor_x_one_line(
        &mut self,
        editor: &crate::editor::Editor,
        text_scale: f32,
    ) -> f32 {
        let text = editor.get_full_text();
        let cursor = editor.cursor.min(text.len());
        self.measure_ui_width(&text[..cursor], text_scale)
    }

    fn begin_api_text_clip(
        &mut self,
        rect: (f32, f32, f32, f32),
        parent: (f32, f32, f32, f32),
    ) -> bool {
        let Some((x, y, w, h)) = api_rect_intersection(rect, parent) else {
            return false;
        };
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round() as i32,
                h.round() as i32,
            );
        }
        true
    }

    fn restore_api_tab_clip(&mut self, rect: (f32, f32, f32, f32)) {
        let (x, y, w, h) = rect;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round() as i32,
                h.round() as i32,
            );
        }
    }

    fn draw_api_editor_selection_multiline(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let Some(anchor) = editor.selection_anchor else {
            return;
        };
        if anchor == editor.cursor {
            return;
        }
        let text = editor.get_full_text();
        let start = anchor.min(editor.cursor).min(text.len());
        let end = anchor.max(editor.cursor).min(text.len());
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(&text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut line_start = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_end = line_start + line.len();
            if line_idx < first_line {
                line_start = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            let sel_start = start.max(line_start).min(line_end);
            let sel_end = end.max(line_start).min(line_end);
            if sel_start < sel_end {
                let prefix = self.api_mono_width(&text[line_start..sel_start]) - scroll_x;
                let raw_w = self.api_mono_width(&text[sel_start..sel_end]);
                let x1 = (x + prefix).max(x);
                let x2 = (x + prefix + raw_w).min(x + w);
                if x2 > x1 {
                    let sel_y = y - line_offset + visible_idx as f32 * line_h;
                    self.push_rect(x1, sel_y, x2 - x1, line_h, self.theme.sel);
                }
            }
            if end <= line_end {
                break;
            }
            line_start = line_end + 1;
        }
    }

    fn draw_api_editor_cursor_multiline(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let text = editor.get_full_text();
        let cursor = editor.cursor.min(text.len());
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(&text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut line_start = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_end = line_start + line.len();
            if cursor <= line_end {
                if line_idx < first_line {
                    return;
                }
                let visible_idx = line_idx - first_line;
                if visible_idx >= max_lines {
                    return;
                }
                let cursor_x = self.api_mono_width(&text[line_start..cursor]) - scroll_x;
                if cursor_x < -2.0 * s || cursor_x > w + 2.0 * s {
                    return;
                }
                self.push_rect(
                    x + cursor_x,
                    y - line_offset + visible_idx as f32 * line_h,
                    1.5 * s,
                    line_h,
                    self.theme.fg,
                );
                return;
            }
            line_start = line_end + 1;
        }
        if max_lines > 0 {
            self.push_rect(x, y, 1.5 * s, line_h, self.theme.fg);
        }
    }

    fn draw_api_editor_selection_multiline_ui(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let Some(anchor) = editor.selection_anchor else {
            return;
        };
        if anchor == editor.cursor {
            return;
        }
        let text = editor.get_full_text();
        let start = anchor.min(editor.cursor).min(text.len());
        let end = anchor.max(editor.cursor).min(text.len());
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(&text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut line_start = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_end = line_start + line.len();
            if line_idx < first_line {
                line_start = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            let sel_start = start.max(line_start).min(line_end);
            let sel_end = end.max(line_start).min(line_end);
            if sel_start < sel_end {
                let prefix = self.api_body_snapped_width(&text[line_start..sel_start]) - scroll_x;
                let raw_w = self.api_body_snapped_width(&text[sel_start..sel_end]);
                let x1 = (x + prefix).max(x);
                let x2 = (x + prefix + raw_w).min(x + w);
                if x2 > x1 {
                    let sel_y = y - line_offset + visible_idx as f32 * line_h;
                    self.push_rect(x1, sel_y, x2 - x1, line_h, self.theme.sel);
                }
            }
            if end <= line_end {
                break;
            }
            line_start = line_end + 1;
        }
    }

    fn draw_api_editor_cursor_multiline_ui(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let text = editor.get_full_text();
        let cursor = editor.cursor.min(text.len());
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(&text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut line_start = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_end = line_start + line.len();
            if cursor <= line_end {
                if line_idx < first_line {
                    return;
                }
                let visible_idx = line_idx - first_line;
                if visible_idx >= max_lines {
                    return;
                }
                let cursor_x = self.api_body_snapped_width(&text[line_start..cursor]) - scroll_x;
                if cursor_x < -2.0 * s || cursor_x > w + 2.0 * s {
                    return;
                }
                self.push_rect(
                    x + cursor_x,
                    y - line_offset + visible_idx as f32 * line_h,
                    1.5 * s,
                    line_h,
                    self.theme.fg,
                );
                return;
            }
            line_start = line_end + 1;
        }
        if max_lines > 0 {
            self.push_rect(x, y, 1.5 * s, line_h, self.theme.fg);
        }
    }

    fn draw_json_text_area(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
        headers: bool,
    ) {
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut byte_idx = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_start = byte_idx;
            let line_end = line_start + line.len();
            if line_idx < first_line {
                byte_idx = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            if headers {
                self.draw_header_lexed_line(
                    line,
                    x - scroll_x,
                    y - line_offset + visible_idx as f32 * line_h,
                    w + scroll_x,
                );
            } else {
                self.draw_json_lexed_line(
                    line,
                    x - scroll_x,
                    y - line_offset + visible_idx as f32 * line_h,
                    w + scroll_x,
                );
            }
            byte_idx = line_end.saturating_add(1);
        }
    }

    fn draw_curl_text_area(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut byte_idx = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_start = byte_idx;
            let line_end = line_start + line.len();
            if line_idx < first_line {
                byte_idx = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            self.draw_curl_lexed_line(
                line,
                x - scroll_x,
                y - line_offset + visible_idx as f32 * line_h,
                w + scroll_x,
            );
            byte_idx = line_end.saturating_add(1);
        }
    }

    fn draw_api_schema_text_area(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
        _headers: bool,
        fold_input: bool,
        route_idx: usize,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(0.0, self.api_schema_text_max_scroll(text, w, h, s));
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut visual_line_idx = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let wrap_count = self.api_schema_line_wrap_count(line, w);
            if visual_line_idx + wrap_count <= first_line {
                visual_line_idx += wrap_count;
                continue;
            }
            if visual_line_idx >= first_line + max_lines {
                break;
            }
            let visible_idx = visual_line_idx.saturating_sub(first_line);
            let baseline_y = y - line_offset + visible_idx as f32 * line_h;
            let trimmed = line.trim_start();
            if matches!(trimmed.as_bytes().first(), Some(b'+' | b'-')) {
                let id = if fold_input {
                    crate::ui_system::UiId::ApiInputSchemaFold(route_idx, line_idx)
                } else {
                    crate::ui_system::UiId::ApiOutputSchemaFold(route_idx, line_idx)
                };
                let indent_w = self.measure_ui_width(&line[..line.len() - trimmed.len()], API_BODY_TEXT_SCALE);
                ui_registry.register_rect(
                    id,
                    x - scroll_x + indent_w,
                    baseline_y - 17.0 * s,
                    16.0 * s,
                    line_h,
                    mx,
                    my,
                );
            }
            self.draw_api_schema_wrapped_line(
                line,
                x - scroll_x,
                y - line_offset,
                w + scroll_x,
                line_h,
                visual_line_idx,
                first_line,
                max_lines,
            );
            visual_line_idx += wrap_count;
        }
    }

    fn api_schema_text_max_scroll(&mut self, text: &str, w: f32, h: f32, s: f32) -> f32 {
        let line_h = api_text_area_line_height(s);
        let lines = text
            .split('\n')
            .map(|line| self.api_schema_line_wrap_count(line, w))
            .sum::<usize>()
            .max(1) as f32;
        (lines * line_h - h).max(0.0)
    }

    fn api_schema_line_wrap_count(&mut self, line: &str, w: f32) -> usize {
        let mut count = 1usize;
        let mut current = line.trim_end();
        let mut available_w = w;
        let continuation_w = self.api_schema_continuation_width(line, w);
        while self.measure_ui_width(current, API_BODY_TEXT_SCALE) > available_w && !current.is_empty() {
            let (_, next_start) = self.api_schema_wrap_split(current, available_w);
            let next = current[next_start..].trim_start();
            if next.len() == current.len() {
                break;
            }
            current = next;
            available_w = continuation_w;
            count = count.saturating_add(1);
        }
        count
    }

    fn api_schema_continuation_width(&mut self, line: &str, w: f32) -> f32 {
        let indent = line.len() - line.trim_start().len();
        let space_w = self.measure_ui_width(" ", API_BODY_TEXT_SCALE);
        (w - indent as f32 * space_w - 2.0 * space_w).max(w * 0.5)
    }

    fn api_schema_wrap_split(&mut self, line: &str, available_w: f32) -> (usize, usize) {
        let mut x = 0.0;
        let mut last_soft = None;
        for (byte_idx, ch) in line.char_indices() {
            let mut buf = [0u8; 4];
            let part = ch.encode_utf8(&mut buf);
            let adv = self.measure_ui_width(part, API_BODY_TEXT_SCALE);
            if x + adv > available_w && byte_idx > 0 {
                let end = last_soft.unwrap_or(byte_idx);
                let mut next = end;
                while next < line.len()
                    && line.as_bytes().get(next).is_some_and(|b| b.is_ascii_whitespace())
                {
                    next += 1;
                }
                return (end, next.max(end));
            }
            x += adv;
            if matches!(ch, ' ' | ',' | '·' | '|') {
                last_soft = Some(byte_idx + ch.len_utf8());
            }
        }
        (line.len(), line.len())
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_schema_wrapped_line(
        &mut self,
        line: &str,
        x: f32,
        y: f32,
        w: f32,
        line_h: f32,
        mut visual_line_idx: usize,
        first_line: usize,
        max_lines: usize,
    ) {
        let indent = line.len() - line.trim_start().len();
        let space_w = self.measure_ui_width(" ", API_BODY_TEXT_SCALE);
        let continuation_x = x + indent as f32 * space_w + 2.0 * space_w;
        let continuation_w = (w - (continuation_x - x)).max(w * 0.5);
        let mut current = line.trim_end();
        let mut draw_x = x;
        let mut available_w = w;
        loop {
            if visual_line_idx >= first_line && visual_line_idx < first_line + max_lines {
                let baseline_y = y + (visual_line_idx - first_line) as f32 * line_h;
                if self.measure_ui_width(current, API_BODY_TEXT_SCALE) <= available_w {
                    self.draw_api_schema_lexed_line(current, draw_x, baseline_y, available_w);
                    break;
                }
                let (end, next_start) = self.api_schema_wrap_split(current, available_w);
                self.draw_api_schema_lexed_line(current[..end].trim_end(), draw_x, baseline_y, available_w);
                current = current[next_start..].trim_start();
            } else if self.measure_ui_width(current, API_BODY_TEXT_SCALE) <= available_w {
                break;
            } else {
                let (_, next_start) = self.api_schema_wrap_split(current, available_w);
                current = current[next_start..].trim_start();
            }
            if current.is_empty() {
                break;
            }
            visual_line_idx = visual_line_idx.saturating_add(1);
            draw_x = continuation_x;
            available_w = continuation_w;
        }
    }

    fn draw_api_schema_lexed_line(&mut self, line: &str, x: f32, y: f32, w: f32) {
        let (main, meta) = line
            .split_once("  · ")
            .map(|(main, meta)| (main, Some(meta)))
            .unwrap_or((line, None));
        let mut draw_x = x;
        let bytes = main.as_bytes();
        let mut idx = 0usize;
        while idx < main.len() {
            if draw_x > x + w {
                break;
            }
            let b = bytes[idx];
            if matches!(b, b'+' | b'-') {
                self.draw_json_colored_segment(
                    &main[idx..idx + 1],
                    [1.0, 0.68, 0.26, 1.0],
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx += 1;
                continue;
            }
            if b == b'"' {
                let end = json_string_end(main, idx);
                let color = if schema_string_is_key(main, end) {
                    crate::highlighter::DRACULA_CYAN
                } else {
                    crate::highlighter::DRACULA_YELLOW
                };
                self.draw_json_colored_segment(&main[idx..end], color, x, y, w, &mut draw_x);
                idx = end;
                continue;
            }
            if b == b'*' {
                self.draw_json_colored_segment(
                    &main[idx..idx + 1],
                    crate::highlighter::DRACULA_PINK,
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx += 1;
                continue;
            }
            if b == b'-' || b.is_ascii_digit() {
                let end = json_number_end(main, idx);
                self.draw_json_colored_segment(
                    &main[idx..end],
                    crate::highlighter::DRACULA_PURPLE,
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx = end;
                continue;
            }
            if let Some(end) = json_keyword_end(main, idx) {
                self.draw_json_colored_segment(
                    &main[idx..end],
                    crate::highlighter::DRACULA_PINK,
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx = end;
                continue;
            }
            let ch = main[idx..].chars().next().unwrap_or(' ');
            let end = idx + ch.len_utf8();
            self.draw_json_colored_segment(&main[idx..end], self.theme.fg, x, y, w, &mut draw_x);
            idx = end;
        }
        if let Some(meta) = meta {
            self.draw_json_colored_segment("  ", [0.56, 0.58, 0.64, 1.0], x, y, w, &mut draw_x);
            self.draw_json_colored_segment(meta, [0.56, 0.58, 0.64, 1.0], x, y, w, &mut draw_x);
        }
    }

    fn draw_api_text_scrollbar(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
    ) {
        let max_scroll = crate::app::api_client::api_text_area_max_scroll(text, h, s);
        if max_scroll <= 0.5 {
            return;
        }
        let track_w = (4.0 * s).max(3.0);
        self.push_rect(x, y, track_w, h, [0.52, 0.54, 0.60, 0.36]);
        let content_h = h + max_scroll;
        let thumb_h = (h / content_h * h).max(22.0 * s).min(h);
        let thumb_y = y + (scroll_y.clamp(0.0, max_scroll) / max_scroll) * (h - thumb_h);
        self.push_rect(x, thumb_y, track_w, thumb_h, [0.70, 0.72, 0.80, 0.88]);
    }

    fn draw_api_schema_scrollbar(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
    ) {
        let max_scroll = self.api_schema_text_max_scroll(text, w, h, s);
        if max_scroll <= 0.5 {
            return;
        }
        let track_w = (4.0 * s).max(3.0);
        self.push_rect(x, y, track_w, h, [0.52, 0.54, 0.60, 0.36]);
        let content_h = h + max_scroll;
        let thumb_h = (h / content_h * h).max(22.0 * s).min(h);
        let thumb_y = y + (scroll_y.clamp(0.0, max_scroll) / max_scroll) * (h - thumb_h);
        self.push_rect(x, thumb_y, track_w, thumb_h, [0.70, 0.72, 0.80, 0.88]);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_text_scrollbar_x(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        track_w: f32,
        visible_w: f32,
        scroll_x: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let max_scroll = api_text_area_max_scroll_x(text, visible_w, |line| {
            self.measure_ui_width(line, API_BODY_TEXT_SCALE)
        });
        if max_scroll <= 0.5 {
            return;
        }
        let track_h = 3.0_f32.max(2.0);
        self.push_rect(x, y, track_w, track_h, [0.52, 0.54, 0.60, 0.22]);
        let content_w = track_w + max_scroll;
        let thumb_w = (track_w / content_w * track_w).max(28.0).min(track_w);
        let thumb_x = x + (scroll_x.clamp(0.0, max_scroll) / max_scroll) * (track_w - thumb_w);
        self.push_rect(thumb_x, y, thumb_w, track_h, [0.64, 0.66, 0.72, 0.70]);
        ui_registry.register_rect(id, x, y - 5.0, track_w, 13.0, mx, my);
    }
}
