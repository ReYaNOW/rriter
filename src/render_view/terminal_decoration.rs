//! Geometry of terminal text decorations: SGR underline styles, strikethrough and the
//! hovered-link underline. Everything here is pure; the renderer turns the output into quads.

use super::terminal_colors::{rgb_to_rgba, terminal_cell_render_colors};
use crate::renderer::Renderer;
use ronsole_term::{Cell, CellFlags, LinkTarget, TermPalette, UnderlineStyle};
use std::ops::Range;

/// Upper bound on quads one cell can produce: a double underline or a single underline plus strike.
pub(crate) const DECORATION_MAX_QUADS: usize = 3;

/// Face index of the bundled Regular terminal font chain.
pub(crate) const TERMINAL_FACE_REGULAR: u8 = 0;
/// Face index of the bundled Bold terminal font.
pub(crate) const TERMINAL_FACE_BOLD: u8 = 1;
/// Face index of the bundled Italic terminal font.
pub(crate) const TERMINAL_FACE_ITALIC: u8 = 2;
/// Face index of the bundled Bold Italic terminal font.
pub(crate) const TERMINAL_FACE_BOLD_ITALIC: u8 = 3;

/// Pattern a decoration quad is drawn with. The renderer maps `Solid` to a plain rectangle and
/// every other variant to the decoration shader mode with the matching template number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecorationKind {
    Solid,
    /// Curly underline, shader template 1.
    Wave,
    /// Dotted underline, shader template 2.
    Dots,
    /// Dashed underline, shader template 3.
    Dashes,
}

/// One rectangle of a decoration. For patterns `u` and `uw` are the phase span in pattern
/// periods; `u` is the unrounded cell origin divided by the period, so neighbouring cells share
/// one absolute phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DecorationQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub kind: DecorationKind,
    pub u: f32,
    pub uw: f32,
    pub color: [f32; 4],
}

/// Decoration state of one cell, already resolved to renderer colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellDecoration {
    pub underline: UnderlineStyle,
    pub underline_color: [f32; 4],
    pub strike: bool,
    pub strike_color: [f32; 4],
}

/// Face index for the styling bits of a cell. Italic and bold select the matching bundled face.
#[inline]
pub(crate) fn terminal_cell_face_from_flags(flags: CellFlags) -> u8 {
    match (flags.contains(CellFlags::BOLD), flags.contains(CellFlags::ITALIC)) {
        (false, false) => TERMINAL_FACE_REGULAR,
        (true, false) => TERMINAL_FACE_BOLD,
        (false, true) => TERMINAL_FACE_ITALIC,
        (true, true) => TERMINAL_FACE_BOLD_ITALIC,
    }
}

/// Absolute horizontal phase of column `col`, computed the same way as the cell origin so that
/// the decoration pattern stays continuous across cells.
#[inline]
pub(crate) fn terminal_decoration_phase(draw_x: f32, col: usize, char_w: f32) -> f32 {
    draw_x + col as f32 * char_w
}

/// Top edge of a row's underline band, keeping the band inside the row.
#[inline]
pub(crate) fn terminal_underline_top(
    baseline_y: f32,
    thickness: f32,
    row_bottom: f32,
    extent: f32,
) -> f32 {
    (baseline_y + thickness).min(row_bottom - extent)
}

/// Builds the rectangles of `decoration` for the cell at column `col`.
///
/// `baseline_y` is the rounded baseline of the row and `row_bottom` the integer bottom edge of
/// the row. `x_height` is the Regular face x-height in pixels, `ui_scale` the window scale factor
/// whose rounded value is the line thickness.
/// Every coordinate is an integer except the pattern phase.
#[allow(clippy::too_many_arguments)]
pub(crate) fn terminal_decoration_quads(
    decoration: &CellDecoration,
    draw_x: f32,
    col: usize,
    char_w: f32,
    baseline_y: f32,
    row_bottom: f32,
    x_height: f32,
    ui_scale: f32,
) -> ([DecorationQuad; DECORATION_MAX_QUADS], usize) {
    let thickness = ui_scale.round().max(1.0);
    let text_x = terminal_decoration_phase(draw_x, col, char_w);
    let x = text_x.round();
    let next_x = terminal_decoration_phase(draw_x, col + 1, char_w).round();
    let width = next_x - x;
    let placeholder = DecorationQuad {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
        kind: DecorationKind::Solid,
        u: 0.0,
        uw: 0.0,
        color: [0.0; 4],
    };
    let mut quads = [placeholder; DECORATION_MAX_QUADS];
    let mut len = 0usize;
    let mut push = |quad: DecorationQuad| {
        quads[len] = quad;
        len += 1;
    };
    let solid = |y: f32, height: f32, color: [f32; 4]| DecorationQuad {
        x,
        y,
        w: width,
        h: height,
        kind: DecorationKind::Solid,
        u: 0.0,
        uw: 0.0,
        color,
    };
    // Pattern quads span three thicknesses so the wave can swing around the band.
    let pattern = |y: f32, kind: DecorationKind, period: f32, color: [f32; 4]| DecorationQuad {
        x,
        y,
        w: width,
        h: 3.0 * thickness,
        kind,
        u: text_x / period,
        uw: (next_x - x) / period,
        color,
    };
    let extent = match decoration.underline {
        UnderlineStyle::None | UnderlineStyle::Single => thickness,
        UnderlineStyle::Double
        | UnderlineStyle::Curly
        | UnderlineStyle::Dotted
        | UnderlineStyle::Dashed => 3.0 * thickness,
    };
    let top = terminal_underline_top(baseline_y, thickness, row_bottom, extent);
    let uc = decoration.underline_color;
    match decoration.underline {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => push(solid(top, thickness, uc)),
        UnderlineStyle::Double => {
            push(solid(top, thickness, uc));
            push(solid(top + 2.0 * thickness, thickness, uc));
        }
        UnderlineStyle::Curly => push(pattern(top, DecorationKind::Wave, char_w, uc)),
        UnderlineStyle::Dotted => push(pattern(top, DecorationKind::Dots, 2.0 * thickness, uc)),
        UnderlineStyle::Dashed => push(pattern(top, DecorationKind::Dashes, 5.0 * thickness, uc)),
    }
    if decoration.strike {
        // Centre the band on the x-height midline: its top edge sits half a thickness higher.
        let strike_y = (baseline_y - x_height * 0.5 - thickness * 0.5).round();
        push(solid(strike_y, thickness, decoration.strike_color));
    }
    (quads, len)
}

/// Column ranges of the hovered link on `row`, or an empty slice when there is no hover or its
/// generation is stale. `cells` is sorted by row, so the row is found by binary search.
#[inline]
pub(crate) fn terminal_hovered_link_row_ranges(
    hovered: Option<&(LinkTarget, u64)>,
    generation: u64,
    row: usize,
) -> &[(usize, Range<usize>)] {
    let Some((link, link_generation)) = hovered else {
        return &[];
    };
    if *link_generation != generation {
        return &[];
    }
    let start = link.cells.partition_point(|(link_row, _)| *link_row < row);
    let end = link.cells.partition_point(|(link_row, _)| *link_row <= row);
    &link.cells[start..end]
}

/// Whether column `col` lies in one of `ranges`, the sorted, disjoint column ranges of one row.
/// `next` is the caller's cursor, so columns must be visited in increasing order.
#[inline]
pub(crate) fn terminal_row_ranges_contain(
    ranges: &[(usize, Range<usize>)],
    next: &mut usize,
    col: usize,
) -> bool {
    while ranges
        .get(*next)
        .is_some_and(|(_, range)| range.end <= col)
    {
        *next += 1;
    }
    ranges
        .get(*next)
        .is_some_and(|(_, range)| range.start <= col)
}

/// Whether a cell is concealed (SGR 8) or hidden by the host. Such a cell gets neither glyph
/// nor decoration.
#[inline]
pub(crate) fn terminal_cell_is_hidden(cell: &Cell) -> bool {
    let flags = cell.flags();
    flags.contains(CellFlags::CONCEAL) || flags.contains(CellFlags::HOST_HIDDEN)
}

/// Decoration of one cell, or `None` when the cell draws none. A hovered link gets a single
/// underline; strikethrough and the underline colour are kept.
#[inline]
pub(crate) fn terminal_cell_decoration(
    cell: &Cell,
    palette: &TermPalette,
    hovered: bool,
) -> Option<CellDecoration> {
    if terminal_cell_is_hidden(cell) {
        return None;
    }
    let strike = cell.flags().contains(CellFlags::STRIKE);
    if !hovered && !strike && matches!(cell.underline(), UnderlineStyle::None) {
        return None;
    }
    // A hovered link replaces any underline style with a single one; the colour is the cell's
    // underline colour when it has one, the rendered foreground otherwise.
    let underline = if hovered {
        UnderlineStyle::Single
    } else {
        cell.underline()
    };
    let (fg, _) = terminal_cell_render_colors(cell, palette);
    let underline_color = cell
        .underline_color()
        .map_or(fg, |color| rgb_to_rgba(palette.rgb(color)));
    Some(CellDecoration {
        underline,
        underline_color,
        strike,
        strike_color: fg,
    })
}

impl Renderer {
    /// Pushes the decorations of one visible row, drawn after the row's glyphs. `row_index` is the
    /// grid row index used for the hovered-link lookup and `draw_y` the top edge of the row.
    /// `scale` is the text scale that places the baseline, `ui_scale` the window scale that sets
    /// the line thickness, and `cols` bounds the walk to the grid width.
    /// Cells without a decoration return before any colour work.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_terminal_row_decorations(
        &mut self,
        row: &[Cell],
        row_index: usize,
        draw_x: f32,
        draw_y: f32,
        char_w: f32,
        char_h: f32,
        scale: f32,
        ui_scale: f32,
        cols: usize,
        palette: &TermPalette,
        hovered: Option<&(LinkTarget, u64)>,
        generation: u64,
    ) {
        let baseline_y = (draw_y + self.baseline_offset * scale).round();
        let row_bottom = (draw_y + char_h).floor();
        let x_height = self.terminal_regular_x_height_at_size(self.final_text_pixel_size(scale));
        let hovered_ranges = terminal_hovered_link_row_ranges(hovered, generation, row_index);
        // Columns are visited in increasing order, so the hovered ranges are walked once per row.
        let mut link_next = 0usize;
        for (col, cell) in row.iter().take(cols).enumerate() {
            let hovered_cell = terminal_row_ranges_contain(hovered_ranges, &mut link_next, col);
            let Some(decoration) = terminal_cell_decoration(cell, palette, hovered_cell) else {
                continue;
            };
            let (quads, len) = terminal_decoration_quads(
                &decoration,
                draw_x,
                col,
                char_w,
                baseline_y,
                row_bottom,
                x_height,
                ui_scale,
            );
            for quad in &quads[..len] {
                match quad.kind {
                    DecorationKind::Solid => {
                        self.push_rect(quad.x, quad.y, quad.w, quad.h, quad.color);
                    }
                    DecorationKind::Wave => self.push_decoration_quad(
                        quad.x, quad.y, quad.w, quad.h, quad.u, quad.uw, quad.color, 1.0,
                    ),
                    DecorationKind::Dots => self.push_decoration_quad(
                        quad.x, quad.y, quad.w, quad.h, quad.u, quad.uw, quad.color, 2.0,
                    ),
                    DecorationKind::Dashes => self.push_decoration_quad(
                        quad.x, quad.y, quad.w, quad.h, quad.u, quad.uw, quad.color, 3.0,
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_view::terminal_colors::terminal_palette;
    use crate::renderer::Theme;
    use crate::theme::ThemeId;
    use ronsole_term::{Term, TermConfig, TermIdentity, TermSink, TerminalColor};
    use std::time::Instant;

    /// Sink that drops replies, titles and clipboard text; the cell tests need only the grid.
    struct NullSink;

    impl TermSink for NullSink {
        fn reply(&mut self, _bytes: &[u8]) {}
        fn title(&mut self, _title: &str) {}
        fn clipboard_set(&mut self, _text: String) {}
        fn working_directory(&mut self, _host: &str, _path: &[u8]) {}
    }

    const WHITE: [f32; 4] = [1.0; 4];

    fn decoration(underline: UnderlineStyle, strike: bool) -> CellDecoration {
        CellDecoration {
            underline,
            underline_color: WHITE,
            strike,
            strike_color: WHITE,
        }
    }

    #[test]
    fn terminal_cell_face_from_flags_selects_bundled_face() {
        assert_eq!(terminal_cell_face_from_flags(CellFlags::empty()), TERMINAL_FACE_REGULAR);
        assert_eq!(terminal_cell_face_from_flags(CellFlags::BOLD), TERMINAL_FACE_BOLD);
        assert_eq!(terminal_cell_face_from_flags(CellFlags::ITALIC), TERMINAL_FACE_ITALIC);
        assert_eq!(
            terminal_cell_face_from_flags(CellFlags::BOLD | CellFlags::ITALIC),
            TERMINAL_FACE_BOLD_ITALIC
        );
    }

    #[test]
    fn decoration_y_is_integer_at_1_333_and_1_0() {
        for scale in [1.0_f32, 1.333] {
            let baseline = 28.0;
            let row_bottom = 40.0;
            let (quads, len) = terminal_decoration_quads(
                &decoration(UnderlineStyle::Single, true),
                10.0,
                0,
                8.0,
                baseline,
                row_bottom,
                9.9,
                scale,
            );
            assert_eq!(len, 2);
            for quad in &quads[..len] {
                assert_eq!(quad.y, quad.y.round(), "scale {scale}");
                assert_eq!(quad.h, quad.h.round(), "scale {scale}");
            }
        }
    }

    #[test]
    fn double_underline_gap_equals_thickness() {
        let (quads, len) = terminal_decoration_quads(
            &decoration(UnderlineStyle::Double, false),
            0.0,
            0,
            8.0,
            28.0,
            40.0,
            9.9,
            2.0,
        );
        assert_eq!(len, 2);
        let (first, second) = (quads[0], quads[1]);
        let thickness = first.h;
        assert_eq!(second.y - (first.y + first.h), thickness);
    }

    #[test]
    fn curly_phase_continuous_between_cells() {
        let draw_x = 10.3_f32;
        let char_w = 8.4_f32 * 1.333;
        for n in 0..50_usize {
            let delta = terminal_decoration_phase(draw_x, n + 1, char_w)
                - terminal_decoration_phase(draw_x, n, char_w);
            assert!((delta - char_w).abs() <= 1e-3, "column {n}: {delta}");
        }
        // Phase of column n is the cell origin expression, unrounded.
        assert_eq!(terminal_decoration_phase(draw_x, 3, char_w), draw_x + 3.0 * char_w);
    }

    #[test]
    fn strike_at_half_x_height() {
        let strike_top = |thickness: f32| {
            let (quads, len) = terminal_decoration_quads(
                &decoration(UnderlineStyle::None, true),
                0.0,
                0,
                8.0,
                28.0,
                40.0,
                9.9,
                thickness,
            );
            assert_eq!(len, 1);
            quads[0].y
        };
        assert_eq!(strike_top(1.0), 23.0);
        assert_eq!(strike_top(2.0), 22.0);
    }

    #[test]
    fn underline_stays_inside_row_at_fractional_scales() {
        let styles = [
            UnderlineStyle::Single,
            UnderlineStyle::Double,
            UnderlineStyle::Curly,
            UnderlineStyle::Dotted,
            UnderlineStyle::Dashed,
        ];
        for scale in [1.0_f32, 1.333, 1.5, 1.75] {
            for style in styles {
                for draw_y_fraction in [0.0_f32, 0.25, 0.5, 0.75] {
                    let char_h = 18.0_f32;
                    let draw_y = 100.0 + draw_y_fraction;
                    let row_bottom = (draw_y + char_h).floor();
                    let baseline = (draw_y + 14.0).round();
                    let (quads, len) = terminal_decoration_quads(
                        &decoration(style, false),
                        0.0,
                        0,
                        8.0,
                        baseline,
                        row_bottom,
                        9.9,
                        scale,
                    );
                    for quad in &quads[..len] {
                        assert!(
                            quad.y + quad.h <= row_bottom,
                            "{style:?} at scale {scale}: bottom {} > {row_bottom}",
                            quad.y + quad.h
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn hovered_link_underlines_range_when_generation_matches() {
        let link = LinkTarget {
            uri: "https://example.invalid".into(),
            cells: vec![(1, 2..5), (2, 0..3), (2, 6..9)],
        };
        let hovered = Some((link, 7_u64));
        assert!(terminal_hovered_link_row_ranges(hovered.as_ref(), 6, 2).is_empty());
        let row_two = terminal_hovered_link_row_ranges(hovered.as_ref(), 7, 2);
        assert_eq!(row_two.len(), 2);
        let mut next = 0usize;
        assert!(terminal_row_ranges_contain(row_two, &mut next, 1));
        assert!(!terminal_row_ranges_contain(row_two, &mut next, 4));
        assert!(terminal_row_ranges_contain(row_two, &mut next, 7));
    }

    /// First cell of `bytes` after parsing them with the real terminal, so SGR sequences such as
    /// 58 (underline colour) are applied.
    fn parsed_cell(bytes: &[u8]) -> Cell {
        let theme = Theme::for_id(ThemeId::Dracula, [0.0; 4]);
        let mut term = Term::new(TermConfig {
            cols: 4,
            rows: 1,
            scrollback_lines: 0,
            identity: TermIdentity {
                name: "rriter",
                version: env!("CARGO_PKG_VERSION"),
            },
            palette: terminal_palette(&theme),
        });
        term.advance(bytes, Instant::now(), &mut NullSink);
        term.row(0).expect("visible row")[0].clone()
    }

    fn assert_rgb_close(actual: [f32; 4], expected: [f32; 3]) {
        for channel in 0..3 {
            assert_eq!(
                (actual[channel] * 255.0).round() as u8,
                (expected[channel] * 255.0).round() as u8,
                "channel {channel}"
            );
        }
    }

    #[test]
    fn decoration_thickness_follows_ui_scale() {
        // Thickness is max(1, round(ui_scale)), the rule of the brief.
        for (ui_scale, expected) in [(1.0_f32, 1.0), (1.333, 1.0), (1.5, 2.0), (1.75, 2.0), (2.0, 2.0)] {
            let (quads, len) = terminal_decoration_quads(
                &decoration(UnderlineStyle::Single, false),
                0.0,
                0,
                8.0,
                28.0,
                40.0,
                9.9,
                ui_scale,
            );
            assert_eq!(len, 1);
            assert_eq!(quads[0].h, expected, "ui_scale {ui_scale}");
        }
    }

    #[test]
    fn decoration_color_after_inverse_and_dim() {
        let theme = Theme::for_id(ThemeId::Dracula, [0.0; 4]);
        let palette = terminal_palette(&theme);
        // INVERSE paints the foreground in the background colour, ANSI 4 here.
        let inverse = parsed_cell(b"\x1b[31;44;7;4mx");
        let deco = terminal_cell_decoration(&inverse, &palette, false).expect("underlined");
        assert_rgb_close(deco.underline_color, [theme.terminal[4][0], theme.terminal[4][1], theme.terminal[4][2]]);
        assert_eq!(deco.strike_color, deco.underline_color);

        // DIM scales the rendered foreground RGB by 0.66 (Ronsole decoration test of the same name).
        let dim = parsed_cell(b"\x1b[2;4;38;2;150;120;90mx");
        let deco = terminal_cell_decoration(&dim, &palette, false).expect("underlined");
        assert_rgb_close(deco.underline_color, [150.0 * 0.66 / 255.0, 120.0 * 0.66 / 255.0, 90.0 * 0.66 / 255.0]);
        assert_eq!(deco.strike_color, deco.underline_color);
    }

    #[test]
    fn underline_color_overrides_fg() {
        let palette = terminal_palette(&Theme::for_id(ThemeId::Dracula, [0.0; 4]));
        let cell = parsed_cell(b"\x1b[4;58;2;10;20;30mx");
        let deco = terminal_cell_decoration(&cell, &palette, false).expect("underlined");
        assert_rgb_close(deco.underline_color, [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]);
        assert_ne!(deco.strike_color, deco.underline_color);
    }

    #[test]
    fn hovered_curly_becomes_single() {
        let palette = terminal_palette(&Theme::for_id(ThemeId::Dracula, [0.0; 4]));
        let curly = Cell::new_for_test(
            'x',
            TerminalColor::default_foreground(),
            TerminalColor::default_background(),
            CellFlags::empty(),
            UnderlineStyle::Curly,
        );
        let plain = terminal_cell_decoration(&curly, &palette, false).expect("curly");
        assert_eq!(plain.underline, UnderlineStyle::Curly);
        let hovered = terminal_cell_decoration(&curly, &palette, true).expect("hovered");
        assert_eq!(hovered.underline, UnderlineStyle::Single);
    }

    #[test]
    fn hovered_link_uses_underline_color_not_fg() {
        let palette = terminal_palette(&Theme::for_id(ThemeId::Dracula, [0.0; 4]));
        let cell = parsed_cell(b"\x1b[58;2;10;20;30mx");
        let deco = terminal_cell_decoration(&cell, &palette, true).expect("hovered");
        assert_eq!(deco.underline, UnderlineStyle::Single);
        assert_rgb_close(deco.underline_color, [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]);
    }

    #[test]
    fn concealed_and_host_hidden_cells_draw_nothing() {
        let palette = terminal_palette(&Theme::for_id(ThemeId::Dracula, [0.0; 4]));
        for flag in [CellFlags::CONCEAL, CellFlags::HOST_HIDDEN] {
            let cell = Cell::new_for_test(
                'x',
                TerminalColor::default_foreground(),
                TerminalColor::default_background(),
                flag,
                UnderlineStyle::Single,
            );
            assert!(terminal_cell_decoration(&cell, &palette, true).is_none());
        }
    }

    #[test]
    fn plain_cell_without_hover_has_no_decoration() {
        let palette = terminal_palette(&Theme::for_id(ThemeId::Dracula, [0.0; 4]));
        let cell = Cell::new_for_test(
            'x',
            TerminalColor::default_foreground(),
            TerminalColor::default_background(),
            CellFlags::empty(),
            UnderlineStyle::None,
        );
        assert!(terminal_cell_decoration(&cell, &palette, false).is_none());
        assert!(terminal_cell_decoration(&cell, &palette, true).is_some());
    }
}
