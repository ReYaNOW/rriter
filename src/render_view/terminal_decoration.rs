//! Geometry of terminal text decorations: SGR underline styles, strikethrough and the
//! hovered-link underline. Everything here is pure; the renderer turns the output into quads.

use ronsole_term::{CellFlags, LinkTarget, UnderlineStyle};
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

/// Face under which a glyph is rasterized and cached. Colour glyphs and characters the styled
/// face does not map are drawn by the Regular chain alone, so they share the Regular entry.
#[inline]
pub(crate) fn terminal_glyph_raster_face(face: u8, prefer_color: bool, face_maps_char: bool) -> u8 {
    if face == TERMINAL_FACE_REGULAR || prefer_color || !face_maps_char {
        TERMINAL_FACE_REGULAR
    } else {
        face
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
/// the row. `x_height` is the Regular face x-height in pixels, `scale` the text scale factor.
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
    scale: f32,
) -> ([DecorationQuad; DECORATION_MAX_QUADS], usize) {
    let thickness = scale.round().max(1.0);
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn bold_glyph_falls_back_to_regular() {
        // A styled face that does not map the character resolves to the Regular chain.
        assert_eq!(terminal_glyph_raster_face(TERMINAL_FACE_BOLD, false, false), TERMINAL_FACE_REGULAR);
        assert_eq!(terminal_glyph_raster_face(TERMINAL_FACE_BOLD, false, true), TERMINAL_FACE_BOLD);
    }

    #[test]
    fn color_and_unmapped_glyphs_share_regular_entry() {
        assert_eq!(terminal_glyph_raster_face(TERMINAL_FACE_ITALIC, true, true), TERMINAL_FACE_REGULAR);
        assert_eq!(terminal_glyph_raster_face(TERMINAL_FACE_ITALIC, false, false), TERMINAL_FACE_REGULAR);
        assert_eq!(terminal_glyph_raster_face(TERMINAL_FACE_REGULAR, true, true), TERMINAL_FACE_REGULAR);
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

    #[test]
    fn decoration_color_after_inverse_and_dim() {
        // The colour is whatever the caller resolved (underline colour or foreground after inverse
        // and dim); the geometry must carry it unchanged.
        let color = [0.2, 0.4, 0.6, 1.0];
        let deco = CellDecoration {
            underline: UnderlineStyle::Single,
            underline_color: color,
            strike: false,
            strike_color: WHITE,
        };
        let (quads, len) = terminal_decoration_quads(&deco, 0.0, 0, 8.0, 28.0, 40.0, 9.9, 1.0);
        assert_eq!(len, 1);
        assert_eq!(quads[0].color, color);
    }
}
