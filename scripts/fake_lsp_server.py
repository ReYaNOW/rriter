#!/usr/bin/env python3
"""Minimal stdio JSON-RPC server used by headless LSP panel tests."""

from __future__ import annotations

import json
from pathlib import Path
import sys


def read_message() -> dict | None:
    headers: dict[str, str] = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b"\r\n", b"\n"):
            break
        name, _, value = line.decode("ascii").partition(":")
        headers[name.lower()] = value.strip()
    length = int(headers["content-length"])
    body = sys.stdin.buffer.read(length)
    return json.loads(body)


def write_message(message: dict) -> None:
    body = json.dumps(message, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii"))
    sys.stdout.buffer.write(body)
    sys.stdout.buffer.flush()


def record_start() -> None:
    path = Path(__file__).with_suffix(".starts")
    try:
        count = int(path.read_text(encoding="ascii"))
    except (FileNotFoundError, ValueError):
        count = 0
    path.write_text(str(count + 1), encoding="ascii")


def main() -> None:
    record_start()
    mode = Path(sys.argv[0]).stem
    crash_after_initialize = "_crash" in mode
    long_hover = "_long" in mode
    publish_diagnostics = "_diagnostics" in mode
    while message := read_message():
        method = message.get("method")
        request_id = message.get("id")
        if method == "initialize":
            write_message(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "capabilities": {
                            "hoverProvider": True,
                            "inlayHintProvider": True,
                        }
                    },
                }
            )
        elif method == "initialized" and crash_after_initialize:
            return
        elif method == "shutdown":
            write_message({"jsonrpc": "2.0", "id": request_id, "result": None})
        elif method == "exit":
            return
        elif method == "textDocument/hover":
            hover_text = "Fake hover from LSP stub for hover_subject"
            if long_hover:
                hover_text = "\n".join(
                    [hover_text, *(f"Detail {line}: hover_subject documentation." for line in range(32))]
                )
            write_message(
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": {
                        "contents": {"kind": "markdown", "value": hover_text}
                    },
                }
            )
        elif method == "textDocument/definition":
            write_message({"jsonrpc": "2.0", "id": request_id, "result": None})
        elif method == "textDocument/inlayHint":
            write_message({"jsonrpc": "2.0", "id": request_id, "result": []})
        elif method == "textDocument/didOpen" and publish_diagnostics:
            document = message.get("params", {}).get("textDocument", {})
            opened_uri = document.get("uri", "")
            write_message(
                {
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": opened_uri,
                        "diagnostics": [
                            {
                                "range": {
                                    "start": {"line": 1, "character": 11},
                                    "end": {"line": 1, "character": 29},
                                },
                                "message": "Name missing_hover_name is not defined",
                                "severity": 1,
                            }
                        ],
                    },
                }
            )


if __name__ == "__main__":
    main()
