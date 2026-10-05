use crate::app::TocPopup;
use crate::render_view::scrollbar_widget::{Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle};
use crate::renderer::Renderer;
use crate::theme::UiRole;
use crate::ui_system::{UiId, UiRegistry};
use glow::HasContext;

const ROW_H: f32 = 28.0;
const HEADER_H: f32 = 36.0;
const EMPTY_ROW_H: f32 = 40.0;

fn popup_size(window_w: f32, window_h: f32, scale: f32, item_count: usize) -> (f32, f32, f32) {
    let width = (360.0 * scale).round().min((window_w - 16.0 * scale).max(0.0));
    let header_h = (HEADER_H * scale).round();
    let content_h = if item_count == 0 {
        (EMPTY_ROW_H * scale).round()
    } else {
        item_count as f32 * (ROW_H * scale).round()
    };
    let max_height = (window_h * 0.6).round().max(header_h + 8.0 * scale);
    (width, (header_h + content_h).min(max_height), content_h)
}

fn truncate_label(renderer: &mut Renderer, text: &str, max_width: f32) -> String {
    if renderer.measure_ui_width(text, 0.9) <= max_width {
        return text.to_string();
    }
    let mut end = text.len();
    while end > 0 {
        end = text[..end].char_indices().last().map_or(0, |(index, _)| index);
        let mut candidate = text[..end].to_string();
        candidate.push('…');
        if renderer.measure_ui_width(&candidate, 0.9) <= max_width {
            return candidate;
        }
    }
    "…".to_string()
}

impl Renderer {
    pub(crate) fn draw_markdown_toc(
        &mut self,
        toc: &mut TocPopup,
        ui_registry: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) -> bool {
        if !toc.open {
            toc.rect = None;
            return false;
        }

        let s = self.scale_factor;
        let (width, height, content_h) = popup_size(self.width, self.height, s, toc.items.len());
        let row_h = (ROW_H * s).round();
        let header_h = (HEADER_H * s).round();
        let x = ((self.width - width) * 0.5).round();
        let y = ((self.height - height) * 0.5).round();
        let content_y = y + header_h;
        let viewport_h = (height - header_h).max(0.0);
        toc.max_scroll = (content_h - viewport_h).max(0.0);
        toc.scroll.clamp_target(0.0, toc.max_scroll);
        toc.scroll.current = toc.scroll.current.clamp(0.0, toc.max_scroll);
        toc.rect = Some((x, y, width, height));

        ui_registry.mark_overlay_start();
        ui_registry.register_blocker(UiId::MarkdownTocToggle, x, y, width, height, mx, my);
        self.push_rounded_rect_border(
            x,
            y,
            width,
            height,
            8.0 * s,
            (1.0 * s).round(),
            self.editor_ui.pick(UiRole::Border, self.theme.sel),
            self.editor_ui.pick(UiRole::BgDialog, [0.075, 0.082, 0.12, 0.98]),
        );
        self.push_rect(x, content_y.round(), width, (1.0 * s).round(), self.editor_ui.ink(0.10));
        self.draw_string_scaled_stable(
            "Содержание",
            (x + (14.0 * s).round()).round(),
            (y + (24.0 * s).round()).round(),
            self.editor_ui.pick(UiRole::TextPrimary, self.theme.fg),
            0.96,
        );

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x.round() as i32,
                (self.height - (content_y + viewport_h)).round() as i32,
                width.round() as i32,
                viewport_h.round() as i32,
            );
        }

        let mut wants_pointer = false;
        if toc.items.is_empty() {
            self.draw_string_scaled_stable(
                "нет заголовков",
                (x + (14.0 * s).round()).round(),
                (content_y + (24.0 * s).round()).round(),
                self.editor_ui.pick(UiRole::TextPrimary, self.theme.fg),
                0.9,
            );
        } else {
            let mut row_y = content_y - toc.scroll.current.round();
            let scrollbar_w = if toc.max_scroll > 0.0 { (12.0 * s).round() } else { 0.0 };
            for (index, item) in toc.items.iter().enumerate() {
                let visible_y = row_y.max(content_y);
                let visible_bottom = (row_y + row_h).min(content_y + viewport_h);
                if visible_bottom > visible_y {
                    let row_rect = (x, visible_y, width, visible_bottom - visible_y);
                    let hovered = ui_registry.register_rect(
                        UiId::MarkdownTocItem(index),
                        row_rect.0,
                        row_rect.1,
                        row_rect.2,
                        row_rect.3,
                        mx,
                        my,
                    );
                    if toc.selected == Some(index) {
                        self.push_rect(
                            x + (2.0 * s).round(),
                            visible_y,
                            width - (4.0 * s).round(),
                            visible_bottom - visible_y,
                            self.editor_ui.pick(UiRole::Selection, [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.55]),
                        );
                    } else if hovered {
                        self.push_rect(
                            x + (2.0 * s).round(),
                            visible_y,
                            width - (4.0 * s).round(),
                            visible_bottom - visible_y,
                            self.editor_ui.ink(0.08),
                        );
                    }
                    wants_pointer |= hovered;
                    let indent = (((item.level.saturating_sub(1)) as f32) * 12.0 * s).round();
                    let text_x = (x + (14.0 * s).round() + indent).round();
                    let text_max = (width - scrollbar_w - indent - 28.0 * s).max(12.0 * s);
                    let label = truncate_label(self, &item.text, text_max);
                    self.draw_string_scaled_stable(
                        &label,
                        text_x,
                        (row_y.round() + (row_h * 0.5).round() + (5.0 * s).round()).round(),
                        self.editor_ui.pick(UiRole::TextPrimary, self.theme.fg),
                        0.9,
                    );
                }
                row_y += row_h;
                if row_y >= content_y + viewport_h {
                    break;
                }
            }
            if toc.max_scroll > 0.0 {
                let lane = (
                    (x + width - (10.0 * s).round()).round(),
                    content_y.round(),
                    (8.0 * s).round(),
                    viewport_h.round(),
                );
                let bar = Scrollbar {
                    style: ScrollbarStyle::MARKDOWN_READ,
                    axis: ScrollbarAxis::Vertical,
                    lane,
                    extent: ScrollbarExtent::new(viewport_h, content_h, toc.scroll.current),
                };
                let _ = self.draw_scrollbar(&bar, s, 1.0, None);
            }
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        wants_pointer || crate::ui_system::point_in_rect(mx, my, (x, y, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toc_row_dimensions_scale_and_round_to_pixels() {
        assert_eq!((ROW_H * 1.25).round(), 35.0);
        assert_eq!((HEADER_H * 1.25).round(), 45.0);
        assert_eq!((EMPTY_ROW_H * 1.25).round(), 50.0);
    }

    #[test]
    fn popup_size_uses_fixed_width_and_caps_tall_lists_at_sixty_percent() {
        assert_eq!(popup_size(1280.0, 720.0, 1.0, 2), (360.0, 92.0, 56.0));
        assert_eq!(popup_size(320.0, 200.0, 1.0, 40), (304.0, 120.0, 1120.0));
        assert_eq!(popup_size(320.0, 200.0, 1.0, 0), (304.0, 76.0, 40.0));
        assert_eq!(popup_size(1280.0, 720.0, 2.0, 1), (720.0, 128.0, 56.0));
    }
}
