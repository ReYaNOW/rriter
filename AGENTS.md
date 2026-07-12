RRiter agent rules. Strict mode. Small patches. Fast UI first. Always think about the most performant and non resource-intensive solution. If you are an Agent, always write in the summary after completed task - why your solution is SOTA in performance and most non resource-intensive solution. If not SOTA, make SOTA. But for SOTA ONLY touch code that you have been writen in current task, dont touch otherwise if not asked. Do not tell about SOTA in bugfix tasks.

## 0. Context Order

Read in order:

1. `AGENTS.md`
2. Use `code-review-graph` MCP as primary project index/call graph
3. Needed source files in workspace
4. `PROJECT_AI_MAP.txt` only as fallback if MCP unavailable
5. `PROJECT_GUIDE.md` only for broader architecture

Do not infer exact code from graph/map. Graph/map = index only.
Always read exact source files before exact patches.

## 0.1 code-review-graph MCP

This project has `code-review-graph` MCP. Use it before Grep/Glob/Read for non-trivial code exploration.

Purpose:

* Avoid broad repo scans.
* Find exact symbols/files fast.
* Check callers/callees before editing.
* Check affected flows and review risk before risky changes.
* Read only source files that matter.

Use `PROJECT_AI_MAP.txt` only when MCP is unavailable or broken.

Do not use graph/map as source code.
Do not create exact patches from graph/map only.
Always read exact source before editing.

### 0.1.1 Required MCP-first workflow

For non-trivial edit/review/debug tasks:

1. Start with `code-review-graph`.
2. Find relevant node by exact file path or exact symbol name.
3. Use graph to inspect callers, callees, impact radius, affected flows, and tests.
4. Pick minimum exact source files.
5. Read exact source files.
6. Patch surgically.
7. If ANY file related to RRiter changed, run `make codex_test`.
8. Summarize graph findings, files changed, verify result.

Do not start with broad Grep/Glob/Read unless MCP unavailable.

### 0.1.2 Tool usage rules

Use these tools by intent:

* `get_minimal_context_tool`

  * First-pass context for task, changed files, or target area.
  * Good for summary, risk, key entities, communities, flow hints.
  * Treat output as triage, not exact code.

* `semantic_search_nodes_tool`

  * Find files/symbols by exact names.
  * Prefer precise queries:

    * `handle_main_mouse_input`
    * `Renderer.draw_editor_visible_text`
    * `src/render_view/editor_text_layer.rs`
    * `UiRegistry`
    * `start_active_api_request`
  * Avoid broad prose queries like `render frame draw hot path ui registry mouse input project search api client`; they may return 0.
  * If broad query returns 0, retry with exact symbol/file names from AGENTS.md, PROJECT_GUIDE.md, or user task.

* `query_graph_tool`

  * Use for structural facts.
  * Preferred patterns:

    * `file_summary` for file contents overview and line ranges.
    * `callers_of` before changing public/shared/hot functions.
    * `callees_of` before changing a function with many dependencies.
    * `tests_for` before/after patch planning.
    * `children_of` only when target format is known to work; if 0, use `file_summary`.
  * Prefer fully qualified targets when available:

    * `/abs/path/src/file.rs::FunctionName`
    * `/abs/path/src/file.rs::Type.method`
  * If relative target returns 0, retry with absolute path from graph search result.

* `get_affected_flows_tool`

  * Use when changing one or more files.
  * Use especially for render/input/API/LSP/editor hot paths.
  * Report affected flows only if useful to task.
  * Do not over-expand source reads just because graph shows many impacted nodes; choose files by risk and direct relevance.

* `get_review_context_tool`

  * Use for review tasks, current changes, risky edits, and multi-file changes.
  * Use to estimate impacted files/nodes, risk, and test gaps.
  * Use before final summary for non-trivial changed files.

* `refactor_tool`

  * Use before renames/moves/dead-code cleanup.
  * Do not delete unrelated dead code.
  * If dead code unrelated to current task appears, mention it only.

* `run_postprocess_tool`

  * Use only when graph appears stale or user asks to refresh analysis.
  * Do not run expensive postprocess repeatedly during normal edits.

* `embed_graph_tool`

  * Use only if semantic search quality is poor and embeddings are missing/stale.
  * Do not run unless needed.

* `generate_wiki_tool`

  * Use only for documentation/architecture summary tasks.
  * Do not use for normal bugfix.

* `get_docs_section_tool`

  * Use when unsure how to use code-review-graph tools.
  * Prefer this over guessing tool syntax.

### 0.1.3 Graph target strategy

Preferred target order:

1. Exact symbol from user task.
2. Exact file path from user task.
3. Known hot file from AGENTS.md.
4. File guide path from PROJECT_GUIDE.md.
5. Fallback map entry from `PROJECT_AI_MAP.txt`.

When searching:

* First query exact name.
* If no result, query file basename.
* If no result, query owner/type name.
* If no result, use PROJECT_GUIDE.md routing.
* If still no result, use Grep/read-only search.

When graph returns standard/library callees (`get`, `map`, `unwrap_or`, `clone`, `round`, etc.), ignore them unless task is specifically about those calls.

### 0.1.3.1 Graph miss fallback for Rust include/impl methods

`callers_of` can return false `0` for Rust methods split through `include!`/module shell files, especially `impl Type { fn method(...) }` in chunk files.

If `callers_of('/abs/path/file.rs::Type.method')` returns `0` but target is an impl method or constructor:

1. Do not treat `0` as proof of no callers.
2. Use `semantic_search_nodes_tool` for exact variants:

   * `Type.method`
   * `Type::method`
   * `method`
   * implementation file basename

3. Run `file_summary` on both:

   * concrete implementation file, for example `src/renderer/renderer_init_methods.rs`
   * include shell/module file, for example `src/renderer.rs`

4. Retry `callers_of` only with canonical node ids/targets returned by graph search or `file_summary`; do not rely only on hand-written target strings.
5. If graph still returns `0`, use exact read-only `rg` as MCP gap fallback, not as first step:

   * constructors: `rg -n "Type::method\\(" src`
   * methods: `rg -n "\\.method\\(" src`
   * function name fallback: `rg -n "method\\(" src`

6. In summary, say: `Graph callers_of returned 0 for include/impl target; verified with exact rg fallback`.

### 0.1.4 Required graph checks by task type

Bug fix:

1. `semantic_search_nodes_tool` for failing symbol/file.
2. `query_graph_tool file_summary` for target file.
3. `query_graph_tool callers_of` for target function if function behavior changes.
4. `query_graph_tool callees_of` for target function if it delegates logic.
5. `query_graph_tool tests_for` when target function/class known.
6. Read exact source.
7. Patch minimal code.
8. Run `make codex_test` if any RRiter file changed.

Render/input/editor hot-path change:

1. Find target symbol/file.
2. `file_summary`.
3. `callers_of`.
4. `callees_of`.
5. `get_affected_flows_tool` for changed files.
6. Read exact source.
7. Patch allocation-light.
8. Run `make codex_test`.

Review current changes:

1. `get_review_context_tool`.
2. `get_affected_flows_tool`.
3. Inspect high-risk files only.
4. Read exact source/diff as needed.
5. Report risks, tests, and minimal fixes.
6. If editing, run `make codex_test`.

Refactor/rename:

1. `semantic_search_nodes_tool` exact symbol.
2. `query_graph_tool callers_of`.
3. `query_graph_tool callees_of`.
4. `refactor_tool` where useful.
5. Read exact source and direct call sites.
6. Patch only required files.
7. Run `make codex_test`.

Architecture explanation:

1. `get_minimal_context_tool`.
2. `semantic_search_nodes_tool` for entrypoints.
3. `query_graph_tool file_summary`.
4. `get_affected_flows_tool` or docs/wiki tool if needed.
5. Do not edit files.

### 0.1.5 Graph freshness

Graph lives in `.code-review-graph/`.

Normal update command:

```bash
code-review-graph update
```

Full rebuild command:

```bash
code-review-graph build
```

Use full build after large moves/splits or graph corruption.

If full postprocess is too slow, acceptable fallback:

```bash
code-review-graph build --skip-postprocess
```

Do not edit `.code-review-graph/` manually.
Do not commit `.code-review-graph/`.

`.code-review-graphignore` must ignore:

```text
target/
.git/
.code-review-graph/
```

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

1. Use `code-review-graph` MCP to inspect relevant files/symbols/call edges.
2. If MCP unavailable, use `PROJECT_AI_MAP.txt` fallback.
3. Pick minimal files.
4. Ask complete files.
5. No exact patches before full source.
6. No exact code from graph/map/index.

Fallback map format for `PROJECT_AI_MAP.txt`:

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
   Graph ref: `file_summary` / `callers_of` / `callees_of` result for target symbol.

2. path/to/other.rs
   Reason: called by / calls previous symbol.
   Graph ref: direct caller/callee from `code-review-graph`.
```

If MCP unavailable and fallback map is used:

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

* Use `code-review-graph` before broad exploration.
* Edit files directly.
* Use unified diff only when showing changes.
* Do not use chat parser `Before/After` blocks.
* Run `make codex_test` if ANY RRiter-related file changed.

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
* `code-review-graph status`
* `code-review-graph update`
* `code-review-graph update --brief`
* `code-review-graph detect-changes --brief`
* `code-review-graph build` only when graph is missing/stale/broken or after structural source changes
* Read-only inspection commands that stay in project root

Forbidden unless user explicitly asks:

* `git *`
* network commands
* package installs
* destructive commands outside project
* commands outside project root
* whole-project formatting
* repeated full graph rebuilds during normal edit loops

## 5. Plan + Verify

Before multi-file/risky change:

```text
1. Graph check X -> read files Y
2. Change X -> verify focused behavior
3. Change Z -> verify make codex_test
```

Bug fix path:

1. Use `code-review-graph` MCP -> smallest relevant path and impact radius.
2. If MCP unavailable, use `PROJECT_AI_MAP.txt` fallback.
3. Read exact source.
4. Find cause.
5. Patch minimal code.
6. Verify with `make codex_test` if ANY RRiter-related file changed.

Primary success check after edits:

```bash
make codex_test
```

Always run `make codex_test` at the end of task if ANY file related to RRiter changed.

Do not run `make fast` for RRiter unless user explicitly asks.

No-edit tasks:

* Do not run `make codex_test` if no files changed.
* Say explicitly: `Verify: not run, no files changed.`

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

### Platform and filesystem invariants

* Route OS-specific directories, path identity, persistence, atomic replacement, dialogs, Clipboard, Trash, URL/file-manager actions, and background process flags through `src/platform.rs` and `src/platform/*`.
* Keep original `PathBuf` values for I/O/display and use `platform::PathKey` or the platform path helpers for equality, deduplication, containment, watcher keys, and open-tab identity.
* Never persist arbitrary paths through `to_string_lossy`; use `encode_persisted_path` and `decode_persisted_path`.
* Decode editor files through `read_text_file` and preserve `TextFileFormat` when saving, so BOM/UTF-16 and LF/CRLF/CR are not silently rewritten.
* Persist editor/application state through atomic sibling-temp replacement. Do not add direct state-file `fs::write` calls.
* Keep Linux-only crates and code (`io-uring`, Wayland extensions, XDG-specific behavior) behind target gates; missing non-Linux tools must degrade to an explicit disabled/error state, never a restart loop.

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

Before editing hot files:

1. Use `code-review-graph` `file_summary`.
2. Check `callers_of` for edited function.
3. Check `callees_of` if logic delegates.
4. Check `get_affected_flows_tool` for changed file.
5. Keep patch allocation-light.

### UI Architecture

Use declarative UI registry for new buttons/elements when applicable:

1. Add `UiId`.
2. Register element during render.
3. Handle action in `src/app/ui_handlers.rs`.
4. Avoid duplicated manual hitboxes.

Before new UI action:

1. Search graph for existing `UiId` / similar handler.
2. Read `src/ui_system.rs`.
3. Read relevant render module.
4. Read `src/app/ui_handlers.rs`.
5. Patch smallest route.

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

| Level   | Rule                                                        |
| ------- | ----------------------------------------------------------- |
| `lite`  | No filler/hedging. Keep full sentences. Tight professional. |
| `full`  | Drop articles. Fragments OK. Short synonyms.                |
| `ultra` | Abbrev. Arrows for cause. One word when one enough.         |

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

Good with MCP:

```text
Need files:
1. src/app/mouse/input.rs
   Reason: exact click routing needed for `App.handle_main_mouse_input`.
   Graph ref: `semantic_search_nodes_tool(handle_main_mouse_input)` -> `query_graph_tool file_summary` / `callers_of`.

2. src/ui_system.rs
   Reason: UI registry hit-test may own click target.
   Graph ref: `semantic_search_nodes_tool(UiRegistry)` -> `query_graph_tool file_summary`.
```

Good with fallback map:

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
Graph: ...
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
* `.code-review-graph/` -> generated code-review graph database. Do not edit manually. Ignored by git.
* `.code-review-graphignore` -> graph ignore rules. Must ignore `target/`, `.git/`, `.code-review-graph/`.
* `PROJECT_AI_MAP.txt` -> fallback AI source index/call map. Not exact source. Use only when `code-review-graph` MCP unavailable.
* `PROJECT_GUIDE.md` -> broader architecture guide.
* `Cargo.toml` -> deps/profile/features.
* `Makefile` -> `make codex_test`, `make api-map`.
* `build.rs` -> build-time resource/platform setup.
* `src/platform.rs` -> cross-platform directories, `PathKey`, reversible path records, text formats, atomic writes, dialogs/Clipboard/Trash/openers, process helpers.
* `src/platform/windows.rs` -> Windows WTF-16 case-insensitive path identity plus UNC/extended-length Win32 path handling.
* `src/platform/tests.rs` -> platform/path/encoding/atomic-write regression tests.
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
* `src/app/git_diff.rs` -> Git diff state/loading and format-preserving worktree writes.
* `src/app/git_diff_tests.rs` -> Git diff reconstruction, rollback, index/worktree encoding, and invalid-text tests.
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
* `src/lsp/ruff_workspace.rs` -> background `ruff check` workspace diagnostics parser/collector.
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
