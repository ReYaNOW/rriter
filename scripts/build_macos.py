#!/usr/bin/env python3
"""Build, bundle, sign, notarize, package, and run RRiter on macOS.

Script uses only Python standard library plus command-line tools shipped with
Xcode Command Line Tools. It can build arm64, x86_64, or Universal 2 bundles,
creates a native .app with Retina metadata and document declarations, signs it,
creates a compressed DMG, and optionally submits artifacts to Apple notarytool.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping, Sequence

from build_common import (
    MenuChoice,
    PlanError,
    PgoMode,
    choose,
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
APP_NAME = "RRiter"
BUNDLE_ID = "com.rriter.RRiter"
ICON_PNG = ROOT / "src" / "icons" / "icon.png"
DIST_DIR = ROOT / "dist" / "macos"
BUILD_SUPPORT_DIR = ROOT / "target" / "rriter-macos-resources"
TARGET_ARM64 = "aarch64-apple-darwin"
TARGET_X86_64 = "x86_64-apple-darwin"


class BuildError(RuntimeError):
    pass


@dataclass(frozen=True)
class BundleArtifacts:
    app: Path
    dmg: Path | None
    notarization_upload: Path | None


def log(message: str) -> None:
    print(f"[rriter-macos] {message}", flush=True)


def run(
    command: Sequence[str | os.PathLike[str]],
    *,
    cwd: Path = ROOT,
    env: Mapping[str, str] | None = None,
    capture: bool = False,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    printable = " ".join(shell_quote(os.fspath(part)) for part in command)
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


def shell_quote(value: str) -> str:
    if value and all(character.isalnum() or character in "-._/:=" for character in value):
        return value
    return "'" + value.replace("'", "'\\''") + "'"


def cargo_version() -> str:
    cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    package = cargo_toml.split("[dependencies]", 1)[0]
    match = re.search(r'^version\s*=\s*"([^"]+)"', package, re.MULTILINE)
    if not match:
        raise BuildError("Cargo.toml package version not found")
    return match.group(1)


def command_exists(name: str) -> bool:
    return shutil.which(name) is not None


def require_commands(names: Sequence[str]) -> None:
    missing = [name for name in names if not command_exists(name)]
    if missing:
        raise BuildError("missing required commands: " + ", ".join(missing))


def rust_target_installed(target: str) -> bool:
    result = run(["rustup", "target", "list", "--installed"], capture=True)
    return target in result.stdout.split()


def ensure_rust_target(target: str, install: bool) -> None:
    if rust_target_installed(target):
        return
    if not install:
        raise BuildError(
            f"Rust target {target} is missing. Run: "
            f"rustup target add {target} --toolchain nightly"
        )
    run(["rustup", "target", "add", target, "--toolchain", "nightly"])


def native_target(machine: str | None = None) -> str:
    machine = machine or os.uname().machine
    if machine == "arm64":
        return TARGET_ARM64
    if machine == "x86_64":
        return TARGET_X86_64
    raise BuildError(f"unsupported native macOS architecture: {machine}")


def targets_for_architecture(
    architecture: str,
    *,
    machine: str | None = None,
) -> list[str]:
    if architecture == "arm64":
        return [TARGET_ARM64]
    if architecture == "x86_64":
        return [TARGET_X86_64]
    if architecture == "universal":
        return [TARGET_ARM64, TARGET_X86_64]
    if architecture == "native":
        return [native_target(machine)]
    raise BuildError(f"unsupported architecture mode: {architecture}")


def validate_pgo_training_targets(
    targets: Sequence[str],
    *,
    machine: str | None = None,
) -> None:
    host = native_target(machine)
    if host == TARGET_X86_64 and TARGET_ARM64 in targets:
        raise BuildError(
            "an Intel Mac cannot execute the arm64 instrumented RRiter needed "
            "for PGO training; create that profile on Apple Silicon"
        )


def validate_minimum_system(value: str) -> str:
    if not re.fullmatch(r"\d+\.\d+(?:\.\d+)?", value):
        raise BuildError(
            "--minimum-system must be a numeric macOS version such as 12.0 or 14.5"
        )
    return value


def cargo_environment(minimum_system: str) -> dict[str, str]:
    environment = os.environ.copy()
    environment["MACOSX_DEPLOYMENT_TARGET"] = validate_minimum_system(minimum_system)
    return environment


def cargo_test_command(target: str) -> list[str]:
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


def cargo_build_command(target: str, *, release: bool) -> list[str]:
    command = ["cargo", "+nightly", "build", "--locked", "--target", target]
    if release:
        command.append("--release")
    return command


def run_macos_tests(*, install_targets: bool, minimum_system: str) -> None:
    test_target = native_target()
    ensure_rust_target(test_target, install_targets)
    run(cargo_test_command(test_target), env=cargo_environment(minimum_system))


def build_target(
    target: str,
    *,
    release: bool,
    install_target: bool,
    minimum_system: str,
    pgo_mode: PgoMode = PgoMode.OFF,
    pgo_profile: Path | None = None,
    pgo_timeout_seconds: int = 300,
) -> Path:
    ensure_rust_target(target, install_target)
    environment = cargo_environment(minimum_system)
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
                rustflags=environment.get("RUSTFLAGS", ""),
                cargo_env=environment,
            )
        )
        if executable is None:
            raise BuildError("PGO pipeline did not produce an executable")
        return executable

    command = cargo_build_command(target, release=release)
    profile = "release" if release else "debug"
    run(command, env=environment)
    executable = ROOT / "target" / target / profile / "rriter"
    if not executable.is_file():
        raise BuildError(f"built executable not found: {executable}")
    return executable


def build_executable(
    architecture: str,
    *,
    release: bool,
    run_tests: bool,
    install_targets: bool,
    minimum_system: str,
    pgo_mode: PgoMode = PgoMode.OFF,
    pgo_profile: Path | None = None,
    pgo_timeout_seconds: int = 300,
) -> tuple[Path, str]:
    targets = targets_for_architecture(architecture)
    if pgo_mode is PgoMode.FRESH:
        validate_pgo_training_targets(targets)
        if native_target() == TARGET_ARM64 and TARGET_X86_64 in targets:
            rosetta = run(
                ["/usr/bin/arch", "-x86_64", "/usr/bin/true"],
                check=False,
            )
            if rosetta.returncode != 0:
                raise BuildError(
                    "x86_64 PGO training requires Rosetta 2; install it with "
                    "softwareupdate --install-rosetta"
                )
    if run_tests:
        run_macos_tests(
            install_targets=install_targets, minimum_system=minimum_system
        )
    if len(targets) > 1 and pgo_profile is not None:
        raise BuildError(
            "--pgo-profile cannot name one file for a Universal 2 build; "
            "use the default target-specific profiles"
        )
    binaries = [
        build_target(
            target,
            release=release,
            install_target=install_targets,
            minimum_system=minimum_system,
            pgo_mode=pgo_mode,
            pgo_profile=pgo_profile,
            pgo_timeout_seconds=pgo_timeout_seconds,
        )
        for target in targets
    ]
    if len(binaries) == 1:
        return binaries[0], targets[0]
    require_commands(["lipo"])
    output = BUILD_SUPPORT_DIR / "universal" / "rriter"
    output.parent.mkdir(parents=True, exist_ok=True)
    run(["lipo", "-create", binaries[0], binaries[1], "-output", output])
    result = run(["lipo", "-archs", output], capture=True)
    architectures = set(result.stdout.split())
    if not {"arm64", "x86_64"}.issubset(architectures):
        raise BuildError(f"Universal 2 verification failed: {result.stdout.strip()}")
    return output, "universal2"


def info_plist(version: str, minimum_system: str) -> dict[str, object]:
    return {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": APP_NAME,
        "CFBundleExecutable": "RRiter",
        "CFBundleIconFile": "RRiter",
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": APP_NAME,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": version,
        "LSApplicationCategoryType": "public.app-category.developer-tools",
        "LSMinimumSystemVersion": minimum_system,
        "LSMultipleInstancesProhibited": True,
        "NSHighResolutionCapable": True,
        "NSHumanReadableCopyright": "Copyright RRiter contributors",
        "NSPrincipalClass": "NSApplication",
        "CFBundleDocumentTypes": [
            {
                "CFBundleTypeName": "Source code and text",
                "CFBundleTypeRole": "Editor",
                "LSHandlerRank": "Alternate",
                "LSItemContentTypes": [
                    "public.source-code",
                    "public.plain-text",
                    "public.json",
                    "public.data",
                ],
            }
        ],
        "UTExportedTypeDeclarations": [],
    }


def write_info_plist(path: Path, version: str, minimum_system: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as output:
        plistlib.dump(info_plist(version, minimum_system), output, sort_keys=True)


def generate_icns(output: Path) -> None:
    require_commands(["sips", "iconutil"])
    iconset = BUILD_SUPPORT_DIR / "RRiter.iconset"
    if iconset.exists():
        shutil.rmtree(iconset)
    iconset.mkdir(parents=True)
    sizes = [16, 32, 128, 256, 512]
    for size in sizes:
        destination = iconset / f"icon_{size}x{size}.png"
        run(["sips", "-z", str(size), str(size), ICON_PNG, "--out", destination])
        retina = iconset / f"icon_{size}x{size}@2x.png"
        run(["sips", "-z", str(size * 2), str(size * 2), ICON_PNG, "--out", retina])
    output.parent.mkdir(parents=True, exist_ok=True)
    run(["iconutil", "-c", "icns", iconset, "-o", output])
    if not output.is_file():
        raise BuildError("iconutil did not create RRiter.icns")


def bundle_layout(app: Path) -> tuple[Path, Path, Path]:
    contents = app / "Contents"
    macos = contents / "MacOS"
    resources = contents / "Resources"
    return contents, macos, resources


def create_bundle(
    executable: Path,
    *,
    version: str,
    architecture_label: str,
    minimum_system: str,
) -> Path:
    app = DIST_DIR / f"RRiter-{version}-{architecture_label}.app"
    if app.exists():
        shutil.rmtree(app)
    contents, macos, resources = bundle_layout(app)
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)
    destination = macos / "RRiter"
    shutil.copy2(executable, destination)
    destination.chmod(0o755)
    write_info_plist(contents / "Info.plist", version, minimum_system)
    (contents / "PkgInfo").write_bytes(b"APPL????")
    generate_icns(resources / "RRiter.icns")
    return app


def entitlements_plist() -> dict[str, object]:
    # RRiter is not sandboxed and uses only public APIs. Empty entitlements keep
    # hardened runtime strict instead of granting JIT or library exceptions.
    return {}


def write_entitlements(path: Path) -> None:
    with path.open("wb") as output:
        plistlib.dump(entitlements_plist(), output, sort_keys=True)


def sign_bundle(
    app: Path,
    *,
    identity: str,
    timestamp: bool,
    hardened_runtime: bool,
) -> None:
    require_commands(["codesign"])
    executable = app / "Contents" / "MacOS" / "RRiter"
    common: list[str | os.PathLike[str]] = [
        "codesign",
        "--force",
        "--sign",
        identity,
    ]
    if hardened_runtime:
        common.extend(["--options", "runtime"])
    if timestamp and identity != "-":
        common.append("--timestamp")

    # Sign nested code first and the bundle last. Avoid --deep for signing: it
    # can silently rewrite nested signatures and produce non-reproducible apps.
    run([*common, executable])
    run([*common, app])
    run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", app])
    run(["codesign", "--display", "--verbose=4", app], check=False)


def create_dmg(app: Path, version: str, architecture_label: str) -> Path:
    require_commands(["hdiutil"])
    dmg = DIST_DIR / f"RRiter-{version}-{architecture_label}.dmg"
    if dmg.exists():
        dmg.unlink()
    with tempfile.TemporaryDirectory(prefix="rriter-dmg-") as directory:
        staging = Path(directory)
        shutil.copytree(app, staging / "RRiter.app", symlinks=True)
        applications = staging / "Applications"
        applications.symlink_to("/Applications")
        run(
            [
                "hdiutil",
                "create",
                "-volname",
                "RRiter",
                "-srcfolder",
                staging,
                "-ov",
                "-format",
                "UDZO",
                dmg,
            ]
        )
    if not dmg.is_file():
        raise BuildError("hdiutil did not create DMG")
    return dmg


def create_notarization_zip(app: Path, version: str, architecture_label: str) -> Path:
    require_commands(["ditto"])
    archive = DIST_DIR / f"RRiter-{version}-{architecture_label}-notarization.zip"
    if archive.exists():
        archive.unlink()
    run(["ditto", "-c", "-k", "--keepParent", app, archive])
    return archive


def notarize(
    artifact: Path,
    *,
    profile: str,
    staple_target: Path,
) -> None:
    require_commands(["xcrun"])
    run(
        [
            "xcrun",
            "notarytool",
            "submit",
            artifact,
            "--keychain-profile",
            profile,
            "--wait",
        ]
    )
    run(["xcrun", "stapler", "staple", staple_target])
    run(["xcrun", "stapler", "validate", staple_target])


def verify_gatekeeper(app: Path, *, required: bool) -> None:
    require_commands(["spctl"])
    result = run(
        ["spctl", "--assess", "--type", "execute", "--verbose=4", app],
        check=False,
    )
    if required and result.returncode != 0:
        raise BuildError("Gatekeeper rejected the signed and notarized app bundle")


def artifact_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package_bundle(
    app: Path,
    *,
    version: str,
    architecture_label: str,
    create_disk_image: bool,
    notary_profile: str | None,
) -> BundleArtifacts:
    upload = None
    dmg = None
    if notary_profile:
        upload = create_notarization_zip(app, version, architecture_label)
        notarize(upload, profile=notary_profile, staple_target=app)
    if create_disk_image:
        dmg = create_dmg(app, version, architecture_label)
        if notary_profile:
            notarize(dmg, profile=notary_profile, staple_target=dmg)
        log(f"DMG SHA-256: {artifact_digest(dmg)}")
    verify_gatekeeper(app, required=notary_profile is not None)
    return BundleArtifacts(app=app, dmg=dmg, notarization_upload=upload)


def self_test() -> None:
    version = cargo_version()
    value = info_plist(version, "12.0")
    required = {
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleExecutable": "RRiter",
        "NSHighResolutionCapable": True,
        "NSPrincipalClass": "NSApplication",
    }
    for key, expected in required.items():
        if value.get(key) != expected:
            raise BuildError(f"Info.plist self-test failed for {key}")
    if targets_for_architecture("native", machine="arm64") != [TARGET_ARM64]:
        raise BuildError("native arm64 target selection failed")
    if targets_for_architecture("native", machine="x86_64") != [TARGET_X86_64]:
        raise BuildError("native x86_64 target selection failed")
    if targets_for_architecture("universal") != [TARGET_ARM64, TARGET_X86_64]:
        raise BuildError("Universal 2 target selection failed")
    validate_pgo_training_targets([TARGET_X86_64], machine="arm64")
    try:
        validate_pgo_training_targets([TARGET_ARM64], machine="x86_64")
    except BuildError:
        pass
    else:
        raise BuildError("Intel hosts must reject arm64 PGO training")
    if cargo_test_command(TARGET_ARM64)[-2:] != ["--", "--test-threads=1"]:
        raise BuildError("macOS tests must follow the project serial-test policy")
    if cargo_build_command(TARGET_ARM64, release=True)[-1] != "--release":
        raise BuildError("release cargo command self-test failed")
    if cargo_environment("12.0").get("MACOSX_DEPLOYMENT_TARGET") != "12.0":
        raise BuildError("deployment target was not applied to Cargo")
    try:
        validate_minimum_system("latest")
    except BuildError:
        pass
    else:
        raise BuildError("invalid deployment targets must be rejected")
    with tempfile.TemporaryDirectory(prefix="rriter-macos-selftest-") as directory:
        plist = Path(directory) / "Info.plist"
        write_info_plist(plist, version, "12.0")
        with plist.open("rb") as source:
            decoded = plistlib.load(source)
        if decoded["CFBundleShortVersionString"] != version:
            raise BuildError("Info.plist version roundtrip failed")
        entitlements = Path(directory) / "entitlements.plist"
        write_entitlements(entitlements)
        with entitlements.open("rb") as source:
            decoded_entitlements = plistlib.load(source)
        if decoded_entitlements != {}:
            raise BuildError("hardened runtime entitlements must remain empty")
    build_plan_self_test()
    log("self-test passed")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--arch",
        choices=["native", "arm64", "x86_64", "universal"],
        default="native",
    )
    parser.add_argument("--debug", action="store_true")
    parser.add_argument("--test", action="store_true")
    parser.add_argument("--tests-only", action="store_true", help="Run tests and stop")
    parser.add_argument("--build-only", action="store_true", help="Build without tests")
    parser.add_argument("--install-targets", action="store_true")
    parser.add_argument("--minimum-system", default="12.0")
    parser.add_argument(
        "--sign-identity",
        default="-",
        help="Developer ID Application identity; '-' performs ad-hoc signing",
    )
    parser.add_argument(
        "--no-hardened-runtime",
        action="store_true",
        help="Disable hardened runtime for local diagnostic builds",
    )
    parser.add_argument("--no-dmg", action="store_true")
    parser.add_argument(
        "--notary-profile",
        help="notarytool keychain profile created with store-credentials",
    )
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--pgo", choices=[mode.value for mode in PgoMode], default="off")
    parser.add_argument("--pgo-profile", type=Path)
    parser.add_argument("--pgo-timeout-seconds", type=int, default=300)
    parser.add_argument("--menu", action="store_true", help="Force the interactive menu")
    parser.add_argument("--yes", action="store_true", help="Do not ask for final confirmation")
    parser.add_argument("--print-plan", action="store_true", help="Print the plan and exit")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def requested_plan(args: argparse.Namespace, raw_argv: Sequence[str]):
    if args.menu and not is_interactive():
        raise BuildError("--menu requires an interactive terminal")
    menu = should_open_menu(raw_argv, force=args.menu)
    if menu:
        plan = interactive_build_plan("macOS", supports_installer=False)
        if plan.build:
            args.arch = str(
                choose(
                    "Target architecture",
                    (
                        MenuChoice("Native", "native", "current Mac"),
                        MenuChoice("Apple Silicon", "arm64"),
                        MenuChoice("Intel", "x86_64"),
                        MenuChoice("Universal 2", "universal", "arm64 + x86_64"),
                    ),
                    default=0,
                )
            )
        if not args.yes:
            print_plan(
                plan,
                platform_lines=(
                    f"Architecture: {args.arch}",
                    f"Minimum macOS: {args.minimum_system}",
                ),
            )
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
        package=not args.no_dmg,
        installer=False,
        run_after_build=args.run,
        pgo=args.pgo,
        pgo_profile=str(args.pgo_profile) if args.pgo_profile else None,
    )


def main(argv: Sequence[str] | None = None) -> int:
    raw_argv = list(sys.argv[1:] if argv is None else argv)
    if not raw_argv and not is_interactive():
        print(
            "[rriter-macos] No interactive terminal detected. "
            "Run with --help or pass explicit build flags.",
            file=sys.stderr,
        )
        return 2
    args = parse_args(raw_argv)
    if args.self_test:
        self_test()
        return 0
    plan = requested_plan(args, raw_argv)
    minimum_system = validate_minimum_system(args.minimum_system)
    print_plan(
        plan,
        platform_lines=(
            f"Architecture: {args.arch}",
            f"Minimum macOS: {minimum_system}",
        ),
    )
    if args.print_plan:
        return 0
    if sys.platform != "darwin":
        raise BuildError("macOS build must run on macOS; use --self-test elsewhere")

    require_commands(["cargo", "rustup"])
    if not plan.build:
        run_macos_tests(
            install_targets=args.install_targets, minimum_system=minimum_system
        )
        log("tests completed; no build requested")
        return 0

    require_commands(["xcrun", "codesign", "sips", "iconutil"])
    version = cargo_version()
    executable, architecture_label = build_executable(
        args.arch,
        release=plan.release,
        run_tests=plan.run_tests,
        install_targets=args.install_targets,
        minimum_system=minimum_system,
        pgo_mode=plan.pgo,
        pgo_profile=args.pgo_profile,
        pgo_timeout_seconds=args.pgo_timeout_seconds,
    )
    app = create_bundle(
        executable,
        version=version,
        architecture_label=architecture_label,
        minimum_system=minimum_system,
    )
    hardened_runtime = not args.no_hardened_runtime
    if args.sign_identity == "-" and args.notary_profile:
        raise BuildError("notarization requires a Developer ID Application identity")
    if args.no_hardened_runtime and args.notary_profile:
        raise BuildError("notarization requires the hardened runtime")
    sign_bundle(
        app,
        identity=args.sign_identity,
        timestamp=args.sign_identity != "-",
        hardened_runtime=hardened_runtime,
    )
    artifacts = package_bundle(
        app,
        version=version,
        architecture_label=architecture_label,
        create_disk_image=plan.package,
        notary_profile=args.notary_profile,
    )
    log(f"app bundle: {artifacts.app}")
    if artifacts.dmg is not None:
        log(f"disk image: {artifacts.dmg}")
    if plan.run_after_build:
        run(["open", artifacts.app])
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (BuildError, PlanError, PgoError, subprocess.CalledProcessError, OSError) as error:
        print(f"[rriter-macos] ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
