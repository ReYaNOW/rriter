# Follow-up F1: code-block scrollbar drag diagnostics — report (TEMPORARY)

Status: DONE. Instrumentation only. The code behaves the same with or without the env var.

## Files changed
- `src/render_view/markdown_code_scroll.rs` (63 lines marked `// TEMP F1 DIAG`)
- `src/app/mouse/cursor.rs` (6 lines marked `// TEMP F1 DIAG`)

Every added line, including separator comment lines, contains `TEMP F1 DIAG`. To remove the instrumentation, delete every line that contains it:

```sh
for f in src/render_view/markdown_code_scroll.rs src/app/mouse/cursor.rs; do grep -v 'TEMP F1 DIAG' "$f" > "$f.tmp" && mv "$f.tmp" "$f"; done
```

This was checked: for both files, `grep -v 'TEMP F1 DIAG'` output is byte-identical to the pre-change copy. Where a tail expression had to be captured, the capture pieces sit on their own marked lines (`let f1_result = // TEMP F1 DIAG`, `; // TEMP F1 DIAG`, `f1_result // TEMP F1 DIAG`), so deleting them restores the original tail expression.

## Gating and cost
- `f1_diag_enabled()` (markdown_code_scroll.rs:10) reads `RRITER_DEBUG_CODE_SCROLL` once into a `OnceLock<bool>`. The value counts as enabled when it is set, not empty, and not `"0"`. When disabled, each log point costs one atomic load plus a branch: no formatting, no allocation, and no `Instant::now`.
- `f1_diag_log()` (:14) writes one `eprintln!` line, `[F1 +<ms>ms] ...`. The timer starts at the first logged event.
- Local `f1_diag!` macro (:19) checks the flag, then formats with `format_args!`.
- `F1State` (:22) is a Display wrapper that prints `dragging= offset= target= current=` for a block's `ScrollState` without creating an entry. Its reader is `MarkdownTabState::f1_diag_state` (:390).
- `App::f1_diag_geom_miss` (:604) says why the geometry lookup returned None: `mode`, `tab_mode`, `body` rect, `renderer_scale`, `layout_ver` vs `editor_ver`, `block_found`. It is only called when enabled.
- When enabled, `drag_markdown_code_scrollbar_to` computes the geometry one extra time for logging. When disabled it does not.

## Log points

| # | Location | Event | Fires |
|---|----------|-------|-------|
| 1a | markdown_code_scroll.rs:591 | `begin ... geom=None` | `App::begin_markdown_code_scrollbar_drag_at` when the geometry lookup fails |
| 1b | markdown_code_scroll.rs:600 | `begin ... geom=...` | the same method after `begin_code_scroll_drag` returns |
| 2a | markdown_code_scroll.rs:612 | `drag ... drag=None` | `App::drag_markdown_code_scrollbar_to` when no drag is active |
| 2b | markdown_code_scroll.rs:625 | `drag block=...` | the same method after each drag update |
| 3 | markdown_code_scroll.rs:485 | `drag_target_none` | `MarkdownTabState::drag_code_scroll_to` when the target is None, just before it ends the drag |
| 4 | markdown_code_scroll.rs:398 | `end_drag` + backtrace | `MarkdownTabState::end_code_scroll_drag` only when it ends an active drag |
| 5a | cursor.rs:197 | `move_top` | first thing in `handle_main_cursor_moved` (after px/py) while `code_scroll_drag` is Some |
| 5b | cursor.rs:748 | `reached code-drag branch` | right before the Reader code-drag branch while `code_scroll_drag` is Some |
| 6 | markdown_code_scroll.rs:383 | `tick` | `MarkdownTabState::update_code_scroll_x` while a drag is active, at most once per 100 ms (`thread_local!` `Instant`) |

### Sample output
Lines 1b, 2b and 4 below are real output from `RRITER_DEBUG_CODE_SCROLL=1 make test TEST_FILTER='code_scrollbar_thumb_drag_is_smooth_and_release_ends_drag'`. The other lines show the exact format strings with example values, because the headless fixture cannot reach the cursor path or trigger those failures.

```
[F1 +0ms] begin block=7 pointer_x=55.363636 geom=track_x=40 track_w=520 view_w=520 content_w=4400 max_scroll=3880 thumb=Some(ScrollbarThumb { start: 40.0, len: 61.454544 }) result=true drag=Some(7) dragging=true offset=15.363636 target=0 current=0
[F1 +0ms] drag block=7 pointer_x=560 geom(track_x,track_w,max_scroll)=Some((40.0, 520.0, 3880.0)) miss=None result=true drag=Some(7) dragging=true offset=15.363636 target=3880 current=0
[F1 +0ms] end_drag block=7 before: dragging=true offset=15.363636 target=3880 current=0
   0: std::backtrace_rs::backtrace::libunwind::trace
   ...
   3: <rriter::app::markdown::MarkdownTabState>::end_code_scroll_drag
             at ./src/render_view/markdown_code_scroll.rs:398:101
   4: <rriter::app::app_state::App>::handle_main_mouse_input_inner
             at ./src/app/mouse/input.rs:503:75
   ...
```

Format-only examples:
```
[F1 +12ms] begin block=7 pointer_x=55 geom=None mode=Read tab_mode=Read body=None renderer_scale=Some(1.0) layout_ver=Some(3) editor_ver=3 block_found=true result=false
[F1 +20ms] move_top px=80 py=412 drag=Some(7) mode=Read
[F1 +20ms] reached code-drag branch px=80 py=412 drag=Some(7) mode=Read
[F1 +20ms] drag block=7 pointer_x=80 geom(track_x,track_w,max_scroll)=None miss=Some("mode=Read tab_mode=Read body=Some((0.0, 40.0, 600.0, 700.0)) renderer_scale=Some(1.0) layout_ver=Some(2) editor_ver=3 block_found=true") result=true drag=None dragging=- offset=- target=- current=- (no entry)
[F1 +20ms] drag_target_none block=7 why=not_dragging|thumb_none|scrollbar_drag_target_none pointer_x=80 track_x=40 track_w=520 view_w=520 content_w=4400 max_scroll=3880 offset=0 drag_offset=15.36 thumb=Some(ScrollbarThumb { start: 40.0, len: 61.45 }) current=0 target=0
[F1 +36ms] tick block=7 dt=0.016 changed=true dragging=true offset=15.36 target=240 current=96.5
```

### How to read the output
- `begin result=true` followed by `move_top` but no `reached code-drag branch`: an early return in `handle_main_cursor_moved` is swallowing the moves.
- No `move_top` lines at all during the drag: `code_scroll_drag` was already cleared. The preceding `end_drag` backtrace shows the caller.
- `drag ... geom=None miss=...`: the drag was ended because the geometry lookup failed. `miss` gives the reason, for example a stale `layout_ver` or no body rect.
- `drag_target_none why=...`: tells which condition inside `drag_code_scroll_to` failed.
- `tick` lines where `target` changes but `current` never follows would point at animation or clamping. If both move, look at draw/offset projection instead.

## How the user runs it
The Makefile has no env-aware run target. Do what `make run` does (`make fast`, then launch the binary) with the variable set and stderr sent to a file. This works in fish and bash:

```sh
cd /home/reyan/projects/rriter
make fast
env RRITER_DEBUG_CODE_SCROLL=1 RUST_BACKTRACE=full target/x86_64-unknown-linux-gnu/release/rriter 2> ~/rriter-f1.log
# in another terminal, optional:
tail -f ~/rriter-f1.log | grep --line-buffered '^\[F1\|^ *[0-9]*: .*rriter'
```

Next, open a Markdown file in Reader mode, drag an overflowing code block's horizontal thumb a few times, and quit. Send `~/rriter-f1.log`. `make fast` builds with `DEBUG=2`, so the backtraces include file:line.

## Verification (run by agent)
- `make test TEST_FILTER='markdown_code_scroll'`: exit 0, `test result: ok. 13 passed; 0 failed`.
- `make test TEST_FILTER='app::mouse'`: exit 0, `test result: ok. 143 passed; 0 failed`.
- `RRITER_DEBUG_CODE_SCROLL=1 make test TEST_FILTER='code_scrollbar_thumb_drag_is_smooth_and_release_ends_drag'`: exit 0, 1 passed, and the output includes the sample lines above.
- `CARGO_BUILD_JOBS=4 RUSTFLAGS="-C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld" cargo +nightly check --target x86_64-unknown-linux-gnu --release`: exit 0, `Finished release`. There are 10 warnings, all already present and none in `markdown_code_scroll.rs` or `mouse/cursor.rs` (grep count 0).
- Removal check: for both files, `grep -v 'TEMP F1 DIAG'` output is identical to the pre-change copies.
- `make fast` and `make codex_test` were not run, as the task required.
