use super::database_ssh::{
    ResolvedSshEndpoint, SshConnectOptions, SshHostKeyPolicy, resolve_builtin_endpoint,
};
use super::{DatabaseSecretBundle, SshConnectionConfig, SshJumpHostConfig};
use russh::client;
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey, load_secret_key};
use std::fmt;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub trait DatabaseIoStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T> DatabaseIoStream for T where T: AsyncRead + AsyncWrite + Send + Unpin {}

#[derive(Debug)]
pub enum DatabaseSshError {
    Io(io::Error),
    Russh(russh::Error),
    Keys(russh::keys::Error),
    UnknownHostKey {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    ChangedHostKey {
        host: String,
        port: u16,
        detail: String,
    },
    AuthenticationFailed {
        host: String,
        username: String,
    },
    Unsupported(String),
}

impl fmt::Display for DatabaseSshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Russh(error) => write!(f, "SSH protocol error: {error}"),
            Self::Keys(error) => write!(f, "SSH key error: {error}"),
            Self::UnknownHostKey {
                host,
                port,
                algorithm,
                fingerprint,
            } => write!(
                f,
                "unknown SSH host key for {host}:{port} ({algorithm}, {fingerprint})"
            ),
            Self::ChangedHostKey { host, port, detail } => {
                write!(f, "SSH host key changed for {host}:{port}: {detail}")
            }
            Self::AuthenticationFailed { host, username } => {
                write!(f, "SSH authentication failed for {username}@{host}")
            }
            Self::Unsupported(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DatabaseSshError {}

impl From<io::Error> for DatabaseSshError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<russh::Error> for DatabaseSshError {
    fn from(value: russh::Error) -> Self {
        Self::Russh(value)
    }
}

impl From<russh::keys::Error> for DatabaseSshError {
    fn from(value: russh::keys::Error) -> Self {
        Self::Keys(value)
    }
}

impl From<russh::AgentAuthError> for DatabaseSshError {
    fn from(value: russh::AgentAuthError) -> Self {
        match value {
            russh::AgentAuthError::Send(error) => {
                Self::Unsupported(format!("SSH agent send failed: {error:?}"))
            }
            russh::AgentAuthError::Key(error) => Self::Keys(error),
        }
    }
}

#[derive(Clone)]
struct BuiltinSshClient {
    host: String,
    port: u16,
    policy: SshHostKeyPolicy,
}

impl client::Handler for BuiltinSshClient {
    type Error = DatabaseSshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        match russh::keys::check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(true) => Ok(true),
            Ok(false) => match self.policy {
                SshHostKeyPolicy::Strict => Err(DatabaseSshError::UnknownHostKey {
                    host: self.host.clone(),
                    port: self.port,
                    algorithm: server_public_key.algorithm().to_string(),
                    fingerprint: server_public_key.fingerprint(HashAlg::Sha256).to_string(),
                }),
                SshHostKeyPolicy::TrustOnce => Ok(true),
                SshHostKeyPolicy::TrustAndStore => {
                    russh::keys::known_hosts::learn_known_hosts(
                        &self.host,
                        self.port,
                        server_public_key,
                    )?;
                    Ok(true)
                }
            },
            Err(russh::keys::Error::KeyChanged { line }) => Err(DatabaseSshError::ChangedHostKey {
                host: self.host.clone(),
                port: self.port,
                detail: format!("known_hosts line {line}"),
            }),
            Err(error) => Err(error.into()),
        }
    }
}

pub struct BuiltinSshStream {
    stream: Box<dyn DatabaseIoStream>,
    _sessions: Vec<client::Handle<BuiltinSshClient>>,
}

impl AsyncRead for BuiltinSshStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.stream).poll_read(cx, buffer)
    }
}

impl AsyncWrite for BuiltinSshStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut *self.stream).poll_write(cx, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut *self.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut *self.stream).poll_shutdown(cx)
    }
}

pub async fn connect_builtin_ssh(
    config: &SshConnectionConfig,
    secrets: &DatabaseSecretBundle,
    options: &SshConnectOptions,
    postgres_host: &str,
    postgres_port: u16,
) -> Result<BuiltinSshStream, DatabaseSshError> {
    let final_endpoint = resolve_builtin_endpoint(config)?;
    let mut sessions = Vec::with_capacity(2);

    let final_session = if let Some(jump_config) = effective_jump_config(config, &final_endpoint)? {
        let jump_endpoint = resolve_builtin_endpoint(&jump_config)?;
        let jump_session = connect_session(
            &jump_endpoint,
            secrets.jump_password.as_ref().map(|value| value.as_str()),
            secrets
                .jump_key_passphrase
                .as_ref()
                .map(|value| value.as_str()),
            options.host_key_policy,
        )
        .await?;
        let channel = jump_session
            .channel_open_direct_tcpip(
                final_endpoint.host.clone(),
                final_endpoint.port.into(),
                "127.0.0.1",
                0,
            )
            .await?;
        let handler = BuiltinSshClient {
            host: final_endpoint.host.clone(),
            port: final_endpoint.port,
            policy: options.host_key_policy,
        };
        let mut session =
            client::connect_stream(Arc::new(client_config()), channel.into_stream(), handler)
                .await?;
        authenticate_session(
            &mut session,
            &final_endpoint,
            secrets.ssh_password.as_ref().map(|value| value.as_str()),
            secrets
                .ssh_key_passphrase
                .as_ref()
                .map(|value| value.as_str()),
        )
        .await?;
        sessions.push(jump_session);
        session
    } else {
        connect_session(
            &final_endpoint,
            secrets.ssh_password.as_ref().map(|value| value.as_str()),
            secrets
                .ssh_key_passphrase
                .as_ref()
                .map(|value| value.as_str()),
            options.host_key_policy,
        )
        .await?
    };

    let channel = final_session
        .channel_open_direct_tcpip(postgres_host, postgres_port.into(), "127.0.0.1", 0)
        .await?;
    sessions.push(final_session);
    Ok(BuiltinSshStream {
        stream: Box::new(channel.into_stream()),
        _sessions: sessions,
    })
}

fn client_config() -> client::Config {
    client::Config {
        nodelay: true,
        inactivity_timeout: Some(std::time::Duration::from_secs(60)),
        keepalive_interval: Some(std::time::Duration::from_secs(15)),
        keepalive_max: 3,
        ..client::Config::default()
    }
}

async fn connect_session(
    endpoint: &ResolvedSshEndpoint,
    password: Option<&str>,
    key_passphrase: Option<&str>,
    policy: SshHostKeyPolicy,
) -> Result<client::Handle<BuiltinSshClient>, DatabaseSshError> {
    let handler = BuiltinSshClient {
        host: endpoint.host.clone(),
        port: endpoint.port,
        policy,
    };
    let mut session = client::connect(
        Arc::new(client_config()),
        (endpoint.host.as_str(), endpoint.port),
        handler,
    )
    .await?;
    authenticate_session(&mut session, endpoint, password, key_passphrase).await?;
    Ok(session)
}

async fn authenticate_session(
    session: &mut client::Handle<BuiltinSshClient>,
    endpoint: &ResolvedSshEndpoint,
    password: Option<&str>,
    key_passphrase: Option<&str>,
) -> Result<(), DatabaseSshError> {
    if let Some(password) = password {
        if session
            .authenticate_password(endpoint.username.clone(), password)
            .await?
            .success()
        {
            return Ok(());
        }
        return Err(authentication_failed(endpoint));
    }

    if let Some(key_path) = endpoint.private_key_path.as_ref() {
        let key = load_secret_key(key_path, key_passphrase)?;
        let hash = session.best_supported_rsa_hash().await?.flatten();
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
        if session
            .authenticate_publickey(endpoint.username.clone(), key)
            .await?
            .success()
        {
            return Ok(());
        }
        return Err(authentication_failed(endpoint));
    }

    if authenticate_with_agent(session, &endpoint.username).await? {
        return Ok(());
    }
    Err(authentication_failed(endpoint))
}

fn authentication_failed(endpoint: &ResolvedSshEndpoint) -> DatabaseSshError {
    DatabaseSshError::AuthenticationFailed {
        host: endpoint.host.clone(),
        username: endpoint.username.clone(),
    }
}

async fn try_agent_identities<S>(
    session: &mut client::Handle<BuiltinSshClient>,
    username: &str,
    agent: &mut AgentClient<S>,
) -> Result<bool, DatabaseSshError>
where
    S: AgentStream + Send + Unpin,
{
    for identity in agent.request_identities().await? {
        let key = match identity {
            AgentIdentity::PublicKey { key, .. } => key,
            AgentIdentity::Certificate { certificate, .. } => {
                PublicKey::new(certificate.public_key().clone(), "")
            }
        };
        let hash = session.best_supported_rsa_hash().await?.flatten();
        if session
            .authenticate_publickey_with(username.to_string(), key, hash, agent)
            .await?
            .success()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(unix)]
async fn authenticate_with_agent(
    session: &mut client::Handle<BuiltinSshClient>,
    username: &str,
) -> Result<bool, DatabaseSshError> {
    let mut agent = AgentClient::connect_env().await?;
    try_agent_identities(session, username, &mut agent).await
}

#[cfg(windows)]
async fn authenticate_with_agent(
    session: &mut client::Handle<BuiltinSshClient>,
    username: &str,
) -> Result<bool, DatabaseSshError> {
    if let Ok(mut agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await
        && try_agent_identities(session, username, &mut agent).await?
    {
        return Ok(true);
    }
    if let Ok(mut agent) = AgentClient::connect_pageant().await {
        return try_agent_identities(session, username, &mut agent).await;
    }
    Ok(false)
}

#[cfg(not(any(unix, windows)))]
async fn authenticate_with_agent(
    _session: &mut client::Handle<BuiltinSshClient>,
    _username: &str,
) -> Result<bool, DatabaseSshError> {
    Ok(false)
}

fn effective_jump_config(
    config: &SshConnectionConfig,
    endpoint: &ResolvedSshEndpoint,
) -> Result<Option<SshConnectionConfig>, DatabaseSshError> {
    if let Some(jump) = &config.jump_host {
        return Ok(Some(jump_to_connection(jump)));
    }
    let Some(proxy_jump) = endpoint.proxy_jump.as_deref() else {
        return Ok(None);
    };
    if proxy_jump.contains(',') || proxy_jump.contains('%') {
        return Err(DatabaseSshError::Unsupported(
            "built-in SSH supports one concrete ProxyJump host; configure the jump host fields explicitly"
                .to_string(),
        ));
    }
    Ok(Some(parse_proxy_jump(proxy_jump)?))
}

fn jump_to_connection(jump: &SshJumpHostConfig) -> SshConnectionConfig {
    SshConnectionConfig {
        host: jump.host.clone(),
        port: jump.port,
        username: jump.username.clone(),
        config_alias: jump.config_alias.clone(),
        private_key_path: jump.private_key_path.clone(),
        remember_password: jump.remember_password,
        remember_key_passphrase: jump.remember_key_passphrase,
        jump_host: None,
    }
}

fn parse_proxy_jump(value: &str) -> Result<SshConnectionConfig, DatabaseSshError> {
    if value.contains(',') || value.contains('%') {
        return Err(DatabaseSshError::Unsupported(
            "built-in SSH supports one concrete ProxyJump host".to_string(),
        ));
    }
    if !value.contains('@') && !value.contains(':') {
        return Ok(SshConnectionConfig {
            config_alias: Some(value.to_string()),
            ..SshConnectionConfig::default()
        });
    }
    let (username, host_port) = value.split_once('@').ok_or_else(|| {
        DatabaseSshError::Unsupported(
            "ProxyJump must use alias or user@host[:port] for built-in SSH".to_string(),
        )
    })?;
    let (host, port) = parse_proxy_jump_host_port(host_port)?;
    Ok(SshConnectionConfig {
        host,
        port,
        username: username.to_string(),
        ..SshConnectionConfig::default()
    })
}

fn parse_proxy_jump_host_port(host_port: &str) -> Result<(String, u16), DatabaseSshError> {
    if let Some(bracketed) = host_port.strip_prefix('[') {
        let Some(close) = bracketed.find(']') else {
            return Err(DatabaseSshError::Unsupported(
                "ProxyJump IPv6 host has no closing bracket".to_string(),
            ));
        };
        let host = &bracketed[..close];
        let suffix = &bracketed[close + 1..];
        let port = if suffix.is_empty() {
            22
        } else {
            suffix
                .strip_prefix(':')
                .ok_or_else(|| {
                    DatabaseSshError::Unsupported(
                        "ProxyJump bracketed host has an invalid suffix".to_string(),
                    )
                })?
                .parse::<u16>()
                .map_err(|_| {
                    DatabaseSshError::Unsupported("ProxyJump port is invalid".to_string())
                })?
        };
        if host.is_empty() {
            return Err(DatabaseSshError::Unsupported(
                "ProxyJump host is empty".to_string(),
            ));
        }
        return Ok((host.to_string(), port));
    }
    if host_port.matches(':').count() > 1 {
        return Ok((host_port.to_string(), 22));
    }
    match host_port.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => Ok((
            host.to_string(),
            port.parse::<u16>().map_err(|_| {
                DatabaseSshError::Unsupported("ProxyJump port is invalid".to_string())
            })?,
        )),
        Some(_) => Err(DatabaseSshError::Unsupported(
            "ProxyJump host is empty".to_string(),
        )),
        None if !host_port.is_empty() => Ok((host_port.to_string(), 22)),
        None => Err(DatabaseSshError::Unsupported(
            "ProxyJump host is empty".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_jump_parser_accepts_alias_and_concrete_target() {
        let alias = parse_proxy_jump("bastion").unwrap();
        assert_eq!(alias.config_alias.as_deref(), Some("bastion"));

        let target = parse_proxy_jump("deploy@host.example.com:2202").unwrap();
        assert_eq!(target.username, "deploy");
        assert_eq!(target.host, "host.example.com");
        assert_eq!(target.port, 2202);
    }

    #[test]
    fn proxy_jump_parser_rejects_ambiguous_chains() {
        assert!(parse_proxy_jump("one,two").is_err());
        assert!(parse_proxy_jump("%h").is_err());
    }

    #[test]
    fn a4_b021_proxy_jump_parser_accepts_ipv6_targets() {
        let target = parse_proxy_jump("deploy@[2001:db8::1]").unwrap();
        assert_eq!(target.host, "2001:db8::1");
        assert_eq!(target.port, 22);

        let target = parse_proxy_jump("deploy@[2001:db8::1]:2202").unwrap();
        assert_eq!(target.host, "2001:db8::1");
        assert_eq!(target.port, 2202);

        let target = parse_proxy_jump("deploy@2001:db8::2").unwrap();
        assert_eq!(target.host, "2001:db8::2");
        assert_eq!(target.port, 22);
    }
}
