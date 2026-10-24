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
pub(crate) fn rgb_to_rgba(color: Rgb) -> [f32; 4] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;
    use ronsole_term::{CellFlags, TerminalColor, UnderlineStyle};

    const TEST_THEMES: [ThemeId; 2] = [ThemeId::Dracula, ThemeId::OneDark];

    fn cell(flags: CellFlags, fg: TerminalColor, bg: TerminalColor) -> Cell {
        Cell::new_for_test('a', fg, bg, flags, UnderlineStyle::None)
    }

    fn default_cell(flags: CellFlags) -> Cell {
        cell(
            flags,
            TerminalColor::default_foreground(),
            TerminalColor::default_background(),
        )
    }

    #[test]
    fn default_foreground_color_unchanged() {
        for id in TEST_THEMES {
            let theme = Theme::for_id(id, [0.0; 4]);
            let palette = terminal_palette(&theme);
            let expected = theme.terminal[7];
            let (fg, bg) = terminal_cell_render_colors(&default_cell(CellFlags::empty()), &palette);

            assert_eq!(bg, None);
            assert_eq!(palette.foreground, theme_rgb(expected));
            for channel in 0..3 {
                // Compared on the u8 channel, so a one-step shift of the palette fails.
                assert_eq!(
                    (fg[channel] * 255.0).round() as u8,
                    (expected[channel] * 255.0).round() as u8,
                );
                assert!((fg[channel] - expected[channel]).abs() < 1.0 / 255.0);
            }
        }
    }

    #[test]
    fn default_background_not_painted() {
        for id in TEST_THEMES {
            let palette = terminal_palette(&Theme::for_id(id, [0.0; 4]));
            let (_, bg) = terminal_cell_render_colors(&default_cell(CellFlags::empty()), &palette);
            assert_eq!(bg, None);
        }
    }

    #[test]
    fn inverse_paints_default_fg_as_background() {
        for id in TEST_THEMES {
            let theme = Theme::for_id(id, [0.0; 4]);
            let palette = terminal_palette(&theme);
            let (plain_fg, _) = terminal_cell_render_colors(&default_cell(CellFlags::empty()), &palette);
            let (inverse_fg, inverse_bg) =
                terminal_cell_render_colors(&default_cell(CellFlags::INVERSE), &palette);

            assert_eq!(inverse_bg, Some(plain_fg));
            for (fg, bg) in inverse_fg.iter().zip(theme.terminal_bg.iter()).take(3) {
                assert_eq!((fg * 255.0).round() as u8, (bg * 255.0).round() as u8);
            }
        }
    }

    #[test]
    fn truecolor_background_painted() {
        let palette = terminal_palette(&Theme::for_id(ThemeId::Dracula, [0.0; 4]));
        let truecolor_bg = cell(
            CellFlags::empty(),
            TerminalColor::default_foreground(),
            TerminalColor::rgb(1, 2, 3),
        );
        let (_, bg) = terminal_cell_render_colors(&truecolor_bg, &palette);
        assert_eq!(bg, Some([1.0 / 255.0, 2.0 / 255.0, 3.0 / 255.0, 1.0]));
    }
}
