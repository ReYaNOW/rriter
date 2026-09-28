fn pending_expired_event(transaction: &PendingTransaction) -> DatabaseEvent {
    match &transaction.target {
        PendingTransactionTarget::Table { table_name } => DatabaseEvent::TransactionExpired {
            connection_id: transaction.connection_id,
            transaction_id: transaction.transaction_id,
            database_name: transaction.database_name.clone(),
            table_name: table_name.clone(),
        },
        PendingTransactionTarget::Query { console_id } => DatabaseEvent::QueryTransactionExpired {
            connection_id: transaction.connection_id,
            transaction_id: transaction.transaction_id,
            database_name: transaction.database_name.clone(),
            console_id: *console_id,
        },
    }
}

async fn rollback_pending_transaction_event(
    transaction: &PendingTransaction,
    success: DatabaseEvent,
    job_id: DatabaseJobId,
) -> DatabaseEvent {
    match finish_pending_transaction(transaction, false).await {
        Ok(()) => success,
        Err(error) => failure(job_id, error),
    }
}

async fn rollback_expired_transaction_event(transaction: &PendingTransaction) -> DatabaseEvent {
    match finish_pending_transaction(transaction, false).await {
        Ok(()) => pending_expired_event(transaction),
        Err(error) => pending_expiry_failure_event(transaction, error.to_string()),
    }
}

fn pending_expiry_failure_event(
    transaction: &PendingTransaction,
    message: String,
) -> DatabaseEvent {
    expiry_failure_event(
        transaction.connection_id,
        transaction.transaction_id,
        &transaction.database_name,
        &transaction.target,
        message,
    )
}

fn expiry_failure_event(
    connection_id: DatabaseConnectionId,
    transaction_id: DatabaseTransactionId,
    database_name: &str,
    target: &PendingTransactionTarget,
    message: String,
) -> DatabaseEvent {
    match target {
        PendingTransactionTarget::Table { table_name } => DatabaseEvent::TransactionExpiryFailed {
            connection_id,
            transaction_id,
            database_name: database_name.to_string(),
            table_name: table_name.clone(),
            message,
        },
        PendingTransactionTarget::Query { console_id } => {
            DatabaseEvent::QueryTransactionExpiryFailed {
                connection_id,
                transaction_id,
                database_name: database_name.to_string(),
                console_id: *console_id,
                message,
            }
        }
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
        DatabaseSshError::UnknownHostKey {
            host,
            port,
            algorithm,
            fingerprint,
        } => Some((host, *port, algorithm, fingerprint)),
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
        let command = DatabaseCommand::CancelJob {
            job_id: DatabaseJobId(44),
        };
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
        assert!(
            !read
                .execution_policy()
                .unwrap()
                .requires_explicit_transaction()
        );
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
        assert!(
            mutation
                .execution_policy()
                .unwrap()
                .requires_explicit_transaction()
        );
        assert!(
            mutation
                .execution_policy()
                .unwrap()
                .requires_global_review()
        );
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
        assert!(matches!(
            event,
            DatabaseEvent::HostKeyConfirmationRequired {
                job_id: DatabaseJobId(9),
                ..
            }
        ));
    }

    #[test]
    fn runtime_can_start_cancel_idle_job_and_shutdown() {
        let mut runtime = DatabaseRuntime::spawn().unwrap();
        runtime
            .send(DatabaseCommand::CancelJob {
                job_id: DatabaseJobId(7),
            })
            .unwrap();
        let event = loop {
            match runtime.try_recv() {
                Ok(event) => break event,
                Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                Err(error) => panic!("runtime disconnected: {error}"),
            }
        };
        assert_eq!(
            event,
            DatabaseEvent::JobCancelled {
                job_id: DatabaseJobId(7)
            }
        );
        runtime.shutdown();
    }

    #[test]
    fn bug_60_host_key_retry_preserves_the_exact_sql_request() {
        let mut command = DatabaseCommand::RunUserSql {
            job_id: DatabaseJobId(60),
            connection: DatabaseConnectionConfig::default(),
            database_name: "analytics".to_string(),
            console_id: SqlConsoleId(600),
            sql: "SELECT 'original';".to_string(),
            source_offset: 17,
            mode: DatabaseQueryMode::ExplainAnalyze,
            secrets: None,
            settings: DatabaseSettings::default(),
            ssh_options: host_key_options(SshHostKeyPolicy::Strict),
        };
        command.set_host_key_policy(SshHostKeyPolicy::TrustOnce);

        match command {
            DatabaseCommand::RunUserSql {
                job_id,
                database_name,
                console_id,
                sql,
                source_offset,
                mode,
                ssh_options,
                ..
            } => {
                assert_eq!(job_id, DatabaseJobId(60));
                assert_eq!(database_name, "analytics");
                assert_eq!(console_id, SqlConsoleId(600));
                assert_eq!(sql, "SELECT 'original';");
                assert_eq!(source_offset, 17);
                assert_eq!(mode, DatabaseQueryMode::ExplainAnalyze);
                assert_eq!(ssh_options.host_key_policy, SshHostKeyPolicy::TrustOnce);
            }
            _ => panic!("host-key retry changed the command variant"),
        }
    }

    #[test]
    fn bug_61_host_key_retry_preserves_query_completion_console() {
        let mut command = DatabaseCommand::LoadQueryCompletion {
            job_id: DatabaseJobId(61),
            connection: DatabaseConnectionConfig::default(),
            database_name: "postgres".to_string(),
            console_id: SqlConsoleId(610),
            secrets: None,
            settings: DatabaseSettings::default(),
            ssh_options: host_key_options(SshHostKeyPolicy::Strict),
        };
        command.set_host_key_policy(SshHostKeyPolicy::TrustAndStore);

        match command {
            DatabaseCommand::LoadQueryCompletion {
                job_id,
                database_name,
                console_id,
                ssh_options,
                ..
            } => {
                assert_eq!(job_id, DatabaseJobId(61));
                assert_eq!(database_name, "postgres");
                assert_eq!(console_id, SqlConsoleId(610));
                assert_eq!(ssh_options.host_key_policy, SshHostKeyPolicy::TrustAndStore);
            }
            _ => panic!("host-key retry changed the command variant"),
        }
    }

    #[test]
    fn a4_b009_expiry_rollback_failure_keeps_transaction_identity() {
        let event = expiry_failure_event(
            DatabaseConnectionId(4),
            DatabaseTransactionId(9),
            "analytics",
            &PendingTransactionTarget::Query {
                console_id: SqlConsoleId(7),
            },
            "network lost".to_string(),
        );
        assert_eq!(
            event,
            DatabaseEvent::QueryTransactionExpiryFailed {
                connection_id: DatabaseConnectionId(4),
                transaction_id: DatabaseTransactionId(9),
                database_name: "analytics".to_string(),
                console_id: SqlConsoleId(7),
                message: "network lost".to_string(),
            }
        );
    }
}
