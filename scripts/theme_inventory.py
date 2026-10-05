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
EDITOR_THEME_ACCESS = re.compile(r"(?<![A-Za-z_0-9])(?:self\.|renderer\.)?theme\.([A-Za-z_][A-Za-z_0-9]*)")
EDITOR_THEME_PASS = re.compile(r"&\s*(?:self\.|renderer\.)?theme\b")
EDITOR_THEME_WHOLE_FILE_EXCEPTIONS = {
    "src/render_view/editor_text_layer.rs": "Editor text, syntax, selection, search and diff rendering",
    "src/render_view/minimap_ui.rs": "Editor minimap rendering",
    "src/render_view/markdown_read.rs": "Markdown Reader content uses editor theme by design",
    "src/render_view/markdown_read_interaction.rs": "Markdown Reader interaction surfaces",
    "src/render_view/markdown_read_media.rs": "Markdown Reader media surfaces",
    "src/render_view/markdown_read_text_layout.rs": "Markdown Reader content layout",
    "src/render_view/markdown_code_scroll.rs": "Markdown Reader code blocks",
    "src/render_view/markdown_toc.rs": "Markdown Reader table of contents",
}
EDITOR_THEME_EXPRESSION_EXCEPTIONS: dict[tuple[str, str], str] = {
    ("src/renderer/renderer_init_methods.rs", "theme.clone"): "Initial renderer UI palette is synchronized with its editor palette before theme selection is applied",
    ("src/render_view/root_frame_layout_renderer.rs", "self.theme.surface_bg"): "Window surface background",
    ("src/render_view/ide_panels/ide_panel_dialog_renderer.rs", "self.theme.terminal_bg"): "Bottom panel under the Terminal tab uses editor theme by design",
    ("src/render_view/terminal_ui.rs", "self.theme.terminal"): "Terminal ANSI palette",
    ("src/render_view/terminal_ui.rs", "self.theme.search_match"): "Search matches inside terminal cells",
    ("src/render_view/terminal_ui.rs", "self.theme.search_match_active"): "Active search match inside terminal cells",
    ("src/render_view/terminal_ui.rs", "self.theme.sel"): "Terminal cell selection and content border",
    ("src/render_view/terminal_ui.rs", "self.theme.fg"): "Default terminal cell text",
    ("src/render_view/terminal_ui.rs", "self.theme.terminal_cursor"): "Terminal cursor",
    ("src/render_view/terminal_ui.rs", "&ansi_colors"): "Alias of the terminal ANSI palette",
    ("src/render_view/terminal_ui.rs", "border_color"): "Alias of the terminal content border colour",
}
EDITOR_THEME_FUNCTION_EXCEPTIONS: dict[tuple[str, str], str] = {
    ("src/render_view/terminal_ui.rs", "terminal_default_cell_text_uses_theme_ansi_seven"): "Test of terminal text using the editor theme's ANSI palette",
    ("src/render_view/root_helpers.rs", "mod_interval_color"): "Editor git gutter colors",
    ("src/render_view/root_helpers.rs", "git_gutter_uses_existing_theme_tokens_for_each_change_kind"): "Test of the editor git gutter colors",
    ("src/render_view/root_frame_editor_chrome_renderer.rs", "draw_root_editor_chrome"): "Editor gutter and scrollbar chrome",
    ("src/render_view/root_frame_editor_chrome_renderer.rs", "draw_root_editor_vertical_scrollbar"): "Editor scrollbar",
    ("src/render_view/root_frame_editor_text_renderer.rs", "draw_root_editor_text"): "Editor text surface",
    ("src/render_view/root_frame_editor_text_renderer.rs", "draw_root_editor_overlays"): "Editor selection, search and diff overlays",
    ("src/render_view/root_frame_editor_text_renderer.rs", "draw_root_editor_gutter"): "Editor gutter and diagnostics",
    ("src/render_view/root_frame_overlay_helpers.rs", "draw_empty_ide_frame"): "Blank editor surface background",
    ("src/render_view/root_frame_overlay_helpers.rs", "draw_blank_editor_area"): "Blank editor surface background",
    ("src/render_view/root_frame_overlay_helpers.rs", "draw_editor_horizontal_scrollbar"): "Editor horizontal scrollbar",
    ("src/render_view/root_frame_helpers.rs", "draw_inline_git_popup_panel"): "Inline editor diff overlay",
    ("src/render_view/root_frame_helpers.rs", "draw_inline_git_text_line"): "Inline editor diff text",
    ("src/render_view/root_frame_helpers.rs", "draw_git_diff_hunk_panel"): "Editor diff overlay",
    ("src/render_view/ui.rs", "draw_empty_ide"): "Empty editor surface background",
    ("src/render_view/search.rs", "draw_search_panel"): "Editor text search overlay",
    ("src/render_view/sticky.rs", "draw_sticky_lines"): "Sticky editor context lines",
}
EXCEPTIONS: dict[tuple[str, str], str] = {
    ("src/render_view/markdown_code_scroll.rs", "theme.syntax"): "Task 13 D: code block in editor theme",
    ("src/render_view/markdown_read_media.rs", "theme.line_num"): "Task 13 D: editor-themed media frame",
    ("src/render_view/ui.rs", "theme.bg"): "Task 13 A: editor background",
    ("src/render_view/ui.rs", "[1.0, 1.0, 1.0, 1.0]"): "Task 13 A: texture tint, not a colour",
    ("src/render_view/ide_panels/ide_panel_dialog_renderer.rs", "[1.0, 1.0, 1.0, 0.15]"): "commented-out code",
    ("src/render_view/scrollbar_widget.rs", "[1.0, 1.0, 1.0, 0.22]"): "SB: fallback, ScrollbarPaint",
    ("src/render_view/scrollbar_widget.rs", "[1.0, 1.0, 1.0, 1.0]"): "SB: fallback, ScrollbarPaint",
    ("src/render_view/scrollbar_widget.rs", "[0.7, 0.33, 0.54, 0.8]"): "SB: fallback, ScrollbarPaint",
    ("src/render_view/scrollbar_widget.rs", "[0.7, 0.33, 0.54, 1.0]"): "SB: fallback, ScrollbarPaint",
    ("src/render_view/scrollbar_widget.rs", "[0.45, 0.45, 0.55, 0.5]"): "SB: PROBLEMS grey, both themes",
    ("src/render_view/scrollbar_widget.rs", "[1.0, 1.0, 1.0, 0.36]"): "SB: unit-test literal",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.20, 0.21, 0.25, 0.55]"): "SB: fallback, ScrollbarPaint",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.48, 0.50, 0.58, 0.90]"): "SB: fallback, ScrollbarPaint",
    ("src/app/lsp_actions.rs", "[1.0, 1.0, 1.0, 0.22]"): "SB: fallback, ScrollbarPaint",
    ("src/app/mouse/input.rs", "[0.7, 0.33, 0.54, 1.0]"): "SB: fallback, ScrollbarPaint",
    ("src/app/api_client/api_client_persistence_logs.rs", "[1.0, 1.0, 1.0, 0.36]"): "SB: fallback, ScrollbarPaint",
    ("src/app/api_client/api_client_persistence_logs.rs", "[1.0, 1.0, 1.0, 0.24]"): "SB: fallback, ScrollbarPaint",
    ("src/app/api_client/api_client_persistence_logs.rs", "[1.0, 1.0, 1.0, 0.28]"): "SB: fallback, ScrollbarPaint",
    ("src/app/project_search.rs", "[1.0, 1.0, 1.0, 0.035]"): "SB: fallback, ScrollbarPaint",
    ("src/app/api_client/api_client_layout_input.rs", "[0.70, 0.72, 0.80, 0.88]"): "Task 13 G: scrollbar descriptor",
    ("src/app/api_client/api_client_layout_input.rs", "[0.64, 0.66, 0.72, 0.70]"): "Task 13 G: scrollbar descriptor",
    ("src/app/api_client/api_client_persistence_logs.rs", "[0.70, 0.72, 0.80, 0.88]"): "Task 13 G: scrollbar descriptor",
    ("src/app/api_client/api_client_persistence_logs.rs", "[0.52, 0.54, 0.60, 0.22]"): "Task 13 G: scrollbar descriptor",
    ("src/app/api_client/api_client_persistence_logs.rs", "[0.64, 0.66, 0.72, 0.70]"): "Task 13 G: scrollbar descriptor",
    ("src/app/api_client/api_client_persistence_logs.rs", "[1.0, 1.0, 1.0, 0.36]"): "Task 13 G: scrollbar descriptor",
    ("src/app/api_client/api_client_persistence_logs.rs", "[1.0, 1.0, 1.0, 0.24]"): "Task 13 G: scrollbar descriptor",
    ("src/app/api_client/api_client_persistence_logs.rs", "[1.0, 1.0, 1.0, 0.28]"): "Task 13 G: scrollbar descriptor",
    ("src/app/keymap_settings.rs", "[0.7, 0.33, 0.54, 1.0]"): "Task 13 G: scrollbar descriptor",
    ("src/app/lsp_actions.rs", "[1.0, 1.0, 1.0, 0.22]"): "Task 13 G: scrollbar descriptor",
    ("src/app/mouse/input.rs", "[0.7, 0.33, 0.54, 1.0]"): "Task 13 H: scrollbar descriptor",
    ("src/app/project_search.rs", "[0.48, 0.48, 0.56, 0.68]"): "Task 13 H: scrollbar descriptor",
    ("src/app/project_search.rs", "[1.0, 1.0, 1.0, 0.035]"): "Task 13 H: scrollbar descriptor",
    ("src/app/project_search_preview.rs", "[0.48, 0.48, 0.56, 0.55]"): "Task 13 H: scrollbar descriptor",
    ("src/render_view/api_client_tab/api_client_tab_field_renderer.rs", "theme.sel"): "Task 9: editor selection color",
    ("src/render_view/api_client_tab/api_client_tab_field_renderer.rs", "theme.fg"): "Task 9: editor foreground color",
    ("src/render_view/api_client_tab/api_client_tab_main_renderer.rs", "theme.line_num"): "Task 9: editor gutter color",
    ("src/render_view/api_client_tab/api_client_tab_main_renderer.rs", "theme.sel"): "Task 9: editor selection color",
    ("src/render_view/api_client_tab/api_client_tab_main_renderer.rs", "theme.fg"): "Task 9: editor caret color",
    ("src/render_view/api_client_tab/api_client_tab_python_renderer.rs", "theme.bg"): "Task 9: editor background color",
    ("src/render_view/api_client_tab/api_client_tab_python_renderer.rs", "theme.fg"): "Task 9: editor caret assertion",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "theme.terminal_bg"): "Task 8: editor-themed database panel",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.35, 0.58, 0.95, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.35, 0.82, 0.48, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.92, 0.78, 0.30, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.95, 0.55, 0.26, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.92, 0.34, 0.38, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.67, 0.42, 0.92, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.30, 0.78, 0.86, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_database_renderer.rs", "[0.56, 0.58, 0.64, 1.0]"): "Task 8: connection tag palette",
    ("src/render_view/ide_panels/ide_panel_dialog_renderer.rs", "theme.terminal_bg"): "Task 8: editor-themed bottom panel",
    ("src/render_view/ide_panels/ide_panel_dialog_renderer.rs", "[1.0, 1.0, 1.0, 0.15]"): "Task 8: commented resize grip",
    ("src/render_view/ide_panels/ide_panel_git_graph_renderer.rs", "theme.terminal"): "Task 8: terminal ANSI palette",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.48, 0.82, 0.52, alpha]"): "Task 8: computed checkbox color",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.07, 0.09, 0.12, alpha]"): "Task 8: computed checkbox color",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[1.0, 1.0, 1.0, if controls_disabled { 0.10 } else { 0.20 }]"): "Task 8: computed disabled tint",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[1.0, 1.0, 1.0, if controls_disabled { 0.07 } else { 0.12 }]"): "Task 8: computed disabled tint",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.48, 0.74, 1.0, 1.0]"): "Task 8: Git graph lane palette",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.52, 0.82, 0.58, 1.0]"): "Task 8: Git graph lane palette",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.97, 0.76, 0.38, 1.0]"): "Task 8: Git graph lane palette",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.95, 0.42, 0.46, 1.0]"): "Task 8: Git graph lane palette",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.56, 0.86, 0.88, 1.0]"): "Task 8: Git graph lane palette",
    ("src/render_view/ide_panels/ide_panel_helpers.rs", "[0.82, 0.68, 1.0, 1.0]"): "Task 8: Git graph lane palette",
    ("src/render_view/markdown_read_text_layout.rs", "[0.47, 0.68, 0.96, 1.0]"): "Task 13 D: editor Markdown content",
    ("src/render_view/markdown_read_text_layout.rs", "[0.95, 0.93, 0.98, 1.0]"): "Task 13 D: editor Markdown content",
    ("src/render_view/markdown_read_text_layout.rs", "[0.78, 0.75, 0.87, 1.0]"): "Task 13 D: editor Markdown content",
    ("src/render_view/markdown_read_text_layout.rs", "[0.73, 0.70, 0.86, 1.0]"): "Task 13 D: editor Markdown content",
    ("src/render_view/minimap_ui.rs", "theme.sel"): "Task 13 C: editor minimap overlay",
    ("src/render_view/minimap_ui.rs", "theme.minimap_bg"): "Task 13 C: editor minimap background",
    ("src/render_view/minimap_ui.rs", "theme.syntax"): "Task 13 C: editor minimap syntax",
    ("src/render_view/minimap_ui.rs", "theme.fg"): "Task 13 C: editor minimap text",
    ("src/render_view/minimap_ui.rs", "theme.minimap_cursor"): "Task 13 C: editor minimap cursor",
    ("src/render_view/root_frame_overlay_helpers.rs", "theme.bg"): "Task 13 C: editor background and track",
    ("src/render_view/tabs_ui.rs", "[1.0, 0.55, 0.18, 1.0]"): "Task 13 B: palette at render use",
    ("src/render_view/tabs_ui.rs", "[0.0, 0.0, 0.0, 0.0]"): "Task 13 B: transparent tab paint",
    ("src/render_view/ui.rs", "[0.14, 0.33, 0.42, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.58, 0.88, 1.0, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.19, 0.36, 0.22, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.66, 0.94, 0.68, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.32, 0.23, 0.42, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.78, 0.64, 1.0, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.42, 0.31, 0.16, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[1.0, 0.79, 0.42, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.44, 0.25, 0.12, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[1.0, 0.68, 0.34, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.35, 0.25, 0.14, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[1.0, 0.72, 0.38, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.17, 0.28, 0.43, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.62, 0.78, 1.0, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.29, 0.24, 0.48, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.74, 0.66, 1.0, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.43, 0.20, 0.30, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[1.0, 0.61, 0.80, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.25, 0.26, 0.29, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.70, 0.72, 0.76, 1.0]"): "Task 13 A: completion badge category",
    ("src/render_view/ui.rs", "[0.55_f32, 0.50, 0.75, 0.9]"): "Task 13 A: decorative editor illustration",
    ("src/render_view/ui.rs", "[1.0, 1.0, 1.0, 1.0]"): "Task 13 A: neutral bitmap texture tint",
    ("src/widgets.rs", "[0.0, 0.0, 0.0, 0.0]"): "Task 13 B: transparent initial fill",
}


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
    # Climb out through the brackets enclosing `offset`: nested calls such as
    # `pick(UiRole::X, [theme.bg[0].min(1.0), ..])` must still count as wrapped.
    depth = 0
    for i in range(offset - 1, max(-1, offset - 2000), -1):
        char = source[i]
        if char in ")]":
            depth += 1
        elif char in "([":
            if depth:
                depth -= 1
            elif char == "(" and re.search(r"\.pick\s*$", source[max(0, i - 40):i]):
                return re.match(r"\s*(?:crate::theme::)?UiRole::[A-Za-z_][A-Za-z_0-9]*\s*,", source[i + 1:offset]) is not None
        elif char in ";{}" and not depth:
            return False
    return False


def rust_functions(source: str):
    """Yield (name, start, end) spans for functions, using brace depth."""
    for match in re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z_0-9]*)\b", source):
        opening = source.find("{", match.end())
        if opening < 0:
            continue
        depth = 0
        for index in range(opening, len(source)):
            if source[index] == "{":
                depth += 1
            elif source[index] == "}":
                depth -= 1
                if depth == 0:
                    yield match.group(1), match.start(), index + 1
                    break


def editor_theme_reads(path: Path):
    rel = path.relative_to(ROOT).as_posix()
    if not (rel.startswith("src/render_view/") or rel == "src/renderer.rs" or rel.startswith("src/renderer/") or rel == "src/widgets.rs"):
        return
    if is_test_file(path):
        return
    source = path.read_text(encoding="utf-8")
    functions = list(rust_functions(source))
    aliases_by_function: dict[str, set[str]] = {}
    alias_decl = re.compile(r"\blet\s+([A-Za-z_][A-Za-z_0-9]*)\s*=\s*&?\s*self\.theme\b")
    for name, start, end in functions:
        aliases_by_function[name] = {match.group(1) for match in alias_decl.finditer(source, start, end)}

    access_pattern = EDITOR_THEME_ACCESS
    candidates = []
    for match in access_pattern.finditer(source):
        candidates.append((match.start(), match.group(0)))
    for name, start, end in functions:
        aliases = aliases_by_function.get(name, ())
        for alias in aliases:
            for match in re.finditer(rf"\b{re.escape(alias)}\.([A-Za-z_][A-Za-z_0-9]*)", source[start:end]):
                candidates.append((start + match.start(), match.group(0)))
            for match in re.finditer(rf"(?<![.\w])&?\s*{re.escape(alias)}\b(?!\s*\.)", source[start:end]):
                if not re.match(r"\s*=", source[start + match.end():]):
                    candidates.append((start + match.start(), match.group(0).strip()))
    # A theme passed by reference to a helper is a theme read even without a field access.
    for match in EDITOR_THEME_PASS.finditer(source):
        candidates.append((match.start(), match.group(0)))

    for offset, expression in sorted(set(candidates)):
        containing = next((name for name, start, end in functions if start <= offset < end), "<module>")
        if rel in EDITOR_THEME_WHOLE_FILE_EXCEPTIONS:
            continue
        if (rel, containing) in EDITOR_THEME_FUNCTION_EXCEPTIONS:
            continue
        if (rel, expression) in EDITOR_THEME_EXPRESSION_EXCEPTIONS:
            continue
        line = source.count("\n", 0, offset) + 1
        yield rel, line, containing, expression


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
    pick_pattern = re.compile(r"\.pick\s*\(\s*(?:crate::theme::)?UiRole::([A-Za-z_][A-Za-z_0-9]*)\s*,")
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
    parser.add_argument("--report-editor-theme-reads", action="store_true")
    parser.add_argument("--files", nargs="*")
    args = parser.parse_args()
    files = [ROOT / item for item in args.files] if args.files else sorted(SRC.rglob("*.rs"))
    records = [record for path in files if path.is_file() for record in scan_file(path)]
    editor_reads = [read for path in files if path.is_file() for read in editor_theme_reads(path)]
    if args.report_editor_theme_reads:
        for rel, line, function, expression in editor_reads:
            print(f"{rel}:{line}: {function}: {expression}")
        return 0
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
                if (rel, match.group(0)) in EXCEPTIONS:
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
        print(f"editor_theme_reads={len(editor_reads)}")
        return 1 if remaining or white or unused or mismatches or editor_reads else 0
    for rel, line, value, context, kind, _offset in records:
        print(f"{rel}:{line}: {value} | {context}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
