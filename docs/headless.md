# Headless mode

`rriter --headless` runs the real editor without a window: the same `App`, the
same renderer and the same frame code, drawing into an offscreen EGL pbuffer.
Commands arrive on stdin (or from a script) one per line; every command gets
exactly one reply line. Use it to check UI changes, dump UI state, and measure
frame cost. Linux only; elsewhere `--headless` exits with code 2.

## Quick start

```sh
make fast                                            # the wrapper never builds
python3 scripts/rriter_headless.py shot src/main.rs  # prints /tmp/rriter-headless/main.rs-<ts>.png
python3 scripts/rriter_headless.py shot . --size 1280x800 --scale 2 --out /tmp/ide.png
python3 scripts/rriter_headless.py run ui-check.txt  # replies on stdout, exit code of rriter
python3 scripts/rriter_headless.py repl              # type commands by hand
python3 scripts/rriter_headless.py bench src/main.rs 480 wheel 0 -3  # summary JSON, then CSV path
```

The wrapper looks for `target/x86_64-unknown-linux-gnu/release/rriter`
(`RRITER_BIN` overrides it); without a binary it prints `run make fast` and
exits with 2. `shot` sends `open` (file) or `workspace` (directory), `settle`,
`screenshot`, `quit`; any `err` goes to stderr with exit code 1. The app's own
stderr passes through. `python3 scripts/rriter_headless.py --self-test`
checks the wrapper without a binary.

## CLI

```
rriter --headless [--script FILE] [--size WxH] [--scale S] [--profile DIR | --profile-from-user]
                  [--hz N | --budget-ms F] [--keep-profile] [--allow-writes] [FILE_OR_DIR]
```

| Option | Meaning |
|---|---|
| `--script FILE` | Read commands from FILE (`-` = stdin, the default). |
| `--size WxH` | Framebuffer size, default `1920x1080`, range 320x200 … 8192x8192. |
| `--scale S` | UI scale, default `1.0`. |
| `--profile DIR` | Reusable profile root (created if missing). |
| `--profile-from-user` | Copy `~/.config/RRiter`, `~/.local/share/RRiter`, `~/.local/state/RRiter` (no cache) into a temp profile. Not with `--profile`. |
| `--hz N` / `--budget-ms F` | Frame budget for `bench`/`record`; mutually exclusive. Default: monitor refresh rate, else 240 Hz. |
| `--keep-profile` | Keep the temp profile on exit. |
| `--allow-writes` | Allow writing opened files, file-tree mutations and Git. Off by default. |
| `FILE_OR_DIR` | Same as a leading `open` (file) or `workspace` (directory). No path — welcome screen. |

Exit codes: `0` all `ok`, ended by `quit`/EOF/closed stdout; `1` an `err` or
unreadable input; `2` bad arguments or platform; `3` EGL or `Renderer` failed
(`headless: EGL setup failed at <stage>: … Try RRITER_EGL_VENDOR=mesa`).

## Protocol

UTF-8, one command per line. Empty lines and lines whose first non-blank
character is `#` are ignored. A reply is one line: `ok`, `ok <payload>` or
`err <reason>`; payloads never contain newlines (JSON is compact). Arguments
are separated by spaces. The last argument of `open`, `workspace`,
`screenshot`, `dump` and `type` is the rest of the line, so paths with spaces
need no quoting. Coordinates are physical pixels of the framebuffer.

| Command | Action | Reply |
|---|---|---|
| `open <path>` | Open a file in a tab (as drag&drop). Missing file or directory → `err`. | `ok tabs=<n> active=<i>` |
| `workspace <dir>` | Open a folder as the workspace (IDE mode, tree, watcher). | `ok` |
| `resize WxH` | Resize the framebuffer; on failure size and context stay. | `ok <w>x<h>` |
| `scale S` | Change the scale factor, 0.5 … 4.0. | `ok` |
| `mouse_move x y` | Move the cursor (hover). | `ok` |
| `click [left\|right\|middle] [down\|up] [alt]` | Press+release, or one phase. Default `left`; `alt` synthesizes Alt/Option for this click. | `ok` |
| `dblclick [btn]` | Two clicks with no pause. | `ok` |
| `wheel dx dy [lines\|px]` | Scroll; default `lines`. | `ok` |
| `key <combo>` | Press+release, e.g. `ctrl+s`, `shift+tab`, `escape`, `f5`. Unknown token → `err unknown key token '<t>'`. | `ok` |
| `type <text>` | Commit text as IME input. Escapes `\n`, `\t`, `\\`. | `ok` |
| `settle [ms]` | Run frames until idle or the budget ends (default 500). | `ok frames=<n> settled=<bool>` |
| `wait <ms>` | Run frames for real time `ms` (≤ 60000). | `ok frames=<n>` |
| `wake <ms>` | Strict loop model: block until the native loop would wake, run one `about_to_wait`, draw only if requested (see Strict loop model). | `ok cause=<c> frame=<bool> flow=<f> deadline_ms=<n\|none>` |
| `idle <ms>` | `wake` repeated for `ms` of input-free time. | `ok frames=<n> redraws=<n> polls=<n> wakes=<n> deadlines=<n> flow=<f> deadline_ms=<n\|none>` |
| `screenshot <out.png>` | Draw one frame and save a PNG; the directory is created. | `ok <abs path> <w>x<h>` |
| `dump [out.json]` | UI state as JSON, inline or into a file. | `ok <json>` / `ok <abs path>` |
| `dialog save\|discard\|cancel` | Answer the unsaved-changes dialog. No dialog → `err no dialog`. | `ok` |
| `picker_answer [<path> ...]` | Queue the paths for the next native picker (`picker_answer` alone answers cancel). | `ok` |
| `bench <frames> [csv=<path>] [action]` | Measure frame cost (see Bench). | `ok <json summary>` |
| `record <frames> <dir> [action]` | `bench` plus a PNG per frame (see Bench). `dir` is one token. | `ok <json summary>` |
| `info` | Refresh rate, frame budget, GL strings, policy, profile root. | `ok <json>` |
| `quit` | Stop. EOF does the same. | `ok` |

Rules:

- Every synthesized event is followed by exactly one frame: `click` and `key`
  draw two (press, release), `dblclick` four. Only `settle`/`wait` run more.
- Between commands nothing runs: background results (LSP, Git, file watcher,
  terminal) reach the UI only through `settle` or `wait`. Script runs are
  reproducible by frame count, not by animation time.
- `settle` stops after two idle steps in a row or at the budget;
  `settled=false` is a timeout, not an error. A pending timer due after the
  budget (e.g. a label that expires in seconds) ends it at once with
  `settled=false`: no frame would change before the budget runs out.
- After an `err` the next command still runs (stdin and `--script` alike);
  the exit code becomes 1. I/O failures read `err io: <error>`. Bad numbers,
  `NaN`/`inf`, wrong argument count and invalid UTF-8 are `err` too.

### Strict loop model (`wake`, `idle`)

`settle` and `wait` step on the driver's own schedule, so they draw frames the
real window would never get: a test with them cannot see a missing wake-up.
`wake`/`idle` step only when the native event loop would run `about_to_wait`
again (`cause`): `redraw` (the previous pass drew a frame), `poll`
(`ControlFlow::Poll`), `ui_waker` (a `UiWaker` event arrived), `deadline` (the
`WaitUntil` instant passed), else `timeout` after `ms` with nothing run. Time
is real: a deadline is reached by sleeping. A frame is drawn only when the pass
requests one. Input commands still run as usual. Headless has no cursor blink,
so an idle app ends in `flow=wait deadline_ms=none`. In Rust tests
`HeadlessSession::native_wake` returns the same with the pass start time.

`dump` → `event_loop`: `control_flow` (`wait`/`poll`/`wait_until`),
`deadline_ms` (ms left to the `WaitUntil`, else `null`), `awaiting_background`,
`wake_pending` (a wake not yet drained), `wake_events` (`UiWaker` events not
yet taken by the driver), `redraw_requested`.

### Example script

```
# ui-check.txt
open /home/me/projects/rriter/src/main.rs
settle
wheel 0 -10
settle 300
screenshot /tmp/rriter-headless/main-scrolled.png
key ctrl+f
type fn main
settle
dump /tmp/rriter-headless/search.json
quit
```

### Clicking what you see

`dump` lists every registered UI element of the last frame in `ui`:
`{"id":"IdeTabExplorer","kind":"IconButton","rect":[x,y,w,h],"overlay":false}`.
Take the center of `rect`, then `mouse_move cx cy` and `click`. `hover` shows
what the cursor is over after `mouse_move`.

Other `dump` keys: `size`, `scale`, `cursor_icon`, `mode`
(`ide|editor|welcome`), `tabs` (path, title, active, modified, cursor, scroll,
markdown, `markdown_media` (`null` unless the tab is a Markdown tab in Read mode, otherwise one object per media element: `source`, `alt`, `state` = `unknown|pending|ready|failed:<MediaError variant>`, `natural_w`/`natural_h`, laid-out `w`/`h` in pixels), `kind` (`normal|git_diff|api_client|database_table|database_query|pdf`)), `markdown_media_stats` (`media_gen`, `loads_started`, `texture_bytes`, `visible_texture_bytes` of the shared media cache), `editor` (line count, byte-offset `cursor` and `extra_cursors`, selection,
highlight version and byte-range spans), `ide_panel`, `overlays`,
`dialog`, `external_request`, `clipboard`, `writes_allowed`.
PDF tabs add `pdf` with `phase` (`engine_missing|engine_starting|loading|ready|error|password_required`), `page_count`, zero-based `current_page`, `scroll`, `textures` (number of uploaded page textures), `search_matches`, `search_done`, `selection_chars`, `engine` (`not_started|starting|ready|missing|failed|installing`), and `engine_message`. For PDF tabs, `scroll_y` equals `pdf.scroll`.
In headless mode, `clipboard` is `{"mode":"memory","text":...}`; `text` is
`null` until something is copied.

### Unsaved-changes dialog

In a window this dialog is a second OS window. Headless draws it centered over
the frame, and `dump` returns its buttons as `dialog.buttons` in screenshot
coordinates. Answer it only with `dialog save|discard|cancel` or `key escape`;
clicking the button coordinates does not reach it.

## Bench

`bench <frames> [csv=<path>] [action]` draws `frames` forced frames and applies
the action before each one: `none` (default), `wheel <dx> <dy>` (line
delta), `key <combo>` (press+release) or `type <text>`. Per frame:

| Column | Meaning |
|---|---|
| `update_ms` | action + `about_to_wait` (CPU) |
| `draw_cpu_ms` | frame draw calls, up to `glFinish` |
| `gpu_ms` | `GL_TIME_ELAPSED` around draw + finish; empty when timer queries are unsupported |
| `total_ms` | the whole step, including the wait for `glFinish` |
| `flush_calls`, `vertices` | renderer flushes and vertices of the frame |
| `root_*_ms`, `chrome_*_ms` | root phases (prep, cache, pre-editor, overlays, chrome) and chrome details |
| `scroll_y` | editor scroll, to see that the action works |

The CSV goes to `csv=<path>` or `${XDG_RUNTIME_DIR:-/tmp}/rriter-headless/bench-<unix_ms>.csv`
(outside the profile, so it survives exit). The summary holds `frames`,
`budget_ms`, `hz`, `hz_source` (`arg|monitor|default`), p50/p95/p99/max of
`total_ms`, `gpu_ms` (or `null`) and `draw_cpu_ms`, `over_budget` (frames with
`total_ms` above the budget), the five `worst` frames, `system`
(`loadavg_before/after`, `cpus`, `gpu_util_before/after` from `nvidia-smi` or
`null`, `process_cpu_ms` of the run) and the absolute `csv` path.

The budget is `--budget-ms`, or `1000 / --hz`, or the highest monitor refresh
rate (probed once without a window), or 240 Hz.

Interpretation: each pbuffer frame is serialized by `glFinish`, so `total_ms` is
the cost of one frame without pipelining — an upper bound for a vsynced window.
CPU phases are the app's cost when `loadavg` stays below `cpus`. `gpu_ms` is
polluted while a game or another GPU load runs: repeat the bench; the minimum
over runs is the lower bound of the app's cost, the spread is external load.
`RRITER_EGL_VENDOR=mesa` renders on the CPU, away from the game's GPU, but its
`gpu_ms` is not representative.

`record <frames> <dir> [action]` (≤ 600 frames) measures the same way, writes
`<dir>/frames.csv` with extra `sticky_anim_progress`, `search_anim_y`,
`tab_scroll` columns and saves every frame as `<dir>/frame-0000.png`, … The
summary adds `motion`: `scroll_y_delta_min/max` and `nonmonotonic_frames` —
frames whose `scroll_y` moves against the direction of the first move.
Readback and PNG encoding stay outside the timings but stretch real time
between frames, so `record` shows the animation's trajectory; frame cost is
what `bench` measures.

## Isolation

- RRiter state lives in the profile root, `--profile DIR` or a temp directory
  `${XDG_RUNTIME_DIR:-/tmp}/rriter-headless-<pid>/` removed on exit unless
  `--keep-profile`. The live editor and other headless runs are untouched.
- Opened files, file-tree mutations and Git are not written without
  `--allow-writes`; a blocked save shows the usual read-only notice. Saving a
  protected file never asks for elevation (`pkexec`) in headless mode.
- File pickers, `open_url` and "reveal in file manager" do nothing; the last
  such request appears in `dump` as `external_request`.
- Headless uses an in-process clipboard; no system clipboard or keyring is
  accessed. Saved database passwords are not found, and remembering one fails.
  No cursor blink; an idle app draws no frames.
- Replies keep the original stdout; the app's own stdout goes to stderr.

Not isolated: opened files are read from their real paths; LSP servers
(`ruff`, `ty`) start on `open *.py` as in a window and write their own caches
(`~/.cache/ruff`, `~/.cache/ty`); a profile with the Terminal panel open starts
a PTY with the user's shell.

## PGO training

`make max` (synonyms `make pgo`, `make pgo-auto`) runs `scripts/pgo_pipeline.py`, which trains the
instrumented binary headless on Linux (no window, no user HOME, no network) and installs the
PGO+Fat-LTO binary. `make max-nopgo` is the plain Fat-LTO build. Needs `target/pdfium.path`
(`make pdfium`) and `llvm-cxxfilt`; the pipeline fails before any build if either is missing.

```
rriter --headless --pgo-train --pgo-scenario S --pgo-workspace W --pgo-report R
       --pgo-timeout-seconds T --profile P
```

`--profile` is the rriter profile directory here (not the `.profdata` path of the pipeline).
Exit codes: `0` scenario finished, `1` a step failed or timed out, `2` launch error (EGL,
arguments, workspace). The report JSON carries `status`, `scenario`, `scenario_version`,
`frames`, `failed_step` and `skipped_groups`.

Scenarios (`--pgo-scenario`): `full` (all groups, saves the session on exit), `startup`
(restores that session), `welcome` (welcome screen), `smoke`, and `group:<name>` for iterating on
one group (`pdf`, `api_mock`, `git_changes`, `editor_ops`, `lsp_nav`, `input_scroll`,
`terminal_ops`). `make pgo-script` always runs the default list (`full,startup,welcome`); to run
one group through the script use
`python3 scripts/pgo_pipeline.py --run-only --run-executable <binary> --scenarios group:<name>`
(`--scenarios` accepts `group:<name>` on Linux and is ignored on other hosts). The focused test
is `make test TEST_FILTER=headless::ui_tests_pgo`.

A group whose external tool is missing (`requires` returns `Err`, e.g. `pdf` without pdfium) is
skipped and listed in `skipped_groups`; the run still succeeds. The pipeline treats a skipped
`pdf` as an error on Linux and checks per-group coverage markers (`scripts/pgo_coverage.py`).
The pipeline runs `--scenarios full,startup,welcome` on Linux (`--install-binary PATH` copies the
result); on other platforms it runs the GUI `full` scenario.

## Limits

- Animations run on the real clock; `wait` is the only way to let them pass.
- `type` cannot send leading spaces (the text starts at the first non-blank
  character); use `key space`.
- The unsaved-changes dialog answers only to `dialog`/`key escape`.
- PNG is sRGB without a color profile; alpha is forced to 255.
- The pbuffer size is capped by the driver (`EGL_MAX_PBUFFER_WIDTH/HEIGHT`);
  a `resize` beyond it is an `err`.
