#!/usr/bin/env python3
"""Terminal parity check: runs one headless scenario on RRiter and Ronsole and compares the replies.

Usage:
    python3 scripts/term_parity/compare.py <scenario> [--keys a,b] [--size WxH] [--scale S]
                                           [--ronsole-dir DIR]

Scenario format: one protocol command per line (the protocol of docs/headless.md, the same for
both drivers), `#` starts a comment. Two markers are common to both apps:

  #@click_ui <UiId>   Click the centre of the element with that id. The rect comes from the
                      `ui` list of RRiter's `dump`. Ronsole's dump has no such list, so this marker
                      is skipped for Ronsole and the skip is printed; Ronsole's terminal is assumed
                      to be the focused view from the start.
  #@row_of <marker>   Replaced by `dump row N`, N being the index in the terminal `text` of the first
                      visible row containing <marker>. The index comes from a probe run per app, so
                      it is right for each app's panel. The row's cells are compared only when that
                      row has the same right-stripped text in both apps; a different text is a DIFF
                      (the `dump row` reply itself carries no text).
  #@link_of <marker>  Like `#@row_of`, but the row is compared by its hyperlink only: for rows whose
                      text differs between the apps (a wrapped line after a resize). In each app the
                      cells with `link` in that row must be one run of one URI that covers the whole row
                      text, from its first to its last character. The URIs must agree; the runs are
                      listed in the report. The cells' other fields and the row text are not compared.

Pixel markers (screenshots, stdlib PNG decoder; the layout comes from `terminal.layout`). A marker
reads one cell: its centre pixel, or the rows of its ink. `_of` variants take the row from the first
visible row containing <marker> (one word, found as by `#@row_of`) instead of <row>. <name> is an
optional label, one word, printed with the result. Each marker takes its own screenshot at its place
in the scenario; the probe run replays the scenario prefix, so the pixels belong to that state.

  #@pixel <col> <row> [<name>]         (or `#@pixel_of <marker> <col> [<name>]`)
                      Centre pixel of the cell at that visible position. Report-only: printed per app,
                      never a cross-app DIFF (glyph pixels depend on the font). The cells of the two apps
                      are at different visible rows when the panels differ in height, so the `_of` form
                      is the usual one.
  #@pixel_bg <col> <row> [<name>]      (or `#@pixel_bg_of <marker> <col> [<name>]`)
                      The cell's centre pixel against the centre pixel of the empty cell at the right
                      edge of the same row, in the same app: for a cell painted with the panel
                      background. Not compared across apps, their backgrounds differ by design.
  #@ink_rows <col> <row> [<name>]      (or `#@ink_rows_of <marker> <col> [<name>]`)
                      Pixel rows of the cell whose colour differs from the cell's top-left pixel by
                      more than 24/255 in any channel: top and bottom, relative to the cell's top
                      edge, and their centre. Report-only (glyph ink differs between the fonts); the
                      within-app checks below read it.
  #@ink_weight <col> <row> [<name>]    (or `#@ink_weight_of <marker> <col> [<name>]`)
                      Ink mass of the cell (`ink_weight`): the largest channel difference from the cell's fill
                      (`fill_colour`, rows from `cell_rows`), summed over the pixels that differ by more than
                      24/255. It grows with the glyph's width and darkness. Report-only across apps; the
                      within-app checks below read it.
  #@line <col> <row> [<name>]          (or `#@line_of <marker> <col> [<name>]`)
                      A horizontal line through a blank cell (strikethrough, later underline): the rows
                      where at least 80% of the cell's width differs from the cell's fill (`fill_colour`: the
                      median colour of the cell box, so an inverse cell's fill counts as background) by more
                      than 24/255. Measured: thickness in px; top and bottom edge in px relative to
                      `layout.baseline`; the centre relative to the baseline divided by the app's `char_h`.
                      The cell top is the fractional `cell0` + row * char_h, so whole-pixel rounding of the
                      cell does not shift the line. MATCH when the thickness differs by at most 1 px and
                      the offset difference times RRiter's `char_h` is at most 1 px; a line missing in one
                      app or in both is DIVERGE. A label containing `underline` also checks the spec L12
                      rule (the top edge is not above the baseline, top >= 0), DIFF otherwise.
                      Add `absent` as the last argument to expect no line: MATCH when no app has a line,
                      DIVERGE (with the numbers) when any app has one.
                      Put the attribute on spaces (SGR 9 around four spaces): glyph ink differs between
                      the fonts, the cell box does not.

Within-app checks (each app against itself; DIFF on failure, the numbers go to the .diff):
  #@centre_within <line> <ink> [tol_px]  The centre of the line measured by `#@line` label <line> is
                      within tol_px (default 1) of the centre of the ink measured by `#@ink_rows` label
                      <ink>, both in the same cell row (L14: strike at the x-height centre).
  #@same_glyph <regular> <bold>       The `#@ink_rows` rows (exact) and the `#@pixel` centre (2/255) of
                      the two labelled cells match (L17: colour emoji share the regular entry). Use the
                      same label for the ink and the pixel marker of one cell.
  #@bolder <bold> <regular>           The `#@ink_weight` of the bold cell is strictly above that of the
                      regular cell of the same letter, in each app (L11: bold is wider or darker).
  #@drawn <label>                     The `#@ink_weight` of the labelled cell is above 0 in each app (L11:
                      a character missing from the styled face is still drawn, by fallback).
Frame check:
  #@frames_grew <n> <m>               In each app, `event_loop.frame_requests` of the m-th `dump` is above
                      that of the n-th (1-based, among the scenario's `dump` commands): the request was
                      counted (L15). The absolute counter is not compared.

The scenario runs twice per app: once as a probe (prefix + `dump`) to find marker coordinates and
row indexes, then in full. Both apps get the same `--size` and `--scale`.

Compared, per reply:
  * `dump row <n>` (cells): `ch`, `flags`, `underline`, `link` exact; `fg`, `bg`, `underline_color`
    within 2/255 per channel. Cells are placed by column in the left half of the row and by distance
    from the right edge in the right half (see `cell_place`), so a right-aligned prompt segment lines up
    when the widths differ. Inverse cells skip `fg`/`bg` only when each side is the host default (null or
    the app's own OSC 11 colour, EXPECTED `palette`); explicit colours and `underline_color` are compared.
    Each app's inverse `fg` must equal its own OSC 11 colour (self-check on the first inverse cell, DIFF
    otherwise).
  * `dump` (paths in PATH_MAP):
    - `text`: the sequence of non-empty rows, right-stripped; runs of spaces collapse on shell-prompt rows
      only (`PROMPT_GLYPHS`, a prompt pads to the width). Blank rows are dropped and the rows are aligned
      at the bottom, because the RRiter panel shows fewer rows than the Ronsole window; the number of rows
      is expected only when the shorter side's panel is full (`panel_full`), otherwise a DIFF.
    - `cursor`: `col` exact, and the row relative to the last non-empty row of `text`. The absolute
      row cannot match across different heights; the relative one can. Empty `text` gives row 0.
    - `modes`, `selection`, `hovered_link`, `title`, `cursor_icon`, `clipboard` exact; `external_request`
      exact after the Q22 normalisation (`external_request_key`);
      `terminal_tabs` (RRiter) vs `tabs` (Ronsole) exact on index, title, active. Tab rects are not
      compared (panel vs window).
    - `cols`, `rows`, `viewport`, `layout`: EXPECTED_DIFF. Counted with their reason, not DIFF lines.
  * `settle`, `wait`, `idle`, `wake` replies: only the `ok` status (frame counters differ by design).
    `mouse_cell` replies (`ok <x> <y>`): only the `ok` status (EXPECTED `mouse_cell`: the centre pixel
    depends on the host geometry). `event_loop` is not in PATH_MAP; see `#@frames_grew`.
  * any other reply: exact string. Replies to clicks injected by `#@click_ui` markers are not compared.

Path differences between the two `dump` shapes live in PATH_MAP, not in the scenario files.
New scenarios: copy l01_sgr.txt and change the commands; no per-app copies.

An OSC 11 reply inside `text` (`rgb:` with three 1-4 digit hex components) has its components masked
before the comparison, the `/` separators stay compared (the colour differs by design, see the `palette`
entry of EXPECTED_DIFF); the colours of both apps are printed as expected differences.
An XTVERSION reply inside `text` (ESC P > | name ( version ) ESC backslash) has only its name masked:
the names are an expected difference (`xtversion`), the version and the framing are compared exactly.

Output: the summary line and the path of the full report, `/tmp/term_parity/<scenario>.diff`, which
holds every DIFF, expected difference and pixel measurement.

Exit status: 0 when nothing differs, 1 otherwise, 2 when a driver fails to start.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path
from typing import NamedTuple
from urllib.parse import unquote

REPO = Path(__file__).resolve().parents[2]
COLOR_TOLERANCE = 2
DEFAULT_RONSOLE_DIR = Path("/home/reyan/projects/ronsole")

# Name, RRiter path in `dump`, Ronsole path in `dump`, mode. Modes: exact (DIFF on mismatch),
# expected (counted with its reason from EXPECTED_DIFF, never a DIFF line).
PATH_MAP: list[tuple[str, tuple[str, ...], tuple[str, ...], str]] = [
    ("text", ("terminal", "text"), ("terminal", "text"), "exact"),
    ("cols", ("terminal", "cols"), ("terminal", "cols"), "expected"),
    ("rows", ("terminal", "rows"), ("terminal", "rows"), "expected"),
    ("modes", ("terminal", "modes"), ("terminal", "modes"), "exact"),
    ("viewport", ("terminal", "viewport"), ("terminal", "viewport"), "expected"),
    ("cursor", ("terminal", "cursor"), ("terminal", "cursor"), "exact"),
    ("selection", ("terminal", "selection"), ("terminal", "selection"), "exact"),
    ("hovered_link", ("terminal", "hovered_link"), ("terminal", "hovered_link"), "exact"),
    ("title", ("terminal", "title"), ("terminal", "title"), "exact"),
    ("cursor_icon", ("cursor_icon",), ("cursor_icon",), "exact"),
    ("terminal_tabs", ("terminal_tabs",), ("tabs",), "exact"),
    ("layout", ("terminal", "layout"), ("terminal", "layout"), "expected"),
    ("clipboard", ("clipboard",), ("clipboard",), "exact"),
    ("external_request", ("external_request",), ("external_request",), "exact"),
]
# Paths that differ by design: the RRiter terminal is the bottom panel, Ronsole's is the whole window.
EXPECTED_DIFF = {
    "text": "panel shows only the bottom rows of the window; the common bottom rows are compared",
    "cols": "panel width differs from window width",
    "rows": "panel height differs from window height",
    "viewport": "scroll window and scrollback follow the panel height",
    "layout": "panel geometry and font metrics differ",
    "palette": "host palette: OSC 11 colour and inverse defaults, spec terminal-parity-design.md:116",
    "xtversion": "app name in the XTVERSION reply, TermIdentity: by design",
    "mouse_cell": "centre pixel of the cell depends on the host geometry; only the ok status is compared",
}
ROW_EXACT = ("ch", "flags", "underline", "link")
ROW_COLOR = ("fg", "bg", "underline_color")
TABS_EXACT = ("index", "title", "active")
TIMED_COMMANDS = ("settle", "wait", "idle", "wake")
HOST_PIXEL_COMMANDS = ("mouse_cell",)
# Pixel markers, see the docstring. Group 1 is the kind, group 2 the `_of` suffix, group 3 the arguments.
PIXEL_MARKER = re.compile(r"#@(pixel_bg|pixel|ink_rows|ink_weight|line)(_of)?\s+(.+)")
# Within-app and frame directives, checked after the measurements; the lines pass through the marker
# resolution and are dropped from the commands.
CENTRE_WITHIN = re.compile(r"#@centre_within\s+(\S+)\s+(\S+)(?:\s+(\d+))?")
SAME_GLYPH = re.compile(r"#@same_glyph\s+(\S+)\s+(\S+)")
BOLDER = re.compile(r"#@bolder\s+(\S+)\s+(\S+)")
DRAWN = re.compile(r"#@drawn\s+(\S+)")
FRAMES_GREW = re.compile(r"#@frames_grew\s+(\d+)\s+(\d+)")
PIXEL_TOLERANCE = 2
INK_THRESHOLD = 24
LINE_COVER = 0.8
LINE_TOLERANCE_PX = 1
UNDERLINE_LABEL = "underline"
LINE_KINDS = ("line", "line_absent")
# OSC 11 reply as printed by the hex reader: ESC ] 1 1 ; r g b : then `rrrr/gggg/bbbb` as ASCII hex, each
# component 1-4 hex digits. Each ASCII hex digit is two hex digits: 30-39 (0-9), 41-46 (A-F), 61-66 (a-f).
# Only the three components are captured; the `/` separators (2f) are left to be compared.
ASCII_HEX_DIGIT = r"(?:3[0-9]|[46][1-6])"
OSC11_COLOUR_HEX = re.compile(
    rf"(?<=1b5d31313b7267623a)((?:{ASCII_HEX_DIGIT}){{1,4}})2f((?:{ASCII_HEX_DIGIT}){{1,4}})2f((?:{ASCII_HEX_DIGIT}){{1,4}})(?!{ASCII_HEX_DIGIT})"
)
# XTVERSION reply as printed by the hex reader: ESC P > | (1b 50 3e 7c) then the app name in hex, then
# ( version ) and ESC backslash (1b 5c). Group 1 is the name; only the name is masked.
XTVERSION_NAME_HEX = re.compile(r"(?<=1b503e7c)((?:[0-9a-f]{2})+?)(?=28(?:[0-9a-f]{2})+?291b5c)")
XTVERSION_MASK = "XTNAME"
DIFF_DIR = Path("/tmp/term_parity")
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
# (kind, label, value). Value: an rgb tuple for `pixel`, a (cell, background) pair for `pixel_bg`,
# (top, bottom, centre) or None for `ink_rows`, a Line or None for `line` and `line_absent`.
Measure = tuple[str, str, object]


class Line(NamedTuple):
    """A measured horizontal line; edges and offsets are in pixels of the app's own image."""

    thickness: float  # px
    top: float  # top edge, px from layout.baseline (negative: above it)
    bottom: float  # bottom edge (exclusive), px from layout.baseline
    offset: float  # centre from layout.baseline, divided by char_h
    char_h: float
    centre: float  # centre inside the cell, px from the cell's top edge
# Row a `#@row_of` or `#@link_of` marker points at: (right-stripped raw text, link_only).
RowRef = tuple[str, bool]


def driver_for(app: str, ronsole_dir: Path) -> Path:
    if app == "rriter":
        return REPO / "scripts" / "rriter_headless.py"
    return ronsole_dir / "scripts" / "ronsole_headless.py"


def parse_scenario(text: str) -> list[str]:
    """Scenario lines with blanks and comments removed; `#@` markers are kept."""
    lines = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or (line.startswith("#") and not line.startswith("#@")):
            continue
        lines.append(line)
    return lines


def run_driver(app: str, ronsole_dir: Path, size: str, scale: str, commands: list[str]) -> list[str]:
    """Runs `commands` through the app's `run` mode and returns the reply lines."""
    with tempfile.TemporaryDirectory(prefix="term-parity-") as tmp:
        script = Path(tmp) / "scenario.txt"
        script.write_text("\n".join(commands) + "\n", encoding="utf-8")
        driver = driver_for(app, ronsole_dir)
        result = subprocess.run(
            [sys.executable, str(driver), "run", str(script), "--size", size, "--scale", scale],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
    if result.returncode not in (0, 1):
        print(f"{app}: driver exited {result.returncode}\n{result.stderr}", file=sys.stderr)
        sys.exit(2)
    return [line for line in result.stdout.splitlines() if line.strip()]


def resolve_markers(
    app: str, ronsole_dir: Path, size: str, scale: str, lines: list[str], px_dir: Path, measures: list[Measure]
) -> list[tuple[str, bool, RowRef | None]]:
    """Replaces `#@` markers by commands, using probe runs for click rects and row indexes.

    Pixel markers are measured here, each with its own probe, and appended to `measures` in
    scenario order; screenshots go to `px_dir`.

    Returns (command, injected, row_ref) triples: `injected` marks the commands a marker added,
    `row_ref` is a RowRef for the row a `#@row_of` or `#@link_of` marker points at, else None.
    """
    resolved: list[tuple[str, bool, RowRef | None]] = []
    for index, line in enumerate(lines):
        click = re.fullmatch(r"#@click_ui\s+(.+)", line)
        row_of = re.fullmatch(r"#@row_of\s+(.+)", line)
        link_of = re.fullmatch(r"#@link_of\s+(.+)", line)
        pixel = PIXEL_MARKER.fullmatch(line)
        if not click and not row_of and not link_of and not pixel:
            resolved.append((line, False, None))
            continue
        if click and app != "rriter":
            print(f"{app}: skipped marker '{line}' (no ui list in dump)")
            continue
        probe_lines = [item for item, _, _ in resolved if not item.startswith("#@")]
        if pixel:
            shot = px_dir / f"{app}-{index}.png"
            measures.append(measure_pixel(app, ronsole_dir, size, scale, probe_lines, pixel, shot))
            continue
        replies = run_driver(app, ronsole_dir, size, scale, probe_lines + ["dump"])
        dump = json.loads(replies[-1].removeprefix("ok ")) if replies else {}
        if row_of or link_of:
            marker = (row_of or link_of).group(1)
            text = get_path(dump, ("terminal", "text")) or []
            found = next((i for i, row in enumerate(text) if marker in row), None)
            if found is None:
                print(f"{app}: marker '{line}' not found in terminal text (line {index + 1})", file=sys.stderr)
                sys.exit(2)
            resolved.append((f"dump row {found}", False, (text[found].rstrip(), link_of is not None)))
            continue
        rect = next(
            (element["rect"] for element in dump.get("ui", []) if element.get("id") == click.group(1)),
            None,
        )
        if rect is None:
            print(f"{app}: marker '{line}' not found in ui list (line {index + 1})", file=sys.stderr)
            sys.exit(2)
        x, y = int(rect[0] + rect[2] / 2), int(rect[1] + rect[3] / 2)
        resolved += [(f"mouse_move {x} {y}", True, None), ("click", True, None)]
    return resolved


def measure_pixel(app: str, ronsole_dir: Path, size: str, scale: str, probe_lines: list[str], match: re.Match, shot: Path) -> Measure:
    """Screenshots the app in the state the marker stands in and reads the marked cell's pixels.

    The probe replays `probe_lines`, then `dump` (layout, text) and `screenshot`. Cell (col, row)
    starts at `cell0` + (col * char_w, row * char_h), all in physical pixels of the framebuffer.
    """
    kind, of, arguments = match.group(1), match.group(2), match.group(3).split()
    if of:
        marker, col, extra = arguments[0], int(arguments[1]), arguments[2:]
        row = None
    else:
        col, row, extra = int(arguments[0]), int(arguments[1]), arguments[2:]
    absent = "absent" in extra
    extra = [word for word in extra if word != "absent"]
    label = extra[0] if extra else match.group(0)
    replies = run_driver(app, ronsole_dir, size, scale, probe_lines + ["dump", f"screenshot {shot}"])
    if len(replies) < 2 or not replies[-2].startswith("ok {") or not replies[-1].startswith("ok "):
        print(f"{app}: pixel probe for '{label}' failed: {replies[-2:]}", file=sys.stderr)
        sys.exit(2)
    terminal = json.loads(replies[-2][3:]).get("terminal") or {}
    layout = terminal.get("layout")
    if layout is None:
        print(f"{app}: no terminal layout for '{label}' (the terminal was not drawn)", file=sys.stderr)
        sys.exit(2)
    text = terminal.get("text") or []
    if of:
        row = next((i for i, line in enumerate(text) if marker in line), None)
        if row is None:
            print(f"{app}: marker '{marker}' not found in terminal text (pixel '{label}')", file=sys.stderr)
            sys.exit(2)
    x0, y0 = layout["cell0"]
    char_w, char_h = layout["char_w"], layout["char_h"]
    cell_top = y0 + row * char_h
    left, top = int(x0 + col * char_w), int(cell_top)
    box_w, box_h = int(char_w), int(char_h)
    image = read_png(shot, top + box_h + 1)
    centre = (int(x0 + (col + 0.5) * char_w), int(y0 + (row + 0.5) * char_h))
    if kind == "line":
        found = line_extent(image, left, top, box_w, box_h, layout["baseline"], char_h, cell_top - top)
        return ("line_absent" if absent else kind), label, found
    if kind == "ink_rows":
        return kind, label, ink_extent(image, left, top, box_w, box_h)
    if kind == "ink_weight":
        return kind, label, ink_weight(image, left, top, box_w, box_h, char_h, cell_top - top)
    if kind == "pixel_bg":
        cols = terminal.get("cols")
        if cols is None or row >= len(text) or text[row][cols - 1:cols].strip():
            print(f"{app}: '{label}' needs an empty last column in row {row} for its background", file=sys.stderr)
            sys.exit(2)
        reference = (int(x0 + (cols - 0.5) * char_w), centre[1])
        return kind, label, (png_rgb(image, *centre), png_rgb(image, *reference))
    return kind, label, png_rgb(image, *centre)


def parse_reply(reply: str):
    if reply.startswith("ok {"):
        return json.loads(reply[3:])
    return reply


def comparable(command: str, reply):
    """Frame counters of timed commands and the host-dependent pixels of `mouse_cell` are not compared; only `ok` is."""
    if isinstance(reply, str) and command.split()[0] in TIMED_COMMANDS + HOST_PIXEL_COMMANDS and reply.startswith("ok"):
        return "ok"
    return reply


def hex_channels(value: str) -> tuple[int, int, int]:
    return int(value[1:3], 16), int(value[3:5], 16), int(value[5:7], 16)


def to_hex(rgb: tuple[int, int, int]) -> str:
    return "#%02x%02x%02x" % rgb


def rgb_close(a: tuple[int, int, int], b: tuple[int, int, int], tolerance: int) -> bool:
    return all(abs(x - y) <= tolerance for x, y in zip(a, b))


def unfilter_row(kind: int, line: bytearray, previous: bytearray, bpp: int) -> bytearray:
    """Reverses one PNG scanline filter (types 0-4) in place; `previous` is the unfiltered line above."""
    if kind == 0:
        return line
    if kind == 1:
        for i in range(bpp, len(line)):
            line[i] = (line[i] + line[i - bpp]) & 0xFF
    elif kind == 2:
        for i in range(len(line)):
            line[i] = (line[i] + previous[i]) & 0xFF
    elif kind == 3:
        for i in range(len(line)):
            left = line[i - bpp] if i >= bpp else 0
            line[i] = (line[i] + ((left + previous[i]) >> 1)) & 0xFF
    elif kind == 4:
        for i in range(len(line)):
            a = line[i - bpp] if i >= bpp else 0
            b = previous[i]
            c = previous[i - bpp] if i >= bpp else 0
            p = a + b - c
            pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            predictor = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
            line[i] = (line[i] + predictor) & 0xFF
    else:
        raise ValueError(f"unknown PNG filter type {kind}")
    return line


def read_png(path: Path, max_row: int) -> tuple[int, int, list[bytearray]]:
    """Decodes an 8-bit RGB or RGBA PNG with the stdlib, rows 0..max_row only.

    Returns (width, channels, rows); alpha is not used. Only the rows up to `max_row` are
    unfiltered, because each row depends on the one above it and the scan stops early.
    """
    data = path.read_bytes()
    if data[:8] != PNG_SIGNATURE:
        raise ValueError(f"{path}: not a PNG")
    pos = 8
    width = height = channels = 0
    idat: list[bytes] = []
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or colour not in (2, 6) or interlace != 0:
                raise ValueError(f"{path}: unsupported PNG (depth {depth}, colour {colour}, interlace {interlace})")
            channels = 3 if colour == 2 else 4
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    stride = width * channels
    raw = zlib.decompress(b"".join(idat))
    rows: list[bytearray] = []
    previous = bytearray(stride)
    for y in range(min(height, max_row + 1)):
        start = y * (stride + 1)
        line = unfilter_row(raw[start], bytearray(raw[start + 1:start + 1 + stride]), previous, channels)
        rows.append(line)
        previous = line
    return width, channels, rows


def png_rgb(image: tuple[int, int, list[bytearray]], x: int, y: int) -> tuple[int, int, int]:
    width, channels, rows = image
    if not (0 <= x < width and 0 <= y < len(rows)):
        raise ValueError(f"pixel ({x}, {y}) outside the decoded image ({width} wide, {len(rows)} rows)")
    offset = x * channels
    return rows[y][offset], rows[y][offset + 1], rows[y][offset + 2]


def ink_extent(image: tuple[int, int, list[bytearray]], left: int, top: int, width: int, height: int) -> tuple[float, float, float] | None:
    """Top and bottom ink rows of a cell box, relative to `top`, and their centre; None without ink."""
    background = png_rgb(image, left, top)
    inked = [
        y
        for y in range(height)
        if any(
            abs(a - b) > INK_THRESHOLD
            for x in range(width)
            for a, b in zip(png_rgb(image, left + x, top + y), background)
        )
    ]
    if not inked:
        return None
    return float(inked[0]), float(inked[-1]), (inked[0] + inked[-1]) / 2


def cell_rows(height: int, char_h: float, fraction: float) -> list[int]:
    """Box rows whose centre lies inside the true cell.

    The whole-pixel box starts `fraction` px above the true cell top, so its first row can be
    outside the cell (the window background, in Ronsole's inverse cells) and is left out.
    """
    return [y for y in range(height) if fraction <= y + 0.5 < fraction + char_h]


def fill_colour(image: tuple[int, int, list[bytearray]], left: int, top: int, width: int, rows: list[int]) -> tuple[int, int, int]:
    """Per-channel median colour of a cell's rows: the cell's fill, whatever a line covers in it.

    The top-left pixel is not used: it can lie outside the cell (see `cell_rows`).
    """
    pixels = [png_rgb(image, left + x, top + y) for y in rows for x in range(width)]
    middle = len(pixels) // 2
    return tuple(sorted(pixel[channel] for pixel in pixels)[middle] for channel in range(3))


def line_extent(
    image: tuple[int, int, list[bytearray]], left: int, top: int, width: int, height: int, baseline: float, char_h: float, fraction: float
) -> Line | None:
    """First horizontal line in a blank cell box, measured against the baseline; None without a line.

    A row belongs to the line when `LINE_COVER` of the cell width differs from the cell's fill
    (`fill_colour`). `top` is the whole-pixel cell top and `fraction` the part of the true top edge
    it leaves out; `baseline` is measured from the true top edge, so the fraction is taken off every edge.
    """
    rows = cell_rows(height, char_h, fraction)
    background = fill_colour(image, left, top, width, rows)
    needed = LINE_COVER * width
    covered = [
        y
        for y in rows
        if sum(
            any(abs(a - b) > INK_THRESHOLD for a, b in zip(png_rgb(image, left + x, top + y), background))
            for x in range(width)
        ) >= needed
    ]
    if not covered:
        return None
    run = [covered[0]]
    for y in covered[1:]:
        if y != run[-1] + 1:
            break
        run.append(y)
    # Continuous edges: pixel row y covers [y, y + 1).
    first, last = float(run[0]), float(run[-1] + 1)
    centre = (first + last) / 2
    return Line(
        thickness=float(len(run)),
        top=first - fraction - baseline,
        bottom=last - fraction - baseline,
        offset=(centre - fraction - baseline) / char_h,
        char_h=char_h,
        centre=centre,
    )


def ink_weight(
    image: tuple[int, int, list[bytearray]], left: int, top: int, width: int, height: int, char_h: float, fraction: float
) -> int:
    """Ink mass of a cell: the largest channel difference from the cell's fill, summed over the pixels above `INK_THRESHOLD`.

    Mass grows with a glyph's width and with its darkness together (spec L11: bold is wider or darker).
    A pixel count alone is not enough: Ronsole's bold L has a narrower stem than its regular L and is
    darker, which only the mass shows.
    """
    rows = cell_rows(height, char_h, fraction)
    background = fill_colour(image, left, top, width, rows)
    total = 0
    for y in rows:
        for x in range(width):
            diff = max(abs(a - b) for a, b in zip(png_rgb(image, left + x, top + y), background))
            if diff > INK_THRESHOLD:
                total += diff
    return total


def cell_place(col: int, cols: int | None) -> tuple[str, int]:
    """Where a cell sits in the row: left half by column, right half by distance from the edge.

    A right-aligned prompt segment sits at different columns when the two terminals differ in width.
    Without a known width the column is used as it is.
    """
    if cols is None or col < cols // 2:
        return ("col", col)
    return ("from right", cols - 1 - col)


def is_host_default(value: str | None, osc11: str | None) -> bool:
    """True when a cell colour is the host default: no colour (null) or the app's own OSC 11 background."""
    return value is None or (osc11 is not None and value.lower() == osc11)


def compare_row(
    name: str,
    rriter: dict,
    ronsole: dict,
    rriter_cols: int | None,
    ronsole_cols: int | None,
    expected: dict[str, tuple[int, str]],
    osc11: tuple[str | None, str | None],
) -> list[str]:
    """Cell comparison of one `dump row` reply. `osc11` is each app's host background colour (`#rrggbb`) or None."""
    diffs: list[str] = []
    rriter_cells = {cell_place(cell["col"], rriter_cols): cell for cell in rriter.get("cells", [])}
    ronsole_cells = {cell_place(cell["col"], ronsole_cols): cell for cell in ronsole.get("cells", [])}
    for place in sorted(set(rriter_cells) | set(ronsole_cells)):
        label = f"{name} {place[0]} {place[1]}"
        left = rriter_cells.get(place)
        right = ronsole_cells.get(place)
        if left is None or right is None:
            diffs.append(f"{label}: present in only one app")
            continue
        for key in ROW_EXACT:
            if left.get(key) != right.get(key):
                diffs.append(f"{label} {key}: rriter={left.get(key)!r} ronsole={right.get(key)!r}")
        inverse = "inverse" in (left.get("flags") or []) or "inverse" in (right.get("flags") or [])
        for key in ROW_COLOR:
            a, b = left.get(key), right.get(key)
            if inverse and key != "underline_color" and is_host_default(a, osc11[0]) and is_host_default(b, osc11[1]):
                # Inverse fg/bg show the host palette, checked per app by the self-check; explicit colours are compared.
                count, first = expected.get("palette", (0, f"{label} {key}: host default not compared"))
                expected["palette"] = (count + 1, first)
                continue
            if a is None or b is None:
                if a != b:
                    diffs.append(f"{label} {key}: rriter={a!r} ronsole={b!r}")
                continue
            if any(abs(x - y) > COLOR_TOLERANCE for x, y in zip(hex_channels(a), hex_channels(b))):
                diffs.append(f"{label} {key}: rriter={a} ronsole={b} (tolerance {COLOR_TOLERANCE})")
    return diffs


def link_summary(row: dict, row_text: str) -> tuple[list[str], set[str], tuple[int, int] | None]:
    """Problems with the hyperlink of one `dump row` reply, its URIs, and the first and last column of its run.

    The run must be one URI over contiguous columns that covers the whole row text (first to last
    character). The columns are the cells' `col` values, the row text is the right-stripped raw text.
    """
    linked = sorted((cell for cell in row.get("cells", []) if cell.get("link")), key=lambda cell: cell["col"])
    if not linked:
        return ["no hyperlink in the row"], set(), None
    cols = [cell["col"] for cell in linked]
    uris = {cell["link"] for cell in linked}
    problems = []
    if len(uris) != 1:
        problems.append(f"{len(uris)} URIs in one row")
    if cols != list(range(cols[0], cols[-1] + 1)):
        problems.append("the link cells are not one contiguous run")
    first, last = len(row_text) - len(row_text.lstrip()), len(row_text) - 1
    if (cols[0], cols[-1]) != (first, last):
        problems.append(f"the run {cols[0]}..{cols[-1]} does not cover the row text {first}..{last}")
    return problems, uris, (cols[0], cols[-1])


def compare_link_row(name: str, left: dict, right: dict, left_ref: RowRef, right_ref: RowRef) -> tuple[list[str], list[str]]:
    """Hyperlink check for a `#@link_of` row, used instead of the cell comparison. Returns (DIFF lines, report lines)."""
    left_problems, left_uris, left_run = link_summary(left, left_ref[0])
    right_problems, right_uris, right_run = link_summary(right, right_ref[0])
    problems = [f"rriter: {problem}" for problem in left_problems] + [f"ronsole: {problem}" for problem in right_problems]
    if left_uris != right_uris:
        problems.append(f"URIs differ: rriter={sorted(left_uris)} ronsole={sorted(right_uris)}")
    diffs = []
    if problems:
        # The link cells of both rows are the dump row to record: the reply itself, reduced to its links.
        linked = {
            "rriter": [cell for cell in left.get("cells", []) if cell.get("link")],
            "ronsole": [cell for cell in right.get("cells", []) if cell.get("link")],
        }
        diffs.append(f"{name} link: {'; '.join(problems)}; dump row links: {json.dumps(linked, ensure_ascii=False)}")
    info = f"{name} link: rriter {sorted(left_uris)} cols {left_run}, ronsole {sorted(right_uris)} cols {right_run}"
    return diffs, [info]


def get_path(value, path: tuple[str, ...]):
    for key in path:
        if not isinstance(value, dict) or key not in value:
            return None
        value = value[key]
    return value


# Glyphs of the shell prompt in the dumps (l01_sgr.diff): the `❯` command marker, the powerline separators
# and the check mark of the right-aligned segment.
PROMPT_GLYPHS = ("❯", "", "", "✔")


def norm_row(row: str) -> str:
    """Right-stripped row; on a shell-prompt row runs of spaces collapse, because a right-aligned prompt pads to the width."""
    row = row.rstrip()
    if any(glyph in row for glyph in PROMPT_GLYPHS):
        return re.sub(r" {2,}", " ", row)
    return row


def xtversion_names(rows: list[str]) -> list[str]:
    """App names found in the XTVERSION replies of the rows (hex-decoded from the raw row text)."""
    return [match.group(1) for row in rows for match in XTVERSION_NAME_HEX.finditer(row)]


def external_request_key(request):
    """Comparable form of an `external_request` (spec Q22): reveal_path(p) is open_url(file://<parent of p>),
    open_path(p) is open_url(file://p). Both sides become the realpath of the file they open; other requests
    stay as they are and are compared literally."""
    if not isinstance(request, dict) or not isinstance(request.get("arg"), str):
        return request
    kind, arg = request.get("kind"), request["arg"]
    if kind == "open_url" and arg.startswith("file://"):
        path = unquote(arg[len("file://"):])
    elif kind == "reveal_path":
        path = os.path.dirname(arg)
    elif kind == "open_path":
        path = arg
    else:
        return request
    return {"file": os.path.realpath(path)}


def dump_view(name: str, dump: dict, path: tuple[str, ...]):
    """The value of one PATH_MAP entry in the form that is compared between the two apps."""
    value = get_path(dump, path)
    if name == "text":
        return [
            XTVERSION_NAME_HEX.sub(XTVERSION_MASK, OSC11_COLOUR_HEX.sub(mask_osc11, norm_row(row)))
            for row in value or []
            if row.strip()
        ]
    if name == "external_request":
        return external_request_key(value)
    if name == "cursor" and value is not None:
        rows = get_path(dump, ("terminal", "text")) or []
        last = max((i for i, row in enumerate(rows) if row.strip()), default=0)
        return {"col": value["col"], "row_from_last_text_row": value["row"] - last}
    if name == "terminal_tabs":
        return [{k: tab.get(k) for k in TABS_EXACT} for tab in value or []]
    return value


def panel_full(dump: dict) -> bool:
    """True when the terminal's visible area is filled: its first text row has content, or the viewport is scrolled."""
    rows = get_path(dump, ("terminal", "text")) or []
    top = get_path(dump, ("terminal", "viewport", "top")) or 0
    return bool(rows and rows[0].strip()) or top > 0


def compare_dump(rriter: dict, ronsole: dict, keys: set[str] | None, expected: dict[str, tuple[int, str]]) -> list[str]:
    """Returns DIFF messages; expected differences go to `expected` as name -> (count, first message)."""
    diffs: list[str] = []
    for name, rriter_path, ronsole_path, mode in PATH_MAP:
        if keys is not None and name not in keys:
            continue
        left = dump_view(name, rriter, rriter_path)
        right = dump_view(name, ronsole, ronsole_path)
        message = f"dump {name}: rriter={json.dumps(left)} ronsole={json.dumps(right)}"
        if name == "text" and len(left) != len(right):
            # The shorter panel shows the bottom rows of the same content: compare those, count the rest.
            # That is only expected when the shorter side's visible area is full; otherwise rows are missing.
            shown = min(len(left), len(right))
            short = rriter if len(left) < len(right) else ronsole
            if shown == 0 or not panel_full(short):
                diffs.append(message)
                continue
            count, first = expected.get(name, (0, message))
            expected[name] = (count + 1, first)
            left, right = left[len(left) - shown:], right[len(right) - shown:]
        if name == "text":
            # Different XTVERSION names are the expected difference; the masked rows still have to match.
            left_names = xtversion_names(get_path(rriter, rriter_path) or [])
            right_names = xtversion_names(get_path(ronsole, ronsole_path) or [])
            if left_names != right_names:
                count, first = expected.get("xtversion", (0, message))
                expected["xtversion"] = (count + 1, first)
        if left == right:
            continue
        if mode == "exact":
            diffs.append(message)
        else:
            count, first = expected.get(name, (0, message))
            expected[name] = (count + 1, first)
    return diffs


def fmt_ink(value) -> str:
    if value is None:
        return "no ink"
    top, bottom, centre = value
    return f"rows {top:g}..{bottom:g} centre {centre:g}"


def fmt_line(value: Line | None) -> str:
    if value is None:
        return "no line"
    return (
        f"thickness {value.thickness:g} px, top {value.top:+.2f} px, bottom {value.bottom:+.2f} px from baseline, "
        f"centre {value.offset:+.4f} char_h from baseline (char_h {value.char_h:g})"
    )


def compare_pixels(left: list[Measure], right: list[Measure], info: list[str]) -> list[str]:
    """Adds one info line per pixel measurement to `info` and returns the DIFF messages.

    Glyph pixels and ink rows are report-only; `pixel_bg` is checked within each app; lines are
    compared across apps, or checked for absence, and underline edges are checked against the baseline.
    """
    diffs: list[str] = []
    for (kind, label, a), (_, _, b) in zip(left, right):
        if kind == "pixel":
            info.append(f"pixel {label} (report-only): rriter={to_hex(a)} ronsole={to_hex(b)}")
        elif kind == "pixel_bg":
            for app, (cell, background) in (("rriter", a), ("ronsole", b)):
                info.append(f"pixel_bg {label} {app}: cell={to_hex(cell)} empty={to_hex(background)}")
                if not rgb_close(cell, background, PIXEL_TOLERANCE):
                    diffs.append(
                        f"pixel_bg {label} {app}: cell={to_hex(cell)} background={to_hex(background)} "
                        f"(tolerance {PIXEL_TOLERANCE})"
                    )
        elif kind in LINE_KINDS:
            if kind == "line_absent":
                verdict = "MATCH" if a is None and b is None else "DIVERGE"
            elif a is None or b is None:
                verdict = "DIVERGE"
            else:
                same = abs(a.thickness - b.thickness) <= LINE_TOLERANCE_PX and abs(a.offset - b.offset) * a.char_h <= LINE_TOLERANCE_PX
                verdict = "MATCH" if same else "DIVERGE"
            info.append(f"{kind} {label}: rriter={fmt_line(a)} ronsole={fmt_line(b)} -> {verdict}")
            if verdict == "DIVERGE":
                diffs.append(f"{kind} {label}: rriter={fmt_line(a)} ronsole={fmt_line(b)} (tolerance {LINE_TOLERANCE_PX} px)")
            if UNDERLINE_LABEL in label:
                for app, value in (("rriter", a), ("ronsole", b)):
                    if value is not None and value.top < 0:
                        diffs.append(
                            f"line {label} {app}: top edge {value.top:+.2f} px above layout.baseline "
                            "(spec L12: the underline top is not above the baseline)"
                        )
        elif kind == "ink_weight":
            info.append(f"ink_weight {label} (report-only): rriter={a} ronsole={b}")
        else:
            info.append(f"ink_rows {label} (report-only): rriter={fmt_ink(a)} ronsole={fmt_ink(b)}")
    return diffs


def find_measure(measures: list[Measure], kinds: tuple[str, ...], label: str):
    """The value of the measurement with that label and one of those kinds; a missing label ends the run."""
    for kind, name, value in measures:
        if name == label and kind in kinds:
            return value
    print(f"no measurement labelled '{label}' ({'/'.join(kinds)}) in the scenario", file=sys.stderr)
    sys.exit(2)


def app_checks(lines: list[str], measures: dict[str, list[Measure]]) -> tuple[list[str], list[str]]:
    """Within-app directives: each app's own measurements against each other. Returns (DIFF lines, info lines)."""
    diffs: list[str] = []
    info: list[str] = []
    for line in lines:
        centre = CENTRE_WITHIN.fullmatch(line)
        glyph = SAME_GLYPH.fullmatch(line)
        bolder = BOLDER.fullmatch(line)
        drawn = DRAWN.fullmatch(line)
        if centre:
            line_label, ink_label = centre.group(1), centre.group(2)
            tolerance = int(centre.group(3) or 1)
            for app, app_measures in measures.items():
                strike = find_measure(app_measures, LINE_KINDS, line_label)
                ink = find_measure(app_measures, ("ink_rows",), ink_label)
                if strike is None or ink is None:
                    diffs.append(f"{app} centre_within {line_label} {ink_label}: no line or no ink to compare")
                    continue
                # Ink rows are inclusive pixel indexes; the centre of their continuous span is (top + bottom + 1) / 2.
                gap = strike.centre - (ink[0] + ink[1] + 1) / 2
                info.append(f"centre_within {app} {line_label} vs {ink_label}: gap {gap:+.2f} px (tolerance {tolerance} px)")
                if abs(gap) > tolerance:
                    diffs.append(f"{app} centre_within {line_label} vs {ink_label}: gap {gap:+.2f} px > {tolerance} px")
        elif glyph:
            regular, bold = glyph.group(1), glyph.group(2)
            for app, app_measures in measures.items():
                rows_regular = find_measure(app_measures, ("ink_rows",), regular)
                rows_bold = find_measure(app_measures, ("ink_rows",), bold)
                pixel_regular = find_measure(app_measures, ("pixel",), regular)
                pixel_bold = find_measure(app_measures, ("pixel",), bold)
                same_rows = rows_regular == rows_bold
                same_pixel = rgb_close(pixel_regular, pixel_bold, PIXEL_TOLERANCE)
                info.append(
                    f"same_glyph {app} {regular} vs {bold}: ink {fmt_ink(rows_regular)} / {fmt_ink(rows_bold)}, "
                    f"pixel {to_hex(pixel_regular)} / {to_hex(pixel_bold)}"
                )
                if not (same_rows and same_pixel):
                    diffs.append(f"{app} same_glyph {regular} vs {bold}: ink rows or centre pixel differ")
        elif bolder:
            bold_label, regular_label = bolder.group(1), bolder.group(2)
            for app, app_measures in measures.items():
                area_bold = find_measure(app_measures, ("ink_weight",), bold_label)
                area_regular = find_measure(app_measures, ("ink_weight",), regular_label)
                info.append(f"bolder {app} {bold_label} vs {regular_label}: ink weight {area_bold} / {area_regular}")
                if not area_bold > area_regular:
                    diffs.append(f"{app} bolder {bold_label} vs {regular_label}: ink weight {area_bold} not above {area_regular}")
        elif drawn:
            for app, app_measures in measures.items():
                area = find_measure(app_measures, ("ink_weight",), drawn.group(1))
                info.append(f"drawn {app} {drawn.group(1)}: ink weight {area}")
                if not area:
                    diffs.append(f"{app} drawn {drawn.group(1)}: no ink (the character is not drawn)")
    return diffs, info


def frame_checks(lines: list[str], replies: dict[str, list[tuple[str, object, RowRef | None]]]) -> tuple[list[str], list[str]]:
    """`#@frames_grew` directives: the frame request counter must rise between the two dumps, in each app."""
    diffs: list[str] = []
    info: list[str] = []
    for line in lines:
        grew = FRAMES_GREW.fullmatch(line)
        if not grew:
            continue
        before, after = int(grew.group(1)), int(grew.group(2))
        for app, entries in replies.items():
            dumps = [reply for _, reply, _ in entries if isinstance(reply, dict) and "event_loop" in reply]
            if not (1 <= before <= len(dumps) and 1 <= after <= len(dumps)):
                diffs.append(f"{app} frames_grew {before} {after}: only {len(dumps)} dumps with event_loop")
                continue
            first = get_path(dumps[before - 1], ("event_loop", "frame_requests"))
            second = get_path(dumps[after - 1], ("event_loop", "frame_requests"))
            info.append(f"frames_grew {app}: frame_requests {first} -> {second}")
            if first is None or second is None or second <= first:
                diffs.append(f"{app} frames_grew {before} {after}: frame_requests {first} -> {second} did not rise")
    return diffs, info


def mask_osc11(match: re.Match) -> str:
    """An OSC 11 reply with its colour components zeroed, same lengths; the `/` separators stay as they are."""
    red, green, blue = match.groups()
    return "0" * len(red) + "2f" + "0" * len(green) + "2f" + "0" * len(blue)


def osc11_colour(rows: list[str]) -> str | None:
    """Colour of the OSC 11 reply printed in a dump's text, as `#rrggbb`; None when no reply is in the rows."""
    for row in rows:
        match = OSC11_COLOUR_HEX.search(row)
        if match:
            # The reply's colour is ASCII `rrrr/gggg/bbbb`; the first two digits of each part are the 8-bit value.
            parts = [bytes.fromhex(part).decode("ascii", "replace") for part in match.groups()]
            return "#" + "".join(part[:2] for part in parts)
    return None


def last_osc11_colour(replies: list[tuple[str, object, RowRef | None]]) -> str | None:
    """OSC 11 colour in the last full `dump` of one app's replies; None without a dump or a reply in it."""
    last_dump = next((reply for _, reply, _ in reversed(replies) if isinstance(reply, dict) and "terminal" in reply), None)
    return osc11_colour(get_path(last_dump, ("terminal", "text")) or []) if last_dump else None


def inverse_fg(replies: list[tuple[str, object, str | None]]) -> str | None:
    """The fg of the first inverse cell in the app's `dump row` replies; None when there is none."""
    for _, reply, _ in replies:
        if isinstance(reply, dict):
            for cell in reply.get("cells", []):
                if "inverse" in (cell.get("flags") or []):
                    # A null fg (the default colour) is reported as "default", so it fails the self-check.
                    return cell.get("fg") or "default"
    return None


def row_key(ref: RowRef | None) -> str | None:
    """The row text that decides whether two rows are compared cell by cell; None without a row reference."""
    return None if ref is None else norm_row(ref[0])


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("scenario", type=Path)
    parser.add_argument("--keys", help="comma-separated dump paths to compare, default all")
    parser.add_argument("--size", default="1280x800")
    parser.add_argument("--scale", default="1.0")
    parser.add_argument("--ronsole-dir", type=Path, default=DEFAULT_RONSOLE_DIR)
    ns = parser.parse_args(argv)
    keys = set(ns.keys.split(",")) if ns.keys else None
    lines = parse_scenario(ns.scenario.read_text(encoding="utf-8"))

    # (command, reply, row_text) per command the scenario itself wrote; marker clicks are not compared.
    replies: dict[str, list[tuple[str, object, RowRef | None]]] = {}
    px_dir = Path(tempfile.mkdtemp(prefix="term-parity-px-"))
    measures: dict[str, list[Measure]] = {"rriter": [], "ronsole": []}
    for app in ("rriter", "ronsole"):
        entries = [
            item
            for item in resolve_markers(app, ns.ronsole_dir, ns.size, ns.scale, lines, px_dir, measures[app])
            if not item[0].startswith("#@")
        ]
        commands = [command for command, _, _ in entries]
        raw = run_driver(app, ns.ronsole_dir, ns.size, ns.scale, commands)
        print(f"{app}: {len(commands)} commands, {len(raw)} replies")
        if len(raw) != len(commands):
            print(f"{app}: reply count {len(raw)} != command count {len(commands)}", file=sys.stderr)
            sys.exit(2)
        replies[app] = [
            (command, comparable(command, parse_reply(reply)), row_text)
            for (command, injected, row_text), reply in zip(entries, raw)
            if not injected
        ]

    diffs: list[str] = []
    link_info: list[str] = []
    expected: dict[str, tuple[int, str]] = {}
    left_all = replies["rriter"]
    right_all = replies["ronsole"]
    if len(left_all) != len(right_all):
        diffs.append(f"reply count: rriter={len(left_all)} ronsole={len(right_all)}")
    # Each app's host background from its last dump: the default colour that inverse cells show.
    colours = {app: last_osc11_colour(replies[app]) for app in ("rriter", "ronsole")}
    # Widths from the latest `dump` of each app; a row reply is placed by them.
    left_cols = right_cols = None
    for index, ((command, left, left_ref), (_, right, right_ref)) in enumerate(zip(left_all, right_all)):
        if isinstance(left, dict) and isinstance(right, dict) and "cells" in left and "cells" in right:
            left_text, right_text = row_key(left_ref), row_key(right_ref)
            if left_ref and right_ref and (left_ref[1] or right_ref[1]):
                row_diffs, row_info = compare_link_row(f"reply {index}", left, right, left_ref, right_ref)
                diffs += row_diffs
                link_info += row_info
            elif left_text != right_text:
                # A row_of row must be the same line in both apps; a different line is a DIFF, never a silent skip.
                diffs.append(f"reply {index}: row text differs, cells not compared (rriter={left_text!r} ronsole={right_text!r})")
            else:
                host_colours = (colours["rriter"], colours["ronsole"])
                diffs += compare_row(f"reply {index}", left, right, left_cols, right_cols, expected, host_colours)
        elif isinstance(left, dict) and isinstance(right, dict) and "terminal" in left and "terminal" in right:
            diffs += compare_dump(left, right, keys, expected)
            left_cols = get_path(left, ("terminal", "cols"))
            right_cols = get_path(right, ("terminal", "cols"))
        elif command.split()[0] in HOST_PIXEL_COMMANDS and left == right == "ok":
            count, first = expected.get("mouse_cell", (0, f"reply {index} {command}"))
            expected["mouse_cell"] = (count + 1, first)
        elif left != right:
            diffs.append(f"reply {index}: rriter={left!r} ronsole={right!r}")

    pixel_info: list[str] = []
    diffs += compare_pixels(measures["rriter"], measures["ronsole"], pixel_info)
    shutil.rmtree(px_dir, ignore_errors=True)
    check_diffs, check_info = app_checks(lines, measures)
    frame_diffs, frame_info = frame_checks(lines, replies)
    diffs += check_diffs + frame_diffs
    pixel_info += check_info + frame_info

    # Palette: each app's inverse default colour must be its own OSC 11 colour; across apps it differs by design.
    palette_info: list[str] = []
    for app in ("rriter", "ronsole"):
        colour = colours[app]
        inverse = inverse_fg(replies[app])
        palette_info.append(f"palette {app}: OSC 11 {colour}, inverse fg {inverse}")
        if colour is None and inverse is not None:
            diffs.append(f"palette {app}: inverse cell present but no OSC 11 reply in the last dump to check it against")
        elif colour is not None and inverse is not None and inverse.lower() != colour:
            diffs.append(f"palette self-check {app}: inverse fg {inverse} != OSC 11 {colour}")
    if colours["rriter"] and colours["ronsole"] and colours["rriter"] != colours["ronsole"]:
        count, first = expected.get("palette", (0, ""))
        expected["palette"] = (count + 1, first or f"OSC 11 rriter={colours['rriter']} ronsole={colours['ronsole']}")

    report = [*palette_info, *pixel_info]
    report += [f"expected: {name} x{count} ({EXPECTED_DIFF[name]}); first: {first}" for name, (count, first) in expected.items()]
    report += link_info
    report += [f"DIFF: {line}" for line in diffs]
    DIFF_DIR.mkdir(parents=True, exist_ok=True)
    report_path = DIFF_DIR / f"{ns.scenario.stem}.diff"
    report_path.write_text("\n".join(report) + "\n", encoding="utf-8")
    print(
        f"{ns.scenario.name}: compared {min(len(left_all), len(right_all))} replies, {len(diffs)} DIFF, "
        f"{sum(count for count, _ in expected.values())} expected, {len(pixel_info)} pixel lines; "
        f"full report: {report_path}"
    )
    return 0 if not diffs else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
