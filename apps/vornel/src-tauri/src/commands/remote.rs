//! RDP / VNC 远程桌面会话。

use std::sync::{Arc, Mutex as StdMutex};

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use vornel_core::{Host, KnownHost, Protocol, Secret};
use vornel_remote::rdp::{RdpOptions, ServerKey};
use vornel_remote::vnc::VncOptions;
use vornel_remote::{Event, Input};

use super::session::save_secret;
use super::{err, CmdResult};
use crate::state::AppState;
use crate::with_vault;

/// 记在“已知主机”里的 RDP 证书公钥使用的算法名。
const RDP_ALG: &str = "rdp-tls";

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RemoteEvent {
    Resize { width: u16, height: u16 },
    Connected,
    Closed { reason: Option<String> },
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn remote_open(
    app: AppHandle,
    state: State<'_, AppState>,
    host_id: Uuid,
    password: Option<String>,
    save_password: bool,
    width: u16,
    height: u16,
    on_frame: Channel,
    on_event: Channel<RemoteEvent>,
) -> CmdResult<String> {
    let (host, saved, known) = with_vault!(state, |v| {
        let host = v.get::<Host>(host_id).map_err(err)?.ok_or("主机不存在")?;
        let saved = v.get::<Secret>(host_id).map_err(err)?.map(|s| s.password);
        let known: Vec<KnownHost> = v
            .list::<KnownHost>()
            .map_err(err)?
            .into_iter()
            .filter(|k| k.host == host.address && k.port == host.port && k.algorithm == RDP_ALG)
            .collect();
        Ok::<_, String>((host, saved, known))
    })?;

    let typed = password.filter(|p| !p.is_empty());
    let pw = match (&typed, saved) {
        (Some(p), _) => p.clone(),
        (None, Some(p)) => p,
        (None, None) => return Err(format!("NEED_PASSWORD:{}", host.name)),
    };

    let session_id = Uuid::new_v4().to_string();
    let frames = on_frame.clone();
    let on_frame_cb = move |rect: Vec<u8>| {
        let _ = frames.send(InvokeResponseBody::Raw(rect));
    };
    let events = on_event.clone();
    let exit_app = app.clone();
    let exit_id = session_id.clone();
    let on_event_cb = move |e: Event| match e {
        Event::Resize { width, height } => {
            let _ = events.send(RemoteEvent::Resize { width, height });
        }
        Event::Closed(reason) => {
            let _ = events.send(RemoteEvent::Closed { reason });
            let app = exit_app.clone();
            let id = exit_id.clone();
            tauri::async_runtime::spawn(async move {
                app.state::<AppState>().remotes.lock().await.remove(&id);
            });
        }
    };

    let session = match host.protocol {
        Protocol::Rdp => {
            let seen: Arc<StdMutex<Option<ServerKey>>> = Arc::new(StdMutex::new(None));
            let seen_cb = seen.clone();
            let known_fps: Vec<String> = known.iter().map(|k| k.fingerprint.clone()).collect();
            let verify = move |key: &ServerKey| {
                if !known_fps.is_empty() && !known_fps.contains(&key.fingerprint) {
                    return Err(format!(
                        "服务器证书与已信任的不一致，可能存在中间人攻击，已拒绝连接。当前指纹：{}",
                        key.fingerprint
                    ));
                }
                if known_fps.is_empty() {
                    if let Ok(mut s) = seen_cb.lock() {
                        *s = Some(key.clone());
                    }
                }
                Ok(())
            };
            let opts = RdpOptions {
                host: host.address.trim().to_string(),
                port: host.port,
                username: host.username.trim().to_string(),
                password: pw.clone(),
                domain: host.domain.clone().filter(|d| !d.trim().is_empty()),
                width: width.max(640),
                height: height.max(480),
            };
            let session = vornel_remote::rdp::connect(opts, verify, on_frame_cb, on_event_cb)
                .await
                .map_err(err)?;
            let first_seen = seen.lock().ok().and_then(|mut s| s.take());
            if let Some(key) = first_seen {
                with_vault!(state, |v| v
                    .put(&KnownHost {
                        id: Uuid::new_v4(),
                        host: host.address.clone(),
                        port: host.port,
                        algorithm: RDP_ALG.into(),
                        fingerprint: key.fingerprint,
                        public_key: key.public_key,
                        added_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    })
                    .map_err(err))?;
            }
            session
        }
        Protocol::Vnc => vornel_remote::vnc::connect(
            VncOptions {
                host: host.address.trim().to_string(),
                port: host.port,
                password: pw.clone(),
            },
            on_frame_cb,
            on_event_cb,
        )
        .await
        .map_err(err)?,
        _ => return Err("该主机不是远程桌面协议".into()),
    };

    if save_password {
        if let Some(p) = typed {
            save_secret(&state, &host, p).await?;
        }
    }

    state
        .remotes
        .lock()
        .await
        .insert(session_id.clone(), session);
    let _ = on_event.send(RemoteEvent::Connected);
    Ok(session_id)
}

#[tauri::command]
pub async fn remote_input(
    state: State<'_, AppState>,
    session_id: String,
    input: Input,
) -> CmdResult<()> {
    match state.remotes.lock().await.get(&session_id) {
        Some(s) => {
            s.input(input);
            Ok(())
        }
        None => Err("会话已关闭".into()),
    }
}

#[tauri::command]
pub async fn remote_close(state: State<'_, AppState>, session_id: String) -> CmdResult<()> {
    // 丢弃会话即断开
    state.remotes.lock().await.remove(&session_id);
    Ok(())
}
