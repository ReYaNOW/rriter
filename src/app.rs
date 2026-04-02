pub mod events;
pub mod input;

use crate::editor::Editor;
use crate::highlighter::Highlighter;
use crate::renderer::{Renderer, Theme};
use arboard::Clipboard;
use glutin::context::PossiblyCurrentContext;
use glutin::display::{GetGlDisplay, GlDisplay}; // <-- Добавлен GlDisplay!
use glutin::surface::{Surface, WindowSurface};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::Window;

#[derive(Clone, Copy, PartialEq)]
pub enum PendingAction {
    Quit,
    OpenFile,
    Faq,
}

pub struct App {
    pub gl_config: Option<glutin::config::Config>,
    pub gl_context: Option<PossiblyCurrentContext>,
    pub gl_surface: Option<Surface<WindowSurface>>,
    pub window: Option<Window>,
    pub dialog_window: Option<Window>,
    pub dialog_surface: Option<Surface<WindowSurface>>,
    pub renderer: Option<Renderer>,
    pub editor: Editor,
    pub clipboard: Clipboard,
    pub theme: Theme,
    pub base_title: String,
    pub file_path: Option<PathBuf>,

    pub file_extension: String,
    pub highlighter: Highlighter,
    pub last_sent_version: u64,

    pub target_scroll_y: f32,
    pub scroll_y: f32,
    pub scroll_velocity: f32,

    pub last_frame: Instant,
    pub last_action: Instant,
    pub last_blink_state: bool,
    pub modifiers: ModifiersState,
    pub is_dragging: bool,
    pub is_dragging_minimap: bool,
    pub minimap_drag_offset_y: f32,
    pub is_focused: bool,
    pub show_fps: bool,
    pub scroll_anim_speed: f32,
    pub show_quit_dialog: bool,
    pub skip_highlight_update: bool,

    pub last_resize_time: Option<Instant>,

    pub last_click_time: Instant,
    pub click_count: u8,
    pub last_click_pos: (f32, f32),

    pub pending_action: PendingAction,
    pub open_file_rx: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,
    pub save_file_rx: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,

    pub show_welcome: bool,
    pub recent_files: Vec<PathBuf>,

    pub show_search: bool,
    pub search_anim_y: f32,
    pub search_editor: Editor,
    pub search_focused: bool,
    pub search_case_sensitive: bool,
    pub search_results: Vec<(usize, usize)>,
    pub search_current_idx: Option<usize>,
    pub is_dragging_search: bool,

    pub faq_editor: Editor,
    pub is_dragging_faq: bool,
    pub faq_scroll_y: f32,
    pub faq_target_scroll_y: f32,
    pub faq_scroll_velocity: f32,
    pub faq_scroll_anim_speed: f32,

    pub is_ready: bool,
    pub is_highlighted_once: bool,
}

impl App {
    pub fn ensure_cursor_visible(
        target_scroll_y: &mut f32,
        editor: &Editor,
        renderer: &mut Renderer,
        window_height: f32,
    ) {
        let (_, cy) = renderer.get_cursor_xy(editor);

        if cy - renderer.baseline_offset < *target_scroll_y {
            *target_scroll_y = (cy - renderer.baseline_offset).max(0.0);
            *target_scroll_y =
                (*target_scroll_y / renderer.line_height).floor() * renderer.line_height;
        } else if cy - renderer.baseline_offset + renderer.line_height
            > *target_scroll_y + window_height
        {
            *target_scroll_y = cy - renderer.baseline_offset + renderer.line_height - window_height;
            *target_scroll_y =
                (*target_scroll_y / renderer.line_height).ceil() * renderer.line_height;
        }

        let max_s = renderer.get_max_scroll(editor, window_height);
        *target_scroll_y = target_scroll_y.clamp(0.0, max_s).round();
    }

    pub fn update_search(&mut self) {
        let previous_match_start = self
            .search_current_idx
            .and_then(|idx| self.search_results.get(idx).map(|&(s, _)| s));

        self.search_results.clear();
        self.search_current_idx = None;
        let query_text = self.search_editor.get_full_text();
        if query_text.is_empty() {
            return;
        }

        let full_text = self.editor.get_full_text();

        let escaped_query = regex::escape(&query_text);
        if let Ok(re) = regex::RegexBuilder::new(&escaped_query)
            .case_insensitive(!self.search_case_sensitive)
            .dot_matches_new_line(true)
            .build()
        {
            for mat in re.find_iter(&full_text) {
                self.search_results.push((mat.start(), mat.end()));
            }
        }

        if !self.search_results.is_empty() {
            if let Some(prev_start) = previous_match_start {
                if let Ok(idx) = self
                    .search_results
                    .binary_search_by_key(&prev_start, |&(s, _)| s)
                {
                    self.search_current_idx = Some(idx);
                    return;
                }
            }

            let cursor = self.editor.cursor;
            let mut nearest_idx = 0;
            let mut min_dist = usize::MAX;

            for (i, &(s_start, s_end)) in self.search_results.iter().enumerate() {
                let dist = if cursor < s_start {
                    s_start - cursor
                } else if cursor > s_end {
                    cursor - s_end
                } else {
                    0
                };

                if dist < min_dist {
                    min_dist = dist;
                    nearest_idx = i;
                    if dist == 0 {
                        break;
                    }
                }
            }
            self.search_current_idx = Some(nearest_idx);
        }
    }

    pub fn jump_to_search_result(&mut self) {
        if let Some(idx) = self.search_current_idx {
            if let Some(&(start, end)) = self.search_results.get(idx) {
                self.editor.cursor = end;
                self.editor.selection_anchor = Some(start);

                if let Some(r) = self.renderer.as_mut() {
                    r.update_cache(&self.editor, false);
                    let line_idx = match r.visual_lines.binary_search_by_key(&end, |v| v.byte_idx) {
                        Ok(i) => i,
                        Err(i) => {
                            if i > 0 {
                                i - 1
                            } else {
                                0
                            }
                        }
                    };
                    let cy = r.baseline_offset + (line_idx as f32 * r.line_height);
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    self.target_scroll_y = (cy - wh / 2.0).max(0.0);
                    let max_s = r.get_max_scroll(&self.editor, wh);
                    self.target_scroll_y = self.target_scroll_y.clamp(0.0, max_s).round();
                    self.scroll_anim_speed = 10.0;
                }
            }
        }
    }

    pub fn update_window_title(window: &Window, base_title: &str, is_dirty: bool) {
        let title = if is_dirty {
            format!("{} * — RRiter", base_title)
        } else {
            format!("{} — RRiter", base_title)
        };
        window.set_title(&title);
    }

    pub fn show_action_dialog(&mut self, event_loop: &ActiveEventLoop, action: PendingAction) {
        if self.dialog_window.is_none() {
            let (title, p_width, p_height) = match action {
                PendingAction::Quit => ("Закрытие документа — RRiter", 600.0, 200.0),
                PendingAction::OpenFile => ("Открытие документа — RRiter", 600.0, 200.0),
                PendingAction::Faq => ("Справка — RRiter", 800.0, 680.0),
            };

            let attrs = Window::default_attributes()
                .with_title(title)
                .with_inner_size(winit::dpi::LogicalSize::new(p_width, p_height))
                .with_resizable(false)
                .with_enabled_buttons(winit::window::WindowButtons::CLOSE)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
            let dialog_window = event_loop.create_window(attrs).unwrap();
            let raw_window_handle = dialog_window.window_handle().unwrap().as_raw();

            let scale = dialog_window.scale_factor();
            let scaled_width = (p_width * scale) as u32;
            let scaled_height = (p_height * scale) as u32;

            let surface_attrs = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new()
                .build(
                    raw_window_handle,
                    NonZeroU32::new(scaled_width).unwrap(),
                    NonZeroU32::new(scaled_height).unwrap(),
                );
            let dialog_surface = unsafe {
                self.gl_config
                    .as_ref()
                    .unwrap()
                    .display()
                    .create_window_surface(self.gl_config.as_ref().unwrap(), &surface_attrs)
                    .unwrap()
            };
            self.dialog_window = Some(dialog_window);
            self.dialog_surface = Some(dialog_surface);
            self.show_quit_dialog = true;
            self.is_dragging = false;
            self.is_dragging_minimap = false;
            self.pending_action = action;
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
        }
    }

    pub fn close_dialog(&mut self) {
        self.dialog_window = None;
        self.dialog_surface = None;
        self.show_quit_dialog = false;
        self.is_dragging_faq = false;
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    pub fn trigger_file_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.open_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new().pick_file();
            let _ = tx.send(file);
        });
    }

    pub fn trigger_save_as_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.save_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_file_name("Безымянный.txt")
                .save_file();
            let _ = tx.send(file);
        });
    }

    pub fn save_current_file(&mut self) -> bool {
        if let Some(path) = self.file_path.clone() {
            let content = self.editor.get_full_text();
            match std::fs::write(&path, &content) {
                Ok(_) => {
                    self.editor.mark_saved();
                    return true;
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    use std::io::Write;
                    use std::process::{Command, Stdio};
                    if let Ok(mut child) = Command::new("pkexec")
                        .arg("tee")
                        .arg(&path)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::null())
                        .spawn()
                    {
                        if let Some(mut stdin) = child.stdin.take() {
                            let _ = stdin.write_all(content.as_bytes());
                        }
                        if let Ok(status) = child.wait() {
                            if status.success() {
                                self.editor.mark_saved();
                                return true;
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        } else {
            self.trigger_save_as_picker();
        }
        false
    }

    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(10);
        crate::save_recent_files(&self.recent_files);
    }

    pub fn load_file(&mut self, path: PathBuf) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            self.show_welcome = false;
            self.add_recent_file(path.clone());

            let old_version = self.editor.version;
            self.editor = Editor::new(content.len() + 8192);
            self.editor.version = old_version + 1;

            if !content.is_empty() {
                let _ = self.editor.insert_str(&content, &[]);
                self.editor.cursor = 0;
                self.editor.clear_history();
            }
            self.editor.set_original_text();

            self.file_path = Some(path.clone());
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            self.base_title = file_name.into_owned();

            if let Some(e) = path.extension() {
                self.file_extension = e.to_string_lossy().to_string();
            } else {
                self.file_extension = String::new();
            }

            self.highlighter.spans.clear();
            self.is_highlighted_once = false;

            self.highlighter.request_update(
                self.editor.version,
                self.editor.get_full_text(),
                self.file_extension.clone(),
            );

            self.target_scroll_y = 0.0;
            self.scroll_y = 0.0;
            self.last_sent_version = u64::MAX;

            self.search_results.clear();
            self.search_current_idx = None;

            if let Some(w) = self.window.as_ref() {
                App::update_window_title(w, &self.base_title, false);
                w.request_redraw();
            }
        }
    }
}
