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

            let idx = (y * width + x) * 4;
            let alpha = (coverage * 255.0).round() as u8;
            if alpha > data[idx + 3] {
                data[idx] = 255;
                data[idx + 1] = 255;
                data[idx + 2] = 255;
                data[idx + 3] = alpha;
            }
        }
    }
}

impl Renderer {
    #[inline(always)]
    fn mono_cell_advance(&self) -> f32 {
        let advance = self.ascii_advances[b'A' as usize];
        if advance > 0.0 {
            advance
        } else {
            self.font_size * 0.6
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn get_custom_braille_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        let mask = braille_dot_mask(c)?;
        let advance = self.mono_cell_advance().round().max(6.0);

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
        let h = (self.font_size * 0.88).round().max(10.0) as i32;

        if self.atlas_x + w + 2 > ATLAS_SIZE_W {
            self.atlas_x = 2;
            self.atlas_y += self.max_row_h + 2;
            self.max_row_h = 0;
        }
        if self.atlas_y + h + 2 > ATLAS_SIZE_H {
            self.glyphs.clear();
            self.ui_glyphs.clear();
            self.atlas_x = 2;
            self.atlas_y = 2;
            self.max_row_h = 0;
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                self.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    ATLAS_SIZE_W,
                    ATLAS_SIZE_H,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
        }

        let width = w as usize;
        let height = h as usize;
        let mut rgba = vec![0u8; width * height * 4];
        let radius = (self.font_size * 0.075).max(1.1);
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
            draw_braille_dot(&mut rgba, width, height, cx, cy, radius);
        }

        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                self.atlas_x,
                self.atlas_y,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&rgba)),
            );
        }

        let info = GlyphInfo {
            u: self.atlas_x as f32 / ATLAS_SIZE_W as f32,
            v: self.atlas_y as f32 / ATLAS_SIZE_H as f32,
            uw: w as f32 / ATLAS_SIZE_W as f32,
            vh: h as f32 / ATLAS_SIZE_H as f32,
            width: w as f32,
            height: h as f32,
            offset_x: 0.0,
            offset_y: h as f32,
            advance,
            is_emoji: 0.0,
        };
        self.atlas_x += w + 2;
        if h > self.max_row_h {
            self.max_row_h = h;
        }
        Some(info)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn get_custom_svg_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        let svg_str = custom_svg_glyph_source(c)?;

        let opt = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_data(svg_str.as_bytes(), &opt).ok()?;

        let target_size = (self.font_size * 1.05).round().max(10.0);
        let w = target_size as i32;
        let h = target_size as i32;

        if self.atlas_x + w + 2 > ATLAS_SIZE_W {
            self.atlas_x = 2;
            self.atlas_y += self.max_row_h + 2;
            self.max_row_h = 0;
        }

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

        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                self.atlas_x,
                self.atlas_y,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(data)),
            );
        }

        let info = GlyphInfo {
            u: self.atlas_x as f32 / ATLAS_SIZE_W as f32,
            v: self.atlas_y as f32 / ATLAS_SIZE_H as f32,
            uw: w as f32 / ATLAS_SIZE_W as f32,
            vh: h as f32 / ATLAS_SIZE_H as f32,
            width: w as f32,
            height: h as f32,
            offset_x: 0.0,
            offset_y: h as f32 * 0.82,
            advance: target_size * 0.85,
            is_emoji: 0.0,
        };

        self.atlas_x += w + 2;
        if h > self.max_row_h {
            self.max_row_h = h;
        }
        Some(info)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn get_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        self.get_glyph_for_color_preference(c, None)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_glyph_for_color_preference(
        &mut self,
        c: char,
        prefer_color: Option<bool>,
    ) -> Option<GlyphInfo> {
        let strict_text = prefer_color == Some(false);
        let cache_presentation = match prefer_color {
            Some(false) => GLYPH_PRESENTATION_TEXT,
            Some(true) => GLYPH_PRESENTATION_EMOJI,
            None => GLYPH_PRESENTATION_AUTO,
        };
        let cache_key = (c, cache_presentation);
        if let Some(g) = self.glyphs.get(&cache_key) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }
        if custom_svg_glyph_source(c).is_some() {
            if let Some(info) = self.get_custom_svg_glyph(c) {
                self.glyphs.insert(cache_key, info);
                return Some(info);
            }
        }

        let mut rendered_image = None;
        let mut glyph_advance = 0.0;
        let prefer_color = prefer_color.unwrap_or_else(|| default_emoji_presentation(c));

        let indices: Vec<usize> = if prefer_color {
            (0..self.fonts.len()).rev().collect()
        } else {
            (0..self.fonts.len()).collect()
        };

        for idx in indices {
            self.fonts[idx].ensure_loaded();
            let font_data = &self.fonts[idx];
            let data = font_data.data_slice();
            if data.is_empty() {
                continue;
            }
            if let Some(font_ref) = FontRef::from_index(data, font_data.index as usize) {
                let glyph_id = font_ref.charmap().map(c);
                if glyph_id != 0 || (c.is_whitespace() && idx == 0) {
                    let head = font_ref.metrics(&[]);
                    glyph_advance = (font_ref.glyph_metrics(&[]).advance_width(glyph_id) as f32
                        * self.font_size)
                        / head.units_per_em as f32;

                    let mut scaler = self
                        .scale_context
                        .builder(font_ref)
                        .size(self.font_size)
                        .hint(true)
                        .build();
                    if let Some(img) = Render::new(&[
                        Source::ColorOutline(0),
                        Source::ColorBitmap(StrikeWith::BestFit),
                        Source::Outline,
                    ])
                    .render(&mut scaler, glyph_id)
                    {
                        if (img.data.len() > 0 || c.is_whitespace())
                            && accept_rendered_glyph_content(prefer_color, img.content)
                        {
                            rendered_image = Some(img);
                            break;
                        }
                    }
                }
            }
        }

        if rendered_image.is_none() {
            if let Some(info) = self.get_custom_braille_glyph(c) {
                self.glyphs.insert(cache_key, info);
                return Some(info);
            }
            if strict_text {
                return None;
            }
            if c != '□' {
                let fallback = self.get_glyph_for_color_preference('□', Some(false));
                if let Some(info) = fallback {
                    self.glyphs.insert(cache_key, info);
                }
                return fallback;
            }
            return None;
        }

        let img = rendered_image.unwrap();
        let w = img.placement.width as i32;
        let h = img.placement.height as i32;

        if c.is_whitespace() || w <= 0 || h <= 0 {
            let info = GlyphInfo {
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
            };
            self.glyphs.insert(cache_key, info);
            return Some(info);
        }

        if self.atlas_x + w + 2 > ATLAS_SIZE_W {
            self.atlas_x = 2;
            self.atlas_y += self.max_row_h + 2;
            self.max_row_h = 0;
        }
        if self.atlas_y + h + 2 > ATLAS_SIZE_H {
            self.glyphs.clear();
            self.ui_glyphs.clear();
            self.atlas_x = 2;
            self.atlas_y = 2;
            self.max_row_h = 0;
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                self.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    ATLAS_SIZE_W,
                    ATLAS_SIZE_H,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
        }

        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let is_color = img.content == Content::Color;
        match img.content {
            Content::Mask => {
                for i in 0..(w * h) as usize {
                    rgba[i * 4] = 255;
                    rgba[i * 4 + 1] = 255;
                    rgba[i * 4 + 2] = 255;
                    rgba[i * 4 + 3] = img.data[i];
                }
            }
            _ => {
                if img.data.len() == rgba.len() {
                    rgba.copy_from_slice(&img.data);
                }
            }
        }

        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                self.atlas_x,
                self.atlas_y,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&rgba)),
            );
        }

        let info = GlyphInfo {
            u: self.atlas_x as f32 / ATLAS_SIZE_W as f32,
            v: self.atlas_y as f32 / ATLAS_SIZE_H as f32,
            uw: w as f32 / ATLAS_SIZE_W as f32,
            vh: h as f32 / ATLAS_SIZE_H as f32,
            width: w as f32,
            height: h as f32,
            offset_x: img.placement.left as f32,
            offset_y: img.placement.top as f32,
            advance: glyph_advance,
            is_emoji: if is_color { 1.0 } else { 0.0 },
        };
        self.glyphs.insert(cache_key, info);
        self.atlas_x += w + 2;
        if h > self.max_row_h {
            self.max_row_h = h;
        }
        Some(info)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_terminal_glyph(
        &mut self,
        c: char,
        prefer_color: Option<bool>,
    ) -> Option<GlyphInfo> {
        let cache_presentation = if terminal_force_text_presentation(c) {
            GLYPH_PRESENTATION_TEXT
        } else {
            match prefer_color {
                Some(false) => GLYPH_PRESENTATION_TEXT,
                Some(true) => GLYPH_PRESENTATION_EMOJI,
                None => GLYPH_PRESENTATION_TERMINAL,
            }
        };
        let cache_key = (c, cache_presentation);
        if let Some(g) = self.glyphs.get(&cache_key) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }

        let glyph = if terminal_force_text_presentation(c) || prefer_color == Some(false) {
            self.get_glyph_for_color_preference(c, Some(false))
                .or_else(|| {
                    if terminal_force_text_presentation(c) {
                        None
                    } else {
                        self.get_glyph_for_color_preference(c, Some(true))
                    }
                })
        } else {
            self.get_glyph_for_color_preference(c, prefer_color)
        }
        .or_else(|| {
                if c != '□' {
                    self.get_glyph_for_color_preference('□', Some(false))
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
        if let Some(g) = self.ui_glyphs.get(&c) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }
        if custom_svg_glyph_source(c).is_some() {
            if let Some(info) = self.get_custom_svg_glyph(c) {
                self.ui_glyphs.insert(c, info);
                return Some(info);
            }
        }

        let mut rendered_image = None;
        let mut glyph_advance = 0.0;
        let prefer_color = default_emoji_presentation(c);
        let indices: Vec<usize> = if prefer_color {
            (0..self.ui_fonts.len()).rev().collect()
        } else {
            (0..self.ui_fonts.len()).collect()
        };

        for idx in indices {
            self.ui_fonts[idx].ensure_loaded();
            let font_data = &self.ui_fonts[idx];
            let data = font_data.data_slice();
            if data.is_empty() {
                continue;
            }
            if let Some(font_ref) = FontRef::from_index(data, font_data.index as usize) {
                let glyph_id = font_ref.charmap().map(c);
                if glyph_id != 0 || (c.is_whitespace() && idx == 0) {
                    let head = font_ref.metrics(&[]);
                    glyph_advance = (font_ref.glyph_metrics(&[]).advance_width(glyph_id) as f32
                        * self.font_size)
                        / head.units_per_em as f32;

                    let mut scaler = self
                        .scale_context
                        .builder(font_ref)
                        .size(self.font_size)
                        .hint(true)
                        .build();
                    if let Some(img) = Render::new(&[
                        Source::ColorOutline(0),
                        Source::ColorBitmap(StrikeWith::BestFit),
                        Source::Outline,
                    ])
                    .render(&mut scaler, glyph_id)
                    {
                        if (img.data.len() > 0 || c.is_whitespace())
                            && accept_rendered_glyph_content(prefer_color, img.content)
                        {
                            rendered_image = Some(img);
                            break;
                        }
                    }
                }
            }
        }

        if rendered_image.is_none() {
            if let Some(info) = self.get_custom_braille_glyph(c) {
                self.ui_glyphs.insert(c, info);
                return Some(info);
            }
            if c != '□' {
                let fallback = self.get_ui_glyph('□');
                if let Some(info) = fallback {
                    self.ui_glyphs.insert(c, info);
                }
                return fallback;
            }
            return None;
        }

        let img = rendered_image.unwrap();
        let w = img.placement.width as i32;
        let h = img.placement.height as i32;

        if c.is_whitespace() || w <= 0 || h <= 0 {
            let info = GlyphInfo {
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
            };
            self.ui_glyphs.insert(c, info);
            return Some(info);
        }

        if self.atlas_x + w + 2 > ATLAS_SIZE_W {
            self.atlas_x = 2;
            self.atlas_y += self.max_row_h + 2;
            self.max_row_h = 0;
        }
        if self.atlas_y + h + 2 > ATLAS_SIZE_H {
            self.glyphs.clear();
            self.ui_glyphs.clear();
            self.atlas_x = 2;
            self.atlas_y = 2;
            self.max_row_h = 0;
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                self.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    ATLAS_SIZE_W,
                    ATLAS_SIZE_H,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
        }

        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let is_color = img.content == Content::Color;
        match img.content {
            Content::Mask => {
                for i in 0..(w * h) as usize {
                    rgba[i * 4] = 255;
                    rgba[i * 4 + 1] = 255;
                    rgba[i * 4 + 2] = 255;
                    rgba[i * 4 + 3] = img.data[i];
                }
            }
            _ => {
                if img.data.len() == rgba.len() {
                    rgba.copy_from_slice(&img.data);
                }
            }
        }

        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                self.atlas_x,
                self.atlas_y,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&rgba)),
            );
        }

        let info = GlyphInfo {
            u: self.atlas_x as f32 / ATLAS_SIZE_W as f32,
            v: self.atlas_y as f32 / ATLAS_SIZE_H as f32,
            uw: w as f32 / ATLAS_SIZE_W as f32,
            vh: h as f32 / ATLAS_SIZE_H as f32,
            width: w as f32,
            height: h as f32,
            offset_x: img.placement.left as f32,
            offset_y: img.placement.top as f32 - self.scale_factor.round().max(1.0),
            advance: glyph_advance,
            is_emoji: if is_color { 1.0 } else { 0.0 },
        };
        self.ui_glyphs.insert(c, info);
        self.atlas_x += w + 2;
        if h > self.max_row_h {
            self.max_row_h = h;
        }
        Some(info)
    }

}
