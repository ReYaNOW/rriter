#!/usr/bin/env python3
"""Download and install the pinned PDFium library for RRiter."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tarfile
import tempfile
import urllib.request


ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST_PATH = os.path.join(ROOT, "pdfium.json")
BASE_URL = "https://github.com/bblanchon/pdfium-binaries/releases/download"


def verify_sha256(path: str, expected: str) -> bool:
    digest = hashlib.sha256()
    with open(path, "rb") as source:
        while True:
            chunk = source.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest() == expected.lower()


def _safe_member_name(name: str) -> str:
    while name.startswith("./"):
        name = name[2:]
    normalized = os.path.normpath(name)
    if os.path.isabs(normalized) or normalized == ".." or normalized.startswith(".." + os.sep):
        raise ValueError("unsafe archive member path")
    return normalized


def extract_lib(archive: str, lib_member: str, dest_file: str) -> None:
    wanted = _safe_member_name(lib_member)
    parent = os.path.dirname(os.path.abspath(dest_file))
    os.makedirs(parent, exist_ok=True)
    temp_path = None
    try:
        with tarfile.open(archive, "r:gz") as package:
            selected = None
            for member in package.getmembers():
                normalized = _safe_member_name(member.name)
                if normalized == wanted:
                    if not member.isfile():
                        raise ValueError("PDFium archive member is not a regular file")
                    selected = member
                    break
            if selected is None:
                raise ValueError(f"в архиве нет {lib_member}")
            source = package.extractfile(selected)
            if source is None:
                raise ValueError("PDFium archive member could not be read")
            fd, temp_path = tempfile.mkstemp(prefix=".pdfium-", dir=parent)
            with os.fdopen(fd, "wb") as output, source:
                while True:
                    chunk = source.read(1024 * 1024)
                    if not chunk:
                        break
                    output.write(chunk)
        os.replace(temp_path, dest_file)
        temp_path = None
    finally:
        if temp_path is not None:
            try:
                os.unlink(temp_path)
            except FileNotFoundError:
                pass


def _data_dir() -> str:
    home = os.environ.get("HOME") or os.environ.get("USERPROFILE") or "."
    if sys.platform == "win32":
        local = os.environ.get("LOCALAPPDATA") or os.path.join(home, "AppData", "Local")
        return os.path.join(local, "RRiter")
    if sys.platform == "darwin":
        return os.path.join(home, "Library", "Application Support", "RRiter")
    data_root = os.environ.get("XDG_DATA_HOME") or os.path.join(home, ".local", "share")
    return os.path.join(data_root, "RRiter")


def _platform_key() -> str:
    if sys.platform == "win32":
        return "win-x64"
    machine = os.uname().machine.lower() if hasattr(os, "uname") else os.environ.get("PROCESSOR_ARCHITECTURE", "").lower()
    if sys.platform == "darwin":
        return "mac-arm64" if machine in ("arm64", "aarch64") else "mac-x64"
    return "linux-x64"


def _download(url: str, destination: str) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "RRiter PDFium fetcher"})
    with urllib.request.urlopen(request, timeout=60) as response, open(destination, "wb") as output:
        while True:
            chunk = response.read(1024 * 1024)
            if not chunk:
                break
            output.write(chunk)


def _fetch(entry: dict[str, str], version: str, dest: str, *, force: bool) -> str:
    installed = os.path.join(dest, os.path.basename(entry["lib"]))
    if os.path.isfile(installed) and not force:
        return installed
    os.makedirs(dest, exist_ok=True)
    fd, archive_path = tempfile.mkstemp(prefix=".pdfium-download-", dir=dest)
    os.close(fd)
    try:
        url = f"{BASE_URL}/{version}/{entry['archive']}"
        _download(url, archive_path)
        if not verify_sha256(archive_path, entry["sha256"]):
            raise RuntimeError("архив повреждён или версия не совпадает")
        extract_lib(archive_path, entry["lib"], installed)
    finally:
        try:
            os.unlink(archive_path)
        except FileNotFoundError:
            pass
    return installed


def _self_test() -> None:
    global _download
    import io

    with tempfile.TemporaryDirectory(prefix="rriter-pdfium-selftest-") as directory:
        archive = os.path.join(directory, "fixture.tgz")
        with tarfile.open(archive, "w:gz") as package:
            for name, data in (("lib/libpdfium.so", b"lib"), ("./lib/other", b"other"), ("../evil", b"evil")):
                info = tarfile.TarInfo(name)
                info.size = len(data)
                package.addfile(info, io.BytesIO(data))

        with open(archive, "rb") as source:
            expected = hashlib.sha256(source.read()).hexdigest()
        if not verify_sha256(archive, expected):
            raise RuntimeError("valid SHA-256 self-test failed")
        if verify_sha256(archive, "0" * 64):
            raise RuntimeError("invalid SHA-256 self-test failed")

        library = os.path.join(directory, "libpdfium.so")
        extract_lib(archive, "lib/libpdfium.so", library)
        with open(library, "rb") as result:
            if result.read() != b"lib":
                raise RuntimeError("byte-exact extraction self-test failed")
        other = os.path.join(directory, "other")
        extract_lib(archive, "lib/other", other)
        with open(other, "rb") as result:
            if result.read() != b"other":
                raise RuntimeError("leading ./ extraction self-test failed")

        try:
            extract_lib(archive, "../evil", os.path.join(directory, "rejected"))
        except ValueError as error:
            if "unsafe archive member path" not in str(error):
                raise RuntimeError(f"unexpected traversal error: {error}")
        else:
            raise RuntimeError("unsafe archive member accepted")
        missing_archive = os.path.join(directory, "missing.tgz")
        with tarfile.open(missing_archive, "w:gz") as package:
            info = tarfile.TarInfo("safe/member")
            info.size = 0
            package.addfile(info, io.BytesIO())
        try:
            extract_lib(missing_archive, "lib/none", os.path.join(directory, "missing"))
        except ValueError as error:
            if "в архиве нет lib/none" not in str(error):
                raise RuntimeError(f"unexpected missing-member error: {error}")
        else:
            raise RuntimeError("missing archive member accepted")

        bad_fetch_dir = os.path.join(directory, "bad-hash")

        def write_invalid_archive(_url: str, destination: str) -> None:
            with open(destination, "wb") as output:
                output.write(b"invalid archive")

        original_download = _download
        try:
            _download = write_invalid_archive
            try:
                _fetch({"archive": "bad.tgz", "sha256": "0" * 64, "lib": "lib/x"}, "test", bad_fetch_dir, force=True)
            except RuntimeError as error:
                if str(error) != "архив повреждён или версия не совпадает":
                    raise
            else:
                raise RuntimeError("bad archive hash was accepted")
        finally:
            _download = original_download
        if os.listdir(bad_fetch_dir):
            raise RuntimeError("bad-hash temporary file was not deleted")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dest")
    parser.add_argument("--platform", dest="platform_key")
    parser.add_argument("--print-hashes", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        _self_test()
        return 0

    with open(MANIFEST_PATH, "r", encoding="utf-8") as source:
        manifest = json.load(source)
    if args.print_hashes:
        for key, entry in manifest["platforms"].items():
            fd, path = tempfile.mkstemp(prefix="pdfium-hash-")
            os.close(fd)
            try:
                _download(f"{BASE_URL}/{manifest['version']}/{entry['archive']}", path)
                digest = hashlib.sha256()
                with open(path, "rb") as source:
                    while True:
                        chunk = source.read(1024 * 1024)
                        if not chunk:
                            break
                        digest.update(chunk)
                print(key, digest.hexdigest())
            finally:
                os.unlink(path)
        return 0

    key = args.platform_key or _platform_key()
    entry = manifest["platforms"].get(key)
    if entry is None:
        print(f"unsupported PDFium platform: {key}", file=sys.stderr)
        return 2
    version_slug = manifest["version"].replace("/", "-")
    dest = args.dest or os.path.join(_data_dir(), "tools", "managed", "pdfium", version_slug)
    try:
        installed = _fetch(entry, manifest["version"], dest, force=args.force)
    except RuntimeError as error:
        print(str(error), file=sys.stderr)
        return 2
    except (OSError, tarfile.TarError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 1
    print(os.path.abspath(installed))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
