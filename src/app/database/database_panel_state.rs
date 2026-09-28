/// How long a requested cancel may stay unconfirmed before the job is recovered.
pub(crate) const DATABASE_CANCEL_CONFIRM_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseContextTarget {
    Connection(DatabaseConnectionId),
    Database(DatabaseConnectionId, usize),
    Table(DatabaseConnectionId, usize, usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseContextAction {
    OpenSql,
    NewSqlConsole,
    Refresh,
    EditConnection,
    TestConnection,
    DeleteConnection,
    CloseConnection,
    ShowDdl,
    EditData,
}

#[derive(Clone, Debug)]
pub struct DatabaseContextMenu {
    pub target: DatabaseContextTarget,
    pub x: f32,
    pub y: f32,
    pub entries: Vec<DatabaseContextAction>,
    pub opened_at: std::time::Instant,
}

#[derive(Clone, Debug)]
pub struct DatabaseHostKeyPrompt {
    pub job_id: DatabaseJobId,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct DatabaseDeletePrompt {
    pub connection_id: DatabaseConnectionId,
    pub blocked_open_tabs: usize,
}

#[derive(Clone, Debug)]
pub struct DatabaseDdlHoverState {
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub table_name: String,
    pub popup: HoverPopup,
    pub rect: Option<(f32, f32, f32, f32)>,
    pub max_scroll: f32,
    pub selection_anchor: Option<usize>,
    pub selection_cursor: Option<usize>,
    pub selecting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabasePendingJobKind {
    TestConnection,
    LoadDatabases,
    LoadTables,
    LoadMetadata,
    LoadDdl,
    CountRows,
    LoadChunk,
    BeginTableSave,
    LoadQueryCompletion,
    RunUserSql,
    CommitTransaction,
    RollbackTransaction,
    SaveConnection,
    DeleteConnection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DatabaseRecoveryTargets {
    pub connection_loading: bool,
    pub database_loading: bool,
    pub dialog_status: bool,
    pub metadata_loading: bool,
    pub count_loading: bool,
    pub chunk_loading: bool,
    pub table_transaction: bool,
    pub query_running: bool,
    pub query_transaction: bool,
}

impl DatabasePendingJobKind {
    pub(crate) const fn recovery_targets(self) -> DatabaseRecoveryTargets {
        let mut targets = DatabaseRecoveryTargets {
            connection_loading: false,
            database_loading: false,
            dialog_status: false,
            metadata_loading: false,
            count_loading: false,
            chunk_loading: false,
            table_transaction: false,
            query_running: false,
            query_transaction: false,
        };
        match self {
            Self::LoadDatabases => targets.connection_loading = true,
            Self::LoadTables => targets.database_loading = true,
            Self::TestConnection | Self::SaveConnection => targets.dialog_status = true,
            Self::LoadMetadata => targets.metadata_loading = true,
            Self::CountRows => targets.count_loading = true,
            Self::LoadChunk => targets.chunk_loading = true,
            Self::BeginTableSave => targets.table_transaction = true,
            Self::RunUserSql => targets.query_running = true,
            Self::CommitTransaction | Self::RollbackTransaction => {
                targets.table_transaction = true;
                targets.query_transaction = true;
            }
            Self::LoadDdl | Self::LoadQueryCompletion | Self::DeleteConnection => {}
        }
        targets
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DatabaseJobOwner {
    Dialog(u64),
    Connection(DatabaseConnectionId),
    Table(super::DatabaseTabId),
    Query(super::SqlConsoleId),
}

#[derive(Clone, Debug)]
pub struct DatabasePendingJob {
    pub id: DatabaseJobId,
    pub kind: DatabasePendingJobKind,
    pub owner: DatabaseJobOwner,
    pub connection_id: DatabaseConnectionId,
    pub database_name: Option<String>,
    pub table_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseQueryTabMeta {
    pub console_id: super::SqlConsoleId,
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub title: String,
}

pub const MAX_QUEUED_DATABASE_COMMANDS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DatabaseQueueResult {
    Queued,
    Full,
}

pub struct DatabasePanelState {
    pub persisted: DatabasePersistedState,
    pub connections: Vec<DatabaseConnectionNode>,
    pub selected_connection: Option<DatabaseConnectionId>,
    pub selected_database: Option<(DatabaseConnectionId, String)>,
    pub selected_table: Option<(DatabaseConnectionId, String, String)>,
    pub last_table_click: Option<((DatabaseConnectionId, String, String), std::time::Instant)>,
    pub scroll: crate::scroll::ScrollState,
    pub dialog: Option<DatabaseConnectionDialog>,
    pub delete_prompt: Option<DatabaseDeletePrompt>,
    pub context_menu: Option<DatabaseContextMenu>,
    pub host_key_prompt: Option<DatabaseHostKeyPrompt>,
    pub host_key_policy_override: Option<super::SshHostKeyPolicy>,
    pub table_modal: Option<DatabaseTableModal>,
    pub(crate) table_modal_layout_cache: RefCell<DatabaseMultilineLayoutCache>,
    pub table_modal_input_dragging: bool,
    pub ddl_hover: RefCell<Option<DatabaseDdlHoverState>>,
    pub(crate) ddl_hover_animation_wake_at: Option<std::time::Instant>,
    pub pending_job: Option<DatabasePendingJob>,
    pub active_command: Option<super::DatabaseCommand>,
    pub queued_commands: VecDeque<(super::DatabaseCommand, DatabasePendingJob)>,
    pub host_key_retry: Option<(super::DatabaseCommand, DatabasePendingJob)>,
    pub cancelled_job_ids: FxHashMap<DatabaseJobId, std::time::Instant>,
    pub cancel_requested_at: Option<std::time::Instant>,
    pub pending_query_mode: Option<super::DatabaseQueryMode>,
    pub generation: DatabaseGeneration,
    pub next_job_id: u64,
    pub next_connection_id: u64,
    pub next_tab_id: u64,
    pub next_console_id: u64,
    pub next_dialog_id: u64,
    pub global_error: Option<String>,
    pub notice: Option<String>,
    pub session_secrets: FxHashMap<DatabaseConnectionId, DatabaseSecretBundle>,
    pub pending_session_secrets: FxHashMap<DatabaseConnectionId, DatabaseSecretBundle>,
    pub open_table_keys: FxHashSet<(DatabaseConnectionId, String, String)>,
    pub open_table_ids: FxHashSet<super::DatabaseTabId>,
    pub open_console_keys: FxHashMap<(DatabaseConnectionId, String), Vec<u64>>,
}

pub(crate) fn database_tree_row_height(scale: f32) -> f32 {
    if !scale.is_finite() || scale <= 0.0 {
        return 0.0;
    }
    (crate::render_view::tree_ui::TREE_ROW_H * scale)
        .round()
        .max(1.0)
}

impl Default for DatabasePanelState {
    fn default() -> Self {
        Self::from_persisted(DatabasePersistedState::default())
    }
}

impl DatabasePanelState {
    pub fn from_persisted(mut persisted: DatabasePersistedState) -> Self {
        let normalization_error = persisted.normalize_and_validate().err();
        let next_connection_id = persisted
            .connections
            .iter()
            .map(|connection| connection.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let next_console_id = persisted
            .consoles
            .iter()
            .map(|console| console.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let expanded_connections = persisted.expanded_connections.clone();
        let mut connections: Vec<_> = persisted
            .connections
            .iter()
            .cloned()
            .map(DatabaseConnectionNode::new)
            .collect();
        for connection in &mut connections {
            connection.expanded = expanded_connections.contains(&connection.config.id);
        }
        let selected_connection = persisted.selected_connection;
        let selected_database = persisted.selected_database.clone();
        Self {
            persisted,
            connections,
            selected_connection,
            selected_database,
            selected_table: None,
            last_table_click: None,
            scroll: crate::scroll::ScrollState::new(15.0),
            dialog: None,
            delete_prompt: None,
            context_menu: None,
            host_key_prompt: None,
            host_key_policy_override: None,
            table_modal: None,
            table_modal_layout_cache: RefCell::new(DatabaseMultilineLayoutCache::default()),
            table_modal_input_dragging: false,
            ddl_hover: RefCell::new(None),
            ddl_hover_animation_wake_at: None,
            pending_job: None,
            active_command: None,
            queued_commands: VecDeque::new(),
            host_key_retry: None,
            cancelled_job_ids: FxHashMap::default(),
            cancel_requested_at: None,
            pending_query_mode: None,
            generation: DatabaseGeneration::default(),
            next_job_id: 1,
            next_connection_id,
            next_tab_id: 1,
            next_console_id,
            next_dialog_id: 1,
            global_error: normalization_error
                .map(|error| format!("Повреждено состояние Database Tools: {error}")),
            notice: None,
            session_secrets: FxHashMap::default(),
            pending_session_secrets: FxHashMap::default(),
            open_table_keys: FxHashSet::default(),
            open_table_ids: FxHashSet::default(),
            open_console_keys: FxHashMap::default(),
        }
    }

    pub fn settings(&self) -> &DatabaseSettings {
        &self.persisted.settings
    }

    pub fn allocate_job_id(&mut self) -> DatabaseJobId {
        let start = self.next_job_id.max(1);
        let mut candidate = start;
        loop {
            let id = DatabaseJobId(candidate);
            let used = self.pending_job.as_ref().is_some_and(|job| job.id == id)
                || self.queued_commands.iter().any(|(_, job)| job.id == id)
                || self
                    .host_key_retry
                    .as_ref()
                    .is_some_and(|(_, job)| job.id == id)
                || self.cancelled_job_ids.contains_key(&id);
            candidate = candidate.wrapping_add(1).max(1);
            if !used {
                self.next_job_id = candidate;
                return id;
            }
        }
    }

    pub(crate) fn activate_command(
        &mut self,
        command: super::DatabaseCommand,
        pending: DatabasePendingJob,
    ) {
        self.active_command = Some(command);
        self.pending_job = Some(pending);
    }

    pub(crate) fn queue_command(
        &mut self,
        command: super::DatabaseCommand,
        pending: DatabasePendingJob,
    ) -> DatabaseQueueResult {
        if self.queued_commands.len() >= MAX_QUEUED_DATABASE_COMMANDS {
            return DatabaseQueueResult::Full;
        }
        // Каждое пользовательское действие сохраняет собственный payload.
        // Не coalesce по owner/kind: database, table, filter, generation и secrets
        // могут различаться при одинаковом виде операции.
        self.queued_commands.push_back((command, pending));
        DatabaseQueueResult::Queued
    }

    pub(crate) fn remove_queued_owner(
        &mut self,
        owner: DatabaseJobOwner,
    ) -> Vec<DatabasePendingJob> {
        let mut removed = Vec::new();
        self.queued_commands.retain(|(_, pending)| {
            if pending.owner == owner {
                removed.push(pending.clone());
                false
            } else {
                true
            }
        });
        if self
            .host_key_retry
            .as_ref()
            .is_some_and(|(_, pending)| pending.owner == owner)
        {
            if let Some((_, pending)) = self.host_key_retry.take() {
                removed.push(pending);
            }
            self.host_key_prompt = None;
        }
        removed
    }

    pub(crate) fn pop_queued_command(
        &mut self,
    ) -> Option<(super::DatabaseCommand, DatabasePendingJob)> {
        self.queued_commands.pop_front()
    }

    pub(crate) fn clear_active_command(&mut self) {
        self.pending_job = None;
        self.active_command = None;
    }

    pub(crate) fn mark_job_cancelled(&mut self, job_id: DatabaseJobId, now: std::time::Instant) {
        self.cancelled_job_ids
            .insert(job_id, now + std::time::Duration::from_secs(30));
    }

    pub(crate) fn prune_cancelled_jobs(&mut self, now: std::time::Instant) {
        self.cancelled_job_ids.retain(|_, expires| *expires > now);
    }

    pub(crate) fn cancel_timed_out(
        &self,
        now: std::time::Instant,
        timeout: std::time::Duration,
    ) -> bool {
        self.cancel_requested_at
            .is_some_and(|started| now.saturating_duration_since(started) >= timeout)
    }

    /// When an unconfirmed cancel gives up; the event loop sets a timer for it.
    pub(crate) fn cancel_deadline(&self) -> Option<std::time::Instant> {
        self.cancel_requested_at
            .map(|started| started + DATABASE_CANCEL_CONFIRM_TIMEOUT)
    }

    pub fn allocate_connection_id(&mut self) -> DatabaseConnectionId {
        let start = self.next_connection_id.max(1);
        let mut candidate = start;
        loop {
            let id = DatabaseConnectionId(candidate);
            let used = self.connections.iter().any(|node| node.config.id == id)
                || self
                    .dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.reserved_connection_id == Some(id))
                || self
                    .pending_job
                    .as_ref()
                    .is_some_and(|job| job.connection_id == id)
                || self
                    .queued_commands
                    .iter()
                    .any(|(_, job)| job.connection_id == id)
                || self
                    .host_key_retry
                    .as_ref()
                    .is_some_and(|(_, job)| job.connection_id == id)
                || self.pending_session_secrets.contains_key(&id);
            candidate = candidate.wrapping_add(1).max(1);
            if !used {
                self.next_connection_id = candidate;
                return id;
            }
        }
    }

    pub fn allocate_dialog_id(&mut self) -> u64 {
        let start = self.next_dialog_id.max(1);
        let mut candidate = start;
        loop {
            let used_by_dialog = self
                .dialog
                .as_ref()
                .is_some_and(|dialog| dialog.session_id == candidate);
            let used_by_pending = self.pending_job.as_ref().is_some_and(
                |job| matches!(job.owner, DatabaseJobOwner::Dialog(id) if id == candidate),
            );
            let used_by_queue = self.queued_commands.iter().any(
                |(_, job)| matches!(job.owner, DatabaseJobOwner::Dialog(id) if id == candidate),
            );
            let used_by_retry = self.host_key_retry.as_ref().is_some_and(
                |(_, job)| matches!(job.owner, DatabaseJobOwner::Dialog(id) if id == candidate),
            );
            let id = candidate;
            candidate = candidate.wrapping_add(1).max(1);
            if !(used_by_dialog || used_by_pending || used_by_queue || used_by_retry) {
                self.next_dialog_id = candidate;
                return id;
            }
        }
    }

    pub fn allocate_tab_id(&mut self) -> super::DatabaseTabId {
        let used: FxHashSet<u64> = self
            .open_table_ids
            .iter()
            .map(|id| id.0)
            .chain(
                self.queued_commands
                    .iter()
                    .filter_map(|(_, job)| match job.owner {
                        DatabaseJobOwner::Table(id) => Some(id.0),
                        _ => None,
                    }),
            )
            .chain(self.pending_job.iter().filter_map(|job| match job.owner {
                DatabaseJobOwner::Table(id) => Some(id.0),
                _ => None,
            }))
            .chain(
                self.host_key_retry
                    .iter()
                    .filter_map(|(_, job)| match job.owner {
                        DatabaseJobOwner::Table(id) => Some(id.0),
                        _ => None,
                    }),
            )
            .collect();
        let start = self.next_tab_id.max(1);
        let mut candidate = start;
        loop {
            candidate = candidate.max(1);
            if !used.contains(&candidate) {
                self.next_tab_id = candidate.wrapping_add(1).max(1);
                return super::DatabaseTabId(candidate);
            }
            candidate = candidate.wrapping_add(1).max(1);
        }
    }

    pub fn allocate_console_id(&mut self) -> super::SqlConsoleId {
        let used_persisted = |candidate| {
            self.persisted
                .consoles
                .iter()
                .any(|console| console.id.0 == candidate)
        };
        let start = self.next_console_id.max(1);
        let mut candidate = start;
        loop {
            let queued = self.queued_commands.iter().any(
                |(_, job)| matches!(job.owner, DatabaseJobOwner::Query(id) if id.0 == candidate),
            );
            let active = self.pending_job.as_ref().is_some_and(
                |job| matches!(job.owner, DatabaseJobOwner::Query(id) if id.0 == candidate),
            );
            let retry = self.host_key_retry.as_ref().is_some_and(
                |(_, job)| matches!(job.owner, DatabaseJobOwner::Query(id) if id.0 == candidate),
            );
            if !used_persisted(candidate) && !queued && !active && !retry {
                self.next_console_id = candidate.wrapping_add(1).max(1);
                return super::SqlConsoleId(candidate);
            }
            candidate = candidate.wrapping_add(1).max(1);
        }
    }

    pub fn connection_index(&self, id: DatabaseConnectionId) -> Option<usize> {
        self.connections
            .iter()
            .position(|connection| connection.config.id == id)
    }

    pub fn connection(&self, id: DatabaseConnectionId) -> Option<&DatabaseConnectionNode> {
        self.connection_index(id)
            .and_then(|index| self.connections.get(index))
    }

    pub fn connection_mut(
        &mut self,
        id: DatabaseConnectionId,
    ) -> Option<&mut DatabaseConnectionNode> {
        let index = self.connection_index(id)?;
        self.connections.get_mut(index)
    }

    pub fn sync_persisted_connections(&mut self) {
        self.persisted.connections.clear();
        self.persisted
            .connections
            .extend(self.connections.iter().map(|node| node.config.clone()));
        self.persisted.selected_connection = self.selected_connection;
        self.persisted.selected_database = self.selected_database.clone();
        self.persisted.expanded_connections.clear();
        self.persisted.expanded_connections.extend(
            self.connections
                .iter()
                .filter(|node| node.expanded)
                .map(|node| node.config.id),
        );
        self.persisted.expanded_databases.clear();
        for connection in &self.connections {
            for database in &connection.databases {
                if database.expanded {
                    self.persisted
                        .expanded_databases
                        .push((connection.config.id, database.name.clone()));
                }
            }
        }
    }

    pub(crate) fn begin_expanded_connection_catalog_loads(&mut self) -> Vec<DatabaseConnectionId> {
        let mut connection_ids = Vec::new();
        for node in &mut self.connections {
            if node.children_state() != DatabaseConnectionChildrenState::ExpandedUnloaded {
                continue;
            }
            node.loading = true;
            node.catalog_load_attempted = true;
            node.status = DatabaseConnectionStatus::Connecting;
            node.status_message = None;
            connection_ids.push(node.config.id);
        }
        connection_ids
    }

    pub(crate) fn toggle_connection_expansion(
        &mut self,
        connection_id: DatabaseConnectionId,
    ) -> bool {
        let Some(node) = self.connection_mut(connection_id) else {
            return false;
        };
        node.expanded = !node.expanded;
        node.expanded && !node.databases_loaded && !node.loading
    }

    pub fn modal_open(&self) -> bool {
        self.dialog.is_some()
            || self.delete_prompt.is_some()
            || self.host_key_prompt.is_some()
            || self.table_modal.is_some()
    }

    pub fn clear_transient_overlays(&mut self) {
        self.context_menu = None;
        *self.ddl_hover.borrow_mut() = None;
        self.global_error = None;
    }

    pub(crate) fn visible_tree_row_count(&self) -> usize {
        let mut rows = 0usize;
        for connection in &self.connections {
            rows += 1;
            let children_state = connection.children_state();
            if children_state == DatabaseConnectionChildrenState::Collapsed {
                continue;
            }
            if connection.databases.is_empty()
                && matches!(
                    children_state,
                    DatabaseConnectionChildrenState::ExpandedUnloaded
                        | DatabaseConnectionChildrenState::ExpandedLoading
                        | DatabaseConnectionChildrenState::ExpandedEmpty
                        | DatabaseConnectionChildrenState::ExpandedError
                )
            {
                rows += 1;
            }
            for database in &connection.databases {
                rows += 1;
                if !database.expanded {
                    continue;
                }
                if database.loading && database.tables.is_empty() {
                    rows += 1;
                }
                rows += database.tables.len();
            }
        }
        rows
    }

    pub(crate) fn max_tree_scroll(&self, panel_height: f32, scale: f32) -> f32 {
        if !panel_height.is_finite() || !scale.is_finite() || panel_height <= 0.0 || scale <= 0.0 {
            return 0.0;
        }
        let toolbar_h = 34.0 * scale;
        let viewport_h = (panel_height - toolbar_h).max(0.0);
        let row_h = database_tree_row_height(scale);
        (self.visible_tree_row_count() as f32 * row_h - viewport_h).max(0.0)
    }

    pub fn selected_connection_refresh_enabled(&self) -> bool {
        self.selected_connection.is_some() && self.pending_job.is_none()
    }

    pub fn selected_connection_delete_enabled(&self) -> bool {
        self.selected_connection.is_some() && self.pending_job.is_none()
    }
}
