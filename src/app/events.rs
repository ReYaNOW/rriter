use crate::app::{App, PendingAction};
use crate::renderer::Renderer;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::PossiblyCurrentGlContext;
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, WindowSurface};
use glutin_winit::DisplayBuilder;
use std::num::NonZeroU32;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowId};

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let template = ConfigTemplateBuilder::new().with_transparency(false);

        let icon_bytes = include_bytes!("../icons/icon.png");
        let icon_image = image::load_from_memory(icon_bytes).unwrap().into_rgba8();
        let (icon_w, icon_h) = icon_image.dimensions();
        let window_icon =
            winit::window::Icon::from_rgba(icon_image.into_raw(), icon_w, icon_h).ok();

        let display_builder = DisplayBuilder::new().with_window_attributes(Some(
            Window::default_attributes()
                .with_title(format!("{} — RRiter", self.base_title))
                .with_inner_size(winit::dpi::LogicalSize::new(
                    self.window_width,
                    self.window_height,
                ))
                .with_name("rriter", "rriter")
                .with_window_icon(window_icon)
                .with_transparent(false),
        ));

        let (window_opt, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs.reduce(|a, _| a).unwrap()
            })
            .unwrap();
        let window = window_opt.unwrap();

        let raw_window_handle = window.window_handle().unwrap().as_raw();

        let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window_handle));

        let display = gl_config.display();
        let not_current_gl_context = unsafe {
            display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .unwrap()
                })
        };

        let attrs = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(window.inner_size().width.max(1)).unwrap(),
            NonZeroU32::new(window.inner_size().height.max(1)).unwrap(),
        );

        let gl_surface = unsafe { display.create_window_surface(&gl_config, &attrs).unwrap() };
        let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

        let _ = gl_surface.set_swap_interval(
            &gl_context,
            glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        );

        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                let c_str = std::ffi::CString::new(s).unwrap();
                display.get_proc_address(c_str.as_c_str()) as *const _
            })
        };

        let scale_factor = window.scale_factor() as f32;
        self.renderer = Some(Renderer::new(gl, scale_factor, self.theme.clone()));
        self.gl_config = Some(gl_config);
        self.window = Some(window);
        self.gl_context = Some(gl_context);
        self.gl_surface = Some(gl_surface);

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(dw) = self.dialog_window.as_ref() {
            if _id == dw.id() {
                match event {
                    WindowEvent::CloseRequested => {
                        self.close_dialog();
                    }
                    WindowEvent::MouseInput {
                        state: winit::event::ElementState::Pressed,
                        button: winit::event::MouseButton::Left,
                        ..
                    } => {
                        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                        let my = self.renderer.as_ref().unwrap().last_mouse_y;
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let (btn_save, btn_discard, btn_cancel) =
                            crate::widgets::get_dialog_buttons(
                                0.0,
                                0.0,
                                660.0 * s,
                                260.0 * s,
                                s,
                                self.renderer.as_mut().unwrap(),
                            );

                        if btn_save.is_hovered(mx, my) {
                            if self.save_current_file() {
                                if let Some(w) = self.window.as_ref() {
                                    App::update_window_title(
                                        w,
                                        &self.base_title,
                                        self.editor.is_dirty(),
                                    );
                                }
                                let action = self.pending_action;
                                self.close_dialog();
                                if action == PendingAction::Quit {
                                    let w = self.window.as_ref().unwrap();
                                    let maximized = w.is_maximized();
                                    let (width, height) = if maximized {
                                        (self.window_width, self.window_height)
                                    } else {
                                        let scale = w.scale_factor();
                                        let size = w.inner_size().to_logical::<f64>(scale);
                                        (size.width, size.height)
                                    };
                                    crate::save_config(&crate::Config {
                                        window_width: width,
                                        window_height: height,
                                        maximized,
                                        ide_workspaces: self.ide_workspaces.clone(),
                                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                                    });
                                    event_loop.exit();
                                } else if action == PendingAction::OpenFile {
                                    self.trigger_file_picker();
                                } else if action == PendingAction::CloseFile {
                                    self.close_current_file();
                                }
                            }
                        } else if btn_discard.is_hovered(mx, my) {
                            let action = self.pending_action;
                            self.close_dialog();
                            if action == PendingAction::Quit {
                                let w = self.window.as_ref().unwrap();
                                let maximized = w.is_maximized();
                                let (width, height) = if maximized {
                                    (self.window_width, self.window_height)
                                } else {
                                    let scale = w.scale_factor();
                                    let size = w.inner_size().to_logical::<f64>(scale);
                                    (size.width, size.height)
                                };
                                crate::save_config(&crate::Config {
                                    window_width: width,
                                    window_height: height,
                                    maximized,
                                    ide_workspaces: self.ide_workspaces.clone(),
                                    ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                                });
                                event_loop.exit();
                            } else if action == PendingAction::OpenFile {
                                self.trigger_file_picker();
                            } else if action == PendingAction::CloseFile {
                                self.close_current_file();
                            }
                        } else if btn_cancel.is_hovered(mx, my) {
                            self.close_dialog();
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        self.renderer.as_mut().unwrap().last_mouse_x = position.x as f32;
                        self.renderer.as_mut().unwrap().last_mouse_y = position.y as f32;
                        dw.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        let gl_context = self.gl_context.as_ref().unwrap();
                        let gl_surface = self.dialog_gl_surface.as_ref().unwrap();
                        gl_context.make_current(gl_surface).unwrap();

                        let r = self.renderer.as_mut().unwrap();
                        let s = r.scale_factor;
                        r.resize((660.0 * s) as u32, (260.0 * s) as u32);

                        unsafe {
                            use glow::HasContext;
                            r.gl.clear_color(0.12, 0.13, 0.22, 1.0);
                            r.gl.clear(glow::COLOR_BUFFER_BIT);
                        }

                        r.draw_dialog_window(&self.base_title);
                        gl_surface.swap_buffers(gl_context).unwrap();

                        let main_surface = self.gl_surface.as_ref().unwrap();
                        gl_context.make_current(main_surface).unwrap();
                        let mw = self.window.as_ref().unwrap().inner_size();
                        self.renderer.as_mut().unwrap().resize(mw.width, mw.height);
                    }
                    _ => {}
                }
                return;
            }
        }

        if self.window.is_none() || _id != self.window.as_ref().unwrap().id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if self.editor.is_dirty() {
                    self.show_action_dialog(event_loop, PendingAction::Quit);
                } else {
                    let w = self.window.as_ref().unwrap();
                    let maximized = w.is_maximized();
                    let (width, height) = if maximized {
                        (self.window_width, self.window_height)
                    } else {
                        let scale = w.scale_factor();
                        let size = w.inner_size().to_logical::<f64>(scale);
                        (size.width, size.height)
                    };
                    crate::save_config(&crate::Config {
                        window_width: width,
                        window_height: height,
                        maximized,
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    });
                    event_loop.exit();
                }
            }
            WindowEvent::Focused(focused) => {
                self.is_focused = focused;
                if focused {
                    if let Some(dw) = self.dialog_window.as_ref() {
                        // НЕ вызываем focus_window() здесь.
                        // Это - главная причина "мерцания" при Alt+Tab, т.к. приложение
                        // начинает бороться с оконным менеджером за фокус.
                        // Вместо этого, фокус будет восстановлен при клике или нажатии
                        // клавиши на основное окно, что является более предсказуемым поведением.
                        dw.request_redraw();
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    let gl_context = self.gl_context.as_ref().unwrap();
                    let gl_surface = self.gl_surface.as_ref().unwrap();
                    gl_surface.resize(
                        gl_context,
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    );
                    self.renderer
                        .as_mut()
                        .unwrap()
                        .resize(size.width, size.height);
                    self.renderer
                        .as_mut()
                        .unwrap()
                        .last_editor_version_for_scroll_x = u64::MAX;
                    self.last_resize_time = Some(Instant::now());
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(mod_state) => self.modifiers = mod_state.state(),
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_main_mouse_wheel(delta);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.handle_main_mouse_input(event_loop, state);
            }
            WindowEvent::CursorMoved { position, .. } => self.handle_main_cursor_moved(position),
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_main_keyboard_input(event_loop, key_event);
            }
            WindowEvent::RedrawRequested => {
                let gl_context = self.gl_context.as_ref().unwrap();
                let gl_surface = self.gl_surface.as_ref().unwrap();

                if !self.is_ready {
                    unsafe {
                        use glow::HasContext;
                        let gl = &self.renderer.as_ref().unwrap().gl;
                        gl.clear_color(self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                    }
                    gl_surface.swap_buffers(gl_context).unwrap();

                    self.is_ready = true;
                    // Применяем максимизацию, если сохранено
                    if !self.tried_maximize {
                        self.tried_maximize = true;
                        if self.should_maximize {
                            if let Some(w) = self.window.as_ref() {
                                w.set_maximized(true);
                            }
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }

                let blink_alpha = if !self.is_focused || self.dialog_window.is_some() {
                    1.0
                } else if self.last_blink_state {
                    1.0
                } else {
                    0.0
                };

                let is_resizing = self.last_resize_time.is_some();

                let lsp_diags = self
                    .lsp
                    .as_ref()
                    .map(|l| l.diagnostics.as_slice())
                    .unwrap_or(&[]);

                // Очищаем UI registry перед новым кадром
                self.ui_registry.clear();

                let (mut wants_pointer, target_sticky) = self.renderer.as_mut().unwrap().draw(
                    &mut self.editor,
                    self.scroll_x.current,
                    self.scroll_y.current,
                    blink_alpha,
                    self.show_fps,
                    &self.highlighter.spans,
                    self.dialog_window.is_some(),
                    is_resizing,
                    &self.search_results,
                    self.search_current_idx,
                    self.search_anim_y,
                    &self.search_editor,
                    self.search_focused,
                    self.search_case_sensitive,
                    self.show_welcome,
                    &self.recent_files,
                    &self.current_sticky_lines,
                    self.sticky_anim_progress,
                    self.sticky_anim_is_adding,
                    self.is_ide_mode,
                    &self.ide_panel,
                    self.show_settings,
                    lsp_diags,
                    &mut self.ui_registry,
                );

                self.target_sticky_lines = target_sticky;

                // Сбрасываем иконку копирования когда popup диагностики закрывается
                if self.renderer.as_ref().unwrap().last_hovered_diag.is_none() {
                    self.ide_panel.diag_copied_idx = None;
                }

                // Сбрасываем иконку копирования когда popup диагностики закрывается
                if self.renderer.as_ref().unwrap().last_hovered_diag.is_none() {
                    self.ide_panel.diag_copied_idx = None;
                }

                let (mx, my, s, minimap_w) = {
                    let r = self.renderer.as_ref().unwrap();
                    (
                        r.last_mouse_x,
                        r.last_mouse_y,
                        r.scale_factor,
                        r.minimap_width,
                    )
                };

                let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
                let window_height = self.window.as_ref().unwrap().inner_size().height as f32;
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_max_scroll(&self.editor, window_height);
                let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };
                let scrollbar_x = window_width - minimap_w - scrollbar_w;

                let mut over_search = false;
                if self.show_search && self.search_anim_y > -10.0 {
                    let search_w = 480.0 * s;
                    let search_h = 52.0 * s;
                    let search_x = scrollbar_x - search_w - 20.0 * s;
                    if mx >= search_x
                        && mx <= search_x + search_w
                        && my >= self.search_anim_y
                        && my <= self.search_anim_y + search_h
                    {
                        over_search = true;
                    }
                }

                if !over_search {
                    let r = self.renderer.as_ref().unwrap();
                    for &(rx, ry, rw, rh, _) in &r.sticky_scroll_rects {
                        if mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh {
                            wants_pointer = true;
                            break;
                        }
                    }
                }

                // LSP actions menu — рисуем поверх всего
                if let Some(menu) = &self.lsp_actions_menu {
                    let wants = self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .draw_lsp_actions_menu(menu, blink_alpha);
                    if wants {
                        wants_pointer = true;
                    }
                }

                if self.autocomplete_active && !self.autocomplete_options.is_empty() {
                    let (cx, cy) = self.renderer.as_mut().unwrap().get_cursor_xy(&self.editor);
                    let render_scroll_y = self.scroll_y.current.round();
                    let rect = self.renderer.as_mut().unwrap().draw_autocomplete(
                        cx,
                        cy - render_scroll_y,
                        &self.autocomplete_options,
                        self.autocomplete_selected_idx,
                        self.autocomplete_anim_progress,
                        self.autocomplete_scroll.current,
                        self.autocomplete_hovered_idx,
                    );
                    self.autocomplete_rect = Some(rect);
                    if self.autocomplete_hovered_idx.is_some() {
                        wants_pointer = true;
                    }
                } else {
                    self.autocomplete_rect = None;
                }

                let mut settings_cursor_mode = 0;
                if self.show_settings || self.settings_anim_progress > 0.0 {
                    // Помечаем границу: элементы оверлея регистрируются ниже.
                    // find_overlay_at() будет искать только среди них.
                    self.ui_registry.mark_overlay_start();
                    settings_cursor_mode = self.renderer.as_mut().unwrap().draw_settings(
                        self.settings_anim_progress,
                        self.settings_tab,
                        &self.faq_editor,
                        self.settings_scroll.current,
                        &self.ide_workspaces,
                        &self.ide_ignore_patterns,
                        &self.settings_ignore_editor,
                        self.settings_ignore_focused,
                        &mut self.settings_ignore_scroll_x,
                        self.settings_ide_scroll.current,
                        blink_alpha,
                        &mut self.ui_registry,
                    );
                    if settings_cursor_mode == 1 {
                        wants_pointer = true;
                    }
                }

                // Проверяем hover на зонах resize IDE-панелей — они требуют специальный курсор
                let mut ide_resize_cursor: Option<winit::window::CursorIcon> = None;
                if self.is_ide_mode && !self.show_welcome && !self.show_settings {
                    let r = self.renderer.as_ref().unwrap();
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let s = r.scale_factor;
                    let sb_w = 48.0 * s;

                    let panel_left_w = if self.ide_panel.any_top_open() {
                        self.ide_panel.left_width * s
                    } else {
                        0.0
                    };
                    let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    if panel_left_w > 0.0 {
                        let resize_x = sb_w + panel_left_w;
                        if (mx - resize_x).abs() < 6.0 * s && my >= 0.0 && my < wh - panel_bottom_h
                        {
                            ide_resize_cursor = Some(winit::window::CursorIcon::EwResize);
                        }
                    }
                    if panel_bottom_h > 0.0 && ide_resize_cursor.is_none() {
                        let resize_y = wh - panel_bottom_h;
                        if (my - resize_y).abs() < 6.0 * s && mx >= sb_w {
                            ide_resize_cursor = Some(winit::window::CursorIcon::NsResize);
                        }
                    }
                }

                let cursor_icon = if let Some(rc) = ide_resize_cursor {
                    rc
                } else if self.ide_panel.is_resizing_left {
                    winit::window::CursorIcon::EwResize
                } else if self.ide_panel.is_resizing_bottom {
                    winit::window::CursorIcon::NsResize
                } else if wants_pointer {
                    winit::window::CursorIcon::Pointer
                } else if !self.show_welcome {
                    let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
                    let window_height = self.window.as_ref().unwrap().inner_size().height as f32;
                    let max_scroll = self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .get_max_scroll(&self.editor, window_height);

                    let r = self.renderer.as_ref().unwrap();
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let padding = r.left_padding;
                    let minimap_w = r.minimap_width;
                    let s = r.scale_factor;
                    let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };

                    let diag_popup_hovered = self
                        .renderer
                        .as_ref()
                        .unwrap()
                        .last_diag_popup_rect
                        .map(|(rx, ry, rw, rh)| {
                            mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh
                        })
                        .unwrap_or(false);
                    let mut is_text = !diag_popup_hovered
                        && mx > padding
                        && mx < (window_width - minimap_w - scrollbar_w);

                    if r.max_scroll_x > 0.0 {
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        if my > wh - 14.0 * s {
                            is_text = false;
                        }
                    }

                    if self.scroll_x.is_dragging || self.scroll_y.is_dragging {
                        is_text = false;
                    }

                    if let Some((rx, ry, rw, rh)) = self.autocomplete_rect {
                        if mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh {
                            is_text = false;
                        }
                    }

                    let panel_bottom_h = if self.is_ide_mode && self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    if panel_bottom_h > 0.0 && my >= window_height - panel_bottom_h {
                        is_text = false;
                    }

                    if self.show_settings
                        || self.dialog_window.is_some()
                        || self.settings_anim_progress >= 1.5
                    {
                        is_text = settings_cursor_mode == 2;
                    }

                    if self.show_search && self.search_anim_y > -10.0 {
                        let search_w = 480.0 * s;
                        let search_h = 52.0 * s;
                        let search_x = window_width - minimap_w - scrollbar_w - search_w - 20.0 * s;
                        let input_x = search_x + 10.0 * s;
                        let input_y = self.search_anim_y + 11.0 * s;
                        let input_w = 260.0 * s;
                        let input_h = 30.0 * s;

                        if mx >= search_x
                            && mx <= search_x + search_w
                            && my >= self.search_anim_y
                            && my <= self.search_anim_y + search_h
                        {
                            if mx >= input_x
                                && mx <= input_x + input_w
                                && my >= input_y
                                && my <= input_y + input_h
                            {
                                is_text = true;
                            } else {
                                is_text = false;
                            }
                        }
                    }

                    if is_text {
                        winit::window::CursorIcon::Text
                    } else {
                        winit::window::CursorIcon::Default
                    }
                } else {
                    winit::window::CursorIcon::Default
                };

                if self.current_cursor != cursor_icon {
                    self.current_cursor = cursor_icon;
                    self.window.as_ref().unwrap().set_cursor(cursor_icon);
                }

                gl_surface.swap_buffers(gl_context).unwrap();
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.016);
        self.last_frame = now;

        let mut needs_redraw = false;

        if self.current_sticky_lines != self.target_sticky_lines {
            let old_len = self.current_sticky_lines.len();
            let new_len = self.target_sticky_lines.len();

            if new_len > old_len {
                self.sticky_anim_progress = 0.0;
                self.sticky_anim_is_adding = true;
                self.current_sticky_lines = self.target_sticky_lines.clone();
            } else if new_len < old_len {
                if self.sticky_anim_is_adding || self.sticky_anim_progress >= 1.0 {
                    self.sticky_anim_progress = 0.0;
                    self.sticky_anim_is_adding = false;
                }
            } else {
                self.sticky_anim_progress = 1.0;
                self.current_sticky_lines = self.target_sticky_lines.clone();
            }
            needs_redraw = true;
        }

        if self.sticky_anim_progress < 1.0 {
            self.sticky_anim_progress += dt * 6.0;
            if self.sticky_anim_progress >= 0.99 {
                self.sticky_anim_progress = 1.0;
                if !self.sticky_anim_is_adding {
                    self.current_sticky_lines = self.target_sticky_lines.clone();
                }
            }
            needs_redraw = true;
        }

        if self.autocomplete_active && self.autocomplete_scroll.update(dt) {
            needs_redraw = true;
        }

        if self.show_settings && self.settings_tab == 0 && self.settings_ide_scroll.update(dt) {
            self.window.as_ref().unwrap().request_redraw();
        }
        if self.show_settings && self.settings_tab == 4 && self.settings_scroll.update(dt) {
            needs_redraw = true;
        }

        if self.scroll_y.update(dt) {
            needs_redraw = true;
        }

        if self.scroll_x.update(dt) {
            needs_redraw = true;
        }

        if self.poll_file_tree() {
            needs_redraw = true;
        }

        // Watcher сигнализирует об изменениях на диске — обновляем дерево
        {
            let mut fs_changed = false;
            if let Some(rx) = &self.file_tree_notify_rx {
                while rx.try_recv().is_ok() {
                    fs_changed = true;
                }
            }
            if fs_changed {
                self.refresh_file_tree();
                needs_redraw = true;
            }
        }
        if self.ide_panel.explorer_scroll.update(dt) {
            needs_redraw = true;
        }
        if self.ide_panel.lsp_scroll_y.update(dt) {
            needs_redraw = true;
        }
        if self.ide_panel.lsp_scroll_x.update(dt) {
            needs_redraw = true;
        }
        for scroll in self.ide_panel.lsp_logs_scroll_y.values_mut() {
            if scroll.update(dt) {
                needs_redraw = true;
            }
        }
        for scroll in self.ide_panel.lsp_logs_scroll_x.values_mut() {
            if scroll.update(dt) {
                needs_redraw = true;
            }
        }

        if self.autocomplete_active && self.autocomplete_anim_progress < 1.0 {
            self.autocomplete_anim_progress += (1.0 - self.autocomplete_anim_progress) * 20.0 * dt;
            if self.autocomplete_anim_progress > 0.99 {
                self.autocomplete_anim_progress = 1.0;
            }
            needs_redraw = true;
        }

        let s = self
            .renderer
            .as_ref()
            .map(|r| r.scale_factor)
            .unwrap_or(1.0);
        let window_height = self.window.as_ref().unwrap().inner_size().height as f32;
        let h = (700.0 * s).min(window_height - 40.0 * s);
        let start_y = window_height + 100.0 * s;
        let open_y = (window_height - h) / 2.0;
        let target_y = if self.show_settings { open_y } else { start_y };

        let diff = target_y - self.settings_y;
        if diff.abs() > 1.5 {
            self.settings_y += diff * 10.0 * dt;
            let total_distance = (start_y - open_y).max(1.0);
            self.settings_anim_progress =
                ((start_y - self.settings_y) / total_distance).clamp(0.0, 1.0);
            needs_redraw = true;
        } else if !self.show_settings && self.settings_anim_progress > 0.0 {
            self.settings_y = start_y;
            self.settings_anim_progress = 0.0;
            needs_redraw = true;
        }

        let target_search_y = if self.show_search { 10.0 } else { -120.0 * s };
        let search_diff = target_search_y - self.search_anim_y;
        if search_diff.abs() > 1.5 {
            self.search_anim_y += search_diff * 20.0 * dt;
            needs_redraw = true;
        }

        if self.is_dragging && !self.scroll_y.is_dragging {
            if let Some(w) = self.window.as_ref() {
                let wh = w.inner_size().height as f32;
                let ww = w.inner_size().width as f32;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;
                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
                let padding = self.renderer.as_ref().unwrap().left_padding;

                let mut drag_scroll_delta_y = 0.0;
                if my < 0.0 {
                    drag_scroll_delta_y = my;
                } else if my > wh {
                    drag_scroll_delta_y = my - wh;
                }

                let mut drag_scroll_delta_x = 0.0;
                let view_right_edge = ww - minimap_w;
                if mx < padding {
                    drag_scroll_delta_x = mx - padding;
                } else if mx > view_right_edge {
                    drag_scroll_delta_x = mx - view_right_edge;
                }

                if drag_scroll_delta_y != 0.0 || drag_scroll_delta_x != 0.0 {
                    if drag_scroll_delta_y != 0.0 {
                        let drag_amount = drag_scroll_delta_y.abs();
                        let speed = (drag_amount.powi(2) * 0.15).clamp(70.0, 4500.0);
                        self.scroll_y.target += drag_scroll_delta_y.signum() * speed * dt;
                    }

                    if drag_scroll_delta_x != 0.0 {
                        let drag_amount = drag_scroll_delta_x.abs();
                        let speed = (drag_amount.powi(2) * 0.15).clamp(70.0, 4500.0);
                        self.scroll_x.target += drag_scroll_delta_x.signum() * speed * dt;
                    }

                    self.editor.set_cursor_at_pos(
                        mx,
                        my + self.scroll_y.current,
                        self.renderer.as_mut().unwrap(),
                        false,
                    );
                    needs_redraw = true;
                }
            }
        }

        if let Some(w) = self.window.as_ref() {
            let max_scroll_y = self
                .renderer
                .as_mut()
                .unwrap()
                .get_max_scroll(&self.editor, w.inner_size().height as f32);
            self.scroll_y.clamp_target(0.0, max_scroll_y);
            self.scroll_y.clamp_current(0.0, max_scroll_y);

            let max_scroll_x = self.renderer.as_ref().unwrap().max_scroll_x;
            self.scroll_x.clamp_target(0.0, max_scroll_x);
            self.scroll_x.clamp_current(0.0, max_scroll_x);
        }

        if let Some(rx) = &self.open_folder_rx {
            if let Ok(result) = rx.try_recv() {
                self.open_folder_rx = None;
                if let Some(path) = result {
                    self.ide_workspaces.push(path.clone());
                    self.ide_panel.file_tree_expanded.insert(path.clone());
                    self.refresh_file_tree();
                    self.start_file_watcher();
                    let w = self.window.as_ref().unwrap();
                    let maximized = w.is_maximized();
                    let (width, height) = if maximized {
                        (self.window_width, self.window_height)
                    } else {
                        let scale = w.scale_factor();
                        let size = w.inner_size().to_logical::<f64>(scale);
                        (size.width, size.height)
                    };
                    crate::save_config(&crate::Config {
                        window_width: width,
                        window_height: height,
                        maximized,
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    });
                }
            }
        }

        if let Some(rx) = &self.open_file_rx {
            if let Ok(result) = rx.try_recv() {
                self.open_file_rx = None;
                if let Some(path) = result {
                    self.load_file(path, true);
                }
            }
        }

        if let Some(rx) = &self.save_file_rx {
            if let Ok(result) = rx.try_recv() {
                self.save_file_rx = None;
                if let Some(path) = result {
                    self.file_path = Some(path.clone());
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    self.base_title = file_name.into_owned();

                    if let Some(e) = path.extension() {
                        self.file_extension = e.to_string_lossy().to_string();
                    } else {
                        self.file_extension = String::new();
                    }

                    self.add_recent_file(path);

                    if self.save_current_file() {
                        if let Some(w) = self.window.as_ref() {
                            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
                        }
                        self.highlighter.reset(
                            self.editor.version,
                            self.editor.get_full_text(),
                            self.file_extension.clone(),
                        );
                    }
                }
            }
        }

        if let Some(last_resize) = self.last_resize_time {
            if now.duration_since(last_resize).as_millis() > 150 {
                self.last_resize_time = None;
                needs_redraw = true;
            } else {
                needs_redraw = true;
            }
        }

        // LSP: опрашиваем события (диагностика, code actions) — раз в кадр, не блокирует
        let mut lsp_events = Vec::new();
        if self.is_ide_mode {
            if let Some(lsp) = &mut self.lsp {
                lsp_events = lsp.poll();
            }
        }

        for event in lsp_events {
            match event {
                crate::lsp::LspEvent::Diagnostics { .. } => {
                    // lsp.diagnostics обновлены внутри poll() автоматически
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
                crate::lsp::LspEvent::CodeActions {
                    request_id,
                    actions,
                } => {
                    // Проверяем: это ответ на Alt+Enter меню?
                    let is_for_menu = self
                        .lsp_actions_menu
                        .as_ref()
                        .and_then(|m| m.pending_request_id)
                        .map(|id| id == request_id)
                        .unwrap_or(false);

                    if is_for_menu {
                        // Добавляем code actions в начало меню
                        if let Some(menu) = &mut self.lsp_actions_menu {
                            let mut new_items: Vec<crate::app::LspActionItem> = actions
                                .into_iter()
                                .filter(|a| a.edit.is_some())
                                .map(crate::app::LspActionItem::CodeAction)
                                .collect();
                            new_items.extend(menu.items.drain(..));
                            menu.items = new_items;
                            menu.pending_request_id = None;
                        }
                        if let Some(w) = self.window.as_ref() {
                            w.request_redraw();
                        }
                    } else if self.pending_fix_all_id == Some(request_id) {
                        // Fix All из панели LSP серверов
                        self.pending_fix_all_id = None;
                        if let Some(action) = actions.into_iter().find(|a| a.edit.is_some()) {
                            if let (Some(edit), Some(path)) = (action.edit, self.file_path.clone())
                            {
                                let new_text = crate::lsp::apply_workspace_edit_to_text(
                                    &self.editor.get_full_text(),
                                    &edit,
                                    &path,
                                );
                                if new_text != self.editor.get_full_text() {
                                    self.apply_full_text_replacement(new_text);
                                }
                            }
                        }
                        if let Some(w) = self.window.as_ref() {
                            w.request_redraw();
                        }
                    }
                }
                crate::lsp::LspEvent::ServerReady => {}
                crate::lsp::LspEvent::StatusChanged { .. } => {}
                crate::lsp::LspEvent::Log { .. } => {} // Fix All ответ
            }
        }

        if self.is_ide_mode {
            if let Some(lsp) = &mut self.lsp {
                // Умная синхронизация без аллокаций каждый кадр.
                // Обновляем UI только если статус или логи реально изменились.
                let lsp_logs_len = lsp.server_logs.get("ruff").map(|l| l.len()).unwrap_or(0);
                let needs_update = self.ide_panel.lsp_servers.is_empty()
                    || self.ide_panel.lsp_servers[0].status != lsp.python_status
                    || self.ide_panel.lsp_servers[0].logs.len() != lsp_logs_len;

                if needs_update {
                    self.ide_panel.lsp_servers = lsp.servers_info();
                    // Синхронизируем Editor для логов (для выделения и копирования)
                    for info in &self.ide_panel.lsp_servers {
                        let new_text = info
                            .logs
                            .iter()
                            .map(|l| l.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let focused = self.ide_panel.lsp_logs_focused.as_deref() == Some(info.name);
                        let entry = self
                            .ide_panel
                            .lsp_log_editors
                            .entry(info.name.to_string())
                            .or_insert_with(|| crate::editor::Editor::new(new_text.len().max(512)));
                        // Пересоздаём только если текст изменился (иначе сбросим выделение)
                        if entry.get_full_text() != new_text {
                            let saved_cursor = if focused { Some(entry.cursor) } else { None };
                            let saved_anchor = if focused {
                                entry.selection_anchor
                            } else {
                                None
                            };
                            *entry = crate::editor::Editor::new(new_text.len().max(512));
                            let _ = entry.insert_str(&new_text);

                            entry.foldable_ranges_bytes.clear();
                            let mut byte_offset = 0;
                            for log in &info.logs {
                                for &(s, e) in &log.folds {
                                    entry.foldable_ranges_bytes.push((
                                        byte_offset + s,
                                        byte_offset + e,
                                        false,
                                    ));
                                }
                                byte_offset += log.text.len() + 1;
                            }
                            entry.rebuild_line_offsets();

                            for &(s, e, _) in &entry.foldable_ranges_bytes {
                                let sl = entry
                                    .line_offsets
                                    .partition_point(|&x| x <= s)
                                    .saturating_sub(1);
                                let el = entry
                                    .line_offsets
                                    .partition_point(|&x| x <= e)
                                    .saturating_sub(1);
                                if el > sl {
                                    entry.folded_lines.insert(sl);
                                    entry.folded_start_bytes.insert(entry.line_offsets[sl]);
                                }
                            }

                            if let Some(c) = saved_cursor {
                                entry.cursor = c.min(new_text.len());
                                entry.selection_anchor =
                                    saved_anchor.map(|a| a.min(new_text.len()));
                            } else {
                                // По умолчанию курсор в конце (хвост лога)
                                entry.cursor = new_text.len();
                                entry.selection_anchor = None;
                            }
                        }
                    }
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }
        }

        if self.highlighter.poll(self.editor.version) {
            self.apply_highlight_results();
            if self.autocomplete_active {
                self.update_autocomplete();
            }
            needs_redraw = true;
        }

        if self.is_focused {
            let blink_state = (now.duration_since(self.last_action).as_millis() / 500) % 2 == 0;
            if blink_state != self.last_blink_state {
                self.last_blink_state = blink_state;
                needs_redraw = true;
            }
        }

        let is_highlighting = !self.is_highlighted_once;

        if needs_redraw {
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if is_highlighting {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                now + std::time::Duration::from_millis(5),
            ));
        } else {
            let next_blink = self.last_action
                + std::time::Duration::from_millis(
                    (now.duration_since(self.last_action).as_millis() / 500 + 1) as u64 * 500,
                );
            event_loop.set_control_flow(ControlFlow::WaitUntil(next_blink));
        }
    }
}
