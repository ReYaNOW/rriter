# Follow-up F2: Database panel "Ожидание загрузки…" after opening panel from sidebar

## User report (verbatim intent)
After restart, if the editor starts with the Database panel NOT open, then the user opens the Database panel from the left sidebar, an expanded connection shows "Ожидание загрузки…" and stays until collapse/expand. If the editor starts with the Database panel already open, no problem.

## Root cause (verified by controller)
- Startup with panel open: `app_ide_tab_methods.rs:~460` calls `reconcile_expanded_database_connections()` → catalog load starts. OK.
- `UiId::SidebarSlot(panel_id)` handler in `src/app/ui_handlers.rs:~1389-1415` also calls `reconcile_expanded_database_connections()` when Database is opened — but a real sidebar click never reaches it:
  - press on `SidebarSlot` only creates `self.ide_panel.drag = Some(PanelDragState { threshold_passed: false, .. })` (`src/app/mouse/input.rs:~1418`), no `handle_ui_click`;
  - release with `!drag.threshold_passed` toggles the panel at `src/app/mouse/input.rs:~1848` ("Клик без движения → переключить панель"), which handles Explorer/Search side effects but NOT Database reconcile.
- Result: restored expanded connection stays `DatabaseConnectionChildrenState::ExpandedUnloaded` → renderer hint "Ожидание загрузки…" (`ide_panel_database_renderer.rs:~333`). Collapse/expand triggers `toggle_connection_expansion` → load.

## Required fix (minimal, surgical)
In the release-without-threshold toggle path (`input.rs:~1848`), when the toggle OPENED `PanelId::Database`, call `self.reconcile_expanded_database_connections()` (same call the SidebarSlot handler uses). Do not refactor or change other panels' behavior. No duplicated logic beyond that one guarded call.

## Test (TDD, must fail first)
Behavioral regression test through the real mouse path: IDE-mode test App with a restored database panel state containing an expanded, unloaded connection (see `src/app/database/database_panel_restored_expansion_tests.rs` for existing fixtures/patterns of restored expansion + reconcile), Database panel closed; register a `UiId::SidebarSlot(PanelId::Database)` rect in `app.ui_registry` under the mouse; drive press + release via the test entry `reviewer_markdown_read_mouse_input` (it calls `handle_main_mouse_input_inner(None, ..)`) or an equivalent existing `#[cfg(test)]` mouse helper; assert panel is open AND the connection is no longer `ExpandedUnloaded` (loading started / pending job or queued command exists, whichever the existing reconcile tests assert). If the real mouse path cannot run headless (e.g. window unwrap), report NEEDS_CONTEXT with the exact blocker instead of writing a source-text test.
