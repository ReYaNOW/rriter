#!/usr/bin/env python3
"""List UI color literals and editor-theme field references for theme migration."""

from __future__ import annotations

import argparse
import re
import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
EDITOR_FILES = {
    "src/app/app_bootstrap.rs",
    "src/app/app_state.rs",
    "src/app/terminal.rs",
    "src/startup_environment.rs",
    "src/renderer/renderer_types.rs",
    "src/theme.rs",
    "src/render_view/root_frame_layout_renderer.rs",
    "src/render_view/markdown_read.rs",
    "src/render_view/markdown_read_interaction.rs",
    "src/render_view/markdown_read_interaction_tests.rs",
    "src/render_view/terminal_ui.rs",
    "src/render_view/root_helpers.rs",
    "src/render_view/editor_text_layer.rs",
    "src/render_view/root_frame_helpers.rs",
    "src/render_view/root_frame_editor_text_renderer.rs",
    "src/render_view/root_frame_editor_chrome_renderer.rs",
    "src/render_view/sticky.rs",
}
TASK_FILES = {
    8: lambda p: p.startswith("src/render_view/ide_panels/"),
    9: lambda p: p.startswith("src/render_view/api_client_tab/")
    or p.startswith("src/render_view/api_client_panel/"),
    10: lambda p: p.startswith("src/render_view/database_"),
    11: lambda p: p in {
        "src/render_view/settings_ui.rs",
        "src/render_view/settings_database_ui.rs",
        "src/render_view/settings_keymap_ui.rs",
        "src/render_view/settings_tool_rows.rs",
    },
    12: lambda p: p == "src/render_view/lsp_ui.rs"
    or p.startswith("src/render_view/ui/")
    or p.startswith("src/lsp/"),
}
ARRAY = re.compile(r"\[([^\[\]]*)\]", re.DOTALL)
CHANNEL = re.compile(
    r"(?:\d+(?:\.\d+)?(?:f32)?|[A-Z][A-Z_0-9]*)(?:\s*/\s*255\.0)?"
)
BYTE_CHANNEL = re.compile(r"(?:[A-Za-z_][A-Za-z_0-9]*|\d+(?:\.\d+)?)(?:\s*/\s*255\.0)")
WHITE = re.compile(r"\[\s*1\.0\s*,\s*1\.0\s*,\s*1\.0\s*,\s*([^\]]+)\]")
THEME_FIELD = re.compile(r"\btheme\.([A-Za-z_][A-Za-z_0-9]*)")
DRACULA_FIELDS = re.compile(r"^\s*([A-Za-z_][A-Za-z_0-9]*):\s*(\[[^\]]*\]),?$", re.MULTILINE)
EXCEPTIONS: dict[tuple[float, ...], str] = {}


def is_test_file(path: Path) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    return (
        path.stem.endswith("_tests")
        or path.stem == "tests"
        or path.stem.startswith("ui_tests_")
        or any(part == "tests" or part.endswith("_tests") for part in path.parts)
        or "headless" in path.parts
    )


def is_color_array(content: str) -> bool:
    parts = [part.strip() for part in content.split(",")]
    if len(parts) != 4:
        return False
    if not all(CHANNEL.fullmatch(part) or BYTE_CHANNEL.fullmatch(part) for part in parts[:3]):
        return False
    if not parts[3]:
        return False
    for part in parts[:3]:
        if "/255.0" not in part:
            number = re.fullmatch(r"(\d+(?:\.\d+)?)(?:f32)?", part)
            if number and float(number.group(1)) > 1.0:
                return False
    return True


def is_ui_path(rel: str) -> bool:
    return (
        rel.startswith(("src/render_view/", "src/lsp/"))
        or rel == "src/widgets.rs"
        or rel.startswith((
            "src/app/api_client/",
            "src/app/keymap_settings.rs",
            "src/app/lsp_actions.rs",
            "src/app/mouse/",
            "src/app/project_search",
            "src/app/ui_handlers/",
        ))
    )


def scan_file(path: Path):
    rel = path.relative_to(ROOT).as_posix()
    if rel in EDITOR_FILES or rel.startswith("src/theme") or is_test_file(path):
        return
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (UnicodeDecodeError, OSError):
        return

    visible = list(lines)
    test_module = re.compile(r"^\s*mod\s+[A-Za-z_][A-Za-z_0-9]*\s*\{")
    for index, line in enumerate(lines):
        if "#[cfg(test)]" not in line:
            continue
        module_index = index if test_module.match(line.strip().split("#[cfg(test)]", 1)[-1].strip()) else index + 1
        if module_index >= len(lines) or not test_module.match(lines[module_index].strip()):
            continue
        indent = lines[module_index][: len(lines[module_index]) - len(lines[module_index].lstrip())]
        close = next(
            (
                end
                for end in range(module_index + 1, len(lines))
                if re.fullmatch(re.escape(indent) + r"\}\s*(?://.*)?", lines[end])
            ),
            None,
        )
        if close is not None:
            for hidden_index in range(index, close + 1):
                visible[hidden_index] = ""
    if rel.startswith(("src/render_view/", "src/lsp/", "src/widgets.rs")):
        for number, line in enumerate(visible, 1):
            for match in THEME_FIELD.finditer(line):
                yield rel, number, "theme." + match.group(1), line.strip(), "theme-field"
    visible_source = "\n".join(visible)
    for match in ARRAY.finditer(visible_source):
        parts = [part.strip() for part in match.group(1).split(",")]
        color_like = is_color_array(match.group(1))
        if color_like:
            number = visible_source.count("\n", 0, match.start()) + 1
            value = "[" + ", ".join(parts) + "]"
            context = lines[number - 1].strip()
            yield rel, number, value, context, "literal"


def group_for(rel: str) -> int:
    for task, matches in TASK_FILES.items():
        if matches(rel):
            return task
    return 13


def f32(value: float) -> float:
    return struct.unpack("!f", struct.pack("!f", value))[0]


def parse_color(value: str) -> tuple[float, ...] | None:
    parts = [part.strip() for part in value.strip()[1:-1].split(",")]
    if len(parts) != 4:
        return None
    result = []
    for part in parts:
        match = re.fullmatch(r"(\d+(?:\.\d+)?)(?:f32)?(?:\s*/\s*255\.0)?", part)
        if not match:
            return None
        channel = f32(float(match.group(1)))
        if "/" in part:
            channel = f32(channel / f32(255.0))
        result.append(channel)
    return tuple(result)


def dracula_values() -> set[tuple[float, ...]]:
    source = (ROOT / "src/theme.rs").read_text(encoding="utf-8")
    constructor = source.split("let mut ui = Self {", 1)[1].split("\n        };", 1)[0]
    values = set()
    for match in DRACULA_FIELDS.finditer(constructor):
        parsed = parse_color(match.group(2))
        if parsed is not None:
            values.add(parsed)
    values.add((0.0, 0.0, 0.0, f32(1.0)))
    return values


def is_shadow_place(rel: str, line: int, context: str) -> bool:
    if "shadow" in context.lower():
        return True
    source = (ROOT / rel).read_text(encoding="utf-8").splitlines()
    function = next(
        (row for row in reversed(source[:line]) if re.search(r"\bfn\s+[A-Za-z_][A-Za-z_0-9]*", row)),
        "",
    )
    return "shadow" in function.lower()


def coverage(records: list[tuple[str, int, str, str, str]]) -> int:
    roles = dracula_values()
    grouped: dict[tuple[float, ...], list[tuple[str, int]]] = {}
    for rel, line, value, context, kind in records:
        if not is_ui_path(rel) or kind != "literal" or not 8 <= group_for(rel) <= 13:
            continue
        parsed = parse_color(value)
        if parsed is None:
            if value.startswith("[1.0, 1.0, 1.0,"):
                continue
            if value.startswith("[0.0, 0.0, 0.0,") and is_shadow_place(rel, line, context):
                continue
            continue
        if parsed[:3] == (f32(1.0), f32(1.0), f32(1.0)):
            continue
        if parsed in roles or parsed in EXCEPTIONS:
            continue
        grouped.setdefault(parsed, []).append((rel, line))
    for value, places in sorted(grouped.items()):
        locations = ", ".join(f"{rel}:{line}" for rel, line in places[:3])
        print(f"{value} | count={len(places)} | {locations}")
    return 1 if grouped else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--coverage", action="store_true")
    parser.add_argument("--files", nargs="*")
    args = parser.parse_args()
    files = [ROOT / item for item in args.files] if args.files else sorted(SRC.rglob("*.rs"))
    records = [record for path in files if path.is_file() for record in scan_file(path)]
    if args.coverage:
        return coverage(records)
    if args.check:
        exceptions: set[tuple[str, str]] = set()
        remaining = [r for r in records if is_ui_path(r[0]) and r[4] == "literal" and (r[0], r[2]) not in exceptions]
        white = []
        for path in files:
            if not path.is_file() or is_test_file(path):
                continue
            rel = path.relative_to(ROOT).as_posix()
            if not is_ui_path(rel) or rel in EDITOR_FILES or rel.startswith("src/theme"):
                continue
            source = path.read_text(encoding="utf-8")
            lines = source.splitlines()
            for match in WHITE.finditer(source):
                number = source.count("\n", 0, match.start()) + 1
                white.append((rel, number, match.group(0), lines[number - 1].strip()))
        for rel, line, value, context, _ in remaining:
            print(f"{rel}:{line}: {value} | {context}")
        for rel, line, value, context in white:
            print(f"WHITE {rel}:{line}: {value} | {context}")
        return 1 if remaining or white else 0
    for rel, line, value, context, kind in records:
        print(f"{rel}:{line}: {value} | {context}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
