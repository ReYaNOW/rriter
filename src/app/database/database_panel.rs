use super::{
    DatabaseConnectionColor, DatabaseConnectionConfig, DatabaseConnectionId,
    DatabaseGeneration, DatabaseJobId, DatabasePersistedState, DatabaseSecretBundle,
    DatabaseSettings, DatabaseTableInfo, DatabaseTableModal, PostgresTlsMode, SshConnectionConfig,
    SshJumpHostConfig,
};
use crate::app::mouse::HoverPopup;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::path::PathBuf;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseConnectionStatus {
    Disconnected,
    Connecting,
    Ready,
    BuiltinSsh,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseDatabaseNode {
    pub name: String,
    pub expanded: bool,
    pub loading: bool,
    pub tables_loaded: bool,
    pub tables: Vec<DatabaseTableInfo>,
    pub error: Option<String>,
}

impl DatabaseDatabaseNode {
    pub fn new(name: String) -> Self {
        Self {
            name,
            expanded: false,
            loading: false,
            tables_loaded: false,
            tables: Vec::new(),
            error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseConnectionNode {
    pub config: DatabaseConnectionConfig,
    pub status: DatabaseConnectionStatus,
    pub expanded: bool,
    pub loading: bool,
    pub databases_loaded: bool,
    pub databases: Vec<DatabaseDatabaseNode>,
    pub status_message: Option<String>,
    pub fallback_reason: Option<String>,
}

impl DatabaseConnectionNode {
    pub fn new(config: DatabaseConnectionConfig) -> Self {
        Self {
            config,
            status: DatabaseConnectionStatus::Disconnected,
            expanded: false,
            loading: false,
            databases_loaded: false,
            databases: Vec::new(),
            status_message: None,
            fallback_reason: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DatabaseFormField {
    DisplayName,
    Host,
    Port,
    Username,
    PostgresPassword,
    MaintenanceDatabase,
    SshHost,
    SshPort,
    SshUsername,
    SshPassword,
    SshPrivateKey,
    SshKeyPassphrase,
    SshConfigAlias,
    JumpHost,
    JumpPort,
    JumpUsername,
    JumpPassword,
    JumpPrivateKey,
    JumpKeyPassphrase,
    JumpConfigAlias,
}

impl DatabaseFormField {
    pub const ALL: [Self; 20] = [
        Self::DisplayName,
        Self::Host,
        Self::Port,
        Self::Username,
        Self::PostgresPassword,
        Self::MaintenanceDatabase,
        Self::SshHost,
        Self::SshPort,
        Self::SshUsername,
        Self::SshPassword,
        Self::SshPrivateKey,
        Self::SshKeyPassphrase,
        Self::SshConfigAlias,
        Self::JumpHost,
        Self::JumpPort,
        Self::JumpUsername,
        Self::JumpPassword,
        Self::JumpPrivateKey,
        Self::JumpKeyPassphrase,
        Self::JumpConfigAlias,
    ];

    pub fn is_secret(self) -> bool {
        matches!(
            self,
            Self::PostgresPassword
                | Self::SshPassword
                | Self::SshKeyPassphrase
                | Self::JumpPassword
                | Self::JumpKeyPassphrase
        )
    }
}

#[derive(Clone, PartialEq)]
pub struct DatabaseDialogInput {
    value: Zeroizing<String>,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
}

impl Eq for DatabaseDialogInput {}

impl std::fmt::Debug for DatabaseDialogInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseDialogInput")
            .field("len", &self.value.len())
            .field("cursor", &self.cursor)
            .field("has_selection", &self.selection_anchor.is_some())
            .finish()
    }
}

impl Default for DatabaseDialogInput {
    fn default() -> Self {
        Self::new("")
    }
}

impl DatabaseDialogInput {
    pub fn new(value: impl Into<String>) -> Self {
        let value = Zeroizing::new(value.into());
        let cursor = value.len();
        Self {
            value,
            cursor,
            selection_anchor: None,
        }
    }

    pub fn text(&self) -> &str {
        self.value.as_str()
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.value.zeroize();
        self.value = Zeroizing::new(value.into());
        self.cursor = self.value.len();
        self.selection_anchor = None;
    }

    pub fn selected_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        Some((anchor.min(self.cursor), anchor.max(self.cursor)))
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.value.len();
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    pub fn set_cursor(&mut self, cursor: usize, selecting: bool) {
        let cursor = clamp_to_char_boundary(self.value.as_str(), cursor);
        let old = self.cursor;
        self.cursor = cursor;
        self.update_selection_after_move(old, selecting);
    }

    pub fn selected_text(&self) -> Option<&str> {
        let (start, end) = self.selected_range()?;
        self.value.get(start..end)
    }

    pub fn insert(&mut self, text: &str, max_bytes: usize) {
        self.delete_selection();
        if self.value.len() >= max_bytes {
            return;
        }
        let available = max_bytes - self.value.len();
        let mut end = text.len().min(available);
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 {
            return;
        }
        self.value.insert_str(self.cursor, &text[..end]);
        self.cursor += end;
    }

    pub fn replace_range(&mut self, start: usize, end: usize, text: &str, max_bytes: usize) {
        let start = clamp_to_char_boundary(self.value.as_str(), start.min(self.value.len()));
        let end = clamp_to_char_boundary(self.value.as_str(), end.min(self.value.len())).max(start);
        let removed = end.saturating_sub(start);
        let available = max_bytes.saturating_sub(self.value.len().saturating_sub(removed));
        let mut insert_end = text.len().min(available);
        while insert_end > 0 && !text.is_char_boundary(insert_end) {
            insert_end -= 1;
        }
        self.value.replace_range(start..end, &text[..insert_end]);
        self.cursor = start + insert_end;
        self.selection_anchor = None;
    }

    pub fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selected_range() else {
            return false;
        };
        self.value.replace_range(start..end, "");
        self.cursor = start;
        self.selection_anchor = None;
        true
    }

    pub fn backspace(&mut self) {
        if self.delete_selection() || self.cursor == 0 {
            return;
        }
        let previous = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.value.replace_range(previous..self.cursor, "");
        self.cursor = previous;
    }

    pub fn delete_forward(&mut self) {
        if self.delete_selection() || self.cursor >= self.value.len() {
            return;
        }
        let next = self.value[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(idx, _)| self.cursor + idx)
            .unwrap_or(self.value.len());
        self.value.replace_range(self.cursor..next, "");
    }

    pub fn delete_word_backward(&mut self) {
        if self.delete_selection() || self.cursor == 0 {
            return;
        }
        let start = previous_word_boundary(self.value.as_str(), self.cursor);
        self.value.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn delete_word_forward(&mut self) {
        if self.delete_selection() || self.cursor >= self.value.len() {
            return;
        }
        let end = next_word_boundary(self.value.as_str(), self.cursor);
        self.value.replace_range(self.cursor..end, "");
    }

    pub fn move_left(&mut self, selecting: bool) {
        let old = self.cursor;
        if let Some((start, end)) = self.selected_range()
            && !selecting
        {
            self.cursor = start.min(end);
            self.selection_anchor = None;
            return;
        }
        self.cursor = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.update_selection_after_move(old, selecting);
    }

    pub fn move_right(&mut self, selecting: bool) {
        let old = self.cursor;
        if let Some((_, end)) = self.selected_range()
            && !selecting
        {
            self.cursor = end;
            self.selection_anchor = None;
            return;
        }
        self.cursor = self.value[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(idx, _)| self.cursor + idx)
            .unwrap_or(self.value.len());
        self.update_selection_after_move(old, selecting);
    }

    pub fn move_word_left(&mut self, selecting: bool) {
        let old = self.cursor;
        self.cursor = previous_word_boundary(self.value.as_str(), self.cursor);
        self.update_selection_after_move(old, selecting);
    }

    pub fn move_word_right(&mut self, selecting: bool) {
        let old = self.cursor;
        self.cursor = next_word_boundary(self.value.as_str(), self.cursor);
        self.update_selection_after_move(old, selecting);
    }

    pub fn move_home(&mut self, selecting: bool) {
        let old = self.cursor;
        self.cursor = 0;
        self.update_selection_after_move(old, selecting);
    }

    pub fn move_end(&mut self, selecting: bool) {
        let old = self.cursor;
        self.cursor = self.value.len();
        self.update_selection_after_move(old, selecting);
    }

    fn update_selection_after_move(&mut self, old: usize, selecting: bool) {
        if selecting {
            self.selection_anchor.get_or_insert(old);
        } else {
            self.selection_anchor = None;
        }
    }

    #[cfg(test)]
    pub fn masked_text(&self, out: &mut String) {
        out.clear();
        out.extend(std::iter::repeat_n('•', self.value.chars().count()));
    }
}

fn clamp_to_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn previous_word_boundary(text: &str, cursor: usize) -> usize {
    let cursor = clamp_to_char_boundary(text, cursor);
    let mut chars = text[..cursor].char_indices().rev().peekable();
    while let Some((_, ch)) = chars.peek().copied() {
        if !ch.is_whitespace() {
            break;
        }
        chars.next();
    }
    let word = chars.peek().is_some_and(|(_, ch)| is_word_char(*ch));
    let mut boundary = cursor;
    while let Some((index, ch)) = chars.peek().copied() {
        if is_word_char(ch) != word {
            break;
        }
        boundary = index;
        chars.next();
    }
    boundary
}

fn next_word_boundary(text: &str, cursor: usize) -> usize {
    let cursor = clamp_to_char_boundary(text, cursor);
    let mut chars = text[cursor..].char_indices().peekable();
    while let Some((_, ch)) = chars.peek().copied() {
        if !ch.is_whitespace() {
            break;
        }
        chars.next();
    }
    let word = chars.peek().is_some_and(|(_, ch)| is_word_char(*ch));
    let mut boundary = cursor;
    while let Some((offset, ch)) = chars.peek().copied() {
        if is_word_char(ch) != word {
            break;
        }
        boundary = cursor + offset + ch.len_utf8();
        chars.next();
    }
    boundary.max(cursor)
}

impl Drop for DatabaseDialogInput {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

#[derive(Debug)]
pub struct DatabaseConnectionDialog {
    pub editing_connection_id: Option<DatabaseConnectionId>,
    pub focused: Option<DatabaseFormField>,
    pub dragging_field: Option<DatabaseFormField>,
    pub display_name: DatabaseDialogInput,
    pub host: DatabaseDialogInput,
    pub port: DatabaseDialogInput,
    pub username: DatabaseDialogInput,
    pub postgres_password: DatabaseDialogInput,
    pub maintenance_database: DatabaseDialogInput,
    pub color: DatabaseConnectionColor,
    pub tls_mode: PostgresTlsMode,
    pub remember_postgres_password: bool,
    pub ssh_enabled: bool,
    pub ssh_host: DatabaseDialogInput,
    pub ssh_port: DatabaseDialogInput,
    pub ssh_username: DatabaseDialogInput,
    pub ssh_password: DatabaseDialogInput,
    pub ssh_private_key: DatabaseDialogInput,
    pub ssh_key_passphrase: DatabaseDialogInput,
    pub ssh_config_alias: DatabaseDialogInput,
    pub remember_ssh_password: bool,
    pub remember_ssh_key_passphrase: bool,
    pub jump_enabled: bool,
    pub jump_host: DatabaseDialogInput,
    pub jump_port: DatabaseDialogInput,
    pub jump_username: DatabaseDialogInput,
    pub jump_password: DatabaseDialogInput,
    pub jump_private_key: DatabaseDialogInput,
    pub jump_key_passphrase: DatabaseDialogInput,
    pub jump_config_alias: DatabaseDialogInput,
    pub remember_jump_password: bool,
    pub remember_jump_key_passphrase: bool,
    pub revealed_secret: Option<DatabaseFormField>,
    pub error: Option<String>,
    pub test_status: Option<String>,
    pub scroll: crate::scroll::ScrollState,
}

impl DatabaseConnectionDialog {
    pub fn new(default_color: DatabaseConnectionColor) -> Self {
        Self {
            editing_connection_id: None,
            focused: Some(DatabaseFormField::DisplayName),
            dragging_field: None,
            display_name: DatabaseDialogInput::new("PostgreSQL"),
            host: DatabaseDialogInput::new("localhost"),
            port: DatabaseDialogInput::new("5432"),
            username: DatabaseDialogInput::default(),
            postgres_password: DatabaseDialogInput::default(),
            maintenance_database: DatabaseDialogInput::new("postgres"),
            color: default_color,
            tls_mode: PostgresTlsMode::Prefer,
            remember_postgres_password: false,
            ssh_enabled: false,
            ssh_host: DatabaseDialogInput::default(),
            ssh_port: DatabaseDialogInput::new("22"),
            ssh_username: DatabaseDialogInput::default(),
            ssh_password: DatabaseDialogInput::default(),
            ssh_private_key: DatabaseDialogInput::default(),
            ssh_key_passphrase: DatabaseDialogInput::default(),
            ssh_config_alias: DatabaseDialogInput::default(),
            remember_ssh_password: false,
            remember_ssh_key_passphrase: false,
            jump_enabled: false,
            jump_host: DatabaseDialogInput::default(),
            jump_port: DatabaseDialogInput::new("22"),
            jump_username: DatabaseDialogInput::default(),
            jump_password: DatabaseDialogInput::default(),
            jump_private_key: DatabaseDialogInput::default(),
            jump_key_passphrase: DatabaseDialogInput::default(),
            jump_config_alias: DatabaseDialogInput::default(),
            remember_jump_password: false,
            remember_jump_key_passphrase: false,
            revealed_secret: None,
            error: None,
            test_status: None,
            scroll: crate::scroll::ScrollState::new(15.0),
        }
    }

    pub fn from_connection(connection: &DatabaseConnectionConfig) -> Self {
        let mut dialog = Self::new(connection.color);
        dialog.editing_connection_id = Some(connection.id);
        dialog.display_name.set_text(connection.display_name.clone());
        dialog.host.set_text(connection.host.clone());
        dialog.port.set_text(connection.port.to_string());
        dialog.username.set_text(connection.username.clone());
        dialog
            .maintenance_database
            .set_text(connection.maintenance_database.clone());
        dialog.tls_mode = connection.tls_mode;
        dialog.remember_postgres_password = connection.remember_postgres_password;
        if let Some(ssh) = &connection.ssh {
            dialog.ssh_enabled = true;
            dialog.ssh_host.set_text(ssh.host.clone());
            dialog.ssh_port.set_text(ssh.port.to_string());
            dialog.ssh_username.set_text(ssh.username.clone());
            dialog.ssh_private_key.set_text(
                ssh.private_key_path
                    .as_deref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            dialog
                .ssh_config_alias
                .set_text(ssh.config_alias.clone().unwrap_or_default());
            dialog.remember_ssh_password = ssh.remember_password;
            dialog.remember_ssh_key_passphrase = ssh.remember_key_passphrase;
            if let Some(jump) = &ssh.jump_host {
                dialog.jump_enabled = true;
                dialog.jump_host.set_text(jump.host.clone());
                dialog.jump_port.set_text(jump.port.to_string());
                dialog.jump_username.set_text(jump.username.clone());
                dialog.jump_private_key.set_text(
                    jump
                        .private_key_path
                        .as_deref()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                dialog
                    .jump_config_alias
                    .set_text(jump.config_alias.clone().unwrap_or_default());
                dialog.remember_jump_password = jump.remember_password;
                dialog.remember_jump_key_passphrase = jump.remember_key_passphrase;
            }
        }
        dialog
    }

    pub fn input(&self, field: DatabaseFormField) -> &DatabaseDialogInput {
        match field {
            DatabaseFormField::DisplayName => &self.display_name,
            DatabaseFormField::Host => &self.host,
            DatabaseFormField::Port => &self.port,
            DatabaseFormField::Username => &self.username,
            DatabaseFormField::PostgresPassword => &self.postgres_password,
            DatabaseFormField::MaintenanceDatabase => &self.maintenance_database,
            DatabaseFormField::SshHost => &self.ssh_host,
            DatabaseFormField::SshPort => &self.ssh_port,
            DatabaseFormField::SshUsername => &self.ssh_username,
            DatabaseFormField::SshPassword => &self.ssh_password,
            DatabaseFormField::SshPrivateKey => &self.ssh_private_key,
            DatabaseFormField::SshKeyPassphrase => &self.ssh_key_passphrase,
            DatabaseFormField::SshConfigAlias => &self.ssh_config_alias,
            DatabaseFormField::JumpHost => &self.jump_host,
            DatabaseFormField::JumpPort => &self.jump_port,
            DatabaseFormField::JumpUsername => &self.jump_username,
            DatabaseFormField::JumpPassword => &self.jump_password,
            DatabaseFormField::JumpPrivateKey => &self.jump_private_key,
            DatabaseFormField::JumpKeyPassphrase => &self.jump_key_passphrase,
            DatabaseFormField::JumpConfigAlias => &self.jump_config_alias,
        }
    }

    pub fn input_mut(&mut self, field: DatabaseFormField) -> &mut DatabaseDialogInput {
        match field {
            DatabaseFormField::DisplayName => &mut self.display_name,
            DatabaseFormField::Host => &mut self.host,
            DatabaseFormField::Port => &mut self.port,
            DatabaseFormField::Username => &mut self.username,
            DatabaseFormField::PostgresPassword => &mut self.postgres_password,
            DatabaseFormField::MaintenanceDatabase => &mut self.maintenance_database,
            DatabaseFormField::SshHost => &mut self.ssh_host,
            DatabaseFormField::SshPort => &mut self.ssh_port,
            DatabaseFormField::SshUsername => &mut self.ssh_username,
            DatabaseFormField::SshPassword => &mut self.ssh_password,
            DatabaseFormField::SshPrivateKey => &mut self.ssh_private_key,
            DatabaseFormField::SshKeyPassphrase => &mut self.ssh_key_passphrase,
            DatabaseFormField::SshConfigAlias => &mut self.ssh_config_alias,
            DatabaseFormField::JumpHost => &mut self.jump_host,
            DatabaseFormField::JumpPort => &mut self.jump_port,
            DatabaseFormField::JumpUsername => &mut self.jump_username,
            DatabaseFormField::JumpPassword => &mut self.jump_password,
            DatabaseFormField::JumpPrivateKey => &mut self.jump_private_key,
            DatabaseFormField::JumpKeyPassphrase => &mut self.jump_key_passphrase,
            DatabaseFormField::JumpConfigAlias => &mut self.jump_config_alias,
        }
    }

    pub fn visible_fields(&self) -> impl Iterator<Item = DatabaseFormField> + '_ {
        DatabaseFormField::ALL.into_iter().filter(|field| {
            if matches!(
                field,
                DatabaseFormField::SshHost
                    | DatabaseFormField::SshPort
                    | DatabaseFormField::SshUsername
                    | DatabaseFormField::SshPassword
                    | DatabaseFormField::SshPrivateKey
                    | DatabaseFormField::SshKeyPassphrase
                    | DatabaseFormField::SshConfigAlias
            ) {
                return self.ssh_enabled;
            }
            if matches!(
                field,
                DatabaseFormField::JumpHost
                    | DatabaseFormField::JumpPort
                    | DatabaseFormField::JumpUsername
                    | DatabaseFormField::JumpPassword
                    | DatabaseFormField::JumpPrivateKey
                    | DatabaseFormField::JumpKeyPassphrase
                    | DatabaseFormField::JumpConfigAlias
            ) {
                return self.ssh_enabled && self.jump_enabled;
            }
            true
        })
    }


    pub fn toggle_secret_visibility(&mut self, field: DatabaseFormField) {
        if !field.is_secret() {
            return;
        }
        self.revealed_secret = if self.revealed_secret == Some(field) {
            None
        } else {
            Some(field)
        };
    }

    pub fn secret_is_revealed(&self, field: DatabaseFormField) -> bool {
        self.revealed_secret == Some(field)
    }

    pub fn focus_next(&mut self, reverse: bool) {
        let fields: Vec<_> = self.visible_fields().collect();
        if fields.is_empty() {
            self.focused = None;
            return;
        }
        let current = self
            .focused
            .and_then(|field| fields.iter().position(|candidate| *candidate == field))
            .unwrap_or(0);
        let next = if reverse {
            current.checked_sub(1).unwrap_or(fields.len() - 1)
        } else {
            (current + 1) % fields.len()
        };
        self.focused = Some(fields[next]);
    }

    pub fn toggle_jump_host(&mut self) {
        self.jump_enabled = !self.jump_enabled;
        if self.jump_enabled {
            self.ssh_enabled = true;
            self.focused = Some(DatabaseFormField::JumpHost);
        } else if self
            .focused
            .is_some_and(|field| matches!(field,
                DatabaseFormField::JumpHost
                    | DatabaseFormField::JumpPort
                    | DatabaseFormField::JumpUsername
                    | DatabaseFormField::JumpPassword
                    | DatabaseFormField::JumpPrivateKey
                    | DatabaseFormField::JumpKeyPassphrase
                    | DatabaseFormField::JumpConfigAlias))
        {
            self.focused = Some(DatabaseFormField::SshHost);
        }
        self.error = None;
        self.test_status = None;
    }

    pub fn build_config(
        &self,
        fallback_id: DatabaseConnectionId,
    ) -> Result<DatabaseConnectionConfig, String> {
        let port = parse_port(self.port.text(), "PostgreSQL port")?;
        let ssh = if self.ssh_enabled {
            let jump_host = if self.jump_enabled {
                Some(SshJumpHostConfig {
                    host: self.jump_host.text().trim().to_string(),
                    port: parse_port(self.jump_port.text(), "SSH jump port")?,
                    username: self.jump_username.text().trim().to_string(),
                    config_alias: non_empty(self.jump_config_alias.text()),
                    private_key_path: path_if_non_empty(self.jump_private_key.text()),
                    remember_password: self.remember_jump_password,
                    remember_key_passphrase: self.remember_jump_key_passphrase,
                })
            } else {
                None
            };
            Some(SshConnectionConfig {
                host: self.ssh_host.text().trim().to_string(),
                port: parse_port(self.ssh_port.text(), "SSH port")?,
                username: self.ssh_username.text().trim().to_string(),
                config_alias: non_empty(self.ssh_config_alias.text()),
                private_key_path: path_if_non_empty(self.ssh_private_key.text()),
                remember_password: self.remember_ssh_password,
                remember_key_passphrase: self.remember_ssh_key_passphrase,
                jump_host,
            })
        } else {
            None
        };
        let config = DatabaseConnectionConfig {
            id: self.editing_connection_id.unwrap_or(fallback_id),
            display_name: self.display_name.text().trim().to_string(),
            color: self.color,
            host: self.host.text().trim().to_string(),
            port,
            username: self.username.text().trim().to_string(),
            maintenance_database: self.maintenance_database.text().trim().to_string(),
            tls_mode: self.tls_mode,
            remember_postgres_password: self.remember_postgres_password,
            ssh,
        };
        config.validate().map_err(|error| error.to_string())?;
        Ok(config)
    }

    pub fn secret_bundle(&self) -> DatabaseSecretBundle {
        DatabaseSecretBundle {
            postgres_password: secret_value(self.postgres_password.text()),
            ssh_password: secret_value(self.ssh_password.text()),
            ssh_key_passphrase: secret_value(self.ssh_key_passphrase.text()),
            jump_password: secret_value(self.jump_password.text()),
            jump_key_passphrase: secret_value(self.jump_key_passphrase.text()),
        }
    }
}

fn parse_port(value: &str, field: &str) -> Result<u16, String> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| format!("{field}: укажите число от 1 до 65535"))
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn path_if_non_empty(value: &str) -> Option<PathBuf> {
    non_empty(value).map(PathBuf::from)
}

fn secret_value(value: &str) -> Option<Zeroizing<String>> {
    (!value.is_empty()).then(|| Zeroizing::new(value.to_string()))
}

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

#[derive(Clone, Debug)]
pub struct DatabasePendingJob {
    pub id: DatabaseJobId,
    pub kind: DatabasePendingJobKind,
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
    pub table_modal_input_dragging: bool,
    pub ddl_hover: RefCell<Option<DatabaseDdlHoverState>>,
    pub pending_job: Option<DatabasePendingJob>,
    pub pending_query_mode: Option<super::DatabaseQueryMode>,
    pub generation: DatabaseGeneration,
    pub next_job_id: u64,
    pub next_connection_id: u64,
    pub next_tab_id: u64,
    pub next_console_id: u64,
    pub global_error: Option<String>,
    pub notice: Option<String>,
    pub session_secrets: FxHashMap<DatabaseConnectionId, DatabaseSecretBundle>,
    pub open_table_keys: FxHashSet<(DatabaseConnectionId, String, String)>,
    pub open_console_keys: FxHashMap<(DatabaseConnectionId, String), Vec<u64>>,
}

impl Default for DatabasePanelState {
    fn default() -> Self {
        Self::from_persisted(DatabasePersistedState::default())
    }
}

impl DatabasePanelState {
    pub fn from_persisted(mut persisted: DatabasePersistedState) -> Self {
        let _ = persisted.normalize_and_validate();
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
            table_modal_input_dragging: false,
            ddl_hover: RefCell::new(None),
            pending_job: None,
            pending_query_mode: None,
            generation: DatabaseGeneration::default(),
            next_job_id: 1,
            next_connection_id,
            next_tab_id: 1,
            next_console_id,
            global_error: None,
            notice: None,
            session_secrets: FxHashMap::default(),
            open_table_keys: FxHashSet::default(),
            open_console_keys: FxHashMap::default(),
        }
    }

    pub fn settings(&self) -> &DatabaseSettings {
        &self.persisted.settings
    }

    pub fn allocate_job_id(&mut self) -> DatabaseJobId {
        let id = DatabaseJobId(self.next_job_id);
        self.next_job_id = self.next_job_id.wrapping_add(1).max(1);
        id
    }

    pub fn allocate_connection_id(&mut self) -> DatabaseConnectionId {
        let id = DatabaseConnectionId(self.next_connection_id);
        self.next_connection_id = self.next_connection_id.wrapping_add(1).max(1);
        id
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
            self.connections.iter().filter(|node| node.expanded).map(|node| node.config.id),
        );
        self.persisted.expanded_databases.clear();
        for connection in &self.connections {
            for database in &connection.databases {
                if database.expanded {
                    self.persisted.expanded_databases.push((connection.config.id, database.name.clone()));
                }
            }
        }
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

    pub fn selected_connection_refresh_enabled(&self) -> bool {
        self.selected_connection.is_some() && self.pending_job.is_none()
    }

    pub fn selected_connection_delete_enabled(&self) -> bool {
        self.selected_connection.is_some() && self.pending_job.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(id: u64) -> DatabaseConnectionConfig {
        DatabaseConnectionConfig {
            id: DatabaseConnectionId(id),
            display_name: format!("Connection {id}"),
            username: "postgres".to_string(),
            ..DatabaseConnectionConfig::default()
        }
    }

    #[test]
    fn dialog_input_edits_unicode_without_breaking_boundaries() {
        let mut input = DatabaseDialogInput::new("аб");
        input.move_left(false);
        input.insert("в", 64);
        assert_eq!(input.text(), "авб");
        input.backspace();
        assert_eq!(input.text(), "аб");
        input.move_home(false);
        input.delete_forward();
        assert_eq!(input.text(), "б");
    }

    #[test]
    fn dialog_input_supports_drag_selection_and_word_navigation() {
        let mut input = DatabaseDialogInput::new("alpha beta");
        input.move_word_left(false);
        assert_eq!(input.cursor, 6);
        input.set_cursor(0, true);
        assert_eq!(input.selected_text(), Some("alpha "));
        input.delete_selection();
        assert_eq!(input.text(), "beta");
        input.move_end(false);
        input.delete_word_backward();
        assert_eq!(input.text(), "");
    }

    #[test]
    fn secret_fields_are_masked_and_debug_does_not_expose_values() {
        let input = DatabaseDialogInput::new("very-secret");
        let mut masked = String::new();
        input.masked_text(&mut masked);
        assert_eq!(masked.chars().count(), 11);
        assert!(!format!("{input:?}").contains("very-secret"));
    }

    #[test]
    fn password_reveal_toggle_never_mutates_secret_text() {
        let mut dialog = DatabaseConnectionDialog::new(DatabaseConnectionColor::Orange);
        dialog.postgres_password.set_text("sëcret-value");
        assert!(!dialog.secret_is_revealed(DatabaseFormField::PostgresPassword));
        dialog.toggle_secret_visibility(DatabaseFormField::PostgresPassword);
        assert!(dialog.secret_is_revealed(DatabaseFormField::PostgresPassword));
        assert_eq!(dialog.postgres_password.text(), "sëcret-value");
        dialog.toggle_secret_visibility(DatabaseFormField::PostgresPassword);
        assert!(!dialog.secret_is_revealed(DatabaseFormField::PostgresPassword));
        assert_eq!(dialog.postgres_password.text(), "sëcret-value");
    }

    #[test]
    fn dialog_builds_direct_and_ssh_configs() {
        let mut dialog = DatabaseConnectionDialog::new(DatabaseConnectionColor::Green);
        dialog.username.set_text("postgres");
        let direct = dialog.build_config(DatabaseConnectionId(7)).unwrap();
        assert_eq!(direct.id, DatabaseConnectionId(7));
        assert!(direct.ssh.is_none());

        dialog.ssh_enabled = true;
        dialog.ssh_host.set_text("bastion.example.com");
        dialog.ssh_username.set_text("deploy");
        dialog.jump_enabled = true;
        dialog.jump_host.set_text("jump.example.com");
        dialog.jump_username.set_text("ops");
        let ssh = dialog.build_config(DatabaseConnectionId(8)).unwrap();
        assert!(ssh.ssh.as_ref().unwrap().jump_host.is_some());
    }

    #[test]
    fn bastion_toggle_enables_ssh_and_reveals_fields() {
        let mut dialog = DatabaseConnectionDialog::new(DatabaseConnectionColor::Orange);
        assert!(!dialog.ssh_enabled);
        assert!(!dialog.jump_enabled);
        dialog.toggle_jump_host();
        assert!(dialog.ssh_enabled);
        assert!(dialog.jump_enabled);
        assert_eq!(dialog.focused, Some(DatabaseFormField::JumpHost));
        assert!(dialog.visible_fields().any(|field| field == DatabaseFormField::JumpHost));
        dialog.toggle_jump_host();
        assert!(dialog.ssh_enabled);
        assert!(!dialog.jump_enabled);
    }

    #[test]
    fn panel_allocates_ids_above_persisted_values() {
        let mut persisted = DatabasePersistedState::default();
        persisted.connections.push(config(40));
        let mut panel = DatabasePanelState::from_persisted(persisted);
        assert_eq!(panel.allocate_connection_id(), DatabaseConnectionId(41));
        assert_eq!(panel.allocate_job_id(), DatabaseJobId(1));
    }

    #[test]
    fn modal_state_blocks_background_and_toolbar_states_follow_selection() {
        let mut panel = DatabasePanelState::default();
        assert!(!panel.modal_open());
        assert!(!panel.selected_connection_refresh_enabled());
        panel.connections.push(DatabaseConnectionNode::new(config(1)));
        panel.selected_connection = Some(DatabaseConnectionId(1));
        assert!(panel.selected_connection_refresh_enabled());
        panel.dialog = Some(DatabaseConnectionDialog::new(DatabaseConnectionColor::Blue));
        assert!(panel.modal_open());
    }
}

impl crate::app::single_line_input::SingleLineInputModel for DatabaseDialogInput {
    fn len_bytes(&self) -> usize {
        self.value.len()
    }

    fn selected_len_bytes(&self) -> usize {
        self.selected_range().map_or(0, |(start, end)| end - start)
    }

    fn select_all(&mut self) {
        DatabaseDialogInput::select_all(self);
    }

    fn selected_text_owned(&self) -> Option<String> {
        self.selected_text().map(str::to_owned)
    }

    fn delete_selection(&mut self) {
        let _ = DatabaseDialogInput::delete_selection(self);
    }

    fn insert_text(&mut self, text: &str) {
        DatabaseDialogInput::insert(self, text, usize::MAX);
    }

    fn backspace(&mut self) {
        DatabaseDialogInput::backspace(self);
    }

    fn delete_forward(&mut self) {
        DatabaseDialogInput::delete_forward(self);
    }

    fn delete_word_backward(&mut self) {
        DatabaseDialogInput::delete_word_backward(self);
    }

    fn delete_word_forward(&mut self) {
        DatabaseDialogInput::delete_word_forward(self);
    }

    fn move_left(&mut self, selecting: bool) {
        DatabaseDialogInput::move_left(self, selecting);
    }

    fn move_right(&mut self, selecting: bool) {
        DatabaseDialogInput::move_right(self, selecting);
    }

    fn move_word_left(&mut self, selecting: bool) {
        DatabaseDialogInput::move_word_left(self, selecting);
    }

    fn move_word_right(&mut self, selecting: bool) {
        DatabaseDialogInput::move_word_right(self, selecting);
    }

    fn move_home(&mut self, selecting: bool) {
        DatabaseDialogInput::move_home(self, selecting);
    }

    fn move_end(&mut self, selecting: bool) {
        DatabaseDialogInput::move_end(self, selecting);
    }
}
