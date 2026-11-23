//! 系统 SSH Agent：用于认证，以及把 Agent 转发给远端。
//!
//! - macOS / Linux：`SSH_AUTH_SOCK` 指向的 Unix socket（ssh-agent、1Password、Secretive 等）
//! - Windows：OpenSSH Agent 的命名管道 `\\.\pipe\openssh-ssh-agent`
//! - iOS：没有系统 Agent

use russh::client::Handle;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::ChannelStream;

use crate::client::Handler;
use crate::SshError;

#[cfg(windows)]
const PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

async fn try_identities<S>(
    handle: &mut Handle<Handler>,
    user: &str,
    mut agent: AgentClient<S>,
) -> Result<bool, SshError>
where
    S: AgentStream + Unpin + Send + 'static,
{
    let identities = agent
        .request_identities()
        .await
        .map_err(|_| SshError::AgentUnavailable)?;
    if identities.is_empty() {
        return Err(SshError::Other("SSH Agent 里没有任何密钥".into()));
    }
    let hash = handle.best_supported_rsa_hash().await?.flatten();
    for id in identities {
        let key = id.public_key().into_owned();
        if let Ok(res) = handle
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
        {
            if res.success() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) async fn authenticate(
    handle: &mut Handle<Handler>,
    user: &str,
) -> Result<bool, SshError> {
    #[cfg(all(unix, not(target_os = "ios")))]
    {
        let agent = AgentClient::connect_env()
            .await
            .map_err(|_| SshError::AgentUnavailable)?;
        try_identities(handle, user, agent).await
    }
    #[cfg(windows)]
    {
        let agent = AgentClient::connect_named_pipe(PIPE)
            .await
            .map_err(|_| SshError::AgentUnavailable)?;
        try_identities(handle, user, agent).await
    }
    #[cfg(not(any(all(unix, not(target_os = "ios")), windows)))]
    {
        let _ = (handle, user);
        Err(SshError::AgentUnavailable)
    }
}

/// 把远端发来的 Agent 请求桥接到本机 Agent。
pub(crate) async fn bridge(mut stream: ChannelStream<russh::client::Msg>) -> Result<(), SshError> {
    #[cfg(all(unix, not(target_os = "ios")))]
    {
        let path = std::env::var("SSH_AUTH_SOCK").map_err(|_| SshError::AgentUnavailable)?;
        let mut local = tokio::net::UnixStream::connect(path).await?;
        tokio::io::copy_bidirectional(&mut stream, &mut local).await?;
        Ok(())
    }
    #[cfg(windows)]
    {
        let mut local = tokio::net::windows::named_pipe::ClientOptions::new().open(PIPE)?;
        tokio::io::copy_bidirectional(&mut stream, &mut local).await?;
        Ok(())
    }
    #[cfg(not(any(all(unix, not(target_os = "ios")), windows)))]
    {
        let _ = &mut stream;
        Err(SshError::AgentUnavailable)
    }
}

#[allow(dead_code)]
fn _assert_traits() {
    fn is_agent_stream<T: AgentStream>() {}
    #[cfg(all(unix, not(target_os = "ios")))]
    is_agent_stream::<tokio::net::UnixStream>();
}
