//! 终端会话、SFTP、端口转发。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use uuid::Uuid;
use vornel_core::{AuthKind, Forward, ForwardKind, Host, Key, KnownHost, Protocol, Secret};
use vornel_ssh::{connect, Auth, ConnectOptions, Connection, Endpoint, FileEntry, HostKey, Sftp};

use super::{err, CmdResult};
use crate::state::{AppState, RunningForward, Session};
use crate::with_vault;

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionEvent {
    HostKey {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        new: bool,
    },
    Connected,
    Closed {
        exit_status: Option<u32>,
    },
}

// ------------------------------------------------------------------ 建立连接

/// 根据主机记录组装一跳连接参数：认证凭据、已信任的主机公钥。
fn endpoint(
    v: &vornel_vault::Vault,
    host: &Host,
    password: Option<String>,
    passphrase: Option<String>,
) -> CmdResult<Endpoint> {
    let known_keys = v
        .list::<KnownHost>()
        .map_err(err)?
        .into_iter()
        .filter(|k| k.host == host.address && k.port == host.port)
        .map(|k| k.public_key)
        .collect();

    let auth = match &host.auth {
        AuthKind::Password => {
            let pw = match password {
                Some(p) => p,
                None => v
                    .get::<Secret>(host.id)
                    .map_err(err)?
                    .map(|s| s.password)
                    .ok_or_else(|| format!("NEED_PASSWORD:{}", host.name))?,
            };
            Auth::Password(pw)
        }
        AuthKind::Key { key_id } => {
            let key = v
                .get::<Key>(*key_id)
                .map_err(err)?
                .ok_or("主机使用的密钥已被删除")?;
            Auth::Key {
                pem: key.private_key,
                passphrase,
            }
        }
        AuthKind::KeyFile { path } => {
            let pem = std::fs::read_to_string(vornel_ssh::expand_tilde(path))
                .map_err(|e| format!("读取私钥文件失败：{e}"))?;
            Auth::Key { pem, passphrase }
        }
        AuthKind::Agent => Auth::Agent,
    };

    Ok(Endpoint {
        host: host.address.trim().to_string(),
        port: host.port,
        user: host.username.trim().to_string(),
        auth,
        known_keys,
    })
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 连接一台已保存的主机（必要时经由跳板机），并记录首次见到的主机公钥。
pub(crate) async fn connect_host(
    state: &AppState,
    host_id: Uuid,
    password: Option<String>,
    passphrase: Option<String>,
) -> CmdResult<(Arc<Connection>, Vec<HostKey>, Host)> {
    let (opts, host) = with_vault!(state, |v| {
        let host = v.get::<Host>(host_id).map_err(err)?.ok_or("主机不存在")?;
        let jump = match host.jump_host {
            Some(jid) => {
                let j = v.get::<Host>(jid).map_err(err)?.ok_or("跳板机已被删除")?;
                Some(endpoint(v, &j, None, None).map_err(|e| {
                    if e.starts_with("NEED_PASSWORD") {
                        format!(
                            "跳板机 {} 使用密码认证，请先在它上面保存密码，或改用密钥 / Agent",
                            j.name
                        )
                    } else {
                        e
                    }
                })?)
            }
            None => None,
        };
        let target = endpoint(v, &host, password, passphrase)?;
        Ok::<_, String>((
            ConnectOptions {
                target,
                jump,
                agent_forward: host.agent_forward,
            },
            host,
        ))
    })?;

    let (conn, keys) = connect(opts).await.map_err(err)?;

    with_vault!(state, |v| {
        for k in keys.iter().filter(|k| k.new) {
            v.put(&KnownHost {
                id: Uuid::new_v4(),
                host: k.host.clone(),
                port: k.port,
                algorithm: k.algorithm.clone(),
                fingerprint: k.fingerprint.clone(),
                public_key: k.public_key.clone(),
                added_at: now_secs(),
            })
            .map_err(err)?;
        }
        Ok::<_, String>(())
    })?;

    Ok((Arc::new(conn), keys, host))
}

// ------------------------------------------------------------------ 终端会话

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn session_open(
    app: AppHandle,
    state: State<'_, AppState>,
    host_id: Uuid,
    password: Option<String>,
    passphrase: Option<String>,
    save_password: bool,
    cols: u32,
    rows: u32,
    on_data: Channel,
    on_event: Channel<SessionEvent>,
) -> CmdResult<String> {
    let protocol = with_vault!(state, |v| v
        .get::<Host>(host_id)
        .map_err(err)?
        .map(|h| h.protocol)
        .ok_or("主机不存在"))?;
    if protocol != Protocol::Ssh {
        return Err("该协议的会话请使用对应的入口".into());
    }

    let password = password.filter(|p| !p.is_empty());
    let (conn, keys, host) = connect_host(
        &state,
        host_id,
        password.clone(),
        passphrase.filter(|p| !p.is_empty()),
    )
    .await?;
    for k in keys {
        let _ = on_event.send(SessionEvent::HostKey {
            host: k.host,
            port: k.port,
            algorithm: k.algorithm,
            fingerprint: k.fingerprint,
            new: k.new,
        });
    }

    if save_password {
        if let Some(pw) = password {
            with_vault!(state, |v| {
                let sync = v
                    .get::<vornel_core::Prefs>(Uuid::nil())
                    .map_err(err)?
                    .unwrap_or_default()
                    .sync_secrets;
                v.put(&Secret {
                    id: host.id,
                    password: pw,
                    sync,
                })
                .map_err(err)?;
                v.put(&Host {
                    save_password: true,
                    ..host.clone()
                })
                .map_err(err)
            })?;
        }
    }

    let session_id = Uuid::new_v4().to_string();
    let data = on_data.clone();
    let exit_channel = on_event.clone();
    let exit_app = app.clone();
    let exit_id = session_id.clone();
    let shell = conn
        .open_shell(
            cols,
            rows,
            move |bytes| {
                let _ = data.send(InvokeResponseBody::Raw(bytes));
            },
            move |exit_status| {
                let _ = exit_channel.send(SessionEvent::Closed { exit_status });
                tauri::async_runtime::spawn(async move {
                    let state = exit_app.state::<AppState>();
                    let removed = state.sessions.lock().await.remove(&exit_id);
                    if let Some(s) = removed {
                        let Session::Ssh { conn, .. } = &*s;
                        conn.disconnect().await;
                    }
                });
            },
        )
        .await
        .map_err(err)?;

    state.sessions.lock().await.insert(
        session_id.clone(),
        Arc::new(Session::Ssh {
            conn,
            shell,
            sftp: Mutex::new(None),
        }),
    );
    let _ = on_event.send(SessionEvent::Connected);
    Ok(session_id)
}

#[tauri::command]
pub async fn session_write(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> CmdResult<()> {
    match &*state.session(&session_id).await? {
        Session::Ssh { shell, .. } => shell.write(data.into_bytes()).await.map_err(err),
    }
}

#[tauri::command]
pub async fn session_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> CmdResult<()> {
    match &*state.session(&session_id).await? {
        Session::Ssh { shell, .. } => shell.resize(cols, rows).await.map_err(err),
    }
}

#[tauri::command]
pub async fn session_close(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    let removed = state.sessions.lock().await.remove(&session_id);
    if let Some(s) = removed {
        match &*s {
            Session::Ssh { conn, shell, sftp } => {
                if let Some(sftp) = sftp.lock().await.take() {
                    sftp.close().await;
                }
                shell.close().await;
                conn.disconnect().await;
            }
        }
    }
    Ok(())
}

/// 主机密钥变化被拒绝后，用户确认“这是预期的变化”：删除旧记录，下次连接重新信任。
#[tauri::command]
pub async fn host_key_trust(state: State<'_, AppState>, host: String, port: u16) -> CmdResult<()> {
    with_vault!(state, |v| {
        for k in v.list::<KnownHost>().map_err(err)? {
            if k.host == host && k.port == port {
                v.delete::<KnownHost>(k.id).map_err(err)?;
            }
        }
        Ok(())
    })
}

// ------------------------------------------------------------------ SFTP

async fn sftp_of(state: &AppState, session_id: &str) -> CmdResult<Arc<Sftp>> {
    let session = state.session(session_id).await?;
    let Session::Ssh { conn, sftp, .. } = &*session;
    let mut slot = sftp.lock().await;
    if let Some(s) = &*slot {
        return Ok(s.clone());
    }
    let s = Arc::new(conn.open_sftp().await.map_err(err)?);
    *slot = Some(s.clone());
    Ok(s)
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

#[tauri::command]
pub async fn sftp_home(state: State<'_, AppState>, session_id: String) -> CmdResult<String> {
    sftp_of(&state, &session_id)
        .await?
        .canonicalize(".")
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn sftp_list(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> CmdResult<Vec<FileEntry>> {
    sftp_of(&state, &session_id)
        .await?
        .list(&path)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn sftp_mkdir(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> CmdResult<()> {
    sftp_of(&state, &session_id)
        .await?
        .mkdir(&path)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    session_id: String,
    from: String,
    to: String,
) -> CmdResult<()> {
    sftp_of(&state, &session_id)
        .await?
        .rename(&from, &to)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> CmdResult<()> {
    sftp_of(&state, &session_id)
        .await?
        .remove(&path)
        .await
        .map_err(err)
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// 下载远端文件到本机目录，返回本机路径。
#[tauri::command]
pub async fn sftp_download(
    state: State<'_, AppState>,
    session_id: String,
    remote: String,
    local_dir: String,
    on_progress: Channel<Progress>,
) -> CmdResult<String> {
    let sftp = sftp_of(&state, &session_id).await?;
    let local = PathBuf::from(&local_dir).join(file_name(&remote));
    sftp.download(&remote, &local, |done, total| {
        let _ = on_progress.send(Progress { done, total });
    })
    .await
    .map_err(err)?;
    Ok(local.to_string_lossy().into_owned())
}

/// 上传本机文件到远端目录，返回远端路径。
#[tauri::command]
pub async fn sftp_upload(
    state: State<'_, AppState>,
    session_id: String,
    local: String,
    remote_dir: String,
    on_progress: Channel<Progress>,
) -> CmdResult<String> {
    let sftp = sftp_of(&state, &session_id).await?;
    let remote = format!("{}/{}", remote_dir.trim_end_matches('/'), file_name(&local));
    sftp.upload(PathBuf::from(&local).as_path(), &remote, |done, total| {
        let _ = on_progress.send(Progress { done, total });
    })
    .await
    .map_err(err)?;
    Ok(remote)
}

#[tauri::command]
pub async fn local_home(app: AppHandle) -> CmdResult<String> {
    app.path()
        .home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(err)
}

#[tauri::command]
pub async fn local_list(path: String) -> CmdResult<Vec<FileEntry>> {
    vornel_ssh::list_local(PathBuf::from(path).as_path())
        .await
        .map_err(err)
}

// ------------------------------------------------------------------ 端口转发

/// 启动一条端口转发；返回实际监听的端口。
#[tauri::command]
pub async fn forward_start(
    state: State<'_, AppState>,
    forward_id: Uuid,
    password: Option<String>,
) -> CmdResult<u16> {
    if state.forwards.lock().await.contains_key(&forward_id) {
        return Err("这条转发已经在运行".into());
    }
    let fwd = with_vault!(state, |v| v
        .get::<Forward>(forward_id)
        .map_err(err)?
        .ok_or("端口转发规则不存在"))?;
    let (conn, _, _) = connect_host(
        &state,
        fwd.host_id,
        password.filter(|p| !p.is_empty()),
        None,
    )
    .await?;
    let bind = fwd.bind_address.trim().to_string();
    let handle = match fwd.kind {
        ForwardKind::Local => {
            conn.local_forward(
                &bind,
                fwd.bind_port,
                fwd.target_host.clone(),
                fwd.target_port,
            )
            .await
        }
        ForwardKind::Dynamic => conn.dynamic_forward(&bind, fwd.bind_port).await,
        ForwardKind::Remote => {
            conn.remote_forward(
                &bind,
                fwd.bind_port,
                fwd.target_host.clone(),
                fwd.target_port,
            )
            .await
        }
    }
    .map_err(err)?;
    let port = handle.bound_port;
    state
        .forwards
        .lock()
        .await
        .insert(forward_id, RunningForward { conn, handle });
    Ok(port)
}

#[tauri::command]
pub async fn forward_stop(state: State<'_, AppState>, forward_id: Uuid) -> CmdResult<()> {
    let running = state.forwards.lock().await.remove(&forward_id);
    if let Some(RunningForward { conn, handle }) = running {
        conn.stop_forward(handle).await;
        conn.disconnect().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn forward_running(state: State<'_, AppState>) -> CmdResult<Vec<Uuid>> {
    Ok(state.forwards.lock().await.keys().copied().collect())
}
