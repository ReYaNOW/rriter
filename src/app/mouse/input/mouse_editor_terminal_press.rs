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
                            let geometry = crate::app::mouse::hover_popup_scrollbar(
                                rect,
                                max_scroll,
                                popup.scroll.current,
                                s,
                            )
                            .geometry(s);
                            let _ = crate::app::mouse::press_scrollbar(
                                &mut popup.scroll,
                                geometry,
                                mx,
                                my,
                            );
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
                        let bar = autocomplete_scrollbar(
                            rect,
                            self.autocomplete_options.len(),
                            self.autocomplete_scroll.current,
                            s,
                        );
                        if mx >= bar.lane.0 {
                            let _ = crate::app::mouse::press_scrollbar(
                                &mut self.autocomplete_scroll,
                                bar.geometry(s),
                                mx,
                                my,
                            );
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
            let in_hover_popup = self
                .hover
                .popup_or_bridge_contains(
                    mx,
                    my,
                    self.renderer.as_ref().unwrap().width,
                    self.renderer.as_ref().unwrap().scale_factor,
                )
                .0;

            if !in_hover_popup {
                clear_hover_popup(&mut self.hover);
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

}
