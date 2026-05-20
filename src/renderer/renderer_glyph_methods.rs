impl Renderer {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn get_custom_svg_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        let svg_str = match c {
            '▶' => {
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\"><path fill=\"#ffffff\" d=\"M8 5.14v14l11-7z\"/></svg>"
            }
            '▼' => {
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\"><path fill=\"#ffffff\" d=\"M5.14 8h14l-7 11z\"/></svg>"
            }
            _ => return None,
        };

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
        if let Some(g) = self.glyphs.get(&c) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }
        if c == '▶' || c == '▼' {
            if let Some(info) = self.get_custom_svg_glyph(c) {
                self.glyphs.insert(c, info);
                return Some(info);
            }
        }

        let mut rendered_image = None;
        let mut glyph_advance = 0.0;
        let is_emoji_char = (c as u32) > 0x2400;

        let indices: Vec<usize> = if is_emoji_char {
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
                        if img.data.len() > 0 || c.is_whitespace() {
                            rendered_image = Some(img);
                            break;
                        }
                    }
                }
            }
        }

        if rendered_image.is_none() {
            if c != '□' {
                let fallback = self.get_glyph('□');
                if let Some(info) = fallback {
                    self.glyphs.insert(c, info);
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
            self.glyphs.insert(c, info);
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
        self.glyphs.insert(c, info);
        self.atlas_x += w + 2;
        if h > self.max_row_h {
            self.max_row_h = h;
        }
        Some(info)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn get_ui_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        if let Some(g) = self.ui_glyphs.get(&c) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
        }
        if c == '▶' || c == '▼' {
            if let Some(info) = self.get_custom_svg_glyph(c) {
                self.ui_glyphs.insert(c, info);
                return Some(info);
            }
        }

        let mut rendered_image = None;
        let mut glyph_advance = 0.0;
        let is_emoji_char = (c as u32) > 0x2400;
        let indices: Vec<usize> = if is_emoji_char {
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
                        if img.data.len() > 0 || c.is_whitespace() {
                            rendered_image = Some(img);
                            break;
                        }
                    }
                }
            }
        }

        if rendered_image.is_none() {
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
