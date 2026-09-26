#!/usr/bin/env python3
"""Drive `rriter --headless`: screenshots, command scripts, and an interactive REPL.

No third-party Python modules are required. The script never builds RRiter; it
runs the release binary (`RRITER_BIN` overrides the path) and speaks the line
protocol described in docs/headless.md.

    python3 scripts/rriter_headless.py shot src/main.rs      # prints the PNG path
    python3 scripts/rriter_headless.py run script.txt --size 1280x800
    python3 scripts/rriter_headless.py repl
"""

from __future__ import annotations

import argparse
import contextlib
import io
import os
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import IO, Sequence

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BIN = REPO_ROOT / "target" / "x86_64-unknown-linux-gnu" / "release" / "rriter"
SHOT_DIR = Path("/tmp/rriter-headless")


def classify(line: str) -> str:
    """Protocol reply kind: `ok`, `err`, or `app` for anything the app printed itself."""
    for kind in ("ok", "err"):
        if line == kind or line.startswith(kind + " "):
            return kind
    return "app"


def parse_cli(argv: Sequence[str]) -> argparse.Namespace:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--size", help="WxH, default 1920x1080")
    common.add_argument("--scale", help="UI scale, default 1.0")
    common.add_argument("--profile", help="reusable profile directory")
    common.add_argument("--profile-from-user", action="store_true", help="copy the user's RRiter state into a temp profile")
    common.add_argument("--hz", help="frame budget as refresh rate")
    common.add_argument("--budget-ms", help="frame budget in milliseconds")
    common.add_argument("--keep-profile", action="store_true", help="keep the temp profile on exit")
    common.add_argument("--allow-writes", action="store_true", help="allow writing opened files, file tree, and Git")

    parser = argparse.ArgumentParser(prog="rriter_headless.py", description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    shot = sub.add_parser("shot", parents=[common], help="open a file or folder, settle, save a PNG")
    shot.add_argument("target", help="file (open) or directory (workspace)")
    shot.add_argument("--out", help="PNG path, default /tmp/rriter-headless/<name>-<ts>.png")
    shot.add_argument("--settle", type=int, default=500, help="settle budget in ms (default 500)")
    run = sub.add_parser("run", parents=[common], help="run a command script")
    run.add_argument("script", help="command file, '-' for stdin")
    sub.add_parser("repl", parents=[common], help="type commands interactively")
    return parser.parse_args(argv)


def headless_args(ns: argparse.Namespace) -> list[str]:
    args = ["--headless"]
    for flag, value in (("--size", ns.size), ("--scale", ns.scale), ("--profile", ns.profile)):
        if value is not None:
            args += [flag, value]
    if ns.profile_from_user:
        args.append("--profile-from-user")
    for flag, value in (("--hz", ns.hz), ("--budget-ms", ns.budget_ms)):
        if value is not None:
            args += [flag, value]
    if ns.keep_profile:
        args.append("--keep-profile")
    if ns.allow_writes:
        args.append("--allow-writes")
    return args


def default_shot_path(target: Path, now: datetime) -> Path:
    return SHOT_DIR / f"{target.name}-{now:%Y%m%d-%H%M%S}.png"


def shot_commands(target: Path, settle_ms: int, out: Path) -> list[str]:
    opener = "workspace" if target.is_dir() else "open"
    return [f"{opener} {target}", f"settle {settle_ms}", f"screenshot {out}", "quit"]


def find_binary() -> Path | None:
    path = Path(os.environ.get("RRITER_BIN") or DEFAULT_BIN)
    if path.is_file():
        return path
    print(f"rriter binary not found at {path}; run make fast", file=sys.stderr)
    return None


def spawn(binary: Path, args: list[str], stdin: int | None) -> subprocess.Popen[str]:
    # stderr is inherited: app diagnostics reach the caller unchanged.
    return subprocess.Popen([str(binary), *args], stdin=stdin, stdout=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")


def read_reply(stdout: IO[str]) -> str | None:
    """Next `ok`/`err` line; other app output goes to stderr. None at EOF."""
    for raw in stdout:
        line = raw.rstrip("\n")
        if classify(line) != "app":
            return line
        print(f"app: {line}", file=sys.stderr)
    return None


def exit_status(code: int) -> int:
    return 128 - code if code < 0 else code


def cmd_shot(binary: Path, ns: argparse.Namespace) -> int:
    target = Path(ns.target).resolve()
    out = Path(ns.out).resolve() if ns.out else default_shot_path(target, datetime.now())
    commands = shot_commands(target, ns.settle, out)
    proc = spawn(binary, headless_args(ns), subprocess.PIPE)
    assert proc.stdin is not None and proc.stdout is not None
    answered = 0
    failed = False
    for command in commands:
        try:
            proc.stdin.write(command + "\n")
            proc.stdin.flush()
        except BrokenPipeError:
            break
        reply = read_reply(proc.stdout)
        if reply is None:
            break
        answered += 1
        if classify(reply) == "err":
            print(f"{command}: {reply}", file=sys.stderr)
            failed = True
    with contextlib.suppress(BrokenPipeError):
        proc.stdin.close()
    while read_reply(proc.stdout) is not None:
        pass
    code = exit_status(proc.wait())
    if failed:
        return 1
    if code != 0:
        return code
    if answered < len(commands):
        print("rriter exited before answering every command", file=sys.stderr)
        return 1
    print(out)
    return 0


def cmd_relay(binary: Path, args: list[str]) -> int:
    """Run with the caller's stdin; protocol replies to stdout, other lines to stderr."""
    proc = spawn(binary, args, None)
    assert proc.stdout is not None
    try:
        while (reply := read_reply(proc.stdout)) is not None:
            print(reply, flush=True)
    except KeyboardInterrupt:
        pass
    return exit_status(proc.wait())


def self_test() -> None:
    import tempfile

    for line, kind in [("ok", "ok"), ("ok x", "ok"), ("err x", "err"), ("err", "err"), ("okay", "app"), ("", "app"), ("error x", "app")]:
        assert classify(line) == kind, (line, classify(line))

    ns = parse_cli(["shot", "src/main.rs", "--size", "800x600", "--scale", "2", "--out", "a.png", "--settle", "250", "--profile-from-user"])
    assert (ns.command, ns.target, ns.size, ns.scale, ns.out, ns.settle, ns.profile_from_user) == ("shot", "src/main.rs", "800x600", "2", "a.png", 250, True), ns
    assert headless_args(ns) == ["--headless", "--size", "800x600", "--scale", "2", "--profile-from-user"], headless_args(ns)
    ns = parse_cli(["shot", "x.rs"])
    assert (ns.settle, ns.out, ns.size) == (500, None, None), ns
    assert headless_args(ns) == ["--headless"], headless_args(ns)
    ns = parse_cli(["run", "s.txt", "--hz", "144", "--allow-writes", "--keep-profile", "--profile", "/tmp/p"])
    assert (ns.command, ns.script) == ("run", "s.txt"), ns
    assert headless_args(ns) == ["--headless", "--profile", "/tmp/p", "--hz", "144", "--keep-profile", "--allow-writes"], headless_args(ns)
    ns = parse_cli(["repl", "--budget-ms", "4.2"])
    assert ns.command == "repl" and headless_args(ns) == ["--headless", "--budget-ms", "4.2"], ns
    for bad in ([], ["frob"], ["shot"], ["shot", "x", "--settle", "soon"]):
        try:
            with contextlib.redirect_stderr(io.StringIO()):
                parse_cli(bad)
        except SystemExit:
            continue
        raise AssertionError(f"parse_cli accepted {bad}")

    now = datetime(2026, 9, 26, 14, 5, 9)
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        file = root / "notes dir" / "main.rs"
        file.parent.mkdir()
        file.write_text("fn main() {}\n")
        assert default_shot_path(file, now) == SHOT_DIR / "main.rs-20260926-140509.png"
        assert default_shot_path(file.parent, now) == SHOT_DIR / "notes dir-20260926-140509.png"
        out = SHOT_DIR / "x.png"
        assert shot_commands(file, 500, out) == [f"open {file}", "settle 500", f"screenshot {out}", "quit"]
        assert shot_commands(file.parent, 0, out) == [f"workspace {file.parent}", "settle 0", f"screenshot {out}", "quit"]
    print("rriter_headless self-test: ok")


def main(argv: Sequence[str]) -> int:
    if list(argv) == ["--self-test"]:
        self_test()
        return 0
    ns = parse_cli(argv)
    binary = find_binary()
    if binary is None:
        return 2
    if ns.command == "shot":
        return cmd_shot(binary, ns)
    if ns.command == "run":
        return cmd_relay(binary, [*headless_args(ns), "--script", ns.script])
    return cmd_relay(binary, headless_args(ns))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
