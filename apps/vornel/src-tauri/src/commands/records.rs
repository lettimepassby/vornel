//! 通用的记录读写：主机、密钥（不含私钥）、代码片段、端口转发、已知主机、偏好设置。

use serde::Serialize;
use serde_json::Value;
use tauri::State;
use uuid::Uuid;
use vornel_core::{Forward, Host, Key, KnownHost, Prefs, Record, Secret, Snippet};
use vornel_vault::Vault;

use super::{err, CmdResult};
use crate::state::AppState;
use crate::with_vault;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub ok: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn vault_status(state: State<'_, AppState>) -> CmdResult<VaultStatus> {
    let ok = state.vault.lock().await.is_some();
    Ok(VaultStatus {
        ok,
        error: state.vault_error.lock().await.clone(),
    })
}

/// 本机数据密钥丢失时：清空本机 Vault 重新开始（云端数据可以再同步回来）。
#[tauri::command]
pub async fn vault_reset(state: State<'_, AppState>) -> CmdResult<()> {
    let mut guard = state.vault.lock().await;
    *guard = None;
    Vault::reset(&state.vault_path, &state.keys).map_err(err)?;
    *guard = Some(Vault::open(&state.vault_path, &state.keys).map_err(err)?);
    *state.vault_error.lock().await = None;
    Ok(())
}

/// 私钥永远不发给界面，界面只需要名称、公钥和指纹。
fn strip_key(mut k: Key) -> Key {
    k.private_key.clear();
    k
}

fn to_values<T: Serialize>(items: Vec<T>) -> CmdResult<Vec<Value>> {
    items
        .into_iter()
        .map(|i| serde_json::to_value(i).map_err(err))
        .collect()
}

#[tauri::command]
pub async fn records_list(state: State<'_, AppState>, kind: String) -> CmdResult<Vec<Value>> {
    with_vault!(state, |v| match kind.as_str() {
        "host" => to_values(v.list::<Host>().map_err(err)?),
        "key" => to_values(
            v.list::<Key>()
                .map_err(err)?
                .into_iter()
                .map(strip_key)
                .collect()
        ),
        "snippet" => to_values(v.list::<Snippet>().map_err(err)?),
        "forward" => to_values(v.list::<Forward>().map_err(err)?),
        "known_host" => to_values(v.list::<KnownHost>().map_err(err)?),
        "prefs" => to_values(vec![v
            .get::<Prefs>(Uuid::nil())
            .map_err(err)?
            .unwrap_or_default()]),
        other => Err(format!("不支持的类型：{other}")),
    })
}

fn parse<T: Record>(value: Value) -> CmdResult<T> {
    serde_json::from_value(value).map_err(|e| format!("数据格式错误：{e}"))
}

#[tauri::command]
pub async fn record_save(state: State<'_, AppState>, kind: String, value: Value) -> CmdResult<()> {
    with_vault!(state, |v| {
        match kind.as_str() {
            "host" => {
                let h: Host = parse(value)?;
                h.validate()?;
                if !h.save_password {
                    v.delete::<Secret>(h.id).map_err(err)?;
                }
                v.put(&h).map_err(err)
            }
            "key" => {
                // 只允许改名称和是否同步；私钥本身只能通过生成 / 导入写入
                let incoming: Key = parse(value)?;
                let mut k = v
                    .get::<Key>(incoming.id)
                    .map_err(err)?
                    .ok_or("密钥不存在")?;
                k.name = incoming.name;
                k.sync = incoming.sync;
                v.put(&k).map_err(err)
            }
            "secret" => {
                // 主机的保存密码（编辑主机时直接填写）；是否同步跟随“同步密码”设置
                let mut s: Secret = parse(value)?;
                if v.get::<Host>(s.id).map_err(err)?.is_none() {
                    return Err("主机不存在".into());
                }
                if s.password.is_empty() {
                    return v.delete::<Secret>(s.id).map(|_| ()).map_err(err);
                }
                s.sync = v
                    .get::<Prefs>(Uuid::nil())
                    .map_err(err)?
                    .unwrap_or_default()
                    .sync_secrets;
                v.put(&s).map_err(err)
            }
            "snippet" => {
                let s: Snippet = parse(value)?;
                if s.name.trim().is_empty() || s.command.trim().is_empty() {
                    return Err("名称和命令都不能为空".into());
                }
                v.put(&s).map_err(err)
            }
            "forward" => {
                let f: Forward = parse(value)?;
                f.validate()?;
                v.put(&f).map_err(err)
            }
            "prefs" => {
                let mut p: Prefs = parse(value)?;
                p.id = Uuid::nil();
                if p.group_styles.values().any(|g| {
                    g.image
                        .as_ref()
                        .is_some_and(|i| i.len() > 64 * 1024 || !i.starts_with("data:image/"))
                }) {
                    return Err("分组图片太大或格式不正确".into());
                }
                let before = v
                    .get::<Prefs>(Uuid::nil())
                    .map_err(err)?
                    .unwrap_or_default();
                if before.sync_secrets != p.sync_secrets {
                    // 切换“同步密码”时更新所有已保存密码的同步标记
                    for mut s in v.list::<Secret>().map_err(err)? {
                        s.sync = p.sync_secrets;
                        v.put(&s).map_err(err)?;
                    }
                }
                v.put(&p).map_err(err)
            }
            other => Err(format!("不支持的类型：{other}")),
        }
    })
}

#[tauri::command]
pub async fn record_delete(state: State<'_, AppState>, kind: String, id: Uuid) -> CmdResult<()> {
    with_vault!(state, |v| {
        match kind.as_str() {
            "host" => {
                v.delete::<Host>(id).map_err(err)?;
                v.delete::<Secret>(id).map_err(err)?;
                for f in v.list::<Forward>().map_err(err)? {
                    if f.host_id == id {
                        v.delete::<Forward>(f.id).map_err(err)?;
                    }
                }
            }
            "key" => {
                v.delete::<Key>(id).map_err(err)?;
            }
            "secret" => {
                v.delete::<Secret>(id).map_err(err)?;
            }
            "snippet" => {
                v.delete::<Snippet>(id).map_err(err)?;
            }
            "forward" => {
                v.delete::<Forward>(id).map_err(err)?;
            }
            "known_host" => {
                v.delete::<KnownHost>(id).map_err(err)?;
            }
            other => return Err(format!("不支持的类型：{other}")),
        }
        Ok(())
    })
}

/// 读取某台主机保存的密码（编辑主机时点“显示密码”）。没有保存时返回 None。
#[tauri::command]
pub async fn secret_get(state: State<'_, AppState>, host_id: Uuid) -> CmdResult<Option<String>> {
    with_vault!(state, |v| Ok(v
        .get::<Secret>(host_id)
        .map_err(err)?
        .map(|s| s.password)))
}

/// 记录主机最近一次连接成功的时间（主机列表按它排序，随同步到其他设备）。
#[tauri::command]
pub async fn host_touch(state: State<'_, AppState>, host_id: Uuid) -> CmdResult<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    with_vault!(state, |v| {
        if let Some(h) = v.get::<Host>(host_id).map_err(err)? {
            v.put(&Host {
                last_connected: Some(now),
                ..h
            })
            .map_err(err)?;
        }
        Ok(now)
    })
}
