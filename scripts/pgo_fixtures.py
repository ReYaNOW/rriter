#!/usr/bin/env python3
"""Training workspace fixtures and host-side inputs of the PGO training runs.

Split out of pgo_pipeline.py: deterministic workspace files (Rust, Python, Dart,
Markdown, a large OpenAPI document) plus lookups of the host tools the headless
scenario groups depend on (pdfium library, managed `ty`).
"""

from __future__ import annotations

import json
import os
import shutil
from pathlib import Path
from typing import Mapping

from pgo_coverage import PgoError

ROOT = Path(__file__).resolve().parents[1]
FIXTURE_VERSION = 7
OPENAPI_BULK_PATH_COUNT = 512
OPENAPI_SCHEMA_COUNT = 192
OPENAPI_BULK_METHODS = ("get", "post", "patch", "delete")
LOCAL_API_MARKER = "RRITER_PGO_LOCAL_API_OK"


def find_pdfium(root: Path) -> Path:
    """Library path from `target/pdfium.path` (written by `make pdfium`)."""

    hint = "pdfium not found, run `make pdfium`"
    path_file = root / "target" / "pdfium.path"
    try:
        text = path_file.read_text(encoding="utf-8").strip()
    except OSError:
        raise PgoError(f"{hint} ({path_file} is missing)") from None
    if not text or not Path(text).is_file():
        raise PgoError(f"{hint} ({path_file} points to {text or 'nothing'})")
    return Path(text)


def managed_ty_bin_dir(environment: Mapping[str, str]) -> Path | None:
    """Bin directory of RRiter's managed `ty`, only when `ty` is not on the host PATH.

    Layout from `tool_installer_download.rs`: `<data>/RRiter/tools/managed/ty/<generation>/bin`.
    """

    if shutil.which("ty", path=environment.get("PATH", os.defpath)):
        return None
    home = environment.get("HOME", "")
    data = environment.get("XDG_DATA_HOME") or (os.path.join(home, ".local", "share") if home else "")
    if not data:
        return None
    candidates = sorted(Path(data, "RRiter", "tools", "managed", "ty").glob("*/bin/ty"))
    return candidates[-1].parent if candidates else None


def _openapi_schema(index: int) -> dict[str, object]:
    related_ref = (
        f"#/components/schemas/Entity{index + 1:03d}"
        if index + 1 < OPENAPI_SCHEMA_COUNT
        else "#/components/schemas/ErrorEnvelope"
    )
    return {
        "type": "object",
        "required": ["id", "name", "state", "created_at"],
        "properties": {
            "id": {"type": "integer", "format": "int64", "minimum": 1},
            "name": {
                "type": "string",
                "minLength": 3,
                "maxLength": 120,
                "example": f"resource-{index:04d}",
            },
            "state": {
                "type": "string",
                "enum": ["queued", "running", "paused", "done", "failed"],
            },
            "created_at": {"type": "string", "format": "date-time"},
            "labels": {
                "type": "array",
                "items": {"type": "string"},
                "maxItems": 32,
            },
            "metrics": {
                "type": "object",
                "additionalProperties": {"type": "number", "format": "double"},
            },
            "related": {"$ref": related_ref},
        },
    }


def _openapi_operation(path_index: int, method: str) -> dict[str, object]:
    schema_index = path_index % OPENAPI_SCHEMA_COUNT
    operation: dict[str, object] = {
        "tags": [f"bulk-{path_index % 32:02d}"],
        "operationId": f"bulk_{method}_{path_index:04d}",
        "summary": f"{method.upper()} bulk resource {path_index:04d}",
        "description": (
            "### Deterministic PGO route\n"
            "- Exercises route filtering and markdown rendering.\n"
            "- Uses nested schemas, parameters, auth, examples, and responses.\n"
            f"- Fixture route index: `{path_index:04d}`."
        ),
        "security": [{"BearerAuth": []}, {"HeaderKey": []}],
        "parameters": [
            {
                "name": "resource_id",
                "in": "path",
                "required": True,
                "description": "Stable resource identifier",
                "schema": {"type": "integer", "format": "int64", "minimum": 1},
                "example": path_index + 1,
            },
            {
                "name": "page_size",
                "in": "query",
                "required": False,
                "description": "Result window size",
                "schema": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 500,
                    "default": 50,
                },
            },
            {
                "name": "include",
                "in": "query",
                "required": False,
                "schema": {
                    "type": "array",
                    "items": {"type": "string", "enum": ["owner", "metrics", "history"]},
                },
            },
        ],
        "responses": {
            "200": {
                "description": "Successful deterministic response",
                "content": {
                    "application/json": {
                        "schema": {"$ref": f"#/components/schemas/Entity{schema_index:03d}"},
                        "examples": {
                            "default": {
                                "value": {
                                    "id": path_index + 1,
                                    "name": f"resource-{path_index:04d}",
                                    "state": "running",
                                }
                            }
                        },
                    }
                },
            },
            "400": {
                "description": "Invalid request",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ErrorEnvelope"}
                    }
                },
            },
            "404": {
                "description": "Resource not found",
                "content": {
                    "application/json": {
                        "schema": {"$ref": "#/components/schemas/ErrorEnvelope"}
                    }
                },
            },
        },
    }
    if method in {"post", "patch"}:
        operation["requestBody"] = {
            "required": True,
            "content": {
                "application/json": {
                    "schema": {"$ref": f"#/components/schemas/Entity{schema_index:03d}"}
                }
            },
        }
    return operation


def _large_openapi_fixture() -> dict[str, object]:
    schemas = {
        f"Entity{index:03d}": _openapi_schema(index)
        for index in range(OPENAPI_SCHEMA_COUNT)
    }
    schemas["ErrorEnvelope"] = {
        "type": "object",
        "required": ["code", "message"],
        "properties": {
            "code": {"type": "string", "example": "fixture_error"},
            "message": {"type": "string"},
            "details": {
                "type": "array",
                "items": {"type": "string"},
            },
        },
    }
    paths: dict[str, object] = {
        "/automation/featured/{resource_id}": {
            "post": {
                **_openapi_operation(0, "post"),
                "tags": ["automation"],
                "operationId": "PGO_FEATURED_WRITE",
                "summary": "PGO_FEATURED_WRITE",
                "description": (
                    "### Featured API Client training route\n"
                    "- Filtered and opened by the native Rust automation.\n"
                    "- Contains path, query, request-body, auth, and response UI."
                ),
            }
        },
        "/automation/ping": {
            "get": {
                "tags": ["automation"],
                "operationId": "PGO_LOCAL_SERVER_PING",
                "summary": "PGO_LOCAL_SERVER_PING",
                "description": (
                    "Calls the local deterministic HTTP server started by "
                    "pgo_pipeline.py and verifies the real API Client request path."
                ),
                "security": [{"BearerAuth": []}],
                "responses": {
                    "200": {
                        "description": "Deterministic local response",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["marker", "accepted"],
                                    "properties": {
                                        "marker": {"type": "string"},
                                        "accepted": {"type": "boolean"},
                                    },
                                },
                                "example": {
                                    "marker": LOCAL_API_MARKER,
                                    "accepted": True,
                                },
                            }
                        },
                    }
                },
            }
        }
    }
    for index in range(OPENAPI_BULK_PATH_COUNT):
        paths[f"/bulk/resources/{{resource_id}}/items/item-{index:04d}"] = {
            method: _openapi_operation(index, method)
            for method in OPENAPI_BULK_METHODS
        }
    return {
        "openapi": "3.0.3",
        "info": {
            "title": "RRiter large PGO API fixture",
            "version": "2.0.0",
            "description": "Large deterministic OpenAPI document generated by pgo_pipeline.py.",
        },
        "servers": [
            {
                "url": "http://127.0.0.1:9/api/v1",
                "description": "Deliberately unreachable local fixture server",
            }
        ],
        "tags": [
            {"name": "automation", "description": "Native PGO automation route"},
            *[
                {"name": f"bulk-{index:02d}", "description": f"Bulk group {index:02d}"}
                for index in range(32)
            ],
        ],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "BearerAuth": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT"},
                "HeaderKey": {"type": "apiKey", "in": "header", "name": "X-API-Key"},
            },
            "schemas": schemas,
        },
    }


def _copy_python_test_fixtures(workspace: Path) -> list[str]:
    source_dir = ROOT / "tests"
    target_dir = workspace / "tests"
    target_dir.mkdir(parents=True, exist_ok=True)
    copied: list[str] = []
    if source_dir.is_dir():
        for source in sorted(source_dir.glob("*.py")):
            if not source.is_file():
                continue
            target = target_dir / source.name
            shutil.copyfile(source, target)
            copied.append(target.relative_to(workspace).as_posix())

    completion_fixture = target_dir / "pgo_completion_hover.py"
    completion_fixture.write_text(
        "from __future__ import annotations\n\n"
        "from dataclasses import dataclass\n"
        "from typing import Any, Iterable\n\n"
        "@dataclass(slots=True)\n"
        "class PgoCompletionModel:\n"
        "    name: str\n"
        "    values: list[int]\n"
        "    metadata: dict[str, Any]\n\n"
        "def pgo_completion_target(model: PgoCompletionModel) -> int:\n"
        "    return sum(model.values) + len(model.metadata)\n\n"
        "def pgo_completion_transform(items: Iterable[PgoCompletionModel]) -> list[int]:\n"
        "    return [pgo_completion_target(item) for item in items]\n\n"
        "async def pgo_hover_target(model: PgoCompletionModel) -> dict[str, int]:\n"
        "    \"\"\"Return a normalized summary used by deterministic PGO hover training.\"\"\"\n"
        "    return {model.name: pgo_completion_target(model)}\n\n"
        "pgo_completion_result = pri\n",
        encoding="utf-8",
    )
    copied.append(completion_fixture.relative_to(workspace).as_posix())
    (workspace / ".rriter-pgo-python-tests.json").write_text(
        json.dumps({"files": copied}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return copied


def _dart_fixture_source() -> str:
    lines = [
        "// Deterministic Dart fixture for parser/highlight/fold/closing-label PGO training.\n",
        "import 'dart:async';\n", "import 'dart:collection';\n\n",
        "class PgoAnnotation { final String name; const PgoAnnotation(this.name); }\n",
        "const pgoAnnotation = PgoAnnotation('rriter-pgo');\n",
        "enum PgoState { idle, running, complete }\n",
        "typedef PgoMapper<T> = T Function(T value);\n",
        "extension PgoIterableExtension on Iterable<int> { int get total => fold(0, (a, b) => a + b); }\n\n",
        "@pgoAnnotation\nabstract class PgoWorker<T extends num> {\n  const PgoWorker();\n  Future<T> run(T value);\n}\n\n",
        "class PgoConfig<T> {\n  final String name;\n  final T? value;\n  const PgoConfig({required this.name, this.value});\n",
        "  String describe() => 'PgoConfig(name: $name, value: $value)';\n}\n\n",
        "const pgoDartCompletionTarget = 'deterministic-completion-marker';\n",
        "const pgoDartBanner = '''RRiter PGO\nDart syntax fixture\nclosing labels\n''';\n\n",
    ]
    for index in range(36):
        lines.extend([
            f"class PgoNode{index} {{\n  final int seed;\n  const PgoNode{index}(this.seed);\n",
            f"  Future<int> compute{index}(List<int> values) async {{\n    var total = seed;\n    final queue = Queue<int>()..addAll(values);\n",
            f"    for (final value in queue) {{\n      if ((value + {index}) % 2 == 0) {{\n        try {{\n          var cursor = value;\n",
            "          while (cursor > 0) {\n            total += cursor;\n            cursor -= 1;\n          }\n",
            "        } catch (error) {\n          total -= error.hashCode;\n        } finally {\n          total += values.length;\n        }\n",
            "      } else {\n        total -= value;\n      }\n    }\n    await Future<void>.delayed(Duration.zero);\n    return total;\n  }\n}\n\n",
        ])
    lines.extend([
        "Future<int> pgoDartTarget(List<int> values, {PgoState state = PgoState.running}) async {\n",
        "  int nested(int value) {\n    if (value > 1) {\n      for (var i = 0; i < value; i++) {\n        value += i;\n      }\n    }\n    return value;\n  }\n",
        "  final int pgoDartTargetValue = nested(values.length);\n  // pgoDartEditTarget\n",
        "  switch (state) {\n    case PgoState.idle:\n      return pgoDartTargetValue;\n    case PgoState.running:\n      return await const PgoNode0(3).compute0(values);\n    case PgoState.complete:\n      return values.total;\n  }\n}\n",
    ])
    return "".join(lines)


def _write_fixture_files(workspace: Path) -> None:
    (workspace / "src").mkdir(parents=True, exist_ok=True)
    (workspace / "lib").mkdir(parents=True, exist_ok=True)
    (workspace / "Cargo.toml").write_text(
        "[package]\nname = \"rriter-pgo-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        encoding="utf-8",
    )
    (workspace / "README.md").write_text(
        "# RRiter PGO Markdown fixture\n\n"
        "Deterministic workspace for native GUI automation and Markdown training.\n\n"
        "## Edit and Read coverage\n\n"
        "### Semantic structures\n\n"
        "Ordinary paragraph text wraps across the preview viewport and keeps source markers visible in Edit mode. "
        "A second sentence makes the paragraph long enough to exercise wrapping deterministically.\n\n"
        "Unicode: кириллица \U0001f600 — проверка UTF-8 границ.\n\n"
        "**strong text** and *emphasis text* plus `inline_code(42)`.\n\n"
        "[deterministic link](https://example.invalid/path)\n\n"
        "> Block quote first line.\n"
        "> Multiline quote continuation with **strong quote text**.\n\n"
        "- unordered item\n"
        "- [ ] unchecked task\n"
        "- [x] checked task\n"
        "  - nested unordered item\n\n"
        "1. ordered first\n"
        "2. ordered second\n\n"
        "---\n\n"
        "| left | center | right |\n"
        "| :--- | :----: | ----: |\n"
        "| alpha | beta | gamma |\n"
        "| кириллица | \U0001f600 | 42 |\n\n"
        "```rust\n"
        "fn main() {\n"
        "    let value = 42;\n"
        "    println!(\"{value}\");\n"
        "}\n"
        "```\n\n"
        "```python\n"
        "def answer():\n"
        "    return 42\n"
        "```\n\n"
        "```bash\n"
        "echo \"markdown pgo\"\n"
        "```\n\n"
        "## Scroll coverage A\n\n"
        "Paragraph A repeats deterministic Markdown body text for real vertical scrolling without a large fixture. "
        "It covers wrapping, glyph layout, and preview virtualization.\n\n"
        "## Scroll coverage B\n\n"
        "Paragraph B repeats deterministic Markdown body text for real vertical scrolling without a large fixture. "
        "It covers wrapping, glyph layout, and preview virtualization.\n\n"
        "## Scroll coverage C\n\n"
        "Paragraph C repeats deterministic Markdown body text for real vertical scrolling without a large fixture. "
        "It covers wrapping, glyph layout, and preview virtualization.\n\n"
        "## Scroll coverage D\n\n"
        "Paragraph D repeats deterministic Markdown body text for real vertical scrolling without a large fixture. "
        "It covers wrapping, glyph layout, and preview virtualization.\n\n"
        "Incremental edit anchor: RRITER_PGO_MARKDOWN_EDIT_TARGET\n",
        encoding="utf-8",
    )
    (workspace / ".rriter-pgo-fixture.json").write_text(
        json.dumps({"fixture_version": FIXTURE_VERSION}, indent=2) + "\n",
        encoding="utf-8",
    )
    (workspace / "src" / "main.rs").write_text(
        "use std::collections::BTreeMap;\n\n"
        "fn summarize(values: &[u64]) -> u64 { values.iter().copied().sum() }\n\n"
        "fn main() {\n"
        "    let mut values = BTreeMap::new();\n"
        "    values.insert(\"alpha\", summarize(&[1, 2, 3]));\n"
        "    println!(\"{values:?}\");\n"
        "}\n",
        encoding="utf-8",
    )
    (workspace / "src" / "worker.py").write_text(
        "from dataclasses import dataclass\n\n"
        "@dataclass(slots=True)\n"
        "class Job:\n"
        "    name: str\n"
        "    weight: int\n\n"
        "def total(items: list[Job]) -> int:\n"
        "    return sum(item.weight for item in items)\n",
        encoding="utf-8",
    )
    (workspace / "pubspec.yaml").write_text(
        "name: rriter_pgo_fixture\nversion: 0.0.0\npublish_to: none\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n",
        encoding="utf-8",
    )
    (workspace / "lib" / "pgo_training.dart").write_text(
        _dart_fixture_source(), encoding="utf-8"
    )
    large = ["// Deterministic large Rust file used for editor render and scroll training.\n"]
    for index in range(6000):
        large.append(
            f"pub fn generated_{index}(value: u64) -> u64 {{ "
            f"value.wrapping_mul({index + 3}).rotate_left({index % 63}) }}\n"
        )
    (workspace / "src" / "large.rs").write_text("".join(large), encoding="utf-8")
    (workspace / "openapi.json").write_text(
        json.dumps(_large_openapi_fixture(), ensure_ascii=False, indent=2)
        + "\n",
        encoding="utf-8",
    )
    _copy_python_test_fixtures(workspace)
