#[inline(always)]
pub(crate) fn default_emoji_presentation(c: char) -> bool {
    matches!(
        c as u32,
            | 0x231A..=0x231B
            | 0x23E9..=0x23F3
            | 0x25FB..=0x25FE
            | 0x2614..=0x2615
            | 0x2648..=0x2653
            | 0x267F
            | 0x2693
            | 0x26A1
            | 0x26AA..=0x26AB
            | 0x26BD..=0x26BE
            | 0x26C4..=0x26C5
            | 0x26CE
            | 0x26D4
            | 0x26EA
            | 0x26F2..=0x26F3
            | 0x26F5
            | 0x26FA
            | 0x26FD
            | 0x2705
            | 0x270A..=0x270B
            | 0x2728
            | 0x274C
            | 0x274E
            | 0x2753..=0x2755
            | 0x2757
            | 0x2795..=0x2797
            | 0x27B0
            | 0x27BF
            | 0x2B07
            | 0x2B1B..=0x2B1C
            | 0x2B50
            | 0x2B55
            | 0x1F000..=0x1FAFF
            | 0x1FC00..=0x1FFFF
    )
}

#[inline(always)]
fn accept_rendered_glyph_content(prefer_color: bool, content: Content) -> bool {
    content != Content::Color || prefer_color
}

fn custom_svg_glyph_source(c: char) -> Option<&'static str> {
    match c {
        '▶' => Some(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\"><path fill=\"#ffffff\" d=\"M8 5.14v14l11-7z\"/></svg>",
        ),
        '▼' => Some(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\"><path fill=\"#ffffff\" d=\"M5.14 8h14l-7 11z\"/></svg>",
        ),
        _ => None,
    }
}

#[inline(always)]
pub(crate) fn terminal_force_text_presentation(c: char) -> bool {
    matches!(c, '✔' | '✓')
}

const GLYPH_PRESENTATION_AUTO: u8 = 0;
const GLYPH_PRESENTATION_TEXT: u8 = 1;
const GLYPH_PRESENTATION_EMOJI: u8 = 2;
const GLYPH_PRESENTATION_TERMINAL: u8 = 3;

#[inline(always)]
pub(crate) fn final_text_pixel_size(font_size: f32, scale: f32) -> f32 {
    (font_size * scale).round().max(1.0)
}

#[inline(always)]
fn glyph_size_key(pixel_size: f32) -> u32 {
    pixel_size.to_bits()
}

/// Bit offset of the terminal face inside the presentation byte of a glyph key. The
/// presentation values use the two low bits, the face the two bits above them.
const TERMINAL_FACE_KEY_SHIFT: u8 = 2;

#[inline(always)]
fn terminal_cache_presentation(c: char, prefer_color: Option<bool>) -> u8 {
    if terminal_force_text_presentation(c) {
        return GLYPH_PRESENTATION_TEXT;
    }
    match prefer_color {
        Some(false) => GLYPH_PRESENTATION_TEXT,
        Some(true) => GLYPH_PRESENTATION_EMOJI,
        None => GLYPH_PRESENTATION_TERMINAL,
    }
}

/// Whether a styled terminal face may draw `c`. Colour glyphs and the custom-drawn glyphs stay on
/// the Regular chain, which owns their rasterization.
#[inline(always)]
fn terminal_styled_face_applies(c: char, prefer_color: Option<bool>) -> bool {
    let colour = !terminal_force_text_presentation(c)
        && prefer_color.unwrap_or_else(|| default_emoji_presentation(c));
    !colour && custom_svg_glyph_source(c).is_none() && braille_dot_mask(c).is_none()
}

/// x-height of a bundled terminal face in em units, from the font metrics (`OS/2` table).
fn terminal_regular_x_height_em(font_data: &'static [u8]) -> f32 {
    FontRef::from_index(font_data, 0)
        .map(|font| {
            let metrics = font.metrics(&[]);
            metrics.x_height / f32::from(metrics.units_per_em.max(1))
        })
        .unwrap_or(0.0)
}

fn rasterize_font_data_glyph(
    font_data: &mut FontData,
    scale_context: &mut ScaleContext,
    c: char,
    pixel_size: f32,
    prefer_color: bool,
    allow_missing_whitespace: bool,
) -> Option<(swash::scale::image::Image, f32)> {
    let font_ref = font_data.font_ref()?;
    let glyph_id = font_ref.charmap().map(c);
    if glyph_id == 0 && !allow_missing_whitespace {
        return None;
    }

    let head = font_ref.metrics(&[]);
    let glyph_advance =
        (font_ref.glyph_metrics(&[]).advance_width(glyph_id) as f32 * pixel_size)
            / head.units_per_em as f32;
    let mut scaler = scale_context
        .builder(font_ref)
        .size(pixel_size)
        .hint(true)
        .build();
    let image = Render::new(&[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ])
    .render(&mut scaler, glyph_id)?;
    ((image.data.len() > 0 || c.is_whitespace())
        && accept_rendered_glyph_content(prefer_color, image.content))
    .then_some((image, glyph_advance))
}

fn rasterize_font_glyph(
    fonts: &mut [FontData],
    scale_context: &mut ScaleContext,
    c: char,
    pixel_size: f32,
    prefer_color: bool,
) -> Option<(swash::scale::image::Image, f32)> {
    if prefer_color {
        for (idx, font_data) in fonts.iter_mut().enumerate().rev() {
            if let Some(rendered) = rasterize_font_data_glyph(
                font_data,
                scale_context,
                c,
                pixel_size,
                true,
                c.is_whitespace() && idx == 0,
            ) {
                return Some(rendered);
            }
        }
    } else {
        for (idx, font_data) in fonts.iter_mut().enumerate() {
            if let Some(rendered) = rasterize_font_data_glyph(
                font_data,
                scale_context,
                c,
                pixel_size,
                false,
                c.is_whitespace() && idx == 0,
            ) {
                return Some(rendered);
            }
        }
    }
    None
}

#[inline(always)]
fn braille_dot_mask(c: char) -> Option<u8> {
    let u = c as u32;
    if (0x2800..=0x28FF).contains(&u) {
        Some((u - 0x2800) as u8)
    } else {
        None
    }
}

#[inline(always)]
fn braille_dot_pos(bit: u8) -> (usize, usize) {
    match bit {
        0 => (0, 0),
        1 => (0, 1),
        2 => (0, 2),
        3 => (1, 0),
        4 => (1, 1),
        5 => (1, 2),
        6 => (0, 3),
        _ => (1, 3),
    }
}

fn draw_braille_dot(data: &mut [u8], width: usize, height: usize, cx: f32, cy: f32, radius: f32) {
    let min_x = (cx - radius - 1.0).floor().max(0.0) as usize;
    let max_x = (cx + radius + 1.0).ceil().min(width as f32) as usize;
    let min_y = (cy - radius - 1.0).floor().max(0.0) as usize;
    let max_y = (cy + radius + 1.0).ceil().min(height as f32) as usize;

    for y in min_y..max_y {
        for x in min_x..max_x {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let coverage = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }

            let idx = y * width + x;
            let alpha = (coverage * 255.0).round() as u8;
            if alpha > data[idx] {
                data[idx] = alpha;
            }
        }
    }
}

impl Renderer {
    #[inline(always)]
    pub(crate) fn final_text_pixel_size(&self, scale: f32) -> f32 {
        final_text_pixel_size(self.font_size, scale)
    }

    #[inline(always)]
    fn mono_cell_advance(&self) -> f32 {
        let advance = self.ascii_advances[b'A' as usize];
        if advance > 0.0 {
            advance
        } else {
            self.font_size * 0.6
        }
    }

    #[inline(always)]
    fn mono_cell_advance_at_size(&self, pixel_size: f32) -> f32 {
        if self.font_size > 0.0 {
            self.mono_cell_advance() * pixel_size / self.font_size
        } else {
            pixel_size * 0.6
        }
    }

    fn upload_rasterized_glyph(
        &mut self,
        image: swash::scale::image::Image,
        glyph_advance: f32,
        offset_y_adjust: f32,
        force_empty_bitmap: bool,
    ) -> Option<GlyphInfo> {
        let w = image.placement.width as i32;
        let h = image.placement.height as i32;
        if force_empty_bitmap || w <= 0 || h <= 0 {
            return Some(GlyphInfo {
                u: 0.0,
                v: 0.0,
                uw: 0.0,
                vh: 0.0,
                width: 0.0,
                height: 0.0,
                offset_x: 0.0,
                offset_y: 0.0,
                advance: glyph_advance,
                is_emoji: 0.0,
            });
        }

        let is_color = image.content == Content::Color;
        let entry = if is_color {
            self.upload_color_rgba(w, h, &image.data)?
        } else {
            self.upload_alpha_atlas(w, h, &image.data)?
        };
        Some(GlyphInfo {
            u: entry.u,
            v: entry.v,
            uw: entry.uw,
            vh: entry.vh,
            width: w as f32,
            height: h as f32,
            offset_x: image.placement.left as f32,
            offset_y: image.placement.top as f32 - offset_y_adjust,
            advance: glyph_advance,
            is_emoji: if is_color { COLOR_ATLAS_MODE } else { 0.0 },
        })
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn get_custom_braille_glyph_at_size(
        &mut self,
        c: char,
        pixel_size: f32,
    ) -> Option<GlyphInfo> {
        let mask = braille_dot_mask(c)?;
        let advance = self.mono_cell_advance_at_size(pixel_size).round().max(6.0);

        if mask == 0 {
            return Some(GlyphInfo {
                u: 0.0,
                v: 0.0,
                uw: 0.0,
                vh: 0.0,
                width: 0.0,
                height: 0.0,
                offset_x: 0.0,
                offset_y: 0.0,
                advance,
                is_emoji: 0.0,
            });
        }

        let w = advance as i32;
        let h = (pixel_size * 0.88).round().max(10.0) as i32;

        let width = w as usize;
        let height = h as usize;
        let mut alpha = vec![0u8; width * height];
        let radius = (pixel_size * 0.075).max(1.1);
        let col_gap = (w as f32 * 0.34).max(radius * 2.35);
        let x0 = w as f32 * 0.5 - col_gap * 0.5;
        let x1 = w as f32 * 0.5 + col_gap * 0.5;
        let top = (h as f32 * 0.15).max(radius + 0.5);
        let bottom = h as f32 - top;
        let row_gap = ((bottom - top) / 3.0).max(radius * 2.2);

        for bit in 0..8 {
            if mask & (1 << bit) == 0 {
                continue;
            }
            let (col, row) = braille_dot_pos(bit);
            let cx = if col == 0 { x0 } else { x1 };
            let cy = top + row as f32 * row_gap;
            draw_braille_dot(&mut alpha, width, height, cx, cy, radius);
        }

        let entry = self.upload_alpha_atlas(w, h, &alpha)?;

        let info = GlyphInfo {
            u: entry.u,
            v: entry.v,
            uw: entry.uw,
            vh: entry.vh,
            width: w as f32,
            height: h as f32,
            offset_x: 0.0,
            offset_y: h as f32,
            advance,
            is_emoji: 0.0,
        };
        Some(info)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn get_custom_svg_glyph_at_size(&mut self, c: char, pixel_size: f32) -> Option<GlyphInfo> {
        let svg_str = custom_svg_glyph_source(c)?;

        let opt = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_data(svg_str.as_bytes(), &opt).ok()?;

        let target_size = (pixel_size * 1.05).round().max(10.0);
        let w = target_size as i32;
        let h = target_size as i32;

        let mut pixmap = tiny_skia::Pixmap::new(w as u32, h as u32)?;
        let scale = target_size / 24.0;
        let transform = tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let data = pixmap.data_mut();
        center_alpha_bbox_y(data, w as usize, h as usize);
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = 255;
            pixel[1] = 255;
            pixel[2] = 255;
        }

        let alpha = Self::rgba_alpha_bytes(data);
        let entry = self.upload_alpha_atlas(w, h, &alpha)?;

        let info = GlyphInfo {
            u: entry.u,
            v: entry.v,
            uw: entry.uw,
            vh: entry.vh,
            width: w as f32,
            height: h as f32,
            offset_x: 0.0,
            offset_y: h as f32 * 0.82,
            advance: target_size * 0.85,
            is_emoji: 0.0,
        };

        Some(info)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn get_custom_svg_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        self.get_custom_svg_glyph_at_size(c, self.font_size)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn get_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        self.get_glyph_for_color_preference(c, None)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_glyph_at_size(&mut self, c: char, pixel_size: f32) -> Option<GlyphInfo> {
        self.get_glyph_for_color_preference_at_size(c, None, pixel_size)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_glyph_for_color_preference(
        &mut self,
        c: char,
        prefer_color: Option<bool>,
    ) -> Option<GlyphInfo> {
        self.get_glyph_for_color_preference_at_size(c, prefer_color, self.font_size)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn get_glyph_for_color_preference_at_size(
        &mut self,
        c: char,
        prefer_color: Option<bool>,
        pixel_size: f32,
    ) -> Option<GlyphInfo> {
        let pixel_size = if pixel_size.is_finite() && pixel_size > 0.0 {
            pixel_size
        } else {
            self.font_size.max(1.0)
        };
        let strict_text = prefer_color == Some(false);
        let cache_presentation = match prefer_color {
            Some(false) => GLYPH_PRESENTATION_TEXT,
            Some(true) => GLYPH_PRESENTATION_EMOJI,
            None => GLYPH_PRESENTATION_AUTO,
        };
        let cache_key = (c, cache_presentation, glyph_size_key(pixel_size));
        if let Some(g) = self.glyphs.get(&cache_key) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }
        if custom_svg_glyph_source(c).is_some()
            && let Some(info) = self.get_custom_svg_glyph_at_size(c, pixel_size)
        {
            self.glyphs.insert(cache_key, info);
            return Some(info);
        }

        let use_color = prefer_color.unwrap_or_else(|| default_emoji_presentation(c));
        let rendered = rasterize_font_glyph(
            &mut self.fonts,
            &mut self.scale_context,
            c,
            pixel_size,
            use_color,
        );
        let Some((image, glyph_advance)) = rendered else {
            if let Some(info) = self.get_custom_braille_glyph_at_size(c, pixel_size) {
                self.glyphs.insert(cache_key, info);
                return Some(info);
            }
            if strict_text {
                return None;
            }
            if c != '□' {
                let fallback =
                    self.get_glyph_for_color_preference_at_size('□', Some(false), pixel_size);
                if let Some(info) = fallback {
                    self.glyphs.insert(cache_key, info);
                }
                return fallback;
            }
            return None;
        };

        let info =
            self.upload_rasterized_glyph(image, glyph_advance, 0.0, c.is_whitespace())?;
        self.glyphs.insert(cache_key, info);
        Some(info)
    }

    pub(crate) fn char_advance_at_size(&mut self, c: char, pixel_size: f32) -> f32 {
        if c == '\n' {
            return 0.0;
        }
        if c == '\t' {
            return self
                .get_glyph_at_size(' ', pixel_size)
                .map_or(pixel_size * 2.4, |glyph| glyph.advance * 4.0);
        }
        self.get_glyph_at_size(c, pixel_size)
            .map_or(pixel_size * 0.6, |glyph| glyph.advance)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_terminal_glyph(
        &mut self,
        c: char,
        prefer_color: Option<bool>,
        face: u8,
    ) -> Option<GlyphInfo> {
        self.get_terminal_glyph_at_size(c, prefer_color, self.font_size, face)
    }

    /// x-height of the Regular terminal face at `pixel_size`, used to place strikethrough.
    #[inline(always)]
    pub(crate) fn terminal_regular_x_height_at_size(&self, pixel_size: f32) -> f32 {
        self.terminal_x_height_em * pixel_size
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    /// Glyph of a terminal cell in styled face `face`. A character the styled face maps is rasterized
    /// from that face; colour glyphs and unmapped characters take the Regular chain, and their Regular
    /// entry is aliased under the face key so both keys share one atlas slot.
    pub(crate) fn get_terminal_glyph_at_size(
        &mut self,
        c: char,
        prefer_color: Option<bool>,
        pixel_size: f32,
        face: u8,
    ) -> Option<GlyphInfo> {
        if face == TERMINAL_FACE_REGULAR {
            return self.get_terminal_regular_glyph_at_size(c, prefer_color, pixel_size);
        }
        let presentation = terminal_cache_presentation(c, prefer_color) | (face << TERMINAL_FACE_KEY_SHIFT);
        let face_key = (c, presentation, glyph_size_key(pixel_size));
        if let Some(g) = self.glyphs.get(&face_key) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }
        if terminal_styled_face_applies(c, prefer_color)
            && let Some(info) = self.get_terminal_styled_glyph(c, face, pixel_size)
        {
            self.glyphs.insert(face_key, info);
            return Some(info);
        }
        let info = self.get_terminal_regular_glyph_at_size(c, prefer_color, pixel_size)?;
        self.glyphs.insert(face_key, info);
        Some(info)
    }

    /// Glyph of `c` from the bundled font of styled `face`; `None` when that face does not map `c`.
    fn get_terminal_styled_glyph(&mut self, c: char, face: u8, pixel_size: f32) -> Option<GlyphInfo> {
        let font = self
            .terminal_styled_fonts
            .get_mut(usize::from(face).checked_sub(1)?)?;
        let (image, advance) = rasterize_font_data_glyph(
            font,
            &mut self.scale_context,
            c,
            pixel_size,
            false,
            false,
        )?;
        self.upload_rasterized_glyph(image, advance, 0.0, c.is_whitespace())
    }

    /// Regular-chain glyph of a terminal cell, cached under its Regular key.
    fn get_terminal_regular_glyph_at_size(
        &mut self,
        c: char,
        prefer_color: Option<bool>,
        pixel_size: f32,
    ) -> Option<GlyphInfo> {
        let cache_presentation = terminal_cache_presentation(c, prefer_color);
        let cache_key = (c, cache_presentation, glyph_size_key(pixel_size));
        if let Some(g) = self.glyphs.get(&cache_key) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }

        let glyph = if terminal_force_text_presentation(c) || prefer_color == Some(false) {
            self.get_glyph_for_color_preference_at_size(c, Some(false), pixel_size)
                .or_else(|| {
                    if terminal_force_text_presentation(c) {
                        None
                    } else {
                        self.get_glyph_for_color_preference_at_size(c, Some(true), pixel_size)
                    }
                })
        } else {
            self.get_glyph_for_color_preference_at_size(c, prefer_color, pixel_size)
        }
        .or_else(|| {
                if c != '□' {
                    self.get_glyph_for_color_preference_at_size('□', Some(false), pixel_size)
                } else {
                    None
                }
            });
        if let Some(info) = glyph {
            self.glyphs.insert(cache_key, info);
        }
        glyph
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn get_ui_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        self.get_ui_glyph_at_size(c, self.font_size)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_ui_glyph_at_size(
        &mut self,
        c: char,
        pixel_size: f32,
    ) -> Option<GlyphInfo> {
        let pixel_size = if pixel_size.is_finite() && pixel_size > 0.0 {
            pixel_size
        } else {
            self.font_size.max(1.0)
        };
        let cache_key = (c, glyph_size_key(pixel_size));
        if let Some(g) = self.ui_glyphs.get(&cache_key) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }
        if custom_svg_glyph_source(c).is_some()
            && let Some(info) = self.get_custom_svg_glyph_at_size(c, pixel_size)
        {
            self.ui_glyphs.insert(cache_key, info);
            return Some(info);
        }

        let prefer_color = default_emoji_presentation(c);
        let rendered = rasterize_font_glyph(
            &mut self.ui_fonts,
            &mut self.scale_context,
            c,
            pixel_size,
            prefer_color,
        );
        let Some((image, glyph_advance)) = rendered else {
            if let Some(info) = self.get_custom_braille_glyph_at_size(c, pixel_size) {
                self.ui_glyphs.insert(cache_key, info);
                return Some(info);
            }
            if c != '□' {
                let fallback = self.get_ui_glyph_at_size('□', pixel_size);
                if let Some(info) = fallback {
                    self.ui_glyphs.insert(cache_key, info);
                }
                return fallback;
            }
            return None;
        };

        let ratio = if self.font_size > 0.0 {
            pixel_size / self.font_size
        } else {
            1.0
        };
        let baseline_nudge = (self.scale_factor * ratio).round().max(1.0);
        let info = self.upload_rasterized_glyph(
            image,
            glyph_advance,
            baseline_nudge,
            c.is_whitespace(),
        )?;
        self.ui_glyphs.insert(cache_key, info);
        Some(info)
    }

}

#[cfg(all(test, target_os = "linux"))]
mod terminal_face_tests {
    use super::*;
    use crate::platform::offscreen_gl::OffscreenContext;
    use crate::render_view::terminal_decoration::{TERMINAL_FACE_BOLD, TERMINAL_FACE_REGULAR};

    const TEST_PIXEL_SIZE: f32 = 16.0;
    const SYSTEM_MONO: &str = "/usr/share/fonts/noto/NotoSansMono-Regular.ttf";
    /// CJK ideograph that the bundled JetBrains Mono faces do not map.
    const UNMAPPED: char = '中';
    /// Colour emoji: drawn by the Regular chain only.
    const EMOJI: char = '😀';

    fn test_renderer() -> (OffscreenContext, Renderer) {
        let context = OffscreenContext::new(64, 64).expect("offscreen EGL context");
        let app = crate::app::reviewer_stage2_test_app().expect("headless App");
        let renderer = Renderer::new(
            context.glow(),
            1.0,
            app.theme.clone(),
            context.requested_context(),
            &mut crate::startup_trace::StartupTrace::disabled(),
        )
        .expect("production Renderer");
        (context, renderer)
    }

    #[test]
    fn terminal_regular_face_is_editor_chain() {
        if !std::path::Path::new(SYSTEM_MONO).exists() {
            return;
        }
        let (_context, mut renderer) = test_renderer();
        assert!(!matches!(renderer.fonts[0].source, FontSource::Static(_)));
        let (first_image, first_advance) = rasterize_font_data_glyph(
            &mut renderer.fonts[0],
            &mut renderer.scale_context,
            'a',
            TEST_PIXEL_SIZE,
            false,
            false,
        )
        .expect("system monospace maps 'a'");
        let (chain_image, chain_advance) = rasterize_font_glyph(
            &mut renderer.fonts,
            &mut renderer.scale_context,
            'a',
            TEST_PIXEL_SIZE,
            false,
        )
        .expect("editor chain maps 'a'");
        assert_eq!(chain_image.data, first_image.data);
        assert_eq!(chain_advance, first_advance);
        let glyph = renderer
            .get_terminal_glyph_at_size('a', None, TEST_PIXEL_SIZE, TERMINAL_FACE_REGULAR)
            .expect("terminal glyph for 'a'");
        assert_eq!(glyph.advance, first_advance);
    }

    #[test]
    fn bold_glyph_falls_back_to_regular() {
        let (_context, mut renderer) = test_renderer();
        let bold_maps = renderer.terminal_styled_fonts[0]
            .font_ref()
            .expect("bundled Bold parses")
            .charmap()
            .map(UNMAPPED);
        assert_eq!(bold_maps, 0, "precondition: Bold does not map the test char");
        let styled = renderer
            .get_terminal_glyph_at_size(UNMAPPED, None, TEST_PIXEL_SIZE, TERMINAL_FACE_BOLD)
            .expect("fallback glyph");
        let regular = renderer
            .get_terminal_glyph_at_size(UNMAPPED, None, TEST_PIXEL_SIZE, TERMINAL_FACE_REGULAR)
            .expect("regular glyph");
        assert_eq!((styled.u, styled.v, styled.advance), (regular.u, regular.v, regular.advance));
    }

    #[test]
    fn bold_glyph_uses_bundled_bold_face() {
        let (_context, mut renderer) = test_renderer();
        let regular = renderer
            .get_terminal_glyph_at_size('a', None, TEST_PIXEL_SIZE, TERMINAL_FACE_REGULAR)
            .expect("regular 'a'");
        let bold = renderer
            .get_terminal_glyph_at_size('a', None, TEST_PIXEL_SIZE, TERMINAL_FACE_BOLD)
            .expect("bold 'a'");
        assert!(bold.u != regular.u || bold.v != regular.v, "bold 'a' gets its own atlas slot");
    }

    #[test]
    fn color_and_unmapped_glyphs_share_regular_entry() {
        let (_context, mut renderer) = test_renderer();
        let size_key = glyph_size_key(TEST_PIXEL_SIZE);
        let bold_presentation =
            GLYPH_PRESENTATION_TERMINAL | (TERMINAL_FACE_BOLD << TERMINAL_FACE_KEY_SHIFT);
        for c in [UNMAPPED, EMOJI] {
            let styled = renderer
                .get_terminal_glyph_at_size(c, None, TEST_PIXEL_SIZE, TERMINAL_FACE_BOLD)
                .expect("glyph under Bold");
            let regular = renderer
                .get_terminal_glyph_at_size(c, None, TEST_PIXEL_SIZE, TERMINAL_FACE_REGULAR)
                .expect("glyph under Regular");
            assert_eq!((styled.u, styled.v, styled.uw, styled.vh), (regular.u, regular.v, regular.uw, regular.vh));
            assert!(renderer.glyphs.contains_key(&(c, bold_presentation, size_key)));
        }
    }

    #[test]
    fn terminal_x_height_is_read_from_bundled_regular() {
        let (_context, renderer) = test_renderer();
        let x_height = renderer.terminal_regular_x_height_at_size(TEST_PIXEL_SIZE);
        assert!(x_height > 0.0 && x_height < TEST_PIXEL_SIZE);
    }

    #[test]
    fn terminal_styled_face_applies_only_to_plain_text_glyphs() {
        // Plain text may use the styled face; colour glyphs and custom-drawn glyphs stay on
        // the Regular chain. Characters the styled face does not map fall back at lookup time.
        assert!(super::terminal_styled_face_applies('a', None));
        assert!(!super::terminal_styled_face_applies('a', Some(true)));
        assert!(!super::terminal_styled_face_applies('😀', None));
        assert!(!super::terminal_styled_face_applies('▶', None));
        assert!(!super::terminal_styled_face_applies('\u{2801}', None));
    }
}
