use super::database_ssh::{
    SshBackendKind, SshConnectOptions, SystemSshTunnel, select_ssh_backend,
    start_system_ssh_tunnel_cancelable,
};
use super::database_ssh_builtin::{BuiltinSshStream, DatabaseIoStream, connect_builtin_ssh};
use super::{
    DatabaseConnectionConfig, DatabaseSecretBundle, DatabaseSettings, PostgresTlsMode,
    MAX_DATABASES_PER_CONNECTION, MAX_PUBLIC_TABLES_PER_DATABASE,
};
use std::fmt;
use std::io;
use std::sync::{OnceLock, mpsc as std_mpsc};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::config::SslMode;
use tokio_postgres::{AsyncMessage, Client, Config, Connection, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;

const TEST_CONNECTION_SQL: &str = "SELECT version(), current_database()";
const LIST_DATABASES_SQL: &str = "SELECT datname\n\
     FROM pg_database\n\
     WHERE datallowconn\n\
       AND NOT datistemplate\n\
       AND has_database_privilege(datname, 'CONNECT')\n\
     ORDER BY datname";
const LIST_PUBLIC_TABLES_SQL: &str = "SELECT c.relname, c.relkind = 'p'\n\
     FROM pg_class AS c\n\
     JOIN pg_namespace AS n ON n.oid = c.relnamespace\n\
     WHERE n.nspname = 'public'\n\
       AND c.relkind IN ('r', 'p')\n\
     ORDER BY c.relname";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseBackendNotice {
    BuiltinSshFallback { reason: String },
    NativeCertificateWarnings { count: usize },
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseServerNotice {
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseConnectionTestResult {
    pub server_version: String,
    pub current_database: String,
    pub ssh_backend: Option<SshBackendKind>,
    pub notices: Vec<DatabaseBackendNotice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseInfo {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseListResult {
    pub databases: Vec<DatabaseInfo>,
    pub ssh_backend: Option<SshBackendKind>,
    pub notices: Vec<DatabaseBackendNotice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableInfo {
    pub name: String,
    pub partitioned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableListResult {
    pub tables: Vec<DatabaseTableInfo>,
    pub ssh_backend: Option<SshBackendKind>,
    pub notices: Vec<DatabaseBackendNotice>,
}

#[derive(Debug)]
pub enum DatabaseBackendError {
    InvalidConfiguration(String),
    Io(io::Error),
    Postgres(tokio_postgres::Error),
    Ssh(super::database_ssh_builtin::DatabaseSshError),
    SshFallback {
        system_error: String,
        builtin_error: super::database_ssh_builtin::DatabaseSshError,
    },
    Timeout(&'static str),
    LimitExceeded(&'static str),
}

impl fmt::Display for DatabaseBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => f.write_str(message),
            Self::Io(error) => write!(f, "{error}"),
            Self::Postgres(error) => write!(f, "PostgreSQL error: {error}"),
            Self::Ssh(error) => write!(f, "{error}"),
            Self::SshFallback {
                system_error,
                builtin_error,
            } => write!(
                f,
                "system OpenSSH failed ({system_error}); built-in SSH fallback failed: {builtin_error}"
            ),
            Self::Timeout(operation) => write!(f, "{operation} timed out"),
            Self::LimitExceeded(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DatabaseBackendError {}

impl From<io::Error> for DatabaseBackendError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tokio_postgres::Error> for DatabaseBackendError {
    fn from(value: tokio_postgres::Error) -> Self {
        Self::Postgres(value)
    }
}

impl From<super::database_ssh_builtin::DatabaseSshError> for DatabaseBackendError {
    fn from(value: super::database_ssh_builtin::DatabaseSshError) -> Self {
        Self::Ssh(value)
    }
}

pub struct PostgresSession {
    pub client: Client,
    driver: JoinHandle<Result<(), tokio_postgres::Error>>,
    _system_tunnel: Option<SystemSshTunnel>,
    pub ssh_backend: Option<SshBackendKind>,
    pub notices: Vec<DatabaseBackendNotice>,
    server_notice_rx: std_mpsc::Receiver<DatabaseServerNotice>,
}

impl PostgresSession {
    pub fn drain_server_notices(&self) -> Vec<DatabaseServerNotice> {
        self.server_notice_rx.try_iter().take(1_024).collect()
    }
}

impl Drop for PostgresSession {
    fn drop(&mut self) {
        self.driver.abort();
    }
}

pub async fn connect_postgres(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<PostgresSession, DatabaseBackendError> {
    connection
        .validate()
        .map_err(|error| DatabaseBackendError::InvalidConfiguration(error.to_string()))?;
    if database_name.trim().is_empty() {
        return Err(DatabaseBackendError::InvalidConfiguration(
            "database name is required".to_string(),
        ));
    }

    let timeout = Duration::from_secs(settings.connect_timeout_seconds);
    let mut system_tunnel = None;
    let mut ssh_backend = None;
    let mut notices = Vec::new();

    let stream: Box<dyn DatabaseIoStream> = if let Some(ssh) = &connection.ssh {
        let selected = select_ssh_backend(ssh, secrets, ssh_options);
        ssh_backend = Some(selected.kind);
        match selected.kind {
            SshBackendKind::System => {
                let executable = selected.executable.ok_or_else(|| {
                    DatabaseBackendError::InvalidConfiguration(
                        "system SSH backend was selected without an executable".to_string(),
                    )
                })?;
                let ssh_config = ssh.clone();
                let pg_host = connection.host.clone();
                let pg_port = connection.port;
                let startup_timeout = Duration::from_secs(settings.ssh_startup_timeout_seconds);
                let cancel = ssh_options.cancel.clone();
                let tunnel_result = tokio::task::spawn_blocking(move || {
                    start_system_ssh_tunnel_cancelable(
                        &executable,
                        &ssh_config,
                        &pg_host,
                        pg_port,
                        startup_timeout,
                        cancel.as_deref(),
                    )
                })
                .await
                .map_err(|error| io::Error::other(format!("system SSH worker failed: {error}")))?;
                match tunnel_result {
                    Ok(tunnel) => {
                        let local_port = tunnel.local_port();
                        let tcp = tokio::time::timeout(
                            timeout,
                            TcpStream::connect(("127.0.0.1", local_port)),
                        )
                        .await
                        .map_err(|_| {
                            DatabaseBackendError::Timeout("PostgreSQL SSH tunnel connect")
                        })??;
                        system_tunnel = Some(tunnel);
                        Box::new(tcp)
                    }
                    Err(system_error) => {
                        if system_error.kind() == io::ErrorKind::Interrupted {
                            return Err(DatabaseBackendError::Io(system_error));
                        }
                        let reason = format!(
                            "системный OpenSSH не смог открыть туннель: {system_error}"
                        );
                        let builtin = tokio::time::timeout(
                            timeout,
                            connect_builtin_ssh(
                                ssh,
                                secrets,
                                ssh_options,
                                &connection.host,
                                connection.port,
                            ),
                        )
                        .await
                        .map_err(|_| DatabaseBackendError::Timeout("built-in SSH fallback"))?
                        .map_err(|builtin_error| DatabaseBackendError::SshFallback {
                            system_error: system_error.to_string(),
                            builtin_error,
                        })?;
                        ssh_backend = Some(SshBackendKind::Builtin);
                        notices.push(DatabaseBackendNotice::BuiltinSshFallback { reason });
                        Box::new(builtin)
                    }
                }
            }
            SshBackendKind::Builtin => {
                if let Some(reason) = selected.reason {
                    notices.push(DatabaseBackendNotice::BuiltinSshFallback { reason });
                }
                let stream: BuiltinSshStream = tokio::time::timeout(
                    timeout,
                    connect_builtin_ssh(
                        ssh,
                        secrets,
                        ssh_options,
                        &connection.host,
                        connection.port,
                    ),
                )
                .await
                .map_err(|_| DatabaseBackendError::Timeout("built-in SSH connect"))??;
                Box::new(stream)
            }
        }
    } else {
        let tcp = tokio::time::timeout(
            timeout,
            TcpStream::connect((connection.host.as_str(), connection.port)),
        )
        .await
        .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL connect"))??;
        Box::new(tcp)
    };

    let mut config = Config::new();
    config
        .host(&connection.host)
        .port(connection.port)
        .user(&connection.username)
        .dbname(database_name)
        .application_name("RRiter Database Tools");
    if let Some(password) = secrets.postgres_password.as_deref() {
        config.password(password.as_bytes());
    }

    let (client, driver, server_notice_rx, certificate_warnings) =
        connect_postgres_stream(config, stream, connection.tls_mode, &connection.host).await?;
    if certificate_warnings > 0 {
        notices.push(DatabaseBackendNotice::NativeCertificateWarnings {
            count: certificate_warnings,
        });
    }
    Ok(PostgresSession {
        client,
        driver,
        _system_tunnel: system_tunnel,
        ssh_backend,
        notices,
        server_notice_rx,
    })
}

async fn connect_postgres_stream(
    mut config: Config,
    stream: Box<dyn DatabaseIoStream>,
    tls_mode: PostgresTlsMode,
    hostname: &str,
) -> Result<
    (
        Client,
        JoinHandle<Result<(), tokio_postgres::Error>>,
        std_mpsc::Receiver<DatabaseServerNotice>,
        usize,
    ),
    DatabaseBackendError,
> {
    match tls_mode {
        PostgresTlsMode::Disable => {
            config.ssl_mode(SslMode::Disable);
            let (client, connection) = config.connect_raw(stream, NoTls).await?;
            let (driver, notice_rx) = spawn_connection_driver(connection);
            Ok((client, driver, notice_rx, 0))
        }
        PostgresTlsMode::Prefer | PostgresTlsMode::Require => {
            install_rustls_provider()?;
            config.ssl_mode(match tls_mode {
                PostgresTlsMode::Prefer => SslMode::Prefer,
                PostgresTlsMode::Require => SslMode::Require,
                PostgresTlsMode::Disable => unreachable!(),
            });
            let (mut make_tls, warnings) = MakeRustlsConnect::with_native_certs().map_err(|errors| {
                DatabaseBackendError::InvalidConfiguration(format!(
                    "no native TLS root certificates could be loaded ({} errors)",
                    errors.len()
                ))
            })?;
            let tls = <MakeRustlsConnect as MakeTlsConnect<Box<dyn DatabaseIoStream>>>::make_tls_connect(
                &mut make_tls,
                hostname,
            )
            .expect("rustls connector construction is infallible");
            let (client, connection) = config.connect_raw(stream, tls).await?;
            let (driver, notice_rx) = spawn_connection_driver(connection);
            Ok((client, driver, notice_rx, warnings.len()))
        }
    }
}

fn spawn_connection_driver<S, T>(
    mut connection: Connection<S, T>,
) -> (
    JoinHandle<Result<(), tokio_postgres::Error>>,
    std_mpsc::Receiver<DatabaseServerNotice>,
)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (notice_tx, notice_rx) = std_mpsc::sync_channel(1_024);
    let driver = tokio::spawn(async move {
        loop {
            match futures_util::future::poll_fn(|cx| connection.poll_message(cx)).await {
                Some(Ok(AsyncMessage::Notice(notice))) => {
                    let _ = notice_tx.try_send(DatabaseServerNotice {
                        severity: notice.severity().to_string(),
                        message: notice.message().to_string(),
                        detail: notice.detail().map(str::to_string),
                        hint: notice.hint().map(str::to_string),
                    });
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error),
                None => return Ok(()),
            }
        }
    });
    (driver, notice_rx)
}

fn install_rustls_provider() -> Result<(), DatabaseBackendError> {
    static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();
    let result = INSTALLED.get_or_init(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .map_err(|_| "a different rustls crypto provider is already installed".to_string())
    });
    result
        .clone()
        .map_err(DatabaseBackendError::InvalidConfiguration)
}

pub async fn test_database_connection(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseConnectionTestResult, DatabaseBackendError> {
    let session = connect_postgres(
        connection,
        secrets,
        &connection.maintenance_database,
        settings,
        ssh_options,
    )
    .await?;
    let row = tokio::time::timeout(
        Duration::from_secs(settings.statement_timeout_seconds),
        session.client.query_one(TEST_CONNECTION_SQL, &[]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL connection test"))??;
    Ok(DatabaseConnectionTestResult {
        server_version: row.get(0),
        current_database: row.get(1),
        ssh_backend: session.ssh_backend,
        notices: session.notices.clone(),
    })
}

pub async fn list_databases(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseListResult, DatabaseBackendError> {
    let session = connect_postgres(
        connection,
        secrets,
        &connection.maintenance_database,
        settings,
        ssh_options,
    )
    .await?;
    // Internal read: intentionally no explicit BEGIN/review transaction.
    let rows = tokio::time::timeout(
        Duration::from_secs(settings.statement_timeout_seconds),
        session.client.query(LIST_DATABASES_SQL, &[]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL database list"))??;
    if rows.len() > MAX_DATABASES_PER_CONNECTION {
        return Err(DatabaseBackendError::LimitExceeded(
            "connection exposes more than 512 databases",
        ));
    }
    Ok(DatabaseListResult {
        databases: rows
            .into_iter()
            .map(|row| DatabaseInfo { name: row.get(0) })
            .collect(),
        ssh_backend: session.ssh_backend,
        notices: session.notices.clone(),
    })
}

pub async fn list_public_tables(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseTableListResult, DatabaseBackendError> {
    let session = connect_postgres(
        connection,
        secrets,
        database_name,
        settings,
        ssh_options,
    )
    .await?;
    // Internal read: intentionally no explicit BEGIN/review transaction.
    let rows = tokio::time::timeout(
        Duration::from_secs(settings.statement_timeout_seconds),
        session.client.query(LIST_PUBLIC_TABLES_SQL, &[]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL public table list"))??;
    if rows.len() > MAX_PUBLIC_TABLES_PER_DATABASE {
        return Err(DatabaseBackendError::LimitExceeded(
            "public schema contains more than 10000 tables",
        ));
    }
    Ok(DatabaseTableListResult {
        tables: rows
            .into_iter()
            .map(|row| DatabaseTableInfo {
                name: row.get(0),
                partitioned: row.get(1),
            })
            .collect(),
        ssh_backend: session.ssh_backend,
        notices: session.notices.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_limit_errors_are_actionable() {
        assert!(DatabaseBackendError::LimitExceeded("too many").to_string().contains("too many"));
    }

    #[test]
    fn internal_read_queries_never_open_explicit_transactions() {
        for query in [TEST_CONNECTION_SQL, LIST_DATABASES_SQL, LIST_PUBLIC_TABLES_SQL] {
            let lower = query.to_ascii_lowercase();
            assert!(!lower.contains("begin"));
            assert!(!lower.contains("commit"));
            assert!(!lower.contains("rollback"));
        }
    }

    #[test]
    fn test_result_can_report_builtin_fallback_without_secrets() {
        let result = DatabaseConnectionTestResult {
            server_version: "PostgreSQL".to_string(),
            current_database: "postgres".to_string(),
            ssh_backend: Some(SshBackendKind::Builtin),
            notices: vec![DatabaseBackendNotice::BuiltinSshFallback {
                reason: "system OpenSSH not found".to_string(),
            }],
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("Builtin"));
        assert!(!debug.contains("actual-password"));
    }

    #[test]
    fn combined_ssh_failure_reports_both_backends_without_credentials() {
        let error = DatabaseBackendError::SshFallback {
            system_error: "host key verification failed".to_string(),
            builtin_error: super::super::database_ssh_builtin::DatabaseSshError::Unsupported(
                "agent unavailable".to_string(),
            ),
        };
        let message = error.to_string();
        assert!(message.contains("system OpenSSH failed"));
        assert!(message.contains("built-in SSH fallback failed"));
        assert!(!message.contains("actual-password"));
    }
}
