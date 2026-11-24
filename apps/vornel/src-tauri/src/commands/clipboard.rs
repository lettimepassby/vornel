//! 剪贴板读写（右键菜单的复制 / 粘贴）。
//!
//! WebView 里直接读剪贴板在 macOS 上会弹出系统确认，所以桌面端走原生剪贴板；
//! iOS 没有这两个命令的实现，界面退回到 WebView 的剪贴板接口。

use super::CmdResult;

/// 整个应用共用一个剪贴板对象：Linux（X11 / Wayland）上剪贴板内容由进程持有，
/// 对象释放后内容就没了，所以不能每次用完就丢。
#[cfg(desktop)]
fn with_clipboard<T>(
    f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, arboard::Error> {
    use std::sync::{Mutex, OnceLock};
    static CLIP: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    let mut guard = CLIP
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new()?);
    }
    f(guard.as_mut().expect("剪贴板已初始化"))
}

#[tauri::command]
pub async fn clipboard_read() -> CmdResult<String> {
    #[cfg(desktop)]
    {
        match with_clipboard(|c| c.get_text()) {
            Ok(t) => Ok(t),
            Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
            Err(e) => Err(format!("读取剪贴板失败：{e}")),
        }
    }
    #[cfg(not(desktop))]
    {
        Err("unsupported".into())
    }
}

#[tauri::command]
pub async fn clipboard_write(text: String) -> CmdResult<()> {
    #[cfg(desktop)]
    {
        with_clipboard(|c| c.set_text(text)).map_err(|e| format!("写入剪贴板失败：{e}"))
    }
    #[cfg(not(desktop))]
    {
        let _ = text;
        Err("unsupported".into())
    }
}
