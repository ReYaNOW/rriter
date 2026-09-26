RRiter agent rules. Strict mode. Small patches. Fast UI first. Prefer the most performant, least resource-intensive solution for the code you write in the task; do not rework other code for that unless asked. In the final report, mention performance only when a real choice was made (allocation, per-frame work, I/O); no boilerplate justification.

## 0. Context and search

Read on demand, not upfront:

* `PROJECT_GUIDE.md` — architecture (§2), detailed file guide (§3), compact file index (§4). Read only the section you need.
* `docs/agents/code-review-graph.md` — graph tool manual, when you decide to use the graph.
* `docs/agents/chat-workflow.md` — only when working without file access (chat, exact-substring patch parser).
* UI checks headless: `python3 scripts/rriter_headless.py shot <file>` prints the PNG path; protocol, `dump`, `bench` — `docs/headless.md`. It drives the prebuilt release binary (`target/x86_64-unknown-linux-gnu/release/rriter`, `make fast` if missing), so real UI behaviour for writing or fixing UI tests is established from `dump`/`shot` on that binary, without cargo; `make test` once at the end. Reason: every `make test` relinks the test binary for minutes, and an agent that re-runs it after each fix spends an hour on a quarter of the work (26.09).

Search (`rg`-first):

* Default: `rg -n` for symbols, strings, error text, file names. Start narrow (exact symbol or distinctive string), widen only if empty.
* The Bash tool runs **zsh**, not bash. Bash-only syntax fails with `bad substitution` (`${var^^}`, `${!var}`, `mapfile`), and an unmatched glob aborts the whole command with `no matches found` instead of passing the pattern through. Keep commands POSIX-ish, or quote the glob and let the tool expand it.
* Read only the files/ranges the search points to. Always read exact source before editing.
* `code-review-graph` MCP is optional: use it when `rg` answers poorly — callers of a widely used function before changing its behavior, test ownership, impact of a multi-file change. Never a mandatory first step.
* Graph output is an index, not source. Rust `include!`-split files can return false `0` callers; verify with `rg`.

## 1. Role

Act as strict experienced programmer.

Priority order:

1. Smooth UI + max FPS for UI/render/input
2. Max reasonable optimization without stability loss
3. Small readable maintainable code
4. Surgical changes only

No speculative features. No broad refactors unless asked.

## 2. Working style

* Act, don't ask: find the cause, change code, run checks. Stop only for destructive or out-of-scope actions.
* Context discipline: search first, then read relevant ranges (offset/limit for large files). Do not read whole large files or guides "just in case".
* Long command output (builds, tests): redirect to a file (`cmd > /tmp/<task>.log 2>&1`), then `grep`/`tail` it. Never dump full logs into context.
* While a long build or test run is in flight (`make codex_test`, `make test`, `make fast`, cargo builds): start it in the background and then **go idle**. No polling the log, no peeking at partial output, no "meanwhile" side work, no thinking out loud. Every such check is a full model turn that re-ships the whole context for nothing. Wait for the completion notification, then read the result once with `grep`/`tail`.
* Feedback loop: after a substantive edit run the narrowest check (`make test TEST_FILTER=<module path>`) and fix what it reports before moving on.
* Fix at the root: if a shared helper is wrong, fix the helper, not one caller; check sibling code paths that use it.
* Batch independent searches/reads in one step.
* Edit files directly. Use unified diff only when showing changes; do not use chat parser `Before/After` blocks.
* Final report: short — root cause (`file:line`), change, verification result (plus the SOTA note from the header when it applies). No essays.
* Task tracking: rriter tasks go to the Kanri board, never Linear (Linear is Skyhold-only). Use the `kanri-task` skill, or `kanri-add "Title" [--desc ...] [--board RRiter] [--column Backlog|Features|Bugs|Doing|Done] [--top] [--list]` — it writes straight into Kanri's JSON store `~/.local/share/tech.trobonox.kanri/.kanri.dat`, keeps a backup, and refuses to write while Kanri is running (Kanri would overwrite the file on its own save).

## 3. Allowed Ops

Allowed file ops inside project:

* Read files
* Search files
* Edit files
* Create only `.rs`, `.py`, `.dart`, `.md`, `.txt`
* Delete only `.rs`, `.py`, `.dart`, `.md`, `.txt` when directly required

Allowed shell commands:

* `ls`
* `grep`
* `rg`
* `python3 gen_project_ai_map.py`
* `make codex_test`
* `make test TEST_FILTER=<module path>` (focused tests)
* No `rustfmt` / `cargo fmt` at all, not even on a single file: the tree is not rustfmt-clean, and rustfmt follows `mod` declarations, so one file drags dozens of untouched files into the diff. Match the surrounding style by hand.
* `make api-map` if still present
* `python3 scripts/build_windows.py --self-test`
* `python3 scripts/build_macos.py --self-test`
* `code-review-graph status`
* `code-review-graph update`
* `code-review-graph update --brief`
* `code-review-graph detect-changes --brief`
* `code-review-graph build` only when graph is missing/stale/broken or after structural source changes
* Read-only inspection commands that stay in project root

Forbidden unless user explicitly asks:

* `git push` or anything other that could change something in remote/origin
* network commands (only mutable)
* package installs
* destructive commands outside project
* whole-project formatting

## 4. Plan + Verify

### Builds and tests with several agents

Only one agent builds or tests at a time, in the main checkout where `target/` is warm. Parallel agents only edit; the integrator builds and runs tests once after integration. A fresh worktree starts with an empty `target/`: the first build takes tens of minutes and gigabytes, and concurrent Cargo processes fight for CPU and for one target directory.

If you are in SuperPowers workflow, you can run tests how you like, you can ignore later required make codex_test 

Bug fix path: `rg` (graph optional for callers/impact) -> read exact source -> find root cause -> minimal patch -> focused test -> `make codex_test` at the end.

Primary success check after edits:

```bash
make codex_test
```

Always run `make codex_test` at the end of task if ANY file related to RRiter changed.

Run it in the background and stay silent until it reports. It takes minutes; checking on it costs a full model turn each time and tells you nothing the completion notification will not.

Do not run `make fast` for RRiter unless user explicitly asks.

No-edit tasks:

* Do not run `make codex_test` if no files changed.
* Say explicitly: `Verify: not run, no files changed.`

## 5. Coding Rules

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
* No new source file under 200 lines unless asked.
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

* Hard limit: no source file above 1600 lines; move logic blocks to new files before a file would exceed it. Add new files to the compact file index in `PROJECT_GUIDE.md` §4.

### Platform and filesystem invariants

* Route OS-specific directories, path identity, persistence, atomic replacement, dialogs, Clipboard, Trash, URL/file-manager actions, and background process flags through `src/platform.rs` and `src/platform/*`.
* Keep original `PathBuf` values for I/O/display and use `platform::PathKey` or the platform path helpers for equality, deduplication, containment, watcher keys, and open-tab identity.
* Never persist arbitrary paths through `to_string_lossy`; use `encode_persisted_path` and `decode_persisted_path`.
* Decode editor files through `read_text_file` and preserve `TextFileFormat` when saving, so BOM/UTF-16 and LF/CRLF/CR are not silently rewritten.
* Persist editor/application state through atomic sibling-temp replacement. Do not add direct state-file `fs::write` calls.
* Keep Linux-only crates and code (`io-uring`, Wayland extensions, XDG-specific behavior) behind target gates; missing non-Linux tools must degrade to an explicit disabled/error state, never a restart loop.
* Launch external tools through `platform::ManagedChild`, `platform::run_command_output`, or a feature wrapper built on those APIs. Do not add unmanaged long-lived `Command::spawn` calls.
* Every long-lived child must belong to the Unix process group or Windows Job Object owned by RRiter. Shutdown must be bounded: graceful request first, then terminate the complete process tree after a timeout.
* Resolve optional tools through `platform::resolve_executable`/`command_for_tool`, including Windows `PATHEXT` and configured path overrides. A missing tool is a stable `Missing`/disabled state and must not cause automatic restart or repeated log spam.
* Route Git CLI work through `src/app/git_panel/git_process.rs`. Preserve Git Credential Manager, ssh-agent, `core.sshCommand`, proxy configuration, and bounded process-tree shutdown; never build Git commands through a shell string.
* Build network clients through `platform::blocking_http_client_builder` / `platform::async_http_client_builder`, so API Client, API Mock, and tool bootstrap share native trust roots and environment/native proxy policy. Feature code may add its own bounded timeout and cache identity.
* Keep API credentials out of ordinary JSON state. Read/write them through `platform::open_user_secret`, `seal_user_secret`, and `atomic_write_secret`; preserve plaintext loading only as a migration path.
* Keep native multipart selections as `PathBuf` values until filesystem access. Display strings are not the source of truth for uploads.
* Keep application shortcuts, terminal Control, and word-navigation modifiers separate. Windows AltGr must remain text input; macOS Command is the application shortcut modifier and Option is word navigation/text input.
* Persist Git/Ruff/Ty/uv/Python/shell overrides through `platform::ToolPaths`; do not mutate process-wide environment variables from settings. Refresh the shared resolution cache after a setting changes.
* Install uv/Ruff/Ty only through `src/app/tool_installer.rs`. Managed installs belong under RRiter data/cache directories, must not edit shell profiles or require elevation, must expose cancelable progress/logs, and must use the managed process-tree API without automatic retry loops.
* Native dialogs on macOS are main-thread operations. Route them through App/event-loop handlers instead of spawning an arbitrary worker thread.
* macOS must request OpenGL 4.1 Core only. Windows may fall back from OpenGL 4.1 Core to 3.3 Core. Keep GLES shaders and desktop GLSL preambles distinct.
* Windows/macOS protected-save elevation must use the validated `--rriter-elevated-save` helper request; never interpolate arbitrary editor paths into a shell command.
* Release packaging is owned by `scripts/build_windows.py` and `scripts/build_macos.py`. Keep manifests, signing, notarization, icons, portable artifacts, and installer/DMG creation there rather than in feature modules.

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

1. Find callers of the edited function (`rg`, or graph `callers_of` for widely used helpers).
2. Check callees if the logic delegates.
3. Keep the patch allocation-light and run the focused tests for the module.

### UI Architecture

Use declarative UI registry for new buttons/elements when applicable:

1. Add `UiId`.
2. Register element during render.
3. Handle action in `src/app/ui_handlers.rs`.
4. Avoid duplicated manual hitboxes.

Before new UI action:

1. Search (`rg`) for existing `UiId` / similar handler.
2. Read `src/ui_system.rs`.
3. Read relevant render module.
4. Read `src/app/ui_handlers.rs`.
5. Patch smallest route.

## 6. Routing

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

Architecture and full file index: `PROJECT_GUIDE.md`.
