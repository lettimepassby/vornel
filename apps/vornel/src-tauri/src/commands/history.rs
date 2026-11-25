//! 命令历史（命令联想用）：加密保存在本机 Vault 的本机设置里，不参与云同步。

use tauri::State;
use uuid::Uuid;
use vornel_core::{command_history, HistoryEntry};

use super::{err, CmdResult};
use crate::state::AppState;
use crate::with_vault;

const META_HISTORY: &str = "command_history";

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tauri::command]
pub async fn history_list(state: State<'_, AppState>) -> CmdResult<Vec<HistoryEntry>> {
    with_vault!(state, |v| Ok(v
        .meta_get::<Vec<HistoryEntry>>(META_HISTORY)
        .map_err(err)?
        .unwrap_or_default()))
}

/// 记录一条执行过的命令；返回是否记下了（空命令、以空格开头的命令等不记录）。
#[tauri::command]
pub async fn history_add(
    state: State<'_, AppState>,
    host_id: Option<Uuid>,
    command: String,
) -> CmdResult<bool> {
    if command_history::normalize(&command).is_none() {
        return Ok(false);
    }
    with_vault!(state, |v| {
        let mut list: Vec<HistoryEntry> =
            v.meta_get(META_HISTORY).map_err(err)?.unwrap_or_default();
        let added = command_history::record(&mut list, host_id, &command, now_ms());
        if added {
            v.meta_set(META_HISTORY, &list).map_err(err)?;
        }
        Ok(added)
    })
}

/// 从历史里删掉一条命令（所有主机上的记录）。
#[tauri::command]
pub async fn history_remove(state: State<'_, AppState>, command: String) -> CmdResult<()> {
    with_vault!(state, |v| {
        let mut list: Vec<HistoryEntry> =
            v.meta_get(META_HISTORY).map_err(err)?.unwrap_or_default();
        if command_history::remove(&mut list, &command) > 0 {
            v.meta_set(META_HISTORY, &list).map_err(err)?;
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn history_clear(state: State<'_, AppState>) -> CmdResult<()> {
    with_vault!(state, |v| v.meta_remove(META_HISTORY).map_err(err))
}
