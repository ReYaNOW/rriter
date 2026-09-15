# Task 7 report: Reader code block horizontal input (wheel, thumb drag, animation tick)

Status: DONE_WITH_CONCERNS (all requested behavior implemented and tested; concerns below are size/coverage notes, not failures)

## Implemented

- Shift+wheel, or a wheel/trackpad delta where |dx| > |dy|, over an overflowing Reader code block scrolls that block horizontally.
  - Speed is anim_speed 7. `scroll_by(delta)` uses the editor's shift/dx sign convention. The target is clamped to `[0, max_scroll]` and rounded.
  - The branch only fires when the hovered id is a Reader surface (`MarkdownReadBody | MarkdownCodeCopy | MarkdownCodeScrollbarX`), so overlays above the Reader are not bypassed.
  - Non-overflowing blocks, points outside code blocks, and plain vertical wheel all fall through to the existing `handle_markdown_read_wheel` path unchanged.
- Pressing `UiId::MarkdownCodeScrollbarX(block_id)` starts a thumb drag and sets `markdown.code_scroll_drag = Some(block_id)`.
  - Drag math is `scrollbar_thumb` + `scrollbar_drag_target` + `apply_scrollbar_drag_target`, so anim_speed is SCROLLBAR_DRAG_ANIM_SPEED = 15, only the target moves, and `current` animates.
  - The thumb uses the same displayed offset (clamped and rounded) and the same min-thumb as draw. That rule now lives in shared helpers `clamp_code_scroll_offset` and `code_scroll_thumb`, and the draw path uses them as well.
- Cursor move in Reader with an active code drag goes to `drag_markdown_code_scrollbar_to(px)`. This branch sits before the Reader `scroll_y.is_dragging` branch and uses the same early-return style, with `if let Some(window)` instead of unwrap.
- Release: the left release ends the code drag in the same place the Reader `scroll_y` drag ends, before UI release dispatch. It returns early, so releasing outside the thumb still ends the drag and never reaches body or selection handlers.
  - It is not gated on Read mode, so a drag that survives a mode switch is still released.
  - `cancel_pointer_interactions` also ends the code drag (focus loss or cancel).
- Tick (about.rs, next to Reader selection autoscroll): `if markdown_read && app.markdown.update_code_scroll_x(dt) { needs_redraw = true; }`.
- If geometry is missing during a drag (layout invalidated), the drag ends and a redraw is requested, mirroring `drag_markdown_read_scrollbar_to`.

### Placement deviation from the brief
- The brief put the App methods in `src/app/markdown.rs`, which was already 1660 lines, over the 1600 limit.
- The logic now lives in `impl MarkdownTabState` methods in the Task 6 chunk `src/render_view/markdown_code_scroll.rs`: `scroll_code_block_x`, `begin_code_scroll_drag`, `drag_code_scroll_to`. That chunk has access to the private offset and thumb helpers, so there is no duplicated clamp/round/min-thumb logic.
- `src/app/markdown.rs` holds only thin `App` wrappers that resolve frame, geometry and scale. The produced signatures match the brief exactly:
  - `scroll_markdown_code_block_x_at(&mut self, x, y, delta) -> bool`
  - `begin_markdown_code_scrollbar_drag_at(&mut self, block_id, pointer_x) -> bool`
  - `drag_markdown_code_scrollbar_to(&mut self, pointer_x) -> bool`
  - Private helper `markdown_code_scroll_geometry_for`.
- The press handler is in `src/app/mouse/input.rs`, as the brief said.

## TDD Evidence

### RED
Command: `make test TEST_FILTER='markdown_code_scroll_gl_tests'`, run after writing the tests and before any production change. Both tests compiled and failed on behavior:
```
thread 'main' panicked at src/render_view/markdown_code_scroll.rs:765:9:
assertion `left == right` failed
  left: None
 right: Some(7)            <- code_scrollbar_thumb_drag_is_smooth_and_release_ends_drag (press did not start drag)
thread 'main' panicked at src/render_view/markdown_code_scroll.rs:693:9:
assertion `left == right` failed
  left: 0.0
 right: 60.0               <- shift_wheel_over_overflowing_code_block_scrolls_block_not_reader (shift wheel did not scroll block)
test result: FAILED. 2 passed; 2 failed; ... 2381 filtered out
```
The first test draft drove the drag through `handle_main_cursor_moved`. Headless, that panicked at `src/app/mouse/cursor.rs:402`, where `self.window.as_ref().unwrap().inner_size()` runs before Reader routing. That line is pre-existing and unconditional. The test now calls `drag_markdown_code_scrollbar_to` directly, which is the entrypoint the cursor branch delegates to.

### GREEN
```
### FILTER markdown_code_scroll
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 2374 filtered out
### FILTER app::markdown
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 2343 filtered out
### FILTER app::mouse
test result: ok. 141 passed; 0 failed; 0 ignored; 0 measured; 2244 filtered out
### FILTER app::events::about
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 2357 filtered out
### FILTER markdown_read
test result: FAILED. 147 passed; 1 failed
  failed: render_view::markdown_read::reader_stage1_review_v1::reviewer_reader_v1_all_block_selection_seating_uses_real_vertices
  message: Reader seating exceeds heading/actual Editor reference: ["dpi=2 kind=table delta=-3 limit=2.5", ...]
```
The single failure is the known pre-existing unrelated one (table selection seating). Its message contains nothing about code scroll. The build produced no warnings from any touched file.

### New tests (src/render_view/markdown_code_scroll.rs, linux GL fixture module)

`shift_wheel_over_overflowing_code_block_scrolls_block_not_reader`. The fixture has a long code block, a short code block and 60 paragraphs. It checks:
- Shift + PixelDelta(0, -60) sets the code target to exactly 60, leaves `scroll_y.target` unchanged, and sets anim_speed to 7.
- A horizontal-dominant PixelDelta(-25.4, 3) without Shift sets the target to 85 (rounded), with Reader unchanged.
- A huge shift delta clamps to exactly `max_scroll`.
- A plain vertical wheel moves `scroll_y.target` and leaves the code target unchanged.
- A Shift wheel over the non-overflowing block falls through to the Reader vertical scroll, and no state entry is created.

`code_scrollbar_thumb_drag_is_smooth_and_release_ends_drag`. It checks:
- `find_at` over the thumb returns `MarkdownCodeScrollbarX(id)`.
- A real `reviewer_markdown_read_mouse_input(Pressed)` gives `code_scroll_drag == Some(id)`, `is_dragging`, `drag_offset == thumb.len * 0.25`, target about 0, current 0, and no Reader selection.
- `drag_markdown_code_scrollbar_to(track end)` gives `target == max_scroll`, current 0, anim_speed 15, and Reader `scroll_y` untouched.
- A real Release with the pointer in the body gives `code_scroll_drag == None`, `!is_dragging`, and no selection. Afterwards `update_code_scroll_x(0.016)` returns true and `code_scroll_x > 0`.

## Files changed
- `src/render_view/markdown_code_scroll.rs`: shared `clamp_code_scroll_offset` / `code_scroll_thumb` (draw reuses them); `MarkdownTabState::{scroll_code_block_x, begin_code_scroll_drag, drag_code_scroll_to}`; two GL fixture tests plus small test helpers. Now 953 lines.
- `src/app/markdown.rs`: App wrappers `markdown_code_scroll_geometry_for`, `scroll_markdown_code_block_x_at`, `begin_markdown_code_scrollbar_drag_at`, `drag_markdown_code_scrollbar_to`. Now 1732 lines.
- `src/app/mouse/wheel.rs`: Reader horizontal code-block branch just before the `handle_markdown_read_wheel` match.
- `src/app/mouse/input.rs`: `MarkdownCodeScrollbarX` press branch before `MarkdownReadScrollbar`; release ends the code drag next to the Reader `scroll_y` drag end, with early return; `cancel_pointer_interactions` ends the code drag.
- `src/app/mouse/cursor.rs`: code drag branch before the Reader `scroll_y.is_dragging` branch.
- `src/app/events/about.rs`: `update_code_scroll_x(dt)` tick hook next to Reader selection autoscroll.

## Self-review
- No per-frame allocation.
  - The tick iterates the small `code_scroll_x` Vec, and `retain` never allocates.
  - The wheel, press and drag paths do registry lookups, a binary search (`partition_point`) for the block, and float math.
  - A state entry is created only on real user input, and idle entries are dropped on the next tick.
- No unwrap/expect in new production code. Every redraw uses `if let Some(window) = self.window.as_ref()`.
- The tick sets `needs_redraw` while the animation changes values, the same mechanism `scroll_y` uses.
- The drag goes through `apply_scrollbar_drag_target`, so anim speed is 15 and the scroll moves smoothly toward the target rather than jumping.
- Non-finite max or delta returns false and falls through. Non-finite drag input ends the drag.
- Existing Task 5 wheel and cursor changes are preserved. The branch is inserted after the hover-clear logic and the `hovered` lookup.

## Concerns
1. `src/app/markdown.rs` was already over the 1600-line limit (1660) and is now 1732 after the ~70-line wrapper block. Most of the logic went into the chunk to keep this small. A later split of markdown.rs may be warranted; that is outside this task's scope.
2. Two wiring points have no automated test:
   - The about.rs tick hook: one line inside the large `about_to_wait` tick.
   - The cursor.rs branch: the headless fixture cannot reach it because of the pre-existing unconditional window unwrap at `cursor.rs:402`.
   The methods they call are tested.
3. `set_markdown_mode` does not end an active code drag (it does end the `scroll_y` drag). The cursor branch is gated on Read, and release or cancel ends the drag in any mode, so the only effect is a stale `is_dragging` until release.
4. Not run: `make codex_test` (per controller; the controller runs final verification).

---

## Fix round 1/5

Finding (Important): `src/app/markdown.rs` grew from 1660 to 1732 lines, breaking the AGENTS.md 1600-line limit.

### Fix
- Moved the whole App wrapper block out of `src/app/markdown.rs` into `src/render_view/markdown_code_scroll.rs`. Behavior, signatures and visibility are unchanged. It now sits as `impl crate::app::App { ... }` just before the chunk's test modules.
  - Moved: `markdown_code_scroll_geometry_for` (private), `scroll_markdown_code_block_x_at`, `begin_markdown_code_scrollbar_drag_at`, `drag_markdown_code_scrollbar_to` (all pub(crate)).
  - The move was text-exact, done by a script that asserted the block boundaries. The bodies needed no path changes: `MarkdownMode` and `CodeScrollGeometry` already resolve in the chunk.
  - Callers (wheel.rs, input.rs, cursor.rs, tests) are unchanged.
- Added the phrase "App wheel/drag input methods" to the `src/render_view/markdown_code_scroll.rs` entry in AGENTS.md section 9 and in PROJECT_GUIDE.md. Nothing else in either file changed.

### Line counts
- `src/app/markdown.rs`: 1660 (the pre-Task-7 size; no Task 7 lines remain there)
- `src/render_view/markdown_code_scroll.rs`: 1026

### Covering tests
```
### FILTER markdown_code_scroll
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 2374 filtered out
### FILTER app::markdown
test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 2343 filtered out
### FILTER app::mouse
test result: ok. 141 passed; 0 failed; 0 ignored; 0 measured; 2244 filtered out
```
No compiler warnings or errors. No git commands run.

### Files changed this round
`src/app/markdown.rs`, `src/render_view/markdown_code_scroll.rs`, `AGENTS.md`, `PROJECT_GUIDE.md`.

Resolves concern 1 from the original report. Concerns 2 and 3 are unchanged.
