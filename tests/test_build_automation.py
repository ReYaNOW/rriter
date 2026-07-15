#!/usr/bin/env python3
"""Fast unit tests for shared build menus and the cross-platform PGO pipeline."""

from __future__ import annotations

import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import build_common
import build_macos
import build_windows
import pgo_pipeline


class FakeTty(io.StringIO):
    def isatty(self) -> bool:
        return True


class BuildCommonTests(unittest.TestCase):
    def test_no_arguments_open_menu_only_on_real_tty(self) -> None:
        self.assertFalse(
            build_common.should_open_menu([], stdin=io.StringIO(), stdout=io.StringIO())
        )
        self.assertTrue(
            build_common.should_open_menu([], stdin=FakeTty(), stdout=FakeTty())
        )
        self.assertFalse(
            build_common.should_open_menu(
                ["--test"], stdin=FakeTty(), stdout=FakeTty()
            )
        )

    def test_interactive_fresh_pgo_plan_is_deterministic(self) -> None:
        answers = iter(
            [
                "3",  # tests + build
                "1",  # release
                "2",  # fresh PGO
                "y",  # package
                "n",  # no installer
                "n",  # do not run
            ]
        )
        output = FakeTty()
        plan = build_common.interactive_build_plan(
            "Windows",
            supports_installer=True,
            input_fn=lambda _prompt: next(answers),
            output=output,
        )
        self.assertTrue(plan.run_tests)
        self.assertTrue(plan.build)
        self.assertEqual(plan.pgo, build_common.PgoMode.FRESH)
        self.assertEqual(
            plan.phases()[:4],
            (
                "tests",
                "instrumented build",
                "automated GUI training",
                "profile merge",
            ),
        )
        self.assertIn("RRiter build menu", output.getvalue())

    def test_tests_only_plan_rejects_build_side_effects(self) -> None:
        with self.assertRaises(build_common.PlanError):
            build_common.BuildPlan(
                run_tests=True,
                build=False,
                package=True,
            ).validate()


class PlatformArgumentTests(unittest.TestCase):
    def test_windows_legacy_flags_and_new_pgo_flags_parse_together(self) -> None:
        args = build_windows.parse_args(
            ["--test", "--installer", "--pgo", "fresh", "--install-target"]
        )
        self.assertTrue(args.test)
        self.assertTrue(args.installer)
        self.assertEqual(args.pgo, "fresh")
        self.assertTrue(args.install_target)

    def test_macos_legacy_flags_and_new_pgo_flags_parse_together(self) -> None:
        args = build_macos.parse_args(
            [
                "--arch",
                "universal",
                "--test",
                "--pgo",
                "reuse",
                "--no-dmg",
            ]
        )
        self.assertEqual(args.arch, "universal")
        self.assertTrue(args.test)
        self.assertEqual(args.pgo, "reuse")
        self.assertTrue(args.no_dmg)

    def test_tests_only_cli_never_requests_packaging(self) -> None:
        windows_args = build_windows.parse_args(["--tests-only"])
        windows_plan = build_windows.requested_plan(windows_args, ["--tests-only"])
        self.assertFalse(windows_plan.build)
        self.assertFalse(windows_plan.package)

        mac_args = build_macos.parse_args(["--tests-only"])
        mac_plan = build_macos.requested_plan(mac_args, ["--tests-only"])
        self.assertFalse(mac_plan.build)
        self.assertFalse(mac_plan.package)

    def test_windows_tests_only_does_not_prepare_packaging_resources(self) -> None:
        with (
            mock.patch.object(
                build_windows,
                "windows_target_environment",
                return_value={"PATH": "fixture"},
            ),
            mock.patch.object(build_windows, "prepare_resources") as prepare,
            mock.patch.object(build_windows, "run") as run,
        ):
            build_windows.run_windows_tests(
                {},
                target=build_windows.DEFAULT_TARGET,
                install_target=False,
            )
        prepare.assert_not_called()
        self.assertEqual(run.call_args.kwargs["env"], {"PATH": "fixture"})


class PgoPipelineTests(unittest.TestCase):
    def test_target_specific_paths_do_not_touch_normal_cargo_target(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rriter-pgo-path-test-") as directory:
            root = Path(directory)
            config = pgo_pipeline.PgoConfig(
                root=root,
                target="x86_64-unknown-linux-gnu",
            )
            paths = pgo_pipeline.paths_for(config)
            self.assertEqual(
                paths.generate_target_dir,
                root
                / "target"
                / "pgo-generate"
                / "x86_64-unknown-linux-gnu",
            )
            self.assertEqual(
                paths.use_target_dir,
                root / "target" / "pgo-use" / "x86_64-unknown-linux-gnu",
            )
            self.assertEqual(
                paths.merged_profile,
                root
                / "target"
                / "pgo-profiles"
                / "x86_64-unknown-linux-gnu"
                / "merged.profdata",
            )
            normal_release = root / "target" / config.target / "release" / "rriter"
            self.assertNotEqual(
                pgo_pipeline.executable_path(
                    paths.generate_target_dir, config.target, "rriter"
                ),
                normal_release,
            )

    def test_fixture_contains_editor_search_git_terminal_and_api_inputs(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rriter-pgo-fixture-test-") as directory:
            workspace = Path(directory)
            pgo_pipeline._write_fixture_files(workspace)
            large = (workspace / "src" / "large.rs").read_text(encoding="utf-8")
            openapi = json.loads((workspace / "openapi.json").read_text(encoding="utf-8"))
            self.assertIn("generated_5999", large)
            self.assertIn("/automation/featured/{resource_id}", openapi["paths"])
            self.assertIn("/automation/ping", openapi["paths"])
            self.assertTrue((workspace / "src" / "worker.py").is_file())
            self.assertTrue((workspace / "README.md").is_file())

    def test_local_api_server_accepts_authenticated_ping(self) -> None:
        import urllib.request

        server = pgo_pipeline.LocalApiServer.start()
        try:
            request = urllib.request.Request(
                f"{server.base_url}/automation/ping",
                headers={
                    "Authorization": f"Bearer {pgo_pipeline.LOCAL_API_TOKEN}"
                },
            )
            with urllib.request.urlopen(request, timeout=5) as response:
                payload = json.loads(response.read().decode("utf-8"))
        finally:
            server.stop()
        self.assertEqual(payload["marker"], pgo_pipeline.LOCAL_API_MARKER)
        self.assertTrue(payload["accepted"])

    def test_windows_executable_suffix_is_target_driven(self) -> None:
        base = Path("target")
        self.assertEqual(
            pgo_pipeline.executable_path(
                base, "x86_64-pc-windows-msvc", "rriter"
            ).name,
            "rriter.exe",
        )
        self.assertEqual(
            pgo_pipeline.executable_path(
                base, "aarch64-apple-darwin", "rriter"
            ).name,
            "rriter",
        )

    def test_training_environment_isolated_for_all_supported_platform_conventions(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rriter-pgo-env-test-") as directory:
            root = Path(directory)
            config = pgo_pipeline.PgoConfig(
                root=root,
                target="x86_64-unknown-linux-gnu",
            )
            paths = pgo_pipeline.paths_for(config)
            paths.state_dir.mkdir(parents=True)
            environment = pgo_pipeline.isolated_runtime_environment(config, paths)
            for name in (
                "HOME",
                "USERPROFILE",
                "XDG_CONFIG_HOME",
                "XDG_CACHE_HOME",
                "XDG_DATA_HOME",
                "XDG_STATE_HOME",
                "APPDATA",
                "LOCALAPPDATA",
            ):
                self.assertTrue(Path(environment[name]).is_dir(), name)
            self.assertIn("%p", environment["LLVM_PROFILE_FILE"])
            self.assertIn("%m", environment["LLVM_PROFILE_FILE"])


if __name__ == "__main__":
    unittest.main()
