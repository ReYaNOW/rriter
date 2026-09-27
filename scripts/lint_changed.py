#!/usr/bin/env python3
"""Keep changed Rust files from adding Clippy warnings or known bad patterns."""

import json
import os
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
BASELINE_PATH = ROOT / "scripts/lint_baseline.json"
MAX_LINES = 1600
CLIPPY_COMMAND = ["cargo", "+nightly", "clippy", "--all-targets", "--message-format=json"]
PATTERNS = (
    ("global-lock", re.compile(r"\bstatic(?:\s+mut)?\s+\w+\s*:\s*[^;]*(?:Mutex|RwLock|OnceLock|LazyLock)\b")),
    ("app-methods-impl", re.compile(r"\bimpl\s+(?:\w+::)*App(?:\s*<[^>]+>)?\s*\{?")),
    ("git-command", re.compile(r"\bCommand\s*::\s*new\s*\(\s*\"git\"")),
)
RENDER_ACCUMULATION = re.compile(r"\b(?:cy|y|row_y)\s*\+=\s*[^;\n]*\*\s*s\b")


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
    for name in untracked:
        if not name.endswith(".rs"):
            continue
        file_path = ROOT / name
        try:
            lines = file_path.read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeError):
            continue
        result.extend((name, index, content) for index, content in enumerate(lines, 1))
    return result


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
    value = {
        "clippy": {
            path: {lint: count for (file, lint), count in sorted(counts.items()) if file == path}
            for path in sorted({file for file, _ in counts})
        },
        "file_length": {path: size for path, size in sorted(lengths.items()) if size > MAX_LINES},
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
    for path, line_no, text in added_lines(base, untracked):
        if path.startswith("src/app/") and PATTERNS[0][1].search(text):
            violations.append((path, line_no, PATTERNS[0][0], "new global static holds a lock"))
        if path.rsplit("/", 1)[-1].endswith("_methods.rs") and PATTERNS[1][1].search(text):
            violations.append((path, line_no, PATTERNS[1][0], "App implementation belongs in its feature state or routing module"))
        if path.startswith("src/render_view/") and RENDER_ACCUMULATION.search(text) and ".round()" not in text:
            violations.append((path, line_no, "fractional-baseline", "avoid accumulating scaled text baselines"))
        if path != "src/app/git_panel/git_process.rs" and PATTERNS[2][1].search(text):
            violations.append((path, line_no, PATTERNS[2][0], "route Git CLI work through git_process.rs"))

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
