#!/usr/bin/env python3
"""Create, validate, and consume RRiter PGO profiles on Linux, Windows, and macOS.

The training process launches the real RRiter executable with its native winit
window and an opt-in internal Rust automation controller. User configuration is
never touched: HOME/XDG/APPDATA are redirected into a disposable state folder.
"""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import platform
import shlex
import shutil
import signal
import subprocess
import sys
import threading
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Mapping, Sequence

from pgo_postgres_fixture import (
    PGO_DATABASE_NAME,
    PGO_DATABASE_USER,
    LocalPostgresFixture,
    PostgresFixtureTelemetry,
)

ROOT = Path(__file__).resolve().parents[1]
SCENARIO_VERSION = 17
FIXTURE_VERSION = 7
DEFAULT_TIMEOUT_SECONDS = 600
OPENAPI_BULK_PATH_COUNT = 512
OPENAPI_SCHEMA_COUNT = 192
OPENAPI_BULK_METHODS = ("get", "post", "patch", "delete")
LOCAL_API_MARKER = "RRITER_PGO_LOCAL_API_OK"
LOCAL_API_TOKEN = "rriter-pgo-bearer-token"
PGO_DATABASE_ENV_HOST = "RRITER_PGO_DATABASE_HOST"
PGO_DATABASE_ENV_PORT = "RRITER_PGO_DATABASE_PORT"
PGO_DATABASE_ENV_NAME = "RRITER_PGO_DATABASE_NAME"
PGO_DATABASE_ENV_USER = "RRITER_PGO_DATABASE_USER"
REQUIRED_DATABASE_SQL_FAMILIES = frozenset(
    {
        "list_databases",
        "list_public_tables",
        "table_metadata",
        "table_constraints",
        "table_indexes",
        "table_count",
        "table_chunk",
        "completion_columns",
        "completion_enums",
        "completion_functions",
        "completion_operators",
        "user_select",
        "explain",
        "begin",
        "set_local",
        "update_returning",
        "rollback",
    }
)


class PgoError(RuntimeError):
    pass


class _PgoApiServer(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self) -> None:
        super().__init__(("127.0.0.1", 0), _PgoApiHandler)
        self.request_count = 0
        self.last_request: dict[str, object] = {}


class _PgoApiHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self) -> None:  # noqa: N802 - required by BaseHTTPRequestHandler
        parsed = urllib.parse.urlsplit(self.path)
        authorization = self.headers.get("Authorization", "")
        expected_path = "/api/v1/automation/ping"
        accepted = (
            parsed.path == expected_path
            and authorization == f"Bearer {LOCAL_API_TOKEN}"
        )
        payload = {
            "marker": LOCAL_API_MARKER,
            "accepted": accepted,
            "method": self.command,
            "path": parsed.path,
            "authorization": authorization,
        }
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        server = self.server
        if isinstance(server, _PgoApiServer):
            server.request_count += 1
            server.last_request = dict(payload)
        self.send_response(200 if accepted else 401)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:
        print(f"[rriter-pgo-api] {format % args}", flush=True)


@dataclass
class LocalApiServer:
    server: _PgoApiServer
    thread: threading.Thread

    @classmethod
    def start(cls) -> "LocalApiServer":
        server = _PgoApiServer()
        thread = threading.Thread(
            target=server.serve_forever,
            name="rriter-pgo-local-api",
            daemon=True,
        )
        thread.start()
        print(
            f"[rriter-pgo] local API fixture: http://127.0.0.1:{server.server_port}/api/v1",
            flush=True,
        )
        return cls(server=server, thread=thread)

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.server.server_port}/api/v1"

    def stop(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)


@dataclass(frozen=True)
class PgoPaths:
    root: Path
    target: str
    profile_dir: Path
    generate_target_dir: Path
    use_target_dir: Path
    training_dir: Path
    fixture_dir: Path
    state_dir: Path
    report_path: Path
    merged_profile: Path
    summary_path: Path
    manifest_path: Path


@dataclass(frozen=True)
class PgoConfig:
    root: Path = ROOT
    target: str = ""
    binary_name: str = "rriter"
    mode: str = "fresh"
    rustflags: str = ""
    build_std: bool = False
    timeout_seconds: int = DEFAULT_TIMEOUT_SECONDS
    profile_path: Path | None = None
    cargo_env: Mapping[str, str] = field(default_factory=dict)
    cargo_command: tuple[str, ...] = ("cargo", "+nightly")
    train_only: bool = False
    run_only: bool = False
    run_executable: Path | None = None
    verbose: bool = True

    def validate(self) -> "PgoConfig":
        if self.mode not in {"fresh", "reuse"}:
            raise PgoError(f"unsupported PGO mode: {self.mode}")
        if not self.target:
            raise PgoError("a Rust target triple is required")
        if self.timeout_seconds < 30:
            raise PgoError("PGO automation timeout must be at least 30 seconds")
        if self.train_only and self.mode != "fresh":
            raise PgoError("--train-only requires fresh profile generation")
        if self.run_only and self.mode != "fresh":
            raise PgoError("--run-only requires fresh mode")
        if self.run_only and self.train_only:
            raise PgoError("--run-only cannot be combined with --train-only")
        if self.run_executable is not None and not self.run_only:
            raise PgoError("--run-executable requires --run-only")
        return self


class Runner:
    def __init__(self, *, verbose: bool = True) -> None:
        self.verbose = verbose

    def run(
        self,
        command: Sequence[str | os.PathLike[str]],
        *,
        cwd: Path,
        env: Mapping[str, str] | None = None,
        timeout: int | None = None,
        capture: bool = False,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        printable = subprocess.list2cmdline([os.fspath(part) for part in command])
        if self.verbose:
            print(f"[rriter-pgo] $ {printable}", flush=True)
        return subprocess.run(
            [os.fspath(part) for part in command],
            cwd=cwd,
            env=dict(env) if env is not None else None,
            timeout=timeout,
            check=check,
            text=True,
            stdout=subprocess.PIPE if capture else None,
            stderr=subprocess.PIPE if capture else None,
        )

    def run_process_tree(
        self,
        command: Sequence[str | os.PathLike[str]],
        *,
        cwd: Path,
        env: Mapping[str, str],
        timeout: int,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        """Run a GUI process in its own process group and bound its whole tree."""

        arguments = [os.fspath(part) for part in command]
        if self.verbose:
            print(
                f"[rriter-pgo] $ {subprocess.list2cmdline(arguments)}",
                flush=True,
            )
        kwargs: dict[str, object] = {}
        if os.name == "nt":
            kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
        else:
            kwargs["start_new_session"] = True
        process = subprocess.Popen(
            arguments,
            cwd=cwd,
            env=dict(env),
            text=True,
            **kwargs,
        )
        try:
            return_code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self._terminate_process_tree(process)
            raise
        completed = subprocess.CompletedProcess(arguments, return_code)
        if check and return_code != 0:
            raise subprocess.CalledProcessError(return_code, arguments)
        return completed

    @staticmethod
    def _terminate_process_tree(process: subprocess.Popen[str]) -> None:
        if os.name == "nt":
            try:
                process.send_signal(signal.CTRL_BREAK_EVENT)
                process.wait(timeout=5)
                return
            except (OSError, subprocess.TimeoutExpired):
                pass
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if process.poll() is None:
                process.kill()
        else:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                return
            try:
                process.wait(timeout=5)
                return
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    return
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def _slug(value: str) -> str:
    return "".join(ch if ch.isalnum() or ch in "-_." else "_" for ch in value)


def paths_for(config: PgoConfig) -> PgoPaths:
    root = config.root.resolve()
    target_slug = _slug(config.target)
    profile_dir = root / "target" / "pgo-profiles" / target_slug
    if config.profile_path:
        profile_path = config.profile_path
        if not profile_path.is_absolute():
            profile_path = root / profile_path
        merged = profile_path.resolve()
    else:
        merged = profile_dir / "merged.profdata"
    manifest = merged.with_suffix(merged.suffix + ".json")
    summary = merged.with_suffix(merged.suffix + ".summary.txt")
    training = root / "target" / "pgo-training" / target_slug
    return PgoPaths(
        root=root,
        target=config.target,
        profile_dir=profile_dir,
        generate_target_dir=root / "target" / "pgo-generate" / target_slug,
        use_target_dir=root / "target" / "pgo-use" / target_slug,
        training_dir=training,
        fixture_dir=training / "workspace",
        state_dir=training / "state",
        report_path=training / "automation-report.json",
        merged_profile=merged,
        summary_path=summary,
        manifest_path=manifest,
    )


def executable_path(target_dir: Path, target: str, binary_name: str) -> Path:
    suffix = ".exe" if target.endswith("windows-msvc") or target.endswith("windows-gnu") else ""
    return target_dir / target / "release" / f"{binary_name}{suffix}"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_fingerprint(root: Path) -> str:
    """Hash the Rust sources that determine whether a saved profile is fresh."""

    candidates = [
        root / "Cargo.toml",
        root / "Cargo.lock",
        root / "build.rs",
        root / "rust-toolchain.toml",
    ]
    candidates.extend(sorted((root / "src").rglob("*.rs")))
    digest = hashlib.sha256()
    for path in candidates:
        if not path.is_file():
            continue
        relative = path.relative_to(root).as_posix().encode("utf-8")
        digest.update(len(relative).to_bytes(4, "big"))
        digest.update(relative)
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
    return digest.hexdigest()


def rustc_identity(config: PgoConfig, runner: Runner) -> str:
    result = runner.run(
        ["rustup", "run", "nightly", "rustc", "-Vv"],
        cwd=config.root,
        env=base_environment(config),
        capture=True,
    )
    return result.stdout.strip()


def llvm_profdata_identity(config: PgoConfig, runner: Runner) -> str:
    result = runner.run(
        llvm_profdata_command("--version"),
        cwd=config.root,
        env=base_environment(config),
        capture=True,
    )
    return result.stdout.strip()


def base_environment(config: PgoConfig) -> dict[str, str]:
    environment = dict(os.environ)
    environment.update({str(key): str(value) for key, value in config.cargo_env.items()})
    return environment


def build_environment(
    config: PgoConfig,
    *,
    target_dir: Path,
    pgo_flags: Sequence[str],
) -> dict[str, str]:
    environment = base_environment(config)
    environment["CARGO_TARGET_DIR"] = str(target_dir)
    try:
        rustflags = shlex.split(config.rustflags) if config.rustflags.strip() else []
    except ValueError as error:
        raise PgoError(f"invalid --rustflags value: {error}") from error
    rustflags.extend(pgo_flags)
    environment.pop("RUSTFLAGS", None)
    if rustflags:
        # Cargo's unit-separator form keeps paths with spaces as one rustc
        # argument on Windows, macOS, and Linux without shell quoting.
        environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(rustflags)
    else:
        environment.pop("CARGO_ENCODED_RUSTFLAGS", None)
    return environment


def instrumented_build_environment(
    config: PgoConfig,
    *,
    target_dir: Path,
    pgo_flags: Sequence[str],
) -> dict[str, str]:
    environment = build_environment(
        config,
        target_dir=target_dir,
        pgo_flags=pgo_flags,
    )
    # LLVM's profiler runtime is not compatible with Rust's immediate-abort
    # strategy. Keep immediate-abort for the final profile-use build only.
    environment["CARGO_PROFILE_RELEASE_PANIC"] = "abort"
    return environment


def cargo_build_command(config: PgoConfig) -> list[str]:
    command = [*config.cargo_command, "build", "--locked"]
    if config.build_std:
        command.extend(["-Z", "build-std=core,alloc,std,panic_abort,test"])
    command.extend(
        ["--target", config.target, "--release", "--bin", config.binary_name]
    )
    return command


def build_instrumented(config: PgoConfig, paths: PgoPaths, runner: Runner) -> Path:
    paths.profile_dir.mkdir(parents=True, exist_ok=True)
    flag = f"-Cprofile-generate={paths.profile_dir}"
    configured_panic = config.cargo_env.get("CARGO_PROFILE_RELEASE_PANIC")
    if runner.verbose and configured_panic == "immediate-abort":
        print(
            "[rriter-pgo] instrumented build uses panic=abort; "
            "the final profile-use build keeps immediate-abort",
            flush=True,
        )
    runner.run(
        cargo_build_command(config),
        cwd=paths.root,
        env=instrumented_build_environment(
            config,
            target_dir=paths.generate_target_dir,
            pgo_flags=[flag],
        ),
    )
    executable = executable_path(paths.generate_target_dir, config.target, config.binary_name)
    if not executable.is_file():
        raise PgoError(f"instrumented RRiter executable not found: {executable}")
    return executable


def build_with_profile(config: PgoConfig, paths: PgoPaths, runner: Runner) -> Path:
    validate_profile(config, paths, runner)
    flags = [
        f"-Cprofile-use={paths.merged_profile}",
        "-Cllvm-args=-pgo-warn-missing-function",
    ]
    runner.run(
        cargo_build_command(config),
        cwd=paths.root,
        env=build_environment(
            config,
            target_dir=paths.use_target_dir,
            pgo_flags=flags,
        ),
    )
    executable = executable_path(paths.use_target_dir, config.target, config.binary_name)
    if not executable.is_file():
        raise PgoError(f"PGO RRiter executable not found: {executable}")
    return executable


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
        "Unicode: \u043a\u0438\u0440\u0438\u043b\u043b\u0438\u0446\u0430 \U0001f600 \u2014 \u043f\u0440\u043e\u0432\u0435\u0440\u043a\u0430 UTF-8 \u0433\u0440\u0430\u043d\u0438\u0446.\n\n"
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
        "| \u043a\u0438\u0440\u0438\u043b\u043b\u0438\u0446\u0430 | \U0001f600 | 42 |\n\n"
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


def _set_openapi_server_url(openapi_path: Path, base_url: str) -> None:
    try:
        document = json.loads(openapi_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PgoError(f"cannot update OpenAPI fixture server: {error}") from error
    document["servers"] = [
        {
            "url": base_url,
            "description": "Live local server started by pgo_pipeline.py",
        }
    ]
    openapi_path.write_text(
        json.dumps(document, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def create_fixture(paths: PgoPaths) -> None:
    if paths.training_dir.exists():
        shutil.rmtree(paths.training_dir)
    paths.fixture_dir.mkdir(parents=True)
    paths.state_dir.mkdir(parents=True)
    _write_fixture_files(paths.fixture_dir)


def isolated_runtime_environment(
    config: PgoConfig,
    paths: PgoPaths,
    *,
    profile_dir: Path | None = None,
    database_endpoint: tuple[str, int] | None = None,
) -> dict[str, str]:
    environment = base_environment(config)
    home = paths.state_dir / "home"
    xdg_config = paths.state_dir / "xdg-config"
    xdg_cache = paths.state_dir / "xdg-cache"
    xdg_data = paths.state_dir / "xdg-data"
    xdg_state = paths.state_dir / "xdg-state"
    appdata = paths.state_dir / "appdata"
    localappdata = paths.state_dir / "localappdata"
    for directory in (
        home,
        xdg_config,
        xdg_cache,
        xdg_data,
        xdg_state,
        appdata,
        localappdata,
    ):
        directory.mkdir(parents=True, exist_ok=True)
    runtime_profile_dir = paths.profile_dir if profile_dir is None else profile_dir
    runtime_profile_dir.mkdir(parents=True, exist_ok=True)
    environment.update(
        {
            "HOME": str(home),
            "USERPROFILE": str(home),
            "XDG_CONFIG_HOME": str(xdg_config),
            "XDG_CACHE_HOME": str(xdg_cache),
            "XDG_DATA_HOME": str(xdg_data),
            "XDG_STATE_HOME": str(xdg_state),
            "APPDATA": str(appdata),
            "LOCALAPPDATA": str(localappdata),
            "LLVM_PROFILE_FILE": str(runtime_profile_dir / "rriter-%p-%m.profraw"),
            "RRITER_PGO_AUTOMATION": "1",
            "RUST_BACKTRACE": "1",
            "NO_PROXY": "127.0.0.1,localhost",
            "no_proxy": "127.0.0.1,localhost",
        }
    )
    if database_endpoint is not None:
        host, port = database_endpoint
        if host != "127.0.0.1" or not (1 <= port <= 65535):
            raise PgoError(
                "PGO database fixture endpoint must be an IPv4 loopback port; "
                f"got {host}:{port}"
            )
        environment.update(
            {
                PGO_DATABASE_ENV_HOST: host,
                PGO_DATABASE_ENV_PORT: str(port),
                PGO_DATABASE_ENV_NAME: PGO_DATABASE_NAME,
                PGO_DATABASE_ENV_USER: PGO_DATABASE_USER,
            }
        )
    if "linux" in config.target:
        environment["WINIT_UNIX_BACKEND"] = "wayland"
    return environment


def database_fixture_telemetry_payload(
    telemetry: PostgresFixtureTelemetry,
) -> dict[str, object]:
    return {
        "accepted_connection_count": telemetry.accepted_connection_count,
        "startup_count": telemetry.startup_count,
        "ssl_request_count": telemetry.ssl_request_count,
        "sql_families": list(telemetry.sql_families),
        "family_counts": dict(telemetry.family_counts),
        "protocol_errors": list(telemetry.protocol_errors),
        "unexpected_sql": list(telemetry.unexpected_sql),
        "peer_disconnects": list(telemetry.peer_disconnects),
        "worker_errors": list(telemetry.worker_errors),
    }


def validate_database_fixture_telemetry(
    telemetry: PostgresFixtureTelemetry,
) -> None:
    if telemetry.accepted_connection_count < 1 or telemetry.startup_count < 1:
        raise PgoError(
            "RRiter did not establish a PostgreSQL wire-protocol session with the PGO fixture; "
            f"accepted={telemetry.accepted_connection_count} startup={telemetry.startup_count}"
        )
    problems: list[str] = []
    if telemetry.ssl_request_count:
        problems.append(f"ssl_request_count={telemetry.ssl_request_count}")
    if telemetry.protocol_errors:
        problems.append(f"protocol_errors={telemetry.protocol_errors}")
    if telemetry.unexpected_sql:
        problems.append(f"unexpected_sql={telemetry.unexpected_sql}")
    if telemetry.worker_errors:
        problems.append(f"worker_errors={telemetry.worker_errors}")
    if problems:
        raise PgoError("PGO PostgreSQL fixture reported errors: " + "; ".join(problems))
    missing = sorted(
        family
        for family in REQUIRED_DATABASE_SQL_FAMILIES
        if telemetry.family_count(family) < 1
    )
    if missing:
        observed = sorted(name for name, count in telemetry.family_counts if count > 0)
        raise PgoError(
            "PGO database workload did not exercise required production SQL families; "
            f"missing={missing} observed={observed}"
        )


def validate_training_environment(config: PgoConfig) -> None:
    target = config.target
    environment = base_environment(config)
    if "windows" in target and os.name != "nt":
        raise PgoError("Windows PGO training must run on Windows")
    if "apple-darwin" in target and sys.platform != "darwin":
        raise PgoError("macOS PGO training must run on macOS")
    if "linux" in target:
        if not sys.platform.startswith("linux"):
            raise PgoError("Linux PGO training must run on Linux")
        if not environment.get("WAYLAND_DISPLAY"):
            raise PgoError(
                "Linux PGO training requires a live Wayland session "
                "(WAYLAND_DISPLAY is not set)"
            )


def describe_pgo_process_failure(returncode: int, *, os_name: str | None = None) -> str:
    platform_name = os.name if os_name is None else os_name
    if platform_name == "posix" and returncode < 0:
        signal_number = -returncode
        try:
            signal_name = signal.Signals(signal_number).name
        except ValueError:
            signal_name = f"signal {signal_number}"
        return f"RRiter PGO process terminated by {signal_name} ({signal_number})"
    return f"RRiter PGO process exited with code {returncode}"


def automation_failure_message(report: Mapping[str, object], report_path: Path) -> str:
    index = report.get("failed_step_index")
    name = report.get("failed_step_name")
    reason = (
        report.get("failure_reason")
        or report.get("failed_step")
        or report.get("status")
        or "unknown error"
    )
    previous = report.get("previous_completed_step")
    return (
        "RRiter automation failed "
        f"step={index if index is not None else 'unknown'} "
        f"name={name if name is not None else 'unknown'} "
        f"reason={reason} "
        f"previous={previous if previous is not None else 'none'} "
        f"report={report_path}"
    )


def run_training(
    config: PgoConfig,
    paths: PgoPaths,
    executable: Path,
    runner: Runner,
    *,
    profile_dir: Path | None = None,
) -> dict[str, object]:
    command = [
        executable,
        "--ide",
        "--pgo-train",
        "--pgo-workspace",
        paths.fixture_dir,
        "--pgo-report",
        paths.report_path,
        "--pgo-timeout-seconds",
        str(config.timeout_seconds),
    ]
    database_fixture = LocalPostgresFixture()
    api_server = LocalApiServer.start()
    try:
        database_fixture.start()
        database_endpoint = database_fixture.endpoint
        print(
            "[rriter-pgo] local PostgreSQL fixture: "
            f"{database_endpoint[0]}:{database_endpoint[1]}/{PGO_DATABASE_NAME}",
            flush=True,
        )
        _set_openapi_server_url(paths.fixture_dir / "openapi.json", api_server.base_url)
        result = runner.run_process_tree(
            command,
            cwd=paths.fixture_dir,
            env=isolated_runtime_environment(
                config,
                paths,
                profile_dir=profile_dir,
                database_endpoint=database_endpoint,
            ),
            timeout=config.timeout_seconds + 45,
            check=False,
        )
        local_request_count = api_server.server.request_count
        local_request = dict(api_server.server.last_request)
    finally:
        database_fixture.stop()
        api_server.stop()
    database_telemetry = database_fixture.telemetry()
    report: dict[str, object] | None = None
    report_error: Exception | None = None
    if paths.report_path.is_file():
        try:
            loaded_report = json.loads(paths.report_path.read_text(encoding="utf-8"))
            if not isinstance(loaded_report, dict):
                raise PgoError("automation report root must be an object")
            report = loaded_report
        except (OSError, json.JSONDecodeError, PgoError) as error:
            report_error = error

    if result.returncode != 0:
        message = describe_pgo_process_failure(result.returncode)
        if report is not None:
            if report.get("status") == "success":
                message += f"; automation report status=success report={paths.report_path}"
            else:
                message += "; " + automation_failure_message(report, paths.report_path)
        elif report_error is not None:
            message += (
                f"; structured automation report is invalid: {report_error}; "
                "inspect the last PGO_AUTOMATION_STEP_START printed above"
            )
        else:
            message += (
                f"; structured automation report is absent: {paths.report_path}; "
                "inspect the last PGO_AUTOMATION_STEP_START printed above"
            )
        raise PgoError(message)
    if not paths.report_path.is_file():
        raise PgoError(
            "RRiter exited with code 0 before writing the structured automation report; "
            "the window may have been closed before the scenario finished; "
            "inspect the last PGO_AUTOMATION_STEP_START printed above: "
            f"{paths.report_path}"
        )
    if report_error is not None:
        raise PgoError(f"invalid automation report: {report_error}") from report_error
    if report is None:
        raise PgoError(f"automation report could not be loaded: {paths.report_path}")
    if report.get("status") != "success":
        raise PgoError(automation_failure_message(report, paths.report_path))
    if int(report.get("scenario_version", -1)) != SCENARIO_VERSION:
        raise PgoError("automation report scenario version does not match the pipeline")
    validate_database_fixture_telemetry(database_telemetry)
    if local_request_count < 1 or not local_request.get("accepted"):
        raise PgoError(
            "RRiter did not complete the authenticated local API request; "
            f"count={local_request_count} last_request={local_request}"
        )
    report["local_api_requests"] = local_request_count
    report["local_api_last_request"] = local_request
    report["database_fixture"] = database_fixture_telemetry_payload(database_telemetry)
    paths.report_path.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return report


def existing_run_executable(config: PgoConfig, paths: PgoPaths) -> Path:
    if config.run_executable is None:
        executable = executable_path(
            paths.generate_target_dir,
            config.target,
            config.binary_name,
        )
        rebuild_command = "make pgo-gen"
        description = "instrumented"
    else:
        executable = config.run_executable
        if not executable.is_absolute():
            executable = paths.root / executable
        executable = executable.resolve()
        rebuild_command = "make pgo-gen-fast"
        description = "fast automation"

    if not executable.is_file():
        raise PgoError(
            f"{description} RRiter executable is missing: {executable}; "
            f"run `{rebuild_command}` before `make pgo-script`"
        )
    automation_sources = [
        paths.root / "src" / "app" / "automation.rs",
        paths.root / "src" / "app" / "automation_database.rs",
    ]
    stale_source = next(
        (
            source
            for source in automation_sources
            if source.is_file() and executable.stat().st_mtime_ns < source.stat().st_mtime_ns
        ),
        None,
    )
    if stale_source is not None:
        relative_source = stale_source.relative_to(paths.root)
        raise PgoError(
            f"{description} RRiter is older than {relative_source}; "
            f"run `{rebuild_command}` and repeat `make pgo-script`"
        )
    return executable


def raw_profiles(paths: PgoPaths) -> list[Path]:
    return sorted(
        path
        for path in paths.profile_dir.glob("*.profraw")
        if path.is_file() and path.stat().st_size > 0
    )


def llvm_profdata_command(
    *arguments: str | os.PathLike[str],
) -> list[str | os.PathLike[str]]:
    return ["rustup", "run", "nightly", "llvm-profdata", *arguments]


def merge_profiles(config: PgoConfig, paths: PgoPaths, runner: Runner) -> list[Path]:
    profiles = raw_profiles(paths)
    if not profiles:
        raise PgoError(f"no non-empty .profraw files were created in {paths.profile_dir}")
    paths.merged_profile.parent.mkdir(parents=True, exist_ok=True)
    runner.run(
        llvm_profdata_command(
            "merge",
            "-sparse",
            "-o",
            paths.merged_profile,
            *profiles,
        ),
        cwd=paths.root,
        env=base_environment(config),
    )
    if not paths.merged_profile.is_file() or paths.merged_profile.stat().st_size == 0:
        raise PgoError(f"llvm-profdata did not create {paths.merged_profile}")
    summary = runner.run(
        llvm_profdata_command(
            "show",
            "--counts",
            paths.merged_profile,
        ),
        cwd=paths.root,
        env=base_environment(config),
        capture=True,
    )
    if not summary.stdout.strip():
        raise PgoError("llvm-profdata produced an empty profile summary")
    paths.summary_path.write_text(summary.stdout, encoding="utf-8")
    return profiles


def compatibility_payload(config: PgoConfig, runner: Runner) -> dict[str, object]:
    lock = config.root / "Cargo.lock"
    if not lock.is_file():
        raise PgoError(f"Cargo.lock not found: {lock}")
    profile_environment = {
        key: str(value)
        for key, value in sorted(config.cargo_env.items())
        if key.startswith("CARGO_PROFILE_")
        or key in {"MACOSX_DEPLOYMENT_TARGET", "RRITER_WINDOWS_RESOURCE"}
    }
    return {
        "schema": 1,
        "scenario_version": SCENARIO_VERSION,
        "fixture_version": FIXTURE_VERSION,
        "target": config.target,
        "rustc": rustc_identity(config, runner),
        "llvm_profdata": llvm_profdata_identity(config, runner),
        "rustflags": shlex.split(config.rustflags) if config.rustflags.strip() else [],
        "build_std": config.build_std,
        "cargo_toml_sha256": sha256_file(config.root / "Cargo.toml"),
        "cargo_lock_sha256": sha256_file(lock),
        "source_sha256": source_fingerprint(config.root),
        "profile_environment": profile_environment,
    }


def write_manifest(
    config: PgoConfig,
    paths: PgoPaths,
    runner: Runner,
    profiles: Sequence[Path],
    report: Mapping[str, object],
) -> None:
    payload = compatibility_payload(config, runner)
    payload.update(
        {
            "profile_sha256": sha256_file(paths.merged_profile),
            "profile_summary_sha256": sha256_file(paths.summary_path),
            "raw_profile_count": len(profiles),
            "automation_completed_steps": report.get("completed_steps", []),
            "automation_skipped_steps": report.get("skipped_steps", []),
            "automation_report_sha256": sha256_file(paths.report_path),
            "created_unix_seconds": int(time.time()),
            "host": {
                "system": platform.system(),
                "machine": platform.machine(),
            },
        }
    )
    paths.manifest_path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def validate_profile(config: PgoConfig, paths: PgoPaths, runner: Runner) -> None:
    if not paths.merged_profile.is_file() or paths.merged_profile.stat().st_size == 0:
        raise PgoError(
            f"PGO profile not found: {paths.merged_profile}. "
            "Run the fresh PGO pipeline first."
        )
    if not paths.manifest_path.is_file():
        raise PgoError(
            f"PGO manifest not found: {paths.manifest_path}. "
            "Profiles without compatibility metadata are not reused automatically."
        )
    try:
        manifest = json.loads(paths.manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PgoError(f"invalid PGO manifest: {error}") from error
    expected = compatibility_payload(config, runner)
    mismatches = [
        key
        for key, value in expected.items()
        if manifest.get(key) != value
    ]
    if manifest.get("profile_sha256") != sha256_file(paths.merged_profile):
        mismatches.append("profile_sha256")
    if not paths.summary_path.is_file() or manifest.get(
        "profile_summary_sha256"
    ) != sha256_file(paths.summary_path):
        mismatches.append("profile_summary_sha256")
    if mismatches:
        raise PgoError(
            "saved PGO profile is incompatible ("
            + ", ".join(sorted(set(mismatches)))
            + "). Create a fresh profile."
        )
    summary = runner.run(
        llvm_profdata_command(
            "show",
            "--counts",
            paths.merged_profile,
        ),
        cwd=paths.root,
        env=base_environment(config),
        capture=True,
    )
    if not summary.stdout.strip():
        raise PgoError("saved PGO profile has an empty llvm-profdata summary")


def run_pipeline(config: PgoConfig, *, runner: Runner | None = None) -> Path | None:
    config = config.validate()
    runner = Runner(verbose=config.verbose) if runner is None else runner
    paths = paths_for(config)
    if config.run_only:
        validate_training_environment(config)
        executable = existing_run_executable(config, paths)
        create_fixture(paths)
        script_profiles = paths.training_dir / "script-profiles"
        report = run_training(
            config,
            paths,
            executable,
            runner,
            profile_dir=script_profiles,
        )
        print(f"[rriter-pgo] automation report: {paths.report_path}", flush=True)
        print(
            "[rriter-pgo] script-only run completed; no build, merge, or PGO-use "
            "build was performed",
            flush=True,
        )
        if report.get("status") != "success":
            raise PgoError("script-only automation did not complete successfully")
        return None
    if config.mode == "fresh":
        validate_training_environment(config)
        if paths.profile_dir.exists():
            shutil.rmtree(paths.profile_dir)
        paths.profile_dir.mkdir(parents=True, exist_ok=True)
        create_fixture(paths)
        executable = build_instrumented(config, paths, runner)
        report = run_training(config, paths, executable, runner)
        profiles = merge_profiles(config, paths, runner)
        write_manifest(config, paths, runner, profiles, report)
        print(f"[rriter-pgo] profile: {paths.merged_profile}", flush=True)
        if config.train_only:
            return None
    return build_with_profile(config, paths, runner)


def parse_env(values: Sequence[str]) -> dict[str, str]:
    environment: dict[str, str] = {}
    for value in values:
        key, separator, item = value.partition("=")
        if not separator or not key:
            raise PgoError(f"invalid --env value {value!r}; expected NAME=VALUE")
        environment[key] = item
    return environment


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target")
    parser.add_argument("--mode", choices=["fresh", "reuse"], default="fresh")
    parser.add_argument("--binary-name", default="rriter")
    parser.add_argument("--rustflags", default="")
    parser.add_argument("--build-std", action="store_true")
    parser.add_argument("--timeout-seconds", type=int, default=DEFAULT_TIMEOUT_SECONDS)
    parser.add_argument("--profile", type=Path)
    parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE")
    parser.add_argument("--train-only", action="store_true")
    parser.add_argument("--run-only", action="store_true")
    parser.add_argument("--run-executable", type=Path)
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def self_test() -> None:
    with __import__("tempfile").TemporaryDirectory(prefix="rriter-pgo-selftest-") as directory:
        root = Path(directory)
        (root / "Cargo.lock").write_text("# fixture\n", encoding="utf-8")
        (root / "Cargo.toml").write_text("[package]\nname='fixture'\n", encoding="utf-8")
        (root / "src").mkdir()
        source = root / "src" / "main.rs"
        source.write_text("fn main() {}\n", encoding="utf-8")
        config = PgoConfig(root=root, target="x86_64-unknown-linux-gnu")
        paths = paths_for(config)
        if executable_path(paths.use_target_dir, config.target, "rriter").name != "rriter":
            raise PgoError("Unix executable path self-test failed")
        if executable_path(paths.use_target_dir, "x86_64-pc-windows-msvc", "rriter").name != "rriter.exe":
            raise PgoError("Windows executable path self-test failed")
        paths.fixture_dir.mkdir(parents=True)
        _write_fixture_files(paths.fixture_dir)
        required = [
            paths.fixture_dir / "src" / "main.rs",
            paths.fixture_dir / "src" / "worker.py",
            paths.fixture_dir / "src" / "large.rs",
            paths.fixture_dir / "pubspec.yaml",
            paths.fixture_dir / "lib" / "pgo_training.dart",
            paths.fixture_dir / "openapi.json",
            paths.fixture_dir / "tests" / "pgo_completion_hover.py",
            paths.fixture_dir / ".rriter-pgo-python-tests.json",
        ]
        if not all(path.is_file() for path in required):
            raise PgoError("fixture self-test failed")
        dart_text = (paths.fixture_dir / "lib" / "pgo_training.dart").read_text(
            encoding="utf-8"
        )
        pubspec_text = (paths.fixture_dir / "pubspec.yaml").read_text(encoding="utf-8")
        generated_dart = _dart_fixture_source()
        if generated_dart != _dart_fixture_source() or dart_text != generated_dart:
            raise PgoError("Dart fixture generation is not deterministic")
        if len(dart_text.splitlines()) < 600 or dart_text.count("while (cursor > 0)") < 30:
            raise PgoError("Dart fixture is unexpectedly small or lacks nested blocks")
        for marker in (
            "import 'dart:async';",
            "extension PgoIterableExtension",
            "Future<int> pgoDartTarget",
            "pgoDartCompletionTarget",
            "switch (state)",
            "// pgoDartEditTarget",
        ):
            if marker not in dart_text:
                raise PgoError(f"Dart fixture marker is missing: {marker}")
        if (
            "package:" in dart_text
            or "dependencies:" in pubspec_text
            or "http://" in pubspec_text
            or "https://" in pubspec_text
        ):
            raise PgoError("Dart fixture must remain offline and dependency-free")
        openapi_path = paths.fixture_dir / "openapi.json"
        openapi_fixture = json.loads(openapi_path.read_text(encoding="utf-8"))
        fixture_paths = openapi_fixture.get("paths", {})
        route_count = sum(
            1
            for path_item in fixture_paths.values()
            if isinstance(path_item, dict)
            for method in OPENAPI_BULK_METHODS
            if method in path_item
        )
        featured = fixture_paths.get("/automation/featured/{resource_id}", {})
        if featured.get("post", {}).get("operationId") != "PGO_FEATURED_WRITE":
            raise PgoError("featured OpenAPI route self-test failed")
        ping = fixture_paths.get("/automation/ping", {})
        if ping.get("get", {}).get("operationId") != "PGO_LOCAL_SERVER_PING":
            raise PgoError("local API OpenAPI route self-test failed")
        if len(fixture_paths) != OPENAPI_BULK_PATH_COUNT + 2:
            raise PgoError("large OpenAPI path count self-test failed")
        if route_count != OPENAPI_BULK_PATH_COUNT * len(OPENAPI_BULK_METHODS) + 2:
            raise PgoError("large OpenAPI route count self-test failed")
        if openapi_path.stat().st_size < 2_000_000:
            raise PgoError("large OpenAPI fixture is unexpectedly small")
        python_manifest = json.loads(
            (paths.fixture_dir / ".rriter-pgo-python-tests.json").read_text(encoding="utf-8")
        )
        python_files = python_manifest.get("files", [])
        if "tests/pgo_completion_hover.py" not in python_files:
            raise PgoError("completion/hover Python fixture is missing from manifest")
        copied_perf = [name for name in python_files if name.startswith("tests/perf_")]
        if len(copied_perf) < 6:
            raise PgoError("not all complex Python performance fixtures were copied")
        expected_python = {
            f"tests/{source.name}" for source in (ROOT / "tests").glob("*.py") if source.is_file()
        }
        if not expected_python.issubset(set(python_files)):
            missing = sorted(expected_python.difference(python_files))
            raise PgoError(f"Python fixture copy is incomplete: {missing}")
        completion_text = (paths.fixture_dir / "tests" / "pgo_completion_hover.py").read_text(
            encoding="utf-8"
        )
        if "pgo_completion_target" not in completion_text or "pgo_hover_target" not in completion_text:
            raise PgoError("completion/hover fixture markers are missing")
        if "pgo_completion_result = pri" not in completion_text:
            raise PgoError("deterministic builtin completion marker is missing")
        automation_source = (ROOT / "src" / "app" / "automation.rs").read_text(
            encoding="utf-8"
        )
        expected_version = (
            f"PGO_AUTOMATION_SCENARIO_VERSION: u32 = {SCENARIO_VERSION};"
        )
        if expected_version not in automation_source:
            raise PgoError(
                "Python and Rust PGO scenario versions differ: "
                f"expected {SCENARIO_VERSION}"
            )
        api_server = LocalApiServer.start()
        try:
            request = urllib.request.Request(
                f"{api_server.base_url}/automation/ping",
                headers={"Authorization": f"Bearer {LOCAL_API_TOKEN}"},
            )
            with urllib.request.urlopen(request, timeout=5) as response:
                response_payload = json.loads(response.read().decode("utf-8"))
        finally:
            api_server.stop()
        if response_payload.get("marker") != LOCAL_API_MARKER:
            raise PgoError("local API server self-test failed")
        command = cargo_build_command(config)
        if command[-5:] != [
            "--target",
            config.target,
            "--release",
            "--bin",
            "rriter",
        ]:
            raise PgoError("Cargo build command self-test failed")
        generate_config = PgoConfig(
            root=root,
            target="x86_64-unknown-linux-gnu",
            cargo_env={"CARGO_PROFILE_RELEASE_PANIC": "immediate-abort"},
        )
        environment = instrumented_build_environment(
            generate_config,
            target_dir=paths.generate_target_dir,
            pgo_flags=[f"-Cprofile-generate={paths.profile_dir}"],
        )
        encoded = environment.get("CARGO_ENCODED_RUSTFLAGS", "").split("\x1f")
        if encoded[-1] != f"-Cprofile-generate={paths.profile_dir}":
            raise PgoError("encoded RUSTFLAGS self-test failed")
        if environment.get("CARGO_PROFILE_RELEASE_PANIC") != "abort":
            raise PgoError("instrumented build must override immediate-abort")
        use_environment = build_environment(
            generate_config,
            target_dir=paths.use_target_dir,
            pgo_flags=[f"-Cprofile-use={paths.merged_profile}"],
        )
        if use_environment.get("CARGO_PROFILE_RELEASE_PANIC") != "immediate-abort":
            raise PgoError("profile-use build must preserve immediate-abort")
        runtime_profiles = paths.training_dir / "script-profiles"
        runtime_environment = isolated_runtime_environment(
            config,
            paths,
            profile_dir=runtime_profiles,
        )
        if runtime_environment.get("LLVM_PROFILE_FILE") != str(
            runtime_profiles / "rriter-%p-%m.profraw"
        ):
            raise PgoError("script-only profile isolation self-test failed")
        automation_source = root / "src" / "app" / "automation.rs"
        automation_source.parent.mkdir(parents=True, exist_ok=True)
        automation_source.write_text("// automation fixture\n", encoding="utf-8")
        instrumented = executable_path(
            paths.generate_target_dir,
            config.target,
            config.binary_name,
        )
        instrumented.parent.mkdir(parents=True, exist_ok=True)
        instrumented.write_text("fixture executable\n", encoding="utf-8")
        now = time.time()
        os.utime(automation_source, (now - 2.0, now - 2.0))
        os.utime(instrumented, (now - 1.0, now - 1.0))
        if existing_run_executable(config, paths) != instrumented:
            raise PgoError("existing instrumented executable self-test failed")
        os.utime(automation_source, (now, now))
        try:
            existing_run_executable(config, paths)
        except PgoError:
            pass
        else:
            raise PgoError("stale instrumented executable self-test failed")
        fast_executable = root / "target" / config.target / "release" / "rriter"
        fast_executable.parent.mkdir(parents=True, exist_ok=True)
        fast_executable.write_text("fast fixture executable\n", encoding="utf-8")
        os.utime(automation_source, (now - 2.0, now - 2.0))
        os.utime(fast_executable, (now - 1.0, now - 1.0))
        fast_config = PgoConfig(
            root=root,
            target=config.target,
            run_only=True,
            run_executable=fast_executable,
        )
        if existing_run_executable(fast_config, paths) != fast_executable.resolve():
            raise PgoError("fast automation executable self-test failed")
        before = source_fingerprint(root)
        source.write_text("fn main() { println!(\"changed\"); }\n", encoding="utf-8")
        if source_fingerprint(root) == before:
            raise PgoError("source fingerprint self-test failed")
    print("[rriter-pgo] self-test passed")


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.self_test:
        self_test()
        return 0
    if not args.target:
        raise PgoError("--target is required unless --self-test is used")
    config = PgoConfig(
        root=ROOT,
        target=args.target,
        binary_name=args.binary_name,
        mode=args.mode,
        rustflags=args.rustflags,
        build_std=args.build_std,
        timeout_seconds=args.timeout_seconds,
        profile_path=args.profile,
        cargo_env=parse_env(args.env),
        train_only=args.train_only,
        run_only=args.run_only,
        run_executable=args.run_executable,
        verbose=not args.quiet,
    )
    executable = run_pipeline(config)
    if executable is not None:
        print(f"[rriter-pgo] executable: {executable}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (PgoError, subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as error:
        print(f"[rriter-pgo] ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
