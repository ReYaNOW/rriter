#!/usr/bin/env python3
"""Capture reproducible RRiter UI screens and compare PNG pixels."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import struct
import threading
import subprocess
import sys
import time
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "scripts" / "rriter_headless.py"
SIZE = "1280x720"
SCALE = "1"
THEMES = {"dracula", "one_dark", "forest", "sepia", "one_light"}
# Wiped at the start of every capture. Paths inside it are shown on screen (Settings shows the
# workspace path), so it must stay the same for every run and theme.
FIXTURES = Path("/tmp/rriter-themes/fixtures")
# Status clock, transient caret area and the API fixture URL (its port is ephemeral, the mask
# must cover the whole text line); fixed in framebuffer coordinates.
MASKS = {
    "status-bar.png": [(1170, 690, 1280, 720)],
    "api-response.png": [(58, 460, 278, 500), (370, 568, 580, 592)],
    "api-auth.png": [(58, 460, 278, 500), (58, 570, 278, 592)],
    "db-query.png": [(295, 467, 660, 496)],
}


class Session:
    def __init__(self, profile: Path, workspace: Path | None, config: dict, extra_env: dict | None = None):
        # The app persists open sidebar panels and tabs into the profile (`panels.txt`,
        # `tabs_ide.txt`), and sidebar slots toggle: a reused profile restores e.g. the Git panel
        # open, and the scripted click then closes it. Every session starts from an empty profile.
        if not profile.resolve().is_relative_to(FIXTURES):
            raise ValueError(f"profile must be under {FIXTURES}: {profile}")
        shutil.rmtree(profile, ignore_errors=True)
        profile.mkdir(parents=True)
        config_dir = profile / "config"
        config_dir.mkdir(exist_ok=True)
        (config_dir / "config.json").write_text(json.dumps(config), encoding="utf-8")
        args = [str(DRIVER), "run", "-", "--size", SIZE, "--scale", SCALE, "--profile", str(profile)]
        env = os.environ.copy()
        env["RRITER_BIN"] = str(ROOT / "target/x86_64-unknown-linux-gnu/release/rriter")
        env.update(extra_env or {})
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

    def wait_for_ui(self, wanted: str, timeout: float = 10.0) -> dict:
        deadline = time.monotonic() + timeout
        while True:
            element = next((e for e in self.dump().get("ui", []) if e.get("id") == wanted), None)
            if element:
                return element
            if time.monotonic() >= deadline:
                raise RuntimeError(f"UI element missing: {wanted} (waited {timeout:g}s)")
            self.command("wait 100")

    def click(self, wanted: str) -> dict:
        element = self.wait_for_ui(wanted)
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


class ApiFixtureHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path == "/openapi.json":
            body = json.dumps({
                "openapi": "3.1.0",
                "info": {"title": "Theme API", "version": "1.0"},
                "servers": [{"url": self.server.api_base}],
                "components": {"securitySchemes": {
                    "BearerAuth": {"type": "http", "scheme": "bearer"}
                }},
                "paths": {"/hello": {"get": {
                    "security": [{"BearerAuth": []}],
                    "responses": {"200": {"description": "Fixed response"}},
                }}},
            }).encode()
        elif self.path == "/hello":
            body = b'{"message":"Hello from the theme fixture"}'
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format: str, *_args: object) -> None:
        pass


def click_at(s: Session, x: float, y: float, double: bool = False) -> None:
    s.command(f"mouse_move {x:g} {y:g}")
    s.command("dblclick" if double else "click")
    s.command("settle 250")


def click_id(s: Session, wanted: str, double: bool = False) -> dict:
    element = s.wait_for_ui(wanted)
    x, y, width, height = element["rect"]
    click_at(s, x + width / 2, y + height / 2, double)
    return s.dump()


def reveal(s: Session, wanted: str, point: tuple[float, float], limit: int = 30) -> None:
    for _ in range(limit + 1):
        element = next((e for e in s.dump().get("ui", []) if e.get("id") == wanted), None)
        if element and element["rect"][3] >= 24:
            return
        s.command(f"mouse_move {point[0]:g} {point[1]:g}")
        s.command("wheel 0 -1")
        s.command("settle 500")
    raise RuntimeError(f"UI element did not enter viewport: {wanted}")


def set_database_field(s: Session, field: str, value: str) -> None:
    click_id(s, f"DatabaseDialogField({field})")
    s.command("key ctrl+a")
    s.command("type " + value)


def capture_api_screens(scratch: Path, out: Path, config: dict) -> list[str]:
    server = ThreadingHTTPServer(("127.0.0.1", 0), ApiFixtureHandler)
    server.api_base = f"http://127.0.0.1:{server.server_port}"
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    shots: list[str] = []
    try:
        s = Session(scratch / (out.name + "-api-response-profile"), scratch / "sample-project", config)
        s.click("SidebarSlot(ApiClient)")
        s.click("ApiImportAdd")
        s.click("ApiImportUrl")
        s.click("ApiImportUrlInput")
        s.command("type " + server.api_base + "/openapi.json")
        s.click("ApiImportUrlConfirm")
        for _ in range(100):
            state = s.dump()
            if any(e.get("id") == "ApiRouteRow(0)" for e in state.get("ui", [])):
                break
            s.command("wait 100")
        reveal(s, "ApiRouteRow(0)", (200, 500))
        click_id(s, "ApiRouteRow(0)")
        reveal(s, "ApiTryRequest", (800, 400))
        click_id(s, "ApiTryRequest")
        for _ in range(150):
            s.command("wait 100")
            if any(e.get("id") == "ApiResponseBody(0)" for e in s.dump().get("ui", [])):
                break
        reveal(s, "ApiResponseBody(0)", (800, 400))
        s.shot(out / "api-response.png")
        s.close()
        shots.append("api-response.png")

        s = Session(scratch / (out.name + "-api-auth-profile"), scratch / "sample-project", config)
        s.click("SidebarSlot(ApiClient)")
        s.click("ApiImportAdd")
        s.click("ApiImportUrl")
        s.click("ApiImportUrlInput")
        s.command("type " + server.api_base + "/openapi.json")
        s.click("ApiImportUrlConfirm")
        for _ in range(100):
            if any(e.get("id") == "ApiAuthRoot" for e in s.dump().get("ui", [])):
                break
            s.command("wait 100")
        reveal(s, "ApiAuthRoot", (200, 500))
        click_id(s, "ApiAuthRoot")
        s.shot(out / "api-auth.png")
        s.close()
        shots.append("api-auth.png")
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
    return shots


def start_database_fixture() -> tuple[subprocess.Popen, int]:
    process = subprocess.Popen(
        [sys.executable, str(ROOT / "scripts" / "postgres_fixture.py"), "--port", "0"],
        cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, encoding="utf-8",
    )
    assert process.stdout is not None
    port_line = process.stdout.readline().strip()
    if not port_line.isdigit():
        process.kill()
        raise RuntimeError(f"PostgreSQL fixture failed to start: {port_line}")
    return process, int(port_line)


def capture_database_screens(scratch: Path, out: Path, config: dict) -> list[str]:
    fixture, port = start_database_fixture()
    shots: list[str] = []
    try:
        s = Session(scratch / (out.name + "-database-table-profile"), scratch / "sample-project", config)
        s.click("SidebarSlot(Database)")
        s.click("DatabaseAdd")
        for field, value in (
            ("DisplayName", "Theme database"), ("Host", "127.0.0.1"), ("Port", str(port)),
            ("Username", "rriter_pgo"), ("PostgresPassword", "fixture"),
            ("MaintenanceDatabase", "rriter_pgo"),
        ):
            set_database_field(s, field, value)
        s.click("DatabaseDialogSave")
        for _ in range(50):
            state = s.dump()
            if any(e.get("id") == "DatabaseConnectionArrow(0)" for e in state.get("ui", [])):
                break
            s.command("wait 100")
        s.click("DatabaseConnectionArrow(0)")
        for _ in range(100):
            state = s.dump()
            if any(e.get("id") == "DatabaseArrow(0, 0)" for e in state.get("ui", [])):
                break
            s.command("wait 100")
        s.click("DatabaseArrow(0, 0)")
        for _ in range(100):
            state = s.dump()
            if any(e.get("id", "").startswith("DatabaseTableRow(0, 0,") for e in state.get("ui", [])):
                break
            s.command("wait 100")
        click_id(s, "DatabaseTableRow(0, 0, 0)", double=True)
        # The tab opens before its rows and row count arrive; wait for the first cell and for the
        # panel's pending job to finish (DatabaseRefresh is only registered while none is pending).
        s.wait_for_ui("DatabaseTableCell(0, 0)", timeout=15)
        s.wait_for_ui("DatabaseRefresh", timeout=15)
        s.command("settle 500")
        s.shot(out / "db-table.png")
        s.close()
        shots.append("db-table.png")

        s = Session(scratch / (out.name + "-database-query-profile"), scratch / "sample-project", config)
        s.click("SidebarSlot(Database)")
        s.click("DatabaseAdd")
        for field, value in (
            ("DisplayName", "Theme database"), ("Host", "127.0.0.1"), ("Port", str(port)),
            ("Username", "rriter_pgo"), ("PostgresPassword", "fixture"),
            ("MaintenanceDatabase", "rriter_pgo"),
        ):
            set_database_field(s, field, value)
        s.click("DatabaseDialogSave")
        for _ in range(50):
            if any(e.get("id") == "DatabaseConnectionArrow(0)" for e in s.dump().get("ui", [])):
                break
            s.command("wait 100")
        s.click("DatabaseConnectionArrow(0)")
        for _ in range(100):
            if any(e.get("id") == "DatabaseRow(0, 0)" for e in s.dump().get("ui", [])):
                break
            s.command("wait 100")
        state = s.dump()
        row = next(e for e in state["ui"] if e.get("id") == "DatabaseRow(0, 0)")
        x, y, width, height = row["rect"]
        s.command(f"mouse_move {x + width / 2:g} {y + height / 2:g}")
        s.command("click right")
        for _ in range(30):
            if any(e.get("id") == "DatabaseContextItem(0)" for e in s.dump().get("ui", [])):
                break
            s.command("wait 100")
        s.click("DatabaseContextItem(0)")
        for _ in range(100):
            state = s.dump()
            if any(e.get("id") == "DatabaseQueryRun" for e in state.get("ui", [])):
                break
            s.command("wait 100")
        body = next(e for e in s.dump()["ui"] if e.get("id") == "EditorTextBody")
        x, y, width, height = body["rect"]
        click_at(s, x + width / 2, y + height / 2)
        s.command("key ctrl+a")
        s.command("type SELECT id, name, active FROM public.pgo_items ORDER BY id LIMIT 3;")
        s.click("DatabaseQueryRun")
        for _ in range(100):
            if any(e.get("id") == "DatabaseQueryResultBody" for e in s.dump().get("ui", [])):
                break
            s.command("wait 100")
        s.shot(out / "db-query.png")
        s.close()
        shots.append("db-query.png")
    finally:
        if fixture.stdin:
            fixture.stdin.close()
        try:
            fixture.wait(timeout=5)
        except subprocess.TimeoutExpired:
            fixture.kill()
            fixture.wait()
    return shots


def capture_hover(scratch: Path, out: Path, config: dict) -> str:
    root = scratch / "rust-hover"
    root.mkdir(parents=True, exist_ok=True)
    (root / "Cargo.toml").write_text(
        '[package]\nname = "theme_hover"\nversion = "0.1.0"\nedition = "2021"\n'
        '[lib]\npath = "main.rs"\n', encoding="utf-8"
    )
    source = root / "main.rs"
    source.write_text("fn main() {\n    hover_subject();\n}\n", encoding="utf-8")
    fake = scratch / "rust-analyzer"
    shutil.copyfile(ROOT / "scripts" / "fake_lsp_server.py", fake)
    fake.chmod(0o755)
    s = Session(scratch / (out.name + "-hover-profile"), root, config, extra_env={"RRITER_RUST_ANALYZER_PATH": str(fake)})
    for _ in range(100):
        if s.dump().get("tabs"):
            break
        s.command("wait 100")
    state = s.dump()
    body = next(e for e in state["ui"] if e.get("id") == "EditorTextBody")
    x, y, _width, _height = body["rect"]
    s.command(f"mouse_move {x + 5 * 8:g} {y + 1 * 18:g}")
    s.command("wait 1000")
    s.shot(out / "hover.png")
    s.close()
    return "hover.png"


def capture(out: Path, theme_editor: str, theme_ui: str, linked: bool) -> list[str]:
    if not str(out.resolve()).startswith("/tmp/rriter-themes/"):
        raise ValueError("snapshot output must be under /tmp/rriter-themes/")
    out.mkdir(parents=True, exist_ok=True)
    # Fresh fixtures every run, at fixed paths: the Git screens show the working tree state.
    scratch = FIXTURES
    shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir(parents=True)
    workspace = scratch / "sample-project"
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

    shots.extend(capture_api_screens(scratch, out, config))
    shots.extend(capture_database_screens(scratch, out, config))
    shots.append(capture_hover(scratch, out, config))

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
