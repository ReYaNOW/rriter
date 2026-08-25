#!/usr/bin/env python3
"""Deterministic PostgreSQL wire-protocol fixture for RRiter PGO training.

This server intentionally implements only the protocol and SQL families used by
RRiter Database Tools. It has no PostgreSQL process dependency and no alternate
RRiter backend: tokio-postgres connects to it over a real loopback TCP socket.
"""

from __future__ import annotations

import collections
import errno
import re
import socket
import struct
import threading
from dataclasses import dataclass, replace
from typing import Mapping, Sequence

POSTGRES_PROTOCOL_VERSION = 196608
POSTGRES_SSL_REQUEST = 80877103
POSTGRES_GSSENC_REQUEST = 80877104
MAX_PROTOCOL_MESSAGE_BYTES = 16 * 1024 * 1024

PGO_DATABASE_NAME = "rriter_pgo"
PGO_DATABASE_USER = "rriter_pgo"
PGO_SCHEMA_NAME = "public"
PGO_TABLE_NAME = "pgo_items"
PGO_ROW_COUNT = 80

OID_BOOL = 16
OID_NAME = 19
OID_INT8 = 20
OID_INT4 = 23
OID_TEXT = 25
OID_OID = 26
OID_OID_ARRAY = 1028
OID_VARCHAR = 1043


class PostgresFixtureError(RuntimeError):
    """Fixture lifecycle, worker, or protocol-health failure."""


class _ProtocolError(RuntimeError):
    pass


class _UnsupportedSql(RuntimeError):
    pass


_PEER_DISCONNECT_ERRNOS = frozenset(
    getattr(errno, name)
    for name in ("EPIPE", "ECONNRESET", "ECONNABORTED")
    if hasattr(errno, name)
)


def _is_peer_disconnect_error(error: BaseException) -> bool:
    if isinstance(error, (BrokenPipeError, ConnectionResetError, ConnectionAbortedError)):
        return True
    return isinstance(error, OSError) and error.errno in _PEER_DISCONNECT_ERRNOS


@dataclass(frozen=True)
class PostgresFixtureTelemetry:
    accepted_connection_count: int
    startup_count: int
    ssl_request_count: int
    sql_statements: tuple[str, ...]
    sql_families: tuple[str, ...]
    family_counts: tuple[tuple[str, int], ...]
    protocol_errors: tuple[str, ...]
    unexpected_sql: tuple[str, ...]
    peer_disconnects: tuple[str, ...]
    worker_errors: tuple[str, ...]

    def family_count(self, family: str) -> int:
        for name, count in self.family_counts:
            if name == family:
                return count
        return 0


@dataclass(frozen=True)
class _Column:
    name: str
    oid: int
    type_size: int


@dataclass(frozen=True)
class _PgoItem:
    id: int
    name: str
    active: bool
    xmin: str


@dataclass(frozen=True)
class _StatementShape:
    family: str
    parameter_oids: tuple[int, ...]
    columns: tuple[_Column, ...]


@dataclass(frozen=True)
class _ExecutionResult:
    columns: tuple[_Column, ...]
    rows: tuple[tuple[object | None, ...], ...]
    command_tag: str


@dataclass(frozen=True)
class _PreparedStatement:
    sql: str
    shape: _StatementShape


@dataclass(frozen=True)
class _Portal:
    statement_name: str
    parameters: tuple[object | None, ...]
    result_formats: tuple[int, ...]


_TEXTLIKE_OIDS = {OID_NAME, OID_TEXT, OID_VARCHAR}

def encode_cstring(value: str) -> bytes:
    encoded = value.encode("utf-8")
    if b"\0" in encoded:
        raise ValueError("PostgreSQL cstring cannot contain NUL")
    return encoded + b"\0"


def encode_protocol_message(code: str | bytes, body: bytes = b"") -> bytes:
    if isinstance(code, str):
        code = code.encode("ascii")
    if len(code) != 1:
        raise ValueError("PostgreSQL protocol message code must be exactly one byte")
    return code + struct.pack("!I", len(body) + 4) + body


def encode_startup_packet(parameters: Mapping[str, str]) -> bytes:
    body = bytearray(struct.pack("!I", POSTGRES_PROTOCOL_VERSION))
    for name, value in parameters.items():
        body.extend(encode_cstring(name))
        body.extend(encode_cstring(value))
    body.append(0)
    return struct.pack("!I", len(body) + 4) + body


def read_protocol_message(stream: socket.socket) -> tuple[bytes, bytes] | None:
    code = _recv_exact(stream, 1, allow_eof=True)
    if code is None:
        return None
    raw_length = _recv_exact(stream, 4)
    assert raw_length is not None
    length = struct.unpack("!I", raw_length)[0]
    if length < 4 or length > MAX_PROTOCOL_MESSAGE_BYTES:
        raise _ProtocolError(f"invalid PostgreSQL message length: {length}")
    body = _recv_exact(stream, length - 4)
    assert body is not None
    return code, body


def normalize_sql(sql: str) -> str:
    return " ".join(sql.strip().rstrip(";").split())


def _recv_exact(
    stream: socket.socket,
    size: int,
    *,
    allow_eof: bool = False,
) -> bytes | None:
    data = bytearray()
    while len(data) < size:
        chunk = stream.recv(size - len(data))
        if not chunk:
            if allow_eof and not data:
                return None
            raise _ProtocolError("unexpected EOF in PostgreSQL protocol message")
        data.extend(chunk)
    return bytes(data)


def _read_cstring(body: bytes, offset: int) -> tuple[str, int]:
    end = body.find(b"\0", offset)
    if end < 0:
        raise _ProtocolError("unterminated PostgreSQL cstring")
    try:
        value = body[offset:end].decode("utf-8")
    except UnicodeDecodeError as error:
        raise _ProtocolError(f"invalid UTF-8 PostgreSQL cstring: {error}") from error
    return value, end + 1


def _read_i16(body: bytes, offset: int) -> tuple[int, int]:
    if offset + 2 > len(body):
        raise _ProtocolError("truncated PostgreSQL int16")
    return struct.unpack_from("!h", body, offset)[0], offset + 2


def _read_i32(body: bytes, offset: int) -> tuple[int, int]:
    if offset + 4 > len(body):
        raise _ProtocolError("truncated PostgreSQL int32")
    return struct.unpack_from("!i", body, offset)[0], offset + 4


def _read_u32(body: bytes, offset: int) -> tuple[int, int]:
    if offset + 4 > len(body):
        raise _ProtocolError("truncated PostgreSQL uint32")
    return struct.unpack_from("!I", body, offset)[0], offset + 4


def _message_authentication_ok() -> bytes:
    return encode_protocol_message("R", struct.pack("!I", 0))


def _message_parameter_status(name: str, value: str) -> bytes:
    return encode_protocol_message("S", encode_cstring(name) + encode_cstring(value))


def _message_backend_key_data(process_id: int, secret_key: int) -> bytes:
    return encode_protocol_message("K", struct.pack("!ii", process_id, secret_key))


def _message_ready(status: str) -> bytes:
    return encode_protocol_message("Z", status.encode("ascii"))


def _message_parameter_description(oids: Sequence[int]) -> bytes:
    body = bytearray(struct.pack("!h", len(oids)))
    for oid in oids:
        body.extend(struct.pack("!I", oid))
    return encode_protocol_message("t", bytes(body))


def _message_row_description(
    columns: Sequence[_Column],
    formats: Sequence[int] | None = None,
) -> bytes:
    body = bytearray(struct.pack("!h", len(columns)))
    for index, column in enumerate(columns):
        if formats is None or not formats:
            format_code = 0
        elif len(formats) == 1:
            format_code = formats[0]
        elif len(formats) == len(columns):
            format_code = formats[index]
        else:
            raise _ProtocolError("invalid PostgreSQL result format count")
        body.extend(encode_cstring(column.name))
        body.extend(struct.pack("!IhIhih", 0, 0, column.oid, column.type_size, -1, format_code))
    return encode_protocol_message("T", bytes(body))


def _message_data_row(
    columns: Sequence[_Column],
    row: Sequence[object | None],
    formats: Sequence[int],
) -> bytes:
    if len(columns) != len(row):
        raise PostgresFixtureError("fixture result row does not match RowDescription")
    body = bytearray(struct.pack("!h", len(row)))
    for index, (column, value) in enumerate(zip(columns, row)):
        if value is None:
            body.extend(struct.pack("!i", -1))
            continue
        if not formats:
            format_code = 0
        elif len(formats) == 1:
            format_code = formats[0]
        elif len(formats) == len(columns):
            format_code = formats[index]
        else:
            raise _ProtocolError("invalid PostgreSQL result format count")
        encoded = _encode_value(column.oid, value, format_code)
        body.extend(struct.pack("!i", len(encoded)))
        body.extend(encoded)
    return encode_protocol_message("D", bytes(body))


def _encode_value(oid: int, value: object, format_code: int) -> bytes:
    if format_code == 0:
        if oid == OID_BOOL:
            return ("t" if bool(value) else "f").encode("ascii")
        return str(value).encode("utf-8")
    if format_code != 1:
        raise _ProtocolError(f"unsupported PostgreSQL format code: {format_code}")
    if oid == OID_BOOL:
        return b"\x01" if bool(value) else b"\x00"
    if oid == OID_INT4:
        return struct.pack("!i", int(value))
    if oid == OID_INT8:
        return struct.pack("!q", int(value))
    if oid == OID_OID:
        return struct.pack("!I", int(value))
    if oid in _TEXTLIKE_OIDS:
        return str(value).encode("utf-8")
    raise _ProtocolError(f"unsupported PostgreSQL binary result OID: {oid}")


def _message_command_complete(tag: str) -> bytes:
    return encode_protocol_message("C", encode_cstring(tag))


def _message_error(message: str, sqlstate: str = "0A000") -> bytes:
    body = bytearray()
    for code, value in (("S", "ERROR"), ("V", "ERROR"), ("C", sqlstate), ("M", message)):
        body.extend(code.encode("ascii"))
        body.extend(encode_cstring(value))
    body.append(0)
    return encode_protocol_message("E", bytes(body))


def _split_sql_statements(sql: str) -> list[str]:
    statements: list[str] = []
    start = 0
    index = 0
    quote: str | None = None
    line_comment = False
    block_depth = 0
    while index < len(sql):
        ch = sql[index]
        nxt = sql[index + 1] if index + 1 < len(sql) else ""
        if line_comment:
            if ch == "\n":
                line_comment = False
            index += 1
            continue
        if block_depth:
            if ch == "/" and nxt == "*":
                block_depth += 1
                index += 2
                continue
            if ch == "*" and nxt == "/":
                block_depth -= 1
                index += 2
                continue
            index += 1
            continue
        if quote == "'":
            if ch == "'" and nxt == "'":
                index += 2
                continue
            if ch == "'":
                quote = None
            index += 1
            continue
        if quote == '"':
            if ch == '"' and nxt == '"':
                index += 2
                continue
            if ch == '"':
                quote = None
            index += 1
            continue
        if ch == "-" and nxt == "-":
            line_comment = True
            index += 2
            continue
        if ch == "/" and nxt == "*":
            block_depth = 1
            index += 2
            continue
        if ch in {"'", '"'}:
            quote = ch
            index += 1
            continue
        if ch == ";":
            statement = sql[start:index].strip()
            if statement:
                statements.append(statement)
            start = index + 1
        index += 1
    tail = sql[start:].strip()
    if tail:
        statements.append(tail)
    return statements


def _references_fixture_table(lower: str) -> bool:
    return bool(re.search(r"\bpublic\s*\.\s*\"?pgo_items\"?(?=\s|$|[.,)])", lower))


def _classify_sql(sql: str) -> str | None:
    normalized = normalize_sql(sql)
    lower = normalized.lower()
    if lower == "begin":
        return "begin"
    if lower == "rollback":
        return "rollback"
    if lower == "commit":
        return "commit"
    if lower.startswith("set local "):
        return "set_local"
    if lower.startswith("select version(), current_database()"):
        return "connection_test"
    if " from pg_database " in f" {lower} " and "has_database_privilege" in lower:
        return "list_databases"
    if (
        "select c.relname, c.relkind = 'p'" in lower
        and "from pg_class as c" in lower
        and "join pg_namespace as n" in lower
    ):
        return "list_public_tables"
    if (
        "pg_get_expr(ad.adbin, ad.adrelid)" in lower
        and "join pg_attribute a" in lower
        and "as is_primary_key" in lower
    ):
        return "table_metadata"
    if "from pg_enum e" in lower and "e.enumtypid = any" in lower:
        return "enum_values"
    if "from pg_constraint con" in lower and "pg_get_constraintdef" in lower:
        return "table_constraints"
    if "from pg_index x" in lower and "pg_get_indexdef" in lower:
        return "table_indexes"
    if (
        lower.startswith("select c.relname, a.attname, pg_catalog.format_type")
        and "join pg_attribute a" in lower
    ):
        return "completion_columns"
    if lower.startswith("select distinct e.enumlabel") and "from pg_type t join pg_enum e" in lower:
        return "completion_enums"
    if lower.startswith("select distinct p.proname") and "from pg_proc p" in lower:
        return "completion_functions"
    if lower.startswith("select distinct oprname from pg_operator"):
        return "completion_operators"
    if lower.startswith("select count(*)::int8 from public.") and _references_fixture_table(lower):
        return "table_count"
    if (
        lower.startswith("select ")
        and "__rriter_source.xmin::text as __rriter_xmin" in lower
        and _references_fixture_table(lower)
    ):
        return "table_chunk"
    if lower.startswith("explain (verbose, format text) ") and _references_fixture_table(lower):
        return "explain"
    if lower.startswith("update public.") and _references_fixture_table(lower) and " returning " in lower:
        return "update_returning"
    if lower.startswith("select ") and _references_fixture_table(lower):
        return "user_select"
    return None


def _shape_for_sql(sql: str) -> _StatementShape:
    family = _classify_sql(sql)
    if family is None:
        raise _UnsupportedSql("unsupported SQL family")
    if family == "connection_test":
        return _StatementShape(
            family,
            (),
            (_Column("version", OID_TEXT, -1), _Column("current_database", OID_NAME, 64)),
        )
    if family == "list_databases":
        return _StatementShape(family, (), (_Column("datname", OID_NAME, 64),))
    if family == "list_public_tables":
        return _StatementShape(
            family,
            (),
            (_Column("relname", OID_NAME, 64), _Column("?column?", OID_BOOL, 1)),
        )
    if family == "table_metadata":
        return _StatementShape(
            family,
            (OID_NAME,),
            (
                _Column("attnum", OID_INT4, 4),
                _Column("attname", OID_NAME, 64),
                _Column("format_type", OID_TEXT, -1),
                _Column("atttypid", OID_INT8, 8),
                _Column("?column?", OID_BOOL, 1),
                _Column("pg_get_expr", OID_TEXT, -1),
                _Column("attidentity", OID_TEXT, -1),
                _Column("attgenerated", OID_TEXT, -1),
                _Column("is_primary_key", OID_BOOL, 1),
                _Column("typtype", OID_TEXT, -1),
            ),
        )
    if family == "enum_values":
        return _StatementShape(
            family,
            (OID_OID_ARRAY,),
            (_Column("enumtypid", OID_INT8, 8), _Column("enumlabel", OID_NAME, 64)),
        )
    if family == "table_constraints":
        return _StatementShape(
            family,
            (OID_NAME,),
            (_Column("conname", OID_NAME, 64), _Column("pg_get_constraintdef", OID_TEXT, -1)),
        )
    if family == "table_indexes":
        return _StatementShape(
            family,
            (OID_NAME,),
            (_Column("relname", OID_NAME, 64), _Column("pg_get_indexdef", OID_TEXT, -1)),
        )
    if family == "completion_columns":
        return _StatementShape(
            family,
            (),
            (
                _Column("relname", OID_NAME, 64),
                _Column("attname", OID_NAME, 64),
                _Column("format_type", OID_TEXT, -1),
            ),
        )
    if family in {"completion_enums", "completion_functions", "completion_operators"}:
        name = {
            "completion_enums": "enumlabel",
            "completion_functions": "proname",
            "completion_operators": "oprname",
        }[family]
        return _StatementShape(family, (), (_Column(name, OID_NAME, 64),))
    if family == "table_count":
        return _StatementShape(family, (), (_Column("count", OID_INT8, 8),))
    if family == "table_chunk":
        return _StatementShape(
            family,
            (),
            (
                _Column("id", OID_TEXT, -1),
                _Column("name", OID_TEXT, -1),
                _Column("active", OID_TEXT, -1),
                _Column("__rriter_xmin", OID_TEXT, -1),
            ),
        )
    if family == "user_select":
        return _StatementShape(
            family,
            (),
            (
                _Column("id", OID_INT4, 4),
                _Column("name", OID_TEXT, -1),
                _Column("active", OID_BOOL, 1),
            ),
        )
    if family == "explain":
        return _StatementShape(family, (), (_Column("QUERY PLAN", OID_TEXT, -1),))
    if family == "update_returning":
        highest = max((int(value) for value in re.findall(r"\$(\d+)", sql)), default=0)
        return _StatementShape(
            family,
            tuple(OID_TEXT for _ in range(highest)),
            (
                _Column("id", OID_TEXT, -1),
                _Column("name", OID_TEXT, -1),
                _Column("active", OID_TEXT, -1),
                _Column("__rriter_xmin", OID_TEXT, -1),
            ),
        )
    return _StatementShape(family, (), ())


class LocalPostgresFixture:
    """Loopback PostgreSQL protocol fixture with per-connection deterministic state."""

    def __init__(
        self,
        *,
        database_name: str = PGO_DATABASE_NAME,
        username: str = PGO_DATABASE_USER,
        row_count: int = PGO_ROW_COUNT,
    ) -> None:
        if row_count < 1:
            raise ValueError("PostgreSQL fixture needs at least one row")
        self.database_name = database_name
        self.username = username
        self._base_rows = tuple(
            _PgoItem(
                id=index,
                name=f"pgo-item-{index:03d}",
                active=index % 3 != 0,
                xmin=str(5_000 + index),
            )
            for index in range(1, row_count + 1)
        )
        self._lock = threading.Lock()
        self._stop_event = threading.Event()
        self._listener: socket.socket | None = None
        self._accept_thread: threading.Thread | None = None
        self._clients: set[socket.socket] = set()
        self._workers: set[threading.Thread] = set()
        self._accepted_connections = 0
        self._startup_count = 0
        self._ssl_request_count = 0
        self._sql_statements: list[str] = []
        self._sql_families: list[str] = []
        self._family_counts: collections.Counter[str] = collections.Counter()
        self._protocol_errors: list[str] = []
        self._unexpected_sql: list[str] = []
        self._peer_disconnects: list[str] = []
        self._worker_errors: list[str] = []

    def start(self) -> "LocalPostgresFixture":
        with self._lock:
            if self._listener is not None:
                raise PostgresFixtureError("PostgreSQL fixture is already running")
            self._stop_event.clear()
            listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(("127.0.0.1", 0))
            listener.listen(16)
            listener.settimeout(0.2)
            self._listener = listener
            thread = threading.Thread(
                target=self._accept_loop,
                name="rriter-pgo-postgres-accept",
                daemon=False,
            )
            self._accept_thread = thread
            thread.start()
        return self

    @property
    def endpoint(self) -> tuple[str, int]:
        with self._lock:
            listener = self._listener
            if listener is None:
                raise PostgresFixtureError("PostgreSQL fixture is not running")
            host, port = listener.getsockname()[:2]
        return str(host), int(port)

    def telemetry(self) -> PostgresFixtureTelemetry:
        with self._lock:
            return PostgresFixtureTelemetry(
                accepted_connection_count=self._accepted_connections,
                startup_count=self._startup_count,
                ssl_request_count=self._ssl_request_count,
                sql_statements=tuple(self._sql_statements),
                sql_families=tuple(self._sql_families),
                family_counts=tuple(sorted(self._family_counts.items())),
                protocol_errors=tuple(self._protocol_errors),
                unexpected_sql=tuple(self._unexpected_sql),
                peer_disconnects=tuple(self._peer_disconnects),
                worker_errors=tuple(self._worker_errors),
            )

    def assert_healthy(self) -> None:
        telemetry = self.telemetry()
        problems = []
        if telemetry.worker_errors:
            problems.append(f"worker errors={telemetry.worker_errors}")
        if telemetry.protocol_errors:
            problems.append(f"protocol errors={telemetry.protocol_errors}")
        if telemetry.unexpected_sql:
            problems.append(f"unexpected SQL={telemetry.unexpected_sql}")
        if problems:
            raise PostgresFixtureError("PostgreSQL fixture unhealthy: " + "; ".join(problems))

    def stop(self) -> None:
        with self._lock:
            listener = self._listener
            accept_thread = self._accept_thread
            if listener is None and accept_thread is None:
                return
            self._stop_event.set()
            self._listener = None
            self._accept_thread = None
        if listener is not None:
            try:
                listener.close()
            except OSError:
                pass
        if accept_thread is not None:
            accept_thread.join(timeout=5)
            if accept_thread.is_alive():
                with self._lock:
                    self._worker_errors.append("accept thread did not stop within 5 seconds")
        with self._lock:
            clients = list(self._clients)
        for client in clients:
            try:
                client.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            try:
                client.close()
            except OSError:
                pass
        with self._lock:
            workers = list(self._workers)
        for worker in workers:
            worker.join(timeout=5)
            if worker.is_alive():
                with self._lock:
                    self._worker_errors.append(
                        f"worker thread {worker.name} did not stop within 5 seconds"
                    )
        with self._lock:
            self._clients.clear()
            self._workers = {worker for worker in self._workers if worker.is_alive()}

    def __enter__(self) -> "LocalPostgresFixture":
        return self.start()

    def __exit__(self, _exc_type: object, _exc: object, _tb: object) -> None:
        self.stop()

    def _accept_loop(self) -> None:
        while not self._stop_event.is_set():
            with self._lock:
                listener = self._listener
            if listener is None:
                return
            try:
                client, _address = listener.accept()
            except socket.timeout:
                continue
            except OSError as error:
                if not self._stop_event.is_set():
                    self._record_worker_error(f"accept failed: {error}")
                return
            with self._lock:
                self._accepted_connections += 1
                connection_id = self._accepted_connections
                self._clients.add(client)
            worker = threading.Thread(
                target=self._worker_entry,
                args=(client, connection_id),
                name=f"rriter-pgo-postgres-{connection_id}",
                daemon=False,
            )
            with self._lock:
                self._workers.add(worker)
            worker.start()

    def _worker_entry(self, client: socket.socket, connection_id: int) -> None:
        try:
            self._serve_connection(client, connection_id)
        except _ProtocolError as error:
            if not self._stop_event.is_set():
                self._record_protocol_error(f"connection {connection_id}: {error}")
        except (ConnectionError, OSError) as error:
            if not self._stop_event.is_set():
                if _is_peer_disconnect_error(error):
                    self._record_peer_disconnect(
                        f"connection {connection_id}: {type(error).__name__}: {error}"
                    )
                else:
                    self._record_worker_error(f"connection {connection_id} I/O: {error}")
        except Exception as error:  # noqa: BLE001 - worker failures must reach telemetry
            self._record_worker_error(
                f"connection {connection_id}: {type(error).__name__}: {error}"
            )
        finally:
            try:
                client.close()
            except OSError:
                pass
            current = threading.current_thread()
            with self._lock:
                self._clients.discard(client)
                self._workers.discard(current)

    def _serve_connection(self, client: socket.socket, connection_id: int) -> None:
        session = _FixtureSession(self, client, connection_id)
        if session.startup():
            session.run()

    def _record_startup(self) -> None:
        with self._lock:
            self._startup_count += 1

    def _record_ssl_request(self) -> None:
        with self._lock:
            self._ssl_request_count += 1

    def _record_sql(self, sql: str, family: str) -> None:
        normalized = normalize_sql(sql)
        with self._lock:
            self._sql_statements.append(normalized)
            self._sql_families.append(family)
            self._family_counts[family] += 1

    def _record_unexpected_sql(self, sql: str) -> None:
        normalized = normalize_sql(sql)
        with self._lock:
            self._unexpected_sql.append(normalized)

    def _record_protocol_error(self, message: str) -> None:
        with self._lock:
            self._protocol_errors.append(message)

    def _record_peer_disconnect(self, message: str) -> None:
        with self._lock:
            self._peer_disconnects.append(message)

    def _record_worker_error(self, message: str) -> None:
        with self._lock:
            self._worker_errors.append(message)


class _FixtureSession:
    def __init__(
        self,
        fixture: LocalPostgresFixture,
        client: socket.socket,
        connection_id: int,
    ) -> None:
        self.fixture = fixture
        self.client = client
        self.connection_id = connection_id
        self.transaction_status = "I"
        self.statements: dict[str, _PreparedStatement] = {}
        self.portals: dict[str, _Portal] = {}
        self.rows = {row.id: row for row in fixture._base_rows}
        self.transaction_snapshot: dict[int, _PgoItem] | None = None
        self.ignore_until_sync = False

    def startup(self) -> bool:
        while True:
            raw_length = _recv_exact(self.client, 4, allow_eof=True)
            if raw_length is None:
                return False
            length = struct.unpack("!I", raw_length)[0]
            if length < 8 or length > MAX_PROTOCOL_MESSAGE_BYTES:
                raise _ProtocolError(f"invalid PostgreSQL startup length: {length}")
            body = _recv_exact(self.client, length - 4)
            assert body is not None
            request_code = struct.unpack_from("!I", body, 0)[0]
            if request_code in {POSTGRES_SSL_REQUEST, POSTGRES_GSSENC_REQUEST}:
                self.fixture._record_ssl_request()
                self.client.sendall(b"N")
                continue
            if request_code != POSTGRES_PROTOCOL_VERSION:
                self.client.sendall(_message_error("unsupported PostgreSQL protocol version", "08P01"))
                return False
            parameters = self._parse_startup_parameters(body[4:])
            if parameters.get("user") != self.fixture.username:
                self.client.sendall(_message_error("invalid fixture user", "28000"))
                return False
            if parameters.get("database") != self.fixture.database_name:
                self.client.sendall(_message_error("invalid fixture database", "3D000"))
                return False
            if parameters.get("client_encoding", "UTF8").upper() != "UTF8":
                self.client.sendall(_message_error("fixture requires UTF8 client_encoding", "22023"))
                return False
            self.fixture._record_startup()
            self.client.sendall(_message_authentication_ok())
            for name, value in (
                ("server_version", "17.0"),
                ("server_encoding", "UTF8"),
                ("client_encoding", "UTF8"),
                ("DateStyle", "ISO, MDY"),
                ("integer_datetimes", "on"),
                ("standard_conforming_strings", "on"),
            ):
                self.client.sendall(_message_parameter_status(name, value))
            self.client.sendall(
                _message_backend_key_data(40_000 + self.connection_id, 90_000 + self.connection_id)
            )
            self.client.sendall(_message_ready("I"))
            return True

    def _parse_startup_parameters(self, body: bytes) -> dict[str, str]:
        output: dict[str, str] = {}
        offset = 0
        while offset < len(body):
            if body[offset] == 0:
                if offset != len(body) - 1:
                    raise _ProtocolError("unexpected bytes after startup terminator")
                return output
            name, offset = _read_cstring(body, offset)
            value, offset = _read_cstring(body, offset)
            output[name] = value
        raise _ProtocolError("startup packet is missing terminator")

    def run(self) -> None:
        while True:
            message = read_protocol_message(self.client)
            if message is None:
                return
            code, body = message
            if code == b"X":
                return
            if self.ignore_until_sync:
                if code == b"S":
                    self.ignore_until_sync = False
                    self.client.sendall(_message_ready(self.transaction_status))
                continue
            if code == b"P":
                self._handle_parse(body)
            elif code == b"D":
                self._handle_describe(body)
            elif code == b"B":
                self._handle_bind(body)
            elif code == b"E":
                self._handle_execute(body)
            elif code == b"S":
                self.client.sendall(_message_ready(self.transaction_status))
            elif code == b"C":
                self._handle_close(body)
            elif code == b"H":
                if body:
                    raise _ProtocolError("Flush message body must be empty")
            elif code == b"Q":
                self._handle_simple_query(body)
            else:
                raise _ProtocolError(f"unsupported frontend message: {code!r}")

    def _handle_parse(self, body: bytes) -> None:
        statement_name, offset = _read_cstring(body, 0)
        sql, offset = _read_cstring(body, offset)
        count, offset = _read_i16(body, offset)
        if count < 0:
            raise _ProtocolError("negative Parse parameter count")
        supplied_oids = []
        for _ in range(count):
            oid, offset = _read_u32(body, offset)
            supplied_oids.append(oid)
        if offset != len(body):
            raise _ProtocolError("trailing bytes in Parse message")
        try:
            shape = _shape_for_sql(sql)
        except _UnsupportedSql:
            self.fixture._record_unexpected_sql(sql)
            self._extended_error(f"unsupported RRiter PGO SQL: {normalize_sql(sql)}")
            return
        if supplied_oids:
            if len(supplied_oids) > len(shape.parameter_oids):
                self._extended_error("too many explicit parameter types", "08P01")
                return
            expected_prefix = shape.parameter_oids[: len(supplied_oids)]
            for supplied, expected in zip(supplied_oids, expected_prefix):
                if supplied not in {0, expected}:
                    self._extended_error("unexpected explicit parameter type", "42804")
                    return
        self.statements[statement_name] = _PreparedStatement(
            sql=sql,
            shape=shape,
        )
        self.client.sendall(encode_protocol_message("1"))

    def _handle_describe(self, body: bytes) -> None:
        if not body:
            raise _ProtocolError("Describe message is empty")
        kind = body[:1]
        name, offset = _read_cstring(body, 1)
        if offset != len(body):
            raise _ProtocolError("trailing bytes in Describe message")
        if kind == b"S":
            statement = self.statements.get(name)
            if statement is None:
                self._extended_error(f"unknown prepared statement {name!r}", "26000")
                return
            self.client.sendall(_message_parameter_description(statement.shape.parameter_oids))
            if statement.shape.columns:
                self.client.sendall(_message_row_description(statement.shape.columns))
            else:
                self.client.sendall(encode_protocol_message("n"))
            return
        if kind == b"P":
            portal = self.portals.get(name)
            if portal is None:
                self._extended_error(f"unknown portal {name!r}", "34000")
                return
            statement = self.statements.get(portal.statement_name)
            if statement is None:
                self._extended_error("portal references missing statement", "26000")
                return
            if statement.shape.columns:
                self.client.sendall(
                    _message_row_description(statement.shape.columns, portal.result_formats)
                )
            else:
                self.client.sendall(encode_protocol_message("n"))
            return
        raise _ProtocolError(f"invalid Describe target: {kind!r}")

    def _handle_bind(self, body: bytes) -> None:
        portal_name, offset = _read_cstring(body, 0)
        statement_name, offset = _read_cstring(body, offset)
        statement = self.statements.get(statement_name)
        if statement is None:
            self._extended_error(f"unknown prepared statement {statement_name!r}", "26000")
            return
        format_count, offset = _read_i16(body, offset)
        if format_count < 0:
            raise _ProtocolError("negative Bind parameter format count")
        parameter_formats = []
        for _ in range(format_count):
            value, offset = _read_i16(body, offset)
            parameter_formats.append(value)
        parameter_count, offset = _read_i16(body, offset)
        if parameter_count < 0:
            raise _ProtocolError("negative Bind parameter count")
        raw_parameters: list[bytes | None] = []
        for _ in range(parameter_count):
            size, offset = _read_i32(body, offset)
            if size == -1:
                raw_parameters.append(None)
                continue
            if size < -1 or offset + size > len(body):
                raise _ProtocolError("invalid Bind parameter length")
            raw_parameters.append(body[offset : offset + size])
            offset += size
        result_format_count, offset = _read_i16(body, offset)
        if result_format_count < 0:
            raise _ProtocolError("negative Bind result format count")
        result_formats = []
        for _ in range(result_format_count):
            value, offset = _read_i16(body, offset)
            if value not in {0, 1}:
                raise _ProtocolError(f"unsupported Bind result format: {value}")
            result_formats.append(value)
        if offset != len(body):
            raise _ProtocolError("trailing bytes in Bind message")
        expected_oids = statement.shape.parameter_oids
        if parameter_count != len(expected_oids):
            self._extended_error(
                f"expected {len(expected_oids)} parameters, got {parameter_count}", "08P01"
            )
            return
        formats = self._expand_formats(parameter_formats, parameter_count, "parameter")
        try:
            parameters = tuple(
                self._decode_parameter(oid, value, formats[index])
                for index, (oid, value) in enumerate(zip(expected_oids, raw_parameters))
            )
        except _UnsupportedSql as error:
            self.fixture._record_unexpected_sql(statement.sql)
            self._extended_error(str(error))
            return
        if result_formats and len(result_formats) not in {1, len(statement.shape.columns)}:
            self._extended_error("invalid result format count", "08P01")
            return
        self.portals[portal_name] = _Portal(
            statement_name=statement_name,
            parameters=parameters,
            result_formats=tuple(result_formats),
        )
        self.client.sendall(encode_protocol_message("2"))

    def _handle_execute(self, body: bytes) -> None:
        portal_name, offset = _read_cstring(body, 0)
        max_rows, offset = _read_i32(body, offset)
        if offset != len(body):
            raise _ProtocolError("trailing bytes in Execute message")
        if max_rows != 0:
            self._extended_error("fixture does not implement portal row suspension")
            return
        portal = self.portals.get(portal_name)
        if portal is None:
            self._extended_error(f"unknown portal {portal_name!r}", "34000")
            return
        statement = self.statements.get(portal.statement_name)
        if statement is None:
            self._extended_error("portal references missing statement", "26000")
            return
        try:
            result = self._execute_statement(
                statement.sql,
                statement.shape,
                portal.parameters,
            )
        except _UnsupportedSql as error:
            self.fixture._record_unexpected_sql(statement.sql)
            self._extended_error(str(error))
            return
        self.fixture._record_sql(statement.sql, statement.shape.family)
        for row in result.rows:
            self.client.sendall(
                _message_data_row(result.columns, row, portal.result_formats)
            )
        self.client.sendall(_message_command_complete(result.command_tag))

    def _handle_close(self, body: bytes) -> None:
        if not body:
            raise _ProtocolError("Close message is empty")
        kind = body[:1]
        name, offset = _read_cstring(body, 1)
        if offset != len(body):
            raise _ProtocolError("trailing bytes in Close message")
        if kind == b"S":
            self.statements.pop(name, None)
            for portal_name, portal in list(self.portals.items()):
                if portal.statement_name == name:
                    self.portals.pop(portal_name, None)
        elif kind == b"P":
            self.portals.pop(name, None)
        else:
            raise _ProtocolError(f"invalid Close target: {kind!r}")
        self.client.sendall(encode_protocol_message("3"))

    def _handle_simple_query(self, body: bytes) -> None:
        sql, offset = _read_cstring(body, 0)
        if offset != len(body):
            raise _ProtocolError("trailing bytes in Query message")
        statements = _split_sql_statements(sql)
        if not statements:
            self.client.sendall(encode_protocol_message("I"))
            self.client.sendall(_message_ready(self.transaction_status))
            return
        for statement_sql in statements:
            try:
                shape = _shape_for_sql(statement_sql)
                result = self._execute_statement(statement_sql, shape, ())
            except _UnsupportedSql as error:
                self.fixture._record_unexpected_sql(statement_sql)
                self._simple_error(str(error))
                return
            self.fixture._record_sql(statement_sql, shape.family)
            if result.columns:
                self.client.sendall(_message_row_description(result.columns))
                for row in result.rows:
                    self.client.sendall(_message_data_row(result.columns, row, ()))
            self.client.sendall(_message_command_complete(result.command_tag))
        self.client.sendall(_message_ready(self.transaction_status))

    def _execute_statement(
        self,
        sql: str,
        shape: _StatementShape,
        parameters: Sequence[object | None],
    ) -> _ExecutionResult:
        family = shape.family
        if family == "rollback":
            if self.transaction_snapshot is not None:
                self.rows = dict(self.transaction_snapshot)
            self.transaction_snapshot = None
            self.transaction_status = "I"
            return _ExecutionResult((), (), "ROLLBACK")
        if family == "commit":
            if self.transaction_status == "E" and self.transaction_snapshot is not None:
                self.rows = dict(self.transaction_snapshot)
            self.transaction_snapshot = None
            self.transaction_status = "I"
            return _ExecutionResult((), (), "COMMIT")
        if self.transaction_status == "E":
            raise _UnsupportedSql("current transaction is aborted; ROLLBACK is required")
        if family == "begin":
            if self.transaction_status != "I":
                raise _UnsupportedSql("nested BEGIN is not supported by the fixture")
            self.transaction_snapshot = dict(self.rows)
            self.transaction_status = "T"
            return _ExecutionResult((), (), "BEGIN")
        if family == "set_local":
            if self.transaction_status != "T":
                raise _UnsupportedSql("SET LOCAL requires an active transaction")
            return _ExecutionResult((), (), "SET")
        if family == "connection_test":
            rows = (("PostgreSQL 17.0 RRiter deterministic PGO fixture", self.fixture.database_name),)
            return _ExecutionResult(shape.columns, rows, "SELECT 1")
        if family == "list_databases":
            rows = ((self.fixture.database_name,),)
            return _ExecutionResult(shape.columns, rows, "SELECT 1")
        if family == "list_public_tables":
            rows = ((PGO_TABLE_NAME, False),)
            return _ExecutionResult(shape.columns, rows, "SELECT 1")
        if family == "table_metadata":
            table_name = self._required_text_parameter(parameters, 0)
            rows: tuple[tuple[object | None, ...], ...] = ()
            if table_name == PGO_TABLE_NAME:
                rows = (
                    (1, "id", "integer", OID_INT4, False, None, "", "", True, "b"),
                    (2, "name", "text", OID_TEXT, False, None, "", "", False, "b"),
                    (3, "active", "boolean", OID_BOOL, False, None, "", "", False, "b"),
                )
            return _ExecutionResult(shape.columns, rows, f"SELECT {len(rows)}")
        if family == "enum_values":
            return _ExecutionResult(shape.columns, (), "SELECT 0")
        if family == "table_constraints":
            table_name = self._required_text_parameter(parameters, 0)
            rows = (
                (("pgo_items_pkey", "PRIMARY KEY (id)"),)
                if table_name == PGO_TABLE_NAME
                else ()
            )
            return _ExecutionResult(shape.columns, rows, f"SELECT {len(rows)}")
        if family == "table_indexes":
            self._required_text_parameter(parameters, 0)
            return _ExecutionResult(shape.columns, (), "SELECT 0")
        if family == "completion_columns":
            rows = (
                (PGO_TABLE_NAME, "id", "integer"),
                (PGO_TABLE_NAME, "name", "text"),
                (PGO_TABLE_NAME, "active", "boolean"),
            )
            return _ExecutionResult(shape.columns, rows, f"SELECT {len(rows)}")
        if family == "completion_enums":
            return _ExecutionResult(shape.columns, (), "SELECT 0")
        if family == "completion_functions":
            rows = tuple((name,) for name in ("coalesce", "count", "lower", "upper"))
            return _ExecutionResult(shape.columns, rows, f"SELECT {len(rows)}")
        if family == "completion_operators":
            rows = tuple((name,) for name in ("=", "<", ">", "+"))
            return _ExecutionResult(shape.columns, rows, f"SELECT {len(rows)}")
        if family == "table_count":
            items = self._table_items_for_sql(sql, apply_limit=False)
            return _ExecutionResult(shape.columns, ((len(items),),), "SELECT 1")
        if family == "table_chunk":
            items = self._table_items_for_sql(sql, apply_limit=True)
            rows = tuple(
                (str(item.id), item.name, "true" if item.active else "false", item.xmin)
                for item in items
            )
            return _ExecutionResult(shape.columns, rows, f"SELECT {len(rows)}")
        if family == "user_select":
            items = self._user_select_items(sql)
            rows = tuple((item.id, item.name, item.active) for item in items)
            return _ExecutionResult(shape.columns, rows, f"SELECT {len(rows)}")
        if family == "explain":
            rows = (
                ("Limit  (cost=0.00..1.80 rows=64 width=37)",),
                ("  ->  Index Scan using pgo_items_pkey on pgo_items",),
                ("        Order By: id",),
            )
            return _ExecutionResult(shape.columns, rows, "EXPLAIN")
        if family == "update_returning":
            return self._execute_update(sql, shape, parameters)
        raise _UnsupportedSql(f"fixture does not execute SQL family {family}")

    def _execute_update(
        self,
        sql: str,
        shape: _StatementShape,
        parameters: Sequence[object | None],
    ) -> _ExecutionResult:
        if self.transaction_status != "T":
            raise _UnsupportedSql("fixture UPDATE requires an active transaction")
        normalized = normalize_sql(sql)
        match = re.search(
            r"set\s+\"?name\"?\s*=\s*\$(\d+)::text::text\s+"
            r"where\s+\"?id\"?\s*=\s*\$(\d+)::text::integer\s+"
            r"and\s+xmin\s*=\s*\$(\d+)::text::xid\s+returning\s+",
            normalized,
            flags=re.IGNORECASE,
        )
        if match is None:
            raise _UnsupportedSql(
                "fixture supports only name UPDATE with PK+xmin and RETURNING"
            )
        name_index, id_index, xmin_index = (int(value) - 1 for value in match.groups())
        name = self._required_text_parameter(parameters, name_index)
        row_id_text = self._required_text_parameter(parameters, id_index)
        xmin = self._required_text_parameter(parameters, xmin_index)
        try:
            row_id = int(row_id_text)
        except ValueError as error:
            raise _UnsupportedSql("fixture UPDATE id parameter is not integer") from error
        current = self.rows.get(row_id)
        if current is None or current.xmin != xmin:
            return _ExecutionResult(shape.columns, (), "UPDATE 0")
        try:
            new_xmin = str(int(current.xmin) + 100_000)
        except ValueError:
            new_xmin = f"9{current.id:09d}"
        updated = replace(current, name=name, xmin=new_xmin)
        self.rows[row_id] = updated
        row = (str(updated.id), updated.name, "true" if updated.active else "false", updated.xmin)
        return _ExecutionResult(shape.columns, (row,), "UPDATE 1")

    def _table_items_for_sql(self, sql: str, *, apply_limit: bool) -> list[_PgoItem]:
        normalized = normalize_sql(sql)
        items = list(self.rows.values())
        where = re.search(
            r"\bwhere\b\s+(.+?)(?=\s+order\s+by\b|\s+offset\b|\s+limit\b|$)",
            normalized,
            flags=re.IGNORECASE,
        )
        if where is not None:
            items = self._apply_where(items, where.group(1))
        order = re.search(
            r"\border\s+by\s+(.+?)(?=\s+offset\b|\s+limit\b|$)",
            normalized,
            flags=re.IGNORECASE,
        )
        if order is not None:
            items = self._apply_order(items, order.group(1))
        if not apply_limit:
            return items
        offset_match = re.search(r"\boffset\s+(\d+)", normalized, flags=re.IGNORECASE)
        limit_match = re.search(r"\blimit\s+(\d+)", normalized, flags=re.IGNORECASE)
        offset = int(offset_match.group(1)) if offset_match else 0
        limit = int(limit_match.group(1)) if limit_match else len(items)
        return items[offset : offset + limit]

    def _apply_where(self, items: list[_PgoItem], expression: str) -> list[_PgoItem]:
        simplified = expression.lower().replace("__rriter_source.", "").replace('"', "").strip()
        bool_match = re.fullmatch(r"active\s*=\s*(true|false)", simplified)
        if bool_match:
            wanted = bool_match.group(1) == "true"
            return [item for item in items if item.active == wanted]
        id_match = re.fullmatch(r"id\s*(=|>=|<=|>|<)\s*(\d+)", simplified)
        if id_match:
            operator, raw_value = id_match.groups()
            value = int(raw_value)
            operations = {
                "=": lambda item: item.id == value,
                ">=": lambda item: item.id >= value,
                "<=": lambda item: item.id <= value,
                ">": lambda item: item.id > value,
                "<": lambda item: item.id < value,
            }
            predicate = operations[operator]
            return [item for item in items if predicate(item)]
        raise _UnsupportedSql(f"unsupported fixture table WHERE: {expression}")

    def _apply_order(self, items: list[_PgoItem], expression: str) -> list[_PgoItem]:
        simplified = expression.lower().replace("__rriter_source.", "").replace('"', "").strip()
        match = re.fullmatch(r"(id|name|active)(?:\s+(asc|desc))?", simplified)
        if match is None:
            raise _UnsupportedSql(f"unsupported fixture table ORDER BY: {expression}")
        column, direction = match.groups()
        key = {
            "id": lambda item: item.id,
            "name": lambda item: item.name,
            "active": lambda item: item.active,
        }[column]
        return sorted(items, key=key, reverse=direction == "desc")

    def _user_select_items(self, sql: str) -> list[_PgoItem]:
        normalized = normalize_sql(sql)
        lower = normalized.lower().replace('"', "")
        if not re.search(r"select\s+id\s*,\s*name\s*,\s*active\s+from\s+public\.pgo_items", lower):
            raise _UnsupportedSql(
                "fixture user SELECT must request id, name, active from public.pgo_items"
            )
        items = list(self.rows.values())
        order_match = re.search(r"\border\s+by\s+(id|name|active)(?:\s+(asc|desc))?", lower)
        if order_match:
            column, direction = order_match.groups()
            key = {
                "id": lambda item: item.id,
                "name": lambda item: item.name,
                "active": lambda item: item.active,
            }[column]
            items.sort(key=key, reverse=direction == "desc")
        limit_match = re.search(r"\blimit\s+(\d+)", lower)
        if limit_match:
            items = items[: int(limit_match.group(1))]
        return items

    def _decode_parameter(self, oid: int, value: bytes | None, format_code: int) -> object | None:
        if value is None:
            return None
        if format_code not in {0, 1}:
            raise _ProtocolError(f"unsupported Bind parameter format: {format_code}")
        if oid in _TEXTLIKE_OIDS:
            try:
                return value.decode("utf-8")
            except UnicodeDecodeError as error:
                raise _ProtocolError(f"invalid UTF-8 Bind parameter: {error}") from error
        if oid == OID_OID_ARRAY:
            return self._decode_oid_array_parameter(value, format_code)
        raise _ProtocolError(f"unsupported Bind parameter OID: {oid}")

    def _decode_oid_array_parameter(self, value: bytes, format_code: int) -> tuple[int, ...]:
        if format_code == 0:
            try:
                text = value.decode("ascii").strip()
            except UnicodeDecodeError as error:
                raise _ProtocolError(f"invalid OID[] text parameter: {error}") from error
            if text == "{}":
                return ()
            if not (text.startswith("{") and text.endswith("}")):
                raise _ProtocolError("invalid OID[] text parameter")
            try:
                return tuple(int(part) for part in text[1:-1].split(",") if part)
            except ValueError as error:
                raise _ProtocolError("invalid OID[] text element") from error

        if len(value) < 12:
            raise _ProtocolError("truncated OID[] binary parameter")
        dimensions, has_null, element_oid = struct.unpack_from("!iiI", value, 0)
        if element_oid != OID_OID:
            raise _ProtocolError(f"OID[] binary parameter has element OID {element_oid}")
        if has_null not in {0, 1}:
            raise _ProtocolError("invalid OID[] binary null flag")
        if dimensions == 0:
            if len(value) != 12:
                raise _ProtocolError("trailing bytes in empty OID[] binary parameter")
            return ()
        if dimensions != 1 or len(value) < 20:
            raise _ProtocolError("fixture supports only one-dimensional OID[] parameters")
        length, _lower_bound = struct.unpack_from("!ii", value, 12)
        if length < 0:
            raise _ProtocolError("negative OID[] dimension length")
        offset = 20
        items: list[int] = []
        for _ in range(length):
            if offset + 4 > len(value):
                raise _ProtocolError("truncated OID[] binary element length")
            item_size = struct.unpack_from("!i", value, offset)[0]
            offset += 4
            if item_size != 4 or offset + item_size > len(value):
                raise _ProtocolError("invalid OID[] binary element")
            items.append(struct.unpack_from("!I", value, offset)[0])
            offset += item_size
        if offset != len(value):
            raise _ProtocolError("trailing bytes in OID[] binary parameter")
        return tuple(items)

    def _required_text_parameter(
        self,
        parameters: Sequence[object | None],
        index: int,
    ) -> str:
        if index >= len(parameters) or parameters[index] is None:
            raise _UnsupportedSql(f"missing text parameter ${index + 1}")
        value = parameters[index]
        if not isinstance(value, str):
            raise _UnsupportedSql(f"parameter ${index + 1} is not text")
        return value

    def _expand_formats(self, formats: Sequence[int], count: int, label: str) -> tuple[int, ...]:
        if not formats:
            return tuple(0 for _ in range(count))
        if len(formats) == 1:
            if formats[0] not in {0, 1}:
                raise _ProtocolError(f"unsupported {label} format: {formats[0]}")
            return tuple(formats[0] for _ in range(count))
        if len(formats) != count:
            raise _ProtocolError(f"invalid {label} format count")
        if any(value not in {0, 1} for value in formats):
            raise _ProtocolError(f"unsupported {label} format")
        return tuple(formats)

    def _extended_error(self, message: str, sqlstate: str = "0A000") -> None:
        if self.transaction_status == "T":
            self.transaction_status = "E"
        self.client.sendall(_message_error(message, sqlstate))
        self.ignore_until_sync = True

    def _simple_error(self, message: str, sqlstate: str = "0A000") -> None:
        if self.transaction_status == "T":
            self.transaction_status = "E"
        self.client.sendall(_message_error(message, sqlstate))
        self.client.sendall(_message_ready(self.transaction_status))
