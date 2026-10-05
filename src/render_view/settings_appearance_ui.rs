use crate::renderer::Renderer;
use crate::renderer::Theme;
use crate::theme::{SyntaxPalette, SyntaxRole, ThemeId, ThemeSelection};
use crate::ui_system::{ThemeTarget, UiId, UiRegistry};

/// Draws the tab shifted up by `scroll_y` (rounded, so hit-boxes follow the
/// pixels) and returns the unscrolled y of the content bottom for the caller's
/// max-scroll computation.
pub(super) fn draw(
    renderer: &mut Renderer,
    x: f32,
    y: f32,
    width: f32,
    scroll_y: f32,
    selection: ThemeSelection,
    ui: &mut UiRegistry,
) -> f32 {
    let s = renderer.scale_factor;
    let scroll = scroll_y.round();
    let y = (y - scroll).round();
    let palette = renderer.ui;
    let linked_h = (30.0 * s).round().max(1.0);
    let linked_hovered = ui.register_rect(
        UiId::SettingsThemeLinked, x, y, width, linked_h,
        renderer.last_mouse_x, renderer.last_mouse_y,
    );
    if linked_hovered {
        renderer.push_rounded_rect(x, y, width, linked_h, 4.0 * s, palette.bg_raised);
    }
    renderer.draw_string_scaled(
        if selection.linked { "✓ Одна тема для редактора и интерфейса" } else { "□ Одна тема для редактора и интерфейса" },
        x.round(), Renderer::tree_row_text_y(y, linked_h, s), palette.text, 1.0,
    );

    let row_h = (32.0 * s).round().max(1.0);
    let row_gap = (4.0 * s).round();
    let sample_w = (112.0 * s).round().min((width - 20.0 * s).max(0.0));
    let sample_h = (22.0 * s).round();
    let swatch_w = (6.0 * s).round().max(1.0);
    let swatch_gap = (2.0 * s).round();
    let mut row_y = (y + linked_h + (12.0 * s).round()).round();
    let lists: &[(ThemeTarget, ThemeId, &'static str)] = if selection.linked {
        &[(ThemeTarget::Both, selection.editor, "Тема")]
    } else {
        &[(ThemeTarget::Editor, selection.editor, "Тема редактора"), (ThemeTarget::Ui, selection.ui, "Тема интерфейса")]
    };
    // The title is drawn on a baseline, so its glyphs rise above `row_y`; for
    // every list after the first, push the title down by its text height so it
    // clears the last row of the previous list at any UI scale.
    let title_clearance = (18.0 * s).round();
    for (list_index, &(target, active_theme, title)) in lists.iter().enumerate() {
        if list_index > 0 {
            row_y = (row_y + title_clearance).round();
        }
        renderer.draw_string_scaled(title, x.round(), row_y.round(), palette.text, 1.0);
        row_y = (row_y + (22.0 * s).round()).round();
        for theme_id in ThemeId::ALL {
        let syntax = SyntaxPalette::for_id(theme_id);
        let row_rect = (x, row_y, width, row_h);
        let hovered = ui.register_rect(
            UiId::SettingsThemePick(target, theme_id),
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
    row_y + scroll
}
