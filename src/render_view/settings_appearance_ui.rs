use crate::renderer::Renderer;
use crate::renderer::Theme;
use crate::theme::{SyntaxPalette, SyntaxRole, ThemeId};
use crate::ui_system::{ThemeTarget, UiId, UiRegistry};

pub(super) fn draw(
    renderer: &mut Renderer,
    x: f32,
    y: f32,
    width: f32,
    active_theme: ThemeId,
    ui: &mut UiRegistry,
) {
    let s = renderer.scale_factor;
    let palette = renderer.ui;
    renderer.draw_string_scaled("Тема", x.round(), y.round(), palette.text, 1.0);

    let row_h = (54.0 * s).round().max(1.0);
    let row_gap = (8.0 * s).round();
    let sample_w = (132.0 * s).round().min((width - 20.0 * s).max(0.0));
    let sample_h = (28.0 * s).round();
    let swatch_w = (7.0 * s).round().max(1.0);
    let swatch_gap = (3.0 * s).round();
    let mut row_y = (y + (34.0 * s).round()).round();

    for theme_id in ThemeId::ALL {
        let syntax = SyntaxPalette::for_id(theme_id);
        let row_rect = (x, row_y, width, row_h);
        let hovered = ui.register_rect(
            UiId::SettingsThemePick(ThemeTarget::Both, theme_id),
            row_rect.0,
            row_rect.1,
            row_rect.2,
            row_rect.3,
            renderer.last_mouse_x,
            renderer.last_mouse_y,
        );
        if hovered {
            renderer.push_rounded_rect(x, row_y, width, row_h, 5.0 * s, palette.bg_raised);
        }

        let sample_y = (row_y + ((row_h - sample_h) * 0.5).round()).round();
        renderer.push_rect(
            x.round(),
            sample_y,
            sample_w,
            sample_h,
            Theme::background_for_id(theme_id),
        );
        let colors = [
            syntax.color(SyntaxRole::KeywordControl),
            syntax.color(SyntaxRole::Function),
            syntax.color(SyntaxRole::String),
            syntax.color(SyntaxRole::Constant),
        ];
        let mut swatch_x = (x + (7.0 * s).round()).round();
        for color in colors {
            renderer.push_rect(swatch_x, (sample_y + (5.0 * s).round()).round(), swatch_w, (sample_h - (10.0 * s).round()).max(1.0), color);
            swatch_x = (swatch_x + swatch_w + swatch_gap).round();
        }
        renderer.draw_string_scaled(
            theme_id.label(),
            (x + sample_w + 14.0 * s).round(),
            Renderer::tree_row_text_y(row_y, row_h, s),
            palette.text,
            1.0,
        );

        if theme_id == active_theme {
            let edge = (1.0 * s).round().max(1.0);
            renderer.push_rect(x.round(), row_y.round(), width, edge, palette.accent);
            renderer.push_rect(x.round(), (row_y + row_h - edge).round(), width, edge, palette.accent);
            renderer.push_rect(x.round(), row_y.round(), edge, row_h, palette.accent);
            renderer.push_rect((x + width - edge).round(), row_y.round(), edge, row_h, palette.accent);
        }
        row_y = (row_y + row_h + row_gap).round();
    }
}
