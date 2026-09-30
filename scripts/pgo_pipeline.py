#!/usr/bin/env python3
"""Create, validate, and consume RRiter PGO profiles on Linux, Windows, and macOS.

The training process launches the real RRiter executable with its native winit
window and an opt-in internal Rust automation controller. User configuration is
never touched: HOME/XDG/APPDATA are redirected into a disposable state folder.
"""

from __future__ import annotations

import argparse
import collections
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
import tempfile
import threading
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import IO, Mapping, Sequence

from pgo_coverage import (
    GROUP_MARKERS,
    PgoError,
    check_markers,
    count_pgo_warnings,
    demangled_functions,
    check_new_raw_profiles,
    find_cxxfilt,
    format_coverage,
    module_summary,
    unrequired_groups,
    verify_raw_profiles,
    zero_modules,
)
from pgo_fixtures import (  # noqa: F401  (re-exported for tests and self-test)
    FIXTURE_VERSION,
    LOCAL_API_MARKER,
    OPENAPI_BULK_METHODS,
    OPENAPI_BULK_PATH_COUNT,
    OPENAPI_SCHEMA_COUNT,
    _copy_python_test_fixtures,
    _dart_fixture_source,
    _large_openapi_fixture,
    _openapi_operation,
    _openapi_schema,
    _write_fixture_files,
    find_pdfium,
    managed_ty_bin_dir,
)
from postgres_fixture import (
    DEFAULT_DATABASE_NAME,
    DEFAULT_DATABASE_USER,
    LocalPostgresFixture,
    PostgresFixtureTelemetry,
)

ROOT = Path(__file__).resolve().parents[1]
SCENARIO_VERSION = 18
DEFAULT_TIMEOUT_SECONDS = 600
ALL_SCENARIOS = ("full", "startup", "welcome")
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
    # Linux headless training runs: `full` must precede `startup` (it saves the session).
    scenarios: tuple[str, ...] = ALL_SCENARIOS
    pdfium_path: Path | None = None
    install_binary: Path | None = None

    def validate(self) -> "PgoConfig":
        if not self.scenarios:
            raise PgoError("at least one PGO scenario is required")
        for name in self.scenarios:
            if name not in ALL_SCENARIOS and not name.startswith("group:"):
                raise PgoError(f"unknown PGO scenario: {name}")
        if "startup" in self.scenarios and "full" not in self.scenarios[: self.scenarios.index("startup")]:
            raise PgoError("scenario startup restores the session saved by full; run full before startup")
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


STDERR_TAIL_LINES = 30
# The marker is printed by RRiter to stdout, which the pipeline does not capture.
STEP_HINT = "inspect the last PGO_AUTOMATION_STEP_START line in the output above (none: it failed before step 0)"


def _start_stderr_pump(stream: IO[str] | None, lines: collections.deque[str]) -> threading.Thread:
    """Copy a child's stderr to ours while keeping the lines for error reports."""

    def pump() -> None:
        if stream is None:
            return
        for line in stream:
            sys.stderr.write(line)
            sys.stderr.flush()
            lines.append(line)

    thread = threading.Thread(target=pump, name="rriter-pgo-stderr", daemon=True)
    thread.start()
    return thread


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
            errors="replace",
            stderr=subprocess.PIPE,
            **kwargs,
        )
        tail: collections.deque[str] = collections.deque(maxlen=STDERR_TAIL_LINES)
        pump = _start_stderr_pump(process.stderr, tail)
        try:
            return_code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self._terminate_process_tree(process)
            pump.join(timeout=5)
            raise
        pump.join(timeout=5)
        completed = subprocess.CompletedProcess(arguments, return_code, stderr="".join(tail))
        if check and return_code != 0:
            raise subprocess.CalledProcessError(return_code, arguments, stderr=completed.stderr)
        return completed

    def run_logged(
        self,
        command: Sequence[str | os.PathLike[str]],
        *,
        cwd: Path,
        env: Mapping[str, str],
    ) -> str:
        """Run a build command, echo its stderr live and return the whole stderr text."""

        arguments = [os.fspath(part) for part in command]
        if self.verbose:
            print(f"[rriter-pgo] $ {subprocess.list2cmdline(arguments)}", flush=True)
        process = subprocess.Popen(
            arguments, cwd=cwd, env=dict(env), text=True, errors="replace", stderr=subprocess.PIPE
        )
        lines: collections.deque[str] = collections.deque()
        pump = _start_stderr_pump(process.stderr, lines)
        return_code = process.wait()
        # A daemon that inherited stderr keeps the pipe open; do not wait for EOF forever.
        pump.join(timeout=5)
        if return_code != 0:
            raise subprocess.CalledProcessError(return_code, arguments)
        return "".join(lines)

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
        report_path=training / "automation-report-full.json",
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
    build_log = runner.run_logged(
        cargo_build_command(config),
        cwd=paths.root,
        env=build_environment(
            config,
            target_dir=paths.use_target_dir,
            pgo_flags=flags,
        ),
    )
    log_pgo_warnings(build_log)
    executable = executable_path(paths.use_target_dir, config.target, config.binary_name)
    if not executable.is_file():
        raise PgoError(f"PGO RRiter executable not found: {executable}")
    return executable


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
    real_environment = dict(environment)
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
                PGO_DATABASE_ENV_NAME: DEFAULT_DATABASE_NAME,
                PGO_DATABASE_ENV_USER: DEFAULT_DATABASE_USER,
            }
        )
    if config.pdfium_path is not None:
        environment["RRITER_PDFIUM_PATH"] = str(config.pdfium_path)
    if "linux" in config.target:
        _provide_ty(environment, real_environment, verbose=config.verbose)
    return environment


def _provide_ty(
    environment: dict[str, str], real_environment: Mapping[str, str], *, verbose: bool
) -> None:
    """Let the lsp_nav group find `ty`: PATH is inherited, a managed install is added to it.

    `real_environment` is the environment before HOME/XDG were redirected into the state
    folder; the managed install lives under the real user data directory.
    """

    if shutil.which("ty", path=environment.get("PATH")) is not None:
        return
    bin_dir = managed_ty_bin_dir(real_environment)
    if bin_dir is not None:
        environment["PATH"] = os.pathsep.join([str(bin_dir), environment.get("PATH", "")])
    elif verbose:
        print(
            "[rriter-pgo] WARNING: `ty` is neither on PATH nor a managed RRiter install; "
            "the lsp_nav group will skip itself and its code stays unprofiled",
            flush=True,
        )


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
    if "windows" in target and os.name != "nt":
        raise PgoError("Windows PGO training must run on Windows")
    if "apple-darwin" in target and sys.platform != "darwin":
        raise PgoError("macOS PGO training must run on macOS")
    if "linux" in target and not sys.platform.startswith("linux"):
        # The Linux training is headless: no display server is needed.
        raise PgoError("Linux PGO training must run on Linux")


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


def active_scenarios(config: PgoConfig, host: str | None = None) -> tuple[str, ...]:
    """Linux runs the configured headless scenarios; other hosts run the GUI `full` one."""

    return config.scenarios if (sys.platform if host is None else host).startswith("linux") else ("full",)


def report_path_for(paths: PgoPaths, scenario: str) -> Path:
    return paths.training_dir / f"automation-report-{_slug(scenario)}.json"


def training_command(
    config: PgoConfig,
    paths: PgoPaths,
    executable: Path,
    scenario: str,
    report_path: Path,
    *,
    host: str | None = None,
) -> list[str | os.PathLike[str]]:
    common: list[str | os.PathLike[str]] = [
        "--pgo-workspace", paths.fixture_dir,
        "--pgo-report", report_path,
        "--pgo-timeout-seconds", str(config.timeout_seconds),
    ]
    if not (sys.platform if host is None else host).startswith("linux"):
        return [executable, "--ide", "--pgo-train", *common]
    # full and startup share one profile dir: startup restores the session full saved.
    profile = "headless-profile-welcome" if scenario == "welcome" else "headless-profile"
    return [
        executable, "--headless", "--pgo-train", "--pgo-scenario", scenario,
        *common, "--profile", paths.state_dir / profile,
    ]


@dataclass
class TrainingFixtures:
    database: LocalPostgresFixture
    api: LocalApiServer
    database_endpoint: tuple[str, int]

    def stop(self) -> None:
        try:
            self.database.stop()
        finally:
            self.api.stop()


def start_fixtures(paths: PgoPaths) -> TrainingFixtures:
    """Start the local PostgreSQL and API fixtures; the caller stops them in a `finally`."""

    database = LocalPostgresFixture()
    api = LocalApiServer.start()
    try:
        database.start()
        endpoint = database.endpoint
        print(
            f"[rriter-pgo] local PostgreSQL fixture: {endpoint[0]}:{endpoint[1]}/{DEFAULT_DATABASE_NAME}",
            flush=True,
        )
        _set_openapi_server_url(paths.fixture_dir / "openapi.json", api.base_url)
    except BaseException:
        try:
            database.stop()
        finally:
            api.stop()
        raise
    return TrainingFixtures(database, api, endpoint)


def _load_report(report_path: Path) -> tuple[dict[str, object] | None, Exception | None]:
    if not report_path.is_file():
        return None, None
    try:
        loaded = json.loads(report_path.read_text(encoding="utf-8"))
        if not isinstance(loaded, dict):
            raise PgoError("automation report root must be an object")
        return loaded, None
    except (OSError, json.JSONDecodeError, PgoError) as error:
        return None, error


def _stderr_tail(result: object) -> str:
    text = getattr(result, "stderr", "")
    return "".join(text.splitlines(keepends=True)[-STDERR_TAIL_LINES:]) if isinstance(text, str) else ""


def reject_skipped_pdf_group(
    report: Mapping[str, object], scenario: str, *, host: str | None = None
) -> None:
    """On Linux pdfium is a preflight requirement, so a skipped `pdf` group is a setup error."""

    if not (sys.platform if host is None else host).startswith("linux"):
        return
    for item in report.get("skipped_groups") or []:
        if isinstance(item, dict) and item.get("group") == "pdf":
            raise PgoError(
                f"scenario {scenario}: the pdf group was skipped ({item.get('reason')}); pdfium is "
                "a required Linux dependency of headless PGO (check RRITER_PDFIUM_PATH, `make pdfium`)"
            )


def run_scenario(
    config: PgoConfig,
    paths: PgoPaths,
    executable: Path,
    runner: Runner,
    fixtures: TrainingFixtures,
    name: str,
    report_path: Path,
    *,
    profile_dir: Path | None = None,
    check_raw: bool = False,
) -> dict[str, object]:
    print(f"[rriter-pgo] scenario {name}", flush=True)
    raw_dir = paths.profile_dir if profile_dir is None else profile_dir
    raw_before = set(raw_dir.glob("*.profraw"))
    result = runner.run_process_tree(
        training_command(config, paths, executable, name, report_path),
        cwd=paths.fixture_dir,
        env=isolated_runtime_environment(
            config, paths, profile_dir=profile_dir, database_endpoint=fixtures.database_endpoint
        ),
        timeout=config.timeout_seconds + 45,
        check=False,
    )
    report, report_error = _load_report(report_path)
    where = f"scenario {name}: "
    if result.returncode != 0:
        message = where + describe_pgo_process_failure(result.returncode)
        if report is not None:
            if report.get("status") == "success":
                message += f"; automation report status=success report={report_path}"
            else:
                message += "; " + automation_failure_message(report, report_path)
        elif report_error is not None:
            message += f"; structured automation report is invalid: {report_error}; {STEP_HINT}"
        else:
            message += f"; structured automation report is absent: {report_path}; {STEP_HINT}"
        tail = _stderr_tail(result)
        if tail:
            message += f"\nlast {STDERR_TAIL_LINES} stderr lines:\n{tail}"
        raise PgoError(message)
    if check_raw:
        check_new_raw_profiles(raw_dir, raw_before, name)
    if report_error is None and report is None:
        raise PgoError(
            f"{where}RRiter exited with code 0 before writing the structured automation report; "
            f"{STEP_HINT}: {report_path}"
        )
    if report_error is not None or report is None:
        raise PgoError(f"{where}invalid automation report: {report_error}") from report_error
    if report.get("status") != "success":
        raise PgoError(where + automation_failure_message(report, report_path))
    if int(report.get("scenario_version", -1)) != SCENARIO_VERSION:
        raise PgoError(f"{where}automation report scenario version does not match the pipeline")
    for skipped in report.get("skipped_groups") or []:
        if isinstance(skipped, dict):
            print(
                f"[rriter-pgo] WARNING: scenario {name} skipped group "
                f"{skipped.get('group')}: {skipped.get('reason')}",
                flush=True,
            )
    reject_skipped_pdf_group(report, name)
    if name == "full":
        telemetry = fixtures.database.telemetry()
        validate_database_fixture_telemetry(telemetry)
        request_count = fixtures.api.server.request_count
        last_request = dict(fixtures.api.server.last_request)
        if request_count < 1 or not last_request.get("accepted"):
            raise PgoError(
                "RRiter did not complete the authenticated local API request; "
                f"count={request_count} last_request={last_request}"
            )
        report["local_api_requests"] = request_count
        report["local_api_last_request"] = last_request
        report["database_fixture"] = database_fixture_telemetry_payload(telemetry)
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return report


def run_training(
    config: PgoConfig,
    paths: PgoPaths,
    executable: Path,
    runner: Runner,
    *,
    profile_dir: Path | None = None,
    check_raw: bool = False,
) -> dict[str, object]:
    """Run every active scenario in order against one set of fixtures.

    The result is the `full` report when it ran (else the last one), with the skipped groups
    of all scenarios merged in `skipped_groups` and the report files in `report_files`.
    `check_raw` requires every run to leave its own .profraw (instrumented binaries only).
    """

    names = active_scenarios(config)
    fixtures = start_fixtures(paths)
    reports: dict[str, dict[str, object]] = {}
    try:
        for name in names:
            reports[name] = run_scenario(
                config, paths, executable, runner, fixtures, name,
                report_path_for(paths, name), profile_dir=profile_dir, check_raw=check_raw,
            )
    finally:
        fixtures.stop()
    result = dict(reports.get("full", reports[names[-1]]))
    result["skipped_groups"] = [
        skipped for report in reports.values() for skipped in report.get("skipped_groups") or []
    ]
    result["report_files"] = {name: str(report_path_for(paths, name)) for name in names}
    return result


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
        *sorted(
            source
            for source in (paths.root / "src" / "app").glob("automation*.rs")
            if not source.stem.endswith("_tests")
        ),
        *sorted((paths.root / "src" / "headless").glob("*.rs")),
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


def llvm_profdata_command(
    *arguments: str | os.PathLike[str],
) -> list[str | os.PathLike[str]]:
    return ["rustup", "run", "nightly", "llvm-profdata", *arguments]


def install_binary(source: Path, destination: Path) -> None:
    """Copy `source` over `destination` atomically (temp file next to it, then os.replace)."""

    destination.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{destination.name}.", dir=destination.parent)
    os.close(descriptor)
    try:
        shutil.copyfile(source, temporary)
        shutil.copymode(source, temporary)
        os.replace(temporary, destination)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise


def check_profile_coverage(
    config: PgoConfig,
    paths: PgoPaths,
    runner: Runner,
    cxxfilt: str,
    report: Mapping[str, object],
) -> None:
    """Write coverage.txt and fail when a scenario group left its marker functions unexecuted."""

    shown = runner.run(
        llvm_profdata_command("show", "--all-functions", "--counts", paths.merged_profile),
        cwd=paths.root,
        env=base_environment(config),
        capture=True,
    )
    functions = demangled_functions(shown.stdout, cxxfilt)
    if not functions:
        raise PgoError("llvm-profdata show listed no functions; the profile cannot be checked")
    summary = module_summary(functions)
    coverage = paths.profile_dir / "coverage.txt"
    coverage.write_text(format_coverage(summary), encoding="utf-8")
    print(f"[rriter-pgo] coverage: {coverage}", flush=True)
    zero = zero_modules(summary)
    if zero:
        print(f"[rriter-pgo] rriter modules with no executed function: {', '.join(zero)}", flush=True)
    skipped = {
        str(item.get("group"))
        for item in report.get("skipped_groups") or []
        if isinstance(item, dict)
    }
    problems = check_markers(
        functions, GROUP_MARKERS, skipped | unrequired_groups(active_scenarios(config))
    )
    if problems:
        raise PgoError(
            "training did not reach code the scenario groups must cover:\n  " + "\n  ".join(problems)
        )


def log_pgo_warnings(build_log: str) -> None:
    warnings = count_pgo_warnings(build_log)
    print(
        f"[rriter-pgo] profile-use build: {warnings.missing} functions without profile data, "
        f"{warnings.mismatch} hash mismatches",
        flush=True,
    )
    if warnings.hot:
        print(f"[rriter-pgo] hot modules affected: {', '.join(warnings.hot)}", flush=True)


def merge_profiles(config: PgoConfig, paths: PgoPaths, runner: Runner) -> list[Path]:
    profiles = verify_raw_profiles(paths.profile_dir)
    paths.merged_profile.parent.mkdir(parents=True, exist_ok=True)
    runner.run(
        llvm_profdata_command(
            "merge",
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
            "automation_report_sha256": {
                name: sha256_file(Path(str(report_file)))
                for name, report_file in dict(report.get("report_files") or {}).items()
            },
            "automation_scenarios": sorted(dict(report.get("report_files") or {})),
            "automation_skipped_groups": report.get("skipped_groups", []),
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
    cxxfilt: str | None = None
    if (config.run_only or config.mode == "fresh") and "linux" in config.target:
        # Preflight before any build: the headless groups need pdfium, the coverage check
        # needs llvm-cxxfilt.
        validate_training_environment(config)
        config = replace(config, pdfium_path=find_pdfium(paths.root))
        if not config.run_only:
            cxxfilt = find_cxxfilt()
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
        print(f"[rriter-pgo] automation reports: {report['report_files']}", flush=True)
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
        report = run_training(config, paths, executable, runner, check_raw=True)
        profiles = merge_profiles(config, paths, runner)
        if cxxfilt is not None:
            check_profile_coverage(config, paths, runner, cxxfilt, report)
        write_manifest(config, paths, runner, profiles, report)
        print(f"[rriter-pgo] profile: {paths.merged_profile}", flush=True)
        if config.train_only:
            return None
    executable = build_with_profile(config, paths, runner)
    if config.install_binary is not None:
        install_binary(executable, config.install_binary)
        print(f"[rriter-pgo] installed: {config.install_binary}", flush=True)
    return executable


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
    parser.add_argument(
        "--scenarios",
        default=",".join(ALL_SCENARIOS),
        help="Linux headless training runs, in order (ignored on other hosts)",
    )
    parser.add_argument("--install-binary", type=Path, metavar="PATH")
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="rriter-pgo-selftest-") as directory:
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
        headless = training_command(config, paths, Path("rriter"), "welcome", paths.report_path, host="linux")
        if headless[1:3] != ["--headless", "--pgo-train"] or "--ide" in headless:
            raise PgoError("headless training command self-test failed")
        if training_command(config, paths, Path("rriter"), "full", paths.report_path, host="win32")[1] != "--ide":
            raise PgoError("GUI training command self-test failed")
        installed = root / "installed" / "rriter"
        install_binary(fast_executable, installed)
        if installed.read_text(encoding="utf-8") != fast_executable.read_text(encoding="utf-8"):
            raise PgoError("install_binary self-test failed")
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
        scenarios=tuple(name.strip() for name in args.scenarios.split(",") if name.strip()),
        install_binary=args.install_binary,
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
