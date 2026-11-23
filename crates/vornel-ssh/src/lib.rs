//! Vornel 的 SSH 连接。
//!
//! 负责建立连接、校验主机密钥（首次信任 + 记录到 known_hosts）、认证，
//! 并打开一个带 PTY 的交互式 Shell。输出通过回调交给调用方（Tauri 层再转发给前端）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle, Msg};
use russh::keys::{
    check_known_hosts_path, known_hosts::learn_known_hosts_path, load_secret_key, HashAlg,
    PrivateKeyWithHashAlg, PublicKeyOrCertificate,
};
use russh::{ChannelMsg, ChannelWriteHalf, Disconnect};

/// 认证方式。
pub enum Auth {
    Password(String),
    KeyFile {
        path: PathBuf,
        passphrase: Option<String>,
    },
}

/// 建立连接所需的全部参数。
pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
    /// Vornel 自己维护的 known_hosts 文件（与系统 ~/.ssh/known_hosts 分开）。
    pub known_hosts: PathBuf,
    pub cols: u32,
    pub rows: u32,
}

/// 连接成功后的主机密钥信息。
#[derive(Debug, Clone)]
pub struct HostKeyInfo {
    /// 形如 `SHA256:k3Jx9q...` 的指纹。
    pub fingerprint: String,
    /// 是否是首次见到这台主机（刚刚写入 known_hosts）。
    pub newly_trusted: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("连接失败：{0}")]
    Protocol(#[from] russh::Error),
    #[error("密钥错误：{0}")]
    Key(#[from] russh::keys::Error),
    #[error("认证失败：用户名、密码或私钥不正确")]
    AuthFailed,
    #[error(
        "主机密钥与上次记录的不一致（known_hosts 第 {line} 行），可能存在中间人攻击，已拒绝连接。当前指纹：{fingerprint}"
    )]
    HostKeyChanged { fingerprint: String, line: usize },
    #[error("连接超时")]
    Timeout,
}

struct ClientHandler {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    info: Arc<Mutex<Option<HostKeyInfo>>>,
}

impl client::Handler for ClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_public_key.public_key();
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();

        let newly_trusted =
            match check_known_hosts_path(&self.host, self.port, &key, &self.known_hosts) {
                Ok(true) => false,
                Ok(false) => {
                    // 首次连接：信任并记录（TOFU）。前端会以 KEY 徽标提示用户核对指纹。
                    learn_known_hosts_path(&self.host, self.port, &key, &self.known_hosts)?;
                    true
                }
                Err(russh::keys::Error::KeyChanged { line }) => {
                    return Err(SshError::HostKeyChanged { fingerprint, line });
                }
                Err(e) => return Err(e.into()),
            };

        if let Ok(mut slot) = self.info.lock() {
            *slot = Some(HostKeyInfo {
                fingerprint,
                newly_trusted,
            });
        }
        Ok(true)
    }
}

/// 一个已打开的交互式 Shell。可以在多个任务间共享（方法都只需要 `&self`）。
pub struct Shell {
    writer: ChannelWriteHalf<Msg>,
    session: Handle<ClientHandler>,
}

impl Shell {
    /// 把用户输入发给远端。
    pub async fn write(&self, data: Vec<u8>) -> Result<(), SshError> {
        self.writer.data_bytes(data).await?;
        Ok(())
    }

    /// 终端尺寸变化时通知远端。
    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        self.writer.window_change(cols, rows, 0, 0).await?;
        Ok(())
    }

    /// 关闭通道并断开连接。
    pub async fn close(&self) -> Result<(), SshError> {
        let _ = self.writer.eof().await;
        let _ = self.writer.close().await;
        self.session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await?;
        Ok(())
    }
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// 连接、认证并打开交互式 Shell。
///
/// - `on_output`：远端输出（stdout 与 stderr）到达时调用，可能被调用很多次。
/// - `on_exit`：Shell 结束时调用一次，参数是退出码（远端没给则为 `None`）。
pub async fn open_shell<O, E>(
    opts: ConnectOptions,
    mut on_output: O,
    on_exit: E,
) -> Result<(Shell, HostKeyInfo), SshError>
where
    O: FnMut(Vec<u8>) + Send + 'static,
    E: FnOnce(Option<u32>) + Send + 'static,
{
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });

    let info = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        host: opts.host.clone(),
        port: opts.port,
        known_hosts: opts.known_hosts.clone(),
        info: info.clone(),
    };

    let mut session = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect(config, (opts.host.as_str(), opts.port), handler),
    )
    .await
    .map_err(|_| SshError::Timeout)??;

    let auth = match opts.auth {
        Auth::Password(password) => session.authenticate_password(&opts.user, password).await?,
        Auth::KeyFile { path, passphrase } => {
            let key = load_secret_key(&path, passphrase.as_deref())?;
            let hash = session.best_supported_rsa_hash().await?.flatten();
            session
                .authenticate_publickey(&opts.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await?
        }
    };
    if !auth.success() {
        return Err(SshError::AuthFailed);
    }

    let channel = session.channel_open_session().await?;
    channel
        .request_pty(false, "xterm-256color", opts.cols, opts.rows, 0, 0, &[])
        .await?;
    channel.request_shell(false).await?;

    let (mut reader, writer) = channel.split();
    tokio::spawn(async move {
        let mut exit_status = None;
        while let Some(msg) = reader.wait().await {
            match msg {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    on_output(data.to_vec());
                }
                ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        on_exit(exit_status);
    });

    let key_info = info
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or(HostKeyInfo {
            fingerprint: String::new(),
            newly_trusted: false,
        });

    Ok((Shell { writer, session }, key_info))
}

/// 把 `~/xxx` 展开成用户主目录下的路径。
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home) = home {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_keeps_absolute_paths() {
        assert_eq!(expand_tilde("/etc/ssh/key"), PathBuf::from("/etc/ssh/key"));
    }

    #[test]
    fn expand_tilde_uses_home() {
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(
                expand_tilde("~/.ssh/id_ed25519"),
                PathBuf::from(home).join(".ssh/id_ed25519")
            );
        }
    }
}
