//! 密钥串：生成与导入。私钥只保存在 Vault 里，不会返回给界面。

use tauri::State;
use uuid::Uuid;
use vornel_core::Key;
use vornel_ssh::keys::{generate_ed25519, import, KeyInfo};

use super::{err, CmdResult};
use crate::state::AppState;
use crate::with_vault;

fn to_key(name: String, info: KeyInfo) -> Key {
    Key {
        id: Uuid::new_v4(),
        name,
        algorithm: info.algorithm,
        public_key: info.public_key,
        private_key: info.private_key,
        encrypted: info.encrypted,
        fingerprint: info.fingerprint,
        sync: false,
    }
}

fn public_view(k: &Key) -> Key {
    Key {
        private_key: String::new(),
        ..k.clone()
    }
}

#[tauri::command]
pub async fn key_generate(state: State<'_, AppState>, name: String) -> CmdResult<Key> {
    let name = if name.trim().is_empty() {
        "vornel".to_string()
    } else {
        name.trim().to_string()
    };
    let key = to_key(name.clone(), generate_ed25519(&name).map_err(err)?);
    with_vault!(state, |v| v.put(&key).map_err(err))?;
    Ok(public_view(&key))
}

#[tauri::command]
pub async fn key_import(
    state: State<'_, AppState>,
    name: String,
    pem: String,
    passphrase: Option<String>,
) -> CmdResult<Key> {
    let info = import(&pem, passphrase.as_deref().filter(|p| !p.is_empty())).map_err(err)?;
    let name = if name.trim().is_empty() {
        info.algorithm.clone()
    } else {
        name.trim().to_string()
    };
    let key = to_key(name, info);
    with_vault!(state, |v| v.put(&key).map_err(err))?;
    Ok(public_view(&key))
}
