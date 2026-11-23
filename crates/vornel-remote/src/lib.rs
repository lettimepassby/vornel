//! Vornel 的远程桌面（ARCHITECTURE.md：“只更新变化的区域”）。
//!
//! 后端解码画面，按矩形区域推送 RGBA 像素：每条消息 = 8 字节头（x, y, w, h，各 u16 小端）+ w×h×4 字节像素。
//! 前端直接 `putImageData` 到 canvas，不走 JSON。

mod keys;
pub mod rdp;
pub mod vnc;

use serde::Deserialize;
use tokio::sync::mpsc;

/// 来自界面的输入事件。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Input {
    /// 鼠标：button 0 左键、1 中键、2 右键；`move` 为 true 表示仅移动
    Pointer {
        x: u16,
        y: u16,
        button: u8,
        down: bool,
        #[serde(rename = "move")]
        moved: bool,
    },
    /// 滚轮：delta 为正表示向上
    Wheel { x: u16, y: u16, delta: i16 },
    /// 键盘：`code` 是物理键位（KeyboardEvent.code），`key` 是字符（KeyboardEvent.key）
    Key {
        code: String,
        key: String,
        down: bool,
    },
}

/// 会话事件。
#[derive(Debug, Clone)]
pub enum Event {
    Resize { width: u16, height: u16 },
    Closed(Option<String>),
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("网络错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("RDP 连接失败：{0}")]
    Rdp(String),
    #[error("VNC 连接失败：{0}")]
    Vnc(String),
    #[error("{0}")]
    Rejected(String),
}

/// 一个远程桌面会话。丢弃时（输入通道关闭）会话循环随之结束并断开连接。
pub struct RemoteSession {
    tx: mpsc::UnboundedSender<Input>,
}

impl RemoteSession {
    pub fn input(&self, input: Input) {
        let _ = self.tx.send(input);
    }
}

/// 把 RGBA 帧缓冲中的一个矩形编码为一条消息；alpha 统一置为 255。
pub(crate) fn encode_rect(fb: &[u8], stride: usize, x: u16, y: u16, w: u16, h: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + w as usize * h as usize * 4);
    for v in [x, y, w, h] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let row_bytes = w as usize * 4;
    for row in 0..h as usize {
        let start = (y as usize + row) * stride + x as usize * 4;
        let Some(src) = fb.get(start..start + row_bytes) else {
            break;
        };
        for px in src.chunks_exact(4) {
            out.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_rect_header_and_alpha() {
        // 2×2 的帧缓冲，取右下角 1×1
        let fb = vec![
            1, 2, 3, 0, 4, 5, 6, 0, //
            7, 8, 9, 0, 10, 11, 12, 0,
        ];
        let out = encode_rect(&fb, 8, 1, 1, 1, 1);
        assert_eq!(&out[..8], &[1, 0, 1, 0, 1, 0, 1, 0]);
        assert_eq!(&out[8..], &[10, 11, 12, 255]);
    }

    #[test]
    fn input_json_shape() {
        let i: Input = serde_json::from_str(
            r#"{"type":"pointer","x":1,"y":2,"button":0,"down":true,"move":false}"#,
        )
        .unwrap();
        assert!(matches!(
            i,
            Input::Pointer {
                down: true,
                moved: false,
                ..
            }
        ));
        let k: Input =
            serde_json::from_str(r#"{"type":"key","code":"KeyA","key":"a","down":true}"#).unwrap();
        assert!(matches!(k, Input::Key { .. }));
    }
}
