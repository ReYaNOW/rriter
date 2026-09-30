#!/usr/bin/env python3
"""Compare headless startup time of rriter and ralk; prints a Markdown table.

Both binaries print `startup: <stage> +<ms>ms` lines to stderr when tracing is
enabled; `first-frame` is the finish line. The script never builds anything.

    python3 scripts/bench_startup.py
    python3 scripts/bench_startup.py --workspace ~/projects/rriter --runs 20 --json out.json
"""

from __future__ import annotations

import argparse
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional, Sequence

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_RRITER = REPO_ROOT / "target" / "x86_64-unknown-linux-gnu" / "release" / "rriter"
DEFAULT_RALK = Path("/home/reyan/projects/rust_kalk/target/x86_64-unknown-linux-gnu/release/ralk")
STAGE_RE = re.compile(r"^startup:\s+(\S+)\s+\+([0-9]+(?:\.[0-9]+)?)ms\s*$", re.MULTILINE)
FINISH = "first-frame"
TIMEOUT_S = 60


def parse_cli(argv: Sequence[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(prog="bench_startup.py", description=__doc__.splitlines()[0])
    p.add_argument("--rriter", default=os.environ.get("RRITER_BIN", str(DEFAULT_RRITER)), help="rriter binary")
    p.add_argument("--ralk", default=os.environ.get("RALK_BIN", str(DEFAULT_RALK)), help="ralk binary")
    p.add_argument("--runs", type=int, default=15, help="measured runs per scenario (default 15)")
    p.add_argument("--warmup", type=int, default=3, help="discarded warmup runs (default 3)")
    p.add_argument("--profile", default="/tmp/rriter-bench-profile", help="rriter profile dir")
    p.add_argument("--workspace", help="directory for the rriter workspace scenario")
    p.add_argument("--json", metavar="FILE", help="dump raw numbers to FILE")
    return p.parse_args(argv)


def run_once(cmd: list[str], env: dict[str, str], stdin: Optional[str]) -> Optional[dict]:
    """One run; None when it failed (non-zero exit, timeout, or no first-frame line)."""
    t0 = time.perf_counter()
    try:
        proc = subprocess.run(
            cmd, env=env, input=stdin, capture_output=True, text=True, timeout=TIMEOUT_S
        )
    except subprocess.TimeoutExpired:
        return None
    wall = (time.perf_counter() - t0) * 1000.0
    if proc.returncode != 0:
        return None
    stages = {name: float(ms) for name, ms in STAGE_RE.findall(proc.stderr)}
    if FINISH not in stages:
        return None
    return {"wall": wall, "first_frame": stages[FINISH], "stages": stages}


def measure(cmd: list[str], env: dict[str, str], stdin: Optional[str], runs: int, warmup: int) -> dict:
    for _ in range(warmup):
        run_once(cmd, env, stdin)
    ok, fail = [], 0
    for _ in range(runs):
        r = run_once(cmd, env, stdin)
        if r is None:
            fail += 1
        else:
            ok.append(r)
    return {"cmd": cmd, "ok": ok, "fail": fail}


def summarize(res: dict) -> Optional[dict]:
    ok = res["ok"]
    if not ok:
        return None
    walls = [r["wall"] for r in ok]
    ffs = [r["first_frame"] for r in ok]
    by_ff = sorted(ok, key=lambda r: r["first_frame"])
    med_run = by_ff[len(by_ff) // 2]
    return {
        "wall_med": statistics.median(walls),
        "wall_min": min(walls),
        "ff_med": statistics.median(ffs),
        "ff_min": min(ffs),
        "stages": med_run["stages"],
    }


def main(argv: Sequence[str]) -> int:
    args = parse_cli(argv)
    Path(args.profile).mkdir(parents=True, exist_ok=True)

    rriter_env = dict(os.environ, RRITER_STARTUP_TRACE="1")
    rriter_base = [args.rriter, "--headless", "--profile", args.profile]
    scenarios = [
        ("ralk headless", args.ralk, [args.ralk, "--headless"], dict(os.environ), None),
        ("rriter welcome", args.rriter, rriter_base, rriter_env, "quit\n"),
    ]
    if args.workspace:
        scenarios.append(
            (f"rriter workspace {args.workspace}", args.rriter, rriter_base + [args.workspace], rriter_env, "quit\n")
        )

    skipped = False
    rows: list[tuple[str, int, Optional[dict]]] = []
    raw: dict[str, dict] = {}
    for name, binary, cmd, env, stdin in scenarios:
        if not Path(binary).is_file():
            print(f"missing: {binary}")
            skipped = True
            continue
        res = measure(cmd, env, stdin, args.runs, args.warmup)
        summ = summarize(res)
        rows.append((name, res["fail"], summ))
        raw[name] = {"cmd": cmd, "fail": res["fail"], "runs": res["ok"], "summary": summ}

    if rows:
        print("| scenario | wall med ms | wall min ms | first-frame med ms | first-frame min ms | fail |")
        print("|---|---:|---:|---:|---:|---:|")
        for name, fail, s in rows:
            if s is None:
                print(f"| {name} | - | - | - | - | fail={fail} |")
                continue
            print(
                f"| {name} | {s['wall_med']:.1f} | {s['wall_min']:.1f} | "
                f"{s['ff_med']:.1f} | {s['ff_min']:.1f} | fail={fail} |"
            )
            if name.startswith("rriter"):
                pairs = " ".join(f"{k}={v:.1f}" for k, v in s["stages"].items())
                print(f"|  | stages (median run): {pairs} | | | | |")

    by_name = {n: s for n, _, s in rows if s}
    ralk, welcome = by_name.get("ralk headless"), by_name.get("rriter welcome")
    ratios = {}
    if ralk and welcome:
        ratios = {
            "first_frame": welcome["ff_med"] / ralk["ff_med"],
            "wall": welcome["wall_med"] / ralk["wall_med"],
        }
        print()
        print(
            f"rriter welcome / ralk: first-frame x{ratios['first_frame']:.2f}, wall x{ratios['wall']:.2f} (medians)"
        )

    if args.json:
        Path(args.json).write_text(json.dumps({"scenarios": raw, "ratios": ratios}, indent=2))

    return 1 if skipped else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
