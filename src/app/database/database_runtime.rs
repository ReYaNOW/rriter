use super::database_catalog::{
    DatabaseDdlResult, DatabaseTableMetadata, load_public_table_metadata,
    reconstruct_public_table_ddl,
};
use super::database_postgres::{
    DatabaseBackendError, DatabaseConnectionTestResult, DatabaseListResult, PostgresSession,
    DatabaseTableListResult, list_databases, list_public_tables, test_database_connection,
};
use super::database_query::{
    DatabaseQueryCompletionResult, DatabaseQueryDiagnostic, DatabaseQueryMessage,
    DatabaseQueryMode, DatabaseQueryResultSet, begin_user_query_transaction,
    finish_user_query_transaction, history_started_now, load_query_completion_metadata,
};
use super::database_secrets::{
    delete_all_database_secrets, load_database_secret_bundle, save_remembered_database_secrets,
};
use super::database_ssh::{SshConnectOptions, SshHostKeyPolicy};
use super::database_ssh_builtin::DatabaseSshError;
use super::database_table::{
    DatabaseChangePlan, DatabaseTableChunkResult, DatabaseTableCountResult,
    begin_table_transaction, count_public_table_rows, finish_table_transaction,
    load_public_table_chunk,
};
use super::{
    DatabaseConnectionConfig, DatabaseConnectionId, DatabaseGeneration, DatabaseJobId,
    DatabaseExecutionPolicy, DatabaseSecretBundle, DatabaseSettings, DatabaseTransactionId,
    SqlConsoleId,
};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::time::Instant;

pub enum DatabaseCommand {
    TestConnection {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    LoadDatabases {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    LoadPublicTables {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        database_name: String,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    LoadMetadata {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        database_name: String,
        table_name: String,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    LoadDdl {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        database_name: String,
        table_name: String,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    CountRows {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        database_name: String,
        metadata: DatabaseTableMetadata,
        where_clause: String,
        generation: DatabaseGeneration,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    LoadChunk {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        database_name: String,
        metadata: DatabaseTableMetadata,
        where_clause: String,
        order_by: String,
        page: usize,
        limit: usize,
        chunk_index: usize,
        generation: DatabaseGeneration,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    LoadQueryCompletion {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        database_name: String,
        console_id: SqlConsoleId,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    RunUserSql {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        database_name: String,
        console_id: SqlConsoleId,
        sql: String,
        source_offset: usize,
        mode: DatabaseQueryMode,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    BeginTableSave {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        plan: DatabaseChangePlan,
        secrets: Option<DatabaseSecretBundle>,
        settings: DatabaseSettings,
        ssh_options: SshConnectOptions,
    },
    CommitTransaction {
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
    },
    RollbackTransaction {
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
    },
    SaveConnectionSecrets {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
        supplied: DatabaseSecretBundle,
    },
    DeleteConnectionSecrets {
        job_id: DatabaseJobId,
        connection_id: DatabaseConnectionId,
    },
    CancelJob {
        job_id: DatabaseJobId,
    },
    Shutdown,
}

impl DatabaseCommand {
    fn execution_policy(&self) -> Option<DatabaseExecutionPolicy> {
        match self {
            Self::TestConnection { .. }
            | Self::LoadDatabases { .. }
            | Self::LoadPublicTables { .. }
            | Self::LoadMetadata { .. }
            | Self::LoadDdl { .. }
            | Self::CountRows { .. }
            | Self::LoadChunk { .. }
            | Self::LoadQueryCompletion { .. } => {
                Some(DatabaseExecutionPolicy::InternalReadAutocommit)
            }
            Self::RunUserSql { .. } => Some(DatabaseExecutionPolicy::UserSqlReview),
            Self::BeginTableSave { .. } => Some(DatabaseExecutionPolicy::TableMutationReview),
            Self::CommitTransaction { .. }
            | Self::RollbackTransaction { .. }
            | Self::SaveConnectionSecrets { .. }
            | Self::DeleteConnectionSecrets { .. }
            | Self::CancelJob { .. }
            | Self::Shutdown => None,
        }
    }

    fn job_id(&self) -> Option<DatabaseJobId> {
        match self {
            Self::TestConnection { job_id, .. }
            | Self::LoadDatabases { job_id, .. }
            | Self::LoadPublicTables { job_id, .. }
            | Self::LoadMetadata { job_id, .. }
            | Self::LoadDdl { job_id, .. }
            | Self::CountRows { job_id, .. }
            | Self::LoadChunk { job_id, .. }
            | Self::LoadQueryCompletion { job_id, .. }
            | Self::RunUserSql { job_id, .. }
            | Self::BeginTableSave { job_id, .. }
            | Self::CommitTransaction { job_id, .. }
            | Self::RollbackTransaction { job_id, .. }
            | Self::SaveConnectionSecrets { job_id, .. }
            | Self::DeleteConnectionSecrets { job_id, .. }
            | Self::CancelJob { job_id } => Some(*job_id),
            Self::Shutdown => None,
        }
    }

    fn starts_job(&self) -> bool {
        matches!(
            self,
            Self::TestConnection { .. }
                | Self::LoadDatabases { .. }
                | Self::LoadPublicTables { .. }
                | Self::LoadMetadata { .. }
                | Self::LoadDdl { .. }
                | Self::CountRows { .. }
                | Self::LoadChunk { .. }
                | Self::LoadQueryCompletion { .. }
                | Self::RunUserSql { .. }
                | Self::BeginTableSave { .. }
                | Self::SaveConnectionSecrets { .. }
                | Self::DeleteConnectionSecrets { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseEvent {
    ConnectionTested {
        job_id: DatabaseJobId,
        result: DatabaseConnectionTestResult,
    },
    DatabasesLoaded {
        job_id: DatabaseJobId,
        result: DatabaseListResult,
    },
    PublicTablesLoaded {
        job_id: DatabaseJobId,
        database_name: String,
        result: DatabaseTableListResult,
    },
    MetadataLoaded {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        result: DatabaseTableMetadata,
    },
    DdlLoaded {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        result: DatabaseDdlResult,
    },
    TableCountLoaded {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        result: DatabaseTableCountResult,
    },
    TableChunkLoaded {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        result: DatabaseTableChunkResult,
    },
    QueryCompletionLoaded {
        job_id: DatabaseJobId,
        result: DatabaseQueryCompletionResult,
    },
    QueryTransactionPrepared {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        console_id: SqlConsoleId,
        sql: String,
        source_offset: usize,
        started_unix_ms: u128,
        result_sets: Vec<DatabaseQueryResultSet>,
        messages: Vec<DatabaseQueryMessage>,
        deadline_unix_ms: u128,
        duration_ms: u64,
        affected_rows: u64,
        mode: DatabaseQueryMode,
    },
    QueryTransactionCommitted {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        console_id: SqlConsoleId,
    },
    QueryTransactionRolledBack {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        console_id: SqlConsoleId,
    },
    QueryTransactionExpired {
        connection_id: DatabaseConnectionId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        console_id: SqlConsoleId,
    },
    QueryFailed {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        database_name: String,
        console_id: SqlConsoleId,
        sql: String,
        started_unix_ms: u128,
        duration_ms: u64,
        message: String,
        diagnostic: Option<DatabaseQueryDiagnostic>,
    },
    TransactionPrepared {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        table_name: String,
        summary: super::DatabaseTableReviewSummary,
        deadline_unix_ms: u128,
    },
    TransactionCommitted {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        table_name: String,
    },
    TransactionRolledBack {
        connection_id: DatabaseConnectionId,
        job_id: DatabaseJobId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        table_name: String,
    },
    TransactionExpired {
        connection_id: DatabaseConnectionId,
        transaction_id: DatabaseTransactionId,
        database_name: String,
        table_name: String,
    },
    ConnectionSecretsSaved {
        job_id: DatabaseJobId,
        connection: DatabaseConnectionConfig,
    },
    ConnectionSecretsDeleted {
        job_id: DatabaseJobId,
        connection_id: DatabaseConnectionId,
    },
    HostKeyConfirmationRequired {
        job_id: DatabaseJobId,
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    JobFailed {
        job_id: DatabaseJobId,
        message: String,
    },
    JobCancelled {
        job_id: DatabaseJobId,
    },
    Busy {
        requested_job_id: DatabaseJobId,
        active_job_id: DatabaseJobId,
    },
}

struct ActiveJob {
    job_id: DatabaseJobId,
    cancel: Arc<AtomicBool>,
    future: Pin<Box<dyn Future<Output = JobOutcome>>>,
}

enum PendingTransactionTarget {
    Table { table_name: String },
    Query { console_id: SqlConsoleId },
}

struct PendingTransaction {
    job_id: DatabaseJobId,
    connection_id: DatabaseConnectionId,
    transaction_id: DatabaseTransactionId,
    database_name: String,
    target: PendingTransactionTarget,
    session: PostgresSession,
    deadline: Instant,
}

struct JobOutcome {
    event: DatabaseEvent,
    pending_transaction: Option<PendingTransaction>,
}

impl JobOutcome {
    fn event(event: DatabaseEvent) -> Self {
        Self {
            event,
            pending_transaction: None,
        }
    }
}

pub struct DatabaseRuntime {
    command_tx: tokio_mpsc::UnboundedSender<DatabaseCommand>,
    event_rx: mpsc::Receiver<DatabaseEvent>,
    worker: Option<thread::JoinHandle<()>>,
}

impl DatabaseRuntime {
    pub fn spawn() -> io::Result<Self> {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("rriter-database-runtime".to_string())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .thread_name("rriter-database-io")
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(worker_loop(command_rx, event_tx)),
                    Err(error) => {
                        let _ = event_tx.send(DatabaseEvent::JobFailed {
                            job_id: DatabaseJobId(0),
                            message: format!("failed to start database runtime: {error}"),
                        });
                    }
                }
            })?;
        Ok(Self {
            command_tx,
            event_rx,
            worker: Some(worker),
        })
    }

    pub fn send(&self, command: DatabaseCommand) -> io::Result<()> {
        self.command_tx.send(command).map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "database runtime is not running")
        })
    }

    #[cfg(test)]
    pub fn try_recv(&self) -> Result<DatabaseEvent, mpsc::TryRecvError> {
        self.event_rx.try_recv()
    }

    pub fn drain_events(&self, output: &mut Vec<DatabaseEvent>) {
        output.extend(self.event_rx.try_iter());
    }

    pub fn shutdown(&mut self) {
        let _ = self.command_tx.send(DatabaseCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for DatabaseRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn worker_loop(
    mut command_rx: tokio_mpsc::UnboundedReceiver<DatabaseCommand>,
    event_tx: mpsc::Sender<DatabaseEvent>,
) {
    let mut active: Option<ActiveJob> = None;
    let mut pending: Option<PendingTransaction> = None;
    loop {
        if let Some(transaction) = pending.take() {
            tokio::select! {
                _ = tokio::time::sleep_until(transaction.deadline) => {
                    let _ = finish_pending_transaction(&transaction, false).await;
                    let _ = event_tx.send(pending_expired_event(transaction));
                }
                command = command_rx.recv() => {
                    let Some(command) = command else {
                        let _ = finish_pending_transaction(&transaction, false).await;
                        break;
                    };
                    match command {
                        DatabaseCommand::CommitTransaction { job_id, transaction_id }
                            if transaction_id == transaction.transaction_id => {
                            let event = match finish_pending_transaction(&transaction, true).await {
                                Ok(()) => pending_finished_event(transaction, job_id, true),
                                Err(error) => failure(job_id, error),
                            };
                            let _ = event_tx.send(event);
                        }
                        DatabaseCommand::RollbackTransaction { job_id, transaction_id }
                            if transaction_id == transaction.transaction_id => {
                            let event = match finish_pending_transaction(&transaction, false).await {
                                Ok(()) => pending_finished_event(transaction, job_id, false),
                                Err(error) => failure(job_id, error),
                            };
                            let _ = event_tx.send(event);
                        }
                        DatabaseCommand::CancelJob { job_id } if job_id == transaction.job_id => {
                            let _ = finish_pending_transaction(&transaction, false).await;
                            let _ = event_tx.send(DatabaseEvent::JobCancelled { job_id });
                        }
                        DatabaseCommand::Shutdown => {
                            let _ = finish_pending_transaction(&transaction, false).await;
                            break;
                        }
                        command if command.starts_job() => {
                            let requested_job_id = command.job_id().unwrap_or(DatabaseJobId(0));
                            let _ = event_tx.send(DatabaseEvent::Busy {
                                requested_job_id,
                                active_job_id: transaction.job_id,
                            });
                            pending = Some(transaction);
                        }
                        _ => pending = Some(transaction),
                    }
                }
            }
            continue;
        }

        if let Some(mut running) = active.take() {
            tokio::select! {
                outcome = &mut running.future => {
                    if let Some(transaction) = outcome.pending_transaction {
                        pending = Some(transaction);
                    }
                    let _ = event_tx.send(outcome.event);
                }
                command = command_rx.recv() => {
                    let Some(command) = command else {
                        running.cancel.store(true, Ordering::Release);
                        break;
                    };
                    match command {
                        DatabaseCommand::Shutdown => {
                            running.cancel.store(true, Ordering::Release);
                            break;
                        }
                        DatabaseCommand::CancelJob { job_id } if job_id == running.job_id => {
                            running.cancel.store(true, Ordering::Release);
                            let _ = event_tx.send(DatabaseEvent::JobCancelled { job_id });
                        }
                        command if command.starts_job() => {
                            let requested_job_id = command.job_id().unwrap_or(DatabaseJobId(0));
                            let _ = event_tx.send(DatabaseEvent::Busy {
                                requested_job_id,
                                active_job_id: running.job_id,
                            });
                            active = Some(running);
                        }
                        _ => active = Some(running),
                    }
                }
            }
            continue;
        }

        let Some(command) = command_rx.recv().await else {
            break;
        };
        match command {
            DatabaseCommand::Shutdown => break,
            DatabaseCommand::CancelJob { job_id } => {
                let _ = event_tx.send(DatabaseEvent::JobCancelled { job_id });
            }
            DatabaseCommand::CommitTransaction { job_id, .. }
            | DatabaseCommand::RollbackTransaction { job_id, .. } => {
                let _ = event_tx.send(DatabaseEvent::JobFailed {
                    job_id,
                    message: "Транзакция больше не активна".to_string(),
                });
            }
            command => {
                let job_id = command.job_id().unwrap_or(DatabaseJobId(0));
                let cancel = Arc::new(AtomicBool::new(false));
                active = Some(ActiveJob {
                    job_id,
                    cancel: Arc::clone(&cancel),
                    future: Box::pin(run_command(command, cancel)),
                });
            }
        }
    }
}

async fn resolve_secrets(
    connection: DatabaseConnectionConfig,
    supplied: Option<DatabaseSecretBundle>,
) -> Result<DatabaseSecretBundle, String> {
    if let Some(secrets) = supplied {
        return Ok(secrets);
    }
    tokio::task::spawn_blocking(move || load_database_secret_bundle(&connection))
        .await
        .map_err(|error| format!("database secret worker failed: {error}"))?
        .map_err(|error| format!("failed to load database secrets: {error}"))
}

async fn run_command(command: DatabaseCommand, cancel: Arc<AtomicBool>) -> JobOutcome {
    let execution_policy = command.execution_policy();
    match command {
        DatabaseCommand::TestConnection { job_id, connection, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match test_database_connection(&connection, &secrets, &settings, &ssh_options).await {
                Ok(result) => DatabaseEvent::ConnectionTested { job_id, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::LoadDatabases { job_id, connection, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match list_databases(&connection, &secrets, &settings, &ssh_options).await {
                Ok(result) => DatabaseEvent::DatabasesLoaded { job_id, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::LoadPublicTables { job_id, connection, database_name, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match list_public_tables(&connection, &secrets, &database_name, &settings, &ssh_options).await {
                Ok(result) => DatabaseEvent::PublicTablesLoaded { job_id, database_name, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::LoadMetadata { job_id, connection, database_name, table_name, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match load_public_table_metadata(&connection, &secrets, &database_name, &table_name, &settings, &ssh_options).await {
                Ok(result) => DatabaseEvent::MetadataLoaded { connection_id: connection.id, job_id, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::LoadDdl { job_id, connection, database_name, table_name, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match reconstruct_public_table_ddl(&connection, &secrets, &database_name, &table_name, &settings, &ssh_options).await {
                Ok(result) => DatabaseEvent::DdlLoaded { connection_id: connection.id, job_id, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::CountRows { job_id, connection, database_name, metadata, where_clause, generation, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match count_public_table_rows(&connection, &secrets, &database_name, &metadata, &where_clause, generation, &settings, &ssh_options).await {
                Ok(result) => DatabaseEvent::TableCountLoaded { connection_id: connection.id, job_id, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::LoadChunk { job_id, connection, database_name, metadata, where_clause, order_by, page, limit, chunk_index, generation, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match load_public_table_chunk(&connection, &secrets, &database_name, &metadata, &where_clause, &order_by, page, limit, chunk_index, generation, &settings, &ssh_options).await {
                Ok(result) => DatabaseEvent::TableChunkLoaded { connection_id: connection.id, job_id, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::LoadQueryCompletion { job_id, connection, database_name, console_id, secrets, settings, mut ssh_options } => {
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            JobOutcome::event(match load_query_completion_metadata(
                &connection, &secrets, &database_name, console_id, &settings, &ssh_options,
            ).await {
                Ok(result) => DatabaseEvent::QueryCompletionLoaded { job_id, result },
                Err(error) => failure(job_id, error),
            })
        }
        DatabaseCommand::RunUserSql { job_id, connection, database_name, console_id, sql, source_offset, mode, secrets, settings, mut ssh_options } => {
            let Some(policy) = execution_policy else {
                return JobOutcome::event(DatabaseEvent::JobFailed {
                    job_id,
                    message: "Внутренняя ошибка: для пользовательского SQL не задан режим выполнения".to_string(),
                });
            };
            if !policy.requires_explicit_transaction() || !policy.requires_global_review() {
                return JobOutcome::event(DatabaseEvent::JobFailed {
                    job_id,
                    message: "Внутренняя ошибка: пользовательский SQL должен выполняться через review-транзакцию".to_string(),
                });
            }
            ssh_options.cancel = Some(cancel);
            let started_unix_ms = history_started_now();
            let started = Instant::now();
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::QueryFailed {
                    connection_id: connection.id,
                    job_id,
                    database_name,
                    console_id,
                    sql,
                    started_unix_ms,
                    duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    message,
                    diagnostic: None,
                }),
            };
            match begin_user_query_transaction(
                &connection, &secrets, &database_name, &sql, source_offset, mode, &settings, &ssh_options,
            ).await {
                Ok((session, prepared)) => {
                    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    let transaction_id = transaction_id(job_id);
                    let review_duration = Duration::from_secs(settings.transaction_review_timeout_seconds);
                    let deadline = Instant::now() + review_duration;
                    let deadline_unix_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                        .saturating_add(review_duration.as_millis());
                    let event = DatabaseEvent::QueryTransactionPrepared {
                        connection_id: connection.id,
                        job_id,
                        transaction_id,
                        database_name: database_name.clone(),
                        console_id,
                        sql,
                        source_offset,
                        started_unix_ms,
                        result_sets: prepared.result_sets,
                        messages: prepared.messages,
                        deadline_unix_ms,
                        duration_ms,
                        affected_rows: prepared.affected_rows,
                        mode: prepared.mode,
                    };
                    JobOutcome {
                        event,
                        pending_transaction: Some(PendingTransaction {
                            job_id,
                            connection_id: connection.id,
                            transaction_id,
                            database_name,
                            target: PendingTransactionTarget::Query { console_id },
                            session,
                            deadline,
                        }),
                    }
                }
                Err(error) => {
                    if unknown_host_key(&error.error).is_some() {
                        JobOutcome::event(failure(job_id, error.error))
                    } else {
                        JobOutcome::event(DatabaseEvent::QueryFailed {
                            connection_id: connection.id,
                            job_id,
                            database_name,
                            console_id,
                            sql,
                            started_unix_ms,
                            duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                            message: error.error.to_string(),
                            diagnostic: error.diagnostic,
                        })
                    }
                }
            }
        }
        DatabaseCommand::BeginTableSave { job_id, connection, plan, secrets, settings, mut ssh_options } => {
            let Some(policy) = execution_policy else {
                return JobOutcome::event(DatabaseEvent::JobFailed {
                    job_id,
                    message: "Внутренняя ошибка: для сохранения таблицы не задан режим выполнения".to_string(),
                });
            };
            if !policy.requires_explicit_transaction() || !policy.requires_global_review() {
                return JobOutcome::event(DatabaseEvent::JobFailed {
                    job_id,
                    message: "Внутренняя ошибка: изменения таблицы должны выполняться через review-транзакцию".to_string(),
                });
            }
            ssh_options.cancel = Some(cancel);
            let secrets = match resolve_secrets(connection.clone(), secrets).await {
                Ok(secrets) => secrets,
                Err(message) => return JobOutcome::event(DatabaseEvent::JobFailed { job_id, message }),
            };
            match begin_table_transaction(&connection, &secrets, &plan, &settings, &ssh_options).await {
                Ok((session, prepared)) => {
                    let transaction_id = transaction_id(job_id);
                    let review_duration = Duration::from_secs(settings.transaction_review_timeout_seconds);
                    let deadline = Instant::now() + review_duration;
                    let deadline_unix_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                        .saturating_add(review_duration.as_millis());
                    let event = DatabaseEvent::TransactionPrepared {
                        connection_id: connection.id,
                        job_id,
                        transaction_id,
                        database_name: plan.database_name.clone(),
                        table_name: plan.table_name.clone(),
                        summary: prepared.summary,
                        deadline_unix_ms,
                    };
                    JobOutcome {
                        event,
                        pending_transaction: Some(PendingTransaction {
                            job_id,
                            connection_id: connection.id,
                            transaction_id,
                            database_name: plan.database_name,
                            target: PendingTransactionTarget::Table {
                                table_name: plan.table_name,
                            },
                            session,
                            deadline,
                        }),
                    }
                }
                Err(error) => JobOutcome::event(failure(job_id, error)),
            }
        }
        DatabaseCommand::SaveConnectionSecrets { job_id, connection, mut supplied } => {
            let connection_for_worker = connection.clone();
            let result = tokio::task::spawn_blocking(move || {
                let existing = load_database_secret_bundle(&connection_for_worker)?;
                if supplied.postgres_password.is_none() { supplied.postgres_password = existing.postgres_password; }
                if supplied.ssh_password.is_none() { supplied.ssh_password = existing.ssh_password; }
                if supplied.ssh_key_passphrase.is_none() { supplied.ssh_key_passphrase = existing.ssh_key_passphrase; }
                if supplied.jump_password.is_none() { supplied.jump_password = existing.jump_password; }
                if supplied.jump_key_passphrase.is_none() { supplied.jump_key_passphrase = existing.jump_key_passphrase; }
                save_remembered_database_secrets(&connection_for_worker, &supplied)
            }).await;
            JobOutcome::event(match result {
                Ok(Ok(())) => DatabaseEvent::ConnectionSecretsSaved { job_id, connection },
                Ok(Err(error)) => DatabaseEvent::JobFailed { job_id, message: format!("failed to save database secrets: {error}") },
                Err(error) => DatabaseEvent::JobFailed { job_id, message: format!("database secret worker failed: {error}") },
            })
        }
        DatabaseCommand::DeleteConnectionSecrets { job_id, connection_id } => {
            let result = tokio::task::spawn_blocking(move || delete_all_database_secrets(connection_id)).await;
            JobOutcome::event(match result {
                Ok(Ok(())) => DatabaseEvent::ConnectionSecretsDeleted { job_id, connection_id },
                Ok(Err(error)) => DatabaseEvent::JobFailed { job_id, message: format!("failed to delete database secrets: {error}") },
                Err(error) => DatabaseEvent::JobFailed { job_id, message: format!("database secret worker failed: {error}") },
            })
        }
        DatabaseCommand::CancelJob { job_id } => JobOutcome::event(DatabaseEvent::JobCancelled { job_id }),
        DatabaseCommand::CommitTransaction { job_id, .. }
        | DatabaseCommand::RollbackTransaction { job_id, .. } => JobOutcome::event(DatabaseEvent::JobFailed {
            job_id,
            message: "Транзакция больше не активна".to_string(),
        }),
        DatabaseCommand::Shutdown => JobOutcome::event(DatabaseEvent::JobCancelled { job_id: DatabaseJobId(0) }),
    }
}

async fn finish_pending_transaction(
    transaction: &PendingTransaction,
    commit: bool,
) -> Result<(), DatabaseBackendError> {
    match transaction.target {
        PendingTransactionTarget::Table { .. } => {
            finish_table_transaction(&transaction.session, commit).await
        }
        PendingTransactionTarget::Query { .. } => {
            finish_user_query_transaction(&transaction.session, commit).await
        }
    }
}

fn pending_expired_event(transaction: PendingTransaction) -> DatabaseEvent {
    match transaction.target {
        PendingTransactionTarget::Table { table_name } => DatabaseEvent::TransactionExpired {
            connection_id: transaction.connection_id,
            transaction_id: transaction.transaction_id,
            database_name: transaction.database_name,
            table_name,
        },
        PendingTransactionTarget::Query { console_id } => DatabaseEvent::QueryTransactionExpired {
            connection_id: transaction.connection_id,
            transaction_id: transaction.transaction_id,
            database_name: transaction.database_name,
            console_id,
        },
    }
}

fn pending_finished_event(
    transaction: PendingTransaction,
    job_id: DatabaseJobId,
    committed: bool,
) -> DatabaseEvent {
    match transaction.target {
        PendingTransactionTarget::Table { table_name } => {
            if committed {
                DatabaseEvent::TransactionCommitted {
                    connection_id: transaction.connection_id,
                    job_id,
                    transaction_id: transaction.transaction_id,
                    database_name: transaction.database_name,
                    table_name,
                }
            } else {
                DatabaseEvent::TransactionRolledBack {
                    connection_id: transaction.connection_id,
                    job_id,
                    transaction_id: transaction.transaction_id,
                    database_name: transaction.database_name,
                    table_name,
                }
            }
        }
        PendingTransactionTarget::Query { console_id } => {
            if committed {
                DatabaseEvent::QueryTransactionCommitted {
                    connection_id: transaction.connection_id,
                    job_id,
                    transaction_id: transaction.transaction_id,
                    database_name: transaction.database_name,
                    console_id,
                }
            } else {
                DatabaseEvent::QueryTransactionRolledBack {
                    connection_id: transaction.connection_id,
                    job_id,
                    transaction_id: transaction.transaction_id,
                    database_name: transaction.database_name,
                    console_id,
                }
            }
        }
    }
}

fn transaction_id(job_id: DatabaseJobId) -> DatabaseTransactionId {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    DatabaseTransactionId(nanos ^ ((job_id.0 as u128) << 64))
}

fn failure(job_id: DatabaseJobId, error: DatabaseBackendError) -> DatabaseEvent {
    if let Some((host, port, algorithm, fingerprint)) = unknown_host_key(&error) {
        return DatabaseEvent::HostKeyConfirmationRequired {
            job_id,
            host: host.to_string(),
            port,
            algorithm: algorithm.to_string(),
            fingerprint: fingerprint.to_string(),
        };
    }
    DatabaseEvent::JobFailed {
        job_id,
        message: error.to_string(),
    }
}

fn unknown_host_key(error: &DatabaseBackendError) -> Option<(&str, u16, &str, &str)> {
    let ssh_error = match error {
        DatabaseBackendError::Ssh(error) => error,
        DatabaseBackendError::SshFallback { builtin_error, .. } => builtin_error,
        _ => return None,
    };
    match ssh_error {
        DatabaseSshError::UnknownHostKey { host, port, algorithm, fingerprint } => {
            Some((host, *port, algorithm, fingerprint))
        }
        _ => None,
    }
}

pub fn host_key_options(policy: SshHostKeyPolicy) -> SshConnectOptions {
    SshConnectOptions {
        host_key_policy: policy,
        ..SshConnectOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_job_ids_include_table_and_transaction_commands() {
        let command = DatabaseCommand::CancelJob { job_id: DatabaseJobId(44) };
        assert_eq!(command.job_id(), Some(DatabaseJobId(44)));
        assert!(!command.starts_job());
        let command = DatabaseCommand::CommitTransaction {
            job_id: DatabaseJobId(5),
            transaction_id: DatabaseTransactionId(9),
        };
        assert_eq!(command.job_id(), Some(DatabaseJobId(5)));
        assert!(!command.starts_job());
        assert_eq!(DatabaseCommand::Shutdown.job_id(), None);
    }

    #[test]
    fn query_commands_have_foreground_job_identity() {
        let command = DatabaseCommand::LoadQueryCompletion {
            job_id: DatabaseJobId(21),
            connection: DatabaseConnectionConfig::default(),
            database_name: "postgres".to_string(),
            console_id: super::SqlConsoleId(3),
            secrets: None,
            settings: DatabaseSettings::default(),
            ssh_options: SshConnectOptions::default(),
        };
        assert_eq!(command.job_id(), Some(DatabaseJobId(21)));
        assert!(command.starts_job());

        let command = DatabaseCommand::RunUserSql {
            job_id: DatabaseJobId(22),
            connection: DatabaseConnectionConfig::default(),
            database_name: "postgres".to_string(),
            console_id: super::SqlConsoleId(4),
            sql: "SELECT 1".to_string(),
            source_offset: 0,
            mode: DatabaseQueryMode::Run,
            secrets: None,
            settings: DatabaseSettings::default(),
            ssh_options: SshConnectOptions::default(),
        };
        assert_eq!(command.job_id(), Some(DatabaseJobId(22)));
        assert!(command.starts_job());
        assert_eq!(
            command.execution_policy(),
            Some(DatabaseExecutionPolicy::UserSqlReview)
        );
    }

    #[test]
    fn database_commands_keep_read_and_review_policies_separate() {
        let read = DatabaseCommand::LoadDatabases {
            job_id: DatabaseJobId(1),
            connection: DatabaseConnectionConfig::default(),
            secrets: None,
            settings: DatabaseSettings::default(),
            ssh_options: SshConnectOptions::default(),
        };
        assert_eq!(
            read.execution_policy(),
            Some(DatabaseExecutionPolicy::InternalReadAutocommit)
        );
        assert!(!read.execution_policy().unwrap().requires_explicit_transaction());
        assert!(!read.execution_policy().unwrap().requires_global_review());

        let mutation = DatabaseCommand::BeginTableSave {
            job_id: DatabaseJobId(2),
            connection: DatabaseConnectionConfig::default(),
            plan: DatabaseChangePlan {
                database_name: "postgres".to_string(),
                table_name: "items".to_string(),
                statements: Vec::new(),
                preview: String::new(),
            },
            secrets: None,
            settings: DatabaseSettings::default(),
            ssh_options: SshConnectOptions::default(),
        };
        assert_eq!(
            mutation.execution_policy(),
            Some(DatabaseExecutionPolicy::TableMutationReview)
        );
        assert!(mutation.execution_policy().unwrap().requires_explicit_transaction());
        assert!(mutation.execution_policy().unwrap().requires_global_review());
    }

    #[test]
    fn unknown_host_key_becomes_typed_ui_event() {
        let event = failure(
            DatabaseJobId(9),
            DatabaseBackendError::Ssh(DatabaseSshError::UnknownHostKey {
                host: "db.example.com".to_string(),
                port: 22,
                algorithm: "ssh-ed25519".to_string(),
                fingerprint: "SHA256:test".to_string(),
            }),
        );
        assert!(matches!(event, DatabaseEvent::HostKeyConfirmationRequired { job_id: DatabaseJobId(9), .. }));
    }

    #[test]
    fn runtime_can_start_cancel_idle_job_and_shutdown() {
        let mut runtime = DatabaseRuntime::spawn().unwrap();
        runtime.send(DatabaseCommand::CancelJob { job_id: DatabaseJobId(7) }).unwrap();
        let event = loop {
            match runtime.try_recv() {
                Ok(event) => break event,
                Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                Err(error) => panic!("runtime disconnected: {error}"),
            }
        };
        assert_eq!(event, DatabaseEvent::JobCancelled { job_id: DatabaseJobId(7) });
        runtime.shutdown();
    }
}
