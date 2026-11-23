//! Vornel 的 SSH：连接（含跳板机）、主机密钥校验、密码 / 密钥 / Agent 认证、
//! 交互式 Shell、命令执行、SFTP、本地 / 远程 / 动态端口转发、Agent 转发、密钥生成与导入。

mod agent;
mod client;
pub mod keys;
mod sftp;
mod shell;

pub use client::{connect, Auth, ConnectOptions, Connection, Endpoint, ForwardHandle, HostKey};
pub use sftp::{list_local, FileEntry, Sftp};
pub use shell::Shell;

/// SSH 层的错误。消息直接展示给用户，所以用中文。
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("连接失败：{0}")]
    Protocol(#[from] russh::Error),
    #[error("密钥错误：{0}")]
    Key(#[from] russh::keys::Error),
    #[error("认证失败：用户名、密码或私钥不正确")]
    AuthFailed,
    #[error("私钥需要口令")]
    NeedPassphrase,
    #[error("没有可用的 SSH Agent（请确认 ssh-agent 正在运行）")]
    AgentUnavailable,
    #[error(
        "{host}:{port} 的主机密钥与已信任的不一致，可能存在中间人攻击，已拒绝连接。当前指纹：{fingerprint}"
    )]
    HostKeyChanged {
        host: String,
        port: u16,
        fingerprint: String,
    },
    #[error("连接超时")]
    Timeout,
    #[error("网络错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("SFTP 错误：{0}")]
    Sftp(String),
    #[error("{0}")]
    Other(String),
}

impl From<russh_sftp::client::error::Error> for SshError {
    fn from(e: russh_sftp::client::error::Error) -> Self {
        SshError::Sftp(e.to_string())
    }
}

/// 把 `~/xxx` 展开成用户主目录下的路径。
pub fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home) = home {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_keeps_absolute_paths() {
        assert_eq!(
            expand_tilde("/etc/ssh/key"),
            std::path::PathBuf::from("/etc/ssh/key")
        );
    }
}
