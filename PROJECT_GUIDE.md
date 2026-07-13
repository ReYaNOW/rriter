Architecture guide for RRiter.

Use this file only when broader architecture context is needed. For file selection, prefer `code-review-graph` MCP.

Use `PROJECT_AI_MAP.txt` only as fallback when MCP is unavailable.

## 1. Product philosophy

RRiter is a lightweight editor built against heavy editor trends: no Electron, no JVM, no heavyweight retained UI framework.

Main principles:

1. Immediate-mode UI.
2. GPU-centric rendering.
3. Async heavy work.
4. Explicit memory control.
5. Minimal abstractions.
6. Smooth input/render path first.

### Benchmark tools

Files:

```text
src/bin/project_search_grep_searcher_bench.rs
src/bin/project_search_io_uring_bench.rs
```

These standalone Cargo binaries benchmark project substring search backends against fixed IDE workspaces without spawning external search processes. The `io_uring` binary is Linux-only and is built only with the `linux-io-uring-bench` feature; it must not enter Windows or macOS all-target builds.

## 2. Core architecture

### Platform and filesystem boundary

Files:

```text
src/platform.rs
src/platform/integration.rs
src/platform/process.rs
src/platform/elevated_save.rs
src/platform/windows.rs
src/platform/macos.rs
src/platform/tests.rs
```

`platform.rs` is the public boundary for behavior that differs by operating system. `platform/integration.rs` owns native config/data/cache/state directories, configured tool discovery, native trust/proxy hooks, and process memory. `platform/process.rs` owns executable discovery, cancelable captured command timeouts, Unix process groups, Windows Job Objects, and deterministic process-tree cleanup. `platform/elevated_save.rs` owns the validated helper protocol used for protected atomic saves. Windows and macOS native APIs remain in their target-specific modules.

Important invariants:

* Keep the original `PathBuf` for filesystem calls and user-visible spelling.
* Use `PathKey` for equality, deduplication, containment caches, open tabs, watchers, and workspaces. Windows keys are case-insensitive WTF-16 and understand drive, UNC, and extended-length prefixes.
* Persist paths with `encode_persisted_path`/`decode_persisted_path`; never serialize arbitrary paths as lossy UTF-8.
* Editor text is normalized to LF internally, while `TextFileFormat` preserves UTF-8/BOM/UTF-16 and the original LF/CRLF/CR style on save.
* Mutable state files and editor saves use sibling-temp atomic replacement. Do not add direct `fs::write` paths for persisted application state.
* Linux-specific Wayland, XDG, FreeDesktop Trash, and `io_uring` behavior stays behind target gates. `src/platform/windows.rs` owns non-lossy Windows path normalization and extended-length Win32 paths.
* Long-lived tools and captured commands use the managed process API. Child processes must not outlive RRiter; graceful shutdown is followed by a bounded full-tree termination.
* Optional executable lookup honors configured overrides, `PATH`, and Windows `PATHEXT`. Missing tools enter a stable disabled state rather than a restart loop.
* User credentials are persisted separately from ordinary state. Windows records are protected with per-user DPAPI entropy; legacy plaintext remains readable only for migration, and Unix fallback files are created atomically with mode `0600`.
* Windows and macOS HTTP clients add user-installed native trust roots and native static proxy settings while preserving explicit proxy environment overrides.
* Application shortcuts, terminal Control, and word-navigation modifiers are separate policies. This preserves Windows AltGr text and native macOS Command/Option behavior.
* Tool overrides are persisted as native paths in `ToolPaths`, resolved without changing global environment variables, and surfaced in settings together with their source and availability.
* macOS native dialogs stay on the main event-loop thread. Windows uses a DPI-aware, long-path-aware manifest and application identity; macOS uses a regular AppKit application lifecycle and default menu.
* OpenGL context policy is explicit: macOS 4.1 Core only, Windows 4.1 Core then 3.3 Core, Linux desktop 4.1/3.3 then GLES 3.0. Renderer diagnostics retain the requested profile, actual GL/GLSL versions, GPU identity, and scale factor.

### Managed Git integration

Files:

```text
src/app/git_panel/git_process.rs
src/app/git_panel/git_panel_graph_helpers.rs
src/app/git_panel/git_panel_status_tests.rs
```

Git graph queries and fetch/pull/push commands use the shared managed-process layer. `RRITER_GIT_PATH` can select a non-default executable, and Windows network operations default to the Schannel backend unless `RRITER_GIT_SSL_BACKEND` overrides it. RRiter does not replace `GIT_SSH_COMMAND`, `core.sshCommand`, Git Credential Manager, ssh-agent, or Git proxy settings. Commands run without a console window, with stdin closed and bounded timeouts, and failures preserve the original diagnostic while adding authentication/certificate/proxy guidance.

Repository identity and graph caches use `PathKey`, so drive-letter/case aliases do not duplicate Windows repositories. Status/stage tests cover `core.autocrlf`, `core.filemode=false`, and case-only renames.

### API Client and API Mock platform runtime

Files:

```text
src/app/api_client.rs
src/app/api_client/api_client_loading_parser.rs
src/app/api_client/api_client_request_runtime.rs
src/app/api_client/api_client_defaults_persist.rs
src/app/api_client/api_client_app_text_methods.rs
src/app/api_mock/server.rs
src/app/api_mock/persist.rs
```

Blocking API requests and the asynchronous API Mock proxy share builders that apply the same trust roots and proxy policy. Direct server-reach timing uses a bounded TCP connect instead of platform-specific `ping`; it is suppressed when proxy routing is active because a direct probe would be misleading.

Authentication is stored separately through the platform secret envelope and atomic secret writer. Ordinary specs, mock state, caches, and OpenAPI exports use atomic replacement. Multipart picker results stay as native `PathBuf` values, including Windows paths with spaces/UNC paths and non-UTF Unix paths; lossy display text is never used as the selected upload path. Copied cURL commands use POSIX quoting on Linux/macOS and explicit `curl.exe` plus PowerShell quoting/continuations on Windows.

`uv python list` and `uv python install` are cancelable managed processes. Closing RRiter cancels those workers, waits briefly, and terminates their complete process trees. Python worker and Ty commands reuse the same platform lifecycle and accept configured executable paths containing spaces.

### Text engine

File:

```text
src/editor.rs
src/editor_navigation.rs
```

RRiter uses gap buffer storage instead of plain `String`.

Important ideas:

* Text stored in flat `Vec<u8>`.
* Gap sits at cursor.
* Insert/delete near cursor is cheap.
* Moving cursor moves gap.
* `line_offsets` gives fast line lookup.
* Undo/redo stores edit operations, not whole snapshots.

`editor_navigation.rs` owns cursor movement, selection expansion, word/line navigation, and folded-code cursor snapping.

### Rendering

Files:

```text
src/renderer.rs
src/render_view.rs
src/render_view/*
```

Rendering is batched.

Important ideas:

* CPU computes vertices.
* GPU draws batches.
* Text, rectangles, SDF rounded rects, noise, icons are pushed into renderer buffers.
* Hot draw path must avoid large allocations.
* `render_view.rs` is frame orchestrator.
* `render_view/editor_text_layer.rs` owns visible editor glyph/background/cursor draw loops.
* `render_view/ide_panels.rs` owns IDE sidebar, explorer/project search panels, and bottom panel shells.
* `render_view/hover_overlays.rs` owns LSP squiggle hover collection and hover overlay draw routing.
* Specialized modules render text, tabs, minimap, terminal, panels, dialogs.

### Syntax highlighting

Files:

```text
src/highlighter.rs
src/highlighter_runtime.rs
src/queries.rs
src/languages/*
```

Tree-sitter runs asynchronously.

Important ideas:

* Main thread sends edits.
* Background thread parses.
* Highlight spans return later.
* Main thread shifts old spans optimistically after edits.
* Queries define capture colors.
* Language injection supported where implemented.

### App events and input

Files:

```text
src/app.rs
src/app/app_state.rs
src/app/project_search.rs
src/app/project_search_grep.rs
src/app/project_search_preview.rs
src/app/project_search_app.rs
src/app/events.rs
src/app/events/*
src/app/keyboard.rs
src/app/keyboard/*
src/app/mouse.rs
src/app/mouse/*
```

Input is split by behavior:

* Window events and frame tick.
* Shared app state lives in `app_state.rs`; behavior impls stay in `app.rs` and submodules.
* Keyboard routing.
* Mouse press/release.
* Cursor movement.
* Wheel routing.
* Source hover handling.

### UI system

Files:

```text
src/ui_system.rs
src/app/ui_handlers.rs
```

Declarative UI registry reduces duplicated render/click logic.

Add new UI action:

1. Add `UiId`.
2. Register button/element during rendering.
3. Add match arm in `handle_ui_click`.
4. Avoid manual duplicated hitboxes unless existing local system requires it.

### LSP

Files:

```text
src/lsp.rs
src/lsp/*
src/languages/*
src/app/lsp_actions.rs
```

LSP client is async and lightweight.

Important ideas:

* Supervisor owns a managed server process and its complete process tree.
* Reader/writer threads handle JSON-RPC I/O.
* Main thread receives `LspEvent`.
* Missing Ruff/Ty binaries produce one stable `Missing` state; crashes use bounded exponential restart and explicit retry resets suppression.
* Shutdown sends the LSP `shutdown`/`exit` sequence, waits briefly, then terminates the process tree if necessary.
* File URIs are generated and parsed through `url::Url`, including Windows drive paths, UNC shares, Unicode, spaces, `#`, and `%`.
* Hover/diagnostic text is normalized and highlighted separately.
* Python-specific formatting lives under `languages/python.rs`.

### Terminal

Files:

```text
src/app/terminal.rs
src/app/terminal_process.rs
src/render_view/terminal_ui.rs
```

Integrated terminal uses PTY and Alacritty terminal grid.

Important ideas:

* `Terminal` owns the grid facade while `TerminalProcess` owns the PTY and process tree.
* Shell selection is platform-aware: PowerShell/cmd on Windows, the configured shell with zsh fallback on macOS, and the configured shell with bash/sh fallback on Linux.
* PTY spawn errors are rendered in the terminal instead of panicking.
* I/O thread reads terminal output in batches.
* Grid is shared with renderer.
* Render path reads visible cells.
* Terminal focused state captures keyboard input.
* Closing a tab or RRiter performs bounded PTY/process-tree shutdown.
* Physical Control is distinct from application shortcuts, so terminal interrupts, Windows AltGr, and macOS Command copy/paste keep native behavior.

### Scroll and animation

File:

```text
src/scroll.rs
```

Reusable scroll physics.

Used by editor, minimap, panels, autocomplete, settings, and similar scrollable surfaces.

## 3. File and folder guide

### Root

#### `build.rs`

Build-time Rust script. It keeps generated icon associations and emits the Windows application manifest linker argument. The manifest declares Per-Monitor V2 DPI awareness, UTF-8 active code page, supported Windows versions, and long-path awareness. `scripts/build_windows.py` supplies the compiled icon/version resource through `RRITER_WINDOWS_RESOURCE`.

#### `scripts/build_windows.py`

Standard-library-only Windows release driver. It discovers Visual Studio Build Tools, imports the x64 MSVC environment, installs/checks the nightly MSVC target, builds native PE resources, optionally runs tests, creates a portable ZIP and Inno Setup installer, signs artifacts, and can launch RRiter. Run its platform-independent checks with `python3 scripts/build_windows.py --self-test`.

#### `scripts/build_macos.py`

Standard-library-only macOS release driver. It builds native or Universal 2 executables, creates a Retina `.app` and ICNS, signs nested code before the bundle under hardened runtime, optionally notarizes/staples, creates a DMG, verifies Gatekeeper, and can launch RRiter. Run `python3 scripts/build_macos.py --self-test` on any platform.

Full operator commands live in `WINDOWS_BUILD.md` and `MACOS_BUILD.md`.

#### `Cargo.toml`

Crate manifest, dependencies, release profile.

Important constraints:

* Release profile optimized.
* Panic abort.
* Graphics stack: `winit`, `glutin`, `glow`.
* Text/render support: `swash`, `image`, `resvg`, `tiny-skia`.
* Syntax: Tree-sitter crates.
* Terminal: `alacritty_terminal`, `portable-pty`.
* Async/background helpers: channels/std threads where used.

#### `Makefile`

Project commands. Main verification command:

```bash
make codex_test
```

Use `make codex_test` when tests matter.

#### `scripts/gen_project_map.py`

Generates fallback `PROJECT_AI_MAP.txt`.

Primary project index/call graph is `code-review-graph` MCP.
Use this script only when maintaining fallback map.

Use after source layout changes if fallback map must stay current:

```bash
make api-map
```

#### `PROJECT_AI_MAP.txt`

Fallback compact AI navigation map.

Use only when `code-review-graph` MCP is unavailable or broken.

Used to select source files and understand approximate call edges.

Not source code. Never create exact patches from this map only.

#### `.code-review-graph/`

Generated graph database for `code-review-graph`.

Do not edit manually.
Do not commit.

Regenerate with:

```bash
code-review-graph build
```

If full postprocess is too slow, usable fallback build:

```bash
code-review-graph build --skip-postprocess
```

Recommended `.code-review-graphignore`:

```text
target/
.git/
.code-review-graph/
```

#### `AGENTS.md`

Main coding-agent rules.

#### `PROJECT_GUIDE.md`

This architecture guide.

### `src/platform.rs` and `src/platform/*`

Cross-platform boundary for native directories, path identity/persistence, text encodings and line endings, atomic filesystem replacement, dialogs, Clipboard, Trash, URL/file-manager integration, modifier policy, managed processes, and release-facing native integrations.

* `src/platform/integration.rs` -> app directories, `ToolPaths`/tool-resolution cache, native trust/proxy dispatch, and process memory.
* `src/platform/process.rs` -> configured executable resolution, Windows `PATHEXT`, cancelable captured output with timeout, Unix process groups, Windows Job Objects, and complete-tree termination.
* `src/platform/elevated_save.rs` -> non-shell validated helper request, elevated atomic replacement, result propagation, and Linux `pkexec` compatibility.
* `src/platform/windows.rs` -> Windows WTF-16 path keys, case folding, UNC/extended-length handling, AppUserModelID, DPAPI, native certificate/proxy discovery, elevation, and process memory.
* `src/platform/macos.rs` -> Keychain secrets, Finder/URL integration, `scutil` proxy parsing, Keychain certificates, Mach memory, and administrator authorization helper.
* `src/platform/tests.rs` -> platform/path/text/atomic-write/modifier/tool-resolution regression tests that can run on Linux, plus target-gated native tests.

Use these APIs instead of introducing platform checks, lossy path strings, unmanaged long-lived child processes, or shell-command strings in feature modules. Native tool paths are configured from settings through `ToolPaths`; feature code must not rewrite process-wide environment variables.

### Window, graphics, and native input bootstrap

`src/app/events/window_runtime.rs` selects the platform GL context plan and owns window/display/surface creation. macOS requests OpenGL 4.1 Core only; Windows falls back from 4.1 Core to 3.3 Core; Linux additionally permits GLES 3.0. `Renderer::new` validates the actual context, selects desktop or GLES shader preambles, and records copyable diagnostics. `ScaleFactorChanged` rebuilds scale-sensitive atlases/caches rather than stretching stale glyph data. IME commits are routed to the active editor/API/modal target as one logical edit.

macOS runs as a regular AppKit application with the default application menu. File dialogs are dispatched on the event-loop thread. Windows startup applies the stable application identity in addition to the embedded DPI/long-path manifest.

### `src/main.rs`

Application entrypoint.

Likely owns startup wiring, event loop creation, renderer/app initialization, and top-level run logic.

Use when task touches startup, window creation, GL context setup, or global app boot.

### `src/app.rs`

Main `App` behavior impls and app-level editor/workspace operations.

Use when changing tab/file/search behavior that crosses app state and subsystems.

Implementation is split through `include!`:

* `src/app/app_ide_tab_methods.rs` -> IDE mode startup, tab titles, reveal/current tab sync.
* `src/app/app_file_tab_methods.rs` -> file/tab open, save, switch, highlight wait.
* `src/app/app_window_external_methods.rs` -> window title, search, close, external file changes.

Autocomplete-specific `App` methods live in `src/app/autocomplete.rs`.
Python completion/fold/source-owner helpers live in `src/app/python_completion.rs`.
Headless app tests are split between `src/app/app_behavior_tests.rs` and `src/app/app_file_behavior_tests.rs`.

Large app files use thin include shells to keep source chunks small:

* `src/app/api_client/*` -> API client loading/parsing, shared native-root/proxy HTTP builders, native upload paths, cancelable Python runtime tasks, request runtime, layout/input helpers, protected auth persistence, and tests.
* `src/app/api_client/api_client_app_mock_contract_methods.rs` -> API mock contract toggles and async OpenAPI export entrypoint.
* `src/app/api_mock/contract.rs` -> structured Python mock contract builder for handler signature, locked classes, runtime args, defaults, and schema export.
* `src/app/api_mock/openapi_export.rs` -> OpenAPI JSON patch/synthesis for selected spec plus manual mock routes.
* `src/app/autocomplete/*` -> detail helpers, detail request/merge flow, Ty autocomplete flow, popup/apply flow.
* `src/app/python_completion/*` -> source/module helpers and class/member helpers.
* `src/app/app_behavior_tests/*` -> autocomplete basics, Ty cache/tree-sitter cases, member owner cases.
* `src/app/app_file_behavior_tests/*` -> file/tab flow, IDE definition jumps, UI/Git/API cases.
* `src/app/git_panel/git_process.rs` -> managed Git CLI, executable override, Schannel/network policy, credential/SSH/proxy preservation, timeout, and failure classification.
* `src/app/git_panel/*` -> Git panel types, `App` graph/actions, graph helpers, status/tests.
* `src/app/git_diff.rs` -> diff loading/render state and format-preserving worktree save flow.
* `src/app/git_diff_tests.rs` -> Git diff reconstruction, rollback, index/worktree encoding, CRLF, and invalid-text regressions.

### `src/app/app_state.rs`

Main `App` state and app-owned structs.

Owns editor state, panel state, UI state, LSP menu state, terminal/search state, file tree state, settings, dialogs, and tab metadata.

Use when adding/removing persistent app fields or panel/tab state types. Keep behavior in `app.rs` or targeted submodules.

### `src/app/events.rs`

High-level window event handling.

Use when behavior depends on `winit` events, resize, redraw, focus, close request, or event routing.

### `src/app/events/about.rs`

Frame tick / `about_to_wait`.

Responsibilities:

* Scroll physics updates.
* Animation updates.
* Polling async systems.
* Delayed dialogs.
* LSP event drain.
* Redraw scheduling.

Use when behavior changes every frame or needs event-loop wakeup.

Helpers/tests split from the frame tick live in `src/app/events/about/*`.

### `src/app/events/source_hover.rs`

Source-backed hover replacement and hover-source logic.

Responsibilities:

* Source signature replacement.
* Module path prefixing.
* Hover-specific tests.

Use when LSP hover text should be enhanced using source context.

### `src/app/keyboard.rs`

Keyboard module router.

Use when keyboard routing between modes changes.

### `src/app/keyboard/main_keys.rs`

Top-level keyboard dispatch.

Responsibilities:

* Dialog/search/settings routing.
* Terminal focus routing.
* Editor-mode routing.
* Global shortcuts.

Use when shortcut behavior depends on current UI mode.

### `src/app/keyboard/editor_keys.rs`

Editor keyboard behavior.

Responsibilities:

* Text edits.
* Autocomplete.
* Tab commands.
* Editor-specific shortcuts.

Use when keystrokes modify text or editor state.

### `src/app/mouse.rs`

Mouse module router.

Use when mouse routing across submodules changes.

### `src/app/mouse/hover_state_core.rs`

Hover popup/state structs and hover bridge geometry.

Use when popup lifetime, combined diagnostic/type hover state, or bridge hit-testing changes.

### `src/app/mouse/hover_mouse_logic.rs`

Hover token normalization, diagnostic hover byte ranges, shared hover visibility helpers, and hover selection byte hit-testing.

Use when hover targets, f-string/string expansion, or popup visibility rules change.

### `src/app/mouse/hover_mouse_tests.rs`

Tests for hover state and hover targeting. Do not delete during refactors.

### `src/app/mouse/input.rs`

Mouse press/release handling.

Responsibilities:

* Clicks.
* Drag start/end.
* UI registry click dispatch.
* Tab/sidebar/panel actions if not yet moved to `ui_handlers`.

Use when button click, drag, panel click, or mouse release behavior changes.

### `src/app/mouse/cursor.rs`

Mouse move handling.

Responsibilities:

* Cursor movement.
* Hover hit-testing.
* Drag update.
* Pointer hover state.

Use when behavior changes while mouse moves.

### `src/app/mouse/wheel.rs`

Mouse wheel routing.

Responsibilities:

* Editor scroll.
* Panel scroll.
* Hover popup scroll.
* Terminal scroll.
* Settings scroll.
* Autocomplete scroll.

Use when wheel behavior is wrong or target-specific scrolling changes.

### `src/app/ui_handlers.rs`

Central UI action handlers.

Responsibilities:

* `UiId` action match.
* Button behavior.
* Dialog button behavior.
* Settings/LSP/file-tree actions when registered through UI system.

Use when adding/changing registered UI button action.

### `src/app/file_tree.rs`

File explorer/tree state and behavior.

Use when task touches project tree, directory expansion, file opening from tree, file watching, or path display.

Submodules:

* `src/app/file_tree_scan.rs` -> background scan/watch and icon raster cache.
* `src/app/file_tree_ops.rs` -> filesystem copy/move/delete/trash helpers.
* `src/app/file_tree_dialog.rs` -> dialog hit-testing and keyboard routing.
* `src/app/file_tree_tests.rs` -> file-tree tests.

### `src/app/file_icons.rs`

File icon mapping.

Use when icon choice by extension/path changes.

### `src/app/lsp_actions.rs`

User-facing LSP actions.

Use when changing code actions, go-to-definition, diagnostics actions, or LSP-triggered editor commands.

### `src/app/terminal.rs`

Terminal state facade.

Responsibilities:

* Terminal grid.
* Delegated keyboard bytes, resize, and dirty/redraw state.
* Ownership of terminal shutdown.

### `src/app/terminal_process.rs`

Managed terminal backend.

Responsibilities:

* Platform-aware shell discovery and arguments.
* Workspace/current-file working directory selection.
* PTY spawn and batched output reader.
* Keyboard bytes and resize requests to PTY.
* Unix process-group/Windows Job Object ownership.
* Bounded graceful shutdown followed by complete-tree termination.

Use when terminal backend/input/output behavior changes.

### `src/editor.rs`

Core text buffer.

Responsibilities:

* Gap buffer.
* Insert/delete.
* Undo/redo.
* Line offsets.
* Text state.
* Selection state if defined there.

Use when text mutation, history, line indexing, or buffer invariants change.

Implementation is split through `include!`:

* `src/editor/editor_core.rs` -> production editor core.
* `src/editor/editor_behavior_tests.rs` -> editor behavior tests.

### `src/editor_navigation.rs`

Cursor and selection navigation.

Responsibilities:

* Cursor movement.
* Word movement.
* Line movement.
* Page movement.
* Selection expansion.
* Fold-aware navigation.

Use when caret movement or selection behavior is wrong.

### `src/renderer.rs`

Low-level renderer.

Responsibilities:

* OpenGL wrapper.
* Shaders.
* Vertex buffer.
* Texture/font atlas.
* Primitive drawing.
* Flush.

Use when changing GPU primitives, shader modes, atlas behavior, or batching.

Vertex layout and primitive geometry helpers live in `src/renderer/geometry.rs`.

Implementation is split through `include!`:

* `src/renderer/renderer_types.rs` -> renderer data types/constants/imports.
* `src/renderer/renderer_init_methods.rs` -> construction, GL setup, atlas bootstrap.
* `src/renderer/renderer_glyph_methods.rs` -> glyph/icon cache lookup.
* `src/renderer/renderer_primitives_tests.rs` -> resize/primitives and renderer tests.

Hot path. Avoid allocations and I/O.

### `src/render_view.rs`

Main frame renderer/orchestrator.

Responsibilities:

* Full draw sequence.
* Cache update.
* Editor layer composition.
* Calling specialized render modules.
* Final GPU flush.

Use when changing overall draw order, layer ordering, viewport/projection behavior, or frame-level UI.

Implementation is split through `include!`:

* `src/render_view/root_helpers.rs` -> frame constants/helpers/tests.
* `src/render_view/root_frame_helpers.rs` -> inline-git and Git diff floating panel helpers.
* `src/render_view/root_frame_overlay_helpers.rs` -> overlay, resize, search, and notice helpers.
* `src/render_view/root_frame_renderer.rs` -> main `Renderer::draw`.
* `src/render_view/api_client_panel/*` and `src/render_view/api_client_tab/*` -> API client renderer chunks.
* `src/render_view/api_client_tab/api_client_tab_mock_contract_renderer.rs` -> Python mock contract toggles and locked class block rendering.

Hot path.

### `src/render_view/editor_text_layer.rs`

Visible editor text layer renderer.

Responsibilities:

* Indent guides.
* Selection/search/identical-word/bracket backgrounds.
* Glyph draw loop for visible visual lines.
* Fold dots and cursor draw.

Hot path. Keep allocation-light.

### `src/render_view/ide_panels.rs`

IDE chrome/panel shell rendering.

Responsibilities:

* Left sidebar buttons.
* Top panel shell and explorer tree rows.
* Bottom panel shell and dispatch to terminal/problems/LSP panels.

Use when panel chrome or explorer row rendering changes.

Implementation is split through `include!`:

* `src/render_view/ide_panels/ide_panel_helpers.rs` -> shared layout/tooltip helpers.
* `src/render_view/ide_panels/ide_panel_side_renderer.rs` -> side/top panels and explorer rows.
* `src/render_view/ide_panels/ide_panel_project_search_renderer.rs` -> project search controls and virtualized result rows.
* `src/render_view/ide_panels/ide_panel_git_tooltip_renderer.rs` -> Git graph/file tooltip drawing.
* `src/render_view/ide_panels/ide_panel_git_graph_renderer.rs` -> Git graph panel.
* `src/render_view/ide_panels/ide_panel_git_workspace_renderer.rs` -> Git workspace panel.
* `src/render_view/ide_panels/ide_panel_dialog_renderer.rs` -> bottom panel and file/Git dialogs.
* `src/render_view/ide_panels/ide_panel_behavior_tests.rs` -> panel behavior/render helper tests.

### `src/render_view/hover_overlays.rs`

Hover overlay orchestration.

Responsibilities:

* LSP squiggle drawing and diagnostic hover collection.
* Diagnostic/type hover visibility routing.
* Dispatch to diagnostic popup and type hover popup rendering.

Use when hover overlay ordering or diagnostic/type popup interaction changes.

### `src/render_view/core_text.rs`

Core editor text rendering.

Responsibilities:

* Visible text.
* Cursor.
* Selection.
* Bracket highlight.
* Identical word highlight.
* Text layout interaction.

Use when editor text visuals are wrong.

Hot path.

### `src/render_view/sticky.rs`

Sticky headers.

Responsibilities:

* Code nesting analysis for sticky lines.
* Sticky line positioning.
* Animation/rendering of sticky headers.

Use when sticky headers are wrong.

### `src/render_view/search.rs`

Search panel rendering.

Responsibilities:

* Ctrl+F panel visuals.
* Search input field.
* Search buttons.
* Result display.

Use when search UI rendering changes.

### `src/render_view/tabs_ui.rs`

Tab bar rendering.

Responsibilities:

* Open file tabs.
* Active tab.
* Close buttons.
* Tab hitbox visuals if not elsewhere.

Use when tab visuals or layout changes.

### `src/render_view/minimap_ui.rs`

Minimap rendering.

Responsibilities:

* Minimap content.
* Minimap viewport.
* Minimap visual scroll indicator.

Use when minimap display changes.

### `src/render_view/terminal_ui.rs`

Terminal panel rendering.

Responsibilities:

* Terminal grid display.
* Terminal cursor/selection if present.
* Focus border.
* Terminal panel visuals.

Use when terminal looks wrong but backend works.

### `src/render_view/lsp_ui.rs`

LSP UI panels.

Responsibilities:

* LSP server panel.
* LSP status visuals.
* Server restart/controls if rendered here.

Use when LSP UI rendering changes.

### `src/render_view/diag_popup_ui.rs`

Diagnostic popup rendering.

Responsibilities:

* Inline or hover diagnostic popup.
* Diagnostic message layout.

Use when diagnostic popup visuals change.

### `src/render_view/settings_ui.rs`

Settings UI rendering.

Responsibilities:

* Settings window.
* Settings tabs.
* Settings controls.

Use when settings visuals or hitboxes registered during rendering change.

### `src/render_view/ui.rs`

General UI rendering.

Responsibilities:

* Dialogs.
* Welcome UI.
* General icons/buttons.
* Misc overlay UI.

Use when UI element is not in a specialized render module.

Prefer `ui_system.rs` + `app/ui_handlers.rs` for new buttons.

### `src/render_view/ui.rs`

General UI rendering module. Use specialized modules first if task clearly belongs elsewhere.

### `src/render_view/ui/hover_widget.rs`

Hover popup layout and rendering.

Responsibilities:

* LSP hover popup layout.
* Inline code styling.
* Hover text selection.
* Scroll hitbox.

Use when hover popup display/selection changes.

### `src/render_view/ui/hover_widget_tests.rs`

Hover widget geometry and animation tests. Do not delete during refactors.

### `src/render_view/ui/problems_panel.rs`

Problems panel rendering.

Responsibilities:

* Grouped diagnostics.
* Problem rows.
* Collapse state visuals.
* URL/copy hitboxes if rendered here.

Use when Problems panel visuals or row behavior changes.

### `src/highlighter.rs`

Tree-sitter highlighter backend.

Responsibilities:

* Parser setup.
* Background parse thread.
* Query execution.
* Highlight span generation.
* Autocomplete data if present.

Use when syntax parsing/highlighting logic changes.

Implementation is split through `include!`:

* `src/highlighter/highlighter_core.rs` -> types, span helpers, query helpers.
* `src/highlighter/highlighter_worker.rs` -> `Highlighter::new` worker setup and inline tests.

Highlighter unit tests also live in `src/highlighter_tests.rs`.

### `src/highlighter_runtime.rs`

Public highlighter runtime API.

Responsibilities:

* Reset/poll/wait APIs.
* Span shifting.
* Flattened span generation.
* Runtime integration with editor.

Use when main thread interaction with highlighter changes.

### `src/queries.rs`

Tree-sitter query definitions.

Use when syntax colors/captures/language injections need change.

### `src/languages/mod.rs`

Language module registry.

Use when adding language-specific support.

### `src/languages/dart.rs`

Dart-specific import-block helpers.

Use when Dart import folding behavior changes.

### `src/languages/python.rs`

Python-specific language helpers.

Responsibilities:

* Python hover signature parsing/highlighting.
* Python-specific syntax details for hover or display.
* Python import-block and docstring highlighting helpers.

Use when Python LSP hover formatting changes.

Python language helper tests live in `src/languages/python_tests.rs`.

### `src/languages/rust.rs`

Rust-specific import-block helpers.

Use when Rust import folding behavior changes.

### `src/lsp.rs`

LSP manager.

Responsibilities:

* Server lifecycle.
* Supervisor thread.
* Command/event API.
* Open/change/hover/definition/code-action commands.

Use when LSP process behavior, restart, request dispatch, or manager state changes.

Implementation is split through `include!`:

* `src/lsp/lsp_process.rs` -> managed process spawn, protocol shutdown, bounded restart supervisor, request send/log helpers, and missing-tool state.
* `src/lsp/lsp_manager.rs` -> `LspManager` facade, platform-aware workspace identity, explicit retry, diagnostics merge accessors, and JSON formatting.
* `src/lsp/ruff_workspace.rs` -> timeout-bounded managed `ruff check` workspace diagnostics parser/collector.

Tests live in `src/lsp/lsp_tests.rs`.

### `src/lsp/protocol.rs`

JSON-RPC/LSP protocol layer.

Responsibilities:

* Framing.
* Encoding/decoding.
* LSP data types.
* Server definitions.
* Response/event parsing.

Use when protocol messages, capabilities, server commands, or wire format changes.

Implementation chunks live in `src/lsp/protocol/*`: wire encoding/dispatch and value parsers.

Protocol tests live in `src/lsp/protocol_tests.rs`.

### `src/lsp/hover.rs`

Hover text normalization/highlighting.

Responsibilities:

* Markdown/plain hover normalization.
* Hover formatting.
* Hover public line-kind bridge for LSP UI.

Use when hover content is wrong after LSP response arrives.

Python hover tests live in `src/lsp/python_hover_tests.rs`.

### `src/scroll.rs`

Scroll physics.

Responsibilities:

* Position.
* Target.
* Velocity.
* Animation speed.
* Smooth updates.

Use when scrolling feels wrong across editor/panels.

### `src/ui_system.rs`

Declarative UI system.

Responsibilities:

* `UiRegistry`.
* `UiId`.
* UI element registration.
* Hover detection.
* Click lookup.
* Pointer/text capture.

Use when adding registered interactive elements or changing hit-testing behavior.

### `src/widgets.rs`

Reusable widgets.

Use when generic button/input/control visuals or behavior change.

### `src/fonts/*`

Bundled fonts.

Do not edit unless changing font assets.

### `src/icons/*`

Bundled icons.

Use when adding/removing icons or changing icon resources.

## 4. Common change routing

### New button

Likely files:

```text
src/ui_system.rs
src/render_view/ui.rs or specialized render module
src/app/ui_handlers.rs
```

### Editor keyboard shortcut

Likely files:

```text
src/app/keyboard/main_keys.rs
src/app/keyboard/editor_keys.rs
src/editor.rs
```

### Mouse click bug

Likely files:

```text
src/app/mouse/input.rs
src/ui_system.rs
src/app/ui_handlers.rs
src/render_view/*
```

### Mouse hover bug

Likely files:

```text
src/app/mouse/cursor.rs
src/render_view/ui/hover_popup.rs
src/lsp/hover.rs
```

### Scroll bug

Likely files:

```text
src/app/mouse/wheel.rs
src/scroll.rs
src/render_view/*
```

### Rendering performance bug

Likely files:

```text
src/render_view.rs
src/render_view/core_text.rs
src/renderer.rs
```

### Syntax highlight bug

Likely files:

```text
src/highlighter.rs
src/highlighter_runtime.rs
src/queries.rs
src/languages/*
```

### LSP hover bug

Likely files:

```text
src/lsp.rs
src/lsp/hover.rs
src/lsp/protocol.rs
src/app/events/source_hover.rs
src/languages/python.rs
src/render_view/ui/hover_popup.rs
```

### Terminal bug

Likely files:

```text
src/app/terminal.rs
src/render_view/terminal_ui.rs
src/app/keyboard/main_keys.rs
```

### File tree bug

Likely files:

```text
src/app/file_tree.rs
src/app/mouse/input.rs
src/render_view/ui.rs
src/app/ui_handlers.rs
```

## 5. Hot path warning list

Treat these as performance-sensitive:

```text
src/render_view.rs
src/render_view/core_text.rs
src/render_view/minimap_ui.rs
src/render_view/terminal_ui.rs
src/renderer.rs
src/app/events/about.rs
src/app/mouse/cursor.rs
src/app/mouse/wheel.rs
src/editor.rs
src/editor_navigation.rs
```

Avoid:

* disk I/O
* large allocation
* repeated string construction
* repeated full-file scans
* unnecessary clone
* blocking calls

## 6. Build and verification

Primary verification:

```bash
make codex_test
```

Use tests when behavior is test-covered:

```bash
make test
```

Regenerate `code-review-graph` after structural source changes:

```bash
code-review-graph build
```

If full postprocess is too slow, use:

```bash
code-review-graph build --skip-postprocess
```

Regenerate fallback AI map only when needed:

```bash
python3 gen_project_ai_map.py
```
