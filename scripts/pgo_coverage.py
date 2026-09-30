#!/usr/bin/env python3
"""Profile coverage, group markers and PGO build-warning accounting for the PGO pipeline.

Everything here is pure text processing except `demangle`/`find_cxxfilt`, so it can be
unit-tested without an LLVM toolchain (tests/test_pgo_coverage.py).
"""

from __future__ import annotations

import re
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Mapping, Sequence


class PgoError(RuntimeError):
    pass


OTHER_MODULE = "<other>"

# Substrings of demangled function names that run only on the path of one scenario part.
# A marker with a zero count (or no entry at all) means the group did not reach its code.
# A marker must be reached only on its own group's path. `move_page_down` (editor PageDown; the
# pdf viewer handles PageDown itself), `update_terminal_search` (every caller is terminal search,
# which only `terminal_ops` opens) and `draw_welcome` (headless enters the IDE before the first
# frame of every scenario but `welcome`) are shared functions that qualify for that reason.
GROUP_MARKERS: dict[str, list[str]] = {
    "pdf": ["pdf::worker::search_one_page", "pdf::pdfium_backend::render"],
    "api_mock": ["api_mock::server::run_server_thread", "api_mock::server::request_to_mock"],
    "git_changes": ["rollback_hunk_text", "jump_active_git_diff_hunk", "rollback_active_git_diff_hunk"],
    "editor_ops": ["toggle_extra_cursor", "apply_at_all_cursors_indexed", "undo_with_groups"],
    "lsp_nav": ["jump_to_definition_target", "parse_definition_target"],
    "input_scroll": ["app::mouse::drag_scrollbar", "apply_scrollbar_drag_target", "move_page_down"],
    "terminal_ops": ["close_terminal_tab_at", "update_terminal_search"],
    "startup": ["state_persistence::load_open_tabs"],
    "welcome": ["draw_welcome", "get_welcome_buttons"],
    "markdown": ["refresh_read_model", "scroll_markdown_read"],
    "database": ["execute_simple_query", "rollback_database_table_transaction"],
    "api_client": ["send_api_request_body"],
    "git_graph": ["ensure_git_graph_loaded", "apply_git_graph_lanes"],
    "terminal": ["TerminalProcess::write_input"],
    "settings": ["save_current_config"],
    "project_search": ["stream_project_search", "run_project_search_roots"],
}
_SCENARIO_ONLY = {"startup", "welcome"}

# Markers are matched against names with `<` and `>` removed, so the v0 form of an inherent
# method, `<rriter::terminal::TerminalProcess>::write_input`, matches `TerminalProcess::write_input`.

# Modules whose functions missing from the profile are listed separately in the log.
HOT_MODULES = ("render_view", "renderer", "editor", "highlighter", "scroll", "app::mouse", "app::keyboard")


@dataclass(frozen=True)
class FunctionProfile:
    name: str
    count: int


@dataclass
class PgoWarnings:
    missing: int = 0
    mismatch: int = 0
    hot: list[str] = field(default_factory=list)


_HEADER = re.compile(r"^ {1,2}(\S+):\s*$")
_NOT_FUNCTIONS = {"Counters", "Hash", "Block counts", "Function count", "Indirect Target Results"}
_FUNCTION_COUNT = re.compile(r"^\s*Function count:\s*(\d+)")
_BLOCK_COUNTS = re.compile(r"^\s*Block counts:\s*\[([^\]]*)\]")


def parse_profdata_show(text: str) -> list[FunctionProfile]:
    """Parse `llvm-profdata show --all-functions --counts`; unknown lines are ignored.

    A function counts as executed when its entry count or any block count is non-zero.
    """

    functions: list[FunctionProfile] = []
    name: str | None = None
    count = 0
    seen = False

    def flush() -> None:
        if name is not None and seen:
            functions.append(FunctionProfile(name, count))

    for line in text.splitlines():
        header = _HEADER.match(line)
        if header and header.group(1) not in _NOT_FUNCTIONS:
            flush()
            name, count, seen = header.group(1), 0, False
            continue
        if name is None:
            continue
        entry = _FUNCTION_COUNT.match(line)
        if entry:
            count, seen = max(count, int(entry.group(1))), True
            continue
        blocks = _BLOCK_COUNTS.match(line)
        if blocks:
            numbers = [int(value) for value in re.findall(r"\d+", blocks.group(1))]
            count, seen = max([count, *numbers]), True
    flush()
    return functions


def _strip_generics(text: str) -> str:
    depth = 0
    kept: list[str] = []
    for char in text:
        if char == "<":
            depth += 1
        elif char == ">":
            depth = max(depth - 1, 0)
        elif depth == 0:
            kept.append(char)
    return "".join(kept)


def _leading_angle_group(text: str) -> str | None:
    """Content of the balanced `<...>` group a name starts with."""

    depth = 0
    for index, char in enumerate(text):
        if char == "<":
            depth += 1
        elif char == ">":
            depth -= 1
            if depth == 0:
                return text[1:index]
    return None


def _top_level_as(text: str) -> str:
    """The part before a top-level ` as ` of `<Type as Trait>` contents."""

    depth = 0
    for index, char in enumerate(text):
        if char == "<":
            depth += 1
        elif char == ">":
            depth -= 1
        elif depth == 0 and text.startswith(" as ", index):
            return text[:index]
    return text


def _is_type_segment(segment: str) -> bool:
    return segment[:1].isupper()


def module_of(demangled: str) -> str:
    """Module path of a demangled function name; `<other>` when it cannot be told.

    Trait impls and inherent impls map to the module of the implementing type, closures
    and generic arguments to the enclosing function's module.
    """

    # Internal-linkage functions are profiled as `<cgu name>;<symbol>`; drop the file prefix.
    text = strip_cgu_prefix(demangled.strip()).strip()
    if not text or text.startswith(("_R", "_Z", "$")):
        return OTHER_MODULE
    from_type = text.startswith("<")
    if from_type:
        inner = _leading_angle_group(text)
        if inner is None:
            return OTHER_MODULE
        text = _top_level_as(inner)
    text = _strip_generics(text).strip().lstrip("&*")
    for prefix in ("mut ", "dyn "):
        text = text.removeprefix(prefix)
    if any(char in text for char in "()[]; "):
        return OTHER_MODULE
    segments = [segment for segment in text.split("::") if segment]
    if not from_type:
        while segments and (segments[-1].startswith("{") or re.fullmatch(r"h[0-9a-f]{16}", segments[-1])):
            segments.pop()
        if segments:
            segments.pop()  # the function itself
    while segments and _is_type_segment(segments[-1]):
        segments.pop()
    return "::".join(segments) if segments else OTHER_MODULE


def module_summary(functions: Iterable[FunctionProfile]) -> dict[str, tuple[int, int]]:
    """Module -> (functions executed, functions total)."""

    summary: dict[str, tuple[int, int]] = {}
    for function in functions:
        hit, total = summary.get(module_of(function.name), (0, 0))
        summary[module_of(function.name)] = (hit + (1 if function.count > 0 else 0), total + 1)
    return summary


def format_coverage(summary: Mapping[str, tuple[int, int]]) -> str:
    lines = ["module | hit | total"]
    lines.extend(f"{module} | {hit} | {total}" for module, (hit, total) in sorted(summary.items()))
    return "\n".join(lines) + "\n"


def zero_modules(summary: Mapping[str, tuple[int, int]], prefix: str = "rriter") -> list[str]:
    """`rriter::*` modules with functions but none executed."""

    return sorted(
        module
        for module, (hit, total) in summary.items()
        if hit == 0 and total > 0 and (module == prefix or module.startswith(prefix + "::"))
    )


def unrequired_groups(scenarios: Sequence[str]) -> set[str]:
    """Marker groups that the run's scenarios could not have exercised."""

    required: set[str] = set()
    for scenario in scenarios:
        if scenario == "full":
            required.update(key for key in GROUP_MARKERS if key not in _SCENARIO_ONLY)
        elif scenario in _SCENARIO_ONLY:
            required.add(scenario)
        elif scenario.startswith("group:"):
            required.add(scenario.removeprefix("group:"))
    return set(GROUP_MARKERS) - required


def _plain(name: str) -> str:
    return name.replace("<", "").replace(">", "")


def check_markers(
    functions: Sequence[FunctionProfile],
    markers: Mapping[str, Sequence[str]],
    skipped: Iterable[str],
) -> list[str]:
    """One `group: marker (reason)` line per marker the profile does not show as executed."""

    skip = set(skipped)
    problems: list[str] = []
    for group, substrings in markers.items():
        if group in skip:
            continue
        for marker in substrings:
            matching = [function for function in functions if marker in _plain(function.name)]
            if not matching:
                problems.append(f"{group}: {marker} (not in the profile)")
            elif not any(function.count > 0 for function in matching):
                problems.append(f"{group}: {marker} (never executed)")
    return problems


_V0_SKIP = re.compile(r"[sB][0-9a-zA-Z]*_")


def _v0_path(name: str) -> str:
    """v0 (`_R…`) mangled name -> `a::b::c`, read from its length-prefixed identifiers.

    Disambiguators (`s<base62>_`) and back-references (`B<base62>_`) are skipped; generic
    arguments are not decoded. This is a heuristic for the hot-module match only.
    """

    index, parts = 2, []
    while index < len(name):
        skip = _V0_SKIP.match(name, index)
        if skip:
            index = skip.end()
        elif name[index].isdigit():
            end = index
            while end < len(name) and name[end].isdigit():
                end += 1
            start = end + 1 if name[end:end + 1] == "_" else end
            parts.append(name[start:start + int(name[index:end])])
            index = start + int(name[index:end])
        else:
            index += 1
    return "::".join(parts)


def _legacy_path(name: str) -> str:
    """Itanium/legacy or v0 Rust mangled name -> `a::b::c`; other names come back unchanged."""

    if name.startswith("_RN"):
        return _v0_path(name)
    if not name.startswith("_ZN"):
        return name
    index, parts = 3, []
    while index < len(name) and name[index].isdigit():
        end = index
        while end < len(name) and name[end].isdigit():
            end += 1
        length = int(name[index:end])
        parts.append(name[end:end + length])
        index = end + length
    return "::".join(parts).replace("$LT$", "<").replace("$GT$", ">").replace("$u20$", " ").replace("..", "::")


_MISSING = re.compile(r"no profile data available for function\s+['\"`]?([^\s'\"`]+)")
_MISMATCH = re.compile(r"hash mismatch\)?[:\s]*['\"`]?([^\s'\"`]*)")


def _is_hot(name: str) -> bool:
    path = "::" + _legacy_path(name) + "::"
    return any(f"::{module}::" in path for module in HOT_MODULES)


def count_pgo_warnings(build_log: str) -> PgoWarnings:
    """Count missing-profile and hash-mismatch warnings of the profile-use build."""

    warnings = PgoWarnings()
    for line in build_log.splitlines():
        for pattern, attribute in ((_MISSING, "missing"), (_MISMATCH, "mismatch")):
            match = pattern.search(line)
            if match is None:
                continue
            setattr(warnings, attribute, getattr(warnings, attribute) + 1)
            name = match.group(1)
            if name and _is_hot(name) and name not in warnings.hot:
                warnings.hot.append(name)
    return warnings


def find_cxxfilt() -> str:
    path = shutil.which("llvm-cxxfilt")
    if path is None:
        raise PgoError(
            "llvm-cxxfilt not found in PATH; it is needed to demangle the profile for the "
            "coverage check and rustup does not ship it (install the distribution's llvm package)"
        )
    return path


def demangle(names: Sequence[str], cxxfilt: str) -> list[str]:
    """Demangle names with one `llvm-cxxfilt` call; raw names on any mismatch."""

    if not names:
        return []
    result = subprocess.run(
        [cxxfilt],
        input="\n".join(names) + "\n",
        text=True,
        capture_output=True,
        check=False,
    )
    lines = result.stdout.splitlines()
    if result.returncode != 0 or len(lines) != len(names):
        return list(names)
    return lines


def strip_cgu_prefix(name: str) -> str:
    """Internal-linkage functions are profiled as `<cgu name>;<symbol>`; keep the symbol."""

    return re.sub(r"^[^;]*;", "", name, count=1)


def demangled_functions(show_text: str, cxxfilt: str) -> list[FunctionProfile]:
    functions = parse_profdata_show(show_text)
    # llvm-cxxfilt leaves `<cgu>;_RN…` whole, so the prefix must go before demangling.
    names = demangle([strip_cgu_prefix(function.name) for function in functions], cxxfilt)
    return [FunctionProfile(name, function.count) for name, function in zip(names, functions)]


def verify_raw_profiles(directory: Path) -> list[Path]:
    """Every .profraw must be non-empty: an empty one is a process that died before its dump."""

    every = sorted(path for path in directory.glob("*.profraw") if path.is_file())
    if not every:
        raise PgoError(f"no .profraw files were created in {directory}")
    empty = [path.name for path in every if path.stat().st_size == 0]
    if empty:
        raise PgoError(f"empty .profraw files in {directory}: {', '.join(empty)}")
    return every


def check_new_raw_profiles(directory: Path, before: Iterable[Path], scenario: str) -> None:
    """The run must have left its own non-empty .profraw beside the `before` snapshot."""

    known = set(before)
    fresh = [path for path in directory.glob("*.profraw") if path.is_file() and path not in known]
    if not fresh:
        raise PgoError(f"scenario {scenario}: the run wrote no new .profraw in {directory}")
    empty = sorted(path.name for path in fresh if path.stat().st_size == 0)
    if empty:
        raise PgoError(f"scenario {scenario}: empty .profraw in {directory}: {', '.join(empty)}")
