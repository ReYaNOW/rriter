# Final review fix wave — report

Status: DONE

## Per finding

### 1. wheel.rs over 1600-line cap (refactor)
- `src/render_view/markdown_code_scroll.rs:486-514` — new `App::try_markdown_code_wheel(hovered, mx, my, dx, dy, shift) -> bool`. Same expression as before, in the same order: Read mode, then `shift || |dx|>|dy|`, then hovered `MarkdownReadBody | MarkdownCodeCopy(_) | MarkdownCodeScrollbarX(_)`, then `scroll_markdown_code_block_x_at(mx, my, shift ? dy : dx)`. If it scrolls, it requests a redraw (`if let Some(window)`) and returns true.
- `src/app/mouse/wheel.rs:1310-1312` — the call site is now 3 lines. It runs right after `let hovered = ...find_at` and before `handle_markdown_read_wheel`, and falls through on false, exactly as before. `UiId` is `Copy`, so `hovered` is still usable afterwards.

### 2. Duplicate search scrollbar_x block in events.rs (refactor)
- `src/app/events.rs:302-324` — new `impl App { fn search_panel_scrollbar_x_for_mode(&self, window_w, minimap_w, scrollbar_w, s) -> f32 }`. Its body is the old block unchanged (`read_w` Option, then `search_panel_scrollbar_x`).
- The two call sites are `src/app/events.rs:840-841` (cursor-moved search hover) and `:1425-1430` (cursor icon). Both return the same values as before.

### 3. Dead `code_block_max_scroll_x` (removal)
- Removed `MarkdownReadLayoutCache::code_block_max_scroll_x` (was `markdown_code_scroll.rs:136-147`) and its only caller, the test `code_block_max_scroll_x_matches_geometry_and_rejects_other_ids` (was ~:668-682). Nothing else was removed; `code_block_by_id` is still used.

### 4. `scroll.current` stays above a shrunk `max_scroll` (behavior)
- `markdown_code_scroll.rs:383` — `scroll_code_block_x` now calls `state.clamp_current(0.0, max_scroll)` right after `clamp_target`. The existing finite guards on `max_scroll` and `delta` are kept, and `clamp_current` maps a non-finite `current` to 0.
- The drag path had the same problem: `apply_scrollbar_drag_target` only sets `target`. So `begin_code_scroll_drag` (:397) and `drag_code_scroll_to` (:429) now call `state.clamp_current(0.0, g.max_scroll)` before the thumb offset is computed.
- Test: `markdown_code_scroll_tests::code_scroll_input_clamps_current_after_layout_shrinks_max_scroll` (:732). It covers wheel, a NaN `current`, drag begin and drag update.

### 5. Code-scroll drag survives mode switch / tab swap (behavior)
- `src/app/markdown.rs:421` and `:451` — `self.markdown.end_code_scroll_drag()` added next to both `self.scroll_y.end_drag()` calls in `set_markdown_mode` (the reverse-unresolved early return and the normal path).
- `src/app/app_ide_tab_methods.rs:660` — `self.markdown.end_code_scroll_drag()` added in `sync_active_tab`, just before `std::mem::swap(&mut self.markdown, &mut self.tabs[ai].markdown)`.
  - `switch_to_tab` already ended the drag through `cancel_pointer_interactions`. Other tab-switch paths skip that, e.g. `open_new_tab`, close tab, git diff, API and Database tab opens.
  - All callers of `sync_active_tab`/`save_tabs_state` are one-off tab events, never per-frame, so a live drag is never cut off.
- Tests (GL fixture):
  - `markdown_code_scroll_gl_tests::markdown_mode_switch_ends_code_scrollbar_drag` (:1072)
  - `markdown_code_scroll_gl_tests::tab_swap_ends_code_scrollbar_drag` (:1083), which uses the real `open_new_tab` path; `save_open_tabs` is a no-op under `cfg(test)`.

## TDD evidence (4, 5)
RED — the tests were added before the fixes (refactors 1-3 were already applied):
```
make test TEST_FILTER='markdown_code_scroll'
test ...markdown_code_scroll_gl_tests::markdown_mode_switch_ends_code_scrollbar_drag ...
  panicked at src/render_view/markdown_code_scroll.rs:1064:9: left: Some(7) right: None
test ...markdown_code_scroll_gl_tests::tab_swap_ends_code_scrollbar_drag ...
  panicked at src/render_view/markdown_code_scroll.rs:1064:9: left: Some(7) right: None
test ...markdown_code_scroll_tests::code_scroll_input_clamps_current_after_layout_shrinks_max_scroll ...
  panicked at src/render_view/markdown_code_scroll.rs:734:9: left: (500.0, 100.0) right: (100.0, 100.0)
test result: FAILED. 10 passed; 3 failed; 0 ignored; 0 measured; 2374 filtered out
```
GREEN, after the fixes:
```
make test TEST_FILTER='markdown_code_scroll'
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 2374 filtered out; finished in 1.06s
```

## Covering test runs (all exit 0)
```
make test TEST_FILTER='markdown_code_scroll' -> ok. 13 passed; 0 failed
make test TEST_FILTER='app::mouse'           -> ok. 141 passed; 0 failed
make test TEST_FILTER='app::markdown'        -> ok. 42 passed; 0 failed
make test TEST_FILTER='search'               -> ok. 109 passed; 0 failed
make test TEST_FILTER='app::events'          -> ok. 65 passed; 0 failed
```
The known pre-existing failure `reviewer_reader_v1_all_block_selection_seating_uses_real_vertices` is not matched by these filters.

## cargo check
`CARGO_BUILD_JOBS=4 RUSTFLAGS="-C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld" cargo +nightly check --target x86_64-unknown-linux-gnu --release` finished with exit 0 and 10 warnings.
- The `code_block_max_scroll_x is never used` warning is gone.
- None of the remaining warnings is in a file touched by this wave. They are:
  - unused `MarkdownDocument` import at markdown_read.rs:7 — present at HEAD, used only by a test
  - automation_database `is_timed_scroll`
  - 5 `database_query_history_*` functions
  - `git_logs_max_scroll_from_content_height`
  - `line_at`
  - `should_draw_empty_ide_file_tree_overlay`

## Final line counts
- src/app/mouse/wheel.rs: 1588 (was 1602)
- src/app/events.rs: 1683 (was 1676)
  - It went up because the helper body (23 lines) is longer than the 17 lines saved at the two call sites.
  - The file was already over 1600 before this wave; it is not within the cap.
- src/app/markdown.rs: 1662 (was 1660; the limit given was 1662)
- src/render_view/markdown_code_scroll.rs: 1094 (was 1026)
- src/app/app_ide_tab_methods.rs: 1524 (was 1523)

## Concerns
- events.rs grew by 7 lines and was already over the 1600 cap; a helper in events.rs was an allowed placement for finding 2.
- Finding 5 is fixed in `sync_active_tab`, which covers every tab-switch path, not only `switch_to_tab`.
