# Subagent rules (rriter)

Read this before starting. The brief gives the task, files, entry points, stop condition and report; everything common lives here.

## Workspace

* Main checkout, shared with other agents. Files of other agents appearing in the tree are normal; do not touch or investigate them.
* Edit only the files the brief names. Source edits through Edit/Write/apply_patch, never `sed -i` or shell redirection onto a source file; new log files are fine.
* No `rustfmt` / `cargo fmt` (the tree is not rustfmt-clean; it drags dozens of files into the diff). Match surrounding style by hand.
* No commits, branches, pushes.

## Builds and runs

* Build or test only when the brief allows it, and only up to the number of runs it gives.
* Long output to a new log file (`cmd > /tmp/<task>.log 2>&1`), then `tail -n 40` / `grep`. Never print full logs.
* Tests in the foreground only: a background run ends your turn without a result.
* Focused tests: `make test TEST_FILTER=<module path>` (e.g. `headless::ui_tests_api_mock`). The prefix `headless::ui_tests_` alone runs ~230 tests — never use it.
* In a sandbox (Codex), run GPU/EGL commands with a sandbox-escape request on the first attempt, never try them inside first: `scripts/rriter_headless.py` (any subcommand) and any `make test`/`cargo test` whose filter matches a `headless::` test (substring match: `TEST_FILTER=theme` matches `headless::ui_tests_themes`). `EGL setup failed` or `Read-only file system` under /run/user is the environment, not a product bug. Reason: each in-sandbox attempt costs a full link and a test run for nothing (05.10).

## Headless UI tests

* Establish real UI behaviour first on the prebuilt release binary: `python3 scripts/rriter_headless.py run|repl|shot|dump` (protocol: docs/headless.md), no cargo. Assert only what you saw.
* `wheel 0 -N` scrolls DOWN. Reach state through UI paths; seed data directly only where no UI path exists (native dialogs, missing controls), then render a frame (`mouse_move 0 0`) and say so in the report.
* Wait for async results with `tests_support::wait_until(session, timeout_ms, what, |s| cond)`, not a fixed `wait`.
* Reuse `tests_support` helpers (src/headless/tests.rs); never copy a helper from another test file — report it as a candidate to move instead.

## Bugs outside the task

Do not fix them. Add a `#[ignore = "bug: …"]` test if cheap and put cause (`file:line`) and repro in the report.

## Report

Up to 25 lines: what changed (`file:line`), result of every run (passed/failed/ignored), what was seeded directly and why, bugs found, what you could not verify.
