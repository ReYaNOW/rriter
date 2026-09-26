// Editor text-surface presses (autocomplete popup and its detail pane,
// Ctrl+click go-to-definition) and terminal mouse-tracking reports.
use super::*;

impl App {
    /// Autocomplete popup / detail pane press, hover popup reset, Ctrl+click definition. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn handle_autocomplete_and_definition_press(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            if self.autocomplete_active {
                let in_main = self
                    .autocomplete_rect
                    .is_some_and(|(x, y, w, h)| mx >= x && mx <= x + w && my >= y && my <= y + h);
                let in_detail = self
                    .autocomplete_detail_rect
                    .is_some_and(|(x, y, w, h)| mx >= x && mx <= x + w && my >= y && my <= y + h);
                if in_detail {
                    if self.autocomplete_detail_max_scroll > 0.0
                        && self.ui_registry.find_at(mx, my)
                            == Some(crate::ui_system::UiId::HoverPopupScroll)
                    {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let max_scroll = self.autocomplete_detail_max_scroll;
                        if let (Some(rect), Some(popup)) = (
                            self.autocomplete_detail_rect,
                            self.autocomplete_detail_popup.as_mut(),
                        ) {
                            if let Some((drag_offset, target)) =
                                crate::app::mouse::hover_popup_scrollbar_drag_target(
                                    rect,
                                    max_scroll,
                                    popup.scroll.current,
                                    my,
                                    s,
                                    None,
                                )
                            {
                                let _ = crate::app::mouse::apply_scrollbar_drag_target(
                                    &mut popup.scroll,
                                    target,
                                    drag_offset,
                                );
                            }
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    if let (Some(rect), Some(popup)) = (
                        self.autocomplete_detail_rect,
                        self.autocomplete_detail_popup.as_ref(),
                    ) {
                        let byte = hover_popup_byte_at(
                            self.renderer.as_mut().unwrap(),
                            popup,
                            rect,
                            mx,
                            my,
                        );
                        self.autocomplete_detail_selection_anchor = Some(byte);
                        self.autocomplete_detail_selection_cursor = Some(byte);
                        self.autocomplete_detail_selecting = true;
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return true;
                }
                if in_main {
                    if let Some(rect) = self.autocomplete_rect {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        if mx >= rect.0 + rect.2 - 14.0 * s {
                            if let Some((drag_offset, target)) = autocomplete_scroll_click_target(
                                my,
                                rect.1,
                                rect.3,
                                self.autocomplete_scroll.current,
                                self.autocomplete_options.len(),
                                s,
                            ) {
                                apply_autocomplete_scroll_drag(
                                    &mut self.autocomplete_scroll,
                                    target,
                                    drag_offset,
                                );
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return true;
                        }
                        if let Some(idx) = autocomplete_item_index_at(
                            mx,
                            my,
                            rect,
                            self.autocomplete_scroll.current,
                            self.autocomplete_options.len(),
                            s,
                        ) {
                            if idx == self.autocomplete_selected_idx {
                                self.apply_autocomplete();
                            } else {
                                self.autocomplete_selected_idx = idx;
                                self.autocomplete_hovered_idx = None;
                                self.request_active_autocomplete_detail_for_index(idx);
                            }
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return true;
                }
                self.close_autocomplete();
                self.window.as_ref().unwrap().request_redraw();
            }
            let in_hover_popup = HOVER_STATE.with(|hover_state| {
                hover_state
                    .borrow()
                    .popup_or_bridge_contains(
                        mx,
                        my,
                        self.renderer.as_ref().unwrap().width,
                        self.renderer.as_ref().unwrap().scale_factor,
                    )
                    .0
            });

            if !in_hover_popup {
                clear_hover_popup(self.renderer.as_mut());
            }

            if self.modifiers.control_key() {
                if let Some(target) = self.ctrl_definition_target_under_mouse() {
                    self.jump_to_definition_target(target);
                    return true;
                }
            }
        }
        false
    }

    /// Forwards press/release as an SGR mouse report to a terminal with mouse tracking enabled.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn forward_terminal_mouse_report(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) {
        if self.is_ide_mode
            && self.ide_panel.is_open(crate::app::PanelId::Terminal)
            && self.ide_panel.terminal_focused
        {
            if let Some(crate::ui_system::UiId::TerminalBody) = self.ui_registry.find_at(mx, my) {
                let active = self.ide_panel.active_terminal;
                let mut tracking = false;
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    if crate::app::terminal::lock_terminal_grid(&term.grid).mouse_tracking {
                        tracking = true;
                    }
                }
                if tracking {
                    let btn_code = terminal_mouse_button_code(button);
                    let is_pressed = state == ElementState::Pressed;
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let (terminal_panel_x, content_y, _, content_h, _) =
                        super::app_panel_scroll_rect(self, crate::app::PanelId::Terminal, s);
                    let panel_x = terminal_panel_x + 10.0 * s;
                    let char_w = self.renderer.as_mut().unwrap().char_advance('A')
                        * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
                    let char_h = self.renderer.as_ref().unwrap().line_height
                        * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
                    let (term_content_y, term_content_h) =
                        crate::render_view::terminal_ui::terminal_body_rect(
                            content_y, content_h, s,
                        );

                    let cell_x = terminal_mouse_cell_x(mx, panel_x, char_w);

                    let mut is_drag = false;
                    let mut cell_y = 1;
                    if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                        let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
                        let scrollback_len = if grid.is_alt {
                            0
                        } else {
                            grid.scrollback.len()
                        };
                        let total_lines = scrollback_len + grid.lines.len();
                        let max_scroll = if grid.is_alt {
                            0.0
                        } else {
                            crate::render_view::terminal_ui::terminal_max_scroll(
                                total_lines,
                                char_h,
                                term_content_h,
                                s,
                            )
                        };
                        let scroll_offset = if grid.is_alt {
                            0.0
                        } else {
                            term.scroll_y.current.min(max_scroll).round()
                        };
                        cell_y = terminal_mouse_cell_y(
                            my,
                            term_content_y,
                            term_content_h,
                            scroll_offset,
                            char_h,
                            s,
                            grid.visible_rows,
                        );

                        if is_pressed {
                            grid.selection = None;
                        } else if let Some((sx, sy, ex, ey)) = grid.selection {
                            if sx != ex || sy != ey {
                                is_drag = true;
                            }
                        }
                    }

                    if !is_drag {
                        let seq = terminal_mouse_sgr_sequence(btn_code, cell_x, cell_y, is_pressed);
                        if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                            let _ = term.write_input(seq.as_bytes());
                        }
                    }
                }
            }
        }
    }
}
