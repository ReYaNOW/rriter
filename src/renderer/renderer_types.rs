use glow::HasContext;
use std::collections::HashMap;
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content};

mod geometry;
pub use geometry::Vertex;
pub(crate) use geometry::{
    glyph_quad_rect, quad_vertices, rounded_rect_gradient_vertices, squiggle_vertices,
};

pub const MAX_VERTICES: usize = 32_768;
pub const ATLAS_SIZE_W: i32 = 1024;
pub const ATLAS_SIZE_H: i32 = 1024;
const POPUP_MOUSE_MOVE_EPS: f32 = 0.5;

#[inline(always)]
fn popup_waiting_for_mouse_move(hide: bool, last_known: (f32, f32), x: f32, y: f32) -> bool {
    hide && (x - last_known.0).abs() <= POPUP_MOUSE_MOVE_EPS
        && (y - last_known.1).abs() <= POPUP_MOUSE_MOVE_EPS
}

fn alpha_bounds_y(data: &[u8], width: usize, height: usize) -> Option<(usize, usize)> {
    let row_len = width.checked_mul(4)?;
    if row_len == 0 || data.len() < row_len.checked_mul(height)? {
        return None;
    }

    let mut min_y = height;
    let mut max_y = 0usize;
    for y in 0..height {
        let row = &data[y * row_len..(y + 1) * row_len];
        if row.chunks_exact(4).any(|px| px[3] != 0) {
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }

    if min_y <= max_y {
        Some((min_y, max_y))
    } else {
        None
    }
}

fn center_alpha_bbox_y(data: &mut [u8], width: usize, height: usize) {
    let Some((min_y, max_y)) = alpha_bounds_y(data, width, height) else {
        return;
    };
    let visible_h = max_y - min_y + 1;
    let target_min_y = (height.saturating_sub(visible_h)) / 2;
    if target_min_y == min_y {
        return;
    }

    let Some(row_len) = width.checked_mul(4) else {
        return;
    };
    if target_min_y < min_y {
        let shift = min_y - target_min_y;
        data.copy_within(shift * row_len..height * row_len, 0);
        data[(height - shift) * row_len..height * row_len].fill(0);
    } else {
        let shift = target_min_y - min_y;
        data.copy_within(0..(height - shift) * row_len, shift * row_len);
        data[0..shift * row_len].fill(0);
    }
}

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
    pub diag_warn: [f32; 4],
    pub diag_error: [f32; 4],
    pub unused: [f32; 4],
}

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
pub enum FontSource {
    Static(&'static [u8]),
    LoadedMmap(std::sync::Arc<memmap2::Mmap>),
    LoadedVec(std::sync::Arc<Vec<u8>>),
    Lazy(
        std::path::PathBuf,
        std::rc::Rc<std::cell::RefCell<Option<Result<std::sync::Arc<memmap2::Mmap>, ()>>>>,
    ),
}

#[derive(Clone, Copy, Debug)]
pub struct GitGraphTooltipHover {
    pub workspace_idx: usize,
    pub commit_idx: usize,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct GitGraphTooltipTextRow {
    pub x: f32,
    pub top: f32,
    pub line_h: f32,
    pub scale: f32,
    pub mono: bool,
    pub start: usize,
    pub end: usize,
}

impl GitGraphTooltipHover {
    #[inline(always)]
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Clone)]
pub struct FontData {
    pub source: FontSource,
    pub index: u32,
}

impl FontData {
    pub fn new_lazy(path: &str) -> Self {
        Self {
            source: FontSource::Lazy(
                std::path::PathBuf::from(path),
                std::rc::Rc::new(std::cell::RefCell::new(None)),
            ),
            index: 0,
        }
    }

    pub fn new_static(data: &'static [u8]) -> Self {
        Self {
            source: FontSource::Static(data),
            index: 0,
        }
    }

    pub fn ensure_loaded(&mut self) {
        let new_source = if let FontSource::Lazy(path, refcell) = &self.source {
            let mut cache = refcell.borrow_mut();
            if cache.is_none() {
                if let Ok(file) = std::fs::File::open(path) {
                    if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
                        *cache = Some(Ok(std::sync::Arc::new(mmap)));
                    } else {
                        *cache = Some(Err(()));
                    }
                } else {
                    *cache = Some(Err(()));
                }
            }
            match cache.as_ref().unwrap() {
                Ok(arc) => Some(FontSource::LoadedMmap(arc.clone())),
                Err(_) => None,
            }
        } else {
            None
        };

        if let Some(ns) = new_source {
            self.source = ns;
        }
    }

    pub fn data_slice(&self) -> &[u8] {
        match &self.source {
            FontSource::Static(d) => d,
            FontSource::LoadedMmap(arc) => &**arc,
            FontSource::LoadedVec(arc) => arc.as_slice(),
            FontSource::Lazy(_, _) => &[],
        }
    }
}

pub struct Renderer {
    pub gl: glow::Context,
    pub program: glow::Program,
    pub proj_loc: Option<glow::UniformLocation>,
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
    pub max_tab_scroll_x: f32,
    pub last_editor_version_for_scroll_x: u64,

    pub last_frame_time: Option<std::time::Instant>,
    pub fps: f32,
    pub frame_count: u32,
    pub time_acc: f32,
    pub search_scroll_x: f32,

    pub fps_string: String,
    pub search_res_string: String,
    pub scratch_buffer: String,
    pub last_search_idx: Option<usize>,
    pub last_search_len: usize,

    pub icons: std::collections::HashMap<crate::widgets::IconType, glow::Texture>,
    pub icon_logo: Option<glow::Texture>,
    /// Кэш SVG-иконок для дерева файлов. Ключ — &'static str из file_icons_map.
    pub file_icon_cache: rustc_hash::FxHashMap<&'static str, glow::Texture>,
    pub sticky_scroll_rects: Vec<(f32, f32, f32, f32, usize)>,
    pub phys_to_visual: Vec<usize>,
    pub hide_popups_until_mouse_move: bool,
    pub tab_hover_timer: f32,
    pub tab_hover_idx: Option<usize>,
    pub tooltip_hover_key: Option<u64>,
    pub tooltip_hover_start: Option<std::time::Instant>,
    pub tooltip_hover_anchor: (f32, f32),
    pub tooltip_hover_mouse: (f32, f32),
    pub last_known_mouse: (f32, f32),
    pub last_editor_version_for_typing: u64,
    pub last_cursor_for_popups: usize,
    pub last_draw_instant: Option<std::time::Instant>,
    pub git_file_tooltip: Option<(usize, usize, String, f32, f32)>,
    pub git_action_tooltip: Option<(u8, usize, String, f32, f32)>,
    pub git_graph_tooltip: Option<(usize, usize, f32, f32)>,
    pub git_graph_tooltip_hover: Option<GitGraphTooltipHover>,
    pub(crate) git_graph_tooltip_text: String,
    pub(crate) git_graph_tooltip_text_rows: Vec<GitGraphTooltipTextRow>,
    pub(crate) git_graph_tooltip_selection_anchor: Option<usize>,
    pub(crate) git_graph_tooltip_selection_cursor: Option<usize>,
    pub(crate) git_graph_tooltip_selecting: bool,
    pub(crate) git_graph_tooltip_stable_w: f32,
    pub(crate) git_graph_tooltip_seen_copied: Option<(usize, usize)>,
    pub(crate) git_graph_tooltip_visible_copied: Option<(usize, usize)>,
    pub git_tooltip_waiting: bool,

    pub was_empty_ide: bool,
    pub empty_ide_art_idx: usize,
    pub identical_words_cache: Vec<(usize, usize)>,
    pub identical_words_cache_editor: usize,
    pub identical_words_cache_version: u64,
    pub identical_words_cache_cursor: usize,
    pub identical_words_cache_selection_anchor: Option<usize>,
    pub bracket_pair_cache: Option<(usize, usize)>,
    pub bracket_pair_cache_version: u64,
    pub bracket_pair_cache_cursor: usize,
    pub lsp_diagnostic_indices: Vec<usize>,
    pub unused_spans_cache: Vec<(usize, usize)>,
    pub current_python_inlay_hints: Vec<crate::app::PythonInlayHint>,
    pub terminal_row_search_results: Vec<(usize, (usize, usize, usize, usize))>,
    pub mod_intervals_cache: Vec<crate::render_view::ModInterval>,
    pub merged_intervals_cache: Vec<crate::render_view::ModInterval>,
    pub tab_x_anim: Vec<f32>,
}
