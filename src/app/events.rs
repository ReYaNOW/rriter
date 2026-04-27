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
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowId};

mod about;
mod source_hover;
use source_hover::*;

#[cfg_attr(coverage_nightly, coverage(off))]
fn save_state_and_exit(app: &App, event_loop: &ActiveEventLoop) {
    let w = app.window.as_ref().unwrap();
    let maximized = w.is_maximized();
    let (width, height) = if maximized {
        (app.window_width, app.window_height)
    } else {
        let scale = w.scale_factor();
        let size = w.inner_size().to_logical::<f64>(scale);
        (size.width, size.height)
    };
    crate::save_config(&crate::Config {
        window_width: width,
        window_height: height,
        maximized,
        ide_workspaces: app.ide_workspaces.clone(),
        ide_ignore_patterns: app.ide_ignore_patterns.clone(),
        enable_telemetry: crate::render_view::TELEMETRY_ENABLED
            .load(std::sync::atomic::Ordering::Relaxed),
    });
    if app.is_ide_mode {
        crate::save_panel_state(&app.ide_panel);
    }
    event_loop.exit();
}

impl ApplicationHandler for App {
    // Coverage rationale:
    // This is the winit/glutin/OpenGL bootstrap boundary. It creates OS window,
    // GL context, swapchain surface and renderer through external APIs. Editor
    // and input state logic stays testable in smaller helpers; this wrapper is
    // not useful as llvm-cov signal.
    #[cfg_attr(coverage_nightly, coverage(off))]
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
        self.window = Some(std::sync::Arc::new(window));
        self.gl_context = Some(gl_context);
        self.gl_surface = Some(gl_surface);

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
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
                                    save_state_and_exit(self, event_loop);
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
                                save_state_and_exit(self, event_loop);
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
                    save_state_and_exit(self, event_loop);
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
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_main_mouse_input(event_loop, state, button);
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

                // Очищаем UI registry перед новым кадром```
                self.ui_registry.clear();

                self.ide_panel.flat_diags.clear();
                if let Some(lsp) = &self.lsp {
                    if self.ide_panel.problems_tab == 0 {
                        if let Some(p) = &self.file_path {
                            let mut diags = lsp
                                .get_diagnostics(p)
                                .iter()
                                .enumerate()
                                .collect::<Vec<_>>();
                            diags.sort_by(|(_, a), (_, b)| {
                                a.start_line
                                    .cmp(&b.start_line)
                                    .then(a.start_col.cmp(&b.start_col))
                            });
                            for (i, _) in diags {
                                self.ide_panel.flat_diags.push((p.clone(), i));
                            }
                        }
                    } else {
                        let mut paths: Vec<_> = lsp.diagnostics.keys().collect();
                        paths.sort();
                        for p in paths {
                            let mut diags = lsp
                                .get_diagnostics(p)
                                .iter()
                                .enumerate()
                                .collect::<Vec<_>>();
                            if diags.is_empty() {
                                continue;
                            }
                            diags.sort_by(|(_, a), (_, b)| {
                                a.start_line
                                    .cmp(&b.start_line)
                                    .then(a.start_col.cmp(&b.start_col))
                            });

                            self.ide_panel.flat_diags.push(((*p).clone(), usize::MAX));

                            if !self.ide_panel.problems_collapsed.contains(p) {
                                for (i, _) in diags {
                                    self.ide_panel.flat_diags.push(((*p).clone(), i));
                                }
                            }
                        }
                    }
                }

                if let Some(log) = &mut self.pending_key_log {
                    if log.t_render.is_none() {
                        log.t_render = Some(std::time::Instant::now());
                    }
                }

                let (mut wants_pointer, target_sticky) = self.renderer.as_mut().unwrap().draw(
                    &mut self.editor,
                    &self.base_title,
                    self.file_path.as_ref(),
                    &self.tabs,
                    self.active_tab,
                    self.scroll_x.current,
                    self.scroll_y.current,
                    blink_alpha,
                    self.show_fps,
                    &self.highlighter.spans,
                    self.dialog_window.is_some(),
                    is_resizing,
                    &self.search_results,
                    self.search_current_idx,
                    self.show_search,
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
                    self.lsp.as_ref(),
                    &mut self.ui_registry,
                    self.tab_scroll.current.round(),
                    &self.highlighter.syntax_errors,
                );

                self.target_sticky_lines = target_sticky;

                // Продолжаем рендерить пока tooltip ещё не показан
                let diag_timer_active = crate::app::mouse::HOVER_STATE.with(|state| {
                    let state = state.borrow();
                    state.diag_hover_timer > 0.0 && state.diag_hover_timer < 0.2
                });
                if diag_timer_active {
                    self.window.as_ref().unwrap().request_redraw();
                }

                // Сбрасываем иконку копирования когда popup диагностики закрывается
                let no_hovered_diags =
                    crate::app::mouse::HOVER_STATE.with(|s| s.borrow().hovered_diags.is_empty());
                if no_hovered_diags {
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
                if let Some(mut menu) = self.lsp_actions_menu.clone() {
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    menu.menu_y += tab_bar_h;
                    let wants = self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .draw_lsp_actions_menu(&menu, blink_alpha);
                    if wants {
                        wants_pointer = true;
                    }
                }

                if self.autocomplete_active && !self.autocomplete_options.is_empty() {
                    let (cx, cy) = self.renderer.as_mut().unwrap().get_cursor_xy(&self.editor);
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    let render_scroll_y = self.scroll_y.current.round() - tab_bar_h;
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

                    // Используем фактическую ширину панели, а не проверку any_top_open()
                    // Потому что панель может быть открыта через bottom группу
                    let panel_left_w = self.ide_panel.left_width * s;
                    let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    let mut effective_bottom_h = panel_bottom_h;
                    if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                        && !self.ide_panel.terminal_focused
                    {
                        effective_bottom_h = 0.0;
                    }

                    if panel_left_w > 0.0 {
                        let resize_x = sb_w + panel_left_w;
                        if (mx - resize_x).abs() < 6.0 * s
                            && my >= 0.0
                            && my < wh - effective_bottom_h
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

                    let diag_popup_hovered = crate::app::mouse::HOVER_STATE
                        .with(|state| state.borrow().diag_rect)
                        .map(|(rx, ry, rw, rh, _, _, _)| {
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

                    let is_terminal_bottom = self.is_ide_mode
                        && self.ide_panel.slots.iter().any(|sl| {
                            sl.group == crate::app::PanelGroup::Bottom
                                && sl.open
                                && sl.id == crate::app::PanelId::Terminal
                        });

                    let is_transparent_terminal =
                        is_terminal_bottom && !self.ide_panel.terminal_focused;

                    if panel_bottom_h > 0.0 && my >= window_height - panel_bottom_h {
                        if !is_transparent_terminal {
                            is_text = false;
                        }
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

                    let hover_popup_hovered = crate::app::mouse::HOVER_STATE.with(|state| {
                        if let Some((x, y, w, h)) = state.borrow().rect {
                            mx >= x && mx <= x + w && my >= y && my <= y + h
                        } else {
                            false
                        }
                    });
                    if hover_popup_hovered {
                        winit::window::CursorIcon::Default
                    } else if is_text || self.ui_registry.wants_text() {
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

                if let Some(log) = self.pending_key_log.take() {
                    let now = std::time::Instant::now();
                    let t_total = now.duration_since(log.t0).as_secs_f64() * 1000.0;

                    let t_highlight = log.t_highlight.unwrap_or(log.t0);
                    let input_to_hl = t_highlight.duration_since(log.t0).as_secs_f64() * 1000.0;

                    let t_render = log.t_render.unwrap_or(t_highlight);
                    let hl_to_render = t_render.duration_since(t_highlight).as_secs_f64() * 1000.0;

                    let render_to_swap = now.duration_since(t_render).as_secs_f64() * 1000.0;

                    if crate::render_view::TELEMETRY_ENABLED
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        println!(
                            "Key: {:?} | Total: {:.2}ms (Input->HL: {:.2}ms, HL->RenderPrep: {:.2}ms, Render+Swap: {:.2}ms)",
                            log.key, t_total, input_to_hl, hl_to_render, render_to_swap
                        );
                    }
                }
            }
            _ => (),
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        about::about_to_wait(self, event_loop);
    }
}
