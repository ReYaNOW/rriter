#!/usr/bin/env python3
"""Fast unit tests for shared build menus and the cross-platform PGO pipeline."""

from __future__ import annotations

import errno
import io
import json
import socket
import struct
import sys
import tempfile
import threading
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
import pgo_postgres_fixture


def complete_pgo_database_telemetry(**overrides: object) -> pgo_postgres_fixture.PostgresFixtureTelemetry:
    values: dict[str, object] = {
        "accepted_connection_count": 6,
        "startup_count": 6,
        "ssl_request_count": 0,
        "sql_statements": (),
        "sql_families": tuple(sorted(pgo_pipeline.REQUIRED_DATABASE_SQL_FAMILIES)),
        "family_counts": tuple(
            (family, 1) for family in sorted(pgo_pipeline.REQUIRED_DATABASE_SQL_FAMILIES)
        ),
        "protocol_errors": (),
        "unexpected_sql": (),
        "peer_disconnects": (),
        "worker_errors": (),
    }
    values.update(overrides)
    return pgo_postgres_fixture.PostgresFixtureTelemetry(**values)


TABLE_METADATA_SQL = """
SELECT
    a.attnum::int4,
    a.attname,
    format_type(a.atttypid, a.atttypmod),
    a.atttypid::int8,
    NOT a.attnotnull,
    pg_get_expr(ad.adbin, ad.adrelid),
    a.attidentity::text,
    a.attgenerated::text,
    EXISTS (
        SELECT 1
        FROM pg_index i
        WHERE i.indrelid = c.oid
          AND i.indisprimary
          AND a.attnum = ANY(i.indkey)
    ) AS is_primary_key,
    t.typtype::text
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_attribute a ON a.attrelid = c.oid
JOIN pg_type t ON t.oid = a.atttypid
LEFT JOIN pg_attrdef ad ON ad.adrelid = c.oid AND ad.adnum = a.attnum
WHERE n.nspname = 'public'
  AND c.relname = $1
  AND c.relkind IN ('r', 'p')
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY a.attnum
"""

UPDATE_SQL = """
UPDATE public."pgo_items"
SET "name" = $1::text::text
WHERE "id" = $2::text::integer
  AND xmin = $3::text::xid
RETURNING
    "id"::text,
    "name"::text,
    "active"::text,
    xmin::text AS __rriter_xmin
"""


class RawPostgresClient:
    """Small raw client that emits the tokio-postgres message shapes RRiter uses."""

    def __init__(self, endpoint: tuple[str, int]) -> None:
        self.socket = socket.create_connection(endpoint, timeout=2)
        self.socket.settimeout(2)

    def startup(self) -> list[tuple[bytes, bytes]]:
        self.socket.sendall(
            pgo_postgres_fixture.encode_startup_packet(
                {
                    "client_encoding": "UTF8",
                    "user": pgo_postgres_fixture.PGO_DATABASE_USER,
                    "database": pgo_postgres_fixture.PGO_DATABASE_NAME,
                    "application_name": "RRiter Database Tools",
                }
            )
        )
        return self.read_until_ready()

    def query(self, sql: str) -> list[tuple[bytes, bytes]]:
        self.send("Q", pgo_postgres_fixture.encode_cstring(sql))
        return self.read_until_ready()

    def prepare(self, name: str, sql: str) -> list[tuple[bytes, bytes]]:
        parse_body = (
            pgo_postgres_fixture.encode_cstring(name)
            + pgo_postgres_fixture.encode_cstring(sql)
            + struct.pack("!h", 0)
        )
        describe_body = b"S" + pgo_postgres_fixture.encode_cstring(name)
        self.socket.sendall(
            pgo_postgres_fixture.encode_protocol_message("P", parse_body)
            + pgo_postgres_fixture.encode_protocol_message("D", describe_body)
            + pgo_postgres_fixture.encode_protocol_message("S")
        )
        return self.read_until_ready()

    def bind_execute(
        self,
        statement_name: str,
        parameters: list[bytes],
        *,
        parameter_format: int = 1,
        result_format: int = 1,
    ) -> list[tuple[bytes, bytes]]:
        body = bytearray()
        body.extend(pgo_postgres_fixture.encode_cstring(""))
        body.extend(pgo_postgres_fixture.encode_cstring(statement_name))
        if parameters:
            body.extend(struct.pack("!h", 1))
            body.extend(struct.pack("!h", parameter_format))
        else:
            body.extend(struct.pack("!h", 0))
        body.extend(struct.pack("!h", len(parameters)))
        for value in parameters:
            body.extend(struct.pack("!i", len(value)))
            body.extend(value)
        body.extend(struct.pack("!h", 1))
        body.extend(struct.pack("!h", result_format))
        execute = pgo_postgres_fixture.encode_cstring("") + struct.pack("!i", 0)
        self.socket.sendall(
            pgo_postgres_fixture.encode_protocol_message("B", bytes(body))
            + pgo_postgres_fixture.encode_protocol_message("E", execute)
            + pgo_postgres_fixture.encode_protocol_message("S")
        )
        return self.read_until_ready()

    def close_statement_with_flush(self, name: str) -> list[tuple[bytes, bytes]]:
        close_body = b"S" + pgo_postgres_fixture.encode_cstring(name)
        self.socket.sendall(
            pgo_postgres_fixture.encode_protocol_message("C", close_body)
            + pgo_postgres_fixture.encode_protocol_message("H")
            + pgo_postgres_fixture.encode_protocol_message("S")
        )
        return self.read_until_ready()

    def send(self, code: str, body: bytes = b"") -> None:
        self.socket.sendall(pgo_postgres_fixture.encode_protocol_message(code, body))

    def read_until_ready(self) -> list[tuple[bytes, bytes]]:
        messages: list[tuple[bytes, bytes]] = []
        while True:
            message = pgo_postgres_fixture.read_protocol_message(self.socket)
            if message is None:
                raise AssertionError("fixture closed connection before ReadyForQuery")
            messages.append(message)
            if message[0] == b"Z":
                return messages

    def close(self) -> None:
        try:
            self.send("X")
        except OSError:
            pass
        self.socket.close()

    def __enter__(self) -> "RawPostgresClient":
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()


def decode_data_row(body: bytes) -> list[bytes | None]:
    count = struct.unpack_from("!h", body, 0)[0]
    offset = 2
    values: list[bytes | None] = []
    for _ in range(count):
        length = struct.unpack_from("!i", body, offset)[0]
        offset += 4
        if length == -1:
            values.append(None)
            continue
        values.append(body[offset : offset + length])
        offset += length
    if offset != len(body):
        raise AssertionError("trailing bytes in DataRow")
    return values


def decode_parameter_oids(body: bytes) -> list[int]:
    count = struct.unpack_from("!h", body, 0)[0]
    return [struct.unpack_from("!I", body, 2 + index * 4)[0] for index in range(count)]


def encode_binary_oid_array(values: list[int]) -> bytes:
    body = bytearray(
        struct.pack(
            "!iiIii",
            1,
            0,
            pgo_postgres_fixture.OID_OID,
            len(values),
            1,
        )
    )
    for value in values:
        body.extend(struct.pack("!iI", 4, value))
    return bytes(body)


def row_description_oids(body: bytes) -> list[int]:
    count = struct.unpack_from("!h", body, 0)[0]
    offset = 2
    oids: list[int] = []
    for _ in range(count):
        terminator = body.index(0, offset)
        offset = terminator + 1
        _table_oid, _attribute, oid, _size, _modifier, _format = struct.unpack_from(
            "!IhIhih", body, offset
        )
        offset += 18
        oids.append(oid)
    return oids


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


class PostgresFixtureTests(unittest.TestCase):
    def test_broken_pipe_is_peer_disconnect(self) -> None:
        error = BrokenPipeError(errno.EPIPE, "Broken pipe")
        self.assertTrue(pgo_postgres_fixture._is_peer_disconnect_error(error))

    def test_connection_reset_is_peer_disconnect(self) -> None:
        error = ConnectionResetError(errno.ECONNRESET, "Connection reset")
        self.assertTrue(pgo_postgres_fixture._is_peer_disconnect_error(error))

    def test_connection_aborted_is_peer_disconnect(self) -> None:
        error = ConnectionAbortedError(errno.ECONNABORTED, "Connection aborted")
        self.assertTrue(pgo_postgres_fixture._is_peer_disconnect_error(error))

    def test_plain_oserror_disconnect_errnos_are_peer_disconnects(self) -> None:
        for error_number in (errno.EPIPE, errno.ECONNRESET, errno.ECONNABORTED):
            with self.subTest(errno=error_number):
                error = OSError("synthetic peer disconnect")
                error.errno = error_number
                self.assertTrue(pgo_postgres_fixture._is_peer_disconnect_error(error))

    def test_worker_peer_disconnect_is_nonfatal_and_observable(self) -> None:
        fixture = pgo_postgres_fixture.LocalPostgresFixture()
        client = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            with mock.patch.object(
                fixture,
                "_serve_connection",
                side_effect=BrokenPipeError(errno.EPIPE, "Broken pipe"),
            ):
                fixture._worker_entry(client, 4)
        finally:
            client.close()

        telemetry = fixture.telemetry()
        self.assertEqual(telemetry.worker_errors, ())
        self.assertEqual(telemetry.protocol_errors, ())
        self.assertEqual(len(telemetry.peer_disconnects), 1)
        self.assertIn("connection 4", telemetry.peer_disconnects[0])
        self.assertIn("BrokenPipeError", telemetry.peer_disconnects[0])
        fixture.assert_healthy()

    def test_real_oserror_remains_fatal_worker_error(self) -> None:
        fixture = pgo_postgres_fixture.LocalPostgresFixture()
        error = OSError(errno.EIO, "fixture I/O failure")
        self.assertFalse(pgo_postgres_fixture._is_peer_disconnect_error(error))
        client = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            with mock.patch.object(fixture, "_serve_connection", side_effect=error):
                fixture._worker_entry(client, 5)
        finally:
            client.close()

        telemetry = fixture.telemetry()
        self.assertEqual(telemetry.peer_disconnects, ())
        self.assertTrue(
            any("fixture I/O failure" in value for value in telemetry.worker_errors)
        )
        with self.assertRaisesRegex(
            pgo_postgres_fixture.PostgresFixtureError, "fixture I/O failure"
        ):
            fixture.assert_healthy()

    def test_start_stop_uses_ephemeral_loopback_and_releases_listener(self) -> None:
        fixture = pgo_postgres_fixture.LocalPostgresFixture()
        fixture.start()
        endpoint = fixture.endpoint
        self.assertEqual(endpoint[0], "127.0.0.1")
        self.assertGreater(endpoint[1], 0)
        with RawPostgresClient(endpoint) as client:
            self.assertEqual(client.startup()[-1], (b"Z", b"I"))
        fixture.stop()

        telemetry = fixture.telemetry()
        self.assertEqual(telemetry.accepted_connection_count, 1)
        probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            probe.settimeout(0.2)
            self.assertNotEqual(probe.connect_ex(endpoint), 0)
        finally:
            probe.close()

    def test_worker_error_is_retained_and_fails_health_check(self) -> None:
        fixture = pgo_postgres_fixture.LocalPostgresFixture()
        worker_started = threading.Event()

        def fail_worker(_client: socket.socket, _connection_id: int) -> None:
            worker_started.set()
            raise RuntimeError("forced worker failure")

        with mock.patch.object(fixture, "_serve_connection", side_effect=fail_worker):
            fixture.start()
            connection = socket.create_connection(fixture.endpoint, timeout=2)
            connection.close()
            self.assertTrue(worker_started.wait(2))
            fixture.stop()

        self.assertTrue(
            any("forced worker failure" in value for value in fixture.telemetry().worker_errors)
        )
        with self.assertRaisesRegex(
            pgo_postgres_fixture.PostgresFixtureError, "forced worker failure"
        ):
            fixture.assert_healthy()

    def test_malformed_protocol_remains_fatal(self) -> None:
        fixture = pgo_postgres_fixture.LocalPostgresFixture()
        fixture.start()
        try:
            with RawPostgresClient(fixture.endpoint) as client:
                client.startup()
                client.socket.sendall(b"Q" + struct.pack("!I", 3))
                self.assertEqual(client.socket.recv(1), b"")
        finally:
            fixture.stop()

        telemetry = fixture.telemetry()
        self.assertEqual(telemetry.worker_errors, ())
        self.assertEqual(telemetry.peer_disconnects, ())
        self.assertTrue(
            any(
                "invalid PostgreSQL message length" in value
                for value in telemetry.protocol_errors
            )
        )
        with self.assertRaisesRegex(
            pgo_postgres_fixture.PostgresFixtureError, "protocol errors"
        ):
            fixture.assert_healthy()

    def test_startup_returns_authentication_parameters_backend_key_and_ready(self) -> None:
        with pgo_postgres_fixture.LocalPostgresFixture() as fixture:
            with RawPostgresClient(fixture.endpoint) as client:
                messages = client.startup()

            codes = [code for code, _body in messages]
            self.assertEqual(codes[0], b"R")
            self.assertEqual(struct.unpack("!I", messages[0][1])[0], 0)
            self.assertIn(b"S", codes)
            self.assertIn(b"K", codes)
            self.assertEqual(codes[-1], b"Z")
            self.assertEqual(messages[-1][1], b"I")
            telemetry = fixture.telemetry()
            self.assertEqual(telemetry.startup_count, 1)
            self.assertEqual(telemetry.protocol_errors, ())

    def test_simple_query_returns_fixture_rows_and_telemetry(self) -> None:
        sql = """
            SELECT id, name, active
            FROM public.pgo_items
            ORDER BY id
            LIMIT 64;
        """
        with pgo_postgres_fixture.LocalPostgresFixture() as fixture:
            with RawPostgresClient(fixture.endpoint) as client:
                client.startup()
                messages = client.query(sql)

            codes = [code for code, _body in messages]
            self.assertEqual(codes[0], b"T")
            self.assertEqual(codes.count(b"D"), 64)
            self.assertEqual(codes[-2:], [b"C", b"Z"])
            first_row = decode_data_row(next(body for code, body in messages if code == b"D"))
            self.assertEqual(first_row, [b"1", b"pgo-item-001", b"t"])
            self.assertEqual(messages[-1][1], b"I")
            telemetry = fixture.telemetry()
            self.assertEqual(telemetry.family_count("user_select"), 1)
            self.assertIn(pgo_postgres_fixture.normalize_sql(sql), telemetry.sql_statements)
            fixture.assert_healthy()

    def test_extended_query_matches_tokio_postgres_binary_formats(self) -> None:
        with pgo_postgres_fixture.LocalPostgresFixture() as fixture:
            with RawPostgresClient(fixture.endpoint) as client:
                client.startup()
                prepare = client.prepare("s1", TABLE_METADATA_SQL)
                self.assertEqual(
                    [code for code, _body in prepare],
                    [b"1", b"t", b"T", b"Z"],
                )
                parameter_description = next(
                    body for code, body in prepare if code == b"t"
                )
                self.assertEqual(
                    decode_parameter_oids(parameter_description),
                    [pgo_postgres_fixture.OID_NAME],
                )
                description = next(body for code, body in prepare if code == b"T")
                self.assertEqual(
                    row_description_oids(description),
                    [
                        pgo_postgres_fixture.OID_INT4,
                        pgo_postgres_fixture.OID_NAME,
                        pgo_postgres_fixture.OID_TEXT,
                        pgo_postgres_fixture.OID_INT8,
                        pgo_postgres_fixture.OID_BOOL,
                        pgo_postgres_fixture.OID_TEXT,
                        pgo_postgres_fixture.OID_TEXT,
                        pgo_postgres_fixture.OID_TEXT,
                        pgo_postgres_fixture.OID_BOOL,
                        pgo_postgres_fixture.OID_TEXT,
                    ],
                )

                execute = client.bind_execute("s1", [b"pgo_items"])
                self.assertEqual(execute[0][0], b"2")
                self.assertEqual(sum(code == b"D" for code, _body in execute), 3)
                first_row = decode_data_row(
                    next(body for code, body in execute if code == b"D")
                )
                self.assertEqual(struct.unpack("!i", first_row[0])[0], 1)
                self.assertEqual(first_row[1], b"id")
                self.assertEqual(struct.unpack("!q", first_row[3])[0], 23)
                self.assertEqual(first_row[4], b"\x00")
                self.assertEqual(first_row[8], b"\x01")
                self.assertEqual(execute[-1], (b"Z", b"I"))

                closed = client.close_statement_with_flush("s1")
                self.assertEqual([code for code, _body in closed], [b"3", b"Z"])

            telemetry = fixture.telemetry()
            self.assertEqual(telemetry.family_count("table_metadata"), 1)
            fixture.assert_healthy()

    def test_extended_oid_array_parameter_uses_postgres_binary_array_layout(self) -> None:
        sql = """
            SELECT e.enumtypid::int8, e.enumlabel
            FROM pg_enum e
            WHERE e.enumtypid = ANY($1::oid[])
            ORDER BY e.enumtypid, e.enumsortorder
        """
        with pgo_postgres_fixture.LocalPostgresFixture() as fixture:
            with RawPostgresClient(fixture.endpoint) as client:
                client.startup()
                prepare = client.prepare("s_enum", sql)
                parameter_description = next(
                    body for code, body in prepare if code == b"t"
                )
                self.assertEqual(
                    decode_parameter_oids(parameter_description),
                    [pgo_postgres_fixture.OID_OID_ARRAY],
                )
                execute = client.bind_execute(
                    "s_enum",
                    [encode_binary_oid_array([pgo_postgres_fixture.OID_INT4])],
                )
                self.assertEqual([code for code, _body in execute], [b"2", b"C", b"Z"])
                self.assertEqual(execute[-1], (b"Z", b"I"))

            self.assertEqual(fixture.telemetry().family_count("enum_values"), 1)
            fixture.assert_healthy()

    def test_unexpected_sql_returns_error_and_is_never_counted_as_success(self) -> None:
        sql = "SELECT definitely_not_a_fixture_query()"
        with pgo_postgres_fixture.LocalPostgresFixture() as fixture:
            with RawPostgresClient(fixture.endpoint) as client:
                client.startup()
                messages = client.query(sql)

            self.assertEqual([code for code, _body in messages], [b"E", b"Z"])
            self.assertEqual(messages[-1][1], b"I")
            telemetry = fixture.telemetry()
            self.assertEqual(
                telemetry.unexpected_sql,
                (pgo_postgres_fixture.normalize_sql(sql),),
            )
            self.assertNotIn(pgo_postgres_fixture.normalize_sql(sql), telemetry.sql_statements)
            with self.assertRaisesRegex(
                pgo_postgres_fixture.PostgresFixtureError, "unexpected SQL"
            ):
                fixture.assert_healthy()

    def test_update_transaction_uses_binary_parameters_and_rollback_restores_rows(self) -> None:
        select_first = """
            SELECT id, name, active
            FROM public.pgo_items
            ORDER BY id
            LIMIT 1;
        """
        with pgo_postgres_fixture.LocalPostgresFixture() as fixture:
            with RawPostgresClient(fixture.endpoint) as client:
                client.startup()
                begin = client.query(
                    "BEGIN; "
                    "SET LOCAL statement_timeout = '5s'; "
                    "SET LOCAL lock_timeout = '2s';"
                )
                self.assertEqual([code for code, _body in begin], [b"C", b"C", b"C", b"Z"])
                self.assertEqual(begin[-1][1], b"T")

                prepare = client.prepare("s2", UPDATE_SQL)
                parameter_description = next(
                    body for code, body in prepare if code == b"t"
                )
                self.assertEqual(
                    decode_parameter_oids(parameter_description),
                    [
                        pgo_postgres_fixture.OID_TEXT,
                        pgo_postgres_fixture.OID_TEXT,
                        pgo_postgres_fixture.OID_TEXT,
                    ],
                )
                self.assertEqual(prepare[-1], (b"Z", b"T"))

                update = client.bind_execute(
                    "s2",
                    [b"pgo-updated", b"1", b"5001"],
                )
                update_row = decode_data_row(
                    next(body for code, body in update if code == b"D")
                )
                self.assertEqual(update_row[1], b"pgo-updated")
                self.assertEqual(update[-1], (b"Z", b"T"))

                changed = client.query(select_first)
                changed_row = decode_data_row(
                    next(body for code, body in changed if code == b"D")
                )
                self.assertEqual(changed_row[1], b"pgo-updated")
                self.assertEqual(changed[-1], (b"Z", b"T"))

                rollback = client.query("ROLLBACK")
                self.assertEqual([code for code, _body in rollback], [b"C", b"Z"])
                self.assertEqual(rollback[-1][1], b"I")

                restored = client.query(select_first)
                restored_row = decode_data_row(
                    next(body for code, body in restored if code == b"D")
                )
                self.assertEqual(restored_row[1], b"pgo-item-001")
                self.assertEqual(restored[-1], (b"Z", b"I"))

            telemetry = fixture.telemetry()
            self.assertEqual(telemetry.family_count("begin"), 1)
            self.assertEqual(telemetry.family_count("set_local"), 2)
            self.assertEqual(telemetry.family_count("update_returning"), 1)
            self.assertEqual(telemetry.family_count("rollback"), 1)
            self.assertEqual(telemetry.family_count("user_select"), 2)
            fixture.assert_healthy()


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
            dart = (workspace / "lib" / "pgo_training.dart").read_text(encoding="utf-8")
            pubspec = (workspace / "pubspec.yaml").read_text(encoding="utf-8")
            openapi = json.loads((workspace / "openapi.json").read_text(encoding="utf-8"))
            self.assertIn("generated_5999", large)
            self.assertGreaterEqual(len(dart.splitlines()), 600)
            self.assertIn("pgoDartTarget", dart)
            self.assertIn("pgoDartCompletionTarget", dart)
            self.assertIn("while (cursor > 0)", dart)
            self.assertIn("try {", dart)
            self.assertIn("finally {", dart)
            self.assertNotIn("package:", dart)
            self.assertNotIn("dependencies:", pubspec)
            self.assertNotIn("http://", pubspec)
            self.assertNotIn("https://", pubspec)
            self.assertIn("/automation/featured/{resource_id}", openapi["paths"])
            self.assertIn("/automation/ping", openapi["paths"])
            self.assertTrue((workspace / "src" / "worker.py").is_file())
            markdown = (workspace / "README.md").read_text(encoding="utf-8")
            for marker in (
                "# RRiter PGO Markdown fixture",
                "## Edit and Read coverage",
                "### Semantic structures",
                "**strong text**",
                "*emphasis text*",
                "`inline_code(42)`",
                "[deterministic link](https://example.invalid/path)",
                "> Multiline quote continuation",
                "- [ ] unchecked task",
                "- [x] checked task",
                "| :--- | :----: | ----: |",
                "```rust",
                "```python",
                "```bash",
                "\u043a\u0438\u0440\u0438\u043b\u043b\u0438\u0446\u0430 \U0001f600",
                "RRITER_PGO_MARKDOWN_EDIT_TARGET",
            ):
                self.assertIn(marker, markdown)
            self.assertGreaterEqual(len(markdown.splitlines()), 60)

    def test_dart_fixture_generation_is_deterministic(self) -> None:
        self.assertEqual(
            pgo_pipeline._dart_fixture_source(),
            pgo_pipeline._dart_fixture_source(),
        )

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
            environment = pgo_pipeline.isolated_runtime_environment(
                config,
                paths,
                database_endpoint=("127.0.0.1", 15432),
            )
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
            self.assertEqual(environment[pgo_pipeline.PGO_DATABASE_ENV_HOST], "127.0.0.1")
            self.assertEqual(environment[pgo_pipeline.PGO_DATABASE_ENV_PORT], "15432")
            self.assertEqual(
                environment[pgo_pipeline.PGO_DATABASE_ENV_NAME],
                pgo_postgres_fixture.PGO_DATABASE_NAME,
            )
            self.assertEqual(
                environment[pgo_pipeline.PGO_DATABASE_ENV_USER],
                pgo_postgres_fixture.PGO_DATABASE_USER,
            )

    def test_database_fixture_telemetry_requires_all_production_workload_families(self) -> None:
        telemetry = complete_pgo_database_telemetry()
        pgo_pipeline.validate_database_fixture_telemetry(telemetry)

        missing_counts = tuple(
            (family, count)
            for family, count in telemetry.family_counts
            if family != "user_select"
        )
        missing = complete_pgo_database_telemetry(family_counts=missing_counts)
        with self.assertRaisesRegex(pgo_pipeline.PgoError, "user_select"):
            pgo_pipeline.validate_database_fixture_telemetry(missing)

        unexpected = complete_pgo_database_telemetry(
            unexpected_sql=("SELECT definitely_not_supported",),
        )
        with self.assertRaisesRegex(pgo_pipeline.PgoError, "unexpected_sql"):
            pgo_pipeline.validate_database_fixture_telemetry(unexpected)

        ssl_request = complete_pgo_database_telemetry(ssl_request_count=1)
        with self.assertRaisesRegex(pgo_pipeline.PgoError, "ssl_request_count"):
            pgo_pipeline.validate_database_fixture_telemetry(ssl_request)

    def test_database_fixture_telemetry_accepts_peer_disconnect(self) -> None:
        telemetry = complete_pgo_database_telemetry(
            peer_disconnects=("connection 4: BrokenPipeError: [Errno 32] Broken pipe",),
        )
        pgo_pipeline.validate_database_fixture_telemetry(telemetry)

    def test_database_fixture_telemetry_rejects_real_worker_error(self) -> None:
        telemetry = complete_pgo_database_telemetry(
            worker_errors=("connection 4 I/O: fixture worker fault",),
        )
        with self.assertRaisesRegex(pgo_pipeline.PgoError, "worker_errors"):
            pgo_pipeline.validate_database_fixture_telemetry(telemetry)

    def test_database_fixture_report_preserves_peer_disconnects(self) -> None:
        peer_disconnects = (
            "connection 4: BrokenPipeError: [Errno 32] Broken pipe",
        )
        payload = pgo_pipeline.database_fixture_telemetry_payload(
            complete_pgo_database_telemetry(peer_disconnects=peer_disconnects)
        )
        self.assertEqual(payload["peer_disconnects"], list(peer_disconnects))

    def test_run_training_starts_database_fixture_before_app_and_stops_on_success(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rriter-pgo-db-training-test-") as directory:
            root = Path(directory)
            config = pgo_pipeline.PgoConfig(
                root=root,
                target="x86_64-unknown-linux-gnu",
                timeout_seconds=30,
            )
            paths = pgo_pipeline.paths_for(config)
            paths.fixture_dir.mkdir(parents=True)
            paths.state_dir.mkdir(parents=True)
            (paths.fixture_dir / "openapi.json").write_text(
                json.dumps({"openapi": "3.1.0", "info": {"title": "x", "version": "1"}, "paths": {}}),
                encoding="utf-8",
            )
            executable = root / "rriter"
            executable.write_text("fixture", encoding="utf-8")

            class FakeApiServer:
                def __init__(self) -> None:
                    self.server = mock.Mock(
                        request_count=1,
                        last_request={"accepted": True},
                    )
                    self.base_url = "http://127.0.0.1:18080/api/v1"
                    self.stopped = False

                def stop(self) -> None:
                    self.stopped = True

            class FakeDatabaseFixture:
                def __init__(self) -> None:
                    self.started = False
                    self.stopped = False

                def start(self) -> "FakeDatabaseFixture":
                    self.started = True
                    return self

                @property
                def endpoint(self) -> tuple[str, int]:
                    self.assert_started()
                    return ("127.0.0.1", 25432)

                def assert_started(self) -> None:
                    self_test.assertTrue(self.started)

                def stop(self) -> None:
                    self.stopped = True

                def telemetry(self) -> pgo_postgres_fixture.PostgresFixtureTelemetry:
                    return complete_pgo_database_telemetry()

            self_test = self
            api = FakeApiServer()
            database = FakeDatabaseFixture()

            class FakeRunner:
                def run_process_tree(self, command: object, **kwargs: object) -> object:
                    database.assert_started()
                    environment = kwargs["env"]
                    self_test.assertEqual(
                        environment[pgo_pipeline.PGO_DATABASE_ENV_PORT], "25432"
                    )
                    paths.report_path.write_text(
                        json.dumps(
                            {
                                "status": "success",
                                "scenario_version": pgo_pipeline.SCENARIO_VERSION,
                                "completed_steps": ["database-wait-user-query-result"],
                                "skipped_steps": [],
                            }
                        ),
                        encoding="utf-8",
                    )
                    return mock.Mock(returncode=0)

            with (
                mock.patch.object(pgo_pipeline.LocalApiServer, "start", return_value=api),
                mock.patch.object(pgo_pipeline, "LocalPostgresFixture", return_value=database),
            ):
                report = pgo_pipeline.run_training(
                    config,
                    paths,
                    executable,
                    FakeRunner(),
                )

            self.assertTrue(database.stopped)
            self.assertTrue(api.stopped)
            self.assertIn("database_fixture", report)
            self.assertIn(
                "user_select",
                report["database_fixture"]["family_counts"],
            )

    def test_pgo_failure_helpers_include_structured_step_and_unix_signal(self) -> None:
        report_path = Path("/tmp/automation-report.json")
        message = pgo_pipeline.automation_failure_message(
            {
                "status": "failed",
                "failed_step": "legacy failure",
                "failed_step_index": 196,
                "failed_step_name": "open-panel:Terminal",
                "failure_reason": "panel did not open: Terminal",
                "previous_completed_step": "wait-8-frames",
            },
            report_path,
        )
        self.assertIn("step=196", message)
        self.assertIn("name=open-panel:Terminal", message)
        self.assertIn("reason=panel did not open: Terminal", message)
        self.assertIn("previous=wait-8-frames", message)
        self.assertIn(f"report={report_path}", message)

        self.assertEqual(
            pgo_pipeline.describe_pgo_process_failure(101, os_name="posix"),
            "RRiter PGO process exited with code 101",
        )
        self.assertEqual(
            pgo_pipeline.describe_pgo_process_failure(-11, os_name="posix"),
            "RRiter PGO process terminated by SIGSEGV (11)",
        )

    def test_run_training_surfaces_structured_automation_failure(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rriter-pgo-structured-failure-test-") as directory:
            root = Path(directory)
            config = pgo_pipeline.PgoConfig(
                root=root,
                target="x86_64-unknown-linux-gnu",
                timeout_seconds=30,
            )
            paths = pgo_pipeline.paths_for(config)
            paths.fixture_dir.mkdir(parents=True)
            paths.state_dir.mkdir(parents=True)
            (paths.fixture_dir / "openapi.json").write_text(
                json.dumps({"openapi": "3.1.0", "info": {"title": "x", "version": "1"}, "paths": {}}),
                encoding="utf-8",
            )
            executable = root / "rriter"
            executable.write_text("fixture", encoding="utf-8")

            api = mock.Mock()
            api.base_url = "http://127.0.0.1:18080/api/v1"
            api.server.request_count = 1
            api.server.last_request = {"accepted": True}
            database = mock.Mock()
            database.start.return_value = database
            database.endpoint = ("127.0.0.1", 25434)
            database.telemetry.return_value = complete_pgo_database_telemetry()

            class FailedRunner:
                def run_process_tree(self, command: object, **kwargs: object) -> object:
                    paths.report_path.write_text(
                        json.dumps(
                            {
                                "status": "failed",
                                "scenario_version": pgo_pipeline.SCENARIO_VERSION,
                                "failed_step": "panel did not open: Terminal",
                                "failed_step_index": 196,
                                "failed_step_name": "open-panel:Terminal",
                                "failed_step_elapsed_ms": 17,
                                "failure_reason": "panel did not open: Terminal",
                                "previous_completed_step": "wait-8-frames",
                            }
                        ),
                        encoding="utf-8",
                    )
                    return mock.Mock(returncode=0)

            with (
                mock.patch.object(pgo_pipeline.LocalApiServer, "start", return_value=api),
                mock.patch.object(pgo_pipeline, "LocalPostgresFixture", return_value=database),
                self.assertRaises(pgo_pipeline.PgoError) as captured,
            ):
                pgo_pipeline.run_training(
                    config,
                    paths,
                    executable,
                    FailedRunner(),
                )

            message = str(captured.exception)
            self.assertIn("step=196", message)
            self.assertIn("name=open-panel:Terminal", message)
            self.assertIn("reason=panel did not open: Terminal", message)
            self.assertIn("previous=wait-8-frames", message)
            self.assertIn(f"report={paths.report_path}", message)
            database.stop.assert_called_once_with()
            api.stop.assert_called_once_with()

    def test_run_training_nonzero_without_report_explains_missing_structured_report(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rriter-pgo-process-failure-test-") as directory:
            root = Path(directory)
            config = pgo_pipeline.PgoConfig(
                root=root,
                target="x86_64-unknown-linux-gnu",
                timeout_seconds=30,
            )
            paths = pgo_pipeline.paths_for(config)
            paths.fixture_dir.mkdir(parents=True)
            paths.state_dir.mkdir(parents=True)
            (paths.fixture_dir / "openapi.json").write_text(
                json.dumps({"openapi": "3.1.0", "info": {"title": "x", "version": "1"}, "paths": {}}),
                encoding="utf-8",
            )
            executable = root / "rriter"
            executable.write_text("fixture", encoding="utf-8")

            api = mock.Mock()
            api.base_url = "http://127.0.0.1:18080/api/v1"
            api.server.request_count = 0
            api.server.last_request = {}
            database = mock.Mock()
            database.start.return_value = database
            database.endpoint = ("127.0.0.1", 25435)
            database.telemetry.return_value = complete_pgo_database_telemetry()

            class FailedRunner:
                def run_process_tree(self, command: object, **kwargs: object) -> object:
                    return mock.Mock(returncode=101)

            with (
                mock.patch.object(pgo_pipeline.LocalApiServer, "start", return_value=api),
                mock.patch.object(pgo_pipeline, "LocalPostgresFixture", return_value=database),
                self.assertRaises(pgo_pipeline.PgoError) as captured,
            ):
                pgo_pipeline.run_training(
                    config,
                    paths,
                    executable,
                    FailedRunner(),
                )

            message = str(captured.exception)
            self.assertIn("exited with code 101", message)
            self.assertIn("structured automation report is absent", message)
            self.assertIn("PGO_AUTOMATION_STEP_START", message)

    def test_run_training_stops_database_fixture_when_app_launch_raises(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rriter-pgo-db-training-fail-test-") as directory:
            root = Path(directory)
            config = pgo_pipeline.PgoConfig(
                root=root,
                target="x86_64-unknown-linux-gnu",
                timeout_seconds=30,
            )
            paths = pgo_pipeline.paths_for(config)
            paths.fixture_dir.mkdir(parents=True)
            paths.state_dir.mkdir(parents=True)
            (paths.fixture_dir / "openapi.json").write_text(
                json.dumps({"openapi": "3.1.0", "info": {"title": "x", "version": "1"}, "paths": {}}),
                encoding="utf-8",
            )
            executable = root / "rriter"
            executable.write_text("fixture", encoding="utf-8")

            api = mock.Mock()
            api.base_url = "http://127.0.0.1:18080/api/v1"
            database = mock.Mock()
            database.start.return_value = database
            database.endpoint = ("127.0.0.1", 25433)

            class RaisingRunner:
                def run_process_tree(self, command: object, **kwargs: object) -> object:
                    raise RuntimeError("launch failed")

            with (
                mock.patch.object(pgo_pipeline.LocalApiServer, "start", return_value=api),
                mock.patch.object(pgo_pipeline, "LocalPostgresFixture", return_value=database),
                self.assertRaisesRegex(RuntimeError, "launch failed"),
            ):
                pgo_pipeline.run_training(
                    config,
                    paths,
                    executable,
                    RaisingRunner(),
                )

            database.stop.assert_called_once_with()
            api.stop.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
