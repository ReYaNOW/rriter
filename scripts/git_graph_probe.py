#!/usr/bin/env python3
import argparse
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/x86_64-unknown-linux-gnu/release/rriter"
BUDGET_KB = 10 * 1024


def run(cmd):
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True)


def parse_max_delta(report):
    for line in report.splitlines():
        if line.startswith("max_graph_delta_kb="):
            try:
                return int(line.split("=", 1)[1])
            except ValueError:
                return None
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("repo", nargs="?", default="/home/reyan/repos/git")
    parser.add_argument("iterations", nargs="?", type=int, default=5)
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()

    if not args.skip_build:
        build = run(["make", "fast"])
        sys.stdout.write(build.stdout)
        sys.stderr.write(build.stderr)
        if build.returncode != 0:
            return build.returncode

    probe = run([str(BIN), "--probe-git-graph", args.repo, str(args.iterations)])
    sys.stdout.write(probe.stdout)
    sys.stderr.write(probe.stderr)
    if probe.returncode != 0:
        return probe.returncode

    max_delta = parse_max_delta(probe.stdout)
    if max_delta is None:
        print("probe failed: missing max_graph_delta_kb", file=sys.stderr)
        return 2
    if max_delta > BUDGET_KB:
        print(
            f"FAIL graph_delta_kb={max_delta} budget_kb={BUDGET_KB}",
            file=sys.stderr,
        )
        return 1

    print(f"OK graph_delta_kb={max_delta} budget_kb={BUDGET_KB}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
