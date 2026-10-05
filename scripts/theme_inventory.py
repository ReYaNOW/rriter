#!/usr/bin/env python3
"""List UI color literals and editor-theme field references for theme migration."""

from __future__ import annotations

import argparse
import re
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
ARRAY = re.compile(r"\[([^\[\]]*)\]", re.DOTALL)
CHANNEL = re.compile(
    r"(?:\d+(?:\.\d+)?(?:f32)?|[A-Z][A-Z_0-9]*)(?:\s*/\s*255\.0)?"
)
BYTE_CHANNEL = re.compile(r"(?:[A-Za-z_][A-Za-z_0-9]*|\d+(?:\.\d+)?)(?:\s*/\s*255\.0)")
WHITE = re.compile(r"\[\s*1\.0\s*,\s*1\.0\s*,\s*1\.0\s*,\s*([^\]]+)\]")
THEME_FIELD = re.compile(r"\btheme\.([A-Za-z_][A-Za-z_0-9]*)")
EXCEPTIONS: dict[tuple[str, str], str] = {}


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
                visible[hidden_index] = " " * len(visible[hidden_index])
    visible_source = "\n".join(visible)
    if is_ui_path(rel):
        for match in THEME_FIELD.finditer(visible_source):
            number = visible_source.count("\n", 0, match.start()) + 1
            context = lines[number - 1].strip()
            yield rel, number, "theme." + match.group(1), context, "theme-field", match.start()
    for match in ARRAY.finditer(visible_source):
        parts = [part.strip() for part in match.group(1).split(",")]
        color_like = is_color_array(match.group(1))
        if color_like:
            number = visible_source.count("\n", 0, match.start()) + 1
            value = "[" + ", ".join(parts) + "]"
            context = lines[number - 1].strip()
            yield rel, number, value, context, "literal", match.start()


def is_pick_second_arg(source: str, offset: int) -> bool:
    prefix = source[max(0, offset - 240):offset]
    return re.search(r"\.pick\s*\(\s*UiRole::[A-Za-z_][A-Za-z_0-9]*\s*,\s*[^)]*$", prefix) is not None


def numeric_color(value: str) -> tuple[float, ...] | None:
    if not is_color_array(value):
        return None
    channels = []
    for part in (part.strip() for part in value.strip()[1:-1].split(",")):
        byte = re.fullmatch(r"(\d+(?:\.\d+)?)\s*/\s*255\.0", part)
        number = byte or re.fullmatch(r"(\d+(?:\.\d+)?)(?:f32)?", part)
        if not number:
            return None
        channel = float(number.group(1))
        if byte:
            channel /= 255.0
        channels.append(channel)
    return tuple(channels)


def role_catalog():
    theme = (SRC / "theme.rs").read_text(encoding="utf-8")
    enum = re.search(r"enum UiRole\s*\{(.*?)\n\}", theme, re.S)
    roles_block = re.search(r"roles:\s*\[\s*\n(.*?)\n\s*\],", theme, re.S)
    if not enum or not roles_block:
        return [], {}
    roles = re.findall(r"^\s*([A-Za-z_][A-Za-z_0-9]*)\s*,", enum.group(1), re.M)
    values = [numeric_color(match.group(0)) for match in ARRAY.finditer(roles_block.group(1))]
    return roles, dict(zip(roles, values))


def pick_representatives(files: list[Path]):
    picks: dict[str, Counter] = {}
    pick_pattern = re.compile(r"\.pick\s*\(\s*UiRole::([A-Za-z_][A-Za-z_0-9]*)\s*,")
    for path in files:
        if not path.is_file() or is_test_file(path):
            continue
        rel = path.relative_to(ROOT).as_posix()
        if not is_ui_path(rel):
            continue
        source = path.read_text(encoding="utf-8")
        for match in pick_pattern.finditer(source):
            tail = source[match.end():].lstrip()
            color = ARRAY.match(tail)
            if color and is_color_array(color.group(1)):
                numeric = numeric_color(color.group(0))
                if numeric is not None:
                    picks.setdefault(match.group(1), Counter())[numeric] += 1
    return picks


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--files", nargs="*")
    args = parser.parse_args()
    files = [ROOT / item for item in args.files] if args.files else sorted(SRC.rglob("*.rs"))
    records = [record for path in files if path.is_file() for record in scan_file(path)]
    if args.check:
        remaining = []
        for rel, line, value, context, kind, offset in records:
            if not is_ui_path(rel):
                continue
            path = ROOT / rel
            source = path.read_text(encoding="utf-8")
            if kind == "literal":
                if is_pick_second_arg(source, offset):
                    continue
                if WHITE.fullmatch(value):
                    continue
                if (rel, value) not in EXCEPTIONS:
                    remaining.append((rel, line, value, context))
            elif kind == "theme-field":
                if is_pick_second_arg(source, offset):
                    continue
                if (rel, value) not in EXCEPTIONS:
                    remaining.append((rel, line, value, context))
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
                if is_pick_second_arg(source, match.start()):
                    continue
                number = source.count("\n", 0, match.start()) + 1
                white.append((rel, number, match.group(0), lines[number - 1].strip()))
        for rel, line, value, context in remaining:
            print(f"{rel}:{line}: {value} | {context}")
        for rel, line, value, context in white:
            print(f"WHITE {rel}:{line}: {value} | {context}")
        roles, representatives = role_catalog()
        picks = pick_representatives(files)
        unused = [role for role in roles if not picks.get(role)]
        mismatches = []
        for role in roles:
            counts = picks.get(role)
            expected = representatives.get(role)
            if not counts or expected is None:
                continue
            frequency = max(counts.values())
            common = [value for value, count in counts.items() if count == frequency]
            if expected not in common:
                mismatches.append((role, expected, common[0], frequency))
        for role in unused:
            print(f"unused role: {role}")
        for role, expected, most_frequent, count in mismatches:
            print(f"representative: {role} roles={expected} most_frequent={most_frequent} ({count} calls)")
        print(f"--check summary: remaining={len(remaining) + len(white)} unused_roles={len(unused)} representative_mismatches={len(mismatches)}")
        return 1 if remaining or white or unused or mismatches else 0
    for rel, line, value, context, kind, _offset in records:
        print(f"{rel}:{line}: {value} | {context}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
