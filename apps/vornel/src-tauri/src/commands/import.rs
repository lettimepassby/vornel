//! 从文件导入主机、密钥、代码片段、已知主机（用于从其他客户端迁移）。
//!
//! 文件格式（JSON）：
//! ```json
//! {
//!   "format": "vornel-import", "version": 1, "source": "Termius",
//!   "keys": [{ "ref": "k1", "name": "work", "privateKey": "-----BEGIN…", "passphrase": "" }],
//!   "hosts": [{ "ref": "h1", "name": "web", "address": "1.2.3.4", "port": 22, "username": "root",
//!               "group": "生产", "protocol": "ssh", "password": "…", "keyRef": "k1",
//!               "jumpRef": "h2", "agentForward": false }],
//!   "snippets": [{ "name": "磁盘", "command": "df -h", "run": false }],
//!   "knownHosts": [{ "host": "1.2.3.4", "port": 22, "publicKey": "ssh-ed25519 AAAA…" }]
//! }
//! ```
//! 重复导入是安全的：同名同地址的主机、同指纹的密钥、相同的已知主机和片段都会跳过。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;
use vornel_core::{AuthKind, Host, Key, KnownHost, Protocol, Secret, Snippet};
use vornel_vault::Vault;

use super::CmdResult;
use crate::state::AppState;
use crate::with_vault;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub format: String,
    #[serde(default)]
    pub keys: Vec<BundleKey>,
    #[serde(default)]
    pub hosts: Vec<BundleHost>,
    #[serde(default)]
    pub snippets: Vec<BundleSnippet>,
    #[serde(default)]
    pub known_hosts: Vec<BundleKnownHost>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleKey {
    #[serde(rename = "ref")]
    pub key_ref: String,
    #[serde(default)]
    pub name: String,
    pub private_key: String,
    #[serde(default)]
    pub passphrase: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleHost {
    #[serde(rename = "ref", default)]
    pub host_ref: String,
    pub name: String,
    pub address: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub protocol: Option<Protocol>,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub key_ref: Option<String>,
    #[serde(default)]
    pub jump_ref: Option<String>,
    #[serde(default)]
    pub agent_forward: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleSnippet {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub run: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleKnownHost {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    pub public_key: String,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub hosts: usize,
    pub keys: usize,
    pub passwords: usize,
    pub snippets: usize,
    pub known_hosts: usize,
    /// 已存在而跳过的条目数
    pub skipped: usize,
    /// 无法导入的条目（名称：原因）
    pub errors: Vec<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn clean_group(g: Option<String>) -> Option<String> {
    g.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// 把导入文件写入 Vault。
pub fn import_into(v: &mut Vault, bundle: Bundle) -> Result<ImportReport, String> {
    if bundle.format != "vornel-import" {
        return Err("不是 Vornel 导入文件（format 应为 vornel-import）".into());
    }
    let e = |x: vornel_vault::VaultError| x.to_string();
    let mut r = ImportReport::default();

    // ---- 密钥：同指纹的复用已有的
    let existing_keys = v.list::<Key>().map_err(e)?;
    let mut key_ids: HashMap<String, Uuid> = HashMap::new();
    for (i, k) in bundle.keys.into_iter().enumerate() {
        let pass = Some(k.passphrase.as_str()).filter(|p| !p.is_empty());
        let info = match vornel_ssh::keys::import_decrypted(&k.private_key, pass) {
            Ok(info) => info,
            Err(err) => {
                r.errors.push(format!("密钥 {}：{err}", k.name));
                continue;
            }
        };
        if let Some(old) = existing_keys
            .iter()
            .find(|x| x.fingerprint == info.fingerprint)
        {
            key_ids.insert(k.key_ref, old.id);
            r.skipped += 1;
            continue;
        }
        let name = k.name.trim();
        let name = if name.is_empty() || name.len() > 64 || name.contains('\n') {
            format!("导入的密钥 {}", i + 1)
        } else {
            name.to_string()
        };
        let key = Key {
            id: Uuid::new_v4(),
            name,
            algorithm: info.algorithm,
            public_key: info.public_key,
            private_key: info.private_key,
            encrypted: info.encrypted,
            fingerprint: info.fingerprint,
            sync: false,
        };
        v.put(&key).map_err(e)?;
        key_ids.insert(k.key_ref, key.id);
        r.keys += 1;
    }

    // ---- 主机：先分配 id，再处理跳板机引用
    let existing_hosts = v.list::<Host>().map_err(e)?;
    let mut host_ids: HashMap<String, Uuid> = HashMap::new();
    let mut pending: Vec<(Host, Option<String>, String)> = Vec::new();
    for h in bundle.hosts {
        let protocol = h.protocol.unwrap_or_default();
        let port = h
            .port
            .filter(|p| *p != 0)
            .unwrap_or(protocol.default_port());
        let (name, address, username) = (
            h.name.trim().to_string(),
            h.address.trim().to_string(),
            h.username.trim().to_string(),
        );
        if address.is_empty() {
            r.errors.push(format!("主机 {name}：没有地址"));
            continue;
        }
        if let Some(old) = existing_hosts.iter().find(|x| {
            x.name == name && x.address == address && x.port == port && x.username == username
        }) {
            if !h.host_ref.is_empty() {
                host_ids.insert(h.host_ref, old.id);
            }
            r.skipped += 1;
            continue;
        }
        let auth = match h.key_ref.as_ref().and_then(|k| key_ids.get(k)) {
            Some(id) => AuthKind::Key { key_id: *id },
            None => AuthKind::Password,
        };
        let host = Host {
            id: Uuid::new_v4(),
            name: if name.is_empty() {
                address.clone()
            } else {
                name
            },
            address,
            port,
            username,
            group: clean_group(h.group),
            protocol,
            auth,
            jump_host: None,
            agent_forward: h.agent_forward,
            save_password: !h.password.is_empty(),
            serial: None,
            domain: None,
        };
        if !h.host_ref.is_empty() {
            host_ids.insert(h.host_ref, host.id);
        }
        pending.push((host, h.jump_ref, h.password));
    }
    for (mut host, jump, password) in pending {
        host.jump_host = jump
            .and_then(|j| host_ids.get(&j).copied())
            .filter(|id| *id != host.id);
        v.put(&host).map_err(e)?;
        r.hosts += 1;
        if !password.is_empty() {
            v.put(&Secret {
                id: host.id,
                password,
                sync: false,
            })
            .map_err(e)?;
            r.passwords += 1;
        }
    }

    // ---- 代码片段：同名同命令跳过
    let existing_snips = v.list::<Snippet>().map_err(e)?;
    for s in bundle.snippets {
        if existing_snips
            .iter()
            .any(|x| x.name == s.name && x.command == s.command)
        {
            r.skipped += 1;
            continue;
        }
        v.put(&Snippet {
            id: Uuid::new_v4(),
            name: s.name,
            command: s.command,
            run: s.run,
        })
        .map_err(e)?;
        r.snippets += 1;
    }

    // ---- 已知主机：同主机、端口、公钥跳过
    let mut known = v.list::<KnownHost>().map_err(e)?;
    for k in bundle.known_hosts {
        let port = k.port.unwrap_or(22);
        let (algorithm, fingerprint, public_key) =
            match vornel_ssh::keys::public_key_info(&k.public_key) {
                Ok(x) => x,
                Err(err) => {
                    r.errors.push(format!("已知主机 {}：{err}", k.host));
                    continue;
                }
            };
        if known
            .iter()
            .any(|x| x.host == k.host && x.port == port && x.fingerprint == fingerprint)
        {
            r.skipped += 1;
            continue;
        }
        let rec = KnownHost {
            id: Uuid::new_v4(),
            host: k.host,
            port,
            algorithm,
            fingerprint,
            public_key,
            added_at: now_secs(),
        };
        v.put(&rec).map_err(e)?;
        known.push(rec);
        r.known_hosts += 1;
    }
    Ok(r)
}

/// 从文件导入（界面上“设置 → 安全 → 导入数据”）。
#[tauri::command]
pub async fn import_file(state: State<'_, AppState>, path: String) -> CmdResult<ImportReport> {
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{e}"))?;
    let bundle: Bundle = serde_json::from_str(&text).map_err(|e| format!("文件格式不正确：{e}"))?;
    with_vault!(state, |v| import_into(v, bundle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vornel_vault::MemoryKeyStore;

    fn vault() -> Vault {
        let p = std::env::temp_dir()
            .join(format!("vornel-import-{}", Uuid::new_v4()))
            .join("v.db");
        Vault::open(&p, &MemoryKeyStore::default()).unwrap()
    }

    fn bundle(pem: &str) -> Bundle {
        serde_json::from_value(serde_json::json!({
            "format": "vornel-import",
            "keys": [{ "ref": "k1", "name": "work", "privateKey": pem }],
            "hosts": [
                { "ref": "h1", "name": "web", "address": "10.0.0.1", "username": "root", "password": "pw1", "group": "生产", "jumpRef": "h2" },
                { "ref": "h2", "name": "bastion", "address": "10.0.0.2", "port": 2222, "username": "ops", "keyRef": "k1" },
                { "name": "tn", "address": "10.0.0.3", "protocol": "telnet" }
            ],
            "snippets": [{ "name": "disk", "command": "df -h" }],
            "knownHosts": [{ "host": "10.0.0.1", "port": 22, "publicKey": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl host" }]
        }))
        .unwrap()
    }

    #[test]
    fn imports_everything_and_is_idempotent() {
        let mut v = vault();
        let k = vornel_ssh::keys::generate_ed25519("t").unwrap();
        let r = import_into(&mut v, bundle(&k.private_key)).unwrap();
        assert_eq!(
            (r.hosts, r.keys, r.passwords, r.snippets, r.known_hosts),
            (3, 1, 1, 1, 1),
            "{r:?}"
        );
        assert!(r.errors.is_empty(), "{:?}", r.errors);

        let hosts = v.list::<Host>().unwrap();
        let web = hosts.iter().find(|h| h.name == "web").unwrap();
        let bastion = hosts.iter().find(|h| h.name == "bastion").unwrap();
        let tn = hosts.iter().find(|h| h.name == "tn").unwrap();
        assert_eq!(web.jump_host, Some(bastion.id));
        assert_eq!(web.group.as_deref(), Some("生产"));
        assert!(web.save_password);
        assert_eq!(v.get::<Secret>(web.id).unwrap().unwrap().password, "pw1");
        assert_eq!(bastion.port, 2222);
        assert!(matches!(bastion.auth, AuthKind::Key { .. }));
        assert_eq!((tn.protocol, tn.port), (Protocol::Telnet, 23));

        // 再导入一次：全部跳过
        let r = import_into(&mut v, bundle(&k.private_key)).unwrap();
        assert_eq!((r.hosts, r.keys, r.snippets, r.known_hosts), (0, 0, 0, 0));
        assert_eq!(r.skipped, 6);
        assert_eq!(v.list::<Host>().unwrap().len(), 3);
    }

    #[test]
    fn rejects_other_formats() {
        let mut v = vault();
        let b: Bundle = serde_json::from_value(serde_json::json!({ "format": "x" })).unwrap();
        assert!(import_into(&mut v, b).is_err());
    }
}
