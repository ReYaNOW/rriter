#!/usr/bin/env python3
"""Build, package, sign, and run RRiter on Windows 11.

No third-party Python modules are required. The script discovers a Visual Studio
Build Tools installation, imports its x64 MSVC environment, creates native PE
resources from the repository PNG, builds with the MSVC Rust target, and can
produce both a portable ZIP and an Inno Setup installer.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import textwrap
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Mapping, Sequence

from build_common import (
    PlanError,
    PgoMode,
    confirm,
    default_build_plan,
    interactive_build_plan,
    is_interactive,
    print_plan,
    self_test as build_plan_self_test,
    should_open_menu,
)
from pgo_pipeline import PgoConfig, PgoError, run_pipeline

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TARGET = "x86_64-pc-windows-msvc"
APP_NAME = "RRiter"
APP_ID = "{{8A36207E-3C0C-47A8-9C8A-683647F299CE}"
PUBLISHER = "RRiter"
ICON_PNG = ROOT / "src" / "icons" / "icon.png"
DIST_DIR = ROOT / "dist" / "windows"
BUILD_SUPPORT_DIR = ROOT / "target" / "rriter-windows-resources"


class BuildError(RuntimeError):
    pass


@dataclass(frozen=True)
class SigningOptions:
    pfx: Path | None
    password_env: str | None
    certificate_sha1: str | None
    timestamp_url: str | None


@dataclass(frozen=True)
class BuildArtifacts:
    executable: Path
    portable_dir: Path
    portable_zip: Path | None
    installer: Path | None


@dataclass(frozen=True)
class MsvcActivation:
    label: str
    script: Path
    arguments: tuple[str, ...]


MSVC_EXIT_MARKER = "__RRITER_MSVC_SETUP_EXIT__="
MSVC_ENV_MARKER = "__RRITER_MSVC_ENV_BEGIN__"
MSVC_REQUIRED_TOOLS = ("cl.exe", "link.exe", "rc.exe")
MSVC_CAPTURE_ENV_KEYS = {
    "RRITER_MSVC_ENV",
    "RRITER_MSVC_LOG",
    "RRITER_MSVC_SETUP",
}


def log(message: str) -> None:
    print(f"[rriter-windows] {message}", flush=True)


def run(
    command: Sequence[str | os.PathLike[str]],
    *,
    env: Mapping[str, str] | None = None,
    cwd: Path = ROOT,
    capture: bool = False,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    printable = subprocess.list2cmdline([os.fspath(part) for part in command])
    log(f"$ {printable}")
    return subprocess.run(
        [os.fspath(part) for part in command],
        cwd=cwd,
        env=dict(env) if env is not None else None,
        check=check,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )


def cargo_version() -> str:
    cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    package = cargo_toml.split("[dependencies]", 1)[0]
    match = re.search(r'^version\s*=\s*"([^"]+)"', package, re.MULTILINE)
    if not match:
        raise BuildError("Cargo.toml package version not found")
    return match.group(1)


def version_quad(version: str) -> tuple[int, int, int, int]:
    pieces: list[int] = []
    for part in version.split("."):
        match = re.match(r"(\d+)", part)
        pieces.append(int(match.group(1)) if match else 0)
    return tuple((pieces + [0, 0, 0, 0])[:4])  # type: ignore[return-value]


def png_dimensions(data: bytes) -> tuple[int, int]:
    signature = b"\x89PNG\r\n\x1a\n"
    if len(data) < 24 or not data.startswith(signature) or data[12:16] != b"IHDR":
        raise BuildError("src/icons/icon.png is not a valid PNG")
    width, height = struct.unpack(">II", data[16:24])
    if width == 0 or height == 0:
        raise BuildError("PNG icon has invalid dimensions")
    return width, height


def write_png_backed_ico(png_path: Path, ico_path: Path) -> None:
    data = png_path.read_bytes()
    width, height = png_dimensions(data)
    width_byte = width if width < 256 else 0
    height_byte = height if height < 256 else 0
    header = struct.pack("<HHH", 0, 1, 1)
    entry = struct.pack(
        "<BBBBHHII",
        width_byte,
        height_byte,
        0,
        0,
        1,
        32,
        len(data),
        6 + 16,
    )
    ico_path.parent.mkdir(parents=True, exist_ok=True)
    ico_path.write_bytes(header + entry + data)


def quote_rc_path(path: Path) -> str:
    return str(path.resolve()).replace("\\", "\\\\").replace('"', '\\"')


def windows_resource_script(icon: Path, version: str) -> str:
    major, minor, patch, build = version_quad(version)
    return textwrap.dedent(
        f'''\
        #include <windows.h>

        101 ICON "{quote_rc_path(icon)}"

        1 VERSIONINFO
        FILEVERSION {major},{minor},{patch},{build}
        PRODUCTVERSION {major},{minor},{patch},{build}
        FILEFLAGSMASK 0x3fL
        #ifdef _DEBUG
        FILEFLAGS VS_FF_DEBUG
        #else
        FILEFLAGS 0x0L
        #endif
        FILEOS VOS_NT_WINDOWS32
        FILETYPE VFT_APP
        FILESUBTYPE 0x0L
        BEGIN
            BLOCK "StringFileInfo"
            BEGIN
                BLOCK "040904B0"
                BEGIN
                    VALUE "CompanyName", "{PUBLISHER}\\0"
                    VALUE "FileDescription", "RRiter code editor\\0"
                    VALUE "FileVersion", "{version}\\0"
                    VALUE "InternalName", "rriter\\0"
                    VALUE "OriginalFilename", "rriter.exe\\0"
                    VALUE "ProductName", "RRiter\\0"
                    VALUE "ProductVersion", "{version}\\0"
                END
            END
            BLOCK "VarFileInfo"
            BEGIN
                VALUE "Translation", 0x0409, 1200
            END
        END
        '''
    )


def inno_setup_script(
    source_exe: Path,
    icon: Path,
    output_dir: Path,
    version: str,
) -> str:
    source = str(source_exe.resolve()).replace('"', '""')
    icon_path = str(icon.resolve()).replace('"', '""')
    output = str(output_dir.resolve()).replace('"', '""')
    version_info = ".".join(str(part) for part in version_quad(version))
    return textwrap.dedent(
        f'''\
        #define MyAppName "{APP_NAME}"
        #define MyAppVersion "{version}"
        #define MyAppPublisher "{PUBLISHER}"
        #define MyAppExeName "rriter.exe"

        [Setup]
        AppId={APP_ID}
        AppName={{#MyAppName}}
        AppVersion={{#MyAppVersion}}
        AppPublisher={{#MyAppPublisher}}
        DefaultDirName={{localappdata}}\\Programs\\RRiter
        DefaultGroupName=RRiter
        DisableProgramGroupPage=yes
        OutputDir={output}
        OutputBaseFilename=RRiter-{version}-x86_64-setup
        SetupIconFile={icon_path}
        UninstallDisplayIcon={{app}}\\{{#MyAppExeName}}
        Compression=lzma2/ultra64
        SolidCompression=yes
        WizardStyle=modern
        PrivilegesRequired=lowest
        ArchitecturesAllowed=x64compatible
        ArchitecturesInstallIn64BitMode=x64compatible
        CloseApplications=yes
        RestartApplications=no
        ChangesAssociations=no
        VersionInfoVersion={version_info}
        VersionInfoCompany={PUBLISHER}
        VersionInfoDescription=RRiter installer
        VersionInfoProductName=RRiter
        VersionInfoProductVersion={version_info}

        [Languages]
        Name: "english"; MessagesFile: "compiler:Default.isl"
        Name: "russian"; MessagesFile: "compiler:Languages\\Russian.isl"

        [Tasks]
        Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

        [Files]
        Source: "{source}"; DestDir: "{{app}}"; Flags: ignoreversion

        [Icons]
        Name: "{{autoprograms}}\\RRiter"; Filename: "{{app}}\\{{#MyAppExeName}}"
        Name: "{{autodesktop}}\\RRiter"; Filename: "{{app}}\\{{#MyAppExeName}}"; Tasks: desktopicon

        [Registry]
        Root: HKCU; Subkey: "Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\rriter.exe"; ValueType: string; ValueName: ""; ValueData: "{{app}}\\{{#MyAppExeName}}"; Flags: uninsdeletekey
        Root: HKCU; Subkey: "Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\rriter.exe"; ValueType: string; ValueName: "Path"; ValueData: "{{app}}"; Flags: uninsdeletekey

        [Run]
        Filename: "{{app}}\\{{#MyAppExeName}}"; Description: "Launch RRiter"; Flags: nowait postinstall skipifsilent
        '''
    )


def find_vswhere() -> Path | None:
    direct = shutil.which("vswhere.exe")
    if direct:
        return Path(direct)
    roots = [
        os.environ.get("ProgramFiles(x86)"),
        os.environ.get("ProgramFiles"),
    ]
    for root in filter(None, roots):
        candidate = Path(root) / "Microsoft Visual Studio" / "Installer" / "vswhere.exe"
        if candidate.is_file():
            return candidate
    return None


def msvc_activation_candidates(installation: Path) -> list[MsvcActivation]:
    candidates = [
        MsvcActivation(
            "vcvars64.bat",
            installation / "VC" / "Auxiliary" / "Build" / "vcvars64.bat",
            (),
        ),
        MsvcActivation(
            "vcvarsall.bat x64",
            installation / "VC" / "Auxiliary" / "Build" / "vcvarsall.bat",
            ("x64",),
        ),
        MsvcActivation(
            "VsDevCmd.bat",
            installation / "Common7" / "Tools" / "VsDevCmd.bat",
            ("-no_logo", "-arch=x64", "-host_arch=x64"),
        ),
    ]
    return [candidate for candidate in candidates if candidate.script.is_file()]


def msvc_capture_script(arguments: Sequence[str]) -> str:
    argument_text = " ".join(arguments)
    suffix = f" {argument_text}" if argument_text else ""
    return textwrap.dedent(
        f'''\
        @echo off
        setlocal DisableDelayedExpansion
        call "%RRITER_MSVC_SETUP%"{suffix} > "%RRITER_MSVC_LOG%" 2>&1
        set "RRITER_MSVC_SETUP_EXIT=%ERRORLEVEL%"
        chcp 65001 >nul
        (
            echo {MSVC_EXIT_MARKER}%RRITER_MSVC_SETUP_EXIT%
            echo {MSVC_ENV_MARKER}
            set
        ) > "%RRITER_MSVC_ENV%"
        exit /b 0
        '''
    )


def command_processor(environment: Mapping[str, str]) -> Path:
    configured = environment.get("COMSPEC") or environment.get("ComSpec")
    if configured:
        candidate = Path(configured.strip().strip('"'))
        if candidate.is_file():
            return candidate
    system_root = environment.get("SYSTEMROOT") or environment.get("SystemRoot")
    if system_root:
        candidate = Path(system_root) / "System32" / "cmd.exe"
        if candidate.is_file():
            return candidate
    resolved = shutil.which(
        "cmd.exe", path=environment.get("PATH") or environment.get("Path")
    )
    if resolved:
        return Path(resolved)
    raise BuildError("cmd.exe not found through COMSPEC, SystemRoot, or PATH")


def msvc_capture_invocation(
    environment: Mapping[str, str],
    capture_script: Path,
) -> tuple[list[str | os.PathLike[str]], Path]:
    return (
        [command_processor(environment), "/d", "/q", "/c", capture_script.name],
        capture_script.parent,
    )


def parse_msvc_environment_dump(output: str) -> tuple[int | None, dict[str, str]]:
    setup_exit: int | None = None
    imported: dict[str, str] = {}
    reading_environment = False
    for raw_line in output.splitlines():
        line = raw_line.rstrip("\r")
        if line.startswith(MSVC_EXIT_MARKER):
            value = line[len(MSVC_EXIT_MARKER) :].strip()
            try:
                setup_exit = int(value)
            except ValueError:
                setup_exit = None
            continue
        if line == MSVC_ENV_MARKER:
            reading_environment = True
            continue
        if not reading_environment or "=" not in line:
            continue
        key, value = line.split("=", 1)
        normalized_key = key.upper()
        if (
            normalized_key
            and normalized_key not in MSVC_CAPTURE_ENV_KEYS
            and normalized_key != "RRITER_MSVC_SETUP_EXIT"
        ):
            imported[normalized_key] = value
    return setup_exit, imported


def diagnostic_tail(text: str, limit: int = 24) -> str:
    lines = [line.rstrip() for line in text.splitlines() if line.strip()]
    if len(lines) > limit:
        lines = [f"... {len(lines) - limit} earlier line(s) omitted ...", *lines[-limit:]]
    return "\n".join(lines)


def normalize_windows_environment(environment: Mapping[str, str]) -> dict[str, str]:
    return {key.upper(): value for key, value in environment.items()}


def activate_msvc_candidate(
    base: Mapping[str, str],
    candidate: MsvcActivation,
) -> tuple[dict[str, str], int | None, str, list[str]]:
    base_environment = normalize_windows_environment(base)
    with tempfile.TemporaryDirectory(prefix="rriter-msvc-env-") as directory:
        work = Path(directory)
        capture_script = work / "capture-msvc-env.cmd"
        environment_dump = work / "msvc-environment.txt"
        diagnostic_log = work / "msvc-setup.log"
        capture_script.write_text(
            msvc_capture_script(candidate.arguments),
            encoding="ascii",
            newline="\r\n",
        )
        command_environment = dict(base_environment)
        command_environment.update(
            {
                "RRITER_MSVC_ENV": str(environment_dump),
                "RRITER_MSVC_LOG": str(diagnostic_log),
                "RRITER_MSVC_SETUP": str(candidate.script),
            }
        )
        command, command_cwd = msvc_capture_invocation(
            command_environment, capture_script
        )
        result = run(
            command,
            cwd=command_cwd,
            env=command_environment,
            capture=True,
            check=False,
        )
        diagnostics = ""
        if diagnostic_log.is_file():
            diagnostics = diagnostic_log.read_text(encoding="utf-8", errors="replace")
        dump = ""
        if environment_dump.is_file():
            dump = environment_dump.read_text(encoding="utf-8", errors="replace")
        setup_exit, imported = parse_msvc_environment_dump(dump)
        activated = dict(base_environment)
        activated.update(imported)
        missing = [
            executable
            for executable in MSVC_REQUIRED_TOOLS
            if not shutil.which(executable, path=activated.get("PATH"))
        ]
        if result.returncode != 0 or not environment_dump.is_file():
            capture_error = result.stderr or result.stdout
            diagnostics = (
                f"cmd.exe environment capture exited with {result.returncode}.\n"
                f"{capture_error}\n{diagnostics}"
            )
        return activated, setup_exit, diagnostic_tail(diagnostics), missing


def import_msvc_environment(base: Mapping[str, str]) -> dict[str, str]:
    environment = normalize_windows_environment(base)
    if all(
        shutil.which(executable, path=environment.get("PATH"))
        for executable in MSVC_REQUIRED_TOOLS
    ):
        return environment
    vswhere = find_vswhere()
    if vswhere is None:
        raise BuildError(
            "Visual Studio Build Tools not found. Install workload "
            "Microsoft.VisualStudio.Workload.VCTools."
        )
    result = run(
        [
            vswhere,
            "-latest",
            "-prerelease",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ],
        env=environment,
        capture=True,
        check=False,
    )
    installation_text = result.stdout.strip()
    if result.returncode != 0 or not installation_text:
        details = diagnostic_tail(result.stderr or result.stdout)
        suffix = f"\nvswhere output:\n{details}" if details else ""
        raise BuildError(
            "Visual Studio C++ Build Tools installation was not found by vswhere." + suffix
        )
    installation = Path(installation_text.splitlines()[0].strip())
    candidates = msvc_activation_candidates(installation)
    if not candidates:
        raise BuildError(
            "No vcvars64.bat, vcvarsall.bat, or VsDevCmd.bat was found under "
            f"{installation}"
        )

    failures: list[str] = []
    for candidate in candidates:
        activated, setup_exit, diagnostics, missing = activate_msvc_candidate(
            environment, candidate
        )
        if not missing:
            if setup_exit not in (None, 0):
                log(
                    f"{candidate.label} returned {setup_exit}, but cl.exe, link.exe, "
                    "and rc.exe were activated; continuing"
                )
            else:
                log(f"MSVC environment activated through {candidate.label}")
            return activated
        failure = (
            f"{candidate.label}: setup exit={setup_exit!r}; "
            f"missing {', '.join(missing)}"
        )
        if diagnostics:
            failure += f"\n{diagnostics}"
        failures.append(failure)

    raise BuildError(
        "Visual Studio was found, but the x64 MSVC environment could not be activated. "
        "In Visual Studio Installer choose Modify, enable Desktop development with C++, "
        "and ensure the latest MSVC x64/x86 tools and Windows 11 SDK are installed.\n\n"
        + "\n\n".join(failures)
    )


def find_signtool(environment: Mapping[str, str]) -> Path | None:
    direct = shutil.which("signtool.exe", path=environment.get("PATH"))
    if direct:
        return Path(direct)
    kits = Path(os.environ.get("ProgramFiles(x86)", "")) / "Windows Kits" / "10" / "bin"
    candidates = sorted(kits.glob("*/*/signtool.exe"), reverse=True)
    return candidates[0] if candidates else None


def find_iscc() -> Path | None:
    direct = shutil.which("ISCC.exe")
    if direct:
        return Path(direct)
    for root_name in ("ProgramFiles(x86)", "ProgramFiles"):
        root = os.environ.get(root_name)
        if not root:
            continue
        candidate = Path(root) / "Inno Setup 6" / "ISCC.exe"
        if candidate.is_file():
            return candidate
    return None


def rust_target_installed(environment: Mapping[str, str], target: str) -> bool:
    result = run(
        ["rustup", "target", "list", "--installed"],
        env=environment,
        capture=True,
    )
    return target in result.stdout.split()


def ensure_rust_target(environment: Mapping[str, str], target: str, install: bool) -> None:
    if rust_target_installed(environment, target):
        return
    if not install:
        raise BuildError(
            f"Rust target {target} is missing. Run: rustup target add {target} "
            "--toolchain nightly"
        )
    run(["rustup", "target", "add", target, "--toolchain", "nightly"], env=environment)


def windows_test_command(target: str) -> list[str]:
    # RRiter highlighter/process tests intentionally share bounded worker resources.
    # Match Makefile test policy: serial execution avoids CI-only scheduler races.
    return [
        "cargo",
        "+nightly",
        "test",
        "--locked",
        "--target",
        target,
        "--",
        "--test-threads=1",
    ]


def prepare_resources(environment: Mapping[str, str], version: str) -> Path:
    BUILD_SUPPORT_DIR.mkdir(parents=True, exist_ok=True)
    icon = BUILD_SUPPORT_DIR / "rriter.ico"
    resource_script = BUILD_SUPPORT_DIR / "rriter.rc"
    resource = BUILD_SUPPORT_DIR / "rriter.res"
    write_png_backed_ico(ICON_PNG, icon)
    resource_script.write_text(windows_resource_script(icon, version), encoding="utf-8")
    rc = shutil.which("rc.exe", path=environment.get("PATH"))
    if not rc:
        raise BuildError("rc.exe not found after importing MSVC environment")
    run([rc, "/nologo", f"/fo{resource}", resource_script], env=environment)
    if not resource.is_file():
        raise BuildError("rc.exe did not create rriter.res")
    return resource


def windows_target_environment(
    environment: Mapping[str, str],
    *,
    target: str,
    install_target: bool,
) -> dict[str, str]:
    ensure_rust_target(environment, target, install_target)
    return dict(environment)


def windows_build_environment(
    environment: Mapping[str, str],
    *,
    target: str,
    install_target: bool,
) -> dict[str, str]:
    build_env = windows_target_environment(
        environment,
        target=target,
        install_target=install_target,
    )
    return with_windows_resources(build_env)


def with_windows_resources(environment: Mapping[str, str]) -> dict[str, str]:
    resource = prepare_resources(environment, cargo_version())
    build_env = dict(environment)
    build_env["RRITER_WINDOWS_RESOURCE"] = str(resource.resolve())
    return build_env


def run_windows_tests(
    environment: Mapping[str, str],
    *,
    target: str,
    install_target: bool,
) -> None:
    test_env = windows_target_environment(
        environment, target=target, install_target=install_target
    )
    run(windows_test_command(target), env=test_env)


def build_rriter(
    environment: Mapping[str, str],
    *,
    target: str,
    release: bool,
    run_tests: bool,
    install_target: bool,
    pgo_mode: PgoMode = PgoMode.OFF,
    pgo_profile: Path | None = None,
    pgo_timeout_seconds: int = 300,
) -> Path:
    target_env = windows_target_environment(
        environment, target=target, install_target=install_target
    )
    if run_tests:
        run(windows_test_command(target), env=target_env)
    build_env = with_windows_resources(target_env)
    if pgo_mode is not PgoMode.OFF:
        if not release:
            raise BuildError("PGO is supported only for release builds")
        executable = run_pipeline(
            PgoConfig(
                root=ROOT,
                target=target,
                mode=pgo_mode.value,
                profile_path=pgo_profile,
                timeout_seconds=pgo_timeout_seconds,
                rustflags=build_env.get("RUSTFLAGS", ""),
                cargo_env=build_env,
            )
        )
        if executable is None:
            raise BuildError("PGO pipeline did not produce an executable")
        return executable

    command = ["cargo", "+nightly", "build", "--locked", "--target", target]
    profile = "debug"
    if release:
        command.append("--release")
        profile = "release"
    run(command, env=build_env)
    executable = ROOT / "target" / target / profile / "rriter.exe"
    if not executable.is_file():
        raise BuildError(f"built executable not found: {executable}")
    return executable


def sign_file(
    path: Path,
    environment: Mapping[str, str],
    options: SigningOptions,
) -> None:
    if options.pfx is None and not options.certificate_sha1:
        return
    signtool = find_signtool(environment)
    if signtool is None:
        raise BuildError("signtool.exe not found in Windows SDK")
    command: list[str | os.PathLike[str]] = [signtool, "sign", "/fd", "SHA256"]
    if options.pfx is not None:
        command.extend(["/f", options.pfx])
        if options.password_env:
            password = environment.get(options.password_env)
            if password is None:
                raise BuildError(f"signing password environment {options.password_env} is unset")
            command.extend(["/p", password])
    else:
        command.extend(["/sha1", options.certificate_sha1 or ""])
    if options.timestamp_url:
        command.extend(["/tr", options.timestamp_url, "/td", "SHA256"])
    command.append(path)
    run(command, env=environment)
    run([signtool, "verify", "/pa", "/v", path], env=environment)


def stage_portable(executable: Path, version: str) -> Path:
    portable = DIST_DIR / f"RRiter-{version}-x86_64"
    if portable.exists():
        shutil.rmtree(portable)
    portable.mkdir(parents=True)
    shutil.copy2(executable, portable / "rriter.exe")
    readme = portable / "README.txt"
    readme.write_text(
        "RRiter portable build\n\nRun rriter.exe. User configuration stays in "
        "%APPDATA%\\RRiter and %LOCALAPPDATA%\\RRiter.\n",
        encoding="utf-8",
    )
    return portable


def zip_portable(portable: Path, version: str) -> Path:
    archive = DIST_DIR / f"RRiter-{version}-x86_64-portable.zip"
    archive.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as output:
        for path in sorted(portable.rglob("*")):
            if path.is_file():
                output.write(path, Path(portable.name) / path.relative_to(portable))
    return archive


def build_installer(
    executable: Path,
    version: str,
    environment: Mapping[str, str],
) -> Path:
    iscc = find_iscc()
    if iscc is None:
        raise BuildError("Inno Setup 6 not found. Install package JRSoftware.InnoSetup.")
    icon = BUILD_SUPPORT_DIR / "rriter.ico"
    script = BUILD_SUPPORT_DIR / "rriter.iss"
    script.write_text(inno_setup_script(executable, icon, DIST_DIR, version), encoding="utf-8")
    run([iscc, script], env=environment)
    installer = DIST_DIR / f"RRiter-{version}-x86_64-setup.exe"
    if not installer.is_file():
        raise BuildError(f"Inno Setup output not found: {installer}")
    return installer


def package(
    executable: Path,
    environment: Mapping[str, str],
    *,
    create_installer: bool,
    signing: SigningOptions,
) -> BuildArtifacts:
    version = cargo_version()
    DIST_DIR.mkdir(parents=True, exist_ok=True)
    sign_file(executable, environment, signing)
    portable = stage_portable(executable, version)
    archive = zip_portable(portable, version)
    installer = None
    if create_installer:
        installer = build_installer(executable, version, environment)
        sign_file(installer, environment, signing)
    return BuildArtifacts(executable, portable, archive, installer)


def windows_msvc_capture_smoke_test() -> None:
    if os.name != "nt":
        return
    with tempfile.TemporaryDirectory(prefix="rriter-msvc-smoke-") as directory:
        root = Path(directory) / "Путь with spaces & symbols"
        tools = root / "tools"
        tools.mkdir(parents=True)
        for executable in MSVC_REQUIRED_TOOLS:
            (tools / executable).write_bytes(b"")
        setup = root / "fake-vcvars64.bat"
        setup.write_text(
            "@echo off\r\n"
            'set "PATH=%~dp0tools;%PATH%"\r\n'
            'set "RRITER_MSVC_SMOKE=ok"\r\n'
            "exit /b 1\r\n",
            encoding="utf-8",
        )
        activated, setup_exit, diagnostics, missing = activate_msvc_candidate(
            os.environ,
            MsvcActivation("smoke vcvars64.bat", setup, ()),
        )
        if setup_exit != 1 or missing or activated.get("RRITER_MSVC_SMOKE") != "ok":
            raise BuildError(
                "MSVC Windows capture smoke test failed: "
                f"setup_exit={setup_exit!r}, missing={missing}, diagnostics={diagnostics!r}"
            )


def self_test() -> None:
    version = cargo_version()
    with tempfile.TemporaryDirectory(prefix="rriter-windows-selftest-") as directory:
        root = Path(directory)
        ico = root / "rriter.ico"
        write_png_backed_ico(ICON_PNG, ico)
        data = ico.read_bytes()
        if data[:6] != struct.pack("<HHH", 0, 1, 1):
            raise BuildError("ICO header self-test failed")
        rc = windows_resource_script(ico, version)
        if "VERSIONINFO" not in rc or "101 ICON" not in rc:
            raise BuildError("resource script self-test failed")
        iss = inno_setup_script(root / "rriter.exe", ico, root, version)
        required = [
            "PrivilegesRequired=lowest",
            "App Paths",
            "UninstallDisplayIcon",
            "AppId={{8A36207E-3C0C-47A8-9C8A-683647F299CE}",
        ]
        if not all(token in iss for token in required):
            raise BuildError("Inno Setup template self-test failed")

        installation = root / "Visual Studio" / "18" / "BuildTools"
        vc_build = installation / "VC" / "Auxiliary" / "Build"
        common_tools = installation / "Common7" / "Tools"
        vc_build.mkdir(parents=True)
        common_tools.mkdir(parents=True)
        for script in (
            vc_build / "vcvars64.bat",
            vc_build / "vcvarsall.bat",
            common_tools / "VsDevCmd.bat",
        ):
            script.write_text("@echo off\n", encoding="ascii")
        candidates = msvc_activation_candidates(installation)
        if [candidate.label for candidate in candidates] != [
            "vcvars64.bat",
            "vcvarsall.bat x64",
            "VsDevCmd.bat",
        ]:
            raise BuildError("MSVC activation candidate order self-test failed")

        capture = msvc_capture_script(("-no_logo", "-arch=x64"))
        capture_tokens = (
            'setlocal DisableDelayedExpansion',
            'chcp 65001 >nul',
            '"%RRITER_MSVC_ENV%"',
            "exit /b 0",
        )
        if "&&" in capture or not all(token in capture for token in capture_tokens):
            raise BuildError("MSVC resilient capture script self-test failed")

        command_root = root / "Путь with spaces & symbols"
        command_root.mkdir()
        fake_cmd = command_root / "cmd.exe"
        fake_cmd.write_bytes(b"")
        capture_path = command_root / "capture-msvc-env.cmd"
        command, command_cwd = msvc_capture_invocation(
            {"COMSPEC": str(fake_cmd)}, capture_path
        )
        if command != [fake_cmd, "/d", "/q", "/c", capture_path.name]:
            raise BuildError("MSVC capture command self-test failed")
        if command_cwd != command_root or any(
            "RRITER_MSVC_CAPTURE" in str(part) for part in command
        ):
            raise BuildError("MSVC capture working-directory self-test failed")

        setup_exit, imported = parse_msvc_environment_dump(
            "diagnostic before environment\n"
            f"{MSVC_EXIT_MARKER}1\n"
            f"{MSVC_ENV_MARKER}\n"
            "Path=C:\\Tools;C:\\Windows\n"
            "INCLUDE=C:\\Include\n"
        )
        if setup_exit != 1 or imported.get("PATH") != r"C:\Tools;C:\Windows":
            raise BuildError("MSVC environment parser self-test failed")
        if "earlier line(s) omitted" not in diagnostic_tail(
            "\n".join(map(str, range(30))), 3
        ):
            raise BuildError("MSVC diagnostic truncation self-test failed")
        test_command = windows_test_command(DEFAULT_TARGET)
        if test_command != [
            "cargo",
            "+nightly",
            "test",
            "--locked",
            "--target",
            DEFAULT_TARGET,
            "--",
            "--test-threads=1",
        ]:
            raise BuildError("Windows serial test command self-test failed")
    build_plan_self_test()
    windows_msvc_capture_smoke_test()
    log("self-test passed")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default=DEFAULT_TARGET)
    parser.add_argument("--debug", action="store_true", help="Build debug profile")
    parser.add_argument("--test", action="store_true", help="Run Windows tests before build")
    parser.add_argument("--tests-only", action="store_true", help="Run tests and stop")
    parser.add_argument("--build-only", action="store_true", help="Build without tests")
    parser.add_argument(
        "--install-target",
        action="store_true",
        help="Install missing nightly Rust target through rustup",
    )
    parser.add_argument("--run", action="store_true", help="Launch built executable")
    parser.add_argument("--no-package", action="store_true", help="Skip portable ZIP")
    parser.add_argument("--installer", action="store_true", help="Build Inno Setup installer")
    parser.add_argument("--pgo", choices=[mode.value for mode in PgoMode], default="off")
    parser.add_argument("--pgo-profile", type=Path)
    parser.add_argument("--pgo-timeout-seconds", type=int, default=300)
    parser.add_argument("--menu", action="store_true", help="Force the interactive menu")
    parser.add_argument("--yes", action="store_true", help="Do not ask for final confirmation")
    parser.add_argument("--print-plan", action="store_true", help="Print the plan and exit")
    parser.add_argument("--sign-pfx", type=Path)
    parser.add_argument("--sign-password-env")
    parser.add_argument("--sign-cert-sha1")
    parser.add_argument(
        "--timestamp-url",
        default=os.environ.get("RRITER_TIMESTAMP_URL", "http://timestamp.digicert.com"),
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def requested_plan(args: argparse.Namespace, raw_argv: Sequence[str]):
    if args.menu and not is_interactive():
        raise BuildError("--menu requires an interactive terminal")
    menu = should_open_menu(raw_argv, force=args.menu)
    if menu:
        plan = interactive_build_plan("Windows", supports_installer=True)
        if not args.yes:
            print_plan(plan, platform_lines=(f"Target:       {args.target}",))
            if not confirm("Start this plan?", default=True):
                raise BuildError("build cancelled")
        return plan
    if args.tests_only and args.build_only:
        raise BuildError("--tests-only and --build-only cannot be combined")
    if args.build_only and args.test:
        raise BuildError("--build-only and --test cannot be combined")
    return default_build_plan(
        run_tests=args.test and not args.build_only,
        tests_only=args.tests_only,
        debug=args.debug,
        package=not args.no_package,
        installer=args.installer,
        run_after_build=args.run,
        pgo=args.pgo,
        pgo_profile=str(args.pgo_profile) if args.pgo_profile else None,
    )


def main(argv: Sequence[str] | None = None) -> int:
    raw_argv = list(sys.argv[1:] if argv is None else argv)
    if not raw_argv and not is_interactive():
        print(
            "[rriter-windows] No interactive terminal detected. "
            "Run with --help or pass explicit build flags.",
            file=sys.stderr,
        )
        return 2
    args = parse_args(raw_argv)
    if args.self_test:
        self_test()
        return 0
    plan = requested_plan(args, raw_argv)
    print_plan(plan, platform_lines=(f"Target:       {args.target}",))
    if args.print_plan:
        return 0
    if os.name != "nt":
        raise BuildError("Windows build must run on Windows 11; use --self-test elsewhere")
    environment = import_msvc_environment(os.environ)
    executable: Path | None = None
    if plan.build:
        executable = build_rriter(
            environment,
            target=args.target,
            release=plan.release,
            run_tests=plan.run_tests,
            install_target=args.install_target,
            pgo_mode=plan.pgo,
            pgo_profile=args.pgo_profile,
            pgo_timeout_seconds=args.pgo_timeout_seconds,
        )
    elif plan.run_tests:
        run_windows_tests(
            environment, target=args.target, install_target=args.install_target
        )

    if executable is None:
        log("tests completed; no build requested")
        return 0

    signing = SigningOptions(
        pfx=args.sign_pfx,
        password_env=args.sign_password_env,
        certificate_sha1=args.sign_cert_sha1,
        timestamp_url=args.timestamp_url,
    )
    artifacts = None
    if plan.package:
        artifacts = package(
            executable,
            environment,
            create_installer=plan.installer,
            signing=signing,
        )
    elif signing.pfx is not None or signing.certificate_sha1:
        sign_file(executable, environment, signing)
    log(f"executable: {executable}")
    if artifacts is not None:
        log(f"portable ZIP: {artifacts.portable_zip}")
        if artifacts.installer is not None:
            log(f"installer: {artifacts.installer}")
    if plan.run_after_build:
        log("launching RRiter")
        subprocess.Popen([str(executable)], cwd=ROOT, env=environment)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (BuildError, PlanError, PgoError, subprocess.CalledProcessError, OSError) as error:
        print(f"[rriter-windows] ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
