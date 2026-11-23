use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use vornel_core::{AuthKind, Host};
use vornel_ssh::{expand_tilde, open_shell, Auth, ConnectOptions};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------- 主机 ----------

#[tauri::command]
pub async fn hosts_list(state: State<'_, AppState>) -> CmdResult<Vec<Host>> {
    Ok(state.hosts.lock().await.list().to_vec())
}

#[tauri::command]
pub async fn hosts_save(state: State<'_, AppState>, host: Host) -> CmdResult<Host> {
    host.validate()?;
    state.hosts.lock().await.upsert(host.clone()).map_err(err)?;
    Ok(host)
}

#[tauri::command]
pub async fn hosts_delete(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.hosts.lock().await.remove(id).map_err(err)?;
    Ok(())
}

// ---------- SSH 会话 ----------

/// 发给前端的会话事件（终端输出走另一条二进制通道）。
#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionEvent {
    Connected {
        fingerprint: String,
        new_host_key: bool,
    },
    Closed {
        exit_status: Option<u32>,
    },
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    host_id: Uuid,
    password: Option<String>,
    passphrase: Option<String>,
    cols: u32,
    rows: u32,
    on_data: Channel,
    on_event: Channel<SessionEvent>,
) -> CmdResult<String> {
    let host = state
        .hosts
        .lock()
        .await
        .get(host_id)
        .cloned()
        .ok_or("主机不存在")?;

    let auth = match host.auth {
        AuthKind::Password => Auth::Password(password.ok_or("请输入密码")?),
        AuthKind::KeyFile { path } => Auth::KeyFile {
            path: expand_tilde(&path),
            passphrase: passphrase.filter(|p| !p.is_empty()),
        },
    };

    let session_id = Uuid::new_v4().to_string();

    let data_channel = on_data.clone();
    let on_output = move |bytes: Vec<u8>| {
        let _ = data_channel.send(InvokeResponseBody::Raw(bytes));
    };

    let exit_channel = on_event.clone();
    let exit_app = app.clone();
    let exit_id = session_id.clone();
    let on_exit = move |exit_status: Option<u32>| {
        let _ = exit_channel.send(SessionEvent::Closed { exit_status });
        tauri::async_runtime::spawn(async move {
            exit_app
                .state::<AppState>()
                .sessions
                .lock()
                .await
                .remove(&exit_id);
        });
    };

    let opts = ConnectOptions {
        host: host.address.clone(),
        port: host.port,
        user: host.username.clone(),
        auth,
        known_hosts: state.known_hosts.clone(),
        cols,
        rows,
    };

    let (shell, key) = open_shell(opts, on_output, on_exit).await.map_err(err)?;
    state
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), Arc::new(shell));

    let _ = on_event.send(SessionEvent::Connected {
        fingerprint: key.fingerprint,
        new_host_key: key.newly_trusted,
    });
    Ok(session_id)
}

#[tauri::command]
pub async fn ssh_write(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> CmdResult<()> {
    let shell = state.session(&session_id).await?;
    shell.write(data.into_bytes()).await.map_err(err)
}

#[tauri::command]
pub async fn ssh_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> CmdResult<()> {
    let shell = state.session(&session_id).await?;
    shell.resize(cols, rows).await.map_err(err)
}

#[tauri::command]
pub async fn ssh_close(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    let shell = state.sessions.lock().await.remove(&session_id);
    if let Some(shell) = shell {
        shell.close().await.map_err(err)?;
    }
    Ok(())
}
