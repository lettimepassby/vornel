use russh::client::Msg;
use russh::{ChannelMsg, ChannelWriteHalf};

use crate::client::Connection;
use crate::SshError;

/// 一个带 PTY 的交互式 Shell。方法只需要 `&self`，可在多个任务间共享。
pub struct Shell {
    writer: ChannelWriteHalf<Msg>,
}

impl Connection {
    /// 打开交互式 Shell。
    ///
    /// - `on_output`：远端输出（stdout 与 stderr）到达时调用
    /// - `on_exit`：Shell 结束时调用一次，参数是退出码
    pub async fn open_shell<O, E>(
        &self,
        cols: u32,
        rows: u32,
        mut on_output: O,
        on_exit: E,
    ) -> Result<Shell, SshError>
    where
        O: FnMut(Vec<u8>) + Send + 'static,
        E: FnOnce(Option<u32>) + Send + 'static,
    {
        let channel = self.open_session().await?;
        channel
            .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
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
        Ok(Shell { writer })
    }
}

impl Shell {
    pub async fn write(&self, data: Vec<u8>) -> Result<(), SshError> {
        self.writer.data_bytes(data).await?;
        Ok(())
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        self.writer.window_change(cols, rows, 0, 0).await?;
        Ok(())
    }

    pub async fn close(&self) {
        let _ = self.writer.eof().await;
        let _ = self.writer.close().await;
    }
}
