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


def targets_for_architecture(architecture: str) -> list[str]:
    if architecture == "arm64":
        return [TARGET_ARM64]
    if architecture == "x86_64":
        return [TARGET_X86_64]
    if architecture == "universal":
        return [TARGET_ARM64, TARGET_X86_64]
    if architecture == "native":
        machine = os.uname().machine
        if machine == "arm64":
            return [TARGET_ARM64]
        if machine == "x86_64":
            return [TARGET_X86_64]
        raise BuildError(f"unsupported native macOS architecture: {machine}")
    raise BuildError(f"unsupported architecture mode: {architecture}")


def build_target(
    target: str,
    *,
    release: bool,
    run_tests: bool,
    install_target: bool,
) -> Path:
    ensure_rust_target(target, install_target)
    if run_tests:
        run(["cargo", "+nightly", "test", "--locked", "--target", target])
    command = ["cargo", "+nightly", "build", "--locked", "--target", target]
    profile = "debug"
    if release:
        command.append("--release")
        profile = "release"
    run(command)
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
) -> tuple[Path, str]:
    targets = targets_for_architecture(architecture)
    binaries = [
        build_target(
            target,
            release=release,
            run_tests=run_tests,
            install_target=install_targets,
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


def verify_gatekeeper(app: Path) -> None:
    require_commands(["spctl"])
    run(["spctl", "--assess", "--type", "execute", "--verbose=4", app], check=False)


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
    verify_gatekeeper(app)
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
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.self_test:
        self_test()
        return 0
    if sys.platform != "darwin":
        raise BuildError("macOS build must run on macOS; use --self-test elsewhere")
    require_commands(["cargo", "rustup", "xcrun", "codesign", "sips", "iconutil"])
    version = cargo_version()
    executable, architecture_label = build_executable(
        args.arch,
        release=not args.debug,
        run_tests=args.test,
        install_targets=args.install_targets,
    )
    app = create_bundle(
        executable,
        version=version,
        architecture_label=architecture_label,
        minimum_system=args.minimum_system,
    )
    hardened_runtime = not args.no_hardened_runtime
    if args.sign_identity == "-" and args.notary_profile:
        raise BuildError("notarization requires a Developer ID Application identity")
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
        create_disk_image=not args.no_dmg,
        notary_profile=args.notary_profile,
    )
    log(f"app bundle: {artifacts.app}")
    if artifacts.dmg is not None:
        log(f"disk image: {artifacts.dmg}")
    if args.run:
        run(["open", artifacts.app])
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (BuildError, subprocess.CalledProcessError, OSError) as error:
        print(f"[rriter-macos] ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
