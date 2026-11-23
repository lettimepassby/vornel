//! Telnet（RFC 854）与串口终端。两者都以“字节流 + 尺寸变化”的方式接到 xterm.js。

pub mod serial;
pub mod telnet;

use tokio::sync::mpsc;

#[derive(Debug, thiserror::Error)]
pub enum TermError {
    #[error("网络错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("串口错误：{0}")]
    Serial(String),
    #[error("{0}")]
    Unsupported(String),
}

pub(crate) enum Cmd {
    Write(Vec<u8>),
    Resize(u16, u16),
}

/// 一个打开的终端会话；丢弃时关闭连接。
pub struct TermSession {
    pub(crate) tx: mpsc::UnboundedSender<Cmd>,
}

impl TermSession {
    pub fn write(&self, data: Vec<u8>) {
        let _ = self.tx.send(Cmd::Write(data));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.tx.send(Cmd::Resize(cols, rows));
    }
}
