impl Renderer {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn resize(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.width = w as f32;
            self.height = h as f32;
            unsafe {
                self.gl.viewport(0, 0, w as i32, h as i32);
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn measure_ui_width(&mut self, text: &str, scale: f32) -> f32 {
        let mut w = 0.0;
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let px = (g.advance * scale).round();
                w += if px <= 0.0 && g.advance > 0.0 { 1.0 } else { px };
            }
        }
        w
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn char_advance(&mut self, c: char) -> f32 {
        if c == '\n' {
            return 0.0;
        }
        if c == '\t' {
            return self.ascii_advances[b' ' as usize] * 4.0;
        }
        let u = c as u32;
        if u < 128 {
            let adv = self.ascii_advances[u as usize];
            if adv > 0.0 {
                return adv;
            }
        }
        self.get_glyph(c).map(|g| g.advance).unwrap_or(10.0)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn push_quad(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u: f32,
        v: f32,
        uw: f32,
        vh: f32,
        color: [f32; 4],
        mode: f32,
    ) {
        self.vertices
            .extend_from_slice(&quad_vertices(x, y, w, h, u, v, uw, vh, color, mode));
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn push_quad_subpixel_y(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u: f32,
        v: f32,
        uw: f32,
        vh: f32,
        color: [f32; 4],
        mode: f32,
    ) {
        let x1 = x.round();
        let y1 = y;
        let x2 = (x + w).round();
        let y2 = y + h;
        let sdf_params = [0.0, 0.0, 0.0];

        self.vertices.extend_from_slice(&[
            Vertex {
                pos: [x1, y1],
                uv: [u, v],
                color,
                mode,
                sdf_params,
            },
            Vertex {
                pos: [x2, y1],
                uv: [u + uw, v],
                color,
                mode,
                sdf_params,
            },
            Vertex {
                pos: [x2, y2],
                uv: [u + uw, v + vh],
                color,
                mode,
                sdf_params,
            },
            Vertex {
                pos: [x1, y1],
                uv: [u, v],
                color,
                mode,
                sdf_params,
            },
            Vertex {
                pos: [x2, y2],
                uv: [u + uw, v + vh],
                color,
                mode,
                sdf_params,
            },
            Vertex {
                pos: [x1, y2],
                uv: [u, v + vh],
                color,
                mode,
                sdf_params,
            },
        ]);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn builtin_icon_svg(icon_type: crate::widgets::IconType) -> Option<&'static [u8]> {
        let data = match icon_type {
            crate::widgets::IconType::Save => include_bytes!("../icons/document-save.svg").as_slice(),
            crate::widgets::IconType::Discard => include_bytes!("../icons/edit-delete.svg").as_slice(),
            crate::widgets::IconType::Cancel => include_bytes!("../icons/dialog-cancel.svg").as_slice(),
            crate::widgets::IconType::Warning => include_bytes!("../icons/dialog-warning.svg").as_slice(),
            crate::widgets::IconType::Error => include_bytes!("../icons/circle-x.svg").as_slice(),
            crate::widgets::IconType::CaseMatch => include_bytes!("../icons/format-text-uppercase.svg").as_slice(),
            crate::widgets::IconType::Up => include_bytes!("../icons/go-up.svg").as_slice(),
            crate::widgets::IconType::Down => include_bytes!("../icons/go-down.svg").as_slice(),
            crate::widgets::IconType::Close => include_bytes!("../icons/window-close.svg").as_slice(),
            crate::widgets::IconType::Plus => include_bytes!("../icons/plus.svg").as_slice(),
            crate::widgets::IconType::GitPlus => include_bytes!("../icons/plus_git.svg").as_slice(),
            crate::widgets::IconType::GitMinus => include_bytes!("../icons/minus.svg").as_slice(),
            crate::widgets::IconType::Terminal => include_bytes!("../icons/atom/icons/ui/terminal.svg").as_slice(),
            crate::widgets::IconType::Explorer => include_bytes!("../icons/atom/icons/ui/files.svg").as_slice(),
            crate::widgets::IconType::Search => include_bytes!("../icons/atom/icons/ui/search.svg").as_slice(),
            crate::widgets::IconType::Git => include_bytes!("../icons/atom/icons/files/git.svg").as_slice(),
            crate::widgets::IconType::GitCompare => include_bytes!("../icons/atom/icons/ui/git-compare.svg").as_slice(),
            crate::widgets::IconType::Branch => include_bytes!("../icons/branch.svg").as_slice(),
            crate::widgets::IconType::Problems => include_bytes!("../icons/problems.svg").as_slice(),
            crate::widgets::IconType::LspServers => include_bytes!("../icons/atom/icons/ui/server.svg").as_slice(),
            crate::widgets::IconType::Api => include_bytes!("../icons/api.svg").as_slice(),
            crate::widgets::IconType::Copy => include_bytes!("../icons/copy.svg").as_slice(),
            crate::widgets::IconType::Check => include_bytes!("../icons/check.svg").as_slice(),
            crate::widgets::IconType::Rollback => include_bytes!("../icons/rollback.svg").as_slice(),
            crate::widgets::IconType::Reload => include_bytes!("../icons/reload.svg").as_slice(),
            crate::widgets::IconType::Person => include_bytes!("../icons/atom/icons/ui/person.svg").as_slice(),
            crate::widgets::IconType::Time => include_bytes!("../icons/time.svg").as_slice(),
            crate::widgets::IconType::NumberCount => include_bytes!("../icons/number_count.svg").as_slice(),
            crate::widgets::IconType::GithubDark => include_bytes!("../icons/atom/icons/files/github_dark.svg").as_slice(),
        };
        Some(data)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn builtin_icon_types() -> &'static [crate::widgets::IconType] {
        &[
            crate::widgets::IconType::Save,
            crate::widgets::IconType::Discard,
            crate::widgets::IconType::Cancel,
            crate::widgets::IconType::Warning,
            crate::widgets::IconType::Error,
            crate::widgets::IconType::CaseMatch,
            crate::widgets::IconType::Up,
            crate::widgets::IconType::Down,
            crate::widgets::IconType::Close,
            crate::widgets::IconType::Plus,
            crate::widgets::IconType::GitPlus,
            crate::widgets::IconType::GitMinus,
            crate::widgets::IconType::Terminal,
            crate::widgets::IconType::Explorer,
            crate::widgets::IconType::Search,
            crate::widgets::IconType::Git,
            crate::widgets::IconType::GitCompare,
            crate::widgets::IconType::Branch,
            crate::widgets::IconType::Problems,
            crate::widgets::IconType::LspServers,
            crate::widgets::IconType::Api,
            crate::widgets::IconType::Copy,
            crate::widgets::IconType::Check,
            crate::widgets::IconType::Rollback,
            crate::widgets::IconType::Reload,
            crate::widgets::IconType::Person,
            crate::widgets::IconType::Time,
            crate::widgets::IconType::NumberCount,
            crate::widgets::IconType::GithubDark,
        ]
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn rasterize_builtin_icon(
        icon_type: crate::widgets::IconType,
    ) -> Option<(Vec<u8>, i32, i32)> {
        let data = Self::builtin_icon_svg(icon_type)?;
        let opt = resvg::usvg::Options::default();
        let svg_data_str = String::from_utf8_lossy(data);
        let mut svg_str = if icon_type == crate::widgets::IconType::Discard {
            svg_data_str.replace("stroke=\"#ffffff\"", "stroke=\"#da4453\"")
        } else if icon_type == crate::widgets::IconType::Problems {
            svg_data_str.replace("#D81B60", "#ffffff")
        } else if icon_type == crate::widgets::IconType::Plus
            || icon_type == crate::widgets::IconType::GitPlus
            || icon_type == crate::widgets::IconType::GitMinus
            || icon_type == crate::widgets::IconType::Terminal
            || icon_type == crate::widgets::IconType::Explorer
            || icon_type == crate::widgets::IconType::Search
            || icon_type == crate::widgets::IconType::Git
            || icon_type == crate::widgets::IconType::GitCompare
            || icon_type == crate::widgets::IconType::Branch
            || icon_type == crate::widgets::IconType::Copy
            || icon_type == crate::widgets::IconType::Check
            || icon_type == crate::widgets::IconType::Rollback
            || icon_type == crate::widgets::IconType::Reload
            || icon_type == crate::widgets::IconType::Person
            || icon_type == crate::widgets::IconType::Time
            || icon_type == crate::widgets::IconType::NumberCount
            || icon_type == crate::widgets::IconType::GithubDark
        {
            svg_data_str
                .replace("currentColor", "#ffffff")
                .replace("fill=\"#000000\"", "fill=\"#ffffff\"")
                .replace("stroke=\"#000000\"", "stroke=\"#ffffff\"")
                .replace("#64B5F6", "#ffffff")
                .replace("#F06292", "#ffffff")
                .replace("#E4E5E6", "#ffffff")
        } else {
            svg_data_str.into_owned()
        };

        let target_stroke_width = match icon_type {
            crate::widgets::IconType::Up | crate::widgets::IconType::Down => "1.7",
            _ => "2.0",
        };
        svg_str = svg_str.replace(
            "stroke-width=\"2\"",
            &format!("stroke-width=\"{}\"", target_stroke_width),
        );

        let tree = resvg::usvg::Tree::from_data(svg_str.as_bytes(), &opt).ok()?;
        let size = tree.size();
        let target_size = match icon_type {
            crate::widgets::IconType::Error | crate::widgets::IconType::Warning => 128.0,
            _ => 64.0,
        };

        let scale = if size.width() > size.height() {
            target_size / size.width()
        } else {
            target_size / size.height()
        };
        let scaled_w = size.width() * scale;
        let scaled_h = size.height() * scale;
        let width = target_size as u32;
        let height = target_size as u32;
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        let dx = (target_size - scaled_w) / 2.0;
        let dy = (target_size - scaled_h) / 2.0;
        let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy);
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let data = pixmap.data_mut();
        for pixel in data.chunks_exact_mut(4) {
            let a = pixel[3] as u32;
            if a > 0 && a < 255 {
                pixel[0] = ((pixel[0] as u32 * 255) / a).min(255) as u8;
                pixel[1] = ((pixel[1] as u32 * 255) / a).min(255) as u8;
                pixel[2] = ((pixel[2] as u32 * 255) / a).min(255) as u8;
            }
        }

        Some((data.to_vec(), width as i32, height as i32))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn reset_texture_atlas(&mut self) {
        self.flush();
        self.glyphs.clear();
        self.ui_glyphs.clear();
        self.icons.clear();
        self.file_icon_cache.clear();
        self.atlas_x = 2;
        self.atlas_y = 2;
        self.max_row_h = 0;
        unsafe {
            use glow::HasContext;
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

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn upload_icon_rgba(
        &mut self,
        width: i32,
        height: i32,
        data: &[u8],
    ) -> Option<IconAtlasEntry> {
        if width <= 0 || height <= 0 || data.is_empty() {
            return None;
        }
        if self.atlas_x + width + 2 > ATLAS_SIZE_W {
            self.atlas_x = 2;
            self.atlas_y += self.max_row_h + 2;
            self.max_row_h = 0;
        }
        if self.atlas_y + height + 2 > ATLAS_SIZE_H {
            self.reset_texture_atlas();
        }
        if self.atlas_x + width + 2 > ATLAS_SIZE_W || self.atlas_y + height + 2 > ATLAS_SIZE_H {
            return None;
        }
        let x = self.atlas_x;
        let y = self.atlas_y;
        unsafe {
            use glow::HasContext;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                x,
                y,
                width,
                height,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(data)),
            );
        }
        self.atlas_x += width + 2;
        self.max_row_h = self.max_row_h.max(height);
        Some(IconAtlasEntry {
            u: x as f32 / ATLAS_SIZE_W as f32,
            v: y as f32 / ATLAS_SIZE_H as f32,
            uw: width as f32 / ATLAS_SIZE_W as f32,
            vh: height as f32 / ATLAS_SIZE_H as f32,
        })
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn upload_builtin_icon(
        &mut self,
        icon_type: crate::widgets::IconType,
    ) -> Option<IconAtlasEntry> {
        let (data, width, height) = Self::rasterize_builtin_icon(icon_type)?;
        self.upload_icon_rgba(width, height, &data)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn load_builtin_icons(&mut self) {
        self.icons.clear();
        for &icon_type in Self::builtin_icon_types() {
            if let Some(entry) = self.upload_builtin_icon(icon_type) {
                self.icons.insert(icon_type, entry);
            }
        }

        unsafe {
            use glow::HasContext;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        }
    }

    /// Рисует волнистое подчёркивание (squiggle) — зигзаг из 2px квадратов.
    /// `x` — начало, `baseline_y` — нижняя граница строки (baseline + descender),
    /// `w` — ширина участка, `color` — цвет.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn push_squiggle(&mut self, x: f32, baseline_y: f32, w: f32, color: [f32; 4]) {
        self.vertices.extend_from_slice(&squiggle_vertices(
            self.scale_factor,
            x,
            baseline_y,
            w,
            color,
        ));
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.push_quad(x, y, w, h, -1.0, -1.0, 0.0, 0.0, color, 2.0);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn push_rounded_rect_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        top_color: [f32; 4],
        bottom_color: [f32; 4],
    ) {
        self.vertices
            .extend_from_slice(&rounded_rect_gradient_vertices(
                x,
                y,
                w,
                h,
                r,
                top_color,
                bottom_color,
            ));
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_data_sources_return_expected_slices() {
        let static_font = FontData::new_static(b"abc");
        assert_eq!(static_font.data_slice(), b"abc");

        let lazy_font = FontData::new_lazy("/definitely/missing/font.ttf");
        assert!(matches!(lazy_font.source, FontSource::Lazy(_, _)));
        assert!(lazy_font.data_slice().is_empty());

        let vec_font = FontData {
            source: FontSource::LoadedVec(std::sync::Arc::new(vec![1, 2, 3, 4])),
            index: 2,
        };
        assert_eq!(vec_font.data_slice(), &[1, 2, 3, 4]);
        assert_eq!(vec_font.index, 2);
    }

    #[test]
    fn font_data_ensure_loaded_keeps_missing_lazy_source_empty() {
        let mut font = FontData::new_lazy("/definitely/missing/font.ttf");
        font.ensure_loaded();

        assert!(font.data_slice().is_empty());
        assert!(matches!(font.source, FontSource::Lazy(_, _)));
    }

    #[test]
    fn font_data_ensure_loaded_maps_existing_lazy_source_once() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("rriter_font_data_{}.bin", std::process::id()));
        std::fs::write(&tmp, b"font-bytes").expect("expected temp font data write");

        let mut font = FontData::new_lazy(tmp.to_string_lossy().as_ref());
        font.ensure_loaded();
        assert_eq!(font.data_slice(), b"font-bytes");
        assert!(matches!(font.source, FontSource::LoadedMmap(_)));

        font.ensure_loaded();
        assert_eq!(font.data_slice(), b"font-bytes");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn popup_mouse_move_gate_waits_for_real_motion_after_focus_restore() {
        let last_known = (120.0, 80.0);

        assert!(popup_waiting_for_mouse_move(true, last_known, 120.0, 80.0));
        assert!(popup_waiting_for_mouse_move(
            true, last_known, 120.25, 80.25
        ));
        assert!(!popup_waiting_for_mouse_move(true, last_known, 121.0, 80.0));
        assert!(!popup_waiting_for_mouse_move(
            false, last_known, 120.0, 80.0
        ));
    }

    #[test]
    fn emoji_presentation_property_keeps_terminal_symbols_text_first() {
        assert!(!default_emoji_presentation('✔'));
        assert!(!default_emoji_presentation('✓'));
        assert!(!default_emoji_presentation('⠋'));
        assert!(!default_emoji_presentation('⣿'));
        assert!(default_emoji_presentation('⬇'));
        assert!(!default_emoji_presentation('☀'));
        assert!(default_emoji_presentation('☔'));
        assert!(default_emoji_presentation('⏳'));
        assert!(default_emoji_presentation('✅'));
        assert!(default_emoji_presentation('😀'));
        assert!(default_emoji_presentation('🎉'));
        assert!(default_emoji_presentation('🟢'));
    }

    #[test]
    fn terminal_symbols_reject_color_emoji_glyph_content_when_text_preferred() {
        assert!(!accept_rendered_glyph_content(false, Content::Color));
        assert!(accept_rendered_glyph_content(false, Content::Mask));
        assert!(accept_rendered_glyph_content(true, Content::Color));
        assert!(accept_rendered_glyph_content(true, Content::Mask));
        assert!(terminal_force_text_presentation('✔'));
        assert!(!terminal_force_text_presentation('✅'));
    }

    #[test]
    fn custom_svg_glyphs_only_cover_builtin_ui_arrows() {
        assert!(custom_svg_glyph_source('▶').is_some());
        assert!(custom_svg_glyph_source('▼').is_some());
        assert!(custom_svg_glyph_source('✔').is_none());
        assert!(custom_svg_glyph_source('✓').is_none());
        assert!(custom_svg_glyph_source('x').is_none());
    }

    #[test]
    fn braille_mask_covers_docker_compose_spinner_chars() {
        assert_eq!(braille_dot_mask('⠋'), Some(0x0B));
        assert_eq!(braille_dot_mask('⠙'), Some(0x19));
        assert_eq!(braille_dot_mask('⣿'), Some(0xFF));
        assert_eq!(braille_dot_mask('\u{2800}'), Some(0));
        assert_eq!(braille_dot_mask('✔'), None);
    }

    #[test]
    fn custom_svg_alpha_bbox_centering_moves_down_glyph_visual_only() {
        let mut rgba = vec![0u8; 4 * 6 * 4];
        for y in 3..6 {
            let px = (y * 4 + 1) * 4;
            rgba[px + 3] = 255;
        }

        center_alpha_bbox_y(&mut rgba, 4, 6);

        assert_eq!(alpha_bounds_y(&rgba, 4, 6), Some((1, 3)));
    }

    #[test]
    fn custom_svg_alpha_bbox_centering_keeps_centered_glyph_in_place() {
        let mut rgba = vec![0u8; 4 * 6 * 4];
        for y in 1..4 {
            let px = (y * 4 + 1) * 4;
            rgba[px + 3] = 255;
        }
        let before = rgba.clone();

        center_alpha_bbox_y(&mut rgba, 4, 6);

        assert_eq!(rgba, before);
        assert_eq!(alpha_bounds_y(&rgba, 4, 6), Some((1, 3)));
    }

    #[test]
    fn vertex_layout_is_plain_data_and_visual_line_fields_are_stable() {
        let vertex = Vertex {
            pos: [1.0, 2.0],
            uv: [0.25, 0.75],
            color: [1.0, 0.5, 0.25, 1.0],
            mode: 3.0,
            sdf_params: [4.0, 5.0, 6.0],
        };
        let bytes = bytemuck::bytes_of(&vertex);
        assert_eq!(bytes.len(), std::mem::size_of::<Vertex>());

        let visual = VisualLine {
            byte_idx: 10,
            physical_line: 2,
            is_soft_wrap: true,
            whitespace_px_width: 12.0,
            text_px_width: 120.0,
            y_offset: 42.0,
            is_folded: true,
            fold_suffix: [' ', '…', ' ', '}'],
            fold_suffix_len: 4,
        };

        assert_eq!(visual.byte_idx, 10);
        assert_eq!(visual.physical_line, 2);
        assert!(visual.is_soft_wrap);
        assert!(visual.is_folded);
        assert_eq!(visual.fold_suffix_len, 4);
    }

    #[test]
    fn renderer_constants_keep_expected_atlas_and_batch_sizes() {
        assert_eq!(ATLAS_SIZE_W, 1024);
        assert_eq!(ATLAS_SIZE_H, 1024);
        assert!(MAX_VERTICES >= 32_768);
    }
}
