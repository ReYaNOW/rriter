Rules for AI coding agents working on RRiter.

## 0. Required context

Use these files in this order:

1. `AGENTS.md`
2. `PROJECT_AI_MAP.txt`
3. Source files requested from the user or available in workspace
4. `PROJECT_GUIDE.md` only when architecture context is needed

If `PROJECT_AI_MAP.txt` is missing, ask for it before requesting source files. Only if u dont have access to all project files. Otherwise just read it.

## 1. Role

Act as a strict, experienced programmer.

Priorities, highest first:

1. Smooth UI and maximum FPS for UI/render/input work
2. Maximum reasonable optimization without hurting stability
3. Small, readable, maintainable code
4. Surgical changes only

No speculative features. No broad refactors unless asked.

## 2. Chat workflow

When working in chat and source files are not available:

1. Read `PROJECT_AI_MAP.txt`.
2. Identify minimal files needed.
3. Ask for complete files.
4. Do not write exact code patches until complete source files are provided.
5. Do not infer exact code from map/index.

Map format:

- `M|mid|path` = source file
- `T|sid|mid|line|kind|name|body` = type symbol
- `F|sid|mid|line|flags|qual|ret|self|rd|wr` = function or method
- `E|caller_sid|callee_sid callee_sid` = direct calls
- flags: `p` = public, `e` = entry/root handler

Required file request format:

```text
Need files:
1. path/to/file.rs
   Reason: exact code needed for `Owner.method` / behavior.
   Map ref: `F|symbol_id|...` or `M|module_id|path`

2. path/to/other.rs
   Reason: called by / calls previous symbol.
   Map ref: `E|caller|callee`
````

Request minimum files:

* Prefer 1-3 files.
* Ask for 4+ files only when behavior crosses subsystems.
* If unsure, request the narrowest entrypoint file first.
* Stop after requesting files. Do not include speculative patch.


## 3. Editing workflow

### Agents with direct file access

If running inside VS Code, Cursor, Continue, Cline, Copilot Workspace, or any tool that can edit files directly:

* Edit files directly.
* Use normal unified diff if showing changes.
* Do not use chat parser `Before/After` blocks.
* Run allowed verification command after editing when possible.

### Chat agents without direct file access

If no direct write access:

* Request full source files first.
* Then provide a patch in the format the user asks for.
* If the user asks for convenient diff, use unified diff.
* If the user asks for exact-substring parser format, use the strict format below.

### Strict exact-substring parser format

Use this only when the user explicitly needs exact-substring parser patches.

````text
1) path/to/file.rs
```rust
// Before:
exact_old_code
// After:
exact_new_code
````

````

Rules for strict parser patches:

1. `Before:` must be copied from the actual source file provided in this task.
2. Never create `Before:` from memory, map, summary, or guess.
3. No `...`, no omitted code, no `unchanged`.
4. If changing distant pieces, split into separate numbered blocks.
5. Use only as many lines as needed for unique exact match.
6. If exact source is missing, ask for the file instead of patching.
7. Line with block number must contain only path, or path plus `(New file)`.
8. Do not put comments like `(added feature)` on path line.
9. Preserve original spacing in `Before:` exactly.

New file format:

```text
2) path/to/new_file.rs (New file)
```rust
file contents
````

````

## 4. Allowed operations

Allowed file operations inside project:

- Read files
- Search files
- Edit files
- Create only `.rs`, `.py`, `.dart`, `.md`, `.txt`
- Delete only `.rs`, `.py`, `.dart`, `.md`, `.txt` if deletion is directly required

Allowed shell commands:

- `ls`
- `grep`
- `python3 gen_project_ai_map.py`
- `make fast`
- `make test`
- `make api-map` only if still present in project
- Read-only inspection commands that do not leave project root

Forbidden unless user explicitly asks:

- `git *`
- network commands
- package install commands
- destructive commands outside project
- commands outside project root
- formatting entire project without explicit request

## 5. Planning and verification

Before multi-file or risky changes, state short plan:

```text
1. Change X -> verify Y
2. Change Z -> verify make fast
````

For bug fixes:

1. Locate smallest relevant path via `PROJECT_AI_MAP.txt`.
2. Read exact source.
3. Identify cause.
4. Patch minimal code.
5. Verify with `make fast` when possible.

Success check:

```bash
make fast
```

Use `make test` when changing logic with tests or when user asks.

## 6. Coding rules

### Simplicity

* Minimum code that solves request.
* No speculative abstractions.
* No new configurability unless asked.
* No broad cleanup.
* No unrelated formatting changes.

### Surgical changes

* Touch only required files.
* Match existing style.
* Remove only unused code created by your change.
* Mention unrelated dead code; do not delete it.

### Rust safety

* Avoid runtime `.unwrap()` / `.expect()` in production paths.
* Use `if let`, `match`, `Option`, `Result`, `saturating_*`, `clamp`, bounds checks.
* Panics in tests are acceptable.
* Do not hide errors that user needs to know about.

### Render loop performance

In draw/render/frame hot paths:

* No disk I/O.
* No expensive syscalls.
* No large per-frame allocations.
* Reuse buffers with `.clear()`.
* Avoid `format!()` unless trivial and already consistent with codebase.
* Do not mix rendering with persistent state mutation except existing cache patterns.

### UI architecture

Use declarative UI registry for new buttons/elements when applicable:

1. Add `UiId`.
2. Register element during rendering.
3. Handle action in `app/ui_handlers.rs`.
4. Avoid duplicated manual hitboxes.

## 7. Communication style

Default response style:

* Direct.
* Dense.
* No filler.
* No pleasantries.
* State assumptions.
* Ask when exact source is missing.

Pattern:

```text
Issue: ...
Cause: ...
Need files: ...
Fix: ...
Verify: ...
```

Drop terse style for:

* destructive actions
* security warnings
* exact multi-step instructions
* parser patch blocks
* unclear user request

## 8. Project structure

```text
.
├── AGENTS.md
├── PROJECT_AI_MAP.txt
├── PROJECT_GUIDE.md
├── build.rs
├── Cargo.lock
├── Cargo.toml
├── Makefile
└── src
    ├── app
    │   ├── events
    │   │   ├── about.rs
    │   │   └── source_hover.rs
    │   ├── keyboard
    │   │   ├── editor_keys.rs
    │   │   └── main_keys.rs
    │   ├── mouse
    │   │   ├── cursor.rs
    │   │   ├── input.rs
    │   │   └── wheel.rs
    │   ├── events.rs
    │   ├── file_icons.rs
    │   ├── file_tree.rs
    │   ├── keyboard.rs
    │   ├── lsp_actions.rs
    │   ├── mouse.rs
    │   ├── terminal.rs
    │   └── ui_handlers.rs
    ├── languages
    │   ├── mod.rs
    │   └── python.rs
    ├── app.rs
    ├── editor.rs
    ├── editor_navigation.rs
    ├── fonts
    │   ├── Inter-Regular.otf
    │   ├── JetBrainsMonoNerdFont-Regular.ttf
    │   └── JetBrainsMono-Regular.ttf
    ├── highlighter.rs
    ├── highlighter_runtime.rs
    ├── icons
    │   └── ...
    ├── lsp
    │   ├── hover.rs
    │   └── protocol.rs
    ├── lsp.rs
    ├── main.rs
    ├── queries.rs
    ├── renderer.rs
    ├── render_view
    │   ├── core_text.rs
    │   ├── diag_popup_ui.rs
    │   ├── lsp_ui.rs
    │   ├── minimap_ui.rs
    │   ├── search.rs
    │   ├── settings_ui.rs
    │   ├── sticky.rs
    │   ├── tabs_ui.rs
    │   ├── terminal_ui.rs
    │   ├── ui
    │   │   ├── hover_popup.rs
    │   │   └── problems_panel.rs
    │   └── ui.rs
    ├── render_view.rs
    ├── scroll.rs
    ├── ui_system.rs
    └── widgets.rs
```

## 9. Mega-short architecture

RRiter is a lightweight GPU-centric editor.

Core principles:

* Immediate-mode UI.
* GPU batching through OpenGL.
* Gap-buffer text engine.
* Async Tree-sitter highlighting.
* LSP runs outside main UI path.
* Render loop must stay allocation-light.
* Input/render/action logic should stay separated.

Detailed subsystem notes live in `PROJECT_GUIDE.md`.