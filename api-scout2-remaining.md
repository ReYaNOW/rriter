# API Client second-pass scout

Read-only inventory; line spans/counts are inclusive. `T` means the active `ApiClientTabState`; it may be passed beside `&mut ApiClientState` (whose feature state is treated as the method receiver, not an App dependency). `R` renderer/text measurement/scale; `U` UI registry geometry; `C` clipboard; `H` App-owned channels, receivers, threads or async work; `D` native dialog; `F` focus, editor or other panels; `X` other App context (tabs, window, timers, platform, external feature state); `?` uncertain. App methods only; free helpers and `#[cfg(test)]` functions are excluded.

## Q1: per-tab state

`ApiClientTabState` is defined at `src/app/api_client.rs:930-969`; it already has `impl ApiClientTabState` at `:1047` (route reset/remember/restore methods through `:1199`). It is stored with identity in `EditorTabKind::ApiClient(ApiClientTabMeta, ApiClientTabState)` at `src/app/app_state.rs:69-72`. `App::active_api_tab_mut_for(spec_id)` at `src/app/api_client/api_client_app_text_methods.rs:1818-1828` borrows the active tab by `self.active_tab`, checks `meta.spec_id`, and returns mutable meta + state. Thus `&mut ApiClientTabState` plus `&mut ApiClientState` is a viable signature when the method's only App dependency is T: these come from disjoint `self.tabs` and `self.ide_panel.api` fields; method-specific geometry/focus/etc. should be passed as values/other refs. Existing state `impl` covers only the tab memory operations, not the six App method files.

## Q2: method inventory

Lengths count the Rust method source from signature through closing brace, inclusive. Grouping rows share the indicated classification.

### `api_client_app_click_methods.rs`

| Class | Methods (`line: length`) |
|---|---|
| T,F | `close_active_api_output_example_menu` 27:16 |
| T,R,U | `start_api_output_schema_menu_scroll_drag` 44:59; `update_api_output_schema_menu_scroll_drag` 104:62; `start_api_python_runtime_scroll_drag` 167:19; `update_api_python_runtime_scroll_drag` 187:22 |
| T,R,U,C,H,D,F,X | `handle_api_client_click` 210:1300 |

### `api_client_app_focus_methods.rs`

| Class | Methods (`line: length`) |
|---|---|
| T,F | `focus_next_api_input` 2:29; `apply_response_token_to_auth` 303:27 |
| T,X | `api_mock_input_schema_text_for_focus_route` 32:31 |
| T,F,X | `api_focus_text` 64:238; `commit_api_focus` 331:235 |

### `api_client_app_mock_contract_methods.rs`

| Class | Methods (`line: length`) |
|---|---|
| H,D,F,X | `trigger_api_mock_export_openapi` 2:24 |
| T,F,H | `edit_api_mock_contract` 29:13; `start_api_mock_contract_tools` 44:6 |
| T,F | `open_api_mock_route_reset_dialog` 51:7; `confirm_api_mock_route_reset` 59:8; `open_api_mock_contract_field_delete_dialog` 68:15; `confirm_api_mock_contract_field_delete` 84:14; `add_api_mock_contract_field_constraint` 99:25 |
| T,X | `api_mock_contract_source_for_route` 125:6; `api_mock_signature_for_route` 132:6; `api_mock_contract_field_prop_text` 139:16; `api_mock_generated_preview` 156:6 |

### `api_client_app_mock_methods.rs`

| Class | Methods (`line: length`) |
|---|---|
| R,X | `scroll_api_python_runtime_overlay` 2:12 |
| F,H,X | `toggle_api_mock_server` 15:13 |
| T | `api_route_override` 29:9; `api_active_route` 40:12; `active_manual_mock_route` 53:9; `active_manual_mock_route_mut` 63:9; `api_route_override_mut` 73:7; `ensure_api_route_override` 81:8; `api_route_python_script` 90:9; `api_route_python_script_mut` 100:9; `api_mock_route_context` 122:9; `api_mock_script_for_tools` 132:9; `api_mock_route_tools_version` 674:24 |
| T,F | `focus_previous_api_mock_python_part` 110:11; `api_mock_edit_text_for_part` 142:21 |
| T,F,X | `ensure_api_mock_hover_editor` 164:17; `api_mock_hover_module_path` 182:17; `api_mock_virtual_hover_source` 485:16; `clear_api_mock_hover_response` 502:15; `toggle_api_route_mock` 771:77; `toggle_api_route_python` 849:149; `reset_api_route_python_part` 999:56; `add_api_manual_route` 1056:28 |
| T,R,F,X | `notify_api_mock_lsp_source` 200:27; `move_api_mock_hover_to_empty_space` 228:15; `api_mock_hover_anchor_for_target` 245:37; `update_api_mock_hover_from_cursor` 283:201; `request_active_api_mock_hover` 518:55; `apply_api_mock_hover_response` 574:42; `refresh_api_mock_python_highlight` 617:47; `ensure_active_api_mock_highlight` 716:54; `api_mock_autocomplete_anchor` 1102:23; `apply_api_mock_autocomplete` 1164:115 |
| T,H,F,X | `queue_api_mock_python_tools` 665:8; `start_api_mock_route_tools_now` 699:16; `start_api_mock_ty_check_now` 1085:16; `request_api_mock_ty_autocomplete` 1133:10 |
| T,F,H,X | `update_api_mock_tree_sitter_autocomplete` 1126:6; `update_api_mock_ty_autocomplete` 1144:9; `update_api_mock_ty_signature_help_autocomplete` 1154:9 |

### `api_client_app_request_methods.rs`

| Class | Methods (`line: length`) |
|---|---|
| T,H,X | `allocate_api_request_id` 197:21; `start_active_api_request` 793:127; `start_active_manual_api_request` 921:96; `poll_api_client` 1018:363; `apply_api_job_response` 1452:32 |
| T,X | `sync_api_mock_proxy_base_to_active_server` 219:20; `update_api_tabs_after_model_load` 1382:69 |
| C,F,X | `copy_hover_popup_selection_or_diagnostic` 240:61 |
| T,F,X | `finish_api_text_edit` 302:65; `api_client_keyboard_surface_visible` 368:4; `handle_api_client_ime_commit` 373:73; `handle_api_client_keyboard_input` 447:319; `move_api_mock_python_vertical_or_focus` 767:25 |

### `api_client_app_text_methods.rs`

| Class | Methods (`line: length`) |
|---|---|
| T | `active_api_route_text` 8:9; `finish_api_route_text_selection` 108:16; `api_text_scroll_for_ui` 147:35; `api_text_scroll_x_for_ui` 183:59; `active_tab_is_api_client` 1804:5; `active_api_tab` 1810:7; `active_api_tab_mut_for` 1818:12; `last_api_route_tab_idx` 1436:9 |
| T,C | `copy_api_route_text_selection` 125:16 |
| X | `pulse_api_cursor_blink` 142:4 |
| T,R,U,F | `begin_api_route_text_selection` 18:48; `drag_api_route_text_selection_from_last_mouse` 67:40; `api_text_max_scroll_x_for_ui` 408:27; `api_text_max_scroll_y_for_ui` 436:50; `api_one_line_max_scroll_x_for_ui` 487:22; `sync_api_one_line_scroll_target` 510:32; `sync_api_multiline_scroll_target` 543:159; `start_api_text_scrollbar_x_drag` 703:57; `drag_api_text_scrollbar_x_from_last_mouse` 761:62; `start_api_text_scrollbar_y_drag` 824:57; `drag_api_text_scrollbar_y_from_last_mouse` 882:62; `place_api_cursor_from_last_click` 945:118; `drag_api_text_cursor_from_last_mouse` 1064:98 |
| T,R,F,X | `api_multiline_text_for_ui` 243:164 |
| D,H,X | `trigger_api_file_picker` 1164:28; `trigger_api_body_file_picker` 1193:46; `trigger_api_python_path_picker` 1241:25; `trigger_api_python_version_list` 1268:3; `trigger_api_python_install` 1273:3; `apply_api_body_file_pick` 1277:14 |
| H,X | `start_api_local_import` 1292:12; `start_api_url_import_from_input` 1305:33; `refresh_api_spec` 1339:32; `ensure_api_model_loaded` 1372:32 |
| T,F,X | `push_api_client_tab` 1405:30; `open_new_api_spec_tab` 1446:54; `open_api_spec_tab` 1501:18; `open_api_auth_tab` 1520:63; `open_api_route` 1584:3; `open_api_route_with_new_tab` 1588:47; `open_api_manual_route` 1636:114; `sync_api_manual_route_tabs` 1751:52; `sync_api_tab_inputs` 1831:50; `focus_api_input` 1882:65 |

## Q3: click match-arm groups

`handle_api_client_click` is one 1300-line dispatcher. Group classes below describe the App dependencies in those arms; groups can overlap because shared pre/post logic (`is_dragging`, cursor placement, redraw/blink) runs across arms.

| Lines | UiId family | Dependencies |
|---|---|---|
| 215-262 | shared text-input drag setup; Import menu/file/URL/input/confirm | T,F; import file adds D,H; cursor placement adds R,U |
| 264-318 | mock-server toggle/details/copy/log and log scrollbar | T,F,C; log scrollbar adds R,U; server start/stop side effect H,X |
| 319-392 | mock mode/proxy + guide open/close/body/scroll | T,F; guide scrollbar adds R,U |
| 393-478 | Python runtime management, version/path pickers, runtime scrolls, manual route path input | T,F; runtime checks/list/install add H,X; pickers add D,H; scrollbar adds R,U |
| 480-512 | mock route enable/details/python/reset/export | T,F; route tooling adds H,X; OpenAPI export adds D,H |
| 513-600 | contract field toggles/constraints/prop input | T,F,H; prop click also cursor geometry R,U |
| 601-660 | mock response/script inputs, manual-route creation/open/method/remove/reset | T,F; add/reset tooling adds H,X |
| 661-732 | spec open/refresh/remove/confirm/cancel/select | X,F,H; tab selection/opening and refresh/load coordination |
| 733-824 | auth/root/tags/routes/filter/route text/server select | T,F,X; route text selection adds R,U; server changes commit shared config |
| 825-933 | auth inputs/save/clear | T,F,X (focus/editor/auth persistence) |
| 934-990 | try request and allowed-value selection | T,F,H,X |
| 991-1013 | response body/headers/curl tabs | T,F |
| 1014-1098 | input/output example/schema tabs and input schema menu/items | T,F; menu geometry/scroll adds R,U |
| 1099-1192 | output status/schema menus and output schema item | T,F; menu drag adds R,U |
| 1193-1293 | schema body/fold click targets | T,F; body cursor placement adds R,U |
| 1294-1299 | response token actions | T,F,X |
| 1300-1351 | path/query/body inputs | T,F; cursor placement adds R,U |
| 1352-1363 | horizontal/vertical text scrollbars | T,R,U |
| 1364-1455 | body field/allowed-value actions | T,F,X |
| 1456-1479 | body file pick | T,D,H,X |
| 1481-1490 | response body click/cursor | T,F,R,U |
| 1491-1501 | API tab body clear focus/selection | T,F |

## Caveats

Class is an architectural extraction estimate from direct method bodies, not a borrow-checker experiment. `X` intentionally captures shared API feature state / spec model access, tab-list operations, window redraw, persistence and process/platform state where the other requested letters do not describe it. `?` was reserved but not required for these first-pass classifications; validate mixed click arms individually before splitting the dispatcher.
