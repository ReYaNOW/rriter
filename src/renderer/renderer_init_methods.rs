impl Renderer {
    #[inline(always)]
    pub(crate) fn delayed_tooltip_anchor(
        &mut self,
        key: Option<u64>,
        anchor_x: f32,
        anchor_y: f32,
        now: std::time::Instant,
    ) -> Option<(f32, f32)> {
        let Some(key) = key else {
            self.reset_delayed_tooltip_anchor();
            return None;
        };

        if self.tooltip_hover_key != Some(key) {
            self.tooltip_hover_key = Some(key);
            self.tooltip_hover_start = Some(now);
            self.tooltip_hover_anchor = (anchor_x, anchor_y);
            self.tooltip_hover_mouse = (self.last_mouse_x, self.last_mouse_y);
            return None;
        }

        let tooltip_ready = self
            .tooltip_hover_start
            .is_some_and(|start| now.duration_since(start).as_secs_f32() > 0.4);
        if tooltip_ready {
            self.tooltip_hover_anchor = (anchor_x, anchor_y);
            self.tooltip_hover_mouse = (self.last_mouse_x, self.last_mouse_y);
            return Some(self.tooltip_hover_anchor);
        }

        let dx = self.last_mouse_x - self.tooltip_hover_mouse.0;
        let dy = self.last_mouse_y - self.tooltip_hover_mouse.1;
        if dx * dx + dy * dy > POPUP_MOUSE_MOVE_EPS * POPUP_MOUSE_MOVE_EPS {
            self.tooltip_hover_start = Some(now);
            self.tooltip_hover_anchor = (anchor_x, anchor_y);
            self.tooltip_hover_mouse = (self.last_mouse_x, self.last_mouse_y);
            return None;
        }

        self.tooltip_hover_start.and_then(|start| {
            (now.duration_since(start).as_secs_f32() > 0.4).then_some(self.tooltip_hover_anchor)
        })
    }

    #[inline(always)]
    pub(crate) fn reset_delayed_tooltip_anchor(&mut self) {
        self.tooltip_hover_key = None;
        self.tooltip_hover_start = None;
        self.tooltip_hover_anchor = (0.0, 0.0);
        self.tooltip_hover_mouse = (0.0, 0.0);
    }

    #[inline(always)]
    pub(crate) fn reset_delayed_tooltip_anchor_namespace(&mut self, namespace: u64) {
        const NAMESPACE_MASK: u64 = 0xf000_0000_0000_0000;
        if self
            .tooltip_hover_key
            .is_some_and(|key| key & NAMESPACE_MASK == namespace)
        {
            self.reset_delayed_tooltip_anchor();
        }
    }

    #[inline(always)]
    pub fn suppress_popups_until_next_mouse_move(&mut self) {
        self.hide_popups_until_mouse_move = true;
        self.last_known_mouse = (self.last_mouse_x, self.last_mouse_y);
    }

    #[inline(always)]
    pub fn popups_waiting_for_mouse_move_at(&self, x: f32, y: f32) -> bool {
        popup_waiting_for_mouse_move(
            self.hide_popups_until_mouse_move,
            self.last_known_mouse,
            x,
            y,
        )
    }

    #[inline(always)]
    pub fn update_popup_mouse_move_gate(&mut self) {
        if !self.popups_waiting_for_mouse_move_at(self.last_mouse_x, self.last_mouse_y) {
            self.hide_popups_until_mouse_move = false;
            self.last_known_mouse = (self.last_mouse_x, self.last_mouse_y);
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn new(gl: glow::Context, scale_factor: f32, theme: Theme) -> Self {
        unsafe {
            let v_shader = gl.create_shader(glow::VERTEX_SHADER).unwrap();
            gl.shader_source(
                v_shader,
                "#version 330
                in vec2 pos; in vec2 uv; in vec4 color; in float mode; in vec3 sdf_params;
                out vec2 v_uv; out vec4 v_col; out float v_mode; flat out vec3 v_sdf_params;
                uniform mat4 proj;
                void main() { 
                    gl_Position = proj * vec4(pos, 0.0, 1.0); 
                    v_uv = uv; v_col = color; v_mode = mode; v_sdf_params = sdf_params; 
                }",
            );
            gl.compile_shader(v_shader);

            let f_shader = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
            gl.shader_source(f_shader, "#version 330
                in vec2 v_uv; in vec4 v_col; in float v_mode; flat in vec3 v_sdf_params;
                out vec4 out_color;
                uniform sampler2D tex;
                uniform sampler2D color_tex;

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
                    } else if (v_mode == 7.0) {
                        int idx = int(v_uv.x);
                        uint m0 = floatBitsToUint(v_sdf_params.x);
                        uint m1 = floatBitsToUint(v_sdf_params.y);
                        uint m2 = floatBitsToUint(v_sdf_params.z);
                        uint mask = (idx < 32) ? m0 : ((idx < 64) ? m1 : m2);
                        uint shift = uint(idx % 32);
                        if (((mask >> shift) & 1u) == 0u) discard;
                        float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
                        out_color = vec4(v_col.rgb + (noise - 0.5) / 128.0, v_col.a);
                    } else if (v_mode == 8.0) {
                        float segment_len = v_sdf_params.x;
                        float radius = v_sdf_params.y;
                        if (v_uv.x < 0.0 || v_uv.x > segment_len) discard;
                        float d = abs(v_uv.y) - radius;
                        float alpha = 1.0 - smoothstep(-0.5, 0.5, d);
                        if (alpha <= 0.0) discard;
                        out_color = vec4(v_col.rgb, v_col.a * alpha);
                    } else if (v_mode == 9.0) {
                        float radius = mod(v_sdf_params.z, 1024.0);
                        float border = floor(v_sdf_params.z / 1024.0);
                        float d = roundedBoxSDF(v_uv, vec2(v_sdf_params.x, v_sdf_params.y), radius);
                        float outer_alpha = 1.0 - smoothstep(-0.5, 0.5, d);
                        float inner_cut = smoothstep(-border - 0.5, -border + 0.5, d);
                        float alpha = outer_alpha * inner_cut;
                        if (alpha <= 0.0) discard;
                        float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
                        out_color = vec4(v_col.rgb + (noise - 0.5) / 128.0, v_col.a * alpha);
                    } else if (v_mode == 10.0) {
                        out_color = texture(color_tex, v_uv) * v_col;
                    } else {
                        vec4 tex_color = texture(tex, v_uv);
                        if (v_mode == 1.0) { out_color = vec4(tex_color.rgb, tex_color.a * v_col.a); }
                        else {
                            float alpha = tex_color.r;
                            out_color = vec4(v_col.rgb, alpha * v_col.a);
                        }
                    }
                }");
            gl.compile_shader(f_shader);

            let program = gl.create_program().unwrap();
            gl.attach_shader(program, v_shader);
            gl.attach_shader(program, f_shader);
            gl.link_program(program);
            let proj_loc = gl.get_uniform_location(program, "proj");
            let tex_loc = gl.get_uniform_location(program, "tex");
            let color_tex_loc = gl.get_uniform_location(program, "color_tex");
            gl.use_program(Some(program));
            gl.uniform_1_i32(tex_loc.as_ref(), 0);
            gl.uniform_1_i32(color_tex_loc.as_ref(), 1);

            gl.delete_shader(v_shader);
            gl.delete_shader(f_shader);

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
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
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
                PRIMARY_ATLAS_INTERNAL_FORMAT as i32,
                ATLAS_SIZE_W,
                ATLAS_SIZE_H,
                0,
                PRIMARY_ATLAS_UPLOAD_FORMAT,
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

            let mut fonts = Vec::new();

            for path in font_paths.iter() {
                if std::path::Path::new(path).exists() {
                    fonts.push(FontData::new_lazy(path));
                    break;
                }
            }

            let nerd_font_data = include_bytes!("../fonts/JetBrainsMonoNerdFont-Regular.ttf");
            let inter_font_data = include_bytes!("../fonts/Inter-Regular.otf");

            fonts.push(FontData::new_static(nerd_font_data));

            let symbol_font_paths = [
                "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
                "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
                "/usr/share/fonts/TTF/NotoSansSymbols2-Regular.ttf",
                "/usr/share/fonts/TTF/NotoSansSymbols-Regular.ttf",
                "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
                "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/ancient-scripts/Symbola_hint.ttf",
            ];

            for path in symbol_font_paths.iter() {
                if std::path::Path::new(path).exists() {
                    fonts.push(FontData::new_lazy(path));
                }
            }

            let emoji_paths = [
                "/usr/share/fonts/noto/NotoColorEmoji.ttf",
                "/usr/share/fonts/noto-emoji/NotoColorEmoji.ttf",
                "/usr/share/fonts/noto/NotoColorEmoji.google.ttf",
            ];

            for path in emoji_paths.iter() {
                if std::path::Path::new(path).exists() {
                    fonts.push(FontData::new_lazy(path));
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

            ui_fonts.push(FontData::new_static(inter_font_data));

            for path in ui_font_paths.iter() {
                if std::path::Path::new(path).exists() {
                    ui_fonts.push(FontData::new_lazy(path));
                    break;
                }
            }

            ui_fonts.push(FontData::new_static(nerd_font_data));

            for f in &fonts {
                ui_fonts.push(f.clone());
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

            let icon_logo = load_icon_from_memory(include_bytes!("../icons/icon.png"), "icon");

            let mut renderer = Self {
                gl,
                program,
                proj_loc,
                vao,
                vbo,
                texture,
                color_texture: None,
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
                color_atlas_x: 2,
                color_atlas_y: 2,
                color_max_row_h: 0,
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
                max_tab_scroll_x: 0.0,
                last_editor_version_for_scroll_x: u64::MAX,
                fps: 0.0,
                frame_count: 0,
                time_acc: 0.0,
                search_scroll_x: 0.0,
                fps_string: String::new(),
                search_res_string: String::new(),
                scratch_buffer: String::with_capacity(256),
                last_search_idx: None,
                last_search_len: 0,
                icons: HashMap::new(),
                file_icon_cache: rustc_hash::FxHashMap::default(),
                icon_logo,
                sticky_scroll_rects: Vec::new(),
                phys_to_visual: Vec::new(),
                phys_to_visual_editor_version: u64::MAX,
                phys_to_visual_line_count: 0,
                phys_to_visual_fold_count: 0,
                phys_to_visual_fold_checksum: 0,
                hide_popups_until_mouse_move: false,
                tab_hover_timer: 0.0,
                tab_hover_idx: None,
                tooltip_hover_key: None,
                tooltip_hover_start: None,
                tooltip_hover_anchor: (0.0, 0.0),
                tooltip_hover_mouse: (0.0, 0.0),
                last_known_mouse: (0.0, 0.0),
                last_editor_version_for_typing: 0,
                last_cursor_for_popups: usize::MAX,
                last_draw_instant: None,
                git_file_tooltip: None,
                git_action_tooltip: None,
                git_graph_tooltip: None,
                git_graph_tooltip_hover: None,
                git_graph_tooltip_text: String::with_capacity(256),
                git_graph_tooltip_text_rows: Vec::with_capacity(8),
                git_graph_tooltip_selection_anchor: None,
                git_graph_tooltip_selection_cursor: None,
                git_graph_tooltip_selecting: false,
                git_graph_tooltip_stable_w: 0.0,
                git_graph_tooltip_seen_copied: None,
                git_graph_tooltip_visible_copied: None,
                git_tooltip_waiting: false,
                was_empty_ide: false,
                empty_ide_art_idx: 0,
                identical_words_cache: Vec::with_capacity(64),
                identical_words_cache_editor: 0,
                identical_words_cache_version: u64::MAX,
                identical_words_cache_cursor: usize::MAX,
                identical_words_cache_selection_anchor: None,
                bracket_pair_cache: None,
                bracket_pair_cache_version: u64::MAX,
                bracket_pair_cache_cursor: usize::MAX,
                lsp_diagnostic_indices: Vec::with_capacity(32),
                unused_spans_cache: Vec::with_capacity(32),
                current_python_inlay_hints: Vec::with_capacity(64),
                terminal_row_search_results: Vec::new(),
                mod_intervals_cache: Vec::with_capacity(64),
                merged_intervals_cache: Vec::with_capacity(64),
                tab_x_anim: Vec::new(),
                tab_display_titles: Vec::new(),
            };

            for i in 32..128u8 {
                let c = i as char;
                if let Some(g) = renderer.get_glyph(c) {
                    renderer.ascii_advances[i as usize] = g.advance;
                }
            }

            renderer.load_builtin_icons();

            // #[cfg(target_os = "linux")]
            // extern "C" { fn malloc_trim(pad: usize) -> i32; }
            // #[cfg(target_os = "linux")]
            // malloc_trim(0);

            renderer
        }
    }

}
