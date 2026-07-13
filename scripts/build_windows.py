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


def import_msvc_environment(base: Mapping[str, str]) -> dict[str, str]:
    environment = dict(base)
    if shutil.which("link.exe", path=environment.get("PATH")) and shutil.which(
        "rc.exe", path=environment.get("PATH")
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
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ],
        env=environment,
        capture=True,
    )
    installation = Path(result.stdout.strip())
    vcvars = installation / "Common7" / "Tools" / "VsDevCmd.bat"
    if not vcvars.is_file():
        raise BuildError(f"VsDevCmd.bat not found under {installation}")
    command = f'call "{vcvars}" -no_logo -arch=x64 -host_arch=x64 >nul && set'
    result = run(["cmd.exe", "/d", "/s", "/c", command], env=environment, capture=True)
    for line in result.stdout.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        if key:
            environment[key] = value
    for executable in ("link.exe", "rc.exe"):
        if not shutil.which(executable, path=environment.get("PATH")):
            raise BuildError(f"MSVC tool {executable} was not activated")
    return environment


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


def build_rriter(
    environment: Mapping[str, str],
    *,
    target: str,
    release: bool,
    run_tests: bool,
    install_target: bool,
) -> Path:
    ensure_rust_target(environment, target, install_target)
    version = cargo_version()
    resource = prepare_resources(environment, version)
    build_env = dict(environment)
    build_env["RRITER_WINDOWS_RESOURCE"] = str(resource.resolve())
    if run_tests:
        run(
            ["cargo", "+nightly", "test", "--locked", "--target", target],
            env=build_env,
        )
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
    log("self-test passed")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default=DEFAULT_TARGET)
    parser.add_argument("--debug", action="store_true", help="Build debug profile")
    parser.add_argument("--test", action="store_true", help="Run Windows tests before build")
    parser.add_argument(
        "--install-target",
        action="store_true",
        help="Install missing nightly Rust target through rustup",
    )
    parser.add_argument("--run", action="store_true", help="Launch built executable")
    parser.add_argument("--no-package", action="store_true", help="Skip portable ZIP")
    parser.add_argument("--installer", action="store_true", help="Build Inno Setup installer")
    parser.add_argument("--sign-pfx", type=Path)
    parser.add_argument("--sign-password-env")
    parser.add_argument("--sign-cert-sha1")
    parser.add_argument(
        "--timestamp-url",
        default=os.environ.get("RRITER_TIMESTAMP_URL", "http://timestamp.digicert.com"),
    )
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.self_test:
        self_test()
        return 0
    if os.name != "nt":
        raise BuildError("Windows build must run on Windows 11; use --self-test elsewhere")
    environment = import_msvc_environment(os.environ)
    executable = build_rriter(
        environment,
        target=args.target,
        release=not args.debug,
        run_tests=args.test,
        install_target=args.install_target,
    )
    signing = SigningOptions(
        pfx=args.sign_pfx,
        password_env=args.sign_password_env,
        certificate_sha1=args.sign_cert_sha1,
        timestamp_url=args.timestamp_url,
    )
    artifacts = None
    if not args.no_package:
        artifacts = package(
            executable,
            environment,
            create_installer=args.installer,
            signing=signing,
        )
    elif signing.pfx is not None or signing.certificate_sha1:
        sign_file(executable, environment, signing)
    log(f"executable: {executable}")
    if artifacts is not None:
        log(f"portable ZIP: {artifacts.portable_zip}")
        if artifacts.installer is not None:
            log(f"installer: {artifacts.installer}")
    if args.run:
        log("launching RRiter")
        subprocess.Popen([str(executable)], cwd=ROOT, env=environment)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (BuildError, subprocess.CalledProcessError, OSError) as error:
        print(f"[rriter-windows] ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
