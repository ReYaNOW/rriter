use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::Renderer;
use glow::HasContext;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_sticky_lines(
        &mut self,
        editor: &Editor,
        spans: &[ColorSpan],
        current_sticky_lines: &[(usize, usize)],
        render_scroll_y: f32,
        render_scroll_x: f32,
        anim_progress: f32,
        anim_is_adding: bool,
        gutter_x: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        tab_bar_h: f32,
    ) -> Vec<(usize, usize)> {
        self.sticky_scroll_rects.clear();
        let mut active_ranges = Vec::new();

        for &(start_b, end_b, is_sticky) in &editor.foldable_ranges_bytes {
            if !is_sticky {
                continue;
            }
            let sl = editor
                .line_offsets
                .partition_point(|&o| o <= start_b)
                .saturating_sub(1);
            let mut el = editor
                .line_offsets
                .partition_point(|&o| o <= end_b)
                .saturating_sub(1);

            for line in sl..=el {
                if let Some(&fold_end) = editor.foldable_lines.get(&line) {
                    el = el.max(fold_end);
                }
            }

            if el > sl {
                active_ranges.push((sl, el));
            }
        }
        active_ranges.sort_unstable_by_key(|&(sl, _)| sl);
        active_ranges.dedup_by_key(|&mut (sl, _)| sl);

        let mut depth_stack: Vec<usize> = Vec::new();
        let mut ranges_with_depth = Vec::new();

        for &(sl, el) in &active_ranges {
            while let Some(&last_el) = depth_stack.last() {
                if sl >= last_el {
                    depth_stack.pop();
                } else {
                    break;
                }
            }
            let depth = depth_stack.len();
            depth_stack.push(el);
            ranges_with_depth.push((sl, el, depth));
        }

        for i in 0..ranges_with_depth.len() {
            let (_, el1, d1) = ranges_with_depth[i];

            let mut next_sl = None;
            for j in (i + 1)..ranges_with_depth.len() {
                let (sl2, _, d2) = ranges_with_depth[j];
                if sl2.saturating_sub(el1) > 6 {
                    break;
                }
                if d2 == d1 {
                    next_sl = Some(sl2);
                    break;
                }
            }

            if let Some(n_sl) = next_sl {
                if n_sl > el1 {
                    ranges_with_depth[i].1 = n_sl - 1;
                }
            }
        }

        let mut target_sticky_lines = Vec::new();
        let mut current_depth = 0;

        for &(sl, el, depth) in &ranges_with_depth {
            if depth != current_depth {
                continue;
            }

            let v_sl = self.phys_to_visual.get(sl).copied().unwrap_or(0);
            let v_el = self.phys_to_visual.get(el).copied().unwrap_or(0);

            let slot_y = depth as f32 * self.line_height + tab_bar_h;
            let line_y = v_sl as f32 * self.line_height - render_scroll_y;
            let push_y = (v_el + 1) as f32 * self.line_height - render_scroll_y;

            if line_y <= slot_y + 0.1 && push_y > slot_y + 0.1 {
                if !target_sticky_lines.iter().any(|&(s, _)| s == sl) {
                    target_sticky_lines.push((sl, el));
                    current_depth += 1;
                }
            }
        }

        if target_sticky_lines.len() > 5 {
            let skip = target_sticky_lines.len() - 5;
            target_sticky_lines.drain(0..skip);
        }

        if !current_sticky_lines.is_empty() {
            let mut y_positions = vec![0.0; current_sticky_lines.len()];

            for i in 0..current_sticky_lines.len() {
                let slot_y = i as f32 * self.line_height;
                y_positions[i] = slot_y + tab_bar_h;
            }

            let s = self.scale_factor;
            let minimap_w = self.minimap_width;
            let rect_w = self.width - minimap_w;

            let (first, second) = editor.text_parts();
            let first_len = first.len();

            for i in (0..current_sticky_lines.len()).rev() {
                let (s_line, _) = current_sticky_lines[i];
                let rect_y = y_positions[i];

                if rect_y + self.line_height < 0.0 {
                    continue;
                }

                let mut alpha = 1.0;
                if i == current_sticky_lines.len() - 1 && anim_progress < 1.0 {
                    let p = anim_progress;
                    alpha = if anim_is_adding {
                        1.0 - (1.0 - p) * (1.0 - p)
                    } else {
                        (1.0 - p) * (1.0 - p)
                    };
                }

                let sticky_bg = [
                    self.theme.minimap_bg[0],
                    self.theme.minimap_bg[1],
                    self.theme.minimap_bg[2],
                    alpha,
                ];
                let shadow_top = [0.0, 0.0, 0.0, 0.4 * alpha];
                let shadow_bottom = [0.0, 0.0, 0.0, 0.0];

                let sticky_x = gutter_x + 2.0;
                let sticky_w = rect_w - sticky_x;

                self.push_rect(sticky_x, rect_y, sticky_w, self.line_height, sticky_bg);
                if i == current_sticky_lines.len() - 1 {
                    self.push_vertical_gradient(
                        sticky_x,
                        rect_y + self.line_height,
                        sticky_w,
                        8.0 * s,
                        shadow_top,
                        shadow_bottom,
                    );
                }

                let mut n = s_line + 1;
                let mut buf = [0u8; 20];
                let mut idx = 20;
                while n > 0 {
                    idx -= 1;
                    buf[idx] = b'0' + (n % 10) as u8;
                    n /= 10;
                }
                if let Ok(num_str) = std::str::from_utf8(&buf[idx..]) {
                    let num_w = self.measure_ui_width(num_str, 1.0);
                    let draw_x = self.left_padding - 24.0 * s - num_w;
                    let base_num_alpha = *self.theme.line_num.get(3).unwrap_or(&1.0);
                    let num_color = [
                        self.theme.line_num[0],
                        self.theme.line_num[1],
                        self.theme.line_num[2],
                        base_num_alpha * alpha,
                    ];
                    self.draw_string_scaled(
                        num_str,
                        draw_x,
                        rect_y + self.baseline_offset,
                        num_color,
                        1.0,
                    );
                }

                let Some(&start_byte) = editor.line_offsets.get(s_line) else {
                    continue;
                };
                let end_byte = *editor.line_offsets.get(s_line + 1).unwrap_or(&editor.len());
                let mut x = self.left_padding - render_scroll_x;

                // Ограничиваем текст зоной редактора — не выходим за гаттер слева
                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    let sc_y = (self.height - (rect_y + self.line_height)).round() as i32;
                    self.gl.scissor(
                        self.left_padding.round() as i32,
                        sc_y,
                        (self.width - self.left_padding - minimap_w).round() as i32,
                        self.line_height.round() as i32,
                    );
                }

                let mut current_offset = start_byte;
                while current_offset < end_byte {
                    let chunk = if current_offset < first_len {
                        let chunk_end = end_byte.min(first_len);
                        &first[current_offset..chunk_end]
                    } else {
                        let chunk_start = current_offset - first_len;
                        let chunk_end = end_byte - first_len;
                        &second[chunk_start..chunk_end]
                    };
                    if chunk.is_empty() {
                        break;
                    }
                    x = self.draw_spanned_ui_line_pixel_snapped_alpha(
                        chunk,
                        spans,
                        Some(current_offset),
                        x,
                        rect_y + self.baseline_offset,
                        self.width - minimap_w - 20.0,
                        1.0,
                        alpha,
                    );
                    current_offset = current_offset.saturating_add(chunk.len());
                    if x > self.width - minimap_w - 20.0 {
                        break;
                    }
                }

                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }

                ui_registry.register_rect(
                    crate::ui_system::UiId::StickyLine(start_byte, i),
                    sticky_x,
                    rect_y,
                    sticky_w,
                    self.line_height,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );

                self.sticky_scroll_rects.push((
                    sticky_x,
                    rect_y,
                    sticky_w,
                    self.line_height,
                    start_byte,
                ));
            }
            self.flush();
        }

        target_sticky_lines
    }
}
