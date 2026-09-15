# Follow-up F1 fix report: code-block scrollbar drag ends mid-drag

Status: DONE

## 1. Diagnostics removal

Pre-diagnostic copies were saved to the scratchpad, then each file was rewritten with `grep -v 'TEMP F1 DIAG'`.

| File | Before | After | `TEMP F1 DIAG` lines |
|---|---|---|---|
| src/render_view/markdown_code_scroll.rs | 1157 | 1094 | 63 removed |
| src/app/mouse/cursor.rs | 1921 | 1915 | 6 removed |

- **cursor.rs:** `diff` shows 6 deletions and 0 additions.
- **markdown_code_scroll.rs:** every deleted line carries the marker. `diff` also reports one extra `}` pair, but that is only diff re-aligning a closing brace; the files are identical to `grep -v` output by construction.
- **Wrapped expressions still valid:** `begin_markdown_code_scrollbar_drag_at` and `drag_markdown_code_scrollbar_to` were wrapped as `let f1_result = … ; f1_result`. With the marker lines gone, each returns its original expression directly as the tail.
- **Compiles:** the test build and the release `cargo check` both compile.
- **Grep is clean:** `grep -rn "TEMP F1 DIAG\|RRITER_DEBUG_CODE_SCROLL\|f1_diag" src` returns nothing (exit 1).

## 2. Code change

- **src/app/app_ide_tab_methods.rs, `App::sync_active_tab`:** removed `self.markdown.end_code_scroll_drag();`. It sat just before `std::mem::swap(&mut self.markdown, &mut self.tabs[ai].markdown);`, which is now line 660.
- **No other production changes.** Drags are still ended by:
  - `set_markdown_mode` (src/app/markdown.rs)
  - left release (src/app/mouse/input.rs)
  - `cancel_pointer_interactions` (src/app/mouse/input.rs:337), which `switch_to_tab` calls before it swaps tabs.

## 3. Tests (src/render_view/markdown_code_scroll.rs, `markdown_code_scroll_gl_tests`)

- **Added `external_changes_check_keeps_code_scrollbar_drag` (line 1097).**
  - Setup: IDE-mode headless App with one tab and a held drag on block 7.
  - Calls the real `app.start_external_changes_check()`. That runs the swap-out/swap-in pair; the tab has no path, so no worker thread is spawned.
  - Asserts `app.markdown.code_scroll_drag == Some(7)` and that the entry for block 7 `is_dragging`.
- **Replaced `tab_swap_ends_code_scrollbar_drag` with `tab_switch_ends_code_scrollbar_drag` (line 1083).**
  - Setup: two tabs, active tab 1, held drag.
  - Calls `app.switch_to_tab(0)`.
  - Asserts the drag ended on `app.tabs[1].markdown` (the outgoing tab's stored state) and is not active on `app.markdown`.
- **Kept `markdown_mode_switch_ends_code_scrollbar_drag` (line 1072)** unchanged.

### RED (diagnostics removed, tests written, `sync_active_tab` still ending the drag)

Command: `make test TEST_FILTER='markdown_code_scroll'`, exit 2.

```
test render_view::markdown_read::markdown_code_scroll_gl_tests::external_changes_check_keeps_code_scrollbar_drag ...
thread 'main' panicked at src/render_view/markdown_code_scroll.rs:1106:9:
assertion `left == right` failed
  left: None
 right: Some(7)
test result: FAILED. 13 passed; 1 failed; 0 ignored; 0 measured; 2376 filtered out
```

This failure is the expected one. The first `sync_active_tab()` in `start_external_changes_check` called `end_code_scroll_drag()` on the active markdown state, so `code_scroll_drag` became `None`. That matches the backtrace recorded in the real app.

`tab_switch_ends_code_scrollbar_drag` already passed at RED, because `switch_to_tab` calls `cancel_pointer_interactions()`.

### GREEN (after the change)

Command: `make test TEST_FILTER='markdown_code_scroll'`, exit 0.

```
test ...markdown_code_scroll_gl_tests::external_changes_check_keeps_code_scrollbar_drag ... ok
test ...markdown_code_scroll_gl_tests::markdown_mode_switch_ends_code_scrollbar_drag ... ok
test ...markdown_code_scroll_gl_tests::tab_switch_ends_code_scrollbar_drag ... ok
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 2376 filtered out
```

## 4. Related test suites (`make test TEST_FILTER=…`)

| Filter | Result |
|---|---|
| `app::mouse` | ok, 143 passed, 0 failed |
| `app::markdown` | ok, 42 passed, 0 failed |
| `app::events` | ok, 65 passed, 0 failed |
| `app_ide_tab` | ok, but matched 0 tests (no test path contains this substring) |
| `app_file_behavior_tests` | ok, 107 passed, 0 failed |

`app_file_behavior_tests` was run because `app_ide_tab` matched nothing. It covers the tab-flow and `switch_to_tab` tests in app_file_tab_flow_tests, app_file_ide_definition_tests and app_file_ui_git_api_tests.

## 5. cargo check and grep

`CARGO_BUILD_JOBS=4 RUSTFLAGS="-C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld" cargo +nightly check --target x86_64-unknown-linux-gnu --release` finished successfully with 10 warnings.

That is the same count as the earlier scratchpad check log, and none are in the changed files:

- markdown_read.rs:7 unused `MarkdownDocument` (already in the baseline log)
- automation_database.rs
- database_query.rs (5)
- git_panel_types.rs
- git_commit_runtime.rs
- root_helpers.rs

No new warnings. `grep -rn "TEMP F1 DIAG\|RRITER_DEBUG_CODE_SCROLL\|f1_diag" src` returns nothing.

## 6. Concerns

- **Test is headless, not a live repro.** The regression test calls the real `start_external_changes_check`, but with a single path-less tab, so no worker thread is spawned. It exercises exactly the `sync_active_tab` swap pair from the recorded backtrace, but the drag behaviour has not been re-checked in the running app.
- **Other `sync_active_tab` callers now keep the drag.** This is intended, since tab switching goes through `cancel_pointer_interactions`. Any future path that changes `active_tab` without `switch_to_tab` or `cancel_pointer_interactions` would carry a held drag in the outgoing tab's stored state.
  - `open_new_tab` is one such path.
  - Its risk is low: the next left release ends the drag on the new active markdown state, and `retain_active_code_scroll_x` keeps the stored state bounded.
