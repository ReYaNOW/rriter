RRiter agent rules. Strict mode. Small patches. Fast UI first. Prefer the most performant, least resource-intensive solution for the code you write in the task; do not rework other code for that unless asked. In the final report, mention performance only when a real choice was made (allocation, per-frame work, I/O); no boilerplate justification.

## 0. Context and search

Read on demand, not upfront:

* `PROJECT_GUIDE.md` — architecture (§2), detailed file guide (§3), compact file index (§4). Read only the section you need.
* `docs/agents/chat-workflow.md` — only when working without file access (chat, exact-substring patch parser).
* `docs/agents/subagent-rules.md` — common rules for every subagent; a brief points to it instead of repeating them. Reason: the same ~1k-character block was copied into each brief (27.09).
* UI checks headless: `python3 scripts/rriter_headless.py shot <file>` prints the PNG path; protocol, `dump`, `bench` — `docs/headless.md`. It drives the prebuilt release binary (`target/x86_64-unknown-linux-gnu/release/rriter`, `make fast` if missing), so real UI behaviour for writing or fixing UI tests is established from `dump`/`shot` on that binary, without cargo; `make codex_test` once at the end (§4). Reason: every `make test` relinks the test binary for minutes, and an agent that re-runs it after each fix spends an hour on a quarter of the work (26.09).
* Delegating UI-test work: size each agent by scenarios, not by file — 3–5 scenarios (~10 min of driver probes) per agent; extending shared test infrastructure (fixture server, stub) is its own step before the test agents, and a scenario whose UI path is still unknown ("find what X supports") goes to a separate agent; at most 6–8 agents in parallel, the rest in waves; each agent writes its scenarios to its own file or the main session merges them into the shared `ui_tests_*.rs` afterwards (never two agents editing one file at once). Reason: one agent given all tree-ops scenarios ran three times longer than its siblings with one or two, and the whole batch waited for it (26.09); DB fixture extension + console tests + table-edit discovery in one agent took 25 min against 10 for its sibling (28.09).
* Headless tests wait for async results (git, terminal, LSP, file save) with `tests_support::wait_until(session, timeout_ms, what, |s| condition)`, not a fixed `wait N`; the timeout keeps the old pause length. A fixed `wait` is only for real delays (hover dwell, animation) and negative checks ("HEAD unchanged"), where the pause gives the async action time to land. Reason: fixed `wait 8000` in 13 git tests cost 195 s of a 608 s `make codex_test` (27.09).
* Tests run in parallel (`TEST_THREADS=8`), each in its own process (`-Z panic-abort-tests`), so a process-wide `Mutex`/lock serializes nothing between tests. Anything a test writes outside the process — state files, temp dirs, ports — must be unique per process: `cfg(test)` dirs carry `std::process::id()` (see `api_config_dir`), servers bind port 0. Reason: three fixed `/tmp/rriter_api_*` dirs kept the suite single-threaded at 11 min; per-PID dirs plus a fixed 10 s redraw loop took the tests from 646 s to 84 s at 8 threads, `make codex_test` from ~13 to ~4 min (28.09).

Search (`rg`-first):

* Default: `rg -n` for symbols, strings, error text, file names. Start narrow (exact symbol or distinctive string), widen only if empty.
* The Bash tool runs **zsh**, not bash. Bash-only syntax fails with `bad substitution` (`${var^^}`, `${!var}`, `mapfile`), and an unmatched glob aborts the whole command with `no matches found` instead of passing the pattern through. Keep commands POSIX-ish, or quote the glob and let the tool expand it.
* Read only the files/ranges the search points to. Always read exact source before editing.
* No code-graph tool: `rg` plus the compiler (`cargo check` errors) answer callers and impact. Reason: the graph went stale after every refactor and gave false `0` callers on `include!`-split files (28.09).

## 1. Role

Act as strict experienced programmer.

Priority order:

1. Smooth UI + max FPS for UI/render/input
2. Max reasonable optimization without stability loss
3. Small readable maintainable code
4. Surgical changes only

No speculative features. No broad refactors unless asked; removing the cause of the bug class you are fixing (§5 Design) is part of the fix, not a broad refactor.

## 2. Working style

* Act, don't ask: find the cause, change code, run checks. Stop only for destructive or out-of-scope actions.
* Context discipline: search first, then read relevant ranges (offset/limit for large files). Do not read whole large files or guides "just in case".
* Long command output (builds, tests): redirect to a file (`cmd > /tmp/<task>.log 2>&1`), then `grep`/`tail` it. Never dump full logs into context.
* While a long build or test run is in flight (`make codex_test`, `make test`, `make fast`, cargo builds): the main session starts it in the background and then **goes idle** (a subagent runs tests in the foreground, or its turn ends before the result). No polling the log, no peeking at partial output, no "meanwhile" side work, no thinking out loud. Every such check is a full model turn that re-ships the whole context for nothing. Wait for the completion notification, then read the result once with `grep`/`tail`.
* Feedback loop: after a substantive edit run the narrowest check (`make test TEST_FILTER=<module path>`) and fix what it reports before moving on. Exception: UI tests are probed on the prebuilt binary via `dump`/`shot` (§0), not relinked per edit.
* Fix at the root: if a shared helper is wrong, fix the helper, not one caller; check sibling code paths that use it.
* Batch independent searches/reads in one step.
* Edit files directly. Use unified diff only when showing changes; do not use chat parser `Before/After` blocks.
* Final report: short — root cause (`file:line`), change, verification result (plus the SOTA note from the header when it applies). No essays.
* Task tracking: rriter tasks go to the Kanri board, never Linear (Linear is Skyhold-only). Use the `kanri-task` skill, or `kanri-add "Title" [--desc ...] [--board RRiter] [--column Backlog|Features|Bugs|Doing|Done] [--top] [--list]`, `kanri-add --move <id> --column <col>` — it writes straight into Kanri's JSON store `~/.local/share/tech.trobonox.kanri/.kanri.dat`, keeps a backup, and refuses to write while Kanri is running (Kanri would overwrite the file on its own save). Move cards yourself when their status changes: work started → Doing, fixed and verified (`make codex_test` green, ignored test lifted) → Done, not a bug → say so and move per the user's call; mention each move in the report. Exception: large features (Features column, multi-step work) — propose the move instead of doing it. Reason: the user had to move cards by hand after every task (26.09).

## 3. Allowed Ops

Allowed file ops inside project:

* Read files
* Search files
* Edit files
* Create only `.rs`, `.py`, `.dart`, `.md`, `.txt`, and the single manifest `pdfium.json` at the repo root (named exception, 30.09)
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
* `python3 scripts/fetch_pdfium.py`
* `make pdfium`
* Read-only inspection commands that stay in project root
* Subagent finds a bug outside its task (other function/module, not blocking the task): do not fix it — add a `#[ignore = "bug: …"]` test if cheap, and put cause (`file:line`) and repro in the report; the main session decides and dispatches the fix. Fix in place only when the bug sits in the code you are already changing or blocks your task. Reason: side fixes land unreviewed, collide with parallel agents' files, and stretch the task toward its turn limit.
* Branches, commits, PRs — only the main session; subagents never commit. Decide yourself, and don't shy away from branches:
  * Small change (one-line fix, doc/rule tweak, a test or two, no behaviour change worth reviewing) → commit straight to `master` and push; several small changes of one session go in one push after one `make codex_test`.
  * Substantial change (feature, bug fix with real logic, refactor, multi-file work, iteration of a larger plan) → branch off fresh `master` (`git switch master && git pull && git switch -c <short-name>`), commit and push it (`-u origin <branch>`); at the end of the verified task `gh pr create --base master` (body ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`), `gh pr merge --squash --delete-branch`, `git switch master && git pull`. Follow-up work on the current branch stays on it.
  * Either way, only after the task is finished and verified (`make codex_test` green) — no WIP or red tests on `master`, don't merge a branch the user asked to keep open, and say in the final report what was pushed or which PR was merged. Reason: the user had to ask for push and merge after every task (26.09, 27.09).
  * Push, `gh pr create` and `gh pr merge` — each its own Bash call (PR body via `--body-file`), never chained with `&&`. Reason: the permission check denied the chained push+PR+merge, separate calls passed (28.09).

Forbidden unless user explicitly asks:

* force-push, deleting remote branches other than the merged PR branch, rewriting pushed history
* network commands (only mutable)
* package installs
* destructive commands outside project
* whole-project formatting

## 4. Plan + Verify

### Builds and tests with several agents

Only one agent builds or tests at a time, in the main checkout where `target/` is warm. Parallel agents only edit; the integrator builds and runs tests once after integration. A fresh worktree starts with an empty `target/`: the first build takes tens of minutes and gigabytes, and concurrent Cargo processes fight for CPU and for one target directory.

Exception: a subagent in its own worktree may build and test in parallel only when the main session explicitly allowed it in the brief and had it warm the worktree's target first with a btrfs reflink copy (`cp -a --reflink=always <main>/target/x86_64-unknown-linux-gnu <main>/target/debug target/`) — instant, no extra disk; dependencies stay cached and only the `rriter` crate rebuilds once (~1.5 min). Reason: a reflinked worktree agent built in 1m19s, then 25–60 s per run, alongside a builder in the main checkout (29.09).

If you are in SuperPowers workflow, you can run tests how you like, you can ignore later required make codex_test 

Bug fix path: `rg` -> read exact source -> find root cause -> minimal patch -> focused test -> `make codex_test` at the end.

Primary success check after edits:

```bash
make codex_test
```

`make codex_test` starts with `scripts/lint_changed.py`: clippy and pattern checks on files changed vs master, against `scripts/lint_baseline.json`. A warning in code you touched gets a real fix, or — when the fix is larger than the task — stays in the baseline plus a `design debt:` Kanri card. Never add `#[allow]` or split a function just to satisfy a lint; never regenerate the baseline to hide new warnings (`make lint-baseline` only when a stage removes warnings). Reason: the gate exists to stop the code getting worse one small fix at a time (27.09).

Run `make codex_test` once per branch/PR, before the commit that goes to `master`, whenever anything that builds or tests changed (`.rs`, `Cargo.*`, `Makefile`, `clippy.toml`, shaders/assets, scripts the tests call). Docs-only changes (`*.md`, `docs/`, `PROJECT_AI_MAP.txt`) skip it and go straight to `master`. Between edits run only focused modules. Size PRs by logical unit (a refactor stage, a feature, a batch of related bugs), not per sub-step: every PR costs one full run. Reason: a full run per tiny PR made the user wait 15 min each time (28.09).

Run it in the background and stay silent until it reports. Before starting it, launch any independent read-only work (review of the next diff, scouting the next stage) — while it runs nobody edits the tree, runs cargo, or drives `rriter_headless.py`: edits would land in the release binary it builds at the end and in the commit untested, cargo waits on the target lock, and the driver's binary is replaced mid-run. Findings that arrive meanwhile are applied after the run. Checking on it costs a full model turn each time and tells you nothing the completion notification will not.

Do not run `make fast` on its own mid-task unless the user explicitly asks; it is rebuilt by `make codex_test` at the end, and once if the release binary is missing (§0).

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

### Design

Small diffs must not repeat the defect's shape: a one-line fix that adds another copy of the forgotten call leaves the next bug armed.

* Fields that must change together are changed by one owning method; do not add or call methods that update only part of them. Reason: `close_dialog()` hid the dialog but kept `pending_action`, so Escape re-armed it (27.09).
* An effect every mutation needs (persist, push to a running server, invalidate a cache, redraw) lives in one commit point, not repeated per `match` arm. Reason: four of five API Mock commit arms forgot to refresh the running server (27.09).
* Bug of the kind "forgot to call X / reset Y": fix it so it cannot be forgotten when that stays inside the function or module you touch; if it needs more, fix locally and add a Kanri card titled `design debt: …` (Backlog) and name it in the report.
* New feature logic goes into the feature's own state type (`ApiClientState`, `DatabasePanel`, …) with its own methods; `impl App` only routes events and runs cross-feature effects. Do not add `impl App` methods to `*_click_methods.rs`/`*_text_methods.rs`-style files that group by action kind.
* No new global mutable state (`static` + `Mutex`/`OnceLock` holding app data); pass it or own it in a struct. Existing globals stay until a card moves them. Reason: they force process-wide test locks and make scenarios untestable without flakes.
* Entities are identified by their natural key where one exists (source URL, path via `PathKey`), not only by an allocated id. Reason: re-importing one OpenAPI URL created duplicate specs (27.09).

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

* Hard limit: no source file above 1600 lines. When you add code to a file already over it, move the behavior unit you touch into its own file in the same change instead of growing the file; if that is larger than the task, add a `design debt:` Kanri card. Add new files to the compact file index in `PROJECT_GUIDE.md` §4. Reason: 55 files passed 1500 lines one small fix at a time (27.09).

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

1. Find callers of the edited function (`rg`).
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
