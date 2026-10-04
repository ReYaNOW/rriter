#!/usr/bin/env python3
"""Capture reproducible RRiter UI screens and compare PNG pixels."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import struct
import subprocess
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "scripts" / "rriter_headless.py"
SIZE = "1280x720"
SCALE = "1"
THEMES = {"dracula", "one_dark", "forest", "sepia", "one_light"}
# Status clock and transient caret area; fixed in framebuffer coordinates.
MASKS = {"status-bar.png": [(1170, 690, 1280, 720)]}


class Session:
    def __init__(self, profile: Path, workspace: Path | None, config: dict):
        profile.mkdir(parents=True, exist_ok=True)
        config_dir = profile / "config"
        config_dir.mkdir(exist_ok=True)
        (config_dir / "config.json").write_text(json.dumps(config), encoding="utf-8")
        args = [str(DRIVER), "run", "-", "--size", SIZE, "--scale", SCALE, "--profile", str(profile)]
        env = os.environ.copy()
        env["RRITER_BIN"] = str(ROOT / "target/x86_64-unknown-linux-gnu/release/rriter")
        self.proc = subprocess.Popen(
            [sys.executable, *args], cwd=ROOT, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8",
        )
        self.command("workspace " + str(workspace) if workspace else "settle 150")
        if workspace:
            self.command("open " + str(workspace / "main.rs"))
            self.command("settle 250")

    def command(self, command: str) -> str:
        assert self.proc.stdin is not None and self.proc.stdout is not None
        self.proc.stdin.write(command + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline().strip()
        if not line.startswith("ok"):
            raise RuntimeError(f"{command!r}: {line}")
        return line[3:].strip()

    def dump(self) -> dict:
        return json.loads(self.command("dump"))

    def click(self, wanted: str) -> dict:
        state = self.dump()
        element = next((e for e in state.get("ui", []) if e.get("id") == wanted), None)
        if not element:
            raise RuntimeError(f"UI element missing: {wanted}")
        x, y, width, height = element["rect"]
        self.command(f"mouse_move {x + width / 2:g} {y + height / 2:g}")
        self.command("click")
        self.command("settle 250")
        return self.dump()

    def shot(self, output: Path) -> None:
        self.command("settle 150")
        self.command("screenshot " + str(output))

    def close(self) -> None:
        if self.proc.stdin and self.proc.poll() is None:
            self.proc.stdin.write("quit\n")
            self.proc.stdin.flush()
        try:
            _, err = self.proc.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            _, err = self.proc.communicate()
            raise RuntimeError("headless driver timed out")
        if self.proc.returncode:
            raise RuntimeError(err[-3000:])


def png_pixels(path: Path) -> tuple[int, int, list[bytes]]:
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"not a PNG: {path}")
    offset, compressed, width, height, color, depth, interlace = 8, bytearray(), 0, 0, 0, 0, 0
    while offset < len(data):
        length = struct.unpack(">I", data[offset:offset + 4])[0]
        kind = data[offset + 4:offset + 8]
        chunk = data[offset + 8:offset + 8 + length]
        offset += length + 12
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
        elif kind == b"IDAT":
            compressed.extend(chunk)
        elif kind == b"IEND":
            break
    channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(color)
    if depth != 8 or channels is None or interlace:
        raise ValueError(f"unsupported PNG encoding in {path}")
    raw = zlib.decompress(compressed)
    stride = width * channels
    rows: list[bytes] = []
    prior = bytearray(stride)
    pos = 0
    for _ in range(height):
        filter_type = raw[pos]
        scan = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += stride + 1
        for i in range(stride):
            left = scan[i - channels] if i >= channels else 0
            up = prior[i]
            upper_left = prior[i - channels] if i >= channels else 0
            if filter_type == 1:
                scan[i] = (scan[i] + left) & 255
            elif filter_type == 2:
                scan[i] = (scan[i] + up) & 255
            elif filter_type == 3:
                scan[i] = (scan[i] + ((left + up) >> 1)) & 255
            elif filter_type == 4:
                p = left + up - upper_left
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - upper_left)
                predictor = left if pa <= pb and pa <= pc else up if pb <= pc else upper_left
                scan[i] = (scan[i] + predictor) & 255
            elif filter_type != 0:
                raise ValueError(f"unsupported PNG filter {filter_type}")
        rows.append(bytes(scan))
        prior = scan
    return width, height, rows


def compare(first: Path, second: Path, excluded: set[str]) -> int:
    names_a = {p.name for p in first.glob("*.png") if p.name not in excluded}
    names_b = {p.name for p in second.glob("*.png") if p.name not in excluded}
    failed = False
    for name in sorted(names_a | names_b):
        if name not in names_a or name not in names_b:
            print(f"{name}: missing from {'first' if name not in names_a else 'second'} directory")
            failed = True
            continue
        wa, ha, a = png_pixels(first / name)
        wb, hb, b = png_pixels(second / name)
        if (wa, ha) != (wb, hb):
            print(f"{name}: frame size differs ({wa}x{ha} vs {wb}x{hb})")
            failed = True
            continue
        masks = MASKS.get(name, [])
        diffs, bounds = 0, [wa, ha, -1, -1]
        for y, (row_a, row_b) in enumerate(zip(a, b)):
            for x in range(wa):
                if any(x1 <= x < x2 and y1 <= y < y2 for x1, y1, x2, y2 in masks):
                    continue
                i = x * (len(row_a) // wa)
                j = x * (len(row_b) // wb)
                if row_a[i:i + len(row_a) // wa] != row_b[j:j + len(row_b) // wb]:
                    diffs += 1
                    bounds[0], bounds[1] = min(bounds[0], x), min(bounds[1], y)
                    bounds[2], bounds[3] = max(bounds[2], x), max(bounds[3], y)
        bbox = "none" if not diffs else f"({bounds[0]},{bounds[1]})-({bounds[2]},{bounds[3]})"
        print(f"{name}: {diffs} differing pixels; bbox {bbox}")
        failed |= diffs != 0
    if not names_a and not names_b:
        print("no PNG files to compare")
        return 1
    return int(failed)


def git_fixture(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)
    (path / "main.rs").write_text("// Dracula theme snapshot\nfn greet(name: &str) -> String {\n    format!(\"hello {name}\")\n}\n\nfn main() {\n    println!(\"{}\", greet(\"world\"));\n}\n", encoding="utf-8")
    (path / "README.md").write_text("# Theme fixture\n\nA reproducible sample workspace.\n", encoding="utf-8")
    env = os.environ | {"GIT_AUTHOR_NAME": "Theme Fixture", "GIT_COMMITTER_NAME": "Theme Fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid", "GIT_COMMITTER_EMAIL": "fixture@example.invalid", "GIT_AUTHOR_DATE": "2024-01-01T00:00:00Z", "GIT_COMMITTER_DATE": "2024-01-01T00:00:00Z"}
    for args in (["init", "-q", str(path)], ["-C", str(path), "add", "."], ["-C", str(path), "commit", "-qm", "Initial fixture"]):
        subprocess.run(["git", *args], check=True, env=env, stdout=subprocess.DEVNULL)
    (path / "main.rs").write_text("// Dracula theme snapshot\nfn greet(name: &str) -> String {\n    format!(\"hello {name}! changed\")\n}\n\nfn main() {\n    println!(\"{}\", greet(\"world\"));\n}\n", encoding="utf-8")
    (path / "notes.txt").write_text("Untracked theme fixture file\n", encoding="utf-8")


def capture(out: Path, theme_editor: str, theme_ui: str, linked: bool) -> list[str]:
    if not str(out.resolve()).startswith("/tmp/rriter-themes/"):
        raise ValueError("snapshot output must be under /tmp/rriter-themes/")
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path("/tmp/rriter-themes/fixtures")
    scratch.mkdir(parents=True, exist_ok=True)
    workspace = scratch / "sample-project"
    if not (workspace / ".git").exists() or not (workspace / "main.rs").exists():
        shutil.rmtree(workspace, ignore_errors=True)
        git_fixture(workspace)
    config = {"theme_linked": linked, "editor_theme": theme_editor, "ui_theme": theme_ui}
    shots: list[str] = []

    def session(name: str, use_workspace: bool = True) -> Session:
        return Session(scratch / (out.name + "-" + name + "-profile"), workspace if use_workspace else None, config)

    def take(name: str, setup=None, use_workspace: bool = True) -> Session:
        s = session(name, use_workspace)
        if setup:
            setup(s)
        s.shot(out / name)
        s.close()
        shots.append(name)
        return s

    take("workspace-editor.png")
    s = session("confirmation-dialog", workspace)
    s.click("EditorTextBody")
    s.command("key ctrl+end")
    s.command("type pending edit")
    s.click("EditorTabClose(0)")
    s.shot(out / "confirmation-dialog.png")
    s.close()
    shots.append("confirmation-dialog.png")

    take("git-working-tree.png", lambda p: p.click("SidebarSlot(Git)"))
    s = session("git-graph", workspace)
    s.click("SidebarSlot(Git)")
    s.click("GitGraphToggle")
    for _ in range(80):
        graph = s.dump()
        if any(e.get("id", "").startswith("GitGraphCommit(") for e in graph.get("ui", [])):
            break
        s.command("wait 100")
    s.shot(out / "git-graph.png")
    s.close()
    shots.append("git-graph.png")
    s = session("git-tooltip", workspace)
    s.click("SidebarSlot(Git)")
    s.click("GitGraphToggle")
    for _ in range(80):
        graph = s.dump()
        if any(e.get("id", "").startswith("GitGraphCommit(") for e in graph.get("ui", [])):
            break
        s.command("wait 100")
    s.click("GitGraphCommit(0, 0)")
    s.command("wait 1200")
    s.shot(out / "git-commit-tooltip.png")
    s.close()
    shots.append("git-commit-tooltip.png")

    s = session("project-search", workspace)
    s.command("key ctrl+shift+f")
    s.command("settle 250")
    s.shot(out / "project-search.png")
    s.close()
    shots.append("project-search.png")

    s = session("api-client", workspace)
    s.click("SidebarSlot(ApiClient)")
    s.shot(out / "api-mock.png")
    shots.append("api-mock.png")
    ids = [e["id"] for e in s.dump().get("ui", [])]
    if "ApiImportAdd" in ids:
        s.click("ApiImportAdd")
        s.shot(out / "api-request.png")
        shots.append("api-request.png")
    s.close()

    s = session("database", workspace)
    s.click("SidebarSlot(Database)")
    s.shot(out / "database-connection.png")
    s.close()
    shots.append("database-connection.png")

    settings = session("settings-probe", workspace)
    settings.command("key f1")
    settings.command("settle 350")
    for index in range(16):
        ids = {e.get("id") for e in settings.dump().get("ui", [])}
        if f"SettingsTab({index})" not in ids:
            break
        settings.close()
        settings = session(f"settings-{index}", workspace)
        settings.command("key f1")
        settings.command("settle 350")
        settings.click(f"SettingsTab({index})")
        name = f"settings-tab-{index}.png"
        settings.shot(out / name)
        settings.close()
        shots.append(name)
        settings = session("settings-probe", workspace)
        settings.command("key f1")
        settings.command("settle 350")
    settings.close()

    s = session("problems", workspace)
    s.click("SidebarSlot(Problems)")
    state = s.dump()
    problem_tab = next((e.get("id") for e in state.get("ui", []) if e.get("id", "").startswith("ProblemsTab(")), None)
    if problem_tab:
        s.click(problem_tab)
        s.shot(out / "problems.png")
        shots.append("problems.png")
    s.close()

    s = session("tabs-status", workspace)
    s.shot(out / "tab-strip.png")
    s.close()
    shots.append("tab-strip.png")
    s = session("status", workspace)
    s.shot(out / "status-bar.png")
    s.close()
    shots.append("status-bar.png")
    return shots


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--theme", choices=sorted(THEMES))
    parser.add_argument("--editor", choices=sorted(THEMES))
    parser.add_argument("--ui", choices=sorted(THEMES))
    parser.add_argument("--out", type=Path)
    parser.add_argument("--compare", nargs=2, type=Path, metavar=("DIR_A", "DIR_B"))
    parser.add_argument("--exclude", nargs="*", default=[])
    args = parser.parse_args()
    if args.compare:
        return compare(*args.compare, set(args.exclude))
    editor = args.editor or args.theme or "dracula"
    ui = args.ui or args.theme or "dracula"
    if args.out is None or (args.theme and (args.editor or args.ui)):
        parser.error("capture requires --out and either --theme or --editor/--ui")
    shots = capture(args.out, editor, ui, args.editor is None and args.ui is None)
    print(f"captured {len(shots)} PNGs in {args.out}")
    for name in shots:
        print(name)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"theme_shots: {error}", file=sys.stderr)
        raise SystemExit(2)
