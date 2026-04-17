# RRiter PROJECT_MAP
# AUTO-GENERATED. Команда: make api-tree (python3 gen_project_map.py)
#
# pub fn name  [LINE] -> RetType
#   SELF:  fn1, fn2            <- вызовы fn из этого же файла
#   CALL module: fn1, fn2      <- вызовы fn из другого файла
#   WRITE: field, other.push   <- self.field мутации / присваивания
#   MATCH: Enum::Variant        <- паттерны match-веток
# enum Name: Var1, Var2         <- варианты enum

FILE src/app/events.rs
  module: app::events

      fn resumed  [19]
    CALL app: update_window_title
    WRITE: gl_config, gl_context, gl_surface, renderer, window

      fn window_event  [102]
    CALL app: update_window_title
    CALL main: save_config
    CALL render_view: draw
    CALL render_view::core_text: get_cursor_xy, get_max_scroll
    CALL render_view::lsp_ui: draw_lsp_actions_menu
    CALL render_view::settings_ui: draw_settings
    CALL render_view::ui: draw_autocomplete
    CALL ui_system: id
    CALL widgets: get_dialog_buttons
    WRITE: autocomplete_rect, current_cursor, is_focused, is_ready, last_resize_time, modifiers, target_sticky_lines, tried_maximize, ui_registry.clear
    MATCH: PendingAction::CloseFile, PendingAction::OpenFile, PendingAction::Quit

      fn about_to_wait  [652]
    CALL app: update_window_title
    CALL main: save_config
    CALL render_view::core_text: get_max_scroll
    WRITE: autocomplete_anim_progress, base_title, current_sticky_lines, file_extension, file_path, ide_workspaces.push, last_blink_state, last_frame, last_resize_time, open_file_rx, open_folder_rx, pending_fix_all_id, save_file_rx, settings_anim_progress, settings_tab, settings_y, sticky_anim_is_adding, sticky_anim_progress
    MATCH: LspEvent::CodeActions, LspEvent::Diagnostics, LspEvent::Log, LspEvent::ServerReady, LspEvent::StatusChanged

────────────────────────────────────────────────────────────

FILE src/app/file_icons.rs
  module: app::file_icons
  types:  FallbackMatcher

  pub fn file_icon_key  [41] -> &'static str

  pub fn folder_icon_key  [82] -> &'static str

  pub fn svg_for_key  [96] -> &'static [u8]

────────────────────────────────────────────────────────────

FILE src/app/file_tree.rs
  module: app::file_tree
  types:  FileNode

  pub fn matches_ignore_pattern  [42] -> bool

  pub fn pre_rasterize_icon  [94]
    CALL app::file_icons: svg_for_key
    CALL widgets: render

      fn read_children  [135] -> (Vec<(String, PathBuf)>, Vec<(String, PathBuf)>)

      fn scan_dir_parallel  [178] -> Vec<FileNode>
    SELF:  matches_ignore_pattern, read_children
    CALL app::file_icons: file_icon_key, folder_icon_key

  pub fn spawn_scan  [274] -> mpsc::Receiver<Vec<FileNode>>
    SELF:  pre_rasterize_icon, scan_dir_parallel

  pub fn spawn_watcher  [341]

  pub fn refresh_file_tree  [378]
    SELF:  spawn_scan
    WRITE: file_tree_rx

  pub fn poll_file_tree  [406] -> bool
    WRITE: file_tree_rx

  pub fn start_file_watcher  [422]
    SELF:  spawn_watcher
    WRITE: file_tree_notify_rx

  pub fn handle_file_tree_click  [435]

  pub fn file_tree_node_at  [454] -> Option<usize>

────────────────────────────────────────────────────────────

FILE src/app/keyboard.rs
  module: app::keyboard

  pub fn handle_search_keyboard_input  [8]
    WRITE: last_action, search_current_idx, search_focused, search_results.clear, show_search

  pub fn handle_editor_keyboard_input  [131]
    CALL app: ensure_cursor_visible, update_window_title
    CALL main: save_config
    CALL render_view::core_text: get_cursor_xy, get_max_scroll
    WRITE: autocomplete_active, autocomplete_selected_idx, is_dragging, is_highlighted_once, last_action, last_sent_version, lsp_actions_menu, search_current_idx, search_focused, search_results.clear, show_search, show_settings
    MATCH: UndoRedoDelta::Delete, UndoRedoDelta::Insert, UndoRedoDelta::Replace

  pub fn handle_main_keyboard_input  [753]
    CALL main: save_config
    WRITE: ide_ignore_patterns.push, last_action, settings_ignore_focused, settings_tab, show_fps, show_settings

────────────────────────────────────────────────────────────

FILE src/app/lsp_actions.rs
  module: app::lsp_actions

  pub fn lsp_panel_bounds  [5] -> Option<(f32, f32, f32, f32)>

  pub fn lsp_panel_total_h  [46] -> f32

  pub fn lsp_server_logs_h  [55] -> f32

  pub fn lsp_server_inner_size  [63] -> (f32, f32)

  pub fn open_lsp_actions_menu  [98]
    CALL lsp: offset_to_lsp_pos
    CALL render_view::core_text: get_cursor_xy
    WRITE: lsp_actions_menu

  pub fn apply_selected_lsp_action  [180]
    WRITE: pending_fix_all_id
    MATCH: LspActionItem::AddNoqa, LspActionItem::AddNoqaAll, LspActionItem::CodeAction, LspActionItem::FixAll, LspActionItem::OrganizeImports

  pub fn insert_noqa_comment  [222]
    CALL app: update_window_title

  pub fn apply_workspace_edit  [310]
    CALL app: update_window_title
    CALL lsp: lsp_pos_to_offset

────────────────────────────────────────────────────────────

FILE src/app/mouse.rs
  module: app::mouse

  pub fn handle_main_mouse_wheel  [7]
    CALL render_view::core_text: get_max_scroll
    CALL render_view::settings_ui: get_faq_max_scroll
    CALL renderer: measure_ui_width
    WRITE: lsp_actions_menu, settings_tab

  pub fn handle_main_mouse_input  [255]
    CALL main: save_panel_state
    CALL render_view::core_text: get_max_scroll
    CALL renderer: get_ui_glyph
    WRITE: autocomplete_active, autocomplete_selected_idx, is_dragging, is_dragging_lsp_log, is_dragging_search, is_dragging_settings_ignore, last_action, lsp_actions_menu, pending_fix_all_id, settings_ignore_focused, show_settings
    MATCH: LspActionItem::AddNoqa, LspActionItem::AddNoqaAll, LspActionItem::CodeAction, LspActionItem::FixAll, LspActionItem::OrganizeImports, UiId::SettingsIdeIgnoreInput

  pub fn handle_main_cursor_moved  [643]
    CALL render_view::core_text: get_max_scroll
    CALL renderer: get_ui_glyph
    WRITE: autocomplete_hovered_idx

────────────────────────────────────────────────────────────

FILE src/app/ui_handlers.rs
  module: app::ui_handlers

  pub fn handle_ui_click  [9]
    CALL app: update_window_title
    CALL main: save_config, save_panel_state
    WRITE: base_title, click_count, dialog_window, editor, file_extension, file_path, ide_ignore_patterns.push, ide_ignore_patterns.remove, ide_workspaces.remove, is_dragging, is_dragging_lsp_log, is_dragging_search, is_ide_mode, last_click_pos, last_click_time, lsp, pending_fix_all_id, search_case_sensitive, search_current_idx, search_focused, search_results.clear, settings_ignore_editor, settings_ignore_focused, settings_tab, show_search, show_welcome
    MATCH: PendingAction::CloseFile, PendingAction::OpenFile, PendingAction::Quit, UiId::BottomPanelBody, UiId::CopyDiagnostic, UiId::DialogCancel, UiId::DialogDiscard, UiId::DialogSave, UiId::EditorFoldArrow, UiId::EditorFoldDots, UiId::EditorMinimap, UiId::EditorScrollbarX, UiId::EditorTab, UiId::EditorTabClose, UiId::EditorTextBody, UiId::FileTreeNode, UiId::LspLogArea, UiId::LspLogFoldToggle, UiId::LspLogScrollX, UiId::LspLogScrollY, UiId::LspScrollX, UiId::LspScrollY, UiId::LspServerFixAll, UiId::LspServerLogs, UiId::LspServerRestart, UiId::LspServerStop, UiId::LspServerToggle, UiId::OpenDiagUrl, UiId::ResizeBottom, UiId::ResizeLeft, UiId::SearchCaseToggle, UiId::SearchClose, UiId::SearchInput, UiId::SearchNext, UiId::SearchPrev, UiId::SettingsIdeAddIgnore, UiId::SettingsIdeAddWorkspace, UiId::SettingsIdeIgnoreInput, UiId::SettingsIdeRemoveIgnore, UiId::SettingsIdeRemoveWorkspace, UiId::SettingsTab, UiId::SidebarSlot, UiId::StickyLine, UiId::WelcomeIdeMode, UiId::WelcomeNewFile, UiId::WelcomeOpenFile, UiId::WelcomeRecentFile

────────────────────────────────────────────────────────────

FILE src/app.rs
  module: app
  types:  EditorTab, PendingAction, PanelId, PanelGroup, PanelSlot, PanelDragState, LspActionItem, LspActionsMenu, IdePanelState, App
  enum LspActionItem: CodeAction, AddNoqa, AddNoqaAll, FixAll, OrganizeImports
  enum PanelGroup: Top, Bottom
  enum PanelId: Explorer, Terminal, Problems, LspServers
  enum PendingAction: Quit, OpenFile, CloseFile

  pub fn label  [55] -> &'static str
    MATCH: PanelId::Explorer, PanelId::LspServers, PanelId::Problems, PanelId::Terminal

  pub fn icon  [63] -> crate::widgets::IconType
    MATCH: PanelId::Explorer, PanelId::LspServers, PanelId::Problems, PanelId::Terminal

      fn default  [145] -> Self

  pub fn any_top_open  [192] -> bool

  pub fn any_bottom_open  [197] -> bool

  pub fn toggle  [202]

  pub fn is_open  [207] -> bool

      fn fuzzy_match  [217] -> Option<Vec<usize>>

  pub fn sync_active_tab  [352]
    CALL app::file_icons: file_icon_key

  pub fn switch_to_tab  [373]
    SELF:  update_window_title
    WRITE: active_tab, autocomplete_active, show_welcome

  pub fn open_new_tab  [412]
    SELF:  update_window_title
    WRITE: active_tab, autocomplete_active, show_welcome, tabs.push

  pub fn close_tab_at  [441]
    SELF:  update_window_title
    WRITE: active_tab, autocomplete_active, show_welcome, tabs.remove

  pub fn open_file_in_tab  [468]

  pub fn ensure_cursor_visible  [488]

  pub fn get_current_word_prefix  [527] -> String

  pub fn update_autocomplete  [547]
    SELF:  fuzzy_match
    WRITE: autocomplete_active, autocomplete_anim_progress, autocomplete_options, autocomplete_options.clear, autocomplete_selected_idx
    MATCH: SymbolKind::Class, SymbolKind::Function, SymbolKind::Keyword, SymbolKind::Parameter, SymbolKind::Unknown

  pub fn ensure_autocomplete_visible  [626]

  pub fn apply_autocomplete  [656]
    SELF:  update_window_title
    WRITE: autocomplete_active, autocomplete_selected_idx

  pub fn update_search  [690]
    WRITE: search_current_idx, search_results.clear, search_results.push

  pub fn jump_to_search_result  [746]

  pub fn update_window_title  [772]

  pub fn show_action_dialog  [781]
    WRITE: dialog_gl_surface, dialog_window, is_dragging, pending_action

  pub fn close_dialog  [823]
    WRITE: dialog_gl_surface, dialog_window

  pub fn close_current_file  [831]
    SELF:  update_window_title
    WRITE: autocomplete_active, base_title, editor, search_current_idx, search_results.clear, show_search, show_welcome

  pub fn trigger_file_picker  [871]
    WRITE: open_file_rx

  pub fn trigger_folder_picker  [880]
    WRITE: open_folder_rx

  pub fn trigger_save_as_picker  [891]
    WRITE: save_file_rx

  pub fn save_current_file  [903] -> bool
    MATCH: ErrorKind::PermissionDenied

  pub fn add_recent_file  [940]
    CALL main: save_recent_files
    WRITE: recent_files.insert, recent_files.retain, recent_files.truncate

  pub fn apply_highlight_results  [949]
    WRITE: is_highlighted_once

  pub fn load_file  [979]
    SELF:  update_window_title
    CALL main: save_recent_files
    WRITE: autocomplete_active, base_title, editor, file_extension, file_path, is_highlighted_once, last_sent_version, recent_files.retain, search_current_idx, search_results.clear, show_welcome

────────────────────────────────────────────────────────────

FILE src/editor.rs
  module: editor
  types:  LineModState, EditOp, HistoryStep, UndoRedoDelta, Editor
  enum EditOp: Insert, Delete, Replace
  enum LineModState: ModifiedUnsaved, ModifiedSaved
  enum UndoRedoDelta: Insert, Delete, Replace

      fn get_diff_info  [7] -> (Vec<bool>, Vec<bool>)

      fn is_delimiter  [83] -> bool

      fn char_class  [90] -> u8

  pub fn new  [158] -> Self

  pub fn shift_folds_insert  [187]
    WRITE: folded_start_bytes

  pub fn shift_folds_delete  [210]
    WRITE: folded_start_bytes

  pub fn rebuild_line_offsets  [239]
    WRITE: foldable_lines.clear, foldable_lines.insert, folded_lines.clear, folded_lines.insert, folded_start_bytes.clear, folded_start_bytes.insert, line_offsets, longest_line_idx

      fn get_line_hashes  [309] -> Vec<u64>

  pub fn ensure_indent_cache_updated  [332]
    WRITE: indent_cache.clear, last_indent_version, version

  pub fn get_cached_indent_levels  [403] -> &[u8]

  pub fn backspace  [407] -> Option<(usize, usize)>
    WRITE: cursor, selection_anchor, sync_edits.push

  pub fn set_original_text  [480]
    WRITE: original_hashes, saved_hashes

  pub fn mark_saved  [486]
    WRITE: saved_hashes

  pub fn is_dirty  [491] -> bool

  pub fn get_line_modification_state  [495] -> Option<LineModState>

  pub fn update_modifications  [499]
    SELF:  get_diff_info
    WRITE: deleted_gaps, is_dirty, line_states

  pub fn clear_history  [571]
    WRITE: history.clear, history_size, redo_stack.clear, sync_edits.clear

      fn push_history  [579]
    WRITE: history.pop_front, history.push_back, redo_stack.clear
    MATCH: EditOp::Delete, EditOp::Insert, EditOp::Replace

  pub fn undo  [658] -> Option<UndoRedoDelta>
    WRITE: cursor, history.pop_back, is_working_history, redo_stack.push_back, selection_anchor, sync_edits.push
    MATCH: EditOp::Delete, EditOp::Insert, EditOp::Replace, SyncEdit::Delete, UndoRedoDelta::Delete, UndoRedoDelta::Insert, UndoRedoDelta::Replace

  pub fn redo  [709] -> Option<UndoRedoDelta>
    WRITE: cursor, history.push_back, is_working_history, redo_stack.pop_back, selection_anchor, sync_edits.push
    MATCH: EditOp::Delete, EditOp::Insert, EditOp::Replace, SyncEdit::Delete, UndoRedoDelta::Delete, UndoRedoDelta::Insert, UndoRedoDelta::Replace

  pub fn text_parts  [760] -> (&str, &str)

  pub fn get_full_text  [768] -> String

      fn move_gap  [776]

      fn insert_str_internal  [795] -> usize
    WRITE: data, gap_end, sync_edits.push

  pub fn replace_range  [819] -> (usize, usize, String)
    WRITE: cursor, selection_anchor, sync_edits.push

  pub fn insert_str  [860] -> (Option<(usize, usize)>, usize)
    WRITE: selection_anchor

  pub fn delete_selection  [887] -> Option<(usize, usize)>
    WRITE: cursor, selection_anchor

  pub fn delete_forward  [925] -> Option<(usize, usize)>
    WRITE: sync_edits.push

  pub fn delete_word_backward  [961] -> Option<(usize, usize)>
    SELF:  is_delimiter
    WRITE: cursor, selection_anchor

  pub fn delete_word_forward  [994] -> Option<(usize, usize)>
    SELF:  is_delimiter
    WRITE: cursor, selection_anchor

  pub fn get_auto_indent  [1028] -> String
    SELF:  char_class, select_expand

  pub fn select_expand  [1058]
    SELF:  char_class

────────────────────────────────────────────────────────────

FILE src/highlighter.rs
  module: highlighter
  types:  ColorSpan, SymbolKind, CompletionItem, SyncEdit, HighlighterMessage, Highlighter, Scope
  enum HighlighterMessage: Reset, Edits
  enum SymbolKind: Variable, Function, Class, Parameter, Keyword, Unknown
  enum SyncEdit: Insert, Delete

      fn get_point  [86] -> tree_sitter::Point

      fn resolve_color  [99] -> [f32; 4]

  pub fn new  [180] -> Self
    SELF:  get_point
    CALL queries: get_ts_config
    MATCH: HighlighterMessage::Edits, HighlighterMessage::Reset, SyncEdit::Delete, SyncEdit::Insert

      fn impl  [996]

────────────────────────────────────────────────────────────

FILE src/lsp.rs
  module: lsp
  types:  LspServerStatus, LogEntry, LspServerInfo, DiagSeverity, QuickFix, Diagnostic, TextChange, WorkspaceEdit, LspEvent, CodeAction, LspServerDef, Cmd, SpawnedProcess, OpenFile, LspProcess, LspManager
  enum Cmd: Restart, Open, Change, Close, CodeAction, Shutdown
  enum DiagSeverity: Error, Warning, Info, Hint
  enum LspEvent: Log, Diagnostics, CodeActions, ServerReady, StatusChanged
  enum LspServerStatus: Starting, Running, Crashed, Disabled

      fn next_id  [30] -> i32

  pub fn offset_to_lsp_pos  [205] -> (u32, u32)

      fn json_escape  [225] -> String

      fn path_to_uri  [244] -> String

      fn uri_to_path  [258] -> PathBuf

      fn make_initialize  [269] -> Vec<u8>
    SELF:  json_escape, path_to_uri
    CALL ui_system: id

      fn make_initialized  [292] -> Vec<u8>

      fn make_did_open  [296] -> Vec<u8>
    SELF:  json_escape

      fn make_did_change_full  [307] -> Vec<u8>
    SELF:  json_escape

      fn make_did_close  [317] -> Vec<u8>
    SELF:  json_escape

      fn make_code_action  [325] -> Vec<u8>
    SELF:  json_escape

      fn make_shutdown  [349] -> Vec<u8>

      fn make_exit  [353] -> Vec<u8>

      fn write_frame  [359] -> bool

      fn parse_diagnostic_value  [371] -> Option<Diagnostic>
    SELF:  parse_text_edit_value

      fn parse_text_edit_value  [449] -> Option<TextChange>

      fn parse_workspace_edit_value  [474] -> WorkspaceEdit
    SELF:  uri_to_path

      fn parse_code_action_value  [510] -> Option<CodeAction>

      fn dispatch_frame  [540]
    SELF:  parse_diagnostic_value, parse_workspace_edit_value, uri_to_path
    MATCH: LspEvent::CodeActions, LspEvent::Diagnostics, LspEvent::Log

      fn spawn_server  [680] -> Option<SpawnedProcess>
    SELF:  dispatch_frame, write_frame

      fn send_and_log  [804] -> Result<(), mpsc::SendError<Vec<u8>>>

      fn run_supervisor  [820]
    SELF:  make_code_action, make_did_change_full, make_did_close, make_did_open, make_exit, make_initialize, make_initialized, make_shutdown, next_id, send_and_log, spawn_server
    MATCH: Cmd::Change, Cmd::Close, Cmd::CodeAction, Cmd::Open, LspEvent::StatusChanged

      fn start  [1013] -> Self
    SELF:  run_supervisor

  pub fn notify_open  [1033]
    SELF:  path_to_uri
    WRITE: current_uri, open_file_data

  pub fn restart  [1046]

  pub fn notify_change  [1052]
    SELF:  path_to_uri
    WRITE: current_uri

  pub fn notify_close  [1063]
    SELF:  path_to_uri
    WRITE: current_uri

  pub fn request_code_actions  [1071] -> i32
    SELF:  encode_diagnostics_json, next_id, path_to_uri

  pub fn poll  [1101] -> Vec<LspEvent>

  pub fn shutdown  [1113]

      fn encode_diagnostics_json  [1120] -> String
    SELF:  json_escape
    MATCH: DiagSeverity::Error, DiagSeverity::Hint, DiagSeverity::Info, DiagSeverity::Warning

  pub fn new  [1169] -> Self

      fn ensure_python  [1183]
    SELF:  start
    WRITE: python, python_status

  pub fn restart_python  [1191]
    SELF:  start
    WRITE: python, python_status

  pub fn disable_python  [1202]
    WRITE: diagnostics.clear, python_disabled, python_status, server_logs.clear

  pub fn enable_python  [1213]
    SELF:  start
    WRITE: python, python_disabled, python_status

  pub fn servers_info  [1228] -> Vec<LspServerInfo>

      fn process_for_ext  [1242] -> Option<&mut LspProcess>

  pub fn notify_open  [1253]
    WRITE: current_path, diagnostics.remove, suppress_diagnostics

  pub fn notify_change  [1270]
    WRITE: suppress_diagnostics

  pub fn notify_close  [1285]
    WRITE: diagnostics.remove

  pub fn request_code_actions  [1300] -> Option<i32>

  pub fn poll  [1332] -> Vec<LspEvent>
    SELF:  format_and_highlight_json
    WRITE: diagnostics.insert, python_status
    MATCH: LspEvent::Diagnostics, LspEvent::Log, LspEvent::StatusChanged

  pub fn get_diagnostics  [1370] -> &[Diagnostic]

  pub fn diagnostics_for_line  [1382] -> Vec<&Diagnostic>

  pub fn request_fix_all  [1390] -> Option<i32>
    SELF:  next_id, path_to_uri

  pub fn request_organize_imports  [1414] -> Option<i32>
    SELF:  next_id, path_to_uri

  pub fn shutdown  [1439]
    WRITE: python_disabled

  pub fn lsp_pos_to_offset  [1451] -> usize

  pub fn apply_workspace_edit_to_text  [1476] -> String
    SELF:  lsp_pos_to_offset

  pub fn format_and_highlight_json  [1500] -> ( String, Vec<crate::highlighter::ColorSpan>, Vec<(usize, usize)>, )
    CALL queries: get_folding_query, get_ts_config

────────────────────────────────────────────────────────────

FILE src/main.rs
  module: main
  types:  Config

      fn default  [33] -> Self

  pub fn load_recent_files  [44] -> Vec<PathBuf>

  pub fn save_recent_files  [60]

  pub fn save_panel_state  [74]
    MATCH: PanelGroup::Bottom, PanelGroup::Top, PanelId::Explorer, PanelId::LspServers, PanelId::Problems, PanelId::Terminal

  pub fn load_panel_state  [104] -> crate::app::IdePanelState

  pub fn save_config  [158]

      fn load_config  [183] -> Config

────────────────────────────────────────────────────────────

FILE src/queries.rs
  module: queries

  pub fn get_params_query  [3] -> Option<&'static str>

  pub fn get_injection_query  [82] -> Option<&'static str>

  pub fn get_folding_query  [131] -> Option<&'static str>

  pub fn get_ts_config  [336] -> Option<(tree_sitter::Language, Vec<&'static str>)>

      fn test_all_tree_sitter_queries_are_valid  [925]
    SELF:  get_folding_query, get_injection_query, get_params_query, get_ts_config

────────────────────────────────────────────────────────────

FILE src/render_view/core_text.rs
  module: render_view::core_text

  pub fn update_cache  [6]
    SELF:  draw_string, draw_string_mono_scaled, draw_string_scaled, flush, get_byte_at_xy, get_cursor_xy, get_max_scroll, measure_mono_width, measure_width, push_rounded_rect, push_vertical_gradient
    WRITE: last_editor_version, last_height, last_scroll_x, last_scroll_y, last_width, vertices.clear, visual_lines.clear, visual_lines.push

  pub fn get_cursor_xy  [209] -> (f32, f32)
    SELF:  draw_string, draw_string_mono_scaled, draw_string_scaled, flush, get_byte_at_xy, get_max_scroll, measure_mono_width, measure_width, push_rounded_rect, push_vertical_gradient
    WRITE: vertices.clear

  pub fn get_byte_at_xy  [344] -> usize

  pub fn measure_width  [443] -> f32

  pub fn get_max_scroll  [469] -> f32

  pub fn flush  [488]
    WRITE: vertices.clear

  pub fn push_vertical_gradient  [530]

  pub fn draw_string  [578]

  pub fn draw_string_scaled  [601]

  pub fn draw_string_mono_scaled  [631]

  pub fn measure_mono_width  [661] -> f32

  pub fn push_rounded_rect  [672]

────────────────────────────────────────────────────────────

FILE src/render_view/lsp_ui.rs
  module: render_view::lsp_ui

  pub fn draw_lsp_servers_panel  [6]
    MATCH: LspServerStatus::Crashed, LspServerStatus::Disabled, LspServerStatus::Running, LspServerStatus::Starting

  pub fn draw_lsp_actions_menu  [780] -> bool
    MATCH: Cow::Borrowed, Cow::Owned, LspActionItem::AddNoqa, LspActionItem::AddNoqaAll, LspActionItem::CodeAction, LspActionItem::FixAll, LspActionItem::OrganizeImports

────────────────────────────────────────────────────────────

FILE src/render_view/search.rs
  module: render_view::search

  pub fn draw_search_panel  [8] -> bool
    WRITE: last_search_idx, last_search_len, search_res_string, search_res_string.clear, search_scroll_x

────────────────────────────────────────────────────────────

FILE src/render_view/settings_ui.rs
  module: render_view::settings_ui

  pub fn get_faq_max_scroll  [6] -> f32

  pub fn draw_settings  [31] -> u8

────────────────────────────────────────────────────────────

FILE src/render_view/sticky.rs
  module: render_view::sticky

  pub fn draw_sticky_lines  [7] -> Vec<(usize, usize)>
    WRITE: sticky_scroll_rects.clear, sticky_scroll_rects.push

────────────────────────────────────────────────────────────

FILE src/render_view/ui.rs
  module: render_view::ui

  pub fn draw_icon  [5]

  pub fn draw_atlas_icon  [17]

  pub fn draw_file_icon  [40]
    WRITE: file_icon_cache.insert

  pub fn draw_autocomplete  [129] -> (f32, f32, f32, f32)
    MATCH: SymbolKind::Class, SymbolKind::Function, SymbolKind::Keyword, SymbolKind::Parameter, SymbolKind::Unknown, SymbolKind::Variable

  pub fn draw_dialog_window  [358] -> bool
    CALL widgets: get_dialog_buttons

  pub fn draw_welcome  [470] -> bool
    CALL widgets: get_welcome_buttons
    WRITE: gl.clear

  pub fn draw_diagnostics_ruler  [714]
    MATCH: DiagSeverity::Error, DiagSeverity::Warning

────────────────────────────────────────────────────────────

FILE src/render_view.rs
  module: render_view
  types:  ModInterval, DiagLayout

  pub fn draw  [22] -> (bool, Vec<(usize, usize)>)
    WRITE: diag_hover_timer, diag_hover_timer_idx, fps, fps_string, fps_string.clear, frame_count, gl.clear, hide_popups_until_mouse_move, last_cursor_for_popups, last_diag_href, last_diag_popup_rect, last_draw_instant, last_editor_version_for_scroll_x, last_editor_version_for_typing, last_frame_time, last_hovered_diags, last_hovered_diags.clear, last_known_mouse, left_padding, max_scroll_x, minimap_width, phys_to_visual.clear, time_acc, visual_lines.clear
    MATCH: DiagSeverity::Error, DiagSeverity::Hint, DiagSeverity::Info, DiagSeverity::Warning

      fn draw_tab_bar  [1972]
    CALL app::file_icons: file_icon_key

      fn draw_minimap  [2058]

────────────────────────────────────────────────────────────

FILE src/renderer.rs
  module: renderer
  types:  Theme, Vertex, GlyphInfo, VisualLine, FontData, Renderer

  pub fn new  [144] -> Self

  pub fn get_custom_svg_glyph  [461] -> Option<GlyphInfo>
    CALL widgets: render
    WRITE: atlas_x, max_row_h

  pub fn get_glyph  [528] -> Option<GlyphInfo>
    CALL widgets: render
    WRITE: atlas_x, atlas_y, glyphs.clear, glyphs.insert, max_row_h, ui_glyphs.clear
    MATCH: Content::Mask

  pub fn get_ui_glyph  [692] -> Option<GlyphInfo>
    CALL widgets: render
    WRITE: atlas_x, atlas_y, glyphs.clear, max_row_h, ui_glyphs.clear, ui_glyphs.insert
    MATCH: Content::Mask

  pub fn resize  [855]
    WRITE: height, width

  pub fn measure_ui_width  [865] -> f32

  pub fn char_advance  [878] -> f32

  pub fn push_quad  [895]

  pub fn load_builtin_icons  [947]
    CALL widgets: render
    WRITE: icons.insert
    MATCH: IconType::Down

  pub fn push_squiggle  [1126]

  pub fn push_rect  [1179]

  pub fn push_rounded_rect_gradient  [1183]

────────────────────────────────────────────────────────────

FILE src/scroll.rs
  module: scroll
  types:  ScrollState

  pub fn new  [11] -> Self

  pub fn update  [22] -> bool
    WRITE: current, velocity

  pub fn clamp_target  [47]
    WRITE: target

  pub fn clamp_current  [51]
    WRITE: current

  pub fn scroll_by  [55]

  pub fn set_target  [59]
    WRITE: target

  pub fn stop_anim  [63]
    WRITE: current, target, velocity

────────────────────────────────────────────────────────────

FILE src/ui_system.rs
  module: ui_system
  types:  UiId, UiElement, UiRegistry
  enum UiElement: Button, IconButton, TextInput, Rect
  enum UiId: WelcomeNewFile, WelcomeOpenFile, WelcomeIdeMode, WelcomeRecentFile, DialogSave, DialogDiscard, DialogCancel, SettingsTab, SettingsIdeAddWorkspace, SettingsIdeRemoveWorkspace, SettingsIdeAddIgnore, SettingsIdeRemoveIgnore, SettingsIdeIgnoreInput, LspServerRestart, LspServerToggle, LspServerStop, LspServerLogs, LspServerFixAll, LspLogFoldToggle, SidebarSlot, FileTreeNode, SearchClose, SearchNext, SearchPrev, SearchCaseToggle, SearchInput, EditorTab, EditorTabClose, EditorFoldArrow, EditorFoldDots, StickyLine, EditorScrollbarY, EditorScrollbarX, EditorTextBody, EditorMinimap, ResizeLeft, ResizeBottom, BottomPanelBody, LspLogArea, LspScrollY, LspScrollX, LspLogScrollY, LspLogScrollX, CopyDiagnostic, OpenDiagUrl

  pub fn contains  [112] -> bool
    MATCH: UiElement::Button, UiElement::IconButton, UiElement::Rect, UiElement::TextInput

  pub fn id  [138] -> UiId
    MATCH: UiElement::Button, UiElement::IconButton, UiElement::Rect, UiElement::TextInput

  pub fn new  [162] -> Self

  pub fn clear  [173]
    WRITE: elements.clear, hovered, overlay_mark, wants_pointer, wants_text

  pub fn mark_overlay_start  [183]
    WRITE: overlay_mark

  pub fn find_overlay_at  [189] -> Option<UiId>

  pub fn register_button  [198] -> bool
    WRITE: elements.push, hovered, wants_pointer

  pub fn register_icon_button  [226] -> bool
    WRITE: elements.push, hovered, wants_pointer

  pub fn register_text_input  [254] -> bool
    WRITE: elements.push, hovered, wants_text

  pub fn register_blocker  [277] -> bool
    WRITE: elements.push

  pub fn register_rect  [294] -> bool
    WRITE: elements.push, hovered, wants_pointer

  pub fn find_at  [316] -> Option<UiId>

  pub fn hovered  [326] -> Option<UiId>

  pub fn reset_cursor_state  [333]
    WRITE: wants_pointer, wants_text

  pub fn wants_pointer  [339] -> bool

  pub fn wants_text  [344] -> bool

  pub fn cursor_code  [349] -> u8

────────────────────────────────────────────────────────────

FILE src/widgets.rs
  module: widgets
  types:  IconType, Button, IconButton
  enum IconType: Save, Discard, Cancel, Warning, CaseMatch, Up, Down, Close, Plus, Terminal, Explorer, Problems, LspServers, Copy, Check

  pub fn is_hovered  [34] -> bool

  pub fn render  [38] -> bool

  pub fn is_hovered  [118] -> bool

  pub fn render  [127] -> bool

  pub fn get_welcome_buttons  [216] -> (Button, Button, Button)

  pub fn get_dialog_buttons  [269] -> (Button, Button, Button)

────────────────────────────────────────────────────────────
