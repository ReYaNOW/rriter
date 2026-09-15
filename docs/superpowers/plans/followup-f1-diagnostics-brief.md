# Follow-up F1: Reader code-block horizontal scrollbar drag — diagnostic instrumentation (TEMPORARY)

## User report
Dragging the code-block horizontal scrollbar thumb in Markdown Reader almost never moves the content ("очень редко drag-тся, криво"; "вообще не двигается при drag"). Clicking the track does move it. Headless tests of the same App methods pass, so the defect is in the real event path, which the fixture cannot drive: `handle_main_cursor_moved` unwraps `self.window` at src/app/mouse/cursor.rs:~402.

## Already ruled out by code reading (do not re-investigate)
- The popup move gate (`POPUP_MOUSE_MOVE_EPS` 0.5 px) swallows only moves of 0.5 px or less.
- The early returns in `handle_main_cursor_moved` before the drag branch (~744) are all gated by unrelated state.
- `clamp_current` / `clamp_target` never touch `is_dragging`.
- Autosave-on-press runs only when the editor loses focus.
- `stop_click_scroll_anims` never touches code-scroll state.
- The `MarkdownReadBody` rect width equals the draw `content_w`.
- `WindowEvent::MouseInput` and `CursorMoved` route directly to the handlers.
- Draw and tick paths don't mutate `code_scroll_drag`.

## Goal
Collect evidence showing WHERE the drag breaks. Add stderr diagnostics, active ONLY when env var `RRITER_DEBUG_CODE_SCROLL` is set. Code must be allocation-free when the var is unset: check once via a `OnceLock<bool>` or `std::sync::LazyLock`. Mark every line with the comment `// TEMP F1 DIAG`.

Log points:
1. `App::begin_markdown_code_scrollbar_drag_at` (src/render_view/markdown_code_scroll.rs): block_id, pointer_x, whether geometry resolved (g.track_x, g.track_w, g.max_scroll, thumb), the begin result, and `code_scroll_drag` / `is_dragging` / `drag_offset` / `target` after.
2. `App::drag_markdown_code_scrollbar_to`: pointer_x, geometry resolved or not, result, then target / current / is_dragging / code_scroll_drag after.
3. `MarkdownTabState::drag_code_scroll_to`: when target is None, log which condition failed (not dragging, thumb None, or scrollbar_drag_target None, with its inputs).
4. `MarkdownTabState::end_code_scroll_drag`: when it ends an active drag, log `std::backtrace::Backtrace::force_capture()` so the caller is known.
5. src/app/mouse/cursor.rs `handle_main_cursor_moved`: at the very top, if `self.markdown.code_scroll_drag.is_some()`, log px/py. Right before the Reader code-drag branch (~744), log "reached code-drag branch". Together these show whether an early return swallows moves.
6. `MarkdownTabState::update_code_scroll_x`: while a drag is active, log current/target per tick. Rate-limit to at most one line per 100ms using an `Instant` stored in a `thread_local!`, or log only on change.

Do NOT change any behavior. Do NOT edit src/app/mouse/input.rs, src/app/ui_handlers.rs or any database file; a parallel fix may be touching input.rs.

## Verification
`make test TEST_FILTER='markdown_code_scroll'` passes; `cargo +nightly check --target x86_64-unknown-linux-gnu --release` passes with no new warnings.
