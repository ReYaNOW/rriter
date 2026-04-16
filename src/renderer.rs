use glow::HasContext;
use std::collections::HashMap;
use std::fs;
use swash::scale::{image::Content, Render, ScaleContext, Source, StrikeWith};
use swash::FontRef;

pub const MAX_VERTICES: usize = 100_000;
pub const ATLAS_SIZE_W: i32 = 1024;
pub const ATLAS_SIZE_H: i32 = 1024;

#[derive(Clone)]
pub struct Theme {
    pub bg: [f32; 4],
    pub fg: [f32; 4],
    pub sel: [f32; 4],
    pub minimap_bg: [f32; 4],
    pub line_num: [f32; 4],
    pub minimap_cursor: [f32; 4],
    pub modified_unsaved: [f32; 4],
    pub modified_saved: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub mode: f32,
    pub sdf_params: [f32; 3],
}

unsafe impl bytemuck::Zeroable for Vertex {}
unsafe impl bytemuck::Pod for Vertex {}

#[derive(Copy, Clone)]
pub struct GlyphInfo {
    pub u: f32,
    pub v: f32,
    pub uw: f32,
    pub vh: f32,
    pub width: f32,
    pub height: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub advance: f32,
    pub is_emoji: f32,
}

// SvgIcon удалён, используем glow::Texture напрямую

#[derive(Clone, Copy, Debug)]
pub struct VisualLine {
    pub byte_idx: usize,
    pub physical_line: usize,
    pub is_soft_wrap: bool,
    pub whitespace_px_width: f32,
    pub text_px_width: f32,
    pub y_offset: f32,
    pub is_folded: bool,
    pub fold_suffix: [char; 4],
    pub fold_suffix_len: u8,
}
#[derive(Clone)]
pub struct FontData {
    pub data: std::borrow::Cow<'static, [u8]>,
    pub index: u32,
}

pub struct Renderer {
    pub gl: glow::Context,
    pub program: glow::Program,
    pub vao: glow::VertexArray,
    pub vbo: glow::Buffer,
    pub texture: glow::Texture,
    pub vertices: Vec<Vertex>,

    pub fonts: Vec<FontData>,
    pub ui_fonts: Vec<FontData>,
    pub scale_context: ScaleContext,
    pub glyphs: HashMap<char, GlyphInfo>,
    pub ui_glyphs: HashMap<char, GlyphInfo>,

    pub ascii_advances: [f32; 128],

    pub atlas_x: i32,
    pub atlas_y: i32,
    pub max_row_h: i32,
    pub font_size: f32,
    pub scale_factor: f32,

    pub theme: Theme,
    pub width: f32,
    pub height: f32,
    pub minimap_width: f32,
    pub line_height: f32,
    pub baseline_offset: f32,
    pub left_padding: f32,
    pub last_mouse_x: f32,
    pub last_mouse_y: f32,

    pub visual_lines: Vec<VisualLine>,
    pub last_editor_version: u64,
    pub last_height: f32,
    pub last_width: f32,

    pub last_scroll_y: f32,
    pub last_scroll_x: f32,
    pub max_scroll_x: f32,
    pub last_editor_version_for_scroll_x: u64,

    pub last_frame_time: Option<std::time::Instant>,
    pub fps: f32,
    pub frame_count: u32,
    pub time_acc: f32,
    pub search_scroll_x: f32,

    pub fps_string: String,
    pub search_res_string: String,
    pub last_search_idx: Option<usize>,
    pub last_search_len: usize,

    pub icons: std::collections::HashMap<crate::widgets::IconType, glow::Texture>,
    pub icon_logo: Option<glow::Texture>,
    /// Кэш SVG-иконок для дерева файлов. Ключ — &'static str из file_icons_map.
    pub file_icon_cache: rustc_hash::FxHashMap<&'static str, glow::Texture>,
    pub sticky_scroll_rects: Vec<(f32, f32, f32, f32, usize)>,
            pub phys_to_visual: Vec<usize>,
        pub last_hovered_diags: Vec<usize>,
        pub last_diag_popup_rect: Option<(f32, f32, f32, f32)>,
        pub last_diag_href: Option<String>,
        pub hide_popups_until_mouse_move: bool,
        pub diag_hover_timer: f32,
        pub diag_hover_timer_idx: Option<usize>,
        pub last_known_mouse: (f32, f32),
        pub last_editor_version_for_typing: u64,
        pub last_cursor_for_popups: usize,
        pub last_draw_instant: Option<std::time::Instant>,
}

impl Renderer {
    pub fn new(gl: glow::Context, scale_factor: f32, theme: Theme) -> Self {
        unsafe {
            let v_shader = gl.create_shader(glow::VERTEX_SHADER).unwrap();
            gl.shader_source(
                v_shader,
                "#version 330
                in vec2 pos; in vec2 uv; in vec4 color; in float mode; in vec3 sdf_params;
                out vec2 v_uv; out vec4 v_col; out float v_mode; out vec3 v_sdf_params;
                uniform mat4 proj;
                void main() { 
                    gl_Position = proj * vec4(pos, 0.0, 1.0); 
                    v_uv = uv; v_col = color; v_mode = mode; v_sdf_params = sdf_params; 
                }",
            );
            gl.compile_shader(v_shader);

            let f_shader = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
            gl.shader_source(f_shader, "#version 330
                in vec2 v_uv; in vec4 v_col; in float v_mode; in vec3 v_sdf_params;
                out vec4 out_color;
                uniform sampler2D tex;
                
                float roundedBoxSDF(vec2 CenterPosition, vec2 Size, float Radius) {
                    return length(max(abs(CenterPosition) - Size + Radius, 0.0)) - Radius;
                }
                
                void main() {
                    if(v_mode == 2.0) { 
                        float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
                        out_color = vec4(v_col.rgb + (noise - 0.5) / 128.0, v_col.a); 
                    } else if(v_mode == 3.0) {
                        float d = roundedBoxSDF(v_uv, vec2(v_sdf_params.x, v_sdf_params.y), v_sdf_params.z);
                        float alpha = 1.0 - smoothstep(-0.5, 0.5, d);
                        if (alpha <= 0.0) discard;
                        float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
                        out_color = vec4(v_col.rgb + (noise - 0.5) / 128.0, v_col.a * alpha);
                                        } else if (v_mode == 4.0) {
                        out_color = v_col;
                    } else if (v_mode == 6.0) {
                                                float wave = v_sdf_params.x * sin(v_uv.x * v_sdf_params.y);
                        float d = abs(v_uv.y - wave) - v_sdf_params.z;
                        float alpha = 1.0 - smoothstep(0.0, 1.5, d);
                        if (alpha <= 0.0) discard;
                        out_color = vec4(v_col.rgb, v_col.a * alpha);
                    } else {
                        vec4 tex_color = texture(tex, v_uv);
                        if (v_mode == 1.0) { out_color = vec4(tex_color.rgb, tex_color.a * v_col.a); }
                        else if (v_mode == 5.0) { out_color = tex_color * v_col; }
                        else { out_color = vec4(v_col.rgb, tex_color.a * v_col.a); }
                    }
                }");
            gl.compile_shader(f_shader);

            let program = gl.create_program().unwrap();
            gl.attach_shader(program, v_shader);
            gl.attach_shader(program, f_shader);
            gl.link_program(program);

            let vao = gl.create_vertex_array().unwrap();
            let vbo = gl.create_buffer().unwrap();
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));

            let vbo_size = (MAX_VERTICES * std::mem::size_of::<Vertex>()) as i32;
            gl.buffer_data_size(glow::ARRAY_BUFFER, vbo_size, glow::DYNAMIC_DRAW);

            let stride = std::mem::size_of::<Vertex>() as i32;
            let pos_loc = gl.get_attrib_location(program, "pos").unwrap();
            gl.vertex_attrib_pointer_f32(pos_loc, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(pos_loc);
            let uv_loc = gl.get_attrib_location(program, "uv").unwrap();
            gl.vertex_attrib_pointer_f32(uv_loc, 2, glow::FLOAT, false, stride, 8);
            gl.enable_vertex_attrib_array(uv_loc);
            let color_loc = gl.get_attrib_location(program, "color").unwrap();
            gl.vertex_attrib_pointer_f32(color_loc, 4, glow::FLOAT, false, stride, 16);
            gl.enable_vertex_attrib_array(color_loc);
            let mode_loc = gl.get_attrib_location(program, "mode").unwrap();
            gl.vertex_attrib_pointer_f32(mode_loc, 1, glow::FLOAT, false, stride, 32);
            gl.enable_vertex_attrib_array(mode_loc);
            let sdf_loc = gl.get_attrib_location(program, "sdf_params").unwrap();
            gl.vertex_attrib_pointer_f32(sdf_loc, 3, glow::FLOAT, false, stride, 36);
            gl.enable_vertex_attrib_array(sdf_loc);

            let texture = gl.create_texture().unwrap();
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_image_2d(
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

            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ZERO,
                glow::ONE,
            );

            let font_paths = [
                "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
                "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
                "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            ];

            let emoji_paths = [
                "/usr/share/fonts/noto/NotoColorEmoji.ttf",
                "/usr/share/fonts/noto-emoji/NotoColorEmoji.ttf",
                "/usr/share/fonts/noto/NotoColorEmoji.google.ttf",
            ];

            let mut fonts = Vec::new();

            for path in font_paths.iter() {
                if let Ok(data) = fs::read(path) {
                    fonts.push(FontData {
                        data: std::borrow::Cow::Owned(data),
                        index: 0,
                    });
                    break;
                }
            }

            let nerd_font_data: std::borrow::Cow<'static, [u8]> = std::borrow::Cow::Borrowed(
                include_bytes!("fonts/JetBrainsMonoNerdFont-Regular.ttf"),
            );
            let inter_font_data: std::borrow::Cow<'static, [u8]> =
                std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-Regular.otf"));

            fonts.push(FontData {
                data: nerd_font_data.clone(),
                index: 0,
            });

            for path in emoji_paths.iter() {
                if let Ok(data) = fs::read(path) {
                    fonts.push(FontData {
                        data: std::borrow::Cow::Owned(data),
                        index: 0,
                    });
                    break;
                }
            }

            let ui_font_paths = [
                "/usr/share/fonts/noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/TTF/NotoSans-Regular.ttf",
                "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/Inter/Inter-Regular.ttf",
            ];

            let mut ui_fonts = Vec::new();

            // Inter теперь первый в приоритете для UI
            ui_fonts.push(FontData {
                data: inter_font_data.clone(),
                index: 0,
            });

            for path in ui_font_paths.iter() {
                if let Ok(data) = fs::read(path) {
                    ui_fonts.push(FontData {
                        data: std::borrow::Cow::Owned(data),
                        index: 0,
                    });
                    break;
                }
            }

            ui_fonts.push(FontData {
                data: nerd_font_data.clone(),
                index: 0,
            });

            if ui_fonts.is_empty() {
                for f in &fonts {
                    ui_fonts.push(FontData {
                        data: f.data.clone(),
                        index: f.index,
                    });
                }
            }

            let load_icon_from_memory = |data: &[u8], _name: &str| -> Option<glow::Texture> {
                let img = image::load_from_memory(data).ok()?.into_rgba8();
                let (w, h) = img.dimensions();
                let tex = gl.create_texture().unwrap();

                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w as i32,
                    h as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&img.into_raw())),
                );

                gl.generate_mipmap(glow::TEXTURE_2D);
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR_MIPMAP_LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_S,
                    glow::CLAMP_TO_EDGE as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_T,
                    glow::CLAMP_TO_EDGE as i32,
                );
                Some(tex)
            };

            let icon_logo = load_icon_from_memory(include_bytes!("icons/icon.png"), "icon");

            let mut renderer = Self {
                gl,
                program,
                vao,
                vbo,
                texture,
                vertices: Vec::with_capacity(MAX_VERTICES),
                fonts,
                ui_fonts,
                scale_context: ScaleContext::new(),
                glyphs: HashMap::new(),
                ui_glyphs: HashMap::new(),
                ascii_advances: [0.0; 128],
                atlas_x: 2,
                atlas_y: 2,
                max_row_h: 0,
                font_size: 18.0 * scale_factor,
                scale_factor,
                theme,
                width: 1000.0,
                height: 800.0,
                minimap_width: 110.0,
                line_height: (26.0 * scale_factor).round(),
                baseline_offset: (19.0 * scale_factor).round(),
                left_padding: (60.0 * scale_factor).round(),
                last_mouse_x: 0.0,
                last_mouse_y: 0.0,
                visual_lines: Vec::new(),
                last_editor_version: u64::MAX,
                last_height: 0.0,
                last_width: 0.0,
                last_frame_time: None,
                last_scroll_y: 0.0,
                last_scroll_x: 0.0,
                max_scroll_x: 0.0,
                last_editor_version_for_scroll_x: u64::MAX,
                fps: 0.0,
                frame_count: 0,
                time_acc: 0.0,
                search_scroll_x: 0.0,
                fps_string: String::new(),
                search_res_string: String::new(),
                last_search_idx: None,
                last_search_len: 0,
                icons: HashMap::new(),
                file_icon_cache: rustc_hash::FxHashMap::default(),
                icon_logo,
                                sticky_scroll_rects: Vec::new(),
                phys_to_visual: Vec::new(),
                last_hovered_diags: Vec::new(),
                last_diag_popup_rect: None,
                last_diag_href: None,
                hide_popups_until_mouse_move: false,
                diag_hover_timer: 0.0,
                diag_hover_timer_idx: None,
                last_known_mouse: (0.0, 0.0),
                last_editor_version_for_typing: u64::MAX,
                last_cursor_for_popups: usize::MAX,
                last_draw_instant: None,
            };

            for i in 32..128u8 {
                let c = i as char;
                if let Some(g) = renderer.get_glyph(c) {
                    renderer.ascii_advances[i as usize] = g.advance;
                }
            }

            renderer.load_builtin_icons();

            renderer
        }
    }

    pub fn get_custom_svg_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        let svg_str = match c {
            '▶' => "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"4 2 16 20\"><path fill=\"#ffffff\" d=\"M8 5.14v14l11-7z\"/></svg>",
            '▼' => "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"2 4 20 16\"><g transform=\"rotate(90 12 12)\"><path fill=\"#ffffff\" d=\"M8 5.14v14l11-7z\"/></g></svg>",
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

        for &idx in &indices {
            let font_data = &self.fonts[idx];
            if let Some(font_ref) = FontRef::from_index(&font_data.data, font_data.index as usize) {
                let glyph_id = font_ref.charmap().map(c);
                if glyph_id != 0 || (c == ' ' && idx == 0) {
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
                        if img.data.len() > 0 || c == ' ' {
                            rendered_image = Some(img);
                            break;
                        }
                    }
                }
            }
        }

        if rendered_image.is_none() {
            if c != '□' {
                return self.get_glyph('□');
            }
            return None;
        }

        let img = rendered_image.unwrap();
        let w = img.placement.width as i32;
        let h = img.placement.height as i32;

        if c == ' ' || w <= 0 || h <= 0 {
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

        for &idx in &indices {
            let font_data = &self.ui_fonts[idx];
            if let Some(font_ref) = FontRef::from_index(&font_data.data, font_data.index as usize) {
                let glyph_id = font_ref.charmap().map(c);
                if glyph_id != 0 || (c == ' ' && idx == 0) {
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
                        if img.data.len() > 0 || c == ' ' {
                            rendered_image = Some(img);
                            break;
                        }
                    }
                }
            }
        }

        if rendered_image.is_none() {
            if c != '□' {
                return self.get_ui_glyph('□');
            }
            return None;
        }

        let img = rendered_image.unwrap();
        let w = img.placement.width as i32;
        let h = img.placement.height as i32;

        if c == ' ' || w <= 0 || h <= 0 {
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
            offset_y: img.placement.top as f32,
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

    pub fn resize(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.width = w as f32;
            self.height = h as f32;
            unsafe {
                self.gl.viewport(0, 0, w as i32, h as i32);
            }
        }
    }

    pub fn measure_ui_width(&mut self, text: &str, scale: f32) -> f32 {
        let mut w = 0.0;
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                w += g.advance * scale;
            }
        }
        w
    }

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
        let x1 = x.round();
        let y1 = y.round();
        let x2 = (x + w).round();
        let y2 = (y + h).round();

        let sdf_params = [0.0, 0.0, 0.0];

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [u, v],
            color,
            mode,
            sdf_params,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [u + uw, v],
            color,
            mode,
            sdf_params,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [u + uw, v + vh],
            color,
            mode,
            sdf_params,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [u, v + vh],
            color,
            mode,
            sdf_params,
        };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }

    pub fn load_builtin_icons(&mut self) {
        let builtin = [
            (
                crate::widgets::IconType::Save,
                include_bytes!("icons/document-save.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Discard,
                include_bytes!("icons/edit-delete.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Cancel,
                include_bytes!("icons/dialog-cancel.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Warning,
                include_bytes!("icons/dialog-warning.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::CaseMatch,
                include_bytes!("icons/format-text-uppercase.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Down,
                include_bytes!("icons/go-down.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Close,
                include_bytes!("icons/window-close.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Plus,
                include_bytes!("icons/plus.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Terminal,
                include_bytes!("icons/atom/icons/ui/terminal.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Explorer,
                include_bytes!("icons/atom/icons/ui/files.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Problems,
                include_bytes!("icons/problems.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::LspServers,
                include_bytes!("icons/atom/icons/ui/server.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Copy,
                include_bytes!("icons/copy.svg").as_slice(),
            ),
            (
                crate::widgets::IconType::Check,
                include_bytes!("icons/check.svg").as_slice(),
            ),
        ];
        let opt = resvg::usvg::Options::default();
        for (icon_type, data) in builtin {
            let svg_data_str = String::from_utf8_lossy(data);
            let mut svg_str = if icon_type == crate::widgets::IconType::Discard {
                // Заменяем жестко прописанный белый цвет на старый розовый #da4453
                svg_data_str.replace("stroke=\"#ffffff\"", "stroke=\"#da4453\"")
            } else if icon_type == crate::widgets::IconType::Problems {
                svg_data_str.replace("#D81B60", "#b0bec5")
            } else if icon_type == crate::widgets::IconType::Plus
                || icon_type == crate::widgets::IconType::Terminal
                || icon_type == crate::widgets::IconType::Explorer
                || icon_type == crate::widgets::IconType::LspServers
                || icon_type == crate::widgets::IconType::Copy
                || icon_type == crate::widgets::IconType::Check
            {
                svg_data_str
                    .replace("currentColor", "#ffffff")
                    .replace("fill=\"#000000\"", "fill=\"#ffffff\"")
                    .replace("stroke=\"#000000\"", "stroke=\"#ffffff\"")
            } else {
                svg_data_str.into_owned()
            };

            // Подбираем идеальную толщину для разных иконок, чтобы они выглядели сбалансированно.
            let target_stroke_width = match icon_type {
                crate::widgets::IconType::Up | crate::widgets::IconType::Down => "1.7", // Стрелки делаем чуть изящнее
                _ => "2.0", // Остальные иконки - "сочные" и жирные
            };
            svg_str = svg_str.replace(
                "stroke-width=\"2\"",
                &format!("stroke-width=\"{}\"", target_stroke_width),
            );

            if let Ok(tree) = resvg::usvg::Tree::from_data(svg_str.as_bytes(), &opt) {
                let size = tree.size();
                // SSAA (Super-Sampling): растеризуем вектор в гигантском разрешении.
                // GPU Mipmaps аппаратно сожмут её до нужного размера без "мыла" и "лесенок".
                let target_size = 128.0;

                let scale = if size.width() > size.height() {
                    target_size / size.width()
                } else {
                    target_size / size.height()
                };
                let scaled_w = size.width() * scale;
                let scaled_h = size.height() * scale;
                let width = target_size as u32;
                let height = target_size as u32;
                if let Some(mut pixmap) = tiny_skia::Pixmap::new(width, height) {
                    // Динамически центрируем SVG на основе его параметров ширины/высоты,
                    // чтобы текстура всегда была идеальным квадратом. Это предотвратит
                    // растягивание иконок и смещение фонового круга при наведении.
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

                    let tex = unsafe {
                        use glow::HasContext;
                        let tex = self.gl.create_texture().unwrap();
                        self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                        self.gl.tex_image_2d(
                            glow::TEXTURE_2D,
                            0,
                            glow::RGBA8 as i32,
                            width as i32,
                            height as i32,
                            0,
                            glow::RGBA,
                            glow::UNSIGNED_BYTE,
                            glow::PixelUnpackData::Slice(Some(data)),
                        );
                        self.gl.generate_mipmap(glow::TEXTURE_2D);
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_MIN_FILTER,
                            glow::LINEAR_MIPMAP_LINEAR as i32,
                        );
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_MAG_FILTER,
                            glow::LINEAR as i32,
                        );
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_WRAP_S,
                            glow::CLAMP_TO_EDGE as i32,
                        );
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_WRAP_T,
                            glow::CLAMP_TO_EDGE as i32,
                        );
                        tex
                    };

                    self.icons.insert(icon_type, tex);
                }
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
    pub fn push_squiggle(&mut self, x: f32, baseline_y: f32, w: f32, color: [f32; 4]) {
        let s = self.scale_factor;
        let amplitude = 1.0 * s;
        let period = 0.6 / s;
        let thickness = 0.05 * s;

        let h = amplitude * 2.0 + thickness * 2.0 + 2.0;
        let y_center = baseline_y + amplitude + thickness;

        let x1 = x.round();
        let y1 = (y_center - h / 2.0).round();
        let x2 = (x + w).round();
        let y2 = (y_center + h / 2.0).round();

        let uv_x0 = 0.0;
        let uv_x1 = x2 - x1;
        let uv_y0 = -(h / 2.0);
        let uv_y1 = h / 2.0;

        let sdf_params = [amplitude, period, thickness];

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [uv_x0, uv_y0],
            color,
            mode: 6.0,
            sdf_params,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [uv_x1, uv_y0],
            color,
            mode: 6.0,
            sdf_params,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [uv_x1, uv_y1],
            color,
            mode: 6.0,
            sdf_params,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [uv_x0, uv_y1],
            color,
            mode: 6.0,
            sdf_params,
        };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }

    pub fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.push_quad(x, y, w, h, -1.0, -1.0, 0.0, 0.0, color, 2.0);
    }

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
        let w_round = w.round();
        let h_round = h.round();
        let x1 = x.round();
        let y1 = y.round();
        let x2 = (x + w).round();
        let y2 = (y + h).round();

        let hw = w_round / 2.0;
        let hh = h_round / 2.0;
        let sdf_params = [hw, hh, r];

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [-hw, -hh],
            color: top_color,
            mode: 3.0,
            sdf_params,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [hw, -hh],
            color: top_color,
            mode: 3.0,
            sdf_params,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [hw, hh],
            color: bottom_color,
            mode: 3.0,
            sdf_params,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [-hw, hh],
            color: bottom_color,
            mode: 3.0,
            sdf_params,
        };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }
}
