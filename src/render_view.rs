pub mod core_text;
pub mod diag_popup_ui;
pub mod lsp_ui;
pub mod minimap_ui;
pub mod search;
pub mod settings_ui;
pub mod sticky;
pub mod tabs_ui;
pub mod terminal_ui;
pub mod ui;

use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::Renderer;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static TELEMETRY: RefCell<Telemetry> = RefCell::new(Telemetry::default());
}

struct Telemetry {
    render_time: f32,
    render_count: u32,
    scroll_time: f32,
    scroll_count: u32,
    type_time: f32,
    type_count: u32,
    last_print: Instant,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            render_time: 0.0,
            render_count: 0,
            scroll_time: 0.0,
            scroll_count: 0,
            type_time: 0.0,
            type_count: 0,
            last_print: Instant::now(),
        }
    }
}
use crate::widgets::IconButton;
use glow::HasContext;

#[derive(Clone, Copy)]
pub struct ModInterval {
    pub top: f32,
    pub bottom: f32,
    pub state: crate::editor::LineModState,
}

impl Renderer {
    pub fn draw(
        &mut self,
        editor: &mut Editor,
        editor_title: &str,
        editor_path: Option<&std::path::PathBuf>,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        scroll_x: f32,
        scroll_y: f32,
        blink_alpha: f32,
        show_fps: bool,
        spans: &[ColorSpan],
        dialog_window_open: bool,
        is_resizing: bool,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        show_search: bool,
        search_anim_y: f32,
        search_editor: &Editor,
        search_focused: bool,
        search_case_sensitive: bool,
        show_welcome: bool,
        recent_files: &[std::path::PathBuf],
        current_sticky_lines: &[(usize, usize)],
        sticky_anim_progress: f32,
        sticky_anim_is_adding: bool,
        is_ide_mode: bool,
        ide_panel: &crate::app::IdePanelState,
        show_settings: bool,
        lsp: Option<&crate::lsp::LspManager>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        tab_scroll_x: f32,
        _syntax_errors: &[(usize, usize)],
    ) -> (bool, Vec<(usize, usize)>) {
        include!("render_view/draw.rs")
    }
}
