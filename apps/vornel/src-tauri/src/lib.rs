mod commands;
mod state;

use std::collections::HashMap;
use std::path::Path;

#[cfg(target_os = "macos")]
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;
use tokio::sync::Mutex;
use vornel_vault::{SystemKeyStore, Vault};

use crate::state::AppState;

/// 旧版本把主机存在 hosts.json，首次启动新版本时导入 Vault。
fn migrate_hosts_json(dir: &Path, vault: &mut Vault) {
    let old = dir.join("hosts.json");
    let Ok(bytes) = std::fs::read(&old) else {
        return;
    };
    if let Ok(hosts) = serde_json::from_slice::<Vec<vornel_core::Host>>(&bytes) {
        for h in &hosts {
            let _ = vault.put(h);
        }
        let _ = std::fs::rename(&old, dir.join("hosts.json.migrated"));
    }
}

/// macOS 菜单栏（显示在屏幕顶部的系统菜单里）。
/// Windows / Linux 不使用菜单栏：它会挂在窗口顶部、颜色不跟随主题；快捷键由前端处理。
#[cfg(target_os = "macos")]
fn build_menu(app: &tauri::App) -> tauri::Result<()> {
    // macOS 默认菜单的 ⌘W 会关闭整个窗口；这里换成“关闭标签”，交给前端处理
    let close_tab = MenuItemBuilder::with_id("close-tab", "关闭标签")
        .accelerator("CmdOrCtrl+W")
        .build(app)?;
    let new_host = MenuItemBuilder::with_id("new-host", "新建主机")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "设置…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "Vornel")
        .item(&PredefinedMenuItem::about(app, Some("关于 Vornel"), None)?)
        .separator()
        .item(&settings)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some("隐藏 Vornel"))?)
        .item(&PredefinedMenuItem::hide_others(app, Some("隐藏其他"))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some("退出 Vornel"))?)
        .build()?;
    let file = SubmenuBuilder::new(app, "文件")
        .item(&new_host)
        .item(&close_tab)
        .build()?;
    // 编辑菜单必须有，否则 macOS 上的复制粘贴快捷键在输入框里不生效
    let edit = SubmenuBuilder::new(app, "编辑")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let window = SubmenuBuilder::new(app, "窗口")
        .minimize()
        .item(&PredefinedMenuItem::fullscreen(app, Some("全屏"))?)
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &window])
        .build()?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if matches!(id, "close-tab" | "new-host" | "settings") {
            let _ = app.emit("menu", id);
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    vornel_sync::init_tls();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_uri_scheme_protocol("vornel-font", commands::fonts::protocol)
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let vault_path = dir.join("vault.db");
            let keys = SystemKeyStore::new("com.lettimepassby.vornel", dir.join("vault.key"));

            let (vault, error) = match Vault::open(&vault_path, &keys) {
                Ok(mut v) => {
                    migrate_hosts_json(&dir, &mut v);
                    (Some(v), None)
                }
                Err(e) => (None, Some(e.to_string())),
            };

            app.manage(AppState {
                vault: Mutex::new(vault),
                vault_error: Mutex::new(error),
                vault_path,
                keys,
                sessions: Mutex::new(HashMap::new()),
                forwards: Mutex::new(HashMap::new()),
                remotes: Mutex::new(HashMap::new()),
            });

            #[cfg(target_os = "macos")]
            build_menu(app)?;
            // Windows：去掉系统标题栏，由界面绘制与主题一致的顶栏和窗口按钮
            #[cfg(windows)]
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_decorations(false);
                let _ = w.set_shadow(true);
            }
            #[cfg(desktop)]
            {
                // 应用内更新：检查、下载、安装由前端驱动；装完用 process 插件重启。
                // iOS 走 App Store，不注册这两个插件。
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
                app.handle().plugin(tauri_plugin_process::init())?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::records::vault_status,
            commands::records::vault_reset,
            commands::records::records_list,
            commands::records::record_save,
            commands::records::record_delete,
            commands::records::secret_get,
            commands::keys::key_generate,
            commands::keys::key_import,
            commands::session::session_open,
            commands::session::session_write,
            commands::session::session_resize,
            commands::session::session_close,
            commands::session::host_key_trust,
            commands::session::sftp_home,
            commands::session::sftp_list,
            commands::session::sftp_mkdir,
            commands::session::sftp_rename,
            commands::session::sftp_remove,
            commands::session::sftp_download,
            commands::session::sftp_upload,
            commands::session::local_home,
            commands::session::local_list,
            commands::session::forward_start,
            commands::session::forward_stop,
            commands::session::forward_running,
            commands::sync::sync_get_config,
            commands::sync::sync_set_config,
            commands::sync::sync_check,
            commands::sync::sync_now,
            commands::sync::sync_oauth,
            commands::sync::sync_change_password,
            commands::import::import_file,
            commands::fonts::font_addons,
            commands::fonts::system_fonts,
            commands::remote::remote_open,
            commands::remote::remote_input,
            commands::remote::remote_close,
            commands::session::serial_ports,
            commands::fonts::font_addon_install,
            commands::fonts::font_addon_remove,
        ])
        .run(tauri::generate_context!())
        .expect("Vornel 启动失败");
}
