#!/usr/bin/env python3
"""Keep changed Rust files from adding Clippy warnings or known bad patterns."""

import json
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
BASELINE_PATH = ROOT / "scripts/lint_baseline.json"
MAX_LINES = 1600
CLIPPY_COMMAND = ["cargo", "+nightly", "clippy", "--all-targets", "--message-format=json"]
PATTERNS = (
    ("global-lock", re.compile(r"\bstatic(?:\s+mut)?\s+\w+\s*:\s*[^;]*(?:Mutex|RwLock|OnceLock|Lazy|LazyLock)\b")),
    ("thread-local", re.compile(r"\bthread_local!\s*\{")),
    ("app-methods-impl", re.compile(r"\bimpl\s+(?:\w+::)*App(?:\s*<[^>]+>)?\s*\{?")),
    ("git-command", re.compile(r"\bCommand\s*::\s*new\s*\(\s*\"git\"")),
)
RENDER_ACCUMULATION = re.compile(r"\b(?:cy|y|row_y)\s*\+=\s*[^;\n]*\*\s*s\b")
# Glyph bitmaps must be placed through the core_text helpers (`push_ui_glyph_at_scale`,
# `push_scaled_glyph`, ...): they draw the glyph rasterized at the final pixel size 1:1.
# Scaling a glyph quad or its metrics by hand resamples the bitmap and rounds each glyph's
# top differently (letters jump vertically at fractional display scales). Only the helper
# modules may do it; elsewhere a deliberate exception needs the marker on the line or the
# line above, e.g. `// lint: subpixel-glyph-ok hover zoom animation`.
SCALED_GLYPH_EXEMPT = {
    "src/render_view/core_text.rs",
    "src/render_view/core_text_editor_helpers.rs",
    "src/renderer/geometry.rs",
}
SUBPIXEL_GLYPH_MARKER = "lint: subpixel-glyph-ok"
GLYPH_RECT_CALL = re.compile(r"(?<!fn )\b(?:glyph_quad_rect|pixel_stable_glyph_rect)\s*\(")
GLYPH_METRIC = re.compile(r"\boffset_[xy]\b|\b(?:g|gi|glyph|\w+_glyph|glyph_\w+)\.(?:width|height)\b")
MULTIPLY_BEFORE = re.compile(r"[\w)\]]\s*\*$")


def run(args, *, text=True):
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=text)


def merge_base():
    return run(["git", "merge-base", "HEAD", "origin/master"]).stdout.strip()


def rust_paths(paths):
    return sorted({p for p in paths if p.endswith(".rs")})


def changed_files(base):
    tracked = run(["git", "diff", "--name-only", base, "--"]).stdout.splitlines()
    untracked = run(["git", "ls-files", "--others", "--exclude-standard"]).stdout.splitlines()
    return rust_paths(tracked + untracked)


def added_lines(base, untracked):
    diff = run(["git", "diff", "--no-ext-diff", "--no-renames", "--unified=0", base, "--"]).stdout
    result = []
    removed = Counter()
    path = None
    line_no = 0
    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            path = line[6:]
        elif line.startswith("@@"):
            match = re.search(r"\+(\d+)(?:,(\d+))?", line)
            if match:
                line_no = int(match.group(1))
        elif line.startswith("+") and not line.startswith("+++"):
            if path and path.endswith(".rs"):
                result.append((path, line_no, line[1:]))
            line_no += 1
        elif line.startswith(" "):
            line_no += 1
        elif line.startswith("-") and not line.startswith("---") and path and path.endswith(".rs"):
            removed[line[1:].strip()] += 1
    for name in untracked:
        if not name.endswith(".rs"):
            continue
        file_path = ROOT / name
        try:
            lines = file_path.read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeError):
            continue
        result.extend((name, index, content) for index, content in enumerate(lines, 1))
    # A line removed elsewhere in the same diff was moved, not written anew (file splits).
    fresh = []
    for entry in result:
        key = entry[2].strip()
        if removed[key] > 0:
            removed[key] -= 1
        else:
            fresh.append(entry)
    return fresh


def _code_part(line):
    return line.split("//", 1)[0]


def _is_multiplied(code, start, end):
    """True when code[start:end] is a `*` operand, directly or inside an enclosing (...) group."""
    while start > 0 and (code[start - 1].isalnum() or code[start - 1] in "_."):
        start -= 1
    spans = [(start, end)]
    depth = 0
    for index in range(start - 1, -1, -1):
        if code[index] == ")":
            depth += 1
        elif code[index] == "(":
            if depth:
                depth -= 1
                continue
            close, inner = len(code), 0
            for probe in range(index, len(code)):
                if code[probe] == "(":
                    inner += 1
                elif code[probe] == ")":
                    inner -= 1
                    if inner == 0:
                        close = probe + 1
                        break
            open_at = index
            # `foo(..)` / `a.max(..)` is one operand together with its callee path.
            while open_at > 0 and (code[open_at - 1].isalnum() or code[open_at - 1] in "_.:"):
                open_at -= 1
            spans.append((open_at, close))
    return any(
        code[span_end:].lstrip().startswith("*") or MULTIPLY_BEFORE.search(code[:span_start].rstrip())
        for span_start, span_end in spans
    )


def _call_args(text, open_paren):
    """Top-level arguments of the call whose `(` is at `open_paren`, and the index after `)`."""
    args, depth, current = [], 0, []
    for index in range(open_paren, len(text)):
        char = text[index]
        if char in "([{":
            depth += 1
            if depth == 1:
                continue
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                args.append("".join(current))
                return [arg.strip() for arg in args if arg.strip()], index + 1
        elif char == "," and depth == 1:
            args.append("".join(current))
            current = []
            continue
        if depth >= 1:
            current.append(char)
    return None, len(text)


def scaled_glyph_violations(path, lines, added):
    """`(line_no, message)` for hand-scaled glyph placement touching the `added` line numbers."""
    if not path.startswith("src/") or path in SCALED_GLYPH_EXEMPT:
        return []

    def allowed(line_no, last_line_no=None):
        window = lines[max(line_no - 2, 0):(last_line_no or line_no)]
        return any(SUBPIXEL_GLYPH_MARKER in line for line in window)

    found = []
    for line_no in sorted(added):
        if not 0 < line_no <= len(lines):
            continue
        code = _code_part(lines[line_no - 1])
        if any(_is_multiplied(code, m.start(), m.end()) for m in GLYPH_METRIC.finditer(code)) and not allowed(line_no):
            found.append((line_no, "glyph metric scaled by hand; use ui_glyph_at_scale / ScaledGlyph metrics"))
    text = "\n".join(_code_part(line) for line in lines)
    for match in GLYPH_RECT_CALL.finditer(text):
        args, end = _call_args(text, match.end() - 1)
        if not args or args[-1] == "1.0":
            continue
        first = text.count("\n", 0, match.start()) + 1
        last = text.count("\n", 0, end) + 1
        if any(first <= line_no <= last for line_no in added) and not allowed(first, last):
            found.append((first, "glyph quad scaled by hand; use push_ui_glyph_at_scale / push_scaled_glyph"))
    return found


def source_line_counts():
    counts = {}
    locations = {}
    for path in ROOT.rglob("*.rs"):
        if any(part in {"target", ".git"} for part in path.parts):
            continue
        try:
            counts[path.relative_to(ROOT).as_posix()] = len(path.read_text(encoding="utf-8").splitlines())
        except (OSError, UnicodeError):
            continue
    return counts


def clippy_counts():
    env = os.environ.copy()
    env.setdefault("CARGO_BUILD_JOBS", "4")
    proc = subprocess.run(CLIPPY_COMMAND, cwd=ROOT, env=env, capture_output=True, text=True)
    if proc.returncode:
        sys.stderr.write(proc.stderr)
        # Known deny errors in untouched files still end with build-finished;
        # without it Clippy never ran (no nightly, broken config) and counts are empty.
        if '"reason":"build-finished"' not in proc.stdout:
            raise SystemExit(f"lint: clippy failed to run (exit {proc.returncode})")
    return parse_clippy_output(proc.stdout)


def parse_clippy_output(output):
    counts = {}
    locations = {}
    errors = []
    root = ROOT.resolve()
    for line in output.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "compiler-message":
            continue
        diagnostic = message.get("message", {})
        code = diagnostic.get("code") or {}
        lint = code.get("code", "")
        if diagnostic.get("level") not in {"warning", "error"}:
            continue
        primary = next((span for span in diagnostic.get("spans", []) if span.get("is_primary")), None)
        if primary is None:
            continue
        filename = Path(primary.get("file_name", ""))
        if not filename.is_absolute():
            filename = (root / filename).resolve()
        try:
            relative = filename.resolve().relative_to(root).as_posix()
        except (OSError, ValueError):
            continue
        if not relative.endswith(".rs"):
            continue
        if diagnostic.get("level") == "error":
            errors.append((relative, primary.get("line_start", 1), diagnostic.get("message", "Clippy compilation error")))
            continue
        if not lint.startswith("clippy::"):
            continue
        key = (relative, lint.removeprefix("clippy::"))
        counts[key] = counts.get(key, 0) + 1
        locations.setdefault(key, primary.get("line_start", 1))
    return counts, locations, errors


def write_baseline(counts, lengths):
    patterns = load_baseline().get("patterns", {})
    value = {
        "clippy": {
            path: {lint: count for (file, lint), count in sorted(counts.items()) if file == path}
            for path in sorted({file for file, _ in counts})
        },
        "file_length": {path: size for path, size in sorted(lengths.items()) if size > MAX_LINES},
        "patterns": patterns,
    }
    BASELINE_PATH.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def load_baseline():
    try:
        return json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"baseline:1: configuration — {error}")
        return {"clippy": {}, "file_length": {}}


def main():
    update = sys.argv[1:] == ["--update-baseline"]
    if sys.argv[1:] not in ([], ["--update-baseline"]):
        print("usage: lint_changed.py [--update-baseline]", file=sys.stderr)
        return 1
    base = merge_base()
    baseline = {} if update else load_baseline()
    lengths = source_line_counts()
    counts, locations, errors = clippy_counts()
    if update:
        write_baseline(counts, lengths)
        return 0

    changed = changed_files(base)
    violations = []
    violations.extend((path, line_no, "clippy-error", message) for path, line_no, message in errors if path in changed)
    length_baseline = baseline.get("file_length", {})
    for path in changed:
        size = lengths.get(path, 0)
        ceiling = length_baseline.get(path)
        if ceiling is not None and size > ceiling:
            violations.append((path, 1, "file-length", f"{size} lines exceeds baseline ceiling {ceiling}"))
        elif size > MAX_LINES and ceiling is None:
            violations.append((path, 1, "file-length", f"{size} lines exceeds the 1600-line limit"))

    untracked = set(run(["git", "ls-files", "--others", "--exclude-standard"]).stdout.splitlines())
    pattern_counts = {}
    pattern_locations = {}
    fresh_lines = added_lines(base, untracked)
    added_by_path = {}
    for path, line_no, _ in fresh_lines:
        added_by_path.setdefault(path, set()).add(line_no)
    for path, added in added_by_path.items():
        try:
            lines = (ROOT / path).read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeError):
            continue
        for line_no, message in scaled_glyph_violations(path, lines, added):
            violations.append((path, line_no, "scaled-glyph-placement", message))
    for path, line_no, text in fresh_lines:
        if path.startswith("src/"):
            for index in (0, 1):
                if PATTERNS[index][1].search(text):
                    key = (path, PATTERNS[index][0])
                    pattern_counts[key] = pattern_counts.get(key, 0) + 1
                    pattern_locations.setdefault(key, line_no)
            filename = path.rsplit("/", 1)[-1]
            if (filename.endswith("_click_methods.rs") or filename.endswith("_text_methods.rs")) and PATTERNS[2][1].search(text):
                key = (path, PATTERNS[2][0])
                pattern_counts[key] = pattern_counts.get(key, 0) + 1
                pattern_locations.setdefault(key, line_no)
        if path.startswith("src/render_view/") and RENDER_ACCUMULATION.search(text) and ".round()" not in text:
            violations.append((path, line_no, "fractional-baseline", "avoid accumulating scaled text baselines"))
        if path != "src/app/git_panel/git_process.rs" and PATTERNS[3][1].search(text):
            violations.append((path, line_no, PATTERNS[3][0], "route Git CLI work through git_process.rs"))

    pattern_baseline = baseline.get("patterns", {})
    for (path, rule), count in sorted(pattern_counts.items()):
        previous = pattern_baseline.get(path, {}).get(rule, 0)
        if count > previous:
            message = "new global state pattern" if rule in {"global-lock", "thread-local"} else "App implementation belongs in its feature state or routing module"
            violations.append((path, pattern_locations[(path, rule)], rule, message))

    clippy_baseline = baseline.get("clippy", {})
    for (path, lint), count in sorted(counts.items()):
        if path not in changed:
            continue
        previous = clippy_baseline.get(path, {}).get(lint, 0)
        if count > previous:
            violations.append((path, locations.get((path, lint), 1), f"clippy::{lint}", f"{count} warnings exceeds baseline {previous}"))
    for path, line_no, rule, message in sorted(violations):
        print(f"{path}:{line_no}: {rule} — {message}")
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
