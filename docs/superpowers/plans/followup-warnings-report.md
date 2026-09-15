# Follow-up: release-build unused warnings report

Goal: zero warnings from the `rriter` crate for the non-test release build (`make fast` flags), without blanket `allow`.

Tooling note: `code-review-graph` MCP was not available in this session. References were found with exact `rg -n "\b<symbol>\b" src`, which also covers `include!` chunks and test modules.

## Per-item findings

| # | Item | References found (outside the definition) | Decision | Edit |
|---|------|-------------------------------------------|----------|------|
| 1 | import `MarkdownDocument` in `src/render_view/markdown_read.rs:7` | The only use in this file is `src/render_view/markdown_read.rs:1859` (`fn parse(...) -> MarkdownDocument` inside `#[cfg(test)] mod tests`). The include chunks (`markdown_scroll.rs`, `markdown_read_interaction.rs`, `markdown_code_scroll.rs`) don't use it. | test-only: move the import into the test module | `markdown_read.rs:7` dropped `MarkdownDocument` from the top-level `use`. `markdown_read.rs:1857` changed the test-module import to `use crate::languages::markdown::{MarkdownDocument, MarkdownParseState};`. No other lines in this file were changed by this task. |
| 2 | `DatabaseAutomationStep::is_timed_scroll` (`src/app/automation_database.rs:120`) | Only the tests at `automation_database.rs:1348-1350`, inside `#[cfg(test)] mod tests` at line 1160 | `#[cfg(test)]` | Added `#[cfg(test)]` at `automation_database.rs:120` |
| 3 | `database_query_history_preview_lines` (`database_query.rs:519`) | Only the test at `database_query.rs:2219`, inside `#[cfg(test)] mod tests` at line 1789 | `#[cfg(test)]` | Added the attribute at `database_query.rs:519` |
| 4 | `database_query_history_is_truncated` (`:523`) | Only the test at `database_query.rs:2220` | `#[cfg(test)]` | Added the attribute at `database_query.rs:524` |
| 5 | `database_query_history_entry_height` (`:527`) | Only the test at `database_query.rs:2503` | `#[cfg(test)]` | Added the attribute at `database_query.rs:529` |
| 6 | `database_query_history_entry_height_px` (`:532`) | `database_query.rs:575` (inside item 7, which is test-only) and the test at `database_query.rs:2496` | `#[cfg(test)]` | Added the attribute at `database_query.rs:535` |
| 7 | `database_query_history_content_height` (`:570`) | Only the test at `database_query.rs:2493` | `#[cfg(test)]` | Added the attribute at `database_query.rs:574` |
| 8 | `git_logs_max_scroll_from_content_height` (`src/app/git_panel/git_panel_types.rs:316`) | Only the test at `git_panel_types.rs:1545`, inside `#[cfg(test)] mod git_log_document_tests` at line 1267 | `#[cfg(test)]` | Added the attribute at `git_panel_types.rs:316` |
| 9 | `GitLogBuffer::line_at` (`src/app/git_panel/git_commit_runtime.rs:162`) | Only tests: `git_commit_runtime.rs:1023`, `:1026`, `:1334`, `:1348` (inside `#[cfg(test)] mod git_commit_runtime_tests` at line 863). Production renderers use `display_line_at` directly. | `#[cfg(test)]` | Added the attribute at `git_commit_runtime.rs:162` |
| 10 | `should_draw_empty_ide_file_tree_overlay` (`src/render_view/root_helpers.rs:550`) | Only the test at `root_helpers.rs:861-864`, inside `#[cfg(test)] mod tests` at line 567. That test runs as `render_view::tests::empty_ide_still_draws_file_tree_overlay`, because the file is included into `render_view.rs`. | `#[cfg(test)]` | Added the attribute at `root_helpers.rs:549`, above the existing `#[inline(always)]` |

Summary:
- Nothing was deleted, because every item is still used by tests.
- No item needed a target or feature cfg gate.
- Private helpers kept: `database_query_history_preview_metrics` and `database_query_history_entry_height_from_metrics` (`database_query.rs:502/508`). Production code still uses them at `database_query.rs:269-270` (history layout).
- `GitLogDisplayLineRef::line` is also still used in production (`ide_panel_git_graph_renderer.rs:507`, `:821`).
- No blanket `allow` was added, and runtime behavior is unchanged.

## cargo check (release, `make fast` flags)

Command: `CARGO_BUILD_JOBS=4 RUSTFLAGS="-C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld" cargo +nightly check --target x86_64-unknown-linux-gnu --release`

Before:
```
warning: unused import: `MarkdownDocument`                                   --> src/render_view/markdown_read.rs:7:39
warning: method `is_timed_scroll` is never used                             --> src/app/automation_database.rs:120:19
warning: function `database_query_history_preview_lines` is never used      --> src/app/database/database_query.rs:519:8
warning: function `database_query_history_is_truncated` is never used       --> src/app/database/database_query.rs:523:8
warning: function `database_query_history_entry_height` is never used       --> src/app/database/database_query.rs:527:8
warning: function `database_query_history_entry_height_px` is never used    --> src/app/database/database_query.rs:532:8
warning: function `database_query_history_content_height` is never used    --> src/app/database/database_query.rs:570:8
warning: function `git_logs_max_scroll_from_content_height` is never used   --> src/app/git_panel/git_panel_types.rs:316:15
warning: method `line_at` is never used                                     --> src/app/git_panel/git_commit_runtime.rs:162:19
warning: function `should_draw_empty_ide_file_tree_overlay` is never used   --> src/render_view/root_helpers.rs:550:4
warning: `rriter` (bin "rriter") generated 10 warnings
```

After:
```
    Checking rriter v0.1.0 (/home/reyan/projects/rriter)
    Finished `release` profile [optimized] target(s) in 8.35s
```
0 warnings, 0 errors.

## Test-build warning comparison

Command: `make test TEST_FILTER='this_filter_matches_nothing_xyz'`
- Before: 0 warnings (0 tests run, 2390 filtered out)
- After: 0 warnings, 0 errors (0 tests run, 2390 filtered out)

No new warnings.

## Focused tests

| Filter | Result |
|--------|--------|
| `database_query` | 71 passed, 0 failed |
| `git_panel` | 83 passed, 0 failed |
| `automation_database` | 7 passed, 0 failed |
| `root_helpers` | 0 matched (the file is included into `render_view.rs`, so test paths are `render_view::tests::*`). Reran with `empty_ide_still_draws_file_tree_overlay`: 1 passed. |
| `markdown_read` | 150 passed, 1 failed. The failure is `render_view::markdown_read::reader_stage1_review_v1::reviewer_reader_v1_all_block_selection_seating_uses_real_vertices` (`markdown_scroll_review_tests.rs:984`, table seating delta), which is the known failure that existed before this change. |

Per the task instructions, `make fast` and `make codex_test` were not run.
