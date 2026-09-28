use crate::app::{App, PendingAction};
use glutin::context::PossiblyCurrentGlContext;
use glutin::surface::GlSurface;
use std::cell::RefCell;
use std::num::NonZeroU32;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;

pub(crate) mod about;
pub(crate) mod main_frame;
pub(crate) mod host_loop;
use host_loop::HostLoop;
mod window_runtime;
#[cfg(test)]
pub(crate) use about::file_watcher_disconnect_message;
mod source_hover;
pub(crate) use source_hover::apply_source_hover_response_to_state;
pub(crate) use source_hover::module_path_from_definition_path;
pub(crate) use source_hover::prepend_hover_module_path;
pub(crate) use source_hover::source_class_signature_from_definition_file;
pub(crate) use source_hover::source_function_signature_from_text;
pub(crate) use source_hover::source_hover_popup_for_editor;
use source_hover::*;

#[derive(Default)]
struct AutocompletePopupStats {
    frames: u32,
    total_ms: f64,
    list_ms: f64,
    refresh_ms: f64,
    layout_ms: f64,
    detail_draw_ms: f64,
    max_total_ms: f64,
    max_list_ms: f64,
    max_detail_draw_ms: f64,
    last_options: usize,
    last_detail_len: usize,
    last_detail_lines: usize,
}

struct AutocompleteFrameStats {
    last_print: Instant,
    last_frame: Option<Instant>,
    was_active: bool,
    opens: u32,
    frames: u32,
    anim_frames: u32,
    measured_gaps: u32,
    slow_gaps: u32,
    gap_ms: f64,
    render_ms: f64,
    swap_ms: f64,
    max_gap_ms: f64,
    max_render_ms: f64,
    max_swap_ms: f64,
    last_options: usize,
    last_detail_len: usize,
    last_anim: f32,
    max_vertices_len: usize,
    max_vertices_cap: usize,
    last_glyphs: usize,
    last_ui_glyphs: usize,
    popup: AutocompletePopupStats,
}

#[inline]
fn markdown_read_cursor_icon(
    wants_pointer: bool,
    popup_blocks_background: bool,
    ui_registry: &crate::ui_system::UiRegistry,
) -> winit::window::CursorIcon {
    if wants_pointer {
        winit::window::CursorIcon::Pointer
    } else if popup_blocks_background {
        winit::window::CursorIcon::Default
    } else if ui_registry.wants_text() {
        winit::window::CursorIcon::Text
    } else {
        winit::window::CursorIcon::Default
    }
}

impl Default for AutocompleteFrameStats {
    fn default() -> Self {
        Self {
            last_print: Instant::now(),
            last_frame: None,
            was_active: false,
            opens: 0,
            frames: 0,
            anim_frames: 0,
            measured_gaps: 0,
            slow_gaps: 0,
            gap_ms: 0.0,
            render_ms: 0.0,
            swap_ms: 0.0,
            max_gap_ms: 0.0,
            max_render_ms: 0.0,
            max_swap_ms: 0.0,
            last_options: 0,
            last_detail_len: 0,
            last_anim: 0.0,
            max_vertices_len: 0,
            max_vertices_cap: 0,
            last_glyphs: 0,
            last_ui_glyphs: 0,
            popup: AutocompletePopupStats::default(),
        }
    }
}

thread_local! {
    static AUTOCOMPLETE_STATS: RefCell<AutocompleteFrameStats> =
        RefCell::new(AutocompleteFrameStats::default());
}

fn autocomplete_log_enabled() -> bool {
    // Headless stdout is the protocol channel.
    crate::render_view::TELEMETRY_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
        && !crate::platform::is_headless()
}

fn autocomplete_frame_start(active: bool) -> (Option<Instant>, Option<Instant>) {
    if !autocomplete_log_enabled() {
        return (None, None);
    }
    let now = Instant::now();
    AUTOCOMPLETE_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        if !active {
            stats.was_active = false;
            stats.last_frame = None;
            return (None, None);
        }
        if active && !stats.was_active {
            stats.opens += 1;
            stats.last_frame = Some(now);
            stats.was_active = true;
            return (Some(now), None);
        }
        stats.was_active = active;
        let last = stats.last_frame.replace(now);
        (Some(now), last)
    })
}

pub(crate) fn reset_autocomplete_frame_stats() {
    if !autocomplete_log_enabled() {
        return;
    }
    AUTOCOMPLETE_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        stats.was_active = false;
        stats.last_frame = None;
    });
}

fn record_autocomplete_popup_perf(
    total_ms: f64,
    list_ms: f64,
    refresh_ms: f64,
    layout_ms: f64,
    detail_draw_ms: f64,
    options: usize,
    detail_len: usize,
    detail_lines: usize,
) {
    if !autocomplete_log_enabled() {
        return;
    }
    AUTOCOMPLETE_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        let popup = &mut stats.popup;
        popup.frames += 1;
        popup.total_ms += total_ms;
        popup.list_ms += list_ms;
        popup.refresh_ms += refresh_ms;
        popup.layout_ms += layout_ms;
        popup.detail_draw_ms += detail_draw_ms;
        popup.max_total_ms = popup.max_total_ms.max(total_ms);
        popup.max_list_ms = popup.max_list_ms.max(list_ms);
        popup.max_detail_draw_ms = popup.max_detail_draw_ms.max(detail_draw_ms);
        popup.last_options = options;
        popup.last_detail_len = detail_len;
        popup.last_detail_lines = detail_lines;
    });
}

fn record_autocomplete_frame_perf(
    frame_start: Instant,
    prev_frame: Option<Instant>,
    swap_start: Instant,
    swap_ms: f64,
    options: usize,
    detail_len: usize,
    anim: f32,
    vertices_len: usize,
    vertices_cap: usize,
    glyphs: usize,
    ui_glyphs: usize,
) {
    if !autocomplete_log_enabled() {
        return;
    }
    AUTOCOMPLETE_STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        stats.frames += 1;
        if anim < 1.0 {
            stats.anim_frames += 1;
        }
        if anim < 1.0 && let Some(prev_frame) = prev_frame {
            let gap_ms = frame_start.duration_since(prev_frame).as_secs_f64() * 1000.0;
            stats.measured_gaps += 1;
            stats.gap_ms += gap_ms;
            stats.max_gap_ms = stats.max_gap_ms.max(gap_ms);
            if gap_ms > 8.5 {
                stats.slow_gaps += 1;
            }
        }
        let render_ms = swap_start.duration_since(frame_start).as_secs_f64() * 1000.0;
        stats.render_ms += render_ms;
        stats.swap_ms += swap_ms;
        stats.max_render_ms = stats.max_render_ms.max(render_ms);
        stats.max_swap_ms = stats.max_swap_ms.max(swap_ms);
        stats.last_options = options;
        stats.last_detail_len = detail_len;
        stats.last_anim = anim;
        stats.max_vertices_len = stats.max_vertices_len.max(vertices_len);
        stats.max_vertices_cap = stats.max_vertices_cap.max(vertices_cap);
        stats.last_glyphs = glyphs;
        stats.last_ui_glyphs = ui_glyphs;

        if stats.last_print.elapsed().as_secs_f32() < 2.0 {
            return;
        }

        let frames = stats.frames.max(1) as f64;
        let gaps = stats.measured_gaps.max(1) as f64;
        let popup_frames = stats.popup.frames.max(1) as f64;
        let fps = if stats.measured_gaps == 0 {
            0.0
        } else {
            1000.0 / (stats.gap_ms / gaps).max(0.001)
        };
        println!(
            "Autocomplete frame: opens={} frames={} anim_frames={} slow_gaps={} fps~{:.0} opts={} detail={}B anim={:.3} avg gap={:.2}ms render={:.2}ms swap={:.2}ms max gap={:.2}ms render={:.2}ms swap={:.2}ms vertices={}/{} glyphs={}/{}",
            stats.opens,
            stats.frames,
            stats.anim_frames,
            stats.slow_gaps,
            fps,
            stats.last_options,
            stats.last_detail_len,
            stats.last_anim,
            stats.gap_ms / gaps,
            stats.render_ms / frames,
            stats.swap_ms / frames,
            stats.max_gap_ms,
            stats.max_render_ms,
            stats.max_swap_ms,
            stats.max_vertices_len,
            stats.max_vertices_cap,
            stats.last_glyphs,
            stats.last_ui_glyphs
        );
        println!(
            "Autocomplete perf: frames={} opts={} detail={}B/{}l avg total={:.2}ms list={:.2}ms refresh={:.2}ms layout={:.2}ms detail_draw={:.2}ms max total={:.2}ms list={:.2}ms detail_draw={:.2}ms",
            stats.popup.frames,
            stats.popup.last_options,
            stats.popup.last_detail_len,
            stats.popup.last_detail_lines,
            stats.popup.total_ms / popup_frames,
            stats.popup.list_ms / popup_frames,
            stats.popup.refresh_ms / popup_frames,
            stats.popup.layout_ms / popup_frames,
            stats.popup.detail_draw_ms / popup_frames,
            stats.popup.max_total_ms,
            stats.popup.max_list_ms,
            stats.popup.max_detail_draw_ms
        );

        let last_frame = stats.last_frame;
        let was_active = stats.was_active;
        let opens = stats.opens;
        *stats = AutocompleteFrameStats::default();
        stats.last_frame = last_frame;
        stats.was_active = was_active;
        stats.opens = opens;
    });
}

fn autocomplete_detail_placement(
    _list_rect: (f32, f32, f32, f32),
    _box_w: f32,
    _box_h: f32,
    _viewport_w: f32,
    _viewport_h: f32,
    _gap: f32,
    _margin: f32,
) -> i8 {
    2
}

impl App {
    /// Правый край поиска: Reader прижимает панель к своему scrollbar, Edit — к minimap.
    fn search_panel_scrollbar_x_for_mode(
        &self,
        window_w: f32,
        minimap_w: f32,
        scrollbar_w: f32,
        s: f32,
    ) -> f32 {
        let read_w = (self.markdown_mode() == crate::app::MarkdownMode::Read).then(|| {
            crate::render_view::markdown_read::markdown_read_scrollbar_width(
                self.markdown.read_scroll_bounds().unwrap_or(0.0),
                s,
            )
        });
        crate::render_view::search::search_panel_scrollbar_x(
            window_w,
            minimap_w,
            scrollbar_w,
            read_w,
        )
    }
}

impl ApplicationHandler<crate::ui_waker::AppWake> for App {
    // Coverage rationale: OS window, GL context, swapchain, and renderer
    // initialization are isolated in the window runtime boundary.
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        window_runtime::resume(self, event_loop);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(dw) = self.confirm_dialog.window() {
            if _id == dw.id() {
                match event {
                    WindowEvent::CloseRequested => {
                        self.cancel_pending_action();
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
                            self.begin_pending_action_save();
                        } else if btn_discard.is_hovered(mx, my) {
                            self.discard_pending_action_changes();
                        } else if btn_cancel.is_hovered(mx, my) {
                            self.cancel_pending_action();
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        self.renderer.as_mut().unwrap().last_mouse_x = position.x as f32;
                        self.renderer.as_mut().unwrap().last_mouse_y = position.y as f32;
                        dw.request_redraw();
                    }
                    WindowEvent::RedrawRequested => {
                        let redraw_result = (|| -> Result<(), String> {
                            let gl_context = self
                                .gl_context
                                .as_ref()
                                .ok_or_else(|| "GL context is unavailable".to_string())?;
                            let gl_surface = self
                                .confirm_dialog
                                .gl_surface()
                                .ok_or_else(|| "dialog GL surface is unavailable".to_string())?;
                            gl_context.make_current(gl_surface).map_err(|error| {
                                format!("failed to activate dialog GL surface: {error}")
                            })?;

                            let r = self
                                .renderer
                                .as_mut()
                                .ok_or_else(|| "renderer is unavailable".to_string())?;
                            let s = r.scale_factor;
                            r.resize((660.0 * s) as u32, (260.0 * s) as u32);

                            unsafe {
                                use glow::HasContext;
                                r.gl.clear_color(0.12, 0.13, 0.22, 1.0);
                                r.gl.clear(glow::COLOR_BUFFER_BIT);
                            }

                            r.draw_dialog_window(&self.base_title);
                            gl_surface.swap_buffers(gl_context).map_err(|error| {
                                format!("failed to present dialog frame: {error}")
                            })?;

                            let main_surface = self
                                .gl_surface
                                .as_ref()
                                .ok_or_else(|| "main GL surface is unavailable".to_string())?;
                            gl_context.make_current(main_surface).map_err(|error| {
                                format!("failed to restore main GL surface: {error}")
                            })?;
                            let mw = self
                                .window
                                .as_ref()
                                .ok_or_else(|| "main window is unavailable".to_string())?
                                .inner_size();
                            self.renderer
                                .as_mut()
                                .ok_or_else(|| "renderer is unavailable".to_string())?
                                .resize(mw.width, mw.height);
                            Ok(())
                        })();
                        if let Err(error) = redraw_result {
                            eprintln!("confirmation dialog disabled after GL error: {error}");
                            self.cancel_pending_action();
                        }
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
                if self.has_unsaved_changes() {
                    self.show_action_dialog(&HostLoop::Native(event_loop), PendingAction::Quit);
                } else {
                    window_runtime::save_state_and_exit(self, &HostLoop::Native(event_loop));
                }
            }
            WindowEvent::Focused(focused) => {
                self.is_focused = focused;
                self.modifiers = winit::keyboard::ModifiersState::empty();
                if focused {
                    self.render_suspended = false;
                    if let Some(r) = self.renderer.as_mut() {
                        r.suppress_popups_until_next_mouse_move();
                    }
                    if let Some(dw) = self.confirm_dialog.window() {
                        // НЕ вызываем focus_window() здесь.
                        // Это - главная причина "мерцания" при Alt+Tab, т.к. приложение
                        // начинает бороться с оконным менеджером за фокус.
                        // Вместо этого, фокус будет восстановлен при клике или нажатии
                        // клавиши на основное окно, что является более предсказуемым поведением.
                        dw.request_redraw();
                    }
                    self.window.as_ref().unwrap().request_redraw();
                } else {
                    self.autosave_current_file_if_dirty();
                    self.render_suspended = true;
                    self.last_frame = Instant::now();
                    self.close_autocomplete();
                    self.cancel_pointer_interactions();
                    crate::app::mouse::suppress_hover_popup_until_mouse_move(
                        self.renderer.as_mut(),
                    );
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.render_suspended = occluded;
                self.last_frame = Instant::now();
                if !occluded {
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // The native surface is still resized before the renderer; the rest is
                // shared with headless `scale` through `handle_main_scale_factor_changed`.
                if let Some(window) = self.window.as_ref() {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        self.gl_surface.as_ref().unwrap().resize(
                            self.gl_context.as_ref().unwrap(),
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        );
                    }
                }
                self.handle_main_scale_factor_changed(scale_factor);
            }
            WindowEvent::Resized(size) => {
                // Same split as `ScaleFactorChanged`: only the native surface stays here.
                if size.width > 0 && size.height > 0 {
                    let gl_context = self.gl_context.as_ref().unwrap();
                    let gl_surface = self.gl_surface.as_ref().unwrap();
                    gl_surface.resize(
                        gl_context,
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    );
                }
                self.handle_main_resized(size);
            }
            WindowEvent::ModifiersChanged(mod_state) => {
                self.modifiers = mod_state.state();
                if !self.modifiers.control_key() {
                    self.clear_ctrl_definition();
                }
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => match window_runtime::dropped_path_kind(&path) {
                Some(window_runtime::DroppedPathKind::File) => {
                    self.open_file_in_tab(path, true);
                }
                Some(window_runtime::DroppedPathKind::Directory) => {
                    self.apply_selected_workspace_folder(path);
                }
                None => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_main_mouse_wheel(delta);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_main_mouse_input(&HostLoop::Native(event_loop), state, button);
            }
            WindowEvent::CursorMoved { position, .. } => self.handle_main_cursor_moved(position),
            WindowEvent::CursorLeft { .. } => {
                if self.markdown.clear_code_copy_transient()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
                let markdown_read_selection = self.markdown_mode()
                    == crate::app::MarkdownMode::Read
                    && self.markdown.read_selecting
                    && !self.show_settings;
                if markdown_read_selection
                    || about::selection_drag_active_on_cursor_leave(
                        self.is_dragging,
                        self.show_settings,
                        self.ide_panel.is_dragging_terminal,
                        self.last_click_ui_id,
                    )
                {
                    let window_size = self.window.as_ref().map(|window| window.inner_size());
                    if let (Some(size), Some(renderer)) = (window_size, self.renderer.as_mut()) {
                        // Wayland CursorLeft carries no new outside position, so project the last
                        // in-surface position across its nearest edge for active selection scroll.
                        let (x, y) = about::project_cursor_outside_window_on_leave(
                            renderer.last_mouse_x,
                            renderer.last_mouse_y,
                            size.width as f32,
                            size.height as f32,
                        );
                        renderer.last_mouse_x = x;
                        renderer.last_mouse_y = y;
                    }
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.handle_main_ime_commit(&text);
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.last_blink_state = true;
            }
            WindowEvent::Ime(Ime::Enabled | Ime::Preedit(_, _)) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_main_keyboard_input(&HostLoop::Native(event_loop), key_event);
            }
            WindowEvent::RedrawRequested => {
                if self.render_suspended {
                    event_loop.set_control_flow(ControlFlow::Wait);
                    return;
                }

                if !self.is_ready {
                    unsafe {
                        use glow::HasContext;
                        let gl = &self.renderer.as_ref().unwrap().gl;
                        gl.clear_color(self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                    }
                    if !self.present_main_surface() {
                        event_loop.set_control_flow(ControlFlow::Wait);
                        return;
                    }
                    crate::platform::finish_present();
                    self.renderer
                        .as_mut()
                        .unwrap()
                        .record_presented_frame(self.show_fps, Instant::now());

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

                let outcome = self.render_main_frame();

                let present_start = Instant::now();
                if !self.present_main_surface() {
                    event_loop.set_control_flow(ControlFlow::Wait);
                    return;
                }
                crate::platform::finish_present();
                let present_elapsed = present_start.elapsed().as_secs_f32();
                self.renderer
                    .as_mut()
                    .unwrap()
                    .record_presented_frame(self.show_fps, Instant::now());
                if crate::render_view::TELEMETRY_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
                {
                    crate::render_view::record_swap_telemetry(
                        present_elapsed,
                        !self.scroll_y.is_settled() || !self.scroll_x.is_settled(),
                    );
                }

                self.finish_main_frame(outcome);
            }
            _ => (),
        }
    }

    /// A background task delivered a result. Nothing to do here: winit runs
    /// `about_to_wait` after this batch, and that pass drains the channels.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: crate::ui_waker::AppWake) {}

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        about::about_to_wait(self, &HostLoop::Native(event_loop));
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Covers native application termination such as macOS Cmd+Q and OS
        // session shutdown, which do not necessarily emit CloseRequested.
        window_runtime::persist_state_and_shutdown(self);
    }
}

#[cfg(test)]
mod tests {
    use super::{autocomplete_detail_placement, markdown_read_cursor_icon};
    use crate::ui_system::{UiId, UiRegistry};

    #[test]
    fn markdown_read_cursor_uses_registry_surface_and_pointer_priority() {
        let mut registry = UiRegistry::new();
        assert!(registry.register_text_region(
            UiId::MarkdownReadBody,
            10.0,
            20.0,
            200.0,
            100.0,
            10.0,
            40.0,
        ));
        assert_eq!(
            markdown_read_cursor_icon(false, false, &registry),
            winit::window::CursorIcon::Text
        );
        assert_eq!(
            markdown_read_cursor_icon(false, true, &registry),
            winit::window::CursorIcon::Default
        );
        assert_eq!(
            markdown_read_cursor_icon(true, true, &registry),
            winit::window::CursorIcon::Pointer
        );

        assert!(registry.register_blocker(UiId::StatusBar, 0.0, 0.0, 300.0, 200.0, 10.0, 40.0,));
        assert_eq!(
            markdown_read_cursor_icon(false, false, &registry),
            winit::window::CursorIcon::Default
        );
    }

    #[test]
    fn autocomplete_detail_placement_stays_below_completion_window() {
        assert_eq!(
            autocomplete_detail_placement(
                (20.0, 40.0, 80.0, 120.0),
                150.0,
                90.0,
                400.0,
                300.0,
                8.0,
                4.0
            ),
            2
        );
        assert_eq!(
            autocomplete_detail_placement(
                (260.0, 120.0, 80.0, 120.0),
                150.0,
                90.0,
                400.0,
                300.0,
                8.0,
                4.0
            ),
            2
        );
        assert_eq!(
            autocomplete_detail_placement(
                (120.0, 140.0, 180.0, 80.0),
                150.0,
                90.0,
                320.0,
                300.0,
                8.0,
                4.0
            ),
            2
        );
        assert_eq!(
            autocomplete_detail_placement(
                (120.0, 30.0, 180.0, 80.0),
                150.0,
                90.0,
                320.0,
                300.0,
                8.0,
                4.0
            ),
            2
        );
    }
}
