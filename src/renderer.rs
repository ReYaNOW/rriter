use glow::HasContext;
use std::collections::HashMap;
use std::fs;
use swash::scale::{image::Content, Render, ScaleContext, Source, StrikeWith};
use swash::FontRef;

pub const MAX_VERTICES: usize = 100_000;
pub const ATLAS_SIZE: i32 = 2048;

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
    pub titlebar_bg: [f32; 4],
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
    pub data: Vec<u8>,
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
    pub dialog_mouse_x: f32,
    pub dialog_mouse_y: f32,
    pub dialog_mouse_pressed: bool,

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

    pub icon_save: Option<glow::Texture>,
    pub icon_discard: Option<glow::Texture>,
    pub icon_cancel: Option<glow::Texture>,
    pub icon_warning: Option<glow::Texture>,

    pub icon_case_match: Option<glow::Texture>,
    pub icon_up: Option<glow::Texture>,
    pub icon_down: Option<glow::Texture>,
    pub icon_close: Option<glow::Texture>,
    pub icon_logo: Option<glow::Texture>,
    pub sticky_scroll_rects: Vec<(f32, f32, f32, f32, usize)>,
    pub phys_to_visual: Vec<usize>,
}

impl Renderer {
    pub fn new(gl: glow::Context, scale_factor: f32, theme: Theme) -> Self {
        unsafe {
            let v_shader = gl.create_shader(glow::VERTEX_SHADER).unwrap();
            gl.shader_source(v_shader, "#version 330
                in vec2 pos; in vec2 uv; in vec4 color; in float mode; in vec3 sdf_params;
                out vec2 v_uv; out vec4 v_col; out float v_mode; out vec3 v_sdf_params;
                uniform mat4 proj;
                void main() { 
                    gl_Position = proj * vec4(pos, 0.0, 1.0); 
                    v_uv = uv; v_col = color; v_mode = mode; v_sdf_params = sdf_params; 
                }");
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
                        out_color = vec4(v_col.rgb, v_col.a * alpha);
                    } else {
                        vec4 tex_color = texture(tex, v_uv);
                        if (v_mode == 1.0) { out_color = vec4(tex_color.rgb, tex_color.a * v_col.a); }
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
                ATLAS_SIZE,
                ATLAS_SIZE,
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
                    fonts.push(FontData { data, index: 0 });
                }
            }

            let nerd_font_data = include_bytes!("fonts/JetBrainsMonoNerdFont-Regular.ttf").to_vec();
            fonts.push(FontData {
                data: nerd_font_data.clone(),
                index: 0,
            });

            for path in emoji_paths.iter() {
                if let Ok(data) = fs::read(path) {
                    fonts.push(FontData { data, index: 0 });
                }
            }

            let ui_font_paths = [
                "/usr/share/fonts/noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/TTF/NotoSans-Regular.ttf",
                "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/Inter/Inter-Regular.ttf",
            ];

            let mut ui_fonts = Vec::new();
            for path in ui_font_paths.iter() {
                if let Ok(data) = fs::read(path) {
                    ui_fonts.push(FontData { data, index: 0 });
                }
            }
            ui_fonts.push(FontData {
                data: nerd_font_data,
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

            let icon_save =
                load_icon_from_memory(include_bytes!("icons/document-save.png"), "document-save");
            let icon_discard =
                load_icon_from_memory(include_bytes!("icons/edit-delete.png"), "edit-delete");
            let icon_cancel =
                load_icon_from_memory(include_bytes!("icons/dialog-cancel.png"), "dialog-cancel");
            let icon_warning =
                load_icon_from_memory(include_bytes!("icons/dialog-warning.png"), "dialog-warning");

            let icon_case_match = load_icon_from_memory(
                include_bytes!("icons/format-text-uppercase.png"),
                "format-text-uppercase",
            );
            let icon_up = load_icon_from_memory(include_bytes!("icons/go-up.png"), "go-up");
            let icon_down = load_icon_from_memory(include_bytes!("icons/go-down.png"), "go-down");
            let icon_close =
                load_icon_from_memory(include_bytes!("icons/window-close.png"), "window-close");
            let icon_logo =
                load_icon_from_memory(include_bytes!("icons/icon.png"), "icon");

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
                dialog_mouse_x: 0.0,
                dialog_mouse_y: 0.0,
                dialog_mouse_pressed: false,
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
                icon_save,
                icon_discard,
                icon_cancel,
                icon_warning,
                icon_case_match,
                icon_up,
                icon_down,
                icon_close,
                icon_logo,
                sticky_scroll_rects: Vec::new(),
                phys_to_visual: Vec::new(),
            };

            for i in 32..128u8 {
                let c = i as char;
                if let Some(g) = renderer.get_glyph(c) {
                    renderer.ascii_advances[i as usize] = g.advance;
                }
            }

            renderer
        }
    }

    pub fn get_glyph(&mut self, c: char) -> Option<GlyphInfo> {
        if let Some(g) = self.glyphs.get(&c) {
            return Some(*g);
        }
        if c == '\n' || c == '\t' || c == '\r' {
            return None;
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

        if self.atlas_x + w + 2 > ATLAS_SIZE {
            self.atlas_x = 2;
            self.atlas_y += self.max_row_h + 2;
            self.max_row_h = 0;
        }
        if self.atlas_y + h + 2 > ATLAS_SIZE {
            self.glyphs.clear();
            self.ui_glyphs.clear();
            self.atlas_x = 2;
            self.atlas_y = 2;
            self.max_row_h = 0;
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                let clear_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
                self.gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    ATLAS_SIZE,
                    ATLAS_SIZE,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&clear_data)),
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
            u: self.atlas_x as f32 / ATLAS_SIZE as f32,
            v: self.atlas_y as f32 / ATLAS_SIZE as f32,
            uw: w as f32 / ATLAS_SIZE as f32,
            vh: h as f32 / ATLAS_SIZE as f32,
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

        if self.atlas_x + w + 2 > ATLAS_SIZE {
            self.atlas_x = 2;
            self.atlas_y += self.max_row_h + 2;
            self.max_row_h = 0;
        }
        if self.atlas_y + h + 2 > ATLAS_SIZE {
            self.glyphs.clear();
            self.ui_glyphs.clear();
            self.atlas_x = 2;
            self.atlas_y = 2;
            self.max_row_h = 0;
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                let clear_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
                self.gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    ATLAS_SIZE,
                    ATLAS_SIZE,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&clear_data)),
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
            u: self.atlas_x as f32 / ATLAS_SIZE as f32,
            v: self.atlas_y as f32 / ATLAS_SIZE as f32,
            uw: w as f32 / ATLAS_SIZE as f32,
            vh: h as f32 / ATLAS_SIZE as f32,
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

        let v1 = Vertex { pos: [x1, y1], uv: [u, v], color, mode, sdf_params };
        let v2 = Vertex { pos: [x2, y1], uv: [u + uw, v], color, mode, sdf_params };
        let v3 = Vertex { pos: [x2, y2], uv: [u + uw, v + vh], color, mode, sdf_params };
        let v4 = Vertex { pos: [x1, y2], uv: [u, v + vh], color, mode, sdf_params };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }

    pub fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.push_quad(x, y, w, h, -1.0, -1.0, 0.0, 0.0, color, 2.0);
    }
}
