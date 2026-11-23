mod commands;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let hosts = vornel_core::HostStore::load(dir.join("hosts.json"))?;
            app.manage(state::AppState::new(hosts, dir.join("known_hosts")));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::hosts_list,
            commands::hosts_save,
            commands::hosts_delete,
            commands::ssh_connect,
            commands::ssh_write,
            commands::ssh_resize,
            commands::ssh_close,
        ])
        .run(tauri::generate_context!())
        .expect("Vornel 启动失败");
}
