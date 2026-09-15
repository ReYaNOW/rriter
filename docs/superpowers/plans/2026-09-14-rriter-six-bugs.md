# RRiter Six Bugs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix five user-reported RRiter bugs (terminal icon clipping under selection, invisible search panel in Markdown Reader, broken horizontal editor scroll, swallowed Reader wheel events, stuck Database "Подключение…") and add horizontal scrolling for overflowing Reader code blocks.

**Architecture:** Every bug fix is TDD against a verified root cause and is patched at the source, not the symptom. The feature reuses existing primitives: `ScrollState` physics, `crate::scroll::scrollbar_thumb` / `scrollbar_drag_target`, `apply_scrollbar_drag_target`, the `UiRegistry` ids, and the Reader layout cache. It adds no new scroll owner for vertical Reader scroll.

**Tech Stack:** Rust nightly, glow/OpenGL immediate-mode renderer, winit, Linux surfaceless EGL test fixture (`crate::render_view::reviewer_stage2_integration`).

**Spec:** User request (2026-09-14), recorded in this plan's "Requirements" section.

## Requirements (verbatim intent)

1. Terminal: icon glyphs must not be cut ("обрубки") when selection is active — do it like ronsole (`/home/reyan/projects/ronsole/src/renderer/terminal_ui.rs:1038-1044, 2522-2660`).
2. Markdown Reader: search panel must be visible.
3. Editor horizontal scroll must reach the end of lines with many inlay hints; typing past the right edge (even `.txt`) must stay scrollable.
4. Reader code blocks wider than the viewport: horizontal scrollbar, wheel/shift-wheel scroll, smooth scrolling while dragging the thumb.
5. Reader: first wheel after idle, and occasional wheel events during scrolling (especially over code blocks), must not be swallowed.
6. Database panel: expanded connection must not show "Подключение…" forever.

## Global Constraints

- Read `AGENTS.md` fully before touching code. Its rules override defaults.
- Do NOT run any `git` command (AGENTS.md §4). No commit steps.
- Focused test: `make test-one TEST='<module::path::test_name>'`. Final: `make codex_test`. NEVER run `make fast`.
- Render/draw hot paths: no per-frame heap allocation, no I/O, reuse buffers, round baselines/geometry (`.round()`), no `cy += N * s` accumulation.
- No runtime `.unwrap()`/`.expect()` in new production code.
- New source files: 200–1600 lines, and they must be listed in both `AGENTS.md` §9 and `PROJECT_GUIDE.md`.
- Match local style (Russian comments are fine). Surgical patches only; no unrelated refactors or formatting.
- Tasks run sequentially. It is one crate and one `target/` dir, so a broken compile in one task would break any parallel task.

---

### Task 1: Terminal background layer before glyph layer (bug 1)

**Root cause (verified):** `src/render_view/terminal_ui.rs:592-720` draws, per row and per cell, the background rect and then that cell's glyph immediately. Selection/search backgrounds of cell `c+1` (and of row `i+1`) are pushed later into the same vertex batch, so they paint over the part of a Nerd-font/emoji icon that overhangs its cell. ronsole draws all backgrounds of the visible rows first, then all glyphs (`TERMINAL_BODY_LAYERS`).

**Files:**
- Modify: `src/render_view/terminal_ui.rs` (row loop ~592-720; tests module ~1026)

**Interfaces:**
- Produces (file-private): `enum TerminalBodyLayer { Background, Glyph }`, `const TERMINAL_BODY_LAYERS: [TerminalBodyLayer; 2]`, `fn terminal_range_contains(row: usize, col: usize, bounds: (usize, usize, usize, usize)) -> bool`, `fn terminal_cell_background(cell_bg: u8, ansi: &[[f32; 4]], in_selection: bool, is_search_result: bool, is_active_search: bool, selection: [f32; 4]) -> Option<[f32; 4]>`

- [ ] **Step 1: Write failing tests** in `mod tests` of `terminal_ui.rs`:

```rust
#[test]
fn terminal_body_background_layer_precedes_overhanging_glyphs() {
    let icon = glyph(12.0, 16.0, 0.0, 12.0, 0.0);
    let (qx, _, qw, _) = crate::renderer::glyph_quad_rect(0.0, 16.0, icon, 1.0);
    let next_cell_left = 8.0;
    assert!(qx + qw > next_cell_left, "fixture icon must overhang into next cell");
    assert_eq!(
        TERMINAL_BODY_LAYERS,
        [TerminalBodyLayer::Background, TerminalBodyLayer::Glyph]
    );
}

#[test]
fn terminal_range_contains_matches_multiline_selection_semantics() {
    // selection from (col 3,row 1) to (col 2,row 3), given reversed on purpose
    let bounds = (2, 3, 3, 1);
    assert!(!terminal_range_contains(1, 2, bounds));
    assert!(terminal_range_contains(1, 3, bounds));
    assert!(terminal_range_contains(2, 0, bounds));
    assert!(terminal_range_contains(3, 2, bounds));
    assert!(!terminal_range_contains(3, 3, bounds));
    assert!(!terminal_range_contains(4, 0, bounds));
}

#[test]
fn terminal_cell_background_priority_is_active_search_selection_match_ansi() {
    let ansi = [[0.1, 0.1, 0.1, 1.0]; 16];
    let sel = [0.2, 0.3, 0.4, 1.0];
    assert_eq!(terminal_cell_background(0, &ansi, false, false, false, sel), None);
    assert_eq!(terminal_cell_background(3, &ansi, false, false, false, sel), Some(ansi[3]));
    assert_eq!(terminal_cell_background(3, &ansi, true, true, false, sel), Some(sel));
    assert_eq!(
        terminal_cell_background(3, &ansi, false, true, false, sel),
        Some([0.6, 0.6, 0.6, 0.35])
    );
    assert_eq!(
        terminal_cell_background(3, &ansi, true, true, true, sel),
        Some([1.0, 0.6, 0.0, 0.5])
    );
}
```

Before writing, verify `crate::renderer::glyph_quad_rect` signature and `crate::app::terminal::ANSI_16_COLORS` type (adapt the `ansi` parameter to `&[[f32; 4]; 16]` if needed) and `normalized_selection_bounds` argument order.

- [ ] **Step 2: Run, expect compile FAIL** (symbols missing)

Run: `make test-one TEST='render_view::terminal_ui::tests'`

- [ ] **Step 3: Implement.**
  - Add the enum/const (`#[derive(Clone, Copy, Debug, PartialEq, Eq)]`) and the two helpers near the other free fns at the top of the file.
  - `terminal_range_contains` does the same `normalized_selection_bounds` + row/col logic as the existing duplicated blocks (lines ~644-658 and ~664-677).
  - `terminal_cell_background` returns: active search, then selection, then search match, then ANSI bg when `cell_bg != 0 && cell_bg < 16`, else `None`.
  - Replace the single `for i in 0..rendered_lines` loop with `for layer in TERMINAL_BODY_LAYERS { for i in 0..rendered_lines { <same culling + flush guard> match layer { Background => {...}, Glyph => {...} } } }`:
    - Background: fill `row_search_results` (only here), then for each cell call the two helpers and `push_rect`.
    - Glyph: the existing `cell.c != ' '` glyph block unchanged.
  - No new allocations; `row_search_results` stays the reused buffer.

- [ ] **Step 4: Run tests, expect PASS**

Run: `make test-one TEST='render_view::terminal_ui::tests'`

---

### Task 2: Database connection hint stuck on "Подключение…" (bug 6)

**Root cause (verified by code reading; not reproduced interactively):**
- DB state machine is correct. The event loop just stops polling the DB worker while the window is suspended.
- `WindowEvent::Focused(false)` / `Occluded(true)` set `render_suspended = true` (`src/app/events.rs:441,451,488`).
- `about_to_wait` then returns at `src/app/events/about.rs:141-144` with `ControlFlow::Wait`, before `app.poll_database_runtime()` (`about.rs:438`).
- The worker only writes to an mpsc channel, and there is no `EventLoopProxy` to wake the loop. So `DatabasesLoaded` sits unread.
- The user's collapse click wakes the loop: the toggle collapses while `loading` is still true, then the poll applies the result, and re-expanding shows data.
- A truly stuck `loading == true` could not be fixed by collapse+expand (`toggle_connection_expansion` returns no-load while loading; test `toggling_connection_already_loading_never_requests_duplicate_load`).

**Files:**
- Modify: `src/app/events/about.rs:141-144`
- Modify/Test: `src/app/events/about/about_helpers.rs` (new pure helper next to `compute_about_wait_plan` ~359; tests in its test module ~1240)

**Interfaces:**
- Produces: `fn suspended_about_wait_plan(now: Instant, database_job_pending: bool) -> AboutWaitPlan`

- [ ] **Step 1: Write failing tests** (about_helpers test module):

```rust
#[test]
fn suspended_wait_plan_keeps_waking_only_while_database_job_is_pending() {
    let now = Instant::now();
    assert!(matches!(suspended_about_wait_plan(now, false), AboutWaitPlan::Wait));
    match suspended_about_wait_plan(now, true) {
        AboutWaitPlan::WaitUntil(at) => {
            assert_eq!(at, now + std::time::Duration::from_millis(100));
        }
        AboutWaitPlan::Wait => panic!("pending database job must keep polling while suspended"),
    }
}

#[test]
fn suspended_about_to_wait_still_polls_database_runtime() {
    let about = include_str!("../about.rs");
    let suspended = about
        .split("if app.render_suspended && !automation_running {")
        .nth(1)
        .expect("suspended branch")
        .split("return;")
        .next()
        .expect("suspended branch return");
    assert!(suspended.contains("app.poll_database_runtime();"));
    assert!(suspended.contains("suspended_about_wait_plan("));
}
```

  Verify the test module's imports (`Instant`, `AboutWaitPlan`) and the `include_str!` relative path pattern used by the existing test at ~1240 before writing.

- [ ] **Step 2: Run, expect FAIL**: `make test-one TEST='app::events::about'`
- [ ] **Step 3: Implement.** In `about_helpers.rs`:

```rust
fn suspended_about_wait_plan(now: Instant, database_job_pending: bool) -> AboutWaitPlan {
    if database_job_pending {
        AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(100))
    } else {
        AboutWaitPlan::Wait
    }
}
```

  In `about.rs` suspended branch:

```rust
if app.render_suspended && !automation_running {
    app.last_frame = now;
    app.poll_database_runtime();
    let database_job_pending = app.ide_panel.database.pending_job.is_some();
    event_loop.set_control_flow(match suspended_about_wait_plan(now, database_job_pending) {
        AboutWaitPlan::Wait => ControlFlow::Wait,
        AboutWaitPlan::WaitUntil(at) => ControlFlow::WaitUntil(at),
    });
    return;
}
```

  - Check `poll_database_runtime` (`database_app_methods.rs:354`) is a cheap no-op without a runtime.
  - Check that its `request_redraw` while suspended is harmless: `RedrawRequested` returns early at `events.rs:586`.
  - Do NOT change suspension/redraw semantics (tested at `about_helpers.rs:1246-1259`).
- [ ] **Step 4: Run, expect PASS**: `make test-one TEST='app::events::about'` and `make test-one TEST='app::database'`

---

### Task 3: Search panel in Markdown Reader (bug 2)

**Root cause (verified):** `Renderer::draw` in `src/render_view/root_frame_renderer.rs:784-876` handles `markdown_read_active` with an early `return` after reader, tabs, status and overlays. It never calls `draw_search_panel_if_visible` (the call exists only on the normal-editor path at ~1509). Search keys, results and highlights work, but the panel is never drawn or registered.

**Files:**
- Modify: `src/render_view/search.rs` (geometry helper; `draw_search_panel` parameter)
- Modify: `src/render_view/root_frame_overlay_helpers.rs:418-446` (`draw_search_panel_if_visible` parameter)
- Modify: `src/render_view/root_frame_renderer.rs` (Read branch ~850; normal branch ~1509)
- Modify: `src/render_view/markdown_read.rs:994` (`markdown_read_scrollbar_width` → `pub(crate)`)
- Modify: `src/app/events.rs` (~815 and ~1398 hit-testing geometry)

**Interfaces:**
- Produces: `pub(crate) fn search_panel_scrollbar_x(window_w: f32, minimap_w: f32, editor_scrollbar_w: f32, markdown_read_scrollbar_w: Option<f32>) -> f32` in `render_view::search`
- Changes: `draw_search_panel(..., blink_alpha: f32, scrollbar_x: f32, ui_registry)` and `draw_search_panel_if_visible(..., blink_alpha: f32, scrollbar_x: f32, ui_registry)`. The parameter was `scrollbar_width`; the function no longer subtracts `self.minimap_width` internally.

- [ ] **Step 1: Write failing tests** (add `#[cfg(test)] mod tests { use super::*; ... }` at the end of `search.rs` if missing):

```rust
#[test]
fn search_panel_scrollbar_x_uses_reader_scrollbar_edge_in_markdown_read() {
    assert_eq!(search_panel_scrollbar_x(1000.0, 119.0, 10.0, None), 871.0);
    assert_eq!(search_panel_scrollbar_x(1000.0, 119.0, 10.0, Some(9.0)), 991.0);
    assert_eq!(search_panel_scrollbar_x(1000.0, 119.0, 10.0, Some(0.0)), 1000.0);
}

#[test]
fn markdown_read_root_branch_draws_search_panel() {
    let source = include_str!("root_frame_renderer.rs");
    let start = source
        .find("if markdown_read_active {\n            if let Some(pre_editor_start)")
        .expect("markdown read root branch");
    let end = start
        + source[start..]
            .find("return (wants_pointer, Vec::new());")
            .expect("markdown read branch return");
    assert!(source[start..end].contains("draw_search_panel_if_visible("));
}
```

- [ ] **Step 2: Run, expect FAIL**: `make test-one TEST='render_view::search::tests'`

- [ ] **Step 3: Implement.**

```rust
pub(crate) fn search_panel_scrollbar_x(
    window_w: f32,
    minimap_w: f32,
    editor_scrollbar_w: f32,
    markdown_read_scrollbar_w: Option<f32>,
) -> f32 {
    match markdown_read_scrollbar_w {
        Some(read_w) => window_w - read_w.max(0.0),
        None => window_w - minimap_w - editor_scrollbar_w,
    }
}
```

  - `draw_search_panel`: take `scrollbar_x: f32` and delete `let scrollbar_x = self.width - self.minimap_width - scrollbar_width;`.
  - `draw_search_panel_if_visible`: forward `scrollbar_x`.
  - Normal branch (~1509): pass `search::search_panel_scrollbar_x(self.width, self.minimap_width, scrollbar_width, None)`.
  - Read branch: after `draw_root_fps_if_visible` and before `draw_root_bottom_status_and_dim`, mirroring the normal order:

```rust
let read_scrollbar_w = markdown_read::markdown_read_scrollbar_width(
    markdown.read_scroll_bounds().unwrap_or(0.0),
    s,
);
wants_pointer |= self.draw_search_panel_if_visible(
    show_search,
    search_anim_y,
    search_editor,
    search_focused,
    search_case_sensitive,
    search_results,
    search_current_idx,
    blink_alpha,
    crate::render_view::search::search_panel_scrollbar_x(
        self.width,
        self.minimap_width,
        0.0,
        Some(read_scrollbar_w),
    ),
    ui_registry,
);
```

  - `events.rs` (both sites): compute `let read_w = (self.markdown_mode() == crate::app::MarkdownMode::Read).then(|| crate::render_view::markdown_read::markdown_read_scrollbar_width(self.markdown.read_scroll_bounds().unwrap_or(0.0), s));`, then `let scrollbar_x = crate::render_view::search::search_panel_scrollbar_x(window_width, minimap_w, scrollbar_w, read_w);`. This keeps hit-testing identical to drawing.
  - Verify `markdown_read` / `search` module paths resolve inside `root_frame_renderer.rs` (it already references `markdown_read::` at ~793).

- [ ] **Step 4: Run, expect PASS**: `make test-one TEST='render_view::search::tests'`

---

### Task 4: Editor horizontal max scroll with inlays and typing (bug 3)

**Root causes (verified):**
- (a) Inlay hints not counted and never trigger a recompute.
  - `max_scroll_x` (`root_frame_renderer.rs:469-491`) uses `measure_width` of `editor.longest_line_idx` only, with no inlay widths.
  - Cursor x does include inlays (`core_text.rs:917`), so cursor-follow, wheel (`wheel.rs:1373-1379`) and the tick clamp (`about.rs:1002-1004`) get stuck.
  - The recompute runs only on `editor.version` change. Hints arrive later with the same version (`root_frame_renderer.rs:160-164`).
- (b) Longest line picked by bytes.
  - `Editor::rebuild_line_offsets` (`editor_core.rs:322-358`, called on every edit) compares byte length.
  - Tabs (1 byte, 4 columns) and Cyrillic (2 bytes, 1 column) pick the wrong line, and the bound can be 0. This is the `.txt` case.
- (c) Stale bound when typing.
  - `ensure_cursor_visible` (`app_file_tab_methods.rs:~379-416`) clamps to the previous frame's `renderer.max_scroll_x` right after an edit.
  - `about.rs:1002` clamps target/current again before the draw recomputes.
- (d) Resize check is dead.
  - `update_cache` sets `last_width = width` (`core_text.rs:859`) before the `(last_width - width).abs() > 0.5` check at `root_frame_renderer.rs:471`.
  - `view_w` also depends on `left_padding` and `minimap_width`, which change without a version bump (IDE left panel resize).

**Files:**
- Modify: `src/editor/editor_core.rs:322-358`
- Modify: `src/render_view/core_text.rs` (new `update_max_scroll_x` + `line_visual_width` near `current_inlay_width_before` ~933)
- Modify: `src/renderer/renderer_types.rs:~367-376`, `src/renderer/renderer_init_methods.rs:~585-594` (new fields)
- Modify: `src/render_view/root_frame_renderer.rs:160-164` (hint sync marks dirty), `:469-491` (replace block with call)
- Modify: `src/app/app_file_tab_methods.rs:~379-416` (`ensure_cursor_visible` refreshes bound before clamp)
- Test: `src/editor/editor_behavior_tests.rs`; linux fixture tests in `src/render_view/core_text.rs` under a new `#[cfg(all(test, target_os = "linux"))] mod max_scroll_x_tests` (pattern: `stage5_overlay_boundary_tests` in `editor_text_layer.rs:1055`)

**Interfaces:**
- Produces on `Renderer`:

```rust
pub(crate) fn update_max_scroll_x(&mut self, editor: &Editor);
pub(crate) fn sync_current_python_inlay_hints(&mut self, hints: &[crate::app::PythonInlayHint]);
// fields:
pub last_view_w_for_scroll_x: f32,        // init -1.0
pub scroll_x_bounds_inlays_dirty: bool,   // init true
```

- [ ] **Step 1: Failing editor test** (`editor_behavior_tests.rs`):

```rust
#[test]
fn longest_line_uses_display_columns_not_bytes() {
    let mut cyrillic = Editor::new(256);
    cyrillic.set_clean_text(&format!("{}\n{}\n", "ж".repeat(30), "x".repeat(40)));
    cyrillic.rebuild_line_offsets();
    assert_eq!(cyrillic.longest_line_idx, 1);

    let mut tabs = Editor::new(256);
    tabs.set_clean_text(&format!("{}\n{}\n", "y".repeat(40), "\t".repeat(12)));
    tabs.rebuild_line_offsets();
    assert_eq!(tabs.longest_line_idx, 1);
}
```

  Confirm the renderer tab width is 4 columns (`rg -n "'\\\\t'" src/render_view/core_text.rs src/renderer`). If it differs, use that constant.

- [ ] **Step 2: Failing linux fixture tests** (`core_text.rs`, new module; use `crate::render_view::reviewer_stage2_integration::fixture`). Check the `PythonInlayHint` fields (`rg -n "pub struct PythonInlayHint" -A8 src/app`) and `ensure_cursor_visible`'s exact name/visibility/signature first:

```rust
#[test]
fn max_scroll_x_counts_inlay_hints_without_version_change() {
    let (_ctx, mut app) = fixture(&format!("{}\n", "a".repeat(60)), 900.0, 1.0);
    let r = app.renderer.as_mut().unwrap();
    r.minimap_width = 0.0;
    r.left_padding = 0.0;
    r.width = r.measure_ui_width("", 1.0) + r.char_advance('a') * 60.0 + 10.0;
    r.update_max_scroll_x(&app.editor);
    assert_eq!(r.max_scroll_x, 0.0);
    r.sync_current_python_inlay_hints(&[/* hint at byte 10 with a long label like ": dict[str, list[int]]" */]);
    r.update_max_scroll_x(&app.editor);
    assert!(r.max_scroll_x > 0.0);
}

#[test]
fn max_scroll_x_recomputes_when_view_width_changes_without_version_change() {
    let (_ctx, mut app) = fixture(&format!("{}\n", "a".repeat(80)), 2000.0, 1.0);
    let r = app.renderer.as_mut().unwrap();
    r.minimap_width = 0.0;
    r.left_padding = 0.0;
    r.update_max_scroll_x(&app.editor);
    assert_eq!(r.max_scroll_x, 0.0);
    r.left_padding = 1990.0;
    r.update_max_scroll_x(&app.editor);
    assert!(r.max_scroll_x > 0.0);
}

#[test]
fn typing_past_right_edge_from_zero_bound_scrolls_horizontally() {
    // fixture with short line; update_cache + update_max_scroll_x -> 0
    // app.editor.cursor = end; app.editor.insert_str(&"w".repeat(300)) (version bump, cursor at end)
    // call ensure_cursor_visible(&mut ty, &mut tx, &app.editor, renderer, renderer.width, renderer.height, 0.0)
    // assert tx > 0.0
}
```

  Write the third test with real calls. Make each test fail on the current code, and fill the hint literal with real field values.

- [ ] **Step 3: Run, expect FAIL**: `make test-one TEST='editor::'`, `make test-one TEST='render_view::core_text'`
- [ ] **Step 4: Implement (b).** In `rebuild_line_offsets`'s byte loop, replace the byte length with a column counter. Per byte: `b'\t' => cols += 4`, `b & 0xC0 == 0x80 => {}`, `b'\n' => { compare cols; reset }`, `_ => cols += 1`. Handle the final unterminated line the same way. No new allocation or pass.
- [ ] **Step 5: Implement (a)(c)(d).**

```rust
fn line_visual_width(&mut self, editor: &Editor, line: usize) -> f32 {
    let Some(&start) = editor.line_offsets.get(line) else { return 0.0; };
    let end = editor.line_offsets.get(line + 1).copied().unwrap_or(editor.len());
    self.visual_x_for_byte_offset(editor, start, end, true)
}

pub(crate) fn sync_current_python_inlay_hints(&mut self, hints: &[crate::app::PythonInlayHint]) {
    if self.current_python_inlay_hints.as_slice() != hints {
        self.current_python_inlay_hints.clear();
        self.current_python_inlay_hints.extend_from_slice(hints);
        self.scroll_x_bounds_inlays_dirty = true;
    }
}

pub(crate) fn update_max_scroll_x(&mut self, editor: &Editor) {
    let view_w = self.width - self.minimap_width - self.left_padding;
    if self.last_editor_version_for_scroll_x == editor.version
        && (self.last_view_w_for_scroll_x - view_w).abs() <= 0.5
        && !self.scroll_x_bounds_inlays_dirty
    {
        return;
    }
    let longest = editor.longest_line_idx;
    let cursor_line = editor
        .line_offsets
        .partition_point(|&offset| offset <= editor.cursor)
        .saturating_sub(1);
    let mut content_w = self.line_visual_width(editor, longest);
    if cursor_line != longest {
        content_w = content_w.max(self.line_visual_width(editor, cursor_line));
    }
    let mut last_line = usize::MAX;
    for idx in 0..self.current_python_inlay_hints.len() {
        let offset = self.current_python_inlay_hints[idx].byte_offset;
        let line = editor.line_offsets.partition_point(|&o| o <= offset).saturating_sub(1);
        if line != last_line && line != longest && line != cursor_line {
            content_w = content_w.max(self.line_visual_width(editor, line));
        }
        last_line = line;
    }
    self.max_scroll_x = if content_w > view_w { content_w - view_w + 100.0 } else { 0.0 };
    self.last_editor_version_for_scroll_x = editor.version;
    self.last_view_w_for_scroll_x = view_w;
    self.scroll_x_bounds_inlays_dirty = false;
}
```

  - `measure_width` already skips `'\n'`, so measuring through the newline byte is fine.
  - Hints are sorted by `byte_offset` (existing `partition_point` users), so `last_line` dedupe is valid.
  - Cost: nothing per frame. On version or width change: longest line + cursor line only, because hints are empty while `python_inlay_hint_version != editor.version` (`events.rs:719-726`). On hint change: one pass over hinted lines.
  - `root_frame_renderer.rs:160-164` → `self.sync_current_python_inlay_hints(python_inlay_hints);`.
  - `:469-491` → `if !markdown_read_active { self.update_max_scroll_x(editor); }`, placed after `left_padding` / `minimap_width` are updated (same position).
  - `ensure_cursor_visible`: `renderer.update_max_scroll_x(editor);` immediately before `*target_scroll_x = target_scroll_x.clamp(0.0, renderer.max_scroll_x).round();`.
  - Add the fields plus their init values (`last_view_w_for_scroll_x: -1.0`, `scroll_x_bounds_inlays_dirty: true`). Keep existing `last_editor_version_for_scroll_x = u64::MAX` resets.
- [ ] **Step 6: Run, expect PASS**: `make test-one TEST='editor::'`, `make test-one TEST='render_view::core_text'`, `make test-one TEST='render_view::editor_text_layer'`

---

### Task 5: Reader wheel events swallowed (bug 5)

**Root cause (verified):**
1. In Reader mode, mouse move still runs editor hover logic. `cursor.rs:970` suppresses hover only for scrollbar drags, and the hover block at `cursor.rs:1035` maps the pointer through the hidden source editor layout (`get_byte_at_xy`, `cursor.rs:1110-1120`).
2. That arms `HOVER_STATE.byte_offset` on almost any word byte (`hover_mouse_logic.rs:197,260`). The Reader never draws a popup, and an empty LSP answer keeps `byte_offset` (`source_hover.rs:1463`).
3. The next wheel reaches `wheel.rs:~262`. `clear_hover_popup` returns true because `byte_offset.is_some()`, so the handler requests a redraw and `return`s before the Reader branch (`wheel.rs:1308`). One wheel notch is lost.

Any small mouse move while reading re-arms this: after idle, between notches, and more often over dense code-block text.

**Files:**
- Modify: `src/app/mouse/cursor.rs:171-176` (helper), `:970-977` (call site), tests `:~1893`
- Modify: `src/app/mouse/wheel.rs:~262`
- Test: `src/app/events/about/about_helpers.rs` linux test module (next to `markdown_reader_scrollbar_real_press_drag_and_release_use_shared_geometry` ~742)

**Interfaces:**
- Changes: `fn should_suppress_editor_hover_for_scroll_drag(scroll_y_dragging: bool, scroll_x_dragging: bool, markdown_read: bool) -> bool`

- [ ] **Step 1: Failing tests.** Extend the unit test in `cursor.rs`:

```rust
#[test]
fn editor_scrollbar_drag_suppresses_hover_only_while_dragging() {
    assert!(!should_suppress_editor_hover_for_scroll_drag(false, false, false));
    assert!(should_suppress_editor_hover_for_scroll_drag(true, false, false));
    assert!(should_suppress_editor_hover_for_scroll_drag(false, true, false));
    assert!(should_suppress_editor_hover_for_scroll_drag(true, true, false));
}

#[test]
fn markdown_reader_never_arms_hidden_editor_hover() {
    assert!(should_suppress_editor_hover_for_scroll_drag(false, false, true));
}
```

  Linux fixture test (about_helpers test module):

```rust
#[cfg(target_os = "linux")]
#[test]
fn markdown_reader_wheel_is_not_swallowed_by_hidden_editor_hover_state() {
    let source = (0..120)
        .map(|i| format!("paragraph {i:03} alpha beta gamma delta\n\n"))
        .collect::<String>();
    let (_context, mut app) =
        crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.25);
    app.set_markdown_mode(crate::app::MarkdownMode::Read);
    crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
    let body = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        .expect("reader body");
    {
        let renderer = app.renderer.as_mut().unwrap();
        renderer.last_mouse_x = body.0 + 40.0;
        renderer.last_mouse_y = body.1 + body.3 * 0.5;
    }
    crate::app::mouse::HOVER_STATE.with(|state| state.borrow_mut().byte_offset = Some(0));
    let before = app.scroll_y.target;
    app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, -36.0),
    ));
    crate::app::mouse::clear_hover_popup(None);
    assert!(app.scroll_y.target > before, "first reader wheel notch must scroll");
}
```

  - Verify the `HOVER_STATE` / `clear_hover_popup` import paths.
  - Verify the `wheel_delta` sign: negative pixel y must give positive `dy` (see `wheel_delta_handles_line_and_pixel_units`); flip if not.
  - Today the fixture test fails: it panics on `self.window.as_ref().unwrap()` inside the swallow branch (window is `None`). The panic is itself proof the swallow branch is taken.

- [ ] **Step 2: Run, expect FAIL**: `make test-one TEST='app::mouse::cursor'`, `make test-one TEST='app::events::about'`
- [ ] **Step 3: Implement.**
  - Helper returns `markdown_read || scroll_y_dragging || scroll_x_dragging`.
  - Call site passes `self.markdown_mode() == crate::app::MarkdownMode::Read`. The existing branch then clears hover and ctrl-definition state and skips the hover block.
  - wheel.rs: replace the `if clear_hover_popup(self.renderer.as_mut()) { ... return; }` block with the code below. This covers hover armed in Edit mode just before toggling to Reader. Edit-mode behavior is unchanged.

```rust
let hover_cleared = clear_hover_popup(self.renderer.as_mut());
if hover_cleared && self.markdown_mode() != crate::app::MarkdownMode::Read {
    self.window.as_ref().unwrap().request_redraw();
    return;
}
```

- [ ] **Step 4: Run, expect PASS**: `make test-one TEST='app::mouse'`, `make test-one TEST='app::events::about'`, `make test-one TEST='app::markdown'`

---

### Task 6: Reader code block horizontal overflow — layout, state, render, hit-testing (feature, bug 4 part A)

**Design:** Each code block gets `content_width` measured once at layout time with the same mono advance used by hit-testing (`mono_char_pixel_advance(ch, 1.0, ...)`). Overflowing blocks reserve a thin scrollbar strip at the bottom. Per-block horizontal `ScrollState`s live in `MarkdownTabState` keyed by block id (`block.source_range.start`, the same id `MarkdownCodeCopy` uses) and are kept only while non-zero or dragging. Drawing offsets code text/highlights by `-scroll_x` under a nested scissor limited to the code viewport. The hit-test adds `scroll_x`.

**Files:**
- Create: `src/render_view/markdown_code_scroll.rs`: `include!` chunk of `markdown_read.rs`, holding geometry and draw helpers plus tests, 200-600 lines. Follow how `markdown_scroll.rs` is included (`rg -n 'include!' src/render_view/markdown_read.rs`).
- Modify: `src/render_view/markdown_read.rs` (`CodeBlock` ~227, `append_code` ~660-741, `draw_markdown_read` block loop ~1440-1476, `draw_markdown_block` Code arm ~1553-1596 + signature)
- Modify: `src/render_view/markdown_read_interaction.rs` (~580 code hit-test local_x)
- Modify: `src/app/markdown.rs` (state fields + methods + `handle_markdown_read_wheel` read_surface ids)
- Modify: `src/ui_system.rs:430` (new `UiId::MarkdownCodeScrollbarX(usize)`)
- Modify: `src/app/ui_handlers.rs:1890` (add id to the no-op release arm)
- Docs: `AGENTS.md` §9 and `PROJECT_GUIDE.md` (new file entry)

**Interfaces:**
- Produces in `render_view::markdown_read` (chunk):

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CodeScrollGeometry {
    pub view_x: f32,
    pub view_w: f32,
    pub content_w: f32,
    pub max_scroll: f32,
    pub track_x: f32,
    pub track_y: f32,
    pub track_w: f32,
    pub track_h: f32,
}
fn code_hscrollbar_reserve(scale: f32) -> f32; // (10.0 * scale).round()
fn code_hscrollbar_track_h(scale: f32) -> f32; // (4.0 * scale).round().max(2.0)
fn code_scroll_geometry(frame_x: f32, content_w: f32, code: &CodeBlock, block_bottom_screen: f32, scale: f32) -> CodeScrollGeometry;
impl MarkdownReadLayoutCache {
    pub(crate) fn code_block_max_scroll_x(&self, block_id: usize, frame_w: f32, scale: f32) -> Option<f32>;
}
impl Renderer {
    pub(crate) fn markdown_code_scroll_geometry(&self, markdown: &MarkdownTabState, editor_version: u64, frame: (f32, f32, f32, f32), scroll_y: f32, block_id: usize) -> Option<CodeScrollGeometry>;
}
```

- Produces in `app::markdown`:

```rust
#[derive(Clone, Debug)]
pub(crate) struct MarkdownCodeScrollX { pub block_id: usize, pub scroll: crate::scroll::ScrollState }
// MarkdownTabState fields:
pub(crate) code_scroll_x: Vec<MarkdownCodeScrollX>,
pub(crate) code_scroll_drag: Option<usize>,
// MarkdownTabState methods:
pub(crate) fn code_scroll_x(&self, block_id: usize) -> f32;               // current, 0.0 if absent
pub(crate) fn code_scroll_state_mut(&mut self, block_id: usize) -> &mut crate::scroll::ScrollState; // inserts ScrollState::new(7.0)
pub(crate) fn update_code_scroll_x(&mut self, dt: f32) -> bool;          // updates all, retains active entries
pub(crate) fn end_code_scroll_drag(&mut self) -> bool;
```

- Adds `CodeBlock.content_width: f32`.

- [ ] **Step 1: Read patterns first.**
  - `src/render_view/markdown_scroll_review_tests.rs` head: how an out-of-hot-file test module reaches the private `layout_with_advance` helpers and `CodeBlock`.
  - `code_block_padding` value.
  - `ScrollState::new` signature.
  - `draw_spanned_editor_line_pixel_snapped_alpha` `max_x` semantics (`src/render_view/core_text.rs:597`).
  - The Reader cursor/UiId mapping test at `markdown_read.rs:2097`.

- [ ] **Step 2: Write failing tests** in the new chunk's `#[cfg(test)]` module (adapt helper access to the pattern found in Step 1):

```rust
#[test]
fn code_block_content_width_uses_longest_line_mono_advance() {
    let cache = layout_with_advance("```\nab\nabcdefghij\n```\n", 900.0, 10.0);
    let code = first_code_block(&cache);
    assert_eq!(code.content_width, 100.0);
}

#[test]
fn only_overflowing_code_block_reserves_hscrollbar_and_has_max_scroll() {
    let long = format!("```\n{}\n```\n", "x".repeat(200));
    let short = "```\nx\n```\n";
    let wide = layout_with_advance(short, 900.0, 10.0);
    let narrow = layout_with_advance(&long, 900.0, 10.0);
    let (wb, wc) = first_code(&wide);
    let (nb, nc) = first_code(&narrow);
    let reserve = code_hscrollbar_reserve(1.0);
    assert_eq!((nb.bottom - nb.top) - (wb.bottom - wb.top), reserve);
    assert_eq!(code_scroll_geometry(0.0, 900.0, wc, wb.bottom, 1.0).max_scroll, 0.0);
    let g = code_scroll_geometry(0.0, 900.0, nc, nb.bottom, 1.0);
    assert_eq!(g.max_scroll, (nc.content_width - g.view_w).max(0.0));
    assert!(g.track_y + g.track_h <= nb.bottom);
    assert!(g.track_y >= nc.lines.last().unwrap().bottom);
}

#[test]
fn code_scroll_state_keeps_only_active_blocks() {
    let mut md = crate::app::MarkdownTabState::default();
    assert_eq!(md.code_scroll_x(7), 0.0);
    md.code_scroll_state_mut(7).jump_to(40.0);
    md.code_scroll_state_mut(9); // untouched
    assert!(!md.update_code_scroll_x(0.016));
    assert_eq!(md.code_scroll_x(7), 40.0);
    assert!(md.code_scroll_x.iter().all(|e| e.block_id != 9));
}
```

  The example uses a one-line source for the "long" case (200 chars × 10 = 2000 px, wider than the 900 px viewport). Use the file's existing test helper names (`layout_with_advance(source, width, advance)`), and write `first_code_block` / `first_code` as local test helpers returning `&CodeBlock` / `(&ReadBlock, &CodeBlock)`.

  Linux GL fixture test (put it in the same chunk under `#[cfg(all(test, target_os = "linux"))]`, using `crate::render_view::reviewer_stage2_integration::{fixture, read_frame}`):

```rust
#[test]
fn scrolled_code_block_hit_test_and_scrollbar_registry_follow_scroll_x() {
    let source = format!("intro\n\n```\n{}\n```\n", "abcdefghij".repeat(40));
    let (_ctx, mut app) = fixture(&source, 600.0, 1.0);
    app.set_markdown_mode(crate::app::MarkdownMode::Read);
    read_frame(&mut app);
    let block_id = app.markdown.read_layout.blocks.iter()
        .find(|b| matches!(b.kind, ReadBlockKind::Code(_))).unwrap().source_range.start;
    assert!(app.ui_registry.rect_for(crate::ui_system::UiId::MarkdownCodeScrollbarX(block_id)).is_some());
    // map the same screen point before/after scrolling by N glyph advances
    // assert source byte grows by N chars (use the Reader byte-at-point fn found near markdown_read_interaction.rs:560)
}
```

  Fill the last two comment lines with real calls after locating the byte-at-point function (the `fn` containing the `ReadBlockKind::Code(code) => { let line_idx = line_box_index(...)` arm at ~580). Set `app.markdown.code_scroll_state_mut(block_id).jump_to(adv * 5.0)`, where `adv = renderer.char_advance('a').round()` (verify against `mono_char_pixel_advance`), re-run `read_frame`, and assert `after == before + 5`.

- [ ] **Step 3: Run, expect FAIL**: `make test-one TEST='render_view::markdown_read'`

- [ ] **Step 4: Implement layout.** In `append_code`:
  - For each pushed `CodeLine`, accumulate `line_w = visible.chars().map(|ch| mono_char_pixel_advance(ch, 1.0, || (self.advance)(ch, true, None))).sum::<f32>()` and track the max into `content_width`.
  - Compute `view_w = ((self.width - CONTENT_PAD * self.scale).round() - x - 2.0 * pad).max(0.0)`. It must match the draw geometry `right - left - 2*pad` with `left = frame_x + code.x`, `right = frame_x + content_w - CONTENT_PAD*scale`.
  - `bottom = (y + pad + if content_width > view_w { code_hscrollbar_reserve(self.scale) } else { 0.0 }).round()`.

- [ ] **Step 5: Implement geometry + state.**

```rust
fn code_scroll_geometry(frame_x: f32, content_w: f32, code: &CodeBlock, block_bottom_screen: f32, scale: f32) -> CodeScrollGeometry {
    let pad = code_block_padding(scale);
    let left = frame_x + code.x;
    let right = frame_x + content_w - CONTENT_PAD * scale;
    let view_x = (left + pad).round();
    let view_w = (right - pad - view_x).max(0.0).round();
    let track_h = code_hscrollbar_track_h(scale);
    let track_y = (block_bottom_screen - pad * 0.5 - track_h).round();
    CodeScrollGeometry {
        view_x,
        view_w,
        content_w: code.content_width,
        max_scroll: (code.content_width - view_w).max(0.0).round(),
        track_x: view_x,
        track_y,
        track_w: view_w,
        track_h,
    }
}
```

  - `update_code_scroll_x`: `let mut changed = false; for e in &mut self.code_scroll_x { changed |= e.scroll.update(dt); } self.code_scroll_x.retain(|e| e.scroll.is_dragging || e.scroll.current != 0.0 || e.scroll.target != 0.0); changed`
  - Add fields to `MarkdownTabState` + `Default`.

- [ ] **Step 6: Implement render.**
  - `draw_markdown_block` gains `code_scroll_x: f32, reader_clip: (f32, f32, f32, f32)`. Callers pass `markdown.code_scroll_x(block.source_range.start)` and `(x, y, w, h)`; the non-code arms ignore them.
  - Code arm: `let g = code_scroll_geometry(frame_x, content_w, code, block.bottom + offset_y, self.scale_factor);`.
  - If `g.max_scroll > 0.0`:
    - `let sx = code_scroll_x.clamp(0.0, g.max_scroll).round();`
    - `self.flush()`; set the scissor to the intersection of `reader_clip` with `[g.view_x, g.view_x + g.view_w]` (GL y flipped as in `draw_markdown_read`).
    - Draw highlights/lines at `left + pad - sx` with `max_x = right - pad`.
    - `self.flush()`; restore the scissor to `reader_clip`.
    - Draw the thumb from `crate::scroll::scrollbar_thumb(g.track_x, g.track_w, g.view_w, g.content_w, sx, (24.0 * self.scale_factor).round())` as `push_rounded_rect(thumb.start.round(), g.track_y, thumb.len.round(), g.track_h, g.track_h * 0.5, faded(self.theme.fg, 0.32))`.
  - Otherwise keep the existing path unchanged (no extra flush).
  - In the `draw_markdown_read` loop, after `draw_markdown_block`: for code blocks with `max_scroll > 0` register `UiId::MarkdownCodeScrollbarX(block_id)` via `ui_registry.register_rect(id, g.track_x, g.track_y - hit_pad, g.track_w, g.track_h + 2.0 * hit_pad, self.last_mouse_x, self.last_mouse_y)` with `hit_pad = (3.0 * self.scale_factor).round()`.
- [ ] **Step 7: Hit-test.** In the Code arm at `markdown_read_interaction.rs:~580`: `let local_x = mouse_x - (frame_x + code.x + pad) + markdown.code_scroll_x(block.source_range.start).round();`.
- [ ] **Step 8: Ids.**
  - Add `MarkdownCodeScrollbarX(usize)` after `MarkdownCodeCopy(usize)`.
  - Add it to `handle_markdown_read_wheel`'s `read_surface` match and to the `ui_handlers.rs:1890` arm.
  - Update the Reader cursor-kind mapping (test at `markdown_read.rs:2097`) so the thumb shows the arrow cursor, not the I-beam.
- [ ] **Step 9: Run, expect PASS**: `make test-one TEST='render_view::markdown_read'` and `make test-one TEST='app::markdown'`

---

### Task 7: Reader code block horizontal input — wheel, thumb drag, animation tick (feature, bug 4 part B)

**Files:**
- Modify: `src/app/markdown.rs` (App methods next to `begin_markdown_read_scrollbar_drag_at` ~604)
- Modify: `src/app/mouse/wheel.rs` (just before `handle_markdown_read_wheel` ~1308)
- Modify: `src/app/mouse/input.rs` (press next to `MarkdownReadScrollbar` ~1256; the release path where Reader drags end)
- Modify: `src/app/mouse/cursor.rs` (~743, next to `drag_markdown_read_scrollbar_to`)
- Modify: `src/app/events/about.rs` (~359, next to Reader selection autoscroll)
- Test: `src/render_view/markdown_code_scroll.rs` (linux fixture tests)

**Interfaces:**
- Consumes (Task 6): `CodeScrollGeometry`, `Renderer::markdown_code_scroll_geometry`, `MarkdownTabState::{code_scroll_state_mut, code_scroll_x, update_code_scroll_x, end_code_scroll_drag, code_scroll_drag}`, `UiId::MarkdownCodeScrollbarX`
- Produces on `App`:

```rust
pub(crate) fn scroll_markdown_code_block_x_at(&mut self, x: f32, y: f32, delta: f32) -> bool;
pub(crate) fn begin_markdown_code_scrollbar_drag_at(&mut self, block_id: usize, pointer_x: f32) -> bool;
pub(crate) fn drag_markdown_code_scrollbar_to(&mut self, pointer_x: f32) -> bool;
```

- [ ] **Step 1: Write failing linux fixture tests:**

```rust
#[test]
fn shift_wheel_over_overflowing_code_block_scrolls_block_not_reader() {
    // fixture with a tall doc + one long code block visible; read_frame
    // put mouse over code block line; record scroll_y.target
    // app.modifiers = shift; app.handle_main_mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -60.0)))
    // assert code_scroll_x target > 0 and scroll_y.target unchanged
    // without shift: same wheel changes scroll_y.target, code target unchanged
}

#[test]
fn code_scrollbar_thumb_drag_is_smooth_and_release_ends_drag() {
    // read_frame; rect_for(MarkdownCodeScrollbarX(id)); mouse on thumb start
    // reviewer_markdown_read_mouse_input(Pressed, Left) -> code_scroll_drag == Some(id), state.is_dragging
    // drag_markdown_code_scrollbar_to(track_x + track_w) -> target > current, anim_speed == 15.0 (SCROLLBAR_DRAG_ANIM_SPEED), current unchanged
    // reviewer_markdown_read_mouse_input(Released, Left) with mouse elsewhere -> code_scroll_drag None, !is_dragging
    // app.markdown.update_code_scroll_x(0.016) returns true (animates)
}
```

  Write real assertions. Before writing, check the existing wheel tests: how the test build supplies `window_h` / modifiers (`src/app/mouse/wheel.rs` `#[cfg(test)]`) and how `MouseScrollDelta::PixelDelta` sign maps through `wheel_delta`. The expected behavior is the same sign convention as the editor's `scroll_x.scroll_by(dx)` / shift → `dy`.

- [ ] **Step 2: Run, expect FAIL**: `make test-one TEST='render_view::markdown_read'`
- [ ] **Step 3: Implement App methods.**
  - `scroll_markdown_code_block_x_at`:
    - Return false unless Read mode.
    - Get `frame = ui_registry.rect_for(MarkdownReadBody)?` and `block_id = renderer.markdown_read_code_block_at(&self.markdown, self.editor.version, frame, self.scroll_y.current, x, y)?`.
    - Get `g = renderer.markdown_code_scroll_geometry(...)?`; return false when `g.max_scroll <= 0.0`.
    - Otherwise `let s = self.markdown.code_scroll_state_mut(block_id); s.anim_speed = 7.0; s.scroll_by(delta); s.clamp_target(0.0, g.max_scroll); s.target = s.target.round(); true`.
  - `begin_markdown_code_scrollbar_drag_at` / `drag_markdown_code_scrollbar_to` mirror `begin_markdown_read_scrollbar_drag_at` / `drag_markdown_read_scrollbar_to` on the x axis:
    - Thumb comes from `crate::scroll::scrollbar_thumb(g.track_x, g.track_w, g.view_w, g.content_w, state.current.round(), (24.0*s).round())`.
    - Target comes from `crate::scroll::scrollbar_drag_target(pointer_x, g.track_x, g.track_w, thumb, g.max_scroll, offset)`.
    - Apply with `crate::app::mouse::apply_scrollbar_drag_target(state, target, offset)`.
    - Set `self.markdown.code_scroll_drag = Some(block_id)` on begin.
    - On missing geometry: `end_code_scroll_drag()`.
  - `end_code_scroll_drag`: if `Some(id)`, `end_drag()` that state; set `None`; return whether anything changed.
- [ ] **Step 4: Wire input.**
  - wheel.rs before the `handle_markdown_read_wheel` match:

```rust
if self.markdown_mode() == crate::app::MarkdownMode::Read && (shift || dx.abs() > dy.abs()) {
    let delta = if shift { dy } else { dx };
    if self.scroll_markdown_code_block_x_at(mx, my, delta) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        return;
    }
}
```

  - input.rs press: `if let crate::ui_system::UiId::MarkdownCodeScrollbarX(block_id) = clicked_id && button == winit::event::MouseButton::Left { if state == ElementState::Pressed { self.focus_document_text_surface(); let _ = self.begin_markdown_code_scrollbar_drag_at(block_id, mx); } request_redraw; return; }`.
  - input.rs release: in the same place that ends the Reader `scroll_y` drag before UI release dispatch (see test comment at `about_helpers.rs:~830`), call `self.markdown.end_code_scroll_drag()`.
  - cursor.rs: before the `scroll_y.is_dragging` Reader branch, `if self.markdown_mode() == Read && self.markdown.code_scroll_drag.is_some() { let _ = self.drag_markdown_code_scrollbar_to(px); request_redraw; return; }`. Match the exact early-return style of the neighboring Reader scrollbar branch.
  - about.rs: `if markdown_read && app.markdown.update_code_scroll_x(dt) { needs_redraw = true; }`.
- [ ] **Step 5: Run, expect PASS**: `make test-one TEST='render_view::markdown_read'`, `make test-one TEST='app::markdown'`, `make test-one TEST='app::mouse::wheel'`

---

### Task 8: Docs + full verification

- [ ] **Step 1:** Add `src/render_view/markdown_code_scroll.rs` entries to `AGENTS.md` §9 (Rendering list, after `markdown_read_interaction.rs`) and to `PROJECT_GUIDE.md` (matching section). Wording: "`markdown_read.rs` include chunk for Reader code-block horizontal overflow: layout-width geometry, per-block scroll state projection, nested scissor draw, thumb geometry/registry, and focused regressions. Hot path."
- [ ] **Step 2:** Run `make codex_test`. Expected: all tests pass, then the fast build succeeds. If anything fails, fix it at the root cause and re-run.
- [ ] **Step 3:** Final whole-change code review (superpowers:requesting-code-review) against the Requirements section.
