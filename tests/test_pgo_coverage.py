#!/usr/bin/env python3
"""Unit tests for PGO profile coverage, group markers and the headless pipeline logic."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import pgo_coverage
import pgo_fixtures
import pgo_pipeline
import postgres_fixture
from pgo_coverage import FunctionProfile

# Shape of `llvm-profdata show --all-functions --counts` for an IR profile (LLVM 22): no
# `Function count:` line, only `Block counts:`; internal-linkage names carry a `<cgu>;` prefix.
SHOW_FRAGMENT = """\
Counters:
  _ZN6rriter3app8git_diff15build_diff_view17h0123456789abcdefE:
    Hash: 0x0a2b1c3d4e5f6071
    Counters: 3
    Block counts: [12, 7, 0]
  _ZN6rriter3pdf6worker17search_one_page17hfedcba9876543210E:
    Hash: 0x1111222233334444
    Counters: 2
    Block counts: [0, 0]
  main:
    Hash: 0x0000000000000001
    Counters: 1
    Block counts: [1]
Instrumentation level: IR  entry_first = 0
Functions shown: 3
Total functions: 3
Maximum function count: 12
Maximum internal block count: 7
"""


class ProfdataParsingTests(unittest.TestCase):
    def test_parse_profdata_show_reads_names_and_counts(self) -> None:
        functions = pgo_coverage.parse_profdata_show(SHOW_FRAGMENT)
        self.assertEqual([f.name for f in functions][-1], "main")
        self.assertEqual(len(functions), 3)
        by_name = {f.name: f.count for f in functions}
        self.assertEqual(by_name["_ZN6rriter3app8git_diff15build_diff_view17h0123456789abcdefE"], 12)
        self.assertEqual(by_name["_ZN6rriter3pdf6worker17search_one_page17hfedcba9876543210E"], 0)

    def test_block_counts_alone_mark_a_function_as_executed(self) -> None:
        text = "  f:\n    Hash: 0x1\n    Counters: 2\n    Block counts: [0, 4]\n"
        self.assertEqual(pgo_coverage.parse_profdata_show(text), [FunctionProfile("f", 4)])

    def test_older_output_with_a_function_count_line_is_still_read(self) -> None:
        text = "  f:\n    Function count: 6\n    Block counts: [0, 4]\n"
        self.assertEqual(pgo_coverage.parse_profdata_show(text), [FunctionProfile("f", 6)])

    def test_module_of_ignores_the_internal_linkage_cgu_prefix(self) -> None:
        self.assertEqual(
            pgo_coverage.module_of("rriter.abc-cgu.03;rriter::app::git_diff::build_diff_view"),
            "rriter::app::git_diff",
        )
        self.assertEqual(
            pgo_coverage.module_of("rriter.abc-cgu.03;<rriter::pdf::worker::Worker>::run"),
            "rriter::pdf::worker",
        )
        self.assertEqual(pgo_coverage.module_of("rriter.abc-cgu.03;_ZN4fake"), pgo_coverage.OTHER_MODULE)

    def test_demangled_functions_strips_the_cgu_prefix_before_demangling(self) -> None:
        show = "  rriter.abc-cgu.03;_RNvCs1_6rriter4work:\n    Hash: 0x1\n    Block counts: [5]\n"
        seen: list[list[str]] = []

        def fake_demangle(names, cxxfilt):
            seen.append(list(names))
            return ["rriter::work"]

        with mock.patch.object(pgo_coverage, "demangle", fake_demangle):
            functions = pgo_coverage.demangled_functions(show, "llvm-cxxfilt")
        self.assertEqual(seen, [["_RNvCs1_6rriter4work"]])
        self.assertEqual(functions, [FunctionProfile("rriter::work", 5)])

    def test_garbage_output_yields_no_functions_and_never_raises(self) -> None:
        for text in ("", "\x00\x01 garbage\n::::\n", "error: no profile\n", "Counters:\n  :\n  \n"):
            self.assertEqual(pgo_coverage.parse_profdata_show(text), [], text)

    def test_module_of_handles_paths_impls_closures_generics_and_garbage(self) -> None:
        cases = {
            "rriter::app::git_diff::build_diff_view": "rriter::app::git_diff",
            "rriter::app::git_diff::build_diff_view::h0123456789abcdef": "rriter::app::git_diff",
            "<rriter::pdf::worker::Worker>::run": "rriter::pdf::worker",
            "<rriter::app::App as core::fmt::Debug>::fmt": "rriter::app",
            "rriter::app::f::{closure#0}": "rriter::app",
            "rriter::app::f::{{closure}}::h0123456789abcdef": "rriter::app",
            "core::ptr::drop_in_place<rriter::app::App>": "core::ptr",
            "<alloc::vec::Vec<u8> as core::clone::Clone>::clone": "alloc::vec",
            "rriter::app::terminal_process::TerminalProcess::write_input": "rriter::app::terminal_process",
            "_RNvCs_garbage": pgo_coverage.OTHER_MODULE,
            "<(u8, u8) as Foo>::bar": pgo_coverage.OTHER_MODULE,
            "main": pgo_coverage.OTHER_MODULE,
            "": pgo_coverage.OTHER_MODULE,
            "std::io::stdio::print": "std::io::stdio",
        }
        for name, expected in cases.items():
            self.assertEqual(pgo_coverage.module_of(name), expected, name)

    def test_module_summary_counts_hit_and_total_per_module(self) -> None:
        functions = [
            FunctionProfile("rriter::a::x", 3),
            FunctionProfile("rriter::a::y", 0),
            FunctionProfile("rriter::b::z", 0),
            FunctionProfile("_Rgarbage", 5),
        ]
        summary = pgo_coverage.module_summary(functions)
        self.assertEqual(summary["rriter::a"], (1, 2))
        self.assertEqual(summary["rriter::b"], (0, 1))
        self.assertEqual(summary[pgo_coverage.OTHER_MODULE], (1, 1))
        self.assertEqual(pgo_coverage.zero_modules(summary), ["rriter::b"])
        table = pgo_coverage.format_coverage(summary)
        self.assertIn("rriter::a | 1 | 2", table)

    def test_demangle_falls_back_to_the_raw_name_on_count_mismatch(self) -> None:
        completed = mock.Mock(returncode=0, stdout="only one line\n")
        with mock.patch.object(pgo_coverage.subprocess, "run", return_value=completed):
            names = pgo_coverage.demangle(["a", "b"], "/usr/bin/llvm-cxxfilt")
        self.assertEqual(names, ["a", "b"])


class MarkerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.markers = {"pdf": ["search_one_page"], "api_mock": ["run_server_thread"]}
        self.functions = [
            FunctionProfile("rriter::pdf::worker::search_one_page", 4),
            FunctionProfile("rriter::app::api_mock::server::run_server_thread", 2),
        ]

    def test_all_markers_found_is_clean(self) -> None:
        self.assertEqual(pgo_coverage.check_markers(self.functions, self.markers, set()), [])

    def test_marker_with_zero_count_is_reported_with_its_group(self) -> None:
        functions = [FunctionProfile("rriter::pdf::worker::search_one_page", 0), self.functions[1]]
        problems = pgo_coverage.check_markers(functions, self.markers, set())
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].startswith("pdf: search_one_page"))

    def test_absent_marker_is_reported(self) -> None:
        problems = pgo_coverage.check_markers(self.functions[:1], self.markers, set())
        self.assertEqual(len(problems), 1)
        self.assertTrue(problems[0].startswith("api_mock: run_server_thread"))

    def test_skipped_group_is_not_required(self) -> None:
        self.assertEqual(pgo_coverage.check_markers([], self.markers, {"pdf", "api_mock"}), [])

    def test_markers_match_v0_demangled_inherent_methods(self) -> None:
        # Real `llvm-cxxfilt` output of the `make max` profile: `<Type>::method`.
        functions = [FunctionProfile("<rriter::app::terminal::Terminal>::write_input", 4)]
        markers = {"terminal": ["terminal::Terminal::write_input"]}
        self.assertEqual(pgo_coverage.check_markers(functions, markers, set()), [])
        functions = [FunctionProfile("<rriter::app::terminal::Terminal>::write_input", 0)]
        self.assertEqual(len(pgo_coverage.check_markers(functions, markers, set())), 1)

    def test_shipped_markers_match_real_profile_names(self) -> None:
        # (demangled name, count) pairs copied from `llvm-profdata show --all-functions --counts`
        # of a full `make max` training run, demangled by `demangled_functions`.
        real = [
            ("rriter::pdf::worker::handle_request", 218),
            ("rriter::pdf::pdfium_backend::render", 11300544),
            ("<rriter::app::app_state::App>::jump_active_git_diff_hunk", 48),
            ("rriter::app::automation_git_changes::rollback_visible_hunk", 44),
            ("<rriter::editor::Editor>::toggle_extra_cursor", 5),
            ("<rriter::editor::Editor>::undo", 9),
            ("rriter::lsp::protocol::definition_position", 3),
            ("rriter::lsp::protocol::parse_definition_target", 6),
            ("rriter::scroll::scrollbar_drag_target", 40),
            ("<rriter::editor::Editor>::move_page_down", 20),
            ("<rriter::app::app_state::App>::close_terminal_tab_at", 1),
            ("<rriter::app::app_state::App>::update_terminal_search", 570171),
            ("rriter::state_persistence::load_open_tabs", 4616),
            (
                "<rriter::app::automation_groups::welcome_steps::{closure#0} as "
                "core::ops::function::FnOnce<(&rriter::app::app_state::App,)>>::call_once",
                1,
            ),
            ("<rriter::app::markdown::MarkdownTabState>::refresh_read_model", 1916),
            ("rriter::app::markdown::scroll_markdown_read", 1196),
            ("<rriter::app::app_state::App>::finish_database_table_transaction", 1),
            ("<reqwest::blocking::request::RequestBuilder>::send", 1),
            ("<rriter::app::app_state::App>::load_git_graph_for_selected_workspace", 82),
            ("<rriter::app::app_state::App>::save_current_config", 56),
            ("<rriter::app::terminal::Terminal>::write_input", 4),
            ("rriter::app::api_client::spawn_api_request", 156),
            (
                "std::sys::backtrace::__rust_begin_short_backtrace::<rriter::app::terminal_process::"
                "install_terminal_io_threads::{closure#2}, ()>",
                2,
            ),
            ("rriter::app::project_search::start_project_search_worker_cancellable", 1),
            ("rriter::app::project_search::run_project_search_roots::{closure#0}", 944),
            ("rriter::app::api_mock::server::run_server_thread::{closure#0}", 36),
            (
                "std::sys::backtrace::__rust_begin_short_backtrace::<rriter::app::api_mock::server::"
                "request_to_mock::{closure#1}, ()>",
                750,
            ),
            (
                "core::ptr::drop_in_place::<rriter::app::database::database_query::"
                "execute_simple_query::{closure#0}>",
                2,
            ),
            ("rriter::app::git_panel::apply_git_graph_lanes", 12000),
        ]
        functions = [FunctionProfile(name, count) for name, count in real]
        self.assertEqual(pgo_coverage.check_markers(functions, pgo_coverage.GROUP_MARKERS, set()), [])

    def test_inlined_functions_are_not_markers(self) -> None:
        # None of these has a profile entry in a real `make max` profile (inlined).
        shipped = {marker for markers in pgo_coverage.GROUP_MARKERS.values() for marker in markers}
        for gone in (
            "search_one_page", "rollback_hunk_text", "rollback_active_git_diff_hunk",
            "apply_at_all_cursors_indexed", "undo_with_groups", "jump_to_definition_target",
            "apply_scrollbar_drag_target", "draw_welcome", "get_welcome_buttons",
            "rollback_database_table_transaction", "ensure_git_graph_loaded",
            "TerminalProcess::write_input", "stream_project_search",
        ):
            self.assertFalse(any(gone in marker for marker in shipped), gone)

    def test_real_cgu_prefixed_v0_names_demangle_before_matching(self) -> None:
        raw = "rriter.5b9d55fe1293a1bc-cgu.0;_RNvMs1_NtNtCs7RF3ZZFCWGC_6rriter3app8terminalNtB5_8Terminal11write_input"
        self.assertEqual(pgo_coverage.strip_cgu_prefix(raw), raw.split(";", 1)[1])

    def test_startup_and_project_search_markers_name_production_functions(self) -> None:
        self.assertEqual(pgo_coverage.GROUP_MARKERS["startup"], ["state_persistence::load_open_tabs"])
        self.assertNotIn("run_project_search", pgo_coverage.GROUP_MARKERS["project_search"])
        self.assertIn("run_project_search_roots", pgo_coverage.GROUP_MARKERS["project_search"])

    def test_shipped_markers_cover_every_full_group_and_both_extra_scenarios(self) -> None:
        for group in (
            "pdf", "api_mock", "git_changes", "editor_ops", "lsp_nav", "input_scroll",
            "terminal_ops", "startup", "welcome",
        ):
            self.assertTrue(pgo_coverage.GROUP_MARKERS.get(group), group)
            self.assertLessEqual(len(pgo_coverage.GROUP_MARKERS[group]), 3, group)

    def test_required_groups_follow_the_scenarios_that_ran(self) -> None:
        all_keys = set(pgo_coverage.GROUP_MARKERS)
        self.assertEqual(pgo_coverage.unrequired_groups(("full", "startup", "welcome")), set())
        self.assertEqual(
            pgo_coverage.unrequired_groups(("full",)), {"startup", "welcome"}
        )
        self.assertEqual(
            pgo_coverage.unrequired_groups(("welcome",)), all_keys - {"welcome"}
        )
        self.assertEqual(pgo_coverage.unrequired_groups(("group:pdf",)), all_keys - {"pdf"})


class WarningTests(unittest.TestCase):
    LOG = (
        "warning: no profile data available for function _ZN6rriter11render_view4draw17h0123456789abcdefE of module x\n"
        "warning: no profile data available for function _ZN6rriter5other4cold17h0123456789abcdefE of module x\n"
        "warning: Function control flow change detected (hash mismatch) _ZN6rriter3app5mouse7handler17h0123456789abcdefE\n"
        "warning: no profile data available for function rriter::editor::insert_str of module y\n"
        "   Compiling something v1.0\n"
    )

    def test_counts_and_hot_functions(self) -> None:
        warnings = pgo_coverage.count_pgo_warnings(self.LOG)
        self.assertEqual(warnings.missing, 3)
        self.assertEqual(warnings.mismatch, 1)
        joined = "\n".join(warnings.hot)
        self.assertIn("render_view", joined)
        self.assertIn("mouse", joined)
        self.assertIn("editor", joined)
        self.assertNotIn("cold", joined)

    def test_v0_mangled_and_demangled_names_count_as_hot(self) -> None:
        log = (
            "warning: no profile data available for function _RNvNtCs1a2B_6rriter11render_view4draw of module x\n"
            "warning: no profile data available for function _RNvNtNtCs1a2B_6rriter3app5mouse7handlerB4_ of module x\n"
            "warning: no profile data available for function _RNvMs_NtCs1a2B_6rriter6editorNtB4_6Editor6insert of module x\n"
            "warning: no profile data available for function _RNvNtCs1a2B_6rriter5other4cold of module x\n"
            "warning: no profile data available for function <rriter::render_view::Frame>::paint of module x\n"
        )
        warnings = pgo_coverage.count_pgo_warnings(log)
        self.assertEqual(warnings.missing, 5)
        self.assertEqual(len(warnings.hot), 4)
        self.assertNotIn("_RNvNtCs1a2B_6rriter5other4cold", warnings.hot)
        self.assertEqual(
            pgo_coverage._legacy_path("_RNvNtCs1a2B_6rriter11render_view4draw"), "rriter::render_view::draw"
        )

    def test_empty_and_garbage_logs(self) -> None:
        for text in ("", "nothing relevant\n\x00"):
            warnings = pgo_coverage.count_pgo_warnings(text)
            self.assertEqual((warnings.missing, warnings.mismatch, warnings.hot), (0, 0, []))


class PreflightTests(unittest.TestCase):
    def test_pdfium_path_file_missing_gives_make_hint(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(pgo_pipeline.PgoError, "make pdfium"):
                pgo_fixtures.find_pdfium(Path(directory))

    def test_pdfium_path_to_a_missing_library_gives_make_hint(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "target").mkdir()
            (root / "target" / "pdfium.path").write_text(f"{root}/nope.so\n", encoding="utf-8")
            with self.assertRaisesRegex(pgo_pipeline.PgoError, "make pdfium"):
                pgo_fixtures.find_pdfium(root)
            (root / "target" / "pdfium.path").write_text("\n", encoding="utf-8")
            with self.assertRaisesRegex(pgo_pipeline.PgoError, "make pdfium"):
                pgo_fixtures.find_pdfium(root)

    def test_pdfium_path_to_an_existing_library_is_returned(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            library = root / "libpdfium.so"
            library.write_bytes(b"x")
            (root / "target").mkdir()
            (root / "target" / "pdfium.path").write_text(f"{library}\n", encoding="utf-8")
            self.assertEqual(pgo_fixtures.find_pdfium(root), library)

    def test_missing_llvm_cxxfilt_is_an_error_with_a_hint(self) -> None:
        with mock.patch.object(pgo_coverage.shutil, "which", return_value=None):
            with self.assertRaisesRegex(pgo_pipeline.PgoError, "llvm-cxxfilt"):
                pgo_coverage.find_cxxfilt()
        with mock.patch.object(pgo_coverage.shutil, "which", return_value="/usr/bin/llvm-cxxfilt"):
            self.assertEqual(pgo_coverage.find_cxxfilt(), "/usr/bin/llvm-cxxfilt")

    def test_managed_ty_directory_is_found_only_when_ty_is_not_on_path(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory)
            bin_dir = data / "RRiter" / "tools" / "managed" / "ty" / "gen1" / "bin"
            bin_dir.mkdir(parents=True)
            (bin_dir / "ty").write_text("#!/bin/sh\n", encoding="utf-8")
            environment = {"XDG_DATA_HOME": str(data), "PATH": "/nonexistent"}
            self.assertEqual(pgo_fixtures.managed_ty_bin_dir(environment), bin_dir)
            self.assertEqual(pgo_fixtures.managed_ty_bin_dir({"XDG_DATA_HOME": str(data / "x")}), None)
            tools = data / "tools"
            tools.mkdir()
            (tools / "ty").write_text("#!/bin/sh\n", encoding="utf-8")
            (tools / "ty").chmod(0o755)
            self.assertIsNone(
                pgo_fixtures.managed_ty_bin_dir({"XDG_DATA_HOME": str(data), "PATH": str(tools)})
            )


def linux_config(root: Path, **overrides: object) -> pgo_pipeline.PgoConfig:
    return pgo_pipeline.PgoConfig(
        root=root, target="x86_64-unknown-linux-gnu", timeout_seconds=30, **overrides
    )


class TrainingCommandTests(unittest.TestCase):
    def test_full_and_welcome_use_headless_argv_with_their_own_profile_dirs_on_linux(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config = linux_config(Path(directory))
            paths = pgo_pipeline.paths_for(config)
            exe = Path("/bin/rriter")
            full = pgo_pipeline.training_command(
                config, paths, exe, "full", paths.report_path, host="linux"
            )
            startup = pgo_pipeline.training_command(
                config, paths, exe, "startup", Path("/r/startup.json"), host="linux"
            )
            welcome = pgo_pipeline.training_command(
                config, paths, exe, "welcome", Path("/r/welcome.json"), host="linux"
            )
        strings = [[str(part) for part in command] for command in (full, startup, welcome)]
        for argv, name in zip(strings, ("full", "startup", "welcome")):
            self.assertEqual(argv[1:3], ["--headless", "--pgo-train"])
            self.assertEqual(argv[argv.index("--pgo-scenario") + 1], name)
            self.assertEqual(argv[argv.index("--pgo-timeout-seconds") + 1], "30")
            self.assertNotIn("--ide", argv)
        profile = lambda argv: Path(argv[argv.index("--profile") + 1])
        self.assertEqual(profile(strings[0]), profile(strings[1]))
        self.assertEqual(profile(strings[0]).name, "headless-profile")
        self.assertEqual(profile(strings[2]).name, "headless-profile-welcome")
        self.assertEqual(profile(strings[0]).parent, paths.state_dir)

    def test_non_linux_hosts_keep_the_gui_argv(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config = pgo_pipeline.PgoConfig(root=Path(directory), target="x86_64-pc-windows-msvc")
            paths = pgo_pipeline.paths_for(config)
            command = pgo_pipeline.training_command(
                config, paths, Path("rriter.exe"), "full", paths.report_path, host="win32"
            )
            argv = [str(part) for part in command]
            self.assertEqual(argv[1:3], ["--ide", "--pgo-train"])
            self.assertNotIn("--headless", argv)
            self.assertNotIn("--pgo-scenario", argv)
            self.assertEqual(pgo_pipeline.active_scenarios(config, host="win32"), ("full",))


class SkippedPdfGroupTests(unittest.TestCase):
    def test_skipped_pdf_group_is_an_error_on_linux_only(self) -> None:
        report = {"skipped_groups": [{"group": "pdf", "reason": "pdfium not found"}]}
        with self.assertRaises(pgo_pipeline.PgoError) as raised:
            pgo_pipeline.reject_skipped_pdf_group(report, "full", host="linux")
        self.assertIn("pdfium", str(raised.exception))
        self.assertIn("full", str(raised.exception))
        pgo_pipeline.reject_skipped_pdf_group(report, "full", host="win32")

    def test_other_skipped_groups_and_empty_reports_are_fine_on_linux(self) -> None:
        pgo_pipeline.reject_skipped_pdf_group({}, "full", host="linux")
        pgo_pipeline.reject_skipped_pdf_group({"skipped_groups": None}, "full", host="linux")
        other = {"skipped_groups": [{"group": "lsp_nav", "reason": "ty not found"}]}
        pgo_pipeline.reject_skipped_pdf_group(other, "full", host="linux")


class ScenarioOrderTests(unittest.TestCase):
    def test_default_scenarios_on_linux_run_full_before_startup(self) -> None:
        config = linux_config(Path("/x"))
        self.assertEqual(
            pgo_pipeline.active_scenarios(config, host="linux"), ("full", "startup", "welcome")
        )

    def test_startup_without_a_preceding_full_is_rejected(self) -> None:
        with self.assertRaisesRegex(pgo_pipeline.PgoError, "startup"):
            linux_config(Path("/x"), scenarios=("startup", "full")).validate()
        with self.assertRaisesRegex(pgo_pipeline.PgoError, "unknown"):
            linux_config(Path("/x"), scenarios=("bogus",)).validate()
        linux_config(Path("/x"), scenarios=("full", "startup", "group:pdf")).validate()


class FakeApi:
    def __init__(self) -> None:
        self.server = mock.Mock(request_count=1, last_request={"accepted": True})
        self.base_url = "http://127.0.0.1:18080/api/v1"
        self.stopped = False

    def stop(self) -> None:
        self.stopped = True


def full_database_telemetry() -> postgres_fixture.PostgresFixtureTelemetry:
    families = tuple(sorted(pgo_pipeline.REQUIRED_DATABASE_SQL_FAMILIES))
    return postgres_fixture.PostgresFixtureTelemetry(
        accepted_connection_count=1,
        startup_count=1,
        ssl_request_count=0,
        sql_statements=(),
        sql_families=families,
        family_counts=tuple((family, 1) for family in families),
        protocol_errors=(),
        unexpected_sql=(),
        peer_disconnects=(),
        worker_errors=(),
    )


class RunTrainingFlowTests(unittest.TestCase):
    def prepare(self, directory: str, **overrides: object):
        root = Path(directory)
        config = linux_config(root, **overrides)
        paths = pgo_pipeline.paths_for(config)
        paths.fixture_dir.mkdir(parents=True)
        paths.state_dir.mkdir(parents=True)
        (paths.fixture_dir / "openapi.json").write_text(
            json.dumps({"openapi": "3.1.0", "info": {}, "paths": {}}), encoding="utf-8"
        )
        database = mock.Mock()
        database.start.return_value = database
        database.endpoint = ("127.0.0.1", 25999)
        database.telemetry.return_value = full_database_telemetry()
        return config, paths, database

    def run_flow(self, config, paths, database, runner, **training_options):
        api = FakeApi()
        with (
            mock.patch.object(pgo_pipeline.LocalApiServer, "start", return_value=api),
            mock.patch.object(pgo_pipeline, "LocalPostgresFixture", return_value=database),
        ):
            try:
                return pgo_pipeline.run_training(config, paths, Path("/bin/rriter"), runner, **training_options), api
            finally:
                self.assertTrue(api.stopped)
                database.stop.assert_called_once_with()

    def test_three_scenarios_run_in_order_with_shared_fixtures_and_own_reports(self) -> None:
        commands: list[list[str]] = []
        with tempfile.TemporaryDirectory() as directory:
            config, paths, database = self.prepare(directory)

            class Runner:
                def run_process_tree(self, command, **kwargs):
                    argv = [str(part) for part in command]
                    commands.append(argv)
                    report = Path(argv[argv.index("--pgo-report") + 1])
                    report.write_text(
                        json.dumps(
                            {
                                "status": "success",
                                "scenario_version": pgo_pipeline.SCENARIO_VERSION,
                                "skipped_groups": [{"group": "lsp_nav", "reason": "ty not found"}],
                            }
                        ),
                        encoding="utf-8",
                    )
                    return mock.Mock(returncode=0, stderr="")

            result, _ = self.run_flow(config, paths, database, Runner())
            self.assertEqual(database.start.call_count, 1)
            self.assertEqual(
                [argv[argv.index("--pgo-scenario") + 1] for argv in commands],
                ["full", "startup", "welcome"],
            )
            for name in ("full", "startup", "welcome"):
                self.assertTrue((paths.training_dir / f"automation-report-{name}.json").is_file())
            self.assertIn("database_fixture", result)
            self.assertEqual(result["skipped_groups"][0]["group"], "lsp_nav")

    def test_failed_status_report_names_the_scenario_step_and_stderr_tail(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config, paths, database = self.prepare(directory, scenarios=("full", "startup"))

            class Runner:
                calls = 0

                def run_process_tree(self, command, **kwargs):
                    Runner.calls += 1
                    argv = [str(part) for part in command]
                    name = argv[argv.index("--pgo-scenario") + 1]
                    report = Path(argv[argv.index("--pgo-report") + 1])
                    status = "success" if name == "full" else "failed"
                    report.write_text(
                        json.dumps(
                            {
                                "status": status,
                                "scenario_version": pgo_pipeline.SCENARIO_VERSION,
                                "failed_step": "restored tabs",
                            }
                        ),
                        encoding="utf-8",
                    )
                    return mock.Mock(returncode=0 if status == "success" else 1, stderr="boom line\n")

            with self.assertRaises(pgo_pipeline.PgoError) as captured:
                self.run_flow(config, paths, database, Runner())
            message = str(captured.exception)
            self.assertIn("startup", message)
            self.assertIn("restored tabs", message)
            self.assertIn("boom line", message)

    def test_scenario_version_mismatch_is_rejected_for_every_scenario(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config, paths, database = self.prepare(directory, scenarios=("welcome",))

            class Runner:
                def run_process_tree(self, command, **kwargs):
                    argv = [str(part) for part in command]
                    Path(argv[argv.index("--pgo-report") + 1]).write_text(
                        json.dumps({"status": "success", "scenario_version": 1}), encoding="utf-8"
                    )
                    return mock.Mock(returncode=0, stderr="")

            with self.assertRaisesRegex(pgo_pipeline.PgoError, "scenario version"):
                self.run_flow(config, paths, database, Runner())

    def test_api_and_database_checks_apply_to_full_only(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config, paths, database = self.prepare(directory, scenarios=("welcome",))
            database.telemetry.return_value = postgres_fixture.PostgresFixtureTelemetry(
                accepted_connection_count=0, startup_count=0, ssl_request_count=0,
                sql_statements=(), sql_families=(), family_counts=(), protocol_errors=(),
                unexpected_sql=(), peer_disconnects=(), worker_errors=(),
            )

            class Runner:
                def run_process_tree(self, command, **kwargs):
                    argv = [str(part) for part in command]
                    Path(argv[argv.index("--pgo-report") + 1]).write_text(
                        json.dumps(
                            {"status": "success", "scenario_version": pgo_pipeline.SCENARIO_VERSION}
                        ),
                        encoding="utf-8",
                    )
                    return mock.Mock(returncode=0, stderr="")

            result, _ = self.run_flow(config, paths, database, Runner())
            self.assertEqual(result["status"], "success")


class RawProfileTests(unittest.TestCase):
    def test_empty_raw_profile_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config = linux_config(Path(directory))
            paths = pgo_pipeline.paths_for(config)
            paths.profile_dir.mkdir(parents=True)
            (paths.profile_dir / "rriter-1-2.profraw").write_bytes(b"data")
            (paths.profile_dir / "rriter-3-4.profraw").write_bytes(b"")
            with self.assertRaisesRegex(pgo_pipeline.PgoError, "rriter-3-4.profraw"):
                pgo_coverage.verify_raw_profiles(paths.profile_dir)


class PerRunRawProfileTests(unittest.TestCase):
    def test_each_run_must_leave_its_own_non_empty_profraw(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "old.profraw").write_bytes(b"data")
            before = {root / "old.profraw"}
            with self.assertRaisesRegex(pgo_coverage.PgoError, "scenario startup: .*no new"):
                pgo_coverage.check_new_raw_profiles(root, before, "startup")
            (root / "new.profraw").write_bytes(b"")
            with self.assertRaisesRegex(pgo_coverage.PgoError, "empty .*new.profraw"):
                pgo_coverage.check_new_raw_profiles(root, before, "startup")
            (root / "new.profraw").write_bytes(b"data")
            pgo_coverage.check_new_raw_profiles(root, before, "startup")

    def test_training_with_check_raw_fails_when_a_later_scenario_writes_no_profile(self) -> None:
        flow = RunTrainingFlowTests()
        with tempfile.TemporaryDirectory() as directory:
            config, paths, database = flow.prepare(directory, scenarios=("full", "startup"))
            paths.profile_dir.mkdir(parents=True)

            class Runner:
                def run_process_tree(self, command, **kwargs):
                    argv = [str(part) for part in command]
                    name = argv[argv.index("--pgo-scenario") + 1]
                    if name == "full":
                        (paths.profile_dir / "rriter-1-1.profraw").write_bytes(b"data")
                    Path(argv[argv.index("--pgo-report") + 1]).write_text(
                        json.dumps({"status": "success", "scenario_version": pgo_pipeline.SCENARIO_VERSION}),
                        encoding="utf-8",
                    )
                    return mock.Mock(returncode=0, stderr="")

            with self.assertRaisesRegex(pgo_pipeline.PgoError, "scenario startup: .*no new"):
                flow.run_flow(config, paths, database, Runner(), check_raw=True)


class InstallBinaryTests(unittest.TestCase):
    def test_install_binary_replaces_the_destination_atomically(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "rriter-new"
            source.write_bytes(b"new")
            destination = root / "out" / "rriter"
            destination.parent.mkdir()
            destination.write_bytes(b"old")
            pgo_pipeline.install_binary(source, destination)
            self.assertEqual(destination.read_bytes(), b"new")
            self.assertEqual([p.name for p in destination.parent.iterdir()], ["rriter"])


if __name__ == "__main__":
    unittest.main()
