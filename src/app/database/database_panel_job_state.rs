use super::*;
use crate::app::database::{DatabaseCommand, DatabaseTableTabMeta, SshHostKeyPolicy};

fn database_command_should_queue(
    has_pending: bool,
    host_key_prompt_open: bool,
    transaction_review_open: bool,
    can_finish_transaction: bool,
) -> bool {
    has_pending
        || host_key_prompt_open
        || (transaction_review_open && !can_finish_transaction)
}

impl DatabasePanelState {
    pub(crate) fn remove_queued_owner_and_secrets(
        &mut self,
        owner: DatabaseJobOwner,
    ) {
        let removed = self.remove_queued_owner(owner);
        for pending in removed {
            if matches!(pending.kind, DatabasePendingJobKind::SaveConnection) {
                self.pending_session_secrets.remove(&pending.connection_id);
            }
        }
    }

    pub(crate) fn stage_pending_session_secrets(
        &mut self,
        connection_id: DatabaseConnectionId,
        secrets: DatabaseSecretBundle,
    ) {
        self.pending_session_secrets.insert(connection_id, secrets);
    }

    pub(crate) fn mark_owner_job_cancelled(
        &mut self,
        owner: DatabaseJobOwner,
        now: std::time::Instant,
    ) -> Option<DatabasePendingJob> {
        let pending = self
            .pending_job
            .as_ref()
            .filter(|pending| pending.owner == owner)
            .cloned()?;
        self.mark_job_cancelled(pending.id, now);
        Some(pending)
    }

    pub(crate) fn command_should_queue(
        &self,
        host_key_prompt_open: bool,
        transaction_review_open: bool,
        pending: &DatabasePendingJob,
    ) -> bool {
        database_command_should_queue(
            self.pending_job.is_some(),
            host_key_prompt_open,
            transaction_review_open,
            Self::command_can_finish_transaction(pending),
        )
    }

    pub(crate) fn command_can_finish_transaction(pending: &DatabasePendingJob) -> bool {
        matches!(
            pending.kind,
            DatabasePendingJobKind::CommitTransaction
                | DatabasePendingJobKind::RollbackTransaction
        )
    }

    pub(crate) fn connection_job_secrets(
        &self,
        connection_id: DatabaseConnectionId,
    ) -> Option<DatabaseSecretBundle> {
        self.session_secrets
            .get(&connection_id)
            .map(DatabaseSecretBundle::clone_for_job)
    }

    pub(crate) fn prepare_load_databases(
        &mut self,
        connection_id: DatabaseConnectionId,
        host_key_policy: SshHostKeyPolicy,
    ) -> Option<(DatabaseCommand, DatabasePendingJob)> {
        let connection = self.connection(connection_id)?.config.clone();
        if let Some(node) = self.connection_mut(connection_id) {
            node.loading = true;
            node.catalog_load_attempted = true;
            node.status = DatabaseConnectionStatus::Connecting;
            node.status_message = None;
        }
        let job_id = self.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadDatabases,
            owner: DatabaseJobOwner::Connection(connection_id),
            connection_id,
            database_name: None,
            table_name: None,
        };
        let command = DatabaseCommand::LoadDatabases {
            job_id,
            connection,
            secrets: self.connection_job_secrets(connection_id),
            settings: self.settings().clone(),
            ssh_options: crate::app::database::host_key_options(host_key_policy),
        };
        Some((command, pending))
    }

    pub(crate) fn prepare_load_public_tables(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        host_key_policy: SshHostKeyPolicy,
    ) -> Option<(DatabaseCommand, DatabasePendingJob)> {
        let connection = self.connection(connection_id)?.config.clone();
        if let Some(node) = self.connection_mut(connection_id)
            && let Some(database) = node.databases.iter_mut().find(|db| db.name == database_name)
        {
            database.loading = true;
            database.error = None;
        }
        let job_id = self.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadTables,
            owner: DatabaseJobOwner::Connection(connection_id),
            connection_id,
            database_name: Some(database_name.to_string()),
            table_name: None,
        };
        let command = DatabaseCommand::LoadPublicTables {
            job_id,
            connection,
            database_name: database_name.to_string(),
            secrets: self.connection_job_secrets(connection_id),
            settings: self.settings().clone(),
            ssh_options: crate::app::database::host_key_options(host_key_policy),
        };
        Some((command, pending))
    }

    pub(crate) fn prepare_load_ddl(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        table_name: &str,
        host_key_policy: SshHostKeyPolicy,
    ) -> Option<(DatabaseCommand, DatabasePendingJob)> {
        let connection = self.connection(connection_id)?.config.clone();
        let job_id = self.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadDdl,
            owner: DatabaseJobOwner::Connection(connection_id),
            connection_id,
            database_name: Some(database_name.to_string()),
            table_name: Some(table_name.to_string()),
        };
        let command = DatabaseCommand::LoadDdl {
            job_id,
            connection,
            database_name: database_name.to_string(),
            table_name: table_name.to_string(),
            secrets: self.connection_job_secrets(connection_id),
            settings: self.settings().clone(),
            ssh_options: crate::app::database::host_key_options(host_key_policy),
        };
        Some((command, pending))
    }

    pub(crate) fn prepare_load_table_metadata(
        &mut self,
        meta: &DatabaseTableTabMeta,
        host_key_policy: SshHostKeyPolicy,
    ) -> Option<(DatabaseCommand, DatabasePendingJob)> {
        let connection = self.connection(meta.connection_id)?.config.clone();
        let job_id = self.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadMetadata,
            owner: DatabaseJobOwner::Table(meta.tab_id),
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        let command = DatabaseCommand::LoadMetadata {
            job_id,
            connection,
            database_name: meta.database_name.clone(),
            table_name: meta.table_name.clone(),
            secrets: self.connection_job_secrets(meta.connection_id),
            settings: self.settings().clone(),
            ssh_options: crate::app::database::host_key_options(host_key_policy),
        };
        Some((command, pending))
    }

    pub(crate) fn recover_pending_job_panel_state(
        &mut self,
        pending: &DatabasePendingJob,
        message: &str,
        cancelled: bool,
    ) {
        let targets = pending.kind.recovery_targets();
        if matches!(pending.kind, DatabasePendingJobKind::SaveConnection) {
            self.pending_session_secrets.remove(&pending.connection_id);
        }
        if let Some(node) = self.connection_mut(pending.connection_id) {
            if targets.connection_loading {
                node.loading = false;
                node.status = if cancelled {
                    DatabaseConnectionStatus::Disconnected
                } else {
                    DatabaseConnectionStatus::Error
                };
                node.status_message = (!cancelled).then(|| message.to_string());
            }
            if targets.database_loading
                && let Some(database_name) = pending.database_name.as_deref()
                && let Some(database) = node
                    .databases
                    .iter_mut()
                    .find(|database| database.name == database_name)
            {
                database.loading = false;
                database.error = (!cancelled).then(|| message.to_string());
            }
        }
        if targets.dialog_status
            && let Some(dialog) = self.dialog.as_mut()
        {
            dialog.test_status = None;
            dialog.error = (!cancelled).then(|| message.to_string());
        }
        if let Some(DatabaseTableModal::Review { state, .. }) = self.table_modal.as_mut()
            && targets.table_transaction
        {
            state.committing = false;
        }
    }
}
