use super::*;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub fn draw_problems_panel(
        &mut self,
        content_x: f32,
        content_y: f32,
        content_w: f32,
        content_h: f32,
        s: f32,
        lsp: Option<&crate::lsp::LspManager>,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let pad_x = 12.0 * s;
        let text_scale = 0.92;
        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        let mut tab_x = content_x + pad_x;
        let tab_y = content_y + 8.0 * s;
        let tab_h = 24.0 * s;

        let tabs = ["Текущий файл", "Все"];
        for (i, t) in tabs.iter().enumerate() {
            let tw = self.measure_ui_width(t, text_scale) + 16.0 * s;
            let is_active = ide_panel.problems_tab == i;
            let bg = if is_active {
                [1.0, 1.0, 1.0, 0.12]
            } else {
                [1.0, 1.0, 1.0, 0.0]
            };
            let fg = if is_active {
                self.theme.fg
            } else {
                [0.65, 0.65, 0.65, 1.0]
            };

            let is_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::ProblemsTab(i),
                tab_x,
                tab_y,
                tw,
                tab_h,
                mx,
                my,
            );

            if is_active || is_hovered {
                let draw_bg = if is_hovered && !is_active {
                    [1.0, 1.0, 1.0, 0.06]
                } else {
                    bg
                };

                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    let sy = (self.height - (tab_y + tab_h)).round() as i32;
                    self.gl.scissor(
                        tab_x.round() as i32,
                        sy,
                        tw.round() as i32,
                        tab_h.round() as i32,
                    );
                }

                self.push_rounded_rect(
                    tab_x.round(),
                    tab_y.round(),
                    tw,
                    tab_h + 4.0 * s,
                    4.0 * s,
                    draw_bg,
                );

                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }
            }

            if is_active {
                self.push_rect(
                    tab_x.round(),
                    (tab_y + tab_h).round(),
                    tw,
                    2.0 * s,
                    [0.741, 0.576, 0.976, 1.0],
                );
            }

            self.draw_string_scaled(
                t,
                tab_x + 8.0 * s,
                (tab_y + tab_h / 2.0 + 4.0 * s).round(),
                fg,
                text_scale,
            );
            tab_x += tw + 8.0 * s;
        }

        let header_bottom_y = tab_y + tab_h + 2.0 * s;
        self.push_rect(
            content_x,
            header_bottom_y.round(),
            content_w,
            1.0,
            [1.0, 1.0, 1.0, 0.08],
        );

        self.flush();

        let list_y = header_bottom_y + 6.0 * s;
        let list_h = content_h - (list_y - content_y);
        if list_h <= 1.0 {
            return;
        }

        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (list_y + list_h)).round() as i32;
            self.gl.scissor(
                content_x.round() as i32,
                sy,
                content_w.round() as i32,
                list_h.round() as i32,
            );
        }

        let scroll_y = ide_panel.problems_scroll.current.round();
        let hover_settled =
            (ide_panel.problems_scroll.current - ide_panel.problems_scroll.target).abs() < 0.5;

        if ide_panel.flat_diags.is_empty() {
            let hint = "Нет ляпов";
            let tw = self.measure_ui_width(hint, text_scale);
            self.draw_string_scaled(
                hint,
                content_x + (content_w - tw) / 2.0,
                (list_y + 32.0 * s).round(),
                [0.45, 0.45, 0.45, 1.0],
                text_scale,
            );
        } else {
            let mut current_y = list_y - scroll_y;
            let item_h = 24.0 * s;

            for (idx, (path, diag_idx)) in ide_panel.flat_diags.iter().enumerate() {
                if *diag_idx == usize::MAX {
                    if current_y + item_h > list_y && current_y < list_y + list_h {
                        ui_registry.register_rect(
                            crate::ui_system::UiId::ProblemFileToggle(idx),
                            content_x,
                            current_y,
                            content_w,
                            item_h,
                            self.last_mouse_x,
                            self.last_mouse_y,
                        );
                        if hover_settled
                            && ui_registry.hovered()
                            == Some(crate::ui_system::UiId::ProblemFileToggle(idx))
                        {
                            self.push_rect(
                                content_x,
                                current_y,
                                content_w,
                                item_h,
                                [1.0, 1.0, 1.0, 0.05],
                            );
                        }

                        let is_collapsed = ide_panel.problems_collapsed.contains(path);
                        let arrow_icon = if is_collapsed {
                            crate::widgets::IconType::Up
                        } else {
                            crate::widgets::IconType::Down
                        };

                        let icon_sz = 22.0 * s;
                        let icon_x = content_x + pad_x - 3.0 * s;
                        let icon_y = current_y + (item_h - icon_sz) / 2.0;
                        self.draw_atlas_icon(
                            arrow_icon,
                            icon_x,
                            icon_y,
                            icon_sz,
                            [0.6, 0.6, 0.6, 1.0],
                        );

                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let text_x = icon_x + icon_sz + 2.0 * s;
                        let text_y = current_y + item_h * 0.7;

                        let (err_count, warn_count) = if let Some(l) = lsp {
                            l.diagnostic_counts_for_path(path)
                        } else {
                            (0, 0)
                        };

                        let mut scratch = std::mem::take(&mut self.scratch_buffer);
                        scratch.clear();
                        let _ =
                            std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", file_name));
                        let title_w = self.measure_ui_width(&scratch, text_scale);
                        self.draw_string_scaled(
                            &scratch,
                            text_x.round(),
                            text_y.round(),
                            self.theme.fg,
                            text_scale,
                        );

                        let mut badges_x = text_x.round() + title_w + 16.0 * s;
                        if err_count > 0 {
                            scratch.clear();
                            let _ = std::fmt::Write::write_fmt(
                                &mut scratch,
                                format_args!("{} Ошибок", err_count),
                            );
                            let ew = self.measure_ui_width(&scratch, text_scale);
                            self.draw_string_scaled(
                                &scratch,
                                badges_x,
                                text_y.round(),
                                self.theme.diag_error,
                                text_scale,
                            );
                            badges_x += ew + 12.0 * s;
                        }
                        if warn_count > 0 {
                            scratch.clear();
                            let _ = std::fmt::Write::write_fmt(
                                &mut scratch,
                                format_args!("{} Предупреждений", warn_count),
                            );
                            self.draw_string_scaled(
                                &scratch,
                                badges_x,
                                text_y.round(),
                                self.theme.diag_warn,
                                text_scale,
                            );
                        }
                        self.scratch_buffer = scratch;
                    }
                    current_y += item_h;
                    continue;
                }

                let diag = if let Some(l) = lsp {
                    if let Some(d) = l.diagnostic_at(path, *diag_idx) {
                        d
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };
                if current_y + item_h > list_y && current_y < list_y + list_h {
                    let is_all_tab = ide_panel.problems_tab == 1;
                    let indent = if is_all_tab { 24.0 * s } else { 0.0 };
                    let icon_sz = 16.0 * s;
                    let icon_x = content_x + pad_x + indent;
                    let icon_y = current_y + (item_h - icon_sz) / 2.0;

                    ui_registry.register_rect(
                        crate::ui_system::UiId::ProblemJump(idx),
                        content_x,
                        current_y,
                        content_w - 14.0 * s,
                        item_h,
                        self.last_mouse_x,
                        self.last_mouse_y,
                    );
                    if hover_settled
                        && ui_registry.hovered() == Some(crate::ui_system::UiId::ProblemJump(idx))
                    {
                        self.push_rect(
                            content_x,
                            current_y,
                            content_w - 14.0 * s,
                            item_h,
                            [1.0, 1.0, 1.0, 0.05],
                        );
                    }

                    let (icon, color) = match diag.severity {
                        crate::lsp::DiagSeverity::Error => (
                            crate::widgets::IconType::Error,
                            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.78],
                        ),
                        crate::lsp::DiagSeverity::Warning => (
                            crate::widgets::IconType::Warning,
                            [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.78],
                        ),
                        _ => (crate::widgets::IconType::Problems, [0.5, 0.5, 0.5, 1.0]),
                    };

                    self.draw_atlas_icon(icon, icon_x, icon_y, icon_sz, color);

                    let text_x = icon_x + icon_sz + 8.0 * s;
                    let text_y = current_y + item_h * 0.7;

                    let mut scratch = std::mem::take(&mut self.scratch_buffer);
                    scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut scratch,
                        format_args!("Строка {}: ", diag.start_line + 1),
                    );
                    let prefix_w = self.measure_ui_width(&scratch, text_scale).round();
                    self.draw_string_scaled(
                        &scratch,
                        text_x.round(),
                        text_y.round(),
                        self.theme.fg,
                        text_scale,
                    );

                    let mut current_tx = text_x.round() + prefix_w;
                    scratch.clear();
                    for ch in diag.message.lines().next().unwrap_or("").chars() {
                        scratch.push(if ch == '\t' { ' ' } else { ch });
                    }
                    let msg_w = self.measure_ui_width(&scratch, text_scale).round();
                    self.draw_string_scaled(
                        &scratch,
                        current_tx,
                        text_y.round(),
                        self.theme.fg,
                        text_scale,
                    );
                    current_tx += msg_w + self.measure_ui_width(" ", text_scale).round();

                    scratch.clear();
                    match (&diag.source, &diag.code) {
                        (Some(src), Some(_)) => {
                            let _ =
                                std::fmt::Write::write_fmt(&mut scratch, format_args!("({} ", src));
                        }
                        (Some(src), None) => {
                            let _ =
                                std::fmt::Write::write_fmt(&mut scratch, format_args!("({})", src));
                        }
                        (None, Some(_)) => scratch.push('('),
                        (None, None) => scratch.push_str("(LSP)"),
                    };

                    let p_w = self.measure_ui_width(&scratch, text_scale).round();
                    self.draw_string_scaled(
                        &scratch,
                        current_tx,
                        text_y.round(),
                        [0.55, 0.55, 0.6, 1.0],
                        text_scale,
                    );
                    self.scratch_buffer = scratch;

                    if let Some(code) = &diag.code {
                        let sfx_x = current_tx + p_w;
                        let sfx_w = self.measure_ui_width(code, text_scale).round();
                        let link_color = [0.72, 0.52, 1.0, 1.0];
                        let sfx_color = if diag.code_href.is_some() {
                            link_color
                        } else {
                            [link_color[0], link_color[1], link_color[2], 0.85]
                        };

                        self.draw_string_scaled(code, sfx_x, text_y.round(), sfx_color, text_scale);
                        self.draw_string_scaled(
                            ")",
                            sfx_x + sfx_w,
                            text_y.round(),
                            [0.55, 0.55, 0.6, 1.0],
                            text_scale,
                        );

                        if diag.code_href.is_some() {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::ProblemUrl(idx),
                                sfx_x - 1.0,
                                current_y,
                                sfx_w + 2.0,
                                item_h,
                                self.last_mouse_x,
                                self.last_mouse_y,
                            );
                            if hover_settled
                                && ui_registry.hovered()
                                == Some(crate::ui_system::UiId::ProblemUrl(idx))
                            {
                                self.push_rect(
                                    sfx_x,
                                    text_y.round() + 1.0,
                                    sfx_w,
                                    1.0,
                                    [link_color[0], link_color[1], link_color[2], 0.9],
                                );
                            } else {
                                self.push_rect(
                                    sfx_x,
                                    text_y.round() + 1.0,
                                    sfx_w,
                                    1.0,
                                    [link_color[0], link_color[1], link_color[2], 0.55],
                                );
                            }
                        }
                    }
                }
                current_y += item_h;
            }

            let total_h = ide_panel.flat_diags.len() as f32 * item_h;
            let track_h = content_h - 40.0 * s;
            if total_h > track_h {
                let max_scroll = total_h - track_h;
                let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
                let thumb_h = (track_h / total_h * track_h).max(20.0 * s);
                let thumb_y = list_y + scroll_ratio * (track_h - thumb_h);

                self.push_rounded_rect(
                    content_x + content_w - 12.0 * s,
                    thumb_y.round(),
                    6.0 * s,
                    thumb_h,
                    3.0 * s,
                    [0.45, 0.45, 0.55, 0.5],
                );
            }
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }
}
