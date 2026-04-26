Architecture guide for RRiter.

Use this file only when broader architecture context is needed. For file selection, prefer `PROJECT_AI_MAP.txt`.

## 1. Product philosophy

RRiter is a lightweight editor built against heavy editor trends: no Electron, no JVM, no heavyweight retained UI framework.

Main principles:

1. Immediate-mode UI.
2. GPU-centric rendering.
3. Async heavy work.
4. Explicit memory control.
5. Minimal abstractions.
6. Smooth input/render path first.

## 2. Core architecture

### Text engine

File:

```text
src/editor.rs
src/editor_navigation.rs
````

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
src/app/events.rs
src/app/events/*
src/app/keyboard.rs
src/app/keyboard/*
src/app/mouse.rs
src/app/mouse/*
```

Input is split by behavior:

* Window events and frame tick.
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

* Supervisor owns server process.
* Reader/writer threads handle JSON-RPC I/O.
* Main thread receives `LspEvent`.
* Hover/diagnostic text is normalized and highlighted separately.
* Python-specific formatting lives under `languages/python.rs`.

### Terminal

Files:

```text
src/app/terminal.rs
src/render_view/terminal_ui.rs
```

Integrated terminal uses PTY and Alacritty terminal grid.

Important ideas:

* PTY process runs outside UI.
* I/O thread reads terminal output.
* Grid is shared with renderer.
* Render path reads visible cells.
* Terminal focused state captures keyboard input.

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

Build-time Rust script. Usually handles compile-time resource or platform setup.

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
make fast
```

Use `make test` when tests matter.

#### `gen_project_ai_map.py`

Generates `PROJECT_AI_MAP.txt`.

Use after source layout changes:

```bash
python3 gen_project_ai_map.py
```

#### `PROJECT_AI_MAP.txt`

Compact AI navigation map.

Used to select source files and understand call edges.

Not source code.

#### `AGENTS.md`

Main coding-agent rules.

#### `PROJECT_GUIDE.md`

This architecture guide.

### `src/main.rs`

Application entrypoint.

Likely owns startup wiring, event loop creation, renderer/app initialization, and top-level run logic.

Use when task touches startup, window creation, GL context setup, or global app boot.

### `src/app.rs`

Main `App` state.

Likely owns editor state, panels, UI state, LSP state, terminal state, file tree state, hover state, settings, dialogs, and input-related fields.

Use when change touches shared application state.

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

### `src/app/file_icons.rs`

File icon mapping.

Use when icon choice by extension/path changes.

### `src/app/lsp_actions.rs`

User-facing LSP actions.

Use when changing code actions, go-to-definition, diagnostics actions, or LSP-triggered editor commands.

### `src/app/terminal.rs`

Terminal backend.

Responsibilities:

* PTY spawn.
* Shell I/O.
* Terminal grid.
* Keyboard bytes to PTY.
* Dirty/redraw signaling.

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

Hot path.

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

### `src/render_view/ui/hover_popup.rs`

Hover popup layout and rendering.

Responsibilities:

* LSP hover popup layout.
* Inline code styling.
* Hover text selection.
* Scroll hitbox.

Use when hover popup display/selection changes.

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

### `src/languages/python.rs`

Python-specific language helpers.

Responsibilities:

* Python hover signature parsing/highlighting.
* Python-specific syntax details for hover or display.

Use when Python LSP hover formatting changes.

### `src/lsp.rs`

LSP manager.

Responsibilities:

* Server lifecycle.
* Supervisor thread.
* Command/event API.
* Open/change/hover/definition/code-action commands.

Use when LSP process behavior, restart, request dispatch, or manager state changes.

### `src/lsp/protocol.rs`

JSON-RPC/LSP protocol layer.

Responsibilities:

* Framing.
* Encoding/decoding.
* LSP data types.
* Server definitions.
* Response/event parsing.

Use when protocol messages, capabilities, server commands, or wire format changes.

### `src/lsp/hover.rs`

Hover text normalization/highlighting.

Responsibilities:

* Markdown/plain hover normalization.
* Hover formatting.
* Hover tests.

Use when hover content is wrong after LSP response arrives.

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
make fast
```

Use tests when behavior is test-covered:

```bash
make test
```

Regenerate AI map after structural source changes:

```bash
python3 gen_project_ai_map.py
```
