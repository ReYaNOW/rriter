use super::{DatabaseConnectionConfig, DatabaseConnectionId, DatabaseSecretBundle};
use std::io;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseSecretKind {
    PostgresPassword,
    SshPassword,
    SshKeyPassphrase,
    JumpPassword,
    JumpKeyPassphrase,
}

impl DatabaseSecretKind {
    fn suffix(self) -> &'static str {
        match self {
            Self::PostgresPassword => "postgres_password",
            Self::SshPassword => "ssh_password",
            Self::SshKeyPassphrase => "ssh_key_passphrase",
            Self::JumpPassword => "jump_password",
            Self::JumpKeyPassphrase => "jump_key_passphrase",
        }
    }
}

pub fn database_secret_purpose(
    connection_id: DatabaseConnectionId,
    kind: DatabaseSecretKind,
) -> String {
    format!("database:{}:{}", connection_id.0, kind.suffix())
}

pub fn store_database_secret(
    connection_id: DatabaseConnectionId,
    kind: DatabaseSecretKind,
    secret: &str,
) -> io::Result<()> {
    crate::platform::store_system_user_secret(
        &database_secret_purpose(connection_id, kind),
        secret.as_bytes(),
    )
}

#[allow(dead_code)] // Loaded by the connection form/runtime integration in stage 3.
pub fn load_database_secret(
    connection_id: DatabaseConnectionId,
    kind: DatabaseSecretKind,
) -> io::Result<Option<Zeroizing<String>>> {
    let Some(bytes) =
        crate::platform::load_system_user_secret(&database_secret_purpose(connection_id, kind))?
    else {
        return Ok(None);
    };
    let bytes = Zeroizing::new(bytes);
    let value = std::str::from_utf8(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .to_owned();
    Ok(Some(Zeroizing::new(value)))
}

pub fn delete_database_secret(
    connection_id: DatabaseConnectionId,
    kind: DatabaseSecretKind,
) -> io::Result<()> {
    crate::platform::delete_system_user_secret(&database_secret_purpose(connection_id, kind))
}

#[allow(dead_code)] // Loaded by the connection form/runtime integration in stage 3.
pub fn load_database_secret_bundle(
    connection: &DatabaseConnectionConfig,
) -> io::Result<DatabaseSecretBundle> {
    let mut bundle = DatabaseSecretBundle::empty();
    if connection.remember_postgres_password {
        bundle.postgres_password =
            load_database_secret(connection.id, DatabaseSecretKind::PostgresPassword)?;
    }
    if let Some(ssh) = &connection.ssh {
        if ssh.remember_password {
            bundle.ssh_password =
                load_database_secret(connection.id, DatabaseSecretKind::SshPassword)?;
        }
        if ssh.remember_key_passphrase {
            bundle.ssh_key_passphrase =
                load_database_secret(connection.id, DatabaseSecretKind::SshKeyPassphrase)?;
        }
        if let Some(jump) = &ssh.jump_host {
            if jump.remember_password {
                bundle.jump_password =
                    load_database_secret(connection.id, DatabaseSecretKind::JumpPassword)?;
            }
            if jump.remember_key_passphrase {
                bundle.jump_key_passphrase =
                    load_database_secret(connection.id, DatabaseSecretKind::JumpKeyPassphrase)?;
            }
        }
    }
    Ok(bundle)
}

pub fn save_remembered_database_secrets(
    connection: &DatabaseConnectionConfig,
    bundle: &DatabaseSecretBundle,
) -> io::Result<()> {
    sync_secret(
        connection.id,
        DatabaseSecretKind::PostgresPassword,
        connection.remember_postgres_password,
        bundle
            .postgres_password
            .as_ref()
            .map(|value| value.as_str()),
    )?;

    let ssh = connection.ssh.as_ref();
    sync_secret(
        connection.id,
        DatabaseSecretKind::SshPassword,
        ssh.is_some_and(|ssh| ssh.remember_password),
        bundle.ssh_password.as_ref().map(|value| value.as_str()),
    )?;
    sync_secret(
        connection.id,
        DatabaseSecretKind::SshKeyPassphrase,
        ssh.is_some_and(|ssh| ssh.remember_key_passphrase),
        bundle
            .ssh_key_passphrase
            .as_ref()
            .map(|value| value.as_str()),
    )?;
    sync_secret(
        connection.id,
        DatabaseSecretKind::JumpPassword,
        ssh.and_then(|ssh| ssh.jump_host.as_ref())
            .is_some_and(|jump| jump.remember_password),
        bundle.jump_password.as_ref().map(|value| value.as_str()),
    )?;
    sync_secret(
        connection.id,
        DatabaseSecretKind::JumpKeyPassphrase,
        ssh.and_then(|ssh| ssh.jump_host.as_ref())
            .is_some_and(|jump| jump.remember_key_passphrase),
        bundle
            .jump_key_passphrase
            .as_ref()
            .map(|value| value.as_str()),
    )
}

fn sync_secret(
    connection_id: DatabaseConnectionId,
    kind: DatabaseSecretKind,
    remember: bool,
    value: Option<&str>,
) -> io::Result<()> {
    if !remember {
        return delete_database_secret(connection_id, kind);
    }
    let value = value.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is marked for storage but no value was supplied",
                kind.suffix()
            ),
        )
    })?;
    store_database_secret(connection_id, kind, value)
}

#[allow(dead_code)] // Called when connection deletion is added in stage 3.
pub fn delete_all_database_secrets(connection_id: DatabaseConnectionId) -> io::Result<()> {
    let mut first_error = None;
    for kind in [
        DatabaseSecretKind::PostgresPassword,
        DatabaseSecretKind::SshPassword,
        DatabaseSecretKind::SshKeyPassphrase,
        DatabaseSecretKind::JumpPassword,
        DatabaseSecretKind::JumpKeyPassphrase,
    ] {
        if let Err(error) = delete_database_secret(connection_id, kind)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purposes_are_stable_and_do_not_include_secret_values() {
        let purpose = database_secret_purpose(
            DatabaseConnectionId(17),
            DatabaseSecretKind::PostgresPassword,
        );
        assert_eq!(purpose, "database:17:postgres_password");
        assert!(!purpose.contains("actual-password"));
    }

    #[test]
    fn missing_remembered_value_is_rejected_before_system_storage() {
        let connection = DatabaseConnectionConfig {
            id: DatabaseConnectionId(12),
            username: "user".to_string(),
            remember_postgres_password: true,
            ..DatabaseConnectionConfig::default()
        };
        let error = save_remembered_database_secrets(&connection, &DatabaseSecretBundle::empty())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("postgres_password"));
    }
}
