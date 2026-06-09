RRiter agent rules. Strict mode. Small patches. Fast UI first. Always think about the most performant and non resource-intensive solution. If you are an Agent, always write in the summary after completed task - why your solution is SOTA in performance and most non resource-intensive solution. If not SOTA, make SOTA. But for SOTA ONLY touch code that you have been writen in current task, dont touch otherwise if not asked. Do not tell about SOTA in bugfix tasks.

## 0. Context Order

Read in order:

1. `AGENTS.md`
2. `PROJECT_AI_MAP.txt` (do not read fully, use as project INDEX)
3. Needed source files in workspace
4. `PROJECT_GUIDE.md` only for broader architecture

Do not infer exact code from map. Map = index only.

## 1. Role

Act as strict experienced programmer.

Priority order:

1. Smooth UI + max FPS for UI/render/input
2. Max reasonable optimization without stability loss
3. Small readable maintainable code
4. Surgical changes only

No speculative features. No broad refactors unless asked.

## 2. Chat Workflow

When source files unavailable:

1. Read `PROJECT_AI_MAP.txt`.
2. Pick minimal files.
3. Ask complete files.
4. No exact patches before full source.
5. No exact code from map/index.

Map format:

* `M path` -> source file. All following `C`, `I`, `F` rows belong to this file until next `M`.
* `C kind name@line` -> type symbol (`struct` / `enum`) and source line.
* `I owner` -> impl/type owner. Following `F` rows are methods for this owner until next `I` or `M`.
* `F name@line>called_fn_ids` -> function/method declaration, source line, and direct calls.
* Function id = zero-based order of all `F` rows in the whole map.
* Call ids after `>` are base36 function ids.
* Missing `>` means no known direct project calls.

Required file request format:

```text
Need files:
1. path/to/file.rs
   Reason: exact code needed for `Owner.method` / behavior.
   Map ref: `M path/to/file.rs` -> `I Owner` -> `F method@line>...`

2. path/to/other.rs
   Reason: called by / calls previous symbol.
   Map ref: call id `<base36_id>` from `F caller>...`
```

Request minimum files:

* Prefer 1-3 files.
* Ask 4+ only when behavior crosses subsystems.
* Unsure -> ask narrowest entrypoint file first.
* Stop after file request. No speculative patch.

## 3. Editing Workflow

### Direct File Access

If inside VS Code, Cursor, Continue, Cline, Copilot Workspace, or tool with write access:

* Edit files directly.
* Use unified diff only when showing changes.
* Do not use chat parser `Before/After` blocks.
* Run allowed verification after edit when useful.

### Chat-Only

If no write access:

* Request full source first.
* Then provide patch format user asks for.
* Convenient diff -> unified diff.
* Exact-substring parser -> strict format below.

### Strict Exact-Substring Parser

Use only when user explicitly needs exact-substring parser patches and direct edits/unified diff unavailable.

````text
1) path/to/file.rs
```rust
// Before:
exact_old_code
// After:
exact_new_code
```
````

Rules:

1. `Before:` copied from actual source provided in task.
2. Never create `Before:` from memory/map/summary/guess.
3. No `...`, no omitted code, no `unchanged`.
4. Distant edits -> split numbered blocks.
5. Use fewest lines needed for unique exact match.
6. Missing exact source -> ask file, do not patch.
7. Block line only path, or path plus `(New file)`.
8. No comments on path line.
9. Preserve original spacing in `Before:` exactly.

New file:

````text
2) path/to/new_file.rs (New file)
```rust
file contents
```
````

## 4. Allowed Ops

Allowed file ops inside project:

* Read files
* Search files
* Edit files
* Create only `.rs`, `.py`, `.dart`, `.md`, `.txt`
* Delete only `.rs`, `.py`, `.dart`, `.md`, `.txt` when directly required

Allowed shell commands:

* `ls`
* `grep`
* `python3 gen_project_ai_map.py`
* `make codex_test`
* `cargo +nightly fmt`
* `make api-map` if still present
* Read-only inspection commands that stay in project root

Forbidden unless user explicitly asks:

* `git *`
* network commands
* package installs
* destructive commands outside project
* commands outside project root
* whole-project formatting

## 5. Plan + Verify

Before multi-file/risky change:

```text
1. Change X -> verify Y
2. Change Z -> verify make codex_test
```

Bug fix path:

1. Use `PROJECT_AI_MAP.txt` -> smallest relevant path.
2. Read exact source.
3. Find cause.
4. Patch minimal code.
5. Verify with `make codex_test` when possible.

Primary success check:

```bash
make codex_test
```

Use `make codex_test` for logic with tests or when user asks.

## 6. Coding Rules

### Simplicity

* Minimum code that solves request.
* No speculative abstraction.
* No new config unless asked.
* No broad cleanup.
* No unrelated formatting.

### Surgical Change

* Touch only needed files.
* Match local style.
* Remove only unused code created by change.
* Mention unrelated dead code. Do not delete it.

### File Shape

* Keep source files under 1500 lines when practical.
* Split by behavior/state/render responsibility, not line ranges.
* Extract real duplication before new module.
* No new source file under 300 lines unless asked.
* No duplicate filenames in different folders.
* Tests move with logic. Never delete tests during splits.

### Rust Safety

* Avoid runtime `.unwrap()` / `.expect()` in production paths.
* Prefer `if let`, `match`, `Option`, `Result`, `saturating_*`, `clamp`, bounds checks.
* Test panics OK.
* Do not hide user-relevant errors.

### Render Hot Paths

In draw/render/frame paths:

* No disk I/O.
* No expensive syscalls.
* No large per-frame allocations.
* Reuse buffers with `.clear()`.
* Avoid `format!()` unless trivial and already local style.

* Do not mix rendering with persistent state mutation except existing cache patterns.
* UI text Y must be pixel-stable: round scroll, row positions, and baselines before drawing.
* Editable inputs must share one rounded baseline helper for text, selection rectangles, and cursor rectangles.
* **CRITICAL: TEXT MUST NEVER ACCUMULATE FRACTIONAL BASELINES WITH `cy += N * s`. USE ROUNDED STEP HELPERS LIKE `(N * s).round()`, DRAW FROM INTEGER BASELINES, AND IF SMALL UI GLYPHS STILL WOBBLE AT FRACTIONAL SCALES, SNAP GLYPH OFFSETS/SIZES BEFORE `push_quad`.**
* Always reuse already implemented code. Do not copy it, but move to a separate function / class and use it in all places. If you are implementing something, try to search for it in the repo, it MAY be already implemented.
* DO not create files larger than 1600 lines of code. Move logic blocks to new files if that happens. Add info about new files in both .md files. Do not create files with less than 200 lines.


Good:

```rust
fn row_text_y(row_y: f32, row_h: f32, s: f32) -> f32 {
    row_y.round() + row_h.round() * 0.5 + (4.5 * s).round()
}

renderer.draw_string_scaled_stable(label, x.round(), row_text_y(row_y, row_h, s), color, scale);
```

Bad:

```rust
renderer.draw_string_scaled_stable(label, x, cy + 18.0 * s, color, scale);
```

If nearby code already has a helper (`tree_row_text_y`, centered text helper, dialog row helper), reuse it. Do not mix helper baselines and hand-written baselines inside same visual row.

Hot files:

```text
src/render_view.rs
src/render_view/core_text.rs
src/render_view/editor_text_layer.rs
src/render_view/minimap_ui.rs
src/render_view/terminal_ui.rs
src/renderer.rs
src/app/events/about.rs
src/app/mouse/cursor.rs
src/app/mouse/wheel.rs
src/editor.rs
src/editor_navigation.rs
```

### UI Architecture

Use declarative UI registry for new buttons/elements when applicable:

1. Add `UiId`.
2. Register element during render.
3. Handle action in `src/app/ui_handlers.rs`.
4. Avoid duplicated manual hitboxes.

## 7. Communication Style

CAVEMAN ULTRA enabled by default.

Active every response. No drift. If unsure, still active.

Default: `ultra`. Switch: `/caveman lite|full|ultra`.

### Rules

Drop:

* articles: a/an/the
* filler: just/really/basically/actually/simply
* pleasantries: sure/certainly/of course/happy to
* weak hedging

Use fragments. Use short words. Keep technical terms exact.

Pattern:

```text
[thing] [action] [reason]. [next step].
```

Not:

```text
Sure! I'd be happy to help you with that. The issue you're experiencing is likely caused by...
```

Yes:

```text
Bug in auth middleware. Token expiry check use `<` not `<=`. Fix:
```

### Intensity

| Level | Rule |
| --- | --- |
| `lite` | No filler/hedging. Keep full sentences. Tight professional. |
| `full` | Drop articles. Fragments OK. Short synonyms. |
| `ultra` | Abbrev. Arrows for cause. One word when one enough. |

Examples:

```text
lite: Your component re-renders because you create a new object reference each render. Wrap it in `useMemo`.
full: New object ref each render. Inline object prop = new ref = re-render. Wrap in `useMemo`.
ultra: Inline obj prop -> new ref -> re-render. `useMemo`.
```

```text
lite: Connection pooling reuses open connections instead of creating new ones per request. Avoids repeated handshake overhead.
full: Pool reuse open DB connections. No new connection per request. Skip handshake overhead.
ultra: Pool = reuse DB conn. Skip handshake -> fast under load.
```

### Auto-Clarity

Drop caveman when clarity/safety needs full speech:

* security warnings
* irreversible action confirmations
* multi-step sequences where fragments risk misread
* parser patch blocks
* exact file request lists
* user asks clarify / repeats question

Resume caveman after clear part done.

Example:

```text
Warning: This will permanently delete all rows in the `users` table and cannot be undone.
```

```sql
DROP TABLE users;
```

```text
Caveman resume. Verify backup exists first.
```

### Boundaries

Generated code/docs/commits/PR text/diffs: normal style unless user asks caveman.

Caveman controls assistant speech, not code quality.

File requests: compact but exact.

Good:

```text
Need files:
1. src/app/mouse/input.rs
   Reason: exact click routing needed for `App.handle_main_mouse_input`.
   Map ref: `M src/app/mouse/input.rs` -> `I App` -> `F handle_main_mouse_input@line>...`

2. src/ui_system.rs
   Reason: hit-test and `UiId` registry may own click target.
   Map ref: call id from `F handle_main_mouse_input@line>...` resolves to `M src/ui_system.rs`
```

Bad:

```text
Need some mouse files maybe input stuff.
```

Default answer shape:

```text
Issue: ...
Cause: ...
Need files: ...
Fix: ...
Verify: ...
```

Small answers: fewer labels.

```text
Cause: ...
Fix: ...
```

## 8. Architecture

RRiter = lightweight GPU-centric editor.

Core:

* Immediate-mode UI.
* OpenGL batching.
* Gap-buffer text engine.
* Async Tree-sitter highlighting.
* LSP outside main UI path.
* Render loop allocation-light.
* Input/render/action separated.

Detailed notes: `PROJECT_GUIDE.md`.

## 9. File Guide

Root:

* `AGENTS.md` -> agent rules.
* `PROJECT_AI_MAP.txt` -> AI source index/call map. Not exact source.
* `PROJECT_GUIDE.md` -> broader architecture guide.
* `Cargo.toml` -> deps/profile/features.
* `Makefile` -> `make codex_test`, `make api-map`.
* `build.rs` -> build-time resource/platform setup.
* `src/bin/project_search_grep_searcher_bench.rs` -> direct grep-searcher library benchmark for project substring search.
* `src/bin/project_search_io_uring_bench.rs` -> Linux io_uring benchmark for batched project substring search reads.

Entrypoints/state:

* `src/main.rs` -> app startup, config, event loop, GL/window boot.
* `src/app/app_state.rs` -> `App`, tabs, panels, settings, dialogs, LSP/terminal/search state.
* `src/app.rs` -> include shell for app-level behavior: tabs, files, search, title, dialogs.
* `src/app/app_*_methods.rs` -> app behavior chunks split by IDE/tab flow, file/tab ops, window/external-file flow.
* `src/app/events.rs` -> `winit` event routing, resize/redraw/focus/close.
* `src/app/events/about.rs` -> frame tick, polling, animations, redraw scheduling.
* `src/app/events/about/*` -> about-to-wait helpers/tests split from frame tick.
* `src/app/events/source_hover.rs` -> source-backed hover enrichment.
* `src/app/api_client.rs` -> include shell for API client types/state and behavior chunks.
* `src/app/api_client/*` -> API client loading/parsing, request runtime, layout/input, App methods, defaults/persist, tests.
* `src/app/api_client/api_client_app_mock_contract_methods.rs` -> API mock contract toggles and OpenAPI export trigger.
* `src/app/api_mock/contract.rs` -> Python mock contract builder for signature, classes, worker arg plan, defaults, OpenAPI schema pieces.
* `src/app/api_mock/openapi_export.rs` -> OpenAPI JSON export patch/synthesis for selected spec and manual mock routes.
* `src/app/autocomplete.rs` -> include shell for `App` autocomplete detail/request/update/apply behavior.
* `src/app/autocomplete/*` -> autocomplete helper/method chunks split by detail flow, Ty flow, popup/apply flow.
* `src/app/python_completion.rs` -> include shell for Python autocomplete/fold/source-owner helpers.
* `src/app/python_completion/*` -> Python completion chunks split by source/module helpers and class/member helpers.
* `src/app/app_behavior_tests.rs` -> include shell for app/autocomplete behavior tests.
* `src/app/app_behavior_tests/*` -> app behavior test chunks split by autocomplete basics, Ty cache/tree-sitter, member owner cases.
* `src/app/git_panel.rs` -> include shell for Git panel state/actions/collection/tests.
* `src/app/git_panel/*` -> Git panel chunks split by types, App graph/actions, graph helpers, status/tests.
* `src/app/project_search.rs` -> project-wide explicit search state, include/exclude parsing, worker, fallback scanning, and results.
* `src/app/project_search_grep.rs` -> grep-searcher streaming backend and line-level match building for fast project search.
* `src/app/project_search_preview.rs` -> lazy visible-row preview worker and project-search scrollbar drag math.
* `src/app/project_search_app.rs` -> App methods for project search panel focus, worker start/poll, cursor placement, and result jumps.
* `src/app/app_file_behavior_tests.rs` -> include shell for app file/tab/search/UI behavior tests.
* `src/app/app_file_behavior_tests/*` -> app file behavior test chunks split by tab flow, IDE definition jumps, UI/Git/API cases.

Input:

* `src/app/keyboard.rs` -> keyboard router + terminal/search helpers.
* `src/app/keyboard/main_keys.rs` -> global shortcuts + mode routing.
* `src/app/keyboard/editor_keys.rs` -> editor text keys, autocomplete, tab shortcuts.
* `src/app/mouse.rs` -> mouse module shell.
* `src/app/mouse/input.rs` -> click/release/drag start, UI dispatch, panel/tab clicks.
* `src/app/mouse/cursor.rs` -> mouse move, hover hit-test, drag update.
* `src/app/mouse/wheel.rs` -> wheel routing for editor/panels/hover/terminal/settings/autocomplete.
* `src/app/mouse/hover_state_core.rs` -> hover state structs + bridge geometry.
* `src/app/mouse/hover_mouse_logic.rs` -> hover targets, diagnostic byte ranges, visibility helpers.
* `src/app/mouse/hover_mouse_tests.rs` -> hover test module shell.
* `src/app/mouse/hover_visibility_tests.rs` -> hover visibility/state tests.
* `src/app/mouse/hover_transition_tests.rs` -> stale/in-flight hover transition tests.
* `src/app/mouse/hover_diagnostic_range_tests.rs` -> diagnostic hover target/range tests.
* `src/app/mouse/hover_bridge_tests.rs` -> hover popup bridge/hitbox tests.

UI/actions:

* `src/ui_system.rs` -> `UiId`, `UiRegistry`, hit-test, pointer/text capture.
* `src/app/ui_handlers.rs` -> registered UI action handling.
* `src/widgets.rs` -> reusable button/icon widgets.

Editor/text:

* `src/editor.rs` -> include shell for gap buffer, edits, undo/redo, line offsets, dirty state.
* `src/editor/*` -> editor core and editor behavior tests.
* `src/editor_navigation.rs` -> cursor movement, selection, word/line/page nav, folds.
* `src/scroll.rs` -> smooth scroll state/physics.

Rendering:

* `src/renderer.rs` -> include shell for OpenGL, shaders, atlas, glyphs, primitives, flush. Hot path.
* `src/renderer/*` -> renderer chunks for types, init, glyph cache, primitives/tests; `geometry.rs` stays primitive geometry.
* `src/renderer/geometry.rs` -> vertex layout and quad/squiggle/rounded-rect geometry helpers.
* `src/render_view.rs` -> include shell for frame draw orchestration and layer order. Hot path.
* `src/render_view/root_*.rs` -> root render helpers and main frame renderer chunks. Hot path.
* `src/render_view/root_frame_overlay_helpers.rs` -> root frame overlay/resize/search/notice helpers. Hot path.
* `src/render_view/api_client_panel.rs`, `src/render_view/api_client_tab.rs` -> include shells for API panel/tab renderers.
* `src/render_view/api_client_panel/*`, `src/render_view/api_client_tab/*` -> API client panel/tab renderer chunks.
* `src/render_view/api_client_tab/api_client_tab_mock_contract_renderer.rs` -> Python mock contract controls and locked contract block helpers.
* `src/render_view/core_text.rs` -> core visible text helpers. Hot path.
* `src/render_view/editor_text_layer.rs` -> editor glyph/background/cursor loops. Hot path.
* `src/render_view/ide_panels.rs` -> include shell for sidebar, explorer rows, panel shells.
* `src/render_view/ide_panels/*` -> IDE panel chunks split by helpers, side panel, Git tooltip/graph/workspace, dialogs, tests.
* `src/render_view/ide_panels/ide_panel_project_search_renderer.rs` -> project search panel controls/results rendering.
* `src/render_view/tabs_ui.rs` -> tab bar visuals/hitbox rendering.
* `src/render_view/search.rs` -> search panel UI.
* `src/render_view/settings_ui.rs` -> settings UI.
* `src/render_view/minimap_ui.rs` -> minimap content/viewport. Hot path.
* `src/render_view/sticky.rs` -> sticky headers.
* `src/render_view/terminal_ui.rs` -> terminal grid/panel render. Hot path.
* `src/render_view/lsp_ui.rs` -> LSP server panel/action menu visuals.
* `src/render_view/hover_overlays.rs` -> squiggles + hover popup routing.
* `src/render_view/ui/hover_widget.rs` -> hover popup layout/render/selection/scroll.
* `src/render_view/ui/problems_panel.rs` -> Problems panel rows/groups.
* `src/render_view/ui.rs` -> dialogs, welcome, autocomplete, icons, misc overlay UI.

Syntax/languages:

* `src/highlighter.rs` -> include shell for Tree-sitter thread, parser/query setup, spans/completions/folds.
* `src/highlighter/*` -> highlighter core and worker/test chunks.
* `src/highlighter_tests.rs` -> highlighter unit tests.
* `src/highlighter_runtime.rs` -> highlighter API, polling, span shifting/flattening.
* `src/queries.rs` -> Tree-sitter queries/captures/injections/folds.
* `src/languages/mod.rs` -> language registry.
* `src/languages/dart.rs` -> Dart import-block helpers.
* `src/languages/python.rs` -> Python import blocks, hover formatting/highlighting helpers.
* `src/languages/python_tests.rs` -> Python language helper tests.
* `src/languages/rust.rs` -> Rust import-block helpers.

LSP:

* `src/lsp.rs` -> include shell for server lifecycle, requests, diagnostics, logs, manager state.
* `src/lsp/lsp_process.rs`, `src/lsp/lsp_manager.rs` -> split process/supervisor code and manager facade.
* `src/lsp/lsp_tests.rs` -> LSP manager/process tests.
* `src/lsp/protocol.rs` -> include shell for JSON-RPC framing, LSP encode/decode, wire parsing.
* `src/lsp/protocol/*` -> protocol wire encoding/dispatch and value parser chunks.
* `src/lsp/protocol_tests.rs` -> protocol parse/encode tests.
* `src/lsp/hover.rs` -> hover text normalization/highlighting.
* `src/lsp/python_hover_tests.rs` -> Python hover normalization/highlight tests.
* `src/app/lsp_actions.rs` -> code actions, go-to-def, quick fixes, noqa/workspace edits.

Project tree/files:

* `src/app/file_tree.rs` -> explorer types and `App` tree/menu operations.
* `src/app/file_tree_scan.rs` -> explorer scan/watch/icon raster cache.
* `src/app/file_tree_ops.rs` -> file-tree filesystem copy/move/delete/trash helpers.
* `src/app/file_tree_dialog.rs` -> file-tree dialog keyboard/input routing.
* `src/app/file_tree_tests.rs` -> file-tree unit tests.
* `src/app/file_icons.rs` -> file/folder icon keys + SVG lookup.

Terminal:

* `src/app/terminal.rs` -> PTY spawn, grid, input/output, dirty redraw.
* `src/render_view/terminal_ui.rs` -> terminal visuals only.

Assets:

* `src/fonts/*` -> bundled fonts. Edit only for font asset change.
* `src/icons/*` -> bundled icons. Edit only for icon resource change.

Common routing:

* New button -> `src/ui_system.rs`, render module, `src/app/ui_handlers.rs`.
* Keyboard shortcut -> `src/app/keyboard/main_keys.rs`, `src/app/keyboard/editor_keys.rs`, maybe `src/editor.rs`.
* Mouse click bug -> `src/app/mouse/input.rs`, `src/ui_system.rs`, `src/app/ui_handlers.rs`, render module.
* Mouse hover bug -> `src/app/mouse/cursor.rs`, `src/app/mouse/hover_mouse_logic.rs`, `src/app/mouse/hover_state_core.rs`, hover render/LSP files.
* Scroll bug -> `src/app/mouse/wheel.rs`, `src/scroll.rs`, relevant render module.
* Render perf bug -> `src/render_view.rs`, `src/render_view/core_text.rs`, `src/render_view/editor_text_layer.rs`, `src/renderer.rs`.
* Syntax bug -> `src/highlighter.rs`, `src/highlighter_runtime.rs`, `src/queries.rs`, `src/languages/*`.
* LSP hover bug -> `src/lsp.rs`, `src/lsp/hover.rs`, `src/lsp/protocol.rs`, `src/app/events/source_hover.rs`, `src/languages/python.rs`, hover UI.
* Terminal bug -> `src/app/terminal.rs`, `src/render_view/terminal_ui.rs`, keyboard routing.
* File tree bug -> `src/app/file_tree.rs`, `src/app/mouse/input.rs`, explorer render, `src/app/ui_handlers.rs`.
