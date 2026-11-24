//! 云同步：后端配置、主密码、立即同步、OAuth 授权。

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use vornel_sync::oauth::{authorize, Provider};
use vornel_sync::{BackendConfig, SyncError, SyncReport};

use super::{err, CmdResult};
use crate::state::AppState;
use crate::with_vault;

const META_CONFIG: &str = "sync.config";

#[tauri::command]
pub async fn sync_get_config(state: State<'_, AppState>) -> CmdResult<Option<BackendConfig>> {
    with_vault!(state, |v| v.meta_get(META_CONFIG).map_err(err))
}

#[tauri::command]
pub async fn sync_set_config(
    state: State<'_, AppState>,
    config: Option<BackendConfig>,
) -> CmdResult<()> {
    if let Some(c) = &config {
        // 保存前先做一次读写测试，权限、密钥、地址的问题在这里就提示出来
        let op = c.operator().map_err(err)?;
        vornel_sync::probe(&op).await.map_err(|e| e.to_string())?;
    }
    with_vault!(state, |v| match &config {
        Some(c) => v.meta_set(META_CONFIG, c).map_err(err),
        None => v.meta_remove(META_CONFIG).map_err(err),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCheck {
    pub configured: bool,
    /// None：可以直接同步；Some(true)：需要设置新的主密码；Some(false)：需要输入已有主密码
    pub need_password: Option<bool>,
}

#[tauri::command]
pub async fn sync_check(state: State<'_, AppState>) -> CmdResult<SyncCheck> {
    let mut guard = state.vault.lock().await;
    let v = guard.as_mut().ok_or("Vault 未打开")?;
    let Some(cfg) = v.meta_get::<BackendConfig>(META_CONFIG).map_err(err)? else {
        return Ok(SyncCheck {
            configured: false,
            need_password: None,
        });
    };
    let op = cfg.operator().map_err(err)?;
    let need_password = vornel_sync::needs_password(&op, v).await.map_err(err)?;
    Ok(SyncCheck {
        configured: true,
        need_password,
    })
}

#[tauri::command]
pub async fn sync_now(
    state: State<'_, AppState>,
    password: Option<String>,
) -> CmdResult<SyncReport> {
    let mut guard = state.vault.lock().await;
    let v = guard.as_mut().ok_or("Vault 未打开")?;
    let cfg: BackendConfig = v
        .meta_get(META_CONFIG)
        .map_err(err)?
        .ok_or("还没有设置同步后端")?;
    let op = cfg.operator().map_err(err)?;
    match vornel_sync::sync(
        &op,
        v,
        &state.keys,
        password.as_deref().filter(|p| !p.is_empty()),
    )
    .await
    {
        Ok(r) => Ok(r),
        Err(SyncError::NeedPassword { first_time }) => Err(format!(
            "NEED_PASSWORD:{}",
            if first_time { "new" } else { "existing" }
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// 打开浏览器完成 OAuth 授权，成功后保存为当前同步后端。
#[tauri::command]
pub async fn sync_oauth(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    client_id: String,
    client_secret: String,
) -> CmdResult<BackendConfig> {
    let opener = app.clone();
    let cfg = authorize(provider, &client_id, &client_secret, move |url| {
        let _ = opener.opener().open_url(url, None::<&str>);
    })
    .await
    .map_err(err)?;
    with_vault!(state, |v| v.meta_set(META_CONFIG, &cfg).map_err(err))?;
    Ok(cfg)
}

/// 修改主密码（只重新包装数据密钥）。
#[tauri::command]
pub async fn sync_change_password(
    state: State<'_, AppState>,
    new_password: String,
) -> CmdResult<()> {
    let mut guard = state.vault.lock().await;
    let v = guard.as_mut().ok_or("Vault 未打开")?;
    let cfg: BackendConfig = v
        .meta_get(META_CONFIG)
        .map_err(err)?
        .ok_or("还没有设置同步后端")?;
    let op = cfg.operator().map_err(err)?;
    vornel_sync::change_password(&op, v, &new_password)
        .await
        .map_err(err)
}
