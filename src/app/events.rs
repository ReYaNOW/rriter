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

mod window_runtime;
mod about;
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
    crate::render_view::TELEMETRY_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
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

impl ApplicationHandler for App {
    // Coverage rationale: OS window, GL context, swapchain, and renderer
    // initialization are isolated in the window runtime boundary.
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        window_runtime::resume(self, event_loop);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(dw) = self.dialog_window.as_ref() {
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
                                .dialog_gl_surface
                                .as_ref()
                                .ok_or_else(|| "dialog GL surface is unavailable".to_string())?;
                            gl_context
                                .make_current(gl_surface)
                                .map_err(|error| format!("failed to activate dialog GL surface: {error}"))?;

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
                            gl_surface
                                .swap_buffers(gl_context)
                                .map_err(|error| format!("failed to present dialog frame: {error}"))?;

                            let main_surface = self
                                .gl_surface
                                .as_ref()
                                .ok_or_else(|| "main GL surface is unavailable".to_string())?;
                            gl_context
                                .make_current(main_surface)
                                .map_err(|error| format!("failed to restore main GL surface: {error}"))?;
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
                            self.close_dialog();
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
                    self.show_action_dialog(event_loop, PendingAction::Quit);
                } else {
                    window_runtime::save_state_and_exit(self, event_loop);
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
                    if let Some(dw) = self.dialog_window.as_ref() {
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
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.update_scale_factor(scale_factor as f32);
                    renderer.last_editor_version_for_scroll_x = u64::MAX;
                }
                if let Some(window) = self.window.as_ref() {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        self.gl_surface.as_ref().unwrap().resize(
                            self.gl_context.as_ref().unwrap(),
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        );
                        self.renderer.as_mut().unwrap().resize(size.width, size.height);
                    }
                    window.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    self.render_suspended = true;
                    self.last_frame = Instant::now();
                } else {
                    self.render_suspended = false;
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
            WindowEvent::ModifiersChanged(mod_state) => {
                self.modifiers = mod_state.state();
                if !self.modifiers.control_key() {
                    self.clear_ctrl_definition();
                }
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => {
                match window_runtime::dropped_path_kind(&path) {
                    Some(window_runtime::DroppedPathKind::File) => {
                        self.open_file_in_tab(path, true);
                    }
                    Some(window_runtime::DroppedPathKind::Directory) => {
                        self.apply_selected_workspace_folder(path);
                    }
                    None => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_main_mouse_wheel(delta);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_main_mouse_input(event_loop, state, button);
            }
            WindowEvent::CursorMoved { position, .. } => self.handle_main_cursor_moved(position),
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
                self.handle_main_keyboard_input(event_loop, key_event);
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

                let (autocomplete_frame_start, autocomplete_prev_frame) =
                    autocomplete_frame_start(self.autocomplete_active);

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
                let query_diagnostics = self.tabs.get(self.active_tab).and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::DatabaseQuery(meta, state) => Some((
                        std::path::PathBuf::from(format!(
                            "SQL-консоль · {}",
                            meta.database_name
                        )),
                        state.editor_diagnostics.clone(),
                    )),
                    _ => None,
                });
                if let Some((path, diagnostics)) = query_diagnostics {
                    self.ide_panel.query_problem_path = Some(path.clone());
                    self.ide_panel.query_problem_diagnostics = diagnostics;
                    if self.ide_panel.problems_tab == 1 {
                        self.ide_panel.flat_diags.push((path.clone(), usize::MAX));
                    }
                    if self.ide_panel.problems_tab == 0
                        || !self.ide_panel.problems_collapsed.contains(&path)
                    {
                        self.ide_panel.flat_diags.extend(
                            (0..self.ide_panel.query_problem_diagnostics.len())
                                .map(|index| (path.clone(), index)),
                        );
                    }
                } else {
                    self.ide_panel.query_problem_path = None;
                    self.ide_panel.query_problem_diagnostics.clear();
                }
                if let Some(lsp) = &self.lsp {
                    if self.ide_panel.problems_tab == 0 {
                        if self.ide_panel.query_problem_path.is_none()
                            && let Some(path) = &self.file_path
                        {
                            let mut diagnostics = lsp.diagnostic_entries_for_path(path);
                            diagnostics.sort_by(|(_, left), (_, right)| {
                                left.start_line
                                    .cmp(&right.start_line)
                                    .then(left.start_col.cmp(&right.start_col))
                            });
                            self.ide_panel.flat_diags.extend(
                                diagnostics.into_iter().map(|(index, _)| (path.clone(), index)),
                            );
                        }
                    } else {
                        for path in lsp.diagnostic_paths() {
                            let mut diagnostics = lsp.diagnostic_entries_for_path(path);
                            if diagnostics.is_empty() {
                                continue;
                            }
                            diagnostics.sort_by(|(_, left), (_, right)| {
                                left.start_line
                                    .cmp(&right.start_line)
                                    .then(left.start_col.cmp(&right.start_col))
                            });
                            self.ide_panel.flat_diags.push(((*path).clone(), usize::MAX));
                            if !self.ide_panel.problems_collapsed.contains(path) {
                                self.ide_panel.flat_diags.extend(
                                    diagnostics
                                        .into_iter()
                                        .map(|(index, _)| ((*path).clone(), index)),
                                );
                            }
                        }
                    }
                }

                if let Some(log) = &mut self.pending_key_log {
                    if log.t_render.is_none() {
                        log.t_render = Some(std::time::Instant::now());
                    }
                }

                let ctrl_definition_range = self.ctrl_definition_highlight_range();
                let python_inlay_hints = if self.python_inlay_hint_path.as_ref()
                    == self.file_path.as_ref()
                    && self.python_inlay_hint_version == self.editor.version
                {
                    self.python_inlay_hints.as_slice()
                } else {
                    &[]
                };
                let closing_hints = if self.file_extension == "dart"
                    && self.closing_hint_state.revision() == self.editor.version
                {
                    self.closing_hint_state.hints()
                } else {
                    &[]
                };
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
                    ctrl_definition_range,
                    python_inlay_hints,
                    closing_hints,
                    &self.ide_workspaces,
                    self.readonly_notice_until
                        .is_some_and(|until| std::time::Instant::now() < until),
                    self.inline_git_popup.as_ref(),
                );

                self.target_sticky_lines = target_sticky;

                // Продолжаем рендерить пока tooltip ещё не показан
                let diag_timer_active = crate::app::mouse::HOVER_STATE.with(|state| {
                    let state = state.borrow();
                    state.diag_hover_timer > 0.0 && state.diag_hover_timer < 0.2
                });
                let git_tooltip_waiting = self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.git_tooltip_waiting);
                if diag_timer_active || git_tooltip_waiting {
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
                    let geometry = crate::render_view::search::search_panel_geometry(scrollbar_x, s);
                    let search_h = 52.0 * s;
                    if mx >= geometry.x
                        && mx <= geometry.x + geometry.w
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
                if let Some(menu) = self.lsp_actions_menu.clone() {
                    let wants = self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .draw_lsp_actions_menu(&menu, blink_alpha);
                    if wants {
                        wants_pointer = true;
                    }
                }

                if self.autocomplete_active {
                    let perf_enabled = autocomplete_log_enabled();
                    let perf_total_start = perf_enabled.then(Instant::now);
                    let mut perf_list_ms = 0.0;
                    let mut perf_refresh_ms = 0.0;
                    let mut perf_layout_ms = 0.0;
                    let mut perf_detail_draw_ms = 0.0;
                    let mut perf_detail_len = 0usize;
                    let mut perf_detail_lines = 0usize;
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        self.active_tab_is_database_query(),
                        s,
                    );
                    let render_scroll_y = self.scroll_y.current.round() - tab_bar_h;
                    let (cx, cy) = self.renderer.as_mut().unwrap().get_cursor_xy(&self.editor);
                    let (anchor_x, anchor_y) = *self
                        .autocomplete_anchor
                        .get_or_insert((cx, cy - render_scroll_y));
                    let perf_list_start = perf_enabled.then(Instant::now);
                    let rect = self.renderer.as_mut().unwrap().draw_autocomplete(
                        anchor_x,
                        anchor_y,
                        &self.autocomplete_options,
                        self.autocomplete_mode,
                        self.autocomplete_selected_idx,
                        self.autocomplete_anim_progress,
                        self.autocomplete_scroll.current,
                        self.autocomplete_hovered_idx,
                        self.autocomplete_min_width,
                    );
                    if let Some(start) = perf_list_start {
                        perf_list_ms = start.elapsed().as_secs_f64() * 1000.0;
                    }
                    self.autocomplete_min_width = self.autocomplete_min_width.max(rect.2);
                    self.autocomplete_rect = Some(rect);
                    if rect.2 > 0.0
                        && rect.3 > 0.0
                        && mx >= rect.0
                        && mx <= rect.0 + rect.2
                        && my >= rect.1
                        && my <= rect.1 + rect.3
                    {
                        crate::app::mouse::clear_hover_popup(self.renderer.as_mut());
                    }
                    let perf_refresh_start = perf_enabled.then(Instant::now);
                    if self.autocomplete_detail_popup.is_none()
                        && self
                            .autocomplete_options
                            .get(self.autocomplete_selected_idx)
                            .and_then(|(item, _)| item.detail.as_deref())
                            .is_some_and(|detail| !detail.trim().is_empty())
                    {
                        self.refresh_autocomplete_detail_popup();
                    }
                    if let Some(start) = perf_refresh_start {
                        perf_refresh_ms = start.elapsed().as_secs_f64() * 1000.0;
                    }
                    let detail_anim_progress = self.autocomplete_anim_progress.clamp(0.0, 1.0);
                    let detail_opacity_p = ((detail_anim_progress - 0.55) / 0.30).clamp(0.0, 1.0);
                    let detail_opacity =
                        detail_opacity_p * detail_opacity_p * (3.0 - 2.0 * detail_opacity_p);
                    if detail_anim_progress > 0.0 && rect.3 > 0.0 {
                        if let Some(mut popup) = self.autocomplete_detail_popup.take() {
                            if perf_enabled {
                                perf_detail_len = popup.text.len();
                                perf_detail_lines = popup.text.lines().count();
                            }
                            let (rx, ry, rw, rh) = rect;
                            let perf_layout_start = perf_enabled.then(Instant::now);
                            let (natural_w, natural_h, max_h) = {
                                let r = self.renderer.as_mut().unwrap();
                                let pad = 12.0 * r.scale_factor;
                                let line_h = 22.0 * r.scale_factor;
                                let gap = 16.0 * r.scale_factor;
                                let margin = 4.0 * r.scale_factor;
                                let min_h = line_h + pad * 2.0;
                                let available_below = r.height - (ry + rh + gap) - margin;
                                let detail_cap_h = line_h * 6.0 + pad * 2.0;
                                let max_h = (r.height * 0.28)
                                    .min(detail_cap_h)
                                    .min(available_below)
                                    .max(min_h);
                                let max_text_w = (r.width - 80.0 * r.scale_factor)
                                    .min(820.0 * r.scale_factor)
                                    .max(320.0 * r.scale_factor);
                                let cache_valid =
                                    popup.layout_cache.as_ref().is_some_and(|cache| {
                                        cache.scale_factor == r.scale_factor
                                            && cache.max_text_w == max_text_w
                                            && cache.span_count == popup.spans.len()
                                            && cache.text_len == popup.text.len()
                                    });
                                if !cache_valid {
                                    popup.layout_cache = Some(
                                        r.build_hover_popup_layout(&popup, max_text_w, line_h),
                                    );
                                }
                                let box_w = popup
                                    .layout_cache
                                    .as_ref()
                                    .map(|layout| layout.max_line_w + pad * 2.0)
                                    .unwrap_or(320.0 * r.scale_factor);
                                let box_h = popup
                                    .layout_cache
                                    .as_ref()
                                    .map(|layout| layout.total_text_h + pad * 2.0)
                                    .unwrap_or(120.0 * r.scale_factor);
                                (box_w, box_h, max_h)
                            };
                            let (box_w, box_h) =
                                self.stable_autocomplete_detail_size(natural_w, natural_h, max_h);
                            let detail_byte_offset = self.active_autocomplete_detail_byte_offset();
                            let detail_phys_line = self
                                .active_autocomplete_detail_editor()
                                .line_offsets
                                .partition_point(|&o| o <= detail_byte_offset)
                                .saturating_sub(1);
                            let (popup_x, popup_y, line_top_y) = {
                                let r = self.renderer.as_mut().unwrap();
                                let gap = 16.0 * r.scale_factor;
                                let margin = 4.0 * r.scale_factor;
                                let placement =
                                    *self.autocomplete_detail_placement.get_or_insert_with(|| {
                                        autocomplete_detail_placement(
                                            (rx, ry, rw, rh),
                                            box_w,
                                            box_h,
                                            r.width,
                                            r.height,
                                            gap,
                                            margin,
                                        )
                                    });
                                let clamp_x = |value: f32| {
                                    value
                                        .max(margin)
                                        .min((r.width - box_w - margin).max(margin))
                                };
                                let (popup_x, popup_y) = match placement {
                                    1 => (clamp_x(rx + rw + gap), ry),
                                    -1 => (clamp_x(rx - gap - box_w), ry),
                                    2 => (clamp_x(rx), ry + rh + gap),
                                    _ => (clamp_x(rx), (ry - gap - box_h).max(margin)),
                                };
                                let vis_line_idx =
                                    r.phys_to_visual.get(detail_phys_line).copied().unwrap_or(0)
                                        as f32;
                                (
                                    popup_x,
                                    popup_y,
                                    vis_line_idx * r.line_height - render_scroll_y,
                                )
                            };
                            if let Some(start) = perf_layout_start {
                                perf_layout_ms = start.elapsed().as_secs_f64() * 1000.0;
                            }
                            popup.byte_offset = detail_byte_offset;
                            popup.anchor_x = popup_x;
                            popup.anchor_y = popup_y;
                            popup.offset_x = Some(0.0);
                            popup.offset_y = Some(popup_y - line_top_y);
                            popup.anim_progress = detail_anim_progress;
                            let selection = self.autocomplete_detail_selection();
                            let use_api_detail_editor = self.api_mock_completion_focus().is_some();
                            let detail_editor = if use_api_detail_editor {
                                &self.ide_panel.api.input_editor
                            } else {
                                &self.editor
                            };
                            let renderer = self.renderer.as_mut().unwrap();
                            let ui_registry = &mut self.ui_registry;
                            let perf_detail_draw_start = perf_enabled.then(Instant::now);
                            let (bx, by, bw, bh, max_scroll) = renderer.draw_hover_popup(
                                &mut popup,
                                None,
                                selection,
                                detail_editor,
                                ui_registry,
                                mx,
                                my,
                                render_scroll_y,
                                &mut wants_pointer,
                                detail_opacity,
                                Some((box_w, box_h)),
                                None,
                            );
                            if let Some(start) = perf_detail_draw_start {
                                perf_detail_draw_ms = start.elapsed().as_secs_f64() * 1000.0;
                            }
                            self.autocomplete_detail_rect = Some((bx, by, bw, bh));
                            if mx >= bx && mx <= bx + bw && my >= by && my <= by + bh {
                                crate::app::mouse::clear_hover_popup(self.renderer.as_mut());
                            }
                            self.autocomplete_detail_max_scroll = max_scroll;
                            self.autocomplete_detail_popup = Some(popup);
                        } else {
                            self.autocomplete_detail_rect = None;
                            self.autocomplete_detail_max_scroll = 0.0;
                        }
                    } else {
                        self.autocomplete_detail_rect = None;
                        self.autocomplete_detail_max_scroll = 0.0;
                    }
                    if let Some(start) = perf_total_start {
                        record_autocomplete_popup_perf(
                            start.elapsed().as_secs_f64() * 1000.0,
                            perf_list_ms,
                            perf_refresh_ms,
                            perf_layout_ms,
                            perf_detail_draw_ms,
                            self.autocomplete_options.len(),
                            perf_detail_len,
                            perf_detail_lines,
                        );
                    }
                    if self.autocomplete_hovered_idx.is_some() {
                        wants_pointer = true;
                    }
                } else {
                    self.autocomplete_rect = None;
                    self.autocomplete_detail_rect = None;
                }

                let popup_blocks_background = self.popup_blocks_background_at(mx, my);
                if popup_blocks_background {
                    self.ui_registry.reset_cursor_state();
                    if self.autocomplete_window_contains(mx, my) {
                        wants_pointer = self.autocomplete_hovered_idx.is_some();
                    } else {
                        wants_pointer = false;
                    }
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
                        &self.tool_paths,
                        &self.tool_installer,
                        &self.dart_settings,
                        &self.dart_tool_state,
                        self.ide_panel
                            .lsp_servers
                            .iter()
                            .find(|server| server.name == "dart")
                            .map(|server| server.status),
                        self.ide_panel.database.settings(),
                        &mut self.ui_registry,
                    );
                    if settings_cursor_mode == 1 {
                        wants_pointer = true;
                    }
                }

                // Проверяем hover на зонах resize IDE-панелей — они требуют специальный курсор
                let mut ide_resize_cursor: Option<winit::window::CursorIcon> = None;
                if self.is_ide_mode
                    && !self.show_welcome
                    && !self.show_settings
                    && !popup_blocks_background
                    && !self.database_blocking_modal_open()
                {
                    let r = self.renderer.as_ref().unwrap();
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let s = r.scale_factor;
                    let sb_w = 48.0 * s;

                    let panel_left_w = self.ide_panel.visible_left_width(s);
                    let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    let effective_bottom_h = if self.ide_panel.bottom_terminal_is_transparent() {
                        0.0
                    } else {
                        panel_bottom_h
                    };

                    if panel_left_w > 0.0 {
                        let resize_x = sb_w + panel_left_w;
                        if (mx - resize_x).abs() < 3.0 * s
                            && my >= 0.0
                            && my < wh - effective_bottom_h
                        {
                            ide_resize_cursor = Some(winit::window::CursorIcon::EwResize);
                        }
                    }
                    if panel_bottom_h > 0.0 && ide_resize_cursor.is_none() {
                        let resize_y =
                            crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s);
                        if (my - resize_y).abs() < 6.0 * s && mx >= sb_w {
                            ide_resize_cursor = Some(winit::window::CursorIcon::NsResize);
                        }
                    }
                    if ide_resize_cursor.is_none()
                        && self.ui_registry.find_at(mx, my)
                            == Some(crate::ui_system::UiId::GitGraphResize)
                    {
                        ide_resize_cursor = Some(winit::window::CursorIcon::NsResize);
                    }
                    let query_results_resizing =
                        self.tabs.get(self.active_tab).is_some_and(|tab| {
                            matches!(
                                &tab.kind,
                                crate::app::EditorTabKind::DatabaseQuery(_, state)
                                    if state.result_view.is_resizing_height
                            )
                        });
                    if ide_resize_cursor.is_none()
                        && (query_results_resizing
                            || self.ui_registry.find_at(mx, my)
                                == Some(crate::ui_system::UiId::DatabaseQueryResultResize))
                    {
                        ide_resize_cursor = Some(winit::window::CursorIcon::NsResize);
                    }
                }

                let cursor_icon = if self.database_blocking_modal_open() {
                    let (mx, my) = self.renderer.as_ref().map_or((-1.0, -1.0), |renderer| {
                        (renderer.last_mouse_x, renderer.last_mouse_y)
                    });
                    match self.ui_registry.find_overlay_at(mx, my) {
                        Some(crate::ui_system::UiId::DatabaseDialogField(_))
                        | Some(crate::ui_system::UiId::DatabaseTableCellEditor)
                        | Some(crate::ui_system::UiId::DatabaseTableModalInput) => {
                            winit::window::CursorIcon::Text
                        }
                        Some(
                            crate::ui_system::UiId::DatabaseDialogBackdrop
                            | crate::ui_system::UiId::DatabaseDialogBody
                            | crate::ui_system::UiId::DatabaseTableModalBody
                            | crate::ui_system::UiId::DatabaseQueryReviewBackdrop
                            | crate::ui_system::UiId::DatabaseQueryReviewBody
                            | crate::ui_system::UiId::DatabaseQueryResultBody
                            | crate::ui_system::UiId::DatabaseQueryReviewMessagesBody,
                        )
                        | None => winit::window::CursorIcon::Default,
                        Some(_) => winit::window::CursorIcon::Pointer,
                    }
                } else if let Some(rc) = ide_resize_cursor {
                    rc
                } else if self.ide_panel.is_resizing_left {
                    winit::window::CursorIcon::EwResize
                } else if self.ide_panel.is_resizing_bottom || self.ide_panel.git.graph_resizing {
                    winit::window::CursorIcon::NsResize
                } else if self.ui_registry.find_at(
                    self.renderer.as_ref().unwrap().last_mouse_x,
                    self.renderer.as_ref().unwrap().last_mouse_y,
                ).is_some_and(|id| matches!(id, crate::ui_system::UiId::DatabaseTableColumnResize(_) | crate::ui_system::UiId::DatabaseQueryColumnResize(_))) {
                    winit::window::CursorIcon::EwResize
                } else if self.api_python_runtime_overlay_active() {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    match self
                        .ui_registry
                        .find_overlay_at(mx, my)
                        .filter(|id| crate::app::App::ui_id_is_api_python_runtime_overlay(*id))
                    {
                        Some(crate::ui_system::UiId::ApiMockPythonUvPathInput)
                        | Some(crate::ui_system::UiId::ApiMockPythonCustomPathInput) => {
                            winit::window::CursorIcon::Text
                        }
                        Some(_) => winit::window::CursorIcon::Pointer,
                        None => winit::window::CursorIcon::Default,
                    }
                } else if self.ide_panel.project_search.help_open {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    match self.ui_registry.find_overlay_at(mx, my) {
                        Some(crate::ui_system::UiId::ProjectSearchHelp) => {
                            winit::window::CursorIcon::Pointer
                        }
                        _ => winit::window::CursorIcon::Default,
                    }
                } else if self.ide_panel.database.context_menu.is_some() {
                    let (mx, my) = {
                        let renderer = self.renderer.as_ref().unwrap();
                        (renderer.last_mouse_x, renderer.last_mouse_y)
                    };
                    crate::app::context_menu::context_menu_cursor(
                        self.ui_registry.find_overlay_at(mx, my),
                    )
                } else if self.ide_panel.file_tree_context_menu.is_some() {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    crate::app::file_tree::file_tree_context_menu_cursor(
                        self.ui_registry.find_overlay_at(mx, my),
                    )
                } else if self.file_tree_modal_overlay_active() {
                    let (mx, my) = {
                        let r = self.renderer.as_ref().unwrap();
                        (r.last_mouse_x, r.last_mouse_y)
                    };
                    match self
                        .ui_registry
                        .find_at(mx, my)
                        .filter(|id| crate::app::App::ui_id_is_file_tree_overlay(*id))
                    {
                        Some(crate::ui_system::UiId::FileTreeCreateInput)
                        | Some(crate::ui_system::UiId::FileTreeRenameInput) => {
                            winit::window::CursorIcon::Text
                        }
                        Some(_) => winit::window::CursorIcon::Pointer,
                        None => winit::window::CursorIcon::Default,
                    }
                } else if let Some(id) = {
                    let r = self.renderer.as_ref().unwrap();
                    self.ui_registry
                        .find_at(r.last_mouse_x, r.last_mouse_y)
                        .filter(|id| {
                            matches!(
                                id,
                                crate::ui_system::UiId::InlineGitPanelBody
                                    | crate::ui_system::UiId::InlineGitPrevHunk
                                    | crate::ui_system::UiId::InlineGitNextHunk
                                    | crate::ui_system::UiId::InlineGitRollbackHunk
                            )
                        })
                } {
                    match id {
                        crate::ui_system::UiId::InlineGitPrevHunk
                        | crate::ui_system::UiId::InlineGitNextHunk
                        | crate::ui_system::UiId::InlineGitRollbackHunk => {
                            winit::window::CursorIcon::Pointer
                        }
                        _ => winit::window::CursorIcon::Default,
                    }
                } else if self.active_tab_is_api_client() {
                    if self.ui_registry.wants_text() {
                        winit::window::CursorIcon::Text
                    } else if wants_pointer {
                        winit::window::CursorIcon::Pointer
                    } else {
                        winit::window::CursorIcon::Default
                    }
                } else if self.active_tab_is_database_table() {
                    if self.ui_registry.wants_text() {
                        winit::window::CursorIcon::Text
                    } else if wants_pointer {
                        winit::window::CursorIcon::Pointer
                    } else {
                        winit::window::CursorIcon::Default
                    }
                } else if wants_pointer {
                    winit::window::CursorIcon::Pointer
                } else if popup_blocks_background {
                    winit::window::CursorIcon::Default
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

                    let is_transparent_terminal =
                        self.is_ide_mode && self.ide_panel.bottom_terminal_is_transparent();

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
                        let scrollbar_x = window_width - minimap_w - scrollbar_w;
                        let geometry = crate::render_view::search::search_panel_geometry(scrollbar_x, s);
                        let search_h = 52.0 * s;
                        let input_x = geometry.x + 10.0 * s;
                        let input_y = self.search_anim_y + 11.0 * s;
                        let input_w = geometry.input_w;
                        let input_h = 30.0 * s;

                        if mx >= geometry.x
                            && mx <= geometry.x + geometry.w
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
                    } else if self.modifiers.control_key() && self.ctrl_definition.target.is_some()
                    {
                        winit::window::CursorIcon::Pointer
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

                self.renderer.as_mut().unwrap().flush();

                let autocomplete_swap_start = autocomplete_frame_start.map(|_| Instant::now());
                let autocomplete_frame_metrics = if autocomplete_swap_start.is_some() {
                    let detail_len = self
                        .autocomplete_detail_popup
                        .as_ref()
                        .map(|popup| popup.text.len())
                        .unwrap_or(0);
                    let r = self.renderer.as_ref().unwrap();
                    Some((
                        self.autocomplete_options.len(),
                        detail_len,
                        self.autocomplete_anim_progress,
                        r.vertices.len(),
                        r.vertices.capacity(),
                        r.glyphs.len(),
                        r.ui_glyphs.len(),
                    ))
                } else {
                    None
                };

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
                if crate::render_view::TELEMETRY_ENABLED
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    crate::render_view::record_swap_telemetry(
                        present_elapsed,
                        !self.scroll_y.is_settled() || !self.scroll_x.is_settled(),
                    );
                }

                if let (Some(frame_start), Some(swap_start), Some(metrics)) = (
                    autocomplete_frame_start,
                    autocomplete_swap_start,
                    autocomplete_frame_metrics,
                ) {
                    record_autocomplete_frame_perf(
                        frame_start,
                        autocomplete_prev_frame,
                        swap_start,
                        swap_start.elapsed().as_secs_f64() * 1000.0,
                        metrics.0,
                        metrics.1,
                        metrics.2,
                        metrics.3,
                        metrics.4,
                        metrics.5,
                        metrics.6,
                    );
                }

                if self.autocomplete_active && self.autocomplete_anim_progress < 1.0 {
                    self.window.as_ref().unwrap().request_redraw();
                }

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

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Covers native application termination such as macOS Cmd+Q and OS
        // session shutdown, which do not necessarily emit CloseRequested.
        window_runtime::persist_state_and_shutdown(self);
    }
}

#[cfg(test)]
mod tests {
    use super::autocomplete_detail_placement;

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
