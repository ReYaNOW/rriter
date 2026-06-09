#!/usr/bin/env python3
"""Probe Ty diagnostic payload shape and local memory cost.

This script is intentionally dependency-free. It measures three things:

1. What Ty reports for a file via `ty check --output-format concise`.
2. How much repeated text exists after normalizing diagnostic symbol names.
3. Memory difference between a full JSON tree and compact diagnostic tuples.

It does not measure Rust heap exactly, but it catches the same algorithmic
problem: building a whole JSON value tree before extracting the few fields
RRiter needs. Run from project root:

    python3 scripts/ty_diag_memory_probe.py tests/perf_diagnostics_stress_12000.py
"""

from __future__ import annotations

import argparse
import gc
import json
import os
import re
import resource
import subprocess
import sys
import time
import tracemalloc
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable


DIAG_RE = re.compile(
    r"^(?P<path>.*?):(?P<line>\d+):(?P<col>\d+): "
    r"(?P<severity>error|warning|info|hint)\[(?P<code>[^\]]+)\] (?P<message>.*)$"
)
BACKTICK_RE = re.compile(r"`[^`]+`")
NAME_RE = re.compile(r"Name `(?P<name>[^`]+)` used when not defined")
LSP_DIAG_RE = re.compile(
    rb'"range":\{"start":\{"line":(?P<sl>\d+),"character":(?P<sc>\d+)'
    rb'\},"end":\{"line":(?P<el>\d+),"character":(?P<ec>\d+)\}\}'
    rb'\,"severity":(?P<sev>\d+),"code":"(?P<code>(?:\\.|[^"])*)"'
    rb'\,"source":"(?P<source>(?:\\.|[^"])*)"'
    rb'\,"message":"(?P<message>(?:\\.|[^"])*)"\}'
)


@dataclass(frozen=True)
class TyDiag:
    path: str
    line: int
    col: int
    end_col: int
    severity: str
    code: str
    source: str
    message: str

    def to_lsp(self) -> dict:
        return {
            "range": {
                "start": {"line": self.line - 1, "character": self.col - 1},
                "end": {"line": self.line - 1, "character": self.end_col - 1},
            },
            "severity": severity_number(self.severity),
            "code": self.code,
            "source": self.source,
            "message": self.message,
        }


def severity_number(value: str) -> int:
    if value == "error":
        return 1
    if value == "warning":
        return 2
    if value == "info":
        return 3
    return 4


def run_ty(path: Path) -> str:
    cmd = [
        "ty",
        "check",
        "--exit-zero",
        "--color",
        "never",
        "--output-format",
        "concise",
        str(path),
    ]
    proc = subprocess.run(
        cmd,
        cwd=Path.cwd(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return proc.stdout


def parse_ty_concise(output: str) -> list[TyDiag]:
    diags: list[TyDiag] = []
    for line in output.splitlines():
        match = DIAG_RE.match(line)
        if not match:
            continue
        message = match.group("message")
        col = int(match.group("col"))
        name = NAME_RE.search(message)
        if name:
            end_col = col + len(name.group("name"))
        else:
            end_col = col + max(1, len(message.split(" ", 1)[0]))
        diags.append(
            TyDiag(
                path=match.group("path"),
                line=int(match.group("line")),
                col=col,
                end_col=end_col,
                severity=match.group("severity"),
                code=match.group("code"),
                source="ty",
                message=message,
            )
        )
    return diags


def normalize_message(message: str) -> str:
    return BACKTICK_RE.sub("`{name}`", message)


def make_lsp_payload(path: Path, diags: list[TyDiag]) -> bytes:
    uri = path.resolve().as_uri()
    body = {
        "jsonrpc": "2.0",
        "id": 2,
        "result": {
            "items": [
                {
                    "uri": uri,
                    "kind": "full",
                    "resultId": "probe-r1",
                    "items": [diag.to_lsp() for diag in diags],
                }
            ]
        },
    }
    return json.dumps(body, separators=(",", ":")).encode()


def rss_kib() -> int:
    return int(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)


def measured(label: str, fn: Callable[[], object]) -> tuple[object, int, int, float]:
    gc.collect()
    before_rss = rss_kib()
    tracemalloc.start()
    start = time.perf_counter()
    result = fn()
    elapsed = time.perf_counter() - start
    _current, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    after_rss = rss_kib()
    print(
        f"{label}: tracemalloc_peak={peak // 1024} KiB "
        f"rss_max_delta={max(0, after_rss - before_rss)} KiB "
        f"time={elapsed:.3f}s"
    )
    return result, peak // 1024, max(0, after_rss - before_rss), elapsed


def full_json_parse(payload: bytes) -> tuple[int, int]:
    value = json.loads(payload)
    items = value["result"]["items"]
    count = 0
    message_bytes = 0
    for item in items:
        for diag in item.get("items", []):
            count += 1
            message_bytes += len(diag.get("message", ""))
    return count, message_bytes


def compact_parse(payload: bytes) -> tuple[int, int, int]:
    # Regex is acceptable here because the script controls the LSP-like payload.
    # It models the Rust fix: extract needed fields without retaining a full
    # generic JSON object tree.
    sources: dict[str, str] = {}
    codes: dict[str, str] = {}
    messages: dict[str, str] = {}
    compact: list[tuple[int, int, int, int, int, str, str, str]] = []
    for match in LSP_DIAG_RE.finditer(payload):
        source = intern(sources, decode_json_string(match.group("source")))
        code = intern(codes, decode_json_string(match.group("code")))
        message = decode_json_string(match.group("message"))
        template = normalize_message(message)
        if template != message:
            message = intern(messages, template)
        compact.append(
            (
                int(match.group("sl")),
                int(match.group("sc")),
                int(match.group("el")),
                int(match.group("ec")),
                int(match.group("sev")),
                source,
                code,
                message,
            )
        )
    return len(compact), len(sources) + len(codes), len(messages)


def decode_json_string(raw: bytes) -> str:
    if b"\\" not in raw:
        return raw.decode()
    return json.loads(b'"' + raw + b'"')


def intern(pool: dict[str, str], value: str) -> str:
    existing = pool.get(value)
    if existing is not None:
        return existing
    pool[value] = value
    return value


def summarize(diags: list[TyDiag], payload: bytes, ty_output: str) -> None:
    messages = [diag.message for diag in diags]
    templates = [normalize_message(message) for message in messages]
    codes = {diag.code for diag in diags}
    sources = {diag.source for diag in diags}
    message_bytes = sum(len(message.encode()) for message in messages)
    template_counts: dict[str, int] = {}
    for template in templates:
        template_counts[template] = template_counts.get(template, 0) + 1
    repeated_templates = sorted(
        template_counts.items(), key=lambda item: item[1], reverse=True
    )

    print("input:")
    print(f"  file={diags[0].path if diags else 'n/a'}")
    print(f"  ty_stdout_bytes={len(ty_output.encode())}")
    print(f"  lsp_like_payload_bytes={len(payload)}")
    print("diagnostics:")
    print(f"  count={len(diags)}")
    print(f"  unique_messages={len(set(messages))}")
    print(f"  unique_templates={len(set(templates))}")
    print(f"  unique_codes={len(codes)}")
    print(f"  unique_sources={len(sources)}")
    print(f"  message_bytes={message_bytes}")
    if repeated_templates:
        top_template, top_count = repeated_templates[0]
        print("top_template:")
        print(f"  count={top_count}")
        print(f"  text={top_template}")


def write_payload(path: Path, payload: bytes) -> None:
    path.write_bytes(payload)
    print(f"wrote_payload={path}")


def parse_args(argv: Iterable[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "path",
        nargs="?",
        default="tests/perf_diagnostics_stress_12000.py",
        help="Python file to check with ty",
    )
    parser.add_argument(
        "--write-payload",
        type=Path,
        help="Optional path to write LSP-like JSON payload for external tools",
    )
    return parser.parse_args(list(argv))


def main(argv: Iterable[str]) -> int:
    args = parse_args(argv)
    path = Path(args.path)
    if not path.exists():
        print(f"missing file: {path}", file=sys.stderr)
        return 2
    ty_output, _peak, _rss, _elapsed = measured("ty_check", lambda: run_ty(path))
    diags = parse_ty_concise(ty_output)
    payload = make_lsp_payload(path, diags)
    summarize(diags, payload, ty_output)
    if args.write_payload:
        write_payload(args.write_payload, payload)
    measured("full_json_tree", lambda: full_json_parse(payload))
    measured("compact_tuple_model", lambda: compact_parse(payload))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
