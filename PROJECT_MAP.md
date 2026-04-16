# 🌳 RRiter Signal Map v4.5
> AI Instructions: Use 'Logic Deps' to track cross-module flow. Utilities are hidden.


## 📄 File: src/queries.rs
### ⚡ fn get_params_query(lang_name: &str) -> Option<&'static str>
### ⚡ fn get_injection_query(lang_name: &str) -> Option<&'static str>
### ⚡ fn get_folding_query(lang_name: &str) -> Option<&'static str>
### ⚡ fn get_ts_config(lang_name: &str) -> Option<(tree_sitter::Language, Vec<&'static str>)>
### ⚡ fn test_all_tree_sitter_queries_are_valid()
  - **Logic Deps:** `queries.rs: get_params_query, get_injection_query, get_ts_config, get_folding_query`
## 📄 File: src/renderer.rs
#### `struct Theme`
#### `struct Vertex`
#### `struct GlyphInfo`
#### `struct VisualLine`
#### `struct FontData`
#### `struct Renderer`
#### `impl Renderer`
### ⚡ fn new(gl: glow::Context, scale_factor: f32, theme: Theme) -> Self
  - **Logic Deps:** `main.rs: main; renderer.rs: get_glyph, load_builtin_icons`
### ⚡ fn get_custom_svg_glyph(&mut self, c: char) -> Option<GlyphInfo>
  - **Logic Deps:** `widgets.rs: render`
### ⚡ fn get_glyph(&mut self, c: char) -> Option<GlyphInfo>
  - **Logic Deps:** `widgets.rs: render; renderer.rs: get_custom_svg_glyph`
### ⚡ fn get_ui_glyph(&mut self, c: char) -> Option<GlyphInfo>
  - **Logic Deps:** `widgets.rs: render; renderer.rs: get_custom_svg_glyph`
### ⚡ fn resize(&mut self, w: u32, h: u32)
### ⚡ fn measure_ui_width(&mut self, text: &str, scale: f32) -> f32
  - **Logic Deps:** `renderer.rs: get_ui_glyph`
### ⚡ fn char_advance(&mut self, c: char) -> f32
  - **Logic Deps:** `renderer.rs: get_glyph`
### ⚡ fn push_quad(&mut self, x: f32, y: f32, w: f32, h: f32, u: f32, v: f32, uw: f32, vh: f32, color: [f32; 4], mode: f32,)
### ⚡ fn load_builtin_icons(&mut self)
  - **Logic Deps:** `widgets.rs: render`
### ⚡ fn push_squiggle(&mut self, x: f32, baseline_y: f32, w: f32, color: [f32; 4])
### ⚡ fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4])
  - **Logic Deps:** `renderer.rs: push_quad`
### ⚡ fn push_rounded_rect_gradient(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, top_color: [f32; 4], bottom_color: [f32; 4],)
## 📄 File: src/widgets.rs
#### `enum IconType`
#### `struct Button`
#### `impl Button`
#### `struct IconButton`
#### `impl IconButton`
### ⚡ fn is_hovered(&self, mx: f32, my: f32) -> bool
### ⚡ fn render(&self, renderer: &mut Renderer, mx: f32, my: f32, scale: f32, pressed: bool,) -> bool
  - **Logic Deps:** `core_text.rs: draw_string_scaled, push_rounded_rect; renderer.rs: measure_ui_width; ui.rs: draw_atlas_icon`
### ⚡ fn is_hovered(&self, mx: f32, my: f32) -> bool
### ⚡ fn render(&self, renderer: &mut Renderer, mx: f32, my: f32, scale: f32, pressed: bool,) -> bool
  - **Logic Deps:** `renderer.rs: push_rect; widgets.rs: is_hovered; core_text.rs: push_rounded_rect; ui.rs: draw_atlas_icon`
### ⚡ fn get_welcome_buttons(_width: f32, x: f32, y: f32, scale: f32, renderer: &mut Renderer,) -> (Button, Button, Button)
  - **Logic Deps:** `renderer.rs: measure_ui_width`
### ⚡ fn get_dialog_buttons(box_x: f32, box_y: f32, box_w: f32, box_h: f32, scale: f32, renderer: &mut Renderer,) -> (Button, Button, Button)
  - **Logic Deps:** `renderer.rs: measure_ui_width`
## 📄 File: src/editor.rs
#### `enum LineModState`
#### `enum EditOp`
#### `struct HistoryStep`
#### `enum UndoRedoDelta`
#### `struct Editor`
#### `impl Editor`
### ⚡ fn get_diff_info(old: &[u64], new: &[u64]) -> (Vec<bool>, Vec<bool>)
### ⚡ fn is_delimiter(b: u8) -> bool
### ⚡ fn char_class(b: u8) -> u8
### ⚡ fn new(capacity: usize) -> Self
### ⚡ fn shift_folds_insert(&mut self, offset: usize, len: usize)
### ⚡ fn shift_folds_delete(&mut self, offset: usize, len: usize)
### ⚡ fn rebuild_line_offsets(&mut self)
  - **Logic Deps:** `editor.rs: text_parts`
### ⚡ fn get_line_hashes(&self) -> Vec<u64>
  - **Logic Deps:** `editor.rs: text_parts`
### ⚡ fn ensure_indent_cache_updated(&mut self)
  - **Logic Deps:** `editor.rs: text_parts`
### ⚡ fn get_cached_indent_levels(&self) -> &[u8]
### ⚡ fn backspace(&mut self) -> Option<(usize, usize)>
  - **Logic Deps:** `editor.rs: push_history, update_modifications, is_char_boundary, shift_folds_delete, delete_selection, move_gap, byte_at`
### ⚡ fn set_original_text(&mut self)
  - **Logic Deps:** `editor.rs: update_modifications, get_line_hashes`
### ⚡ fn mark_saved(&mut self)
  - **Logic Deps:** `editor.rs: update_modifications, get_line_hashes`
### ⚡ fn is_dirty(&self) -> bool
### ⚡ fn get_line_modification_state(&self, line: usize) -> Option<LineModState>
### ⚡ fn update_modifications(&mut self)
  - **Logic Deps:** `editor.rs: get_diff_info, rebuild_line_offsets, get_line_hashes`
### ⚡ fn clear_history(&mut self)
  - **Logic Deps:** `editor.rs: update_modifications`
### ⚡ fn push_history(&mut self, step: HistoryStep)
### ⚡ fn undo(&mut self) -> Option<UndoRedoDelta>
  - **Logic Deps:** `editor.rs: update_modifications, insert_str_internal, shift_folds_delete, move_gap`
### ⚡ fn redo(&mut self) -> Option<UndoRedoDelta>
  - **Logic Deps:** `editor.rs: update_modifications, insert_str_internal, shift_folds_delete, move_gap`
### ⚡ fn text_parts(&self) -> (&str, &str)
### ⚡ fn get_full_text(&self) -> String
  - **Logic Deps:** `editor.rs: text_parts`
### ⚡ fn move_gap(&mut self, target: usize)
### ⚡ fn insert_str_internal(&mut self, s: &str) -> usize
  - **Logic Deps:** `editor.rs: shift_folds_insert, move_gap`
### ⚡ fn insert_str(&mut self, s: &str) -> (Option<(usize, usize)>, usize)
  - **Logic Deps:** `editor.rs: push_history, update_modifications, insert_str_internal, delete_selection`
### ⚡ fn delete_selection(&mut self) -> Option<(usize, usize)>
  - **Logic Deps:** `editor.rs: push_history, update_modifications, shift_folds_delete, move_gap, byte_at`
### ⚡ fn delete_forward(&mut self) -> Option<(usize, usize)>
  - **Logic Deps:** `editor.rs: push_history, update_modifications, is_char_boundary, shift_folds_delete, delete_selection, move_gap, byte_at`
### ⚡ fn delete_word_backward(&mut self) -> Option<(usize, usize)>
  - **Logic Deps:** `editor.rs: is_delimiter, delete_selection, byte_at`
### ⚡ fn delete_word_forward(&mut self) -> Option<(usize, usize)>
  - **Logic Deps:** `editor.rs: is_delimiter, delete_selection, byte_at`
### ⚡ fn get_auto_indent(&self) -> String
  - **Logic Deps:** `editor.rs: byte_at`
### ⚡ fn select_expand(&mut self)
  - **Logic Deps:** `editor.rs: byte_at, char_class`
### ⚡ fn select_word(&mut self)
  - **Logic Deps:** `editor.rs: byte_at, is_delimiter`
### ⚡ fn select_line(&mut self)
  - **Logic Deps:** `editor.rs: byte_at`
### ⚡ fn is_char_boundary(&self, index: usize) -> bool
  - **Logic Deps:** `editor.rs: byte_at`
### ⚡ fn byte_at(&self, idx: usize) -> u8
### ⚡ fn len(&self) -> usize
### ⚡ fn utf16_col_to_byte_advance(&self, line_idx: usize, mut f: F)
  - **Logic Deps:** `editor.rs: byte_at`
### ⚡ fn get_selection(&self) -> Option<String>
  - **Logic Deps:** `editor.rs: byte_at`
### ⚡ fn select_all(&mut self)
### ⚡ fn handle_selection(&mut self, shift: bool)
### ⚡ fn snap_cursor_out_of_fold(&mut self, old_cursor: usize)
  - **Logic Deps:** `editor.rs: move_end`
### ⚡ fn move_left(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: snap_cursor_out_of_fold, is_char_boundary, handle_selection`
### ⚡ fn move_right(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: snap_cursor_out_of_fold, is_char_boundary, handle_selection`
### ⚡ fn move_word_left(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: snap_cursor_out_of_fold, char_class, handle_selection, byte_at`
### ⚡ fn move_word_right(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: snap_cursor_out_of_fold, handle_selection, char_class, byte_at`
### ⚡ fn move_home(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: handle_selection, byte_at`
### ⚡ fn move_end(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: handle_selection, byte_at`
### ⚡ fn move_start_of_file(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: handle_selection`
### ⚡ fn move_end_of_file(&mut self, shift: bool)
  - **Logic Deps:** `editor.rs: handle_selection`
### ⚡ fn move_up(&mut self, renderer: &mut Renderer, shift: bool)
  - **Logic Deps:** `core_text.rs: get_cursor_xy, get_byte_at_xy; editor.rs: handle_selection`
### ⚡ fn move_down(&mut self, renderer: &mut Renderer, shift: bool)
  - **Logic Deps:** `core_text.rs: get_cursor_xy, get_byte_at_xy; editor.rs: handle_selection`
### ⚡ fn set_cursor_at_pos(&mut self, target_x: f32, target_y: f32, renderer: &mut Renderer, is_click: bool,)
  - **Logic Deps:** `core_text.rs: get_byte_at_xy`
### ⚡ fn move_page_up(&mut self, renderer: &mut Renderer, shift: bool, step: f32)
  - **Logic Deps:** `core_text.rs: get_cursor_xy, get_byte_at_xy; editor.rs: handle_selection`
### ⚡ fn move_page_down(&mut self, renderer: &mut Renderer, shift: bool, step: f32)
  - **Logic Deps:** `core_text.rs: get_cursor_xy, get_byte_at_xy; editor.rs: handle_selection`
## 📄 File: src/lsp.rs
#### `enum LspServerStatus`
#### `struct LogEntry`
#### `struct LspServerInfo`
#### `enum DiagSeverity`
#### `struct Diagnostic`
#### `struct TextChange`
#### `struct WorkspaceEdit`
#### `enum LspEvent`
#### `struct CodeAction`
#### `struct LspServerDef`
#### `enum Cmd`
#### `struct SpawnedProcess`
#### `struct OpenFile`
#### `struct LspProcess`
#### `impl LspProcess`
#### `struct LspManager`
#### `impl LspManager`
### ⚡ fn next_id() -> i32
### ⚡ fn offset_to_lsp_pos(text: &str, offset: usize, line_offsets: &[usize]) -> (u32, u32)
### ⚡ fn json_escape(s: &str) -> String
### ⚡ fn path_to_uri(path: &str) -> String
### ⚡ fn uri_to_path(uri: &str) -> PathBuf
### ⚡ fn make_initialize(id: i32, workspace: Option<&Path>) -> Vec<u8>
  - **Logic Deps:** `ui_system.rs: id; lsp.rs: json_escape, path_to_uri`
### ⚡ fn make_initialized() -> Vec<u8>
### ⚡ fn make_did_open(uri: &str, lang: &str, version: i32, text: &str) -> Vec<u8>
  - **Logic Deps:** `lsp.rs: json_escape`
### ⚡ fn make_did_change_full(uri: &str, version: i32, text: &str) -> Vec<u8>
  - **Logic Deps:** `lsp.rs: json_escape`
### ⚡ fn make_did_close(uri: &str) -> Vec<u8>
  - **Logic Deps:** `lsp.rs: json_escape`
### ⚡ fn make_code_action(id: i32, uri: &str, sl: u32, sc: u32, el: u32, ec: u32, diag_json: &str,) -> Vec<u8>
  - **Logic Deps:** `lsp.rs: json_escape`
### ⚡ fn make_shutdown(id: i32) -> Vec<u8>
### ⚡ fn make_exit() -> Vec<u8>
### ⚡ fn write_frame(writer: &mut BufWriter<std::process::ChildStdin>, body: &[u8]) -> bool
### ⚡ fn parse_diagnostic_value(v: &serde_json::Value) -> Option<Diagnostic>
### ⚡ fn parse_text_edit_value(v: &serde_json::Value) -> Option<TextChange>
### ⚡ fn parse_workspace_edit_value(v: &serde_json::Value) -> WorkspaceEdit
  - **Logic Deps:** `lsp.rs: uri_to_path`
### ⚡ fn parse_code_action_value(v: &serde_json::Value) -> Option<CodeAction>
### ⚡ fn dispatch_frame(body: &[u8], event_tx: &Sender<LspEvent>, server_name: &'static str, out_tx: &Sender<Vec<u8>>,)
  - **Logic Deps:** `lsp.rs: parse_workspace_edit_value, parse_diagnostic_value, uri_to_path`
### ⚡ fn spawn_server(def: &'static LspServerDef, workspace: Option<&Path>, event_tx: Sender<LspEvent>,) -> Option<SpawnedProcess>
  - **Logic Deps:** `lsp.rs: dispatch_frame, write_frame`
### ⚡ fn send_and_log(out_tx: &Sender<Vec<u8>>, event_tx: &Sender<LspEvent>, server_name: &'static str, msg: Vec<u8>,) -> Result<(), mpsc::SendError<Vec<u8>>>
### ⚡ fn run_supervisor(def: &'static LspServerDef, workspace: Option<PathBuf>, cmd_rx: Receiver<Cmd>, event_tx: Sender<LspEvent>,)
  - **Logic Deps:** `lsp.rs: make_initialized, make_did_close, make_shutdown, make_did_open, make_did_change_full, next_id, send_and_log, make_initialize, make_code_action, make_exit, spawn_server`
### ⚡ fn start(def: &'static LspServerDef, workspace: Option<PathBuf>) -> Self
  - **Logic Deps:** `lsp.rs: run_supervisor`
### ⚡ fn notify_open(&mut self, path: &PathBuf, text: &str, version: i32)
  - **Logic Deps:** `lsp.rs: path_to_uri`
### ⚡ fn restart(&mut self)
### ⚡ fn notify_change(&mut self, path: &PathBuf, text: &str, version: i32)
  - **Logic Deps:** `lsp.rs: path_to_uri`
### ⚡ fn notify_close(&mut self, path: &PathBuf)
  - **Logic Deps:** `lsp.rs: path_to_uri`
### ⚡ fn request_code_actions(&mut self, path: &PathBuf, start_line: u32, start_col: u32, end_line: u32, end_col: u32, diagnostics: &[Diagnostic],) -> i32
  - **Logic Deps:** `lsp.rs: next_id, encode_diagnostics_json, path_to_uri`
### ⚡ fn poll(&self) -> Vec<LspEvent>
### ⚡ fn shutdown(self)
### ⚡ fn encode_diagnostics_json(diags: &[Diagnostic]) -> String
  - **Logic Deps:** `lsp.rs: json_escape`
### ⚡ fn new(workspace: Option<PathBuf>) -> Self
### ⚡ fn ensure_python(&mut self)
  - **Logic Deps:** `lsp.rs: start`
### ⚡ fn restart_python(&mut self)
  - **Logic Deps:** `lsp.rs: restart, start`
### ⚡ fn disable_python(&mut self)
  - **Logic Deps:** `lsp.rs: shutdown`
### ⚡ fn enable_python(&mut self)
  - **Logic Deps:** `lsp.rs: notify_open, start`
### ⚡ fn servers_info(&self) -> Vec<LspServerInfo>
### ⚡ fn process_for_ext(&mut self, ext: &str) -> Option<&mut LspProcess>
  - **Logic Deps:** `lsp.rs: ensure_python`
### ⚡ fn notify_open(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32)
  - **Logic Deps:** `lsp.rs: process_for_ext`
### ⚡ fn notify_change(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32)
  - **Logic Deps:** `lsp.rs: process_for_ext`
### ⚡ fn notify_close(&mut self, ext: &str)
  - **Logic Deps:** `lsp.rs: process_for_ext`
### ⚡ fn request_code_actions(&mut self, ext: &str, start_line: u32, start_col: u32, end_line: u32, end_col: u32, relevant_diags: &[Diagnostic],) -> Option<i32>
  - **Logic Deps:** `lsp.rs: process_for_ext`
### ⚡ fn poll(&mut self) -> Vec<LspEvent>
  - **Logic Deps:** `lsp.rs: path_to_uri, format_and_highlight_json`
### ⚡ fn diagnostics_for_line(&self, line: u32) -> Vec<&Diagnostic>
### ⚡ fn request_fix_all(&mut self, ext: &str) -> Option<i32>
  - **Logic Deps:** `lsp.rs: process_for_ext, next_id, path_to_uri`
### ⚡ fn shutdown(mut self)
### ⚡ fn lsp_pos_to_offset(text: &str, line: u32, col: u32) -> usize
### ⚡ fn apply_workspace_edit_to_text(text: &str, edit: &WorkspaceEdit, path: &PathBuf) -> String
  - **Logic Deps:** `lsp.rs: lsp_pos_to_offset`
### ⚡ fn format_and_highlight_json(raw_text: &str,) -> (
  String,
  Vec<crate::highlighter::ColorSpan>,
  Vec<(usize, usize)>,
)
  - **Logic Deps:** `queries.rs: get_folding_query, get_ts_config`
## 📄 File: src/main.rs
#### `struct Config`
#### `impl Default for Config`
### ⚡ fn default() -> Self
### ⚡ fn load_recent_files() -> Vec<PathBuf>
### ⚡ fn save_recent_files(files: &[PathBuf])
### ⚡ fn save_panel_state(state: &crate::app::IdePanelState)
### ⚡ fn load_panel_state() -> crate::app::IdePanelState
### ⚡ fn save_config(config: &Config)
### ⚡ fn load_config() -> Config
  - **Logic Deps:** `main.rs: save_config`
### ⚡ fn get_kde_color(target_group: &str, target_key: &str) -> Option<[f32
### ⚡ fn load_dracula() -> Theme
  - **Logic Deps:** `main.rs: get_kde_color`
### ⚡ fn main()
  - **Logic Deps:** `main.rs: load_recent_files, load_panel_state, save_recent_files, load_config, load_dracula; file_tree.rs: refresh_file_tree, start_file_watcher; editor.rs: insert_str, set_original_text, get_full_text, clear_history; app.rs: is_open; highlighter.rs: reset`
## 📄 File: src/render_view.rs
#### `struct ModInterval`
#### `impl Renderer`
### ⚡ fn draw(&mut self, editor: &mut Editor, scroll_x: f32, scroll_y: f32, blink_alpha: f32, show_fps: bool, spans: &[ColorSpan], dialog_window_open: bool, is_resizing: bool, search_results: &[(usize, usize)
  - **Logic Deps:** `ui_system.rs: register_text_input, hovered, register_rect, reset_cursor_state, register_icon_button, wants_pointer, register_blocker; app.rs: any_bottom_open, icon, label, any_top_open, is_open; core_text.rs: update_cache, measure_width, push_rounded_rect, draw_string_mono_scaled, get_max_scroll, draw_string_scaled, measure_mono_width, draw_string; render_view.rs: draw_minimap; widgets.rs: render`
### ⚡ fn draw_minimap(&mut self, editor: &Editor, spans: &[ColorSpan], render_scroll_y: f32, max_scroll: f32, total_lines: usize, visible_cursor_line: usize,)
  - **Logic Deps:** `editor.rs: text_parts; renderer.rs: push_rect`
## 📄 File: src/app.rs
#### `enum PendingAction`
#### `enum PanelId`
#### `impl PanelId`
#### `enum PanelGroup`
#### `struct PanelSlot`
#### `struct PanelDragState`
#### `enum LspActionItem`
#### `struct LspActionsMenu`
#### `struct IdePanelState`
#### `impl Default for IdePanelState`
#### `impl IdePanelState`
#### `struct App`
#### `impl App`
### ⚡ fn label(self) -> &'static str
### ⚡ fn icon(self) -> crate::widgets::IconType
  - **Logic Deps:** `lsp.rs: CodeAction`
### ⚡ fn default() -> Self
### ⚡ fn any_top_open(&self) -> bool
### ⚡ fn any_bottom_open(&self) -> bool
### ⚡ fn toggle(&mut self, id: PanelId)
### ⚡ fn is_open(&self, id: PanelId) -> bool
### ⚡ fn fuzzy_match(pattern: &str, target: &str) -> Option<Vec<usize>>
### ⚡ fn ensure_cursor_visible(target_scroll_y: &mut f32, target_scroll_x: &mut f32, editor: &Editor, renderer: &mut Renderer, window_width: f32, window_height: f32,)
  - **Logic Deps:** `core_text.rs: get_max_scroll, get_cursor_xy`
### ⚡ fn get_current_word_prefix(&self) -> String
  - **Logic Deps:** `editor.rs: byte_at`
### ⚡ fn update_autocomplete(&mut self)
  - **Logic Deps:** `app.rs: get_current_word_prefix, fuzzy_match`
### ⚡ fn ensure_autocomplete_visible(&mut self)
  - **Logic Deps:** `scroll.rs: set_target, clamp_target`
### ⚡ fn apply_autocomplete(&mut self)
  - **Logic Deps:** `app.rs: get_current_word_prefix, update_window_title; editor.rs: insert_str, is_dirty, backspace; highlighter.rs: shift_delete, shift_insert`
### ⚡ fn update_search(&mut self)
  - **Logic Deps:** `editor.rs: get_full_text; lsp.rs: start`
### ⚡ fn jump_to_search_result(&mut self)
  - **Logic Deps:** `scroll.rs: clamp_target; core_text.rs: get_max_scroll`
### ⚡ fn update_window_title(window: &Window, base_title: &str, is_dirty: bool)
### ⚡ fn show_action_dialog(&mut self, event_loop: &ActiveEventLoop, action: PendingAction)
### ⚡ fn close_dialog(&mut self)
### ⚡ fn close_current_file(&mut self)
  - **Logic Deps:** `editor.rs: set_original_text; app.rs: update_window_title; highlighter.rs: reset; lsp.rs: notify_close`
### ⚡ fn trigger_file_picker(&mut self)
### ⚡ fn trigger_folder_picker(&mut self)
### ⚡ fn trigger_save_as_picker(&mut self)
### ⚡ fn save_current_file(&mut self) -> bool
  - **Logic Deps:** `editor.rs: mark_saved, get_full_text; app.rs: trigger_save_as_picker`
### ⚡ fn add_recent_file(&mut self, path: PathBuf)
  - **Logic Deps:** `highlighter.rs: wait_for_first_result, poll; main.rs: save_recent_files`
### ⚡ fn apply_highlight_results(&mut self)
### ⚡ fn load_file(&mut self, path: PathBuf, add_to_history: bool)
  - **Logic Deps:** `lsp.rs: notify_open; highlighter.rs: wait_for_first_result, reset; editor.rs: insert_str, set_original_text, get_full_text, clear_history; app.rs: apply_highlight_results, update_window_title, add_recent_file; main.rs: save_recent_files`
## 📄 File: src/highlighter.rs
#### `struct ColorSpan`
#### `enum SymbolKind`
#### `struct CompletionItem`
#### `enum SyncEdit`
#### `enum HighlighterMessage`
#### `struct Highlighter`
#### `struct Scope`
#### `impl Highlighter`
### ⚡ fn get_point(text: &str, byte_offset: usize) -> tree_sitter::Point
### ⚡ fn resolve_color(name: &str, node_text: &str, start_byte: usize, param_scopes: &[Scope],) -> [f32
### ⚡ fn new() -> Self
  - **Logic Deps:** `highlighter.rs: resolve_color, flatten_spans, get_point; queries.rs: get_injection_query, get_params_query, get_folding_query, get_ts_config; editor.rs: insert_str, is_char_boundary`
### ⚡ fn reset(&self, version: u64, text: String, ext: String)
### ⚡ fn apply_edits(&self, version: u64, edits: Vec<SyncEdit>)
### ⚡ fn poll(&mut self, current_editor_version: u64) -> bool
### ⚡ fn wait_for_first_result(&mut self, version: u64, timeout: std::time::Duration) -> bool
  - **Logic Deps:** `highlighter.rs: poll`
### ⚡ fn shift_insert(&mut self, offset: usize, len: usize, text_opt: Option<&str>)
### ⚡ fn shift_delete(&mut self, offset: usize, len: usize)
### ⚡ fn get_bracket_color(depth: usize) -> [f32
### ⚡ fn flatten_spans(mut spans: Vec<ColorSpan>, len: usize, text: &str, byte_colors: &mut Vec<[f32; 4]>, error_ranges: Vec<(usize, usize)
  - **Logic Deps:** `highlighter.rs: get_bracket_color`
## 📄 File: src/scroll.rs
#### `struct ScrollState`
#### `impl ScrollState`
### ⚡ fn new(anim_speed: f32) -> Self
### ⚡ fn update(&mut self, dt: f32) -> bool
### ⚡ fn clamp_target(&mut self, min: f32, max: f32)
### ⚡ fn clamp_current(&mut self, min: f32, max: f32)
### ⚡ fn scroll_by(&mut self, delta: f32)
### ⚡ fn set_target(&mut self, target: f32)
### ⚡ fn stop_anim(&mut self)
## 📄 File: src/ui_system.rs
#### `enum UiId`
#### `enum UiElement`
#### `impl UiElement`
#### `struct UiRegistry`
#### `impl UiRegistry`
### ⚡ fn contains(&self, mx: f32, my: f32) -> bool
### ⚡ fn id(&self) -> UiId
### ⚡ fn new() -> Self
### ⚡ fn clear(&mut self)
### ⚡ fn mark_overlay_start(&mut self)
### ⚡ fn find_overlay_at(&self, mx: f32, my: f32) -> Option<UiId>
  - **Logic Deps:** `ui_system.rs: id`
### ⚡ fn register_button(&mut self, id: UiId, button: &Button, renderer: &mut Renderer, mx: f32, my: f32, scale: f32, pressed: bool,) -> bool
  - **Logic Deps:** `widgets.rs: render`
### ⚡ fn register_icon_button(&mut self, id: UiId, icon_button: &IconButton, renderer: &mut Renderer, mx: f32, my: f32, scale: f32, pressed: bool,) -> bool
  - **Logic Deps:** `widgets.rs: render`
### ⚡ fn register_text_input(&mut self, id: UiId, x: f32, y: f32, w: f32, h: f32, mx: f32, my: f32,) -> bool
### ⚡ fn register_blocker(&mut self, id: UiId, x: f32, y: f32, w: f32, h: f32, mx: f32, my: f32,) -> bool
### ⚡ fn register_rect(&mut self, id: UiId, x: f32, y: f32, w: f32, h: f32, mx: f32, my: f32,) -> bool
### ⚡ fn find_at(&self, mx: f32, my: f32) -> Option<UiId>
  - **Logic Deps:** `ui_system.rs: id`
### ⚡ fn hovered(&self) -> Option<UiId>
### ⚡ fn reset_cursor_state(&mut self)
### ⚡ fn wants_pointer(&self) -> bool
### ⚡ fn wants_text(&self) -> bool
### ⚡ fn cursor_code(&self) -> u8
## 📄 File: src/app/file_icons.rs
> Определение иконки для файла/папки.
#### `struct FallbackMatcher`
### ⚡ fn file_icon_key(name: &str) -> &'static str
### ⚡ fn folder_icon_key(name: &str) -> &'static str
### ⚡ fn svg_for_key(key: &str, is_folder: bool) -> &'static [u8]
## 📄 File: src/app/keyboard.rs
#### `impl App`
### ⚡ fn handle_search_keyboard_input(&mut self, key_event: KeyEvent)
  - **Logic Deps:** `editor.rs: move_left, get_selection, insert_str, select_all, move_home, move_word_left, move_word_right, move_right, delete_selection, delete_forward, backspace, move_end; app.rs: update_search, jump_to_search_result`
### ⚡ fn handle_editor_keyboard_input(&mut self, event_loop: &ActiveEventLoop, key_event: KeyEvent,)
  - **Logic Deps:** `editor.rs: move_page_up, delete_word_forward, get_full_text, move_end_of_file, delete_forward, redo, move_word_right, move_right, move_page_down, is_dirty, delete_word_backward, select_all, select_expand, delete_selection, backspace, move_down, move_left, get_auto_indent, get_selection, insert_str, move_start_of_file, move_home, move_up, move_word_left, undo, move_end; app.rs: show_action_dialog, update_autocomplete, jump_to_search_result, close_current_file, apply_autocomplete, ensure_autocomplete_visible, update_search, trigger_file_picker, save_current_file, ensure_cursor_visible, update_window_title; main.rs: save_config; lsp_actions.rs: apply_selected_lsp_action, open_lsp_actions_menu; core_text.rs: get_cursor_xy, get_max_scroll`
### ⚡ fn handle_main_keyboard_input(&mut self, event_loop: &ActiveEventLoop, key_event: KeyEvent,)
  - **Logic Deps:** `editor.rs: move_left, delete_word_forward, get_selection, insert_str, delete_word_backward, get_full_text, select_all, move_home, move_word_left, move_word_right, delete_forward, delete_selection, move_right, backspace, move_end; keyboard.rs: handle_search_keyboard_input, handle_editor_keyboard_input; file_tree.rs: refresh_file_tree; main.rs: save_config; app.rs: close_dialog`
## 📄 File: src/app/file_tree.rs
> Логика проводника файлов: структуры данных, фоновый скан, методы App.
#### `struct FileNode`
#### `impl App`
### ⚡ fn matches_ignore_pattern(name: &str, patterns: &[&str]) -> bool
### ⚡ fn pre_rasterize_icon(key: &'static str, is_folder: bool)
  - **Logic Deps:** `widgets.rs: render; file_icons.rs: svg_for_key`
### ⚡ fn read_children(dir: &PathBuf) -> (Vec<(String, PathBuf)>, Vec<(String, PathBuf)>)
### ⚡ fn scan_dir_parallel(path: PathBuf, name: String, depth: usize, expanded: &FxHashSet<PathBuf>, is_root: bool, max_depth: usize, gitignore: &ignore::gitignore::Gitignore, all_patterns: &[&str],) -> Vec<FileNode>
  - **Logic Deps:** `file_icons.rs: file_icon_key, folder_icon_key; file_tree.rs: matches_ignore_pattern, read_children`
### ⚡ fn spawn_scan(roots: Vec<PathBuf>, expanded: FxHashSet<PathBuf>, user_patterns: Vec<String>,) -> mpsc::Receiver<Vec<FileNode>>
  - **Logic Deps:** `file_tree.rs: scan_dir_parallel, pre_rasterize_icon`
### ⚡ fn spawn_watcher(paths: Vec<PathBuf>, tx: mpsc::Sender<()
### ⚡ fn refresh_file_tree(&mut self)
  - **Logic Deps:** `file_tree.rs: spawn_scan`
### ⚡ fn poll_file_tree(&mut self) -> bool
### ⚡ fn start_file_watcher(&mut self)
  - **Logic Deps:** `file_tree.rs: spawn_watcher`
### ⚡ fn handle_file_tree_click(&mut self, node_idx: usize)
  - **Logic Deps:** `app.rs: load_file; file_tree.rs: refresh_file_tree`
### ⚡ fn file_tree_node_at(&self, mx: f32, my: f32) -> Option<usize>
  - **Logic Deps:** `app.rs: any_top_open, is_open`
## 📄 File: src/app/lsp_actions.rs
#### `impl App`
### ⚡ fn lsp_panel_bounds(&self) -> Option<(f32, f32, f32, f32)>
  - **Logic Deps:** `app.rs: any_bottom_open`
### ⚡ fn lsp_panel_total_h(&self, s: f32) -> f32
  - **Logic Deps:** `lsp_actions.rs: lsp_server_logs_h`
### ⚡ fn lsp_server_logs_h(&self, info: &crate::lsp::LspServerInfo, s: f32) -> f32
  - **Logic Deps:** `lsp_actions.rs: lsp_server_inner_size`
### ⚡ fn lsp_server_inner_size(&self, info: &crate::lsp::LspServerInfo, s: f32,) -> (f32, f32)
### ⚡ fn open_lsp_actions_menu(&mut self)
  - **Logic Deps:** `lsp.rs: diagnostics_for_line, request_code_actions; core_text.rs: get_cursor_xy`
### ⚡ fn apply_selected_lsp_action(&mut self)
  - **Logic Deps:** `lsp.rs: apply_workspace_edit_to_text, CodeAction; editor.rs: get_full_text; lsp_actions.rs: insert_noqa_comment, apply_full_text_replacement`
### ⚡ fn insert_noqa_comment(&mut self, line: u32, codes: &[String])
  - **Logic Deps:** `editor.rs: byte_at, insert_str, is_dirty, get_full_text, delete_forward; app.rs: update_window_title; highlighter.rs: apply_edits, shift_delete, shift_insert; lsp.rs: notify_change`
### ⚡ fn apply_full_text_replacement(&mut self, new_text: String)
  - **Logic Deps:** `editor.rs: insert_str, set_original_text; app.rs: update_window_title; highlighter.rs: reset; lsp.rs: notify_change`
## 📄 File: src/app/ui_handlers.rs
#### `impl App`
### ⚡ fn handle_ui_click(&mut self, id: UiId)
  - **Logic Deps:** `editor.rs: select_word, get_full_text, select_line, insert_str, set_original_text, set_cursor_at_pos; lsp.rs: restart_python, enable_python, request_fix_all, servers_info, disable_python; main.rs: save_config, save_panel_state; app.rs: jump_to_search_result, toggle, trigger_folder_picker, trigger_save_as_picker, update_search, trigger_file_picker, update_window_title, is_open; highlighter.rs: reset`
## 📄 File: src/app/events.rs
#### `impl ApplicationHandler for App`
### ⚡ fn resumed(&mut self, event_loop: &ActiveEventLoop)
  - **Logic Deps:** `editor.rs: is_dirty; app.rs: update_window_title`
### ⚡ fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent)
  - **Logic Deps:** `ui_system.rs: mark_overlay_start, id, find_overlay_at; app.rs: any_bottom_open, show_action_dialog, update_window_title, trigger_file_picker, close_dialog, save_current_file, close_current_file, any_top_open; mouse.rs: handle_main_mouse_input, handle_main_cursor_moved, handle_main_mouse_wheel; editor.rs: is_dirty; lsp_ui.rs: draw_lsp_actions_menu`
### ⚡ fn about_to_wait(&mut self, event_loop: &ActiveEventLoop)
  - **Logic Deps:** `lsp.rs: apply_workspace_edit_to_text, servers_info; editor.rs: get_full_text, rebuild_line_offsets, is_dirty, insert_str, set_cursor_at_pos; lsp_actions.rs: apply_full_text_replacement; app.rs: update_autocomplete, load_file, apply_highlight_results, save_current_file, update_window_title, add_recent_file; main.rs: save_config`
## 📄 File: src/app/mouse.rs
#### `impl App`
### ⚡ fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta)
  - **Logic Deps:** `app.rs: any_bottom_open, is_open; settings_ui.rs: get_faq_max_scroll; lsp_actions.rs: lsp_server_logs_h, lsp_server_inner_size, lsp_panel_total_h, lsp_panel_bounds; renderer.rs: measure_ui_width; core_text.rs: get_max_scroll`
### ⚡ fn handle_main_mouse_input(&mut self, _event_loop: &ActiveEventLoop, state: ElementState)
  - **Logic Deps:** `lsp.rs: apply_workspace_edit_to_text, CodeAction; renderer.rs: get_ui_glyph; scroll.rs: clamp_current, clamp_target; lsp_actions.rs: insert_noqa_comment, apply_full_text_replacement; file_tree.rs: refresh_file_tree`
### ⚡ fn handle_main_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>)
  - **Logic Deps:** `renderer.rs: get_ui_glyph, char_advance; editor.rs: text_parts, get_full_text, set_cursor_at_pos; lsp_actions.rs: lsp_server_logs_h, lsp_server_inner_size, lsp_panel_total_h, lsp_panel_bounds; core_text.rs: get_max_scroll; file_tree.rs: file_tree_node_at`
## 📄 File: src/render_view/core_text.rs
#### `impl Renderer`
### ⚡ fn update_cache(&mut self, editor: &Editor, scroll_x: f32, scroll_y: f32, _is_resizing: bool,)
  - **Logic Deps:** `editor.rs: text_parts, byte_at; renderer.rs: char_advance, measure_ui_width`
### ⚡ fn get_cursor_xy(&mut self, editor: &Editor) -> (f32, f32)
  - **Logic Deps:** `editor.rs: text_parts, byte_at; core_text.rs: measure_width; renderer.rs: char_advance, measure_ui_width`
### ⚡ fn get_byte_at_xy(&mut self, editor: &Editor, target_x: f32, target_y: f32) -> usize
  - **Logic Deps:** `editor.rs: text_parts, byte_at; renderer.rs: char_advance`
### ⚡ fn measure_width(&mut self, first: &str, second: &str, start: usize, end: usize) -> f32
  - **Logic Deps:** `renderer.rs: char_advance`
### ⚡ fn get_max_scroll(&mut self, editor: &Editor, window_height: f32) -> f32
### ⚡ fn flush(&mut self)
### ⚡ fn push_vertical_gradient(&mut self, x: f32, y: f32, w: f32, h: f32, top: [f32; 4], bottom: [f32; 4],)
### ⚡ fn draw_string(&mut self, text: &str, mut x: f32, y: f32, color: [f32; 4])
  - **Logic Deps:** `renderer.rs: get_glyph, push_quad`
### ⚡ fn draw_string_scaled(&mut self, text: &str, mut x: f32, y: f32, color: [f32; 4], scale: f32,)
  - **Logic Deps:** `renderer.rs: get_ui_glyph, push_quad`
### ⚡ fn draw_string_mono_scaled(&mut self, text: &str, mut x: f32, y: f32, color: [f32; 4], scale: f32,)
  - **Logic Deps:** `renderer.rs: push_quad, get_glyph`
### ⚡ fn measure_mono_width(&mut self, text: &str, scale: f32) -> f32
  - **Logic Deps:** `renderer.rs: char_advance`
### ⚡ fn push_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: [f32; 4])
## 📄 File: src/render_view/ui.rs
#### `impl Renderer`
### ⚡ fn draw_icon(&mut self, tex: &glow::Texture, x: f32, y: f32, w: f32, h: f32)
  - **Logic Deps:** `renderer.rs: push_quad`
### ⚡ fn draw_atlas_icon(&mut self, icon: crate::widgets::IconType, x: f32, y: f32, size: f32, color: [f32; 4],)
  - **Logic Deps:** `renderer.rs: push_quad`
### ⚡ fn draw_file_icon(&mut self, key: &'static str, _is_folder: bool, x: f32, y: f32, size: f32,)
  - **Logic Deps:** `renderer.rs: push_quad`
### ⚡ fn draw_autocomplete(&mut self, x: f32, mut y: f32, options: &[(crate::highlighter::CompletionItem, Vec<usize>)
  - **Logic Deps:** `core_text.rs: measure_width, draw_string_scaled, push_rounded_rect; renderer.rs: get_glyph, push_quad`
### ⚡ fn draw_dialog_window(&mut self, base_title: &str) -> bool
  - **Logic Deps:** `core_text.rs: draw_string_scaled, push_rounded_rect, push_vertical_gradient; ui_system.rs: register_button, wants_pointer; widgets.rs: get_dialog_buttons; ui.rs: draw_atlas_icon`
### ⚡ fn draw_welcome(&mut self, recent_files: &[std::path::PathBuf], ui_registry: &mut crate::ui_system::UiRegistry,) -> bool
  - **Logic Deps:** `ui_system.rs: wants_pointer, register_rect, register_button; renderer.rs: push_rect, measure_ui_width; widgets.rs: get_welcome_buttons; ui.rs: draw_icon; core_text.rs: push_vertical_gradient, draw_string_scaled, push_rounded_rect`
## 📄 File: src/render_view/sticky.rs
#### `impl Renderer`
### ⚡ fn draw_sticky_lines(&mut self, editor: &Editor, spans: &[ColorSpan], current_sticky_lines: &[(usize, usize)
  - **Logic Deps:** `editor.rs: text_parts; renderer.rs: char_advance, push_rect, measure_ui_width, get_glyph, push_quad; ui_system.rs: register_rect; core_text.rs: push_vertical_gradient, draw_string_scaled`
## 📄 File: src/render_view/search.rs
#### `impl Renderer`
### ⚡ fn draw_search_panel(&mut self, search_anim_y: f32, search_editor: &Editor, search_focused: bool, search_case_sensitive: bool, search_results: &[(usize, usize)
  - **Logic Deps:** `renderer.rs: get_ui_glyph, push_rect, push_quad; ui_system.rs: register_text_input, wants_pointer, register_icon_button; editor.rs: get_full_text; core_text.rs: draw_string_scaled, push_rounded_rect`
## 📄 File: src/render_view/settings_ui.rs
#### `impl Renderer`
### ⚡ fn get_faq_max_scroll(&mut self, faq_editor: &Editor, dialog_height: f32) -> f32
  - **Logic Deps:** `editor.rs: get_full_text`
### ⚡ fn draw_settings(&mut self, anim_progress: f32, active_tab: usize, faq_editor: &Editor, scroll_y: f32, ide_workspaces: &[std::path::PathBuf], ide_ignore_patterns: &[String], settings_ignore_editor: &Editor, settings_ignore_focused: bool, settings_ignore_scroll_x: &mut f32, ide_scroll_y: f32, blink_alpha: f32, ui_registry: &mut crate::ui_system::UiRegistry,) -> u8
  - **Logic Deps:** `widgets.rs: render; renderer.rs: get_ui_glyph, push_rect, measure_ui_width, push_rounded_rect_gradient, push_quad; ui_system.rs: register_text_input, wants_text, wants_pointer, register_rect, reset_cursor_state; editor.rs: get_full_text; settings_ui.rs: get_faq_max_scroll`
## 📄 File: src/render_view/lsp_ui.rs
#### `impl Renderer`
### ⚡ fn draw_lsp_servers_panel(&mut self, content_x: f32, content_y: f32, content_w: f32, content_h: f32, s: f32, ide_panel: &crate::app::IdePanelState, fix_all_active: bool, ui_registry: &mut crate::ui_system::UiRegistry,)
  - **Logic Deps:** `editor.rs: text_parts; core_text.rs: measure_width, draw_string_scaled, push_rounded_rect; ui_system.rs: register_rect, register_blocker; renderer.rs: char_advance, measure_ui_width, get_glyph, push_quad`
### ⚡ fn draw_lsp_actions_menu(&mut self, menu: &crate::app::LspActionsMenu, _blink_alpha: f32,) -> bool
  - **Logic Deps:** `lsp.rs: CodeAction; core_text.rs: draw_string_scaled, push_rounded_rect; renderer.rs: measure_ui_width`