use crate::renderer::Theme;
use ronsole_term::{Cell, Rgb, TermPalette};

/// Converts one theme channel (0.0..=1.0) to the u8 channel the terminal palette uses.
#[inline(always)]
fn theme_channel(value: f32) -> u8 {
    (value * 255.0).round() as u8
}

#[inline(always)]
fn theme_rgb(color: [f32; 4]) -> Rgb {
    Rgb {
        r: theme_channel(color[0]),
        g: theme_channel(color[1]),
        b: theme_channel(color[2]),
    }
}

#[inline(always)]
fn rgb_to_rgba(color: Rgb) -> [f32; 4] {
    [
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        1.0,
    ]
}

/// Builds the terminal palette from the theme. Default foreground is `terminal[7]`,
/// the colour the renderer has always used for unstyled text.
pub(crate) fn terminal_palette(theme: &Theme) -> TermPalette {
    TermPalette {
        foreground: theme_rgb(theme.terminal[7]),
        background: theme_rgb(theme.terminal_bg),
        cursor: theme_rgb(theme.terminal_cursor),
        ansi: std::array::from_fn(|index| theme_rgb(theme.terminal[index])),
    }
}

/// Resolves a cell to `(foreground, background)` in renderer colours. The background
/// is `None` when it is the default background, which is never painted.
#[inline(always)]
pub(crate) fn terminal_cell_render_colors(
    cell: &Cell,
    palette: &TermPalette,
) -> ([f32; 4], Option<[f32; 4]>) {
    let colors = palette.cell_colors(cell);
    let bg = if colors.bg_is_default {
        None
    } else {
        Some(rgb_to_rgba(colors.bg))
    };
    (rgb_to_rgba(colors.fg), bg)
}
