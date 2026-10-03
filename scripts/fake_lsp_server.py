#!/usr/bin/env python3
"""Minimal stdio JSON-RPC server used by headless LSP panel tests."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import threading


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


WRITE_LOCK = threading.Lock()


def write_message(message: dict) -> None:
    body = json.dumps(message, separators=(",", ":")).encode("utf-8")
    # The delayed serverStatus timer writes from another thread.
    with WRITE_LOCK:
        sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii"))
        sys.stdout.buffer.write(body)
        sys.stdout.buffer.flush()


def server_status(quiescent: bool) -> None:
    write_message(
        {
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": {"health": "ok", "quiescent": quiescent},
        }
    )


def record_init_options(params: dict) -> None:
    """Append the received initializationOptions as one JSON line next to the script."""
    path = Path(sys.argv[0])
    path = path.with_name(path.name + ".init.jsonl")
    line = json.dumps(params.get("initializationOptions"), separators=(",", ":"))
    with path.open("a", encoding="utf-8") as handle:
        handle.write(line + "\n")


def sibling_uri(uri: str) -> str | None:
    """`dir/sibling.rs` next to a file:// URI; None when the URI has no usable directory."""
    if not uri.startswith("file://"):
        return None
    directory, separator, _ = uri.rpartition("/")
    if not separator or directory in ("file:", "file:/"):
        return None
    return f"{directory}/sibling.rs"


def diagnostics_message(uri: str, version: int | None, text: str) -> dict:
    return {
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "version": version,
            "diagnostics": [
                {
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 1},
                    },
                    "message": text,
                    "severity": 1,
                }
            ],
        },
    }


def record_start() -> None:
    path = Path(__file__).with_suffix(".starts")
    try:
        count = int(path.read_text(encoding="ascii"))
    except (FileNotFoundError, ValueError):
        count = 0
    path.write_text(str(count + 1), encoding="ascii")


def main() -> None:
    # Version probe (rust_workspace::resolve_rust_root_with): answer and exit
    # without counting it as a server start.
    if "--version" in sys.argv[1:]:
        print(f"{Path(sys.argv[0]).stem} 0.0.0-fake")
        return
    record_start()
    mode = Path(sys.argv[0]).stem
    crash_after_initialize = "_crash" in mode
    long_hover = "_long" in mode
    publish_diagnostics = "_diagnostics" in mode
    # A server that never answers textDocument/definition (overloaded ty).
    definition_never_answers = "_nodefinition" in mode
    # A server that answers textDocument/definition with a JSON-RPC error.
    definition_error = "_definitionerror" in mode
    # rust-analyzer style: experimental/serverStatus after `initialized`.
    send_server_status = "_serverstatus" in mode
    # Also publish diagnostics for a file that was never opened (sibling.rs).
    publish_unopened = "_unopened" in mode
    log_init_options = "_initlog" in mode
    while message := read_message():
        method = message.get("method")
        request_id = message.get("id")
        if method == "initialize":
            if log_init_options:
                record_init_options(message.get("params") or {})
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
        elif method == "initialized" and send_server_status:
            server_status(False)
            threading.Timer(0.3, server_status, args=(True,)).start()
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
            if definition_never_answers:
                continue
            if definition_error:
                write_message(
                    {
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {"code": -32603, "message": "definition failed"},
                    }
                )
                continue
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
                                "code": "unresolved-reference",
                                "codeDescription": {
                                    "href": "https://docs.astral.sh/ty/rules/unresolved-reference"
                                },
                            }
                        ],
                    },
                }
            )
            if publish_unopened:
                sibling = sibling_uri(opened_uri)
                if sibling is not None:
                    write_message(diagnostics_message(sibling, None, "fake: unopened sibling"))


if __name__ == "__main__":
    main()
