//! 端到端加密的云同步（ARCHITECTURE.md 第 3 节）。
//!
//! 云端目录结构：
//! ```text
//! vornel/
//! ├─ vault.json        主密码包装后的数据密钥（明文 JSON，不含任何用户数据）
//! ├─ manifest.bin      加密的记录索引：键 → 版本（HLC）
//! └─ records/<kind>-<uuid>.bin   每条记录一个加密文件
//! ```

pub mod oauth;

use std::collections::BTreeMap;

use opendal::layers::{RetryLayer, TimeoutLayer};
use opendal::{services, ErrorKind, Operator};
use serde::{Deserialize, Serialize};
use vornel_vault::{unwrap_key, Hlc, KeyStore, Vault, VaultError, WrappedKey};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("{}", describe_storage(.0))]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error("数据格式错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("需要主密码")]
    NeedPassword {
        /// 云端还没有同步库：输入的密码会成为新的主密码
        first_time: bool,
    },
    #[error("配置错误：{0}")]
    Config(String),
    #[error("授权失败：{0}")]
    OAuth(String),
    /// 连接测试某一步失败
    #[error("{step}失败：{source}")]
    Probe {
        step: &'static str,
        source: Box<SyncError>,
    },
}

/// 把存储后端的错误翻译成用户能看懂的提示，并附上原始信息方便排查。
pub fn describe_storage(e: &opendal::Error) -> String {
    let raw = e.to_string();
    let hint = if raw.contains("SignatureDoesNotMatch") {
        "签名校验失败：Secret Access Key 不正确，或 Region 与存储桶不匹配。"
    } else if raw.contains("InvalidAccessKeyId") {
        "Access Key ID 不存在或已被删除。"
    } else if raw.contains("NoSuchBucket") {
        "存储桶不存在：请检查 Bucket 名称（区分大小写）和 Endpoint。"
    } else {
        match e.kind() {
            ErrorKind::PermissionDenied => {
                "云存储拒绝访问（403）：访问密钥没有这个存储桶的读写权限。\
                 Cloudflare R2 的 API 令牌需要选择“对象读和写”，并且“应用到”里包含这个存储桶；\
                 AWS S3 需要 s3:GetObject / PutObject / DeleteObject / ListBucket 权限。"
            }
            ErrorKind::NotFound => "找不到存储位置：请检查 Bucket、地址或目录是否存在。",
            ErrorKind::ConfigInvalid => "同步配置有误，请检查各项填写。",
            ErrorKind::RateLimited => "请求太频繁，被云存储限流，请稍后再试。",
            _ if raw.contains("dns") || raw.contains("connect") || raw.contains("timed out") => {
                "无法连接到云存储：请检查网络和 Endpoint 地址。"
            }
            _ => "访问云存储失败。",
        }
    };
    format!("{hint}\n\n详细信息：{raw}")
}

/// 连接测试：确认能读、写、删除同步目录里的文件。保存同步设置时调用，问题能在第一步就暴露出来。
pub async fn probe(op: &Operator) -> Result<()> {
    let step = |step: &'static str| {
        move |e: opendal::Error| SyncError::Probe {
            step,
            source: Box::new(SyncError::Storage(e)),
        }
    };
    const PROBE: &str = ".vornel-probe";
    read_optional(op, VAULT_FILE)
        .await
        .map_err(|e| SyncError::Probe {
            step: "读取",
            source: Box::new(e),
        })?;
    op.write(PROBE, b"vornel".to_vec())
        .await
        .map_err(step("写入"))?;
    let back = op.read(PROBE).await.map_err(step("读回"))?.to_vec();
    if back != b"vornel" {
        return Err(SyncError::Config("写入后读回的内容不一致".into()));
    }
    op.delete(PROBE).await.map_err(step("删除"))?;
    Ok(())
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// 进程启动时调用：把 ring 安装为 rustls 的默认加密后端，并为 OpenDAL 安装 HTTP 传输。
///
/// OpenDAL 关闭了 `auto-register-services`（它会连带启用 aws-lc），所以 HTTP 传输必须手动安装，
/// 否则 S3、WebDAV、云盘等所有基于 HTTP 的后端都会报“default HTTP transport is not installed”。
/// 可以重复调用。
pub fn init_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    opendal_http_transport_reqwest::install_default();
}

// ------------------------------------------------------------------ 后端配置

/// 同步后端。保存在 Vault 的加密本机设置里，不参与同步。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BackendConfig {
    /// 本地文件夹（可指向 iCloud Drive、Syncthing、网盘同步目录）
    Folder { path: String },
    /// S3 / R2 / MinIO 等兼容服务
    S3 {
        endpoint: String,
        bucket: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
        #[serde(default)]
        root: String,
    },
    /// WebDAV（Nextcloud、坚果云等）
    Webdav {
        endpoint: String,
        username: String,
        password: String,
        #[serde(default)]
        root: String,
    },
    /// Google Drive（drive.file 窄权限）
    Gdrive {
        client_id: String,
        #[serde(default)]
        client_secret: String,
        refresh_token: String,
    },
    OneDrive {
        client_id: String,
        #[serde(default)]
        client_secret: String,
        refresh_token: String,
    },
    Dropbox {
        client_id: String,
        #[serde(default)]
        client_secret: String,
        refresh_token: String,
    },
}

fn join_root(root: &str) -> String {
    let r = root.trim().trim_matches('/');
    if r.is_empty() {
        "/vornel/".into()
    } else {
        format!("/{r}/vornel/")
    }
}

impl BackendConfig {
    pub fn label(&self) -> &'static str {
        match self {
            BackendConfig::Folder { .. } => "本地文件夹",
            BackendConfig::S3 { .. } => "S3",
            BackendConfig::Webdav { .. } => "WebDAV",
            BackendConfig::Gdrive { .. } => "Google Drive",
            BackendConfig::OneDrive { .. } => "OneDrive",
            BackendConfig::Dropbox { .. } => "Dropbox",
        }
    }

    pub fn operator(&self) -> Result<Operator> {
        let op = match self {
            BackendConfig::Folder { path } => {
                if path.trim().is_empty() {
                    return Err(SyncError::Config("请选择文件夹".into()));
                }
                let root = std::path::Path::new(path.trim()).join("vornel");
                Operator::new(services::Fs::default().root(&root.to_string_lossy()))?
            }
            BackendConfig::S3 {
                endpoint,
                bucket,
                region,
                access_key_id,
                secret_access_key,
                root,
            } => {
                let mut b = services::S3::default()
                    .bucket(bucket.trim())
                    .region(if region.trim().is_empty() {
                        "auto"
                    } else {
                        region.trim()
                    })
                    .access_key_id(access_key_id.trim())
                    .secret_access_key(secret_access_key.trim())
                    .root(&join_root(root));
                if !endpoint.trim().is_empty() {
                    b = b.endpoint(endpoint.trim());
                }
                Operator::new(b)?
            }
            BackendConfig::Webdav {
                endpoint,
                username,
                password,
                root,
            } => Operator::new(
                services::Webdav::default()
                    .endpoint(endpoint.trim())
                    .username(username)
                    .password(password)
                    .root(&join_root(root)),
            )?,
            BackendConfig::Gdrive {
                client_id,
                client_secret,
                refresh_token,
            } => Operator::new(
                services::Gdrive::default()
                    .client_id(client_id)
                    .client_secret(client_secret)
                    .refresh_token(refresh_token)
                    .root("/vornel/"),
            )?,
            BackendConfig::OneDrive {
                client_id,
                client_secret,
                refresh_token,
            } => Operator::new(
                services::Onedrive::default()
                    .client_id(client_id)
                    .client_secret(client_secret)
                    .refresh_token(refresh_token)
                    .root("/Apps/Vornel/"),
            )?,
            BackendConfig::Dropbox {
                client_id,
                client_secret,
                refresh_token,
            } => Operator::new(
                services::Dropbox::default()
                    .client_id(client_id)
                    .client_secret(client_secret)
                    .refresh_token(refresh_token)
                    .root("/vornel/"),
            )?,
        };
        Ok(op
            .layer(TimeoutLayer::new())
            .layer(RetryLayer::new().with_max_times(3)))
    }
}

// ------------------------------------------------------------------ 同步

const VAULT_FILE: &str = "vault.json";
const MANIFEST_FILE: &str = "manifest.bin";
/// 本机记录的“已加入的同步库”的数据密钥 id
const META_KEY_ID: &str = "sync.keyId";

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub pulled: usize,
    pub pushed: usize,
    pub purged: usize,
    /// 本次是否新建了云端同步库
    pub created: bool,
    /// 本次是否加入了已有的同步库（本机数据已用云端密钥重新加密）
    pub joined: bool,
}

fn record_path(key: &str) -> String {
    format!("records/{}.bin", key.replace(':', "-"))
}

async fn read_optional(op: &Operator, path: &str) -> Result<Option<Vec<u8>>> {
    match op.read(path).await {
        Ok(buf) => Ok(Some(buf.to_vec())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn read_manifest(op: &Operator, vault: &mut Vault) -> Result<BTreeMap<String, Hlc>> {
    match read_optional(op, MANIFEST_FILE).await? {
        Some(bytes) => Ok(serde_json::from_slice(&vault.open_sealed(&bytes)?)?),
        None => Ok(BTreeMap::new()),
    }
}

/// 检查云端状态，返回是否需要主密码（用于 UI 提前弹出输入框）。
pub async fn needs_password(op: &Operator, vault: &mut Vault) -> Result<Option<bool>> {
    let local_id: Option<String> = vault.meta_get(META_KEY_ID)?;
    match read_optional(op, VAULT_FILE).await? {
        None => Ok(Some(true)),
        Some(bytes) => {
            let remote: WrappedKey = serde_json::from_slice(&bytes)?;
            if local_id.as_deref() == Some(remote.key_id.as_str()) {
                Ok(None)
            } else {
                Ok(Some(false))
            }
        }
    }
}

/// 执行一次完整同步。
///
/// - 云端还没有同步库：需要 `password`，用它包装本机数据密钥并创建同步库。
/// - 云端已有同步库但本机还没加入：需要 `password` 解开云端数据密钥，本机数据改用它重新加密。
/// - 之后的同步不再需要密码。
pub async fn sync(
    op: &Operator,
    vault: &mut Vault,
    keys: &dyn KeyStore,
    password: Option<&str>,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    // 1. 数据密钥
    let local_id: Option<String> = vault.meta_get(META_KEY_ID)?;
    match read_optional(op, VAULT_FILE).await? {
        None => {
            let password = password.ok_or(SyncError::NeedPassword { first_time: true })?;
            let key_id = uuid::Uuid::new_v4().to_string();
            let wrapped = vault.wrap_key(&key_id, password)?;
            op.write(VAULT_FILE, serde_json::to_vec_pretty(&wrapped)?)
                .await?;
            vault.meta_set(META_KEY_ID, &key_id)?;
            report.created = true;
        }
        Some(bytes) => {
            let remote: WrappedKey = serde_json::from_slice(&bytes)?;
            if local_id.as_deref() != Some(remote.key_id.as_str()) {
                let password = password.ok_or(SyncError::NeedPassword { first_time: false })?;
                let dek = unwrap_key(&remote, password)?;
                vault.rekey(dek, keys)?;
                vault.meta_set(META_KEY_ID, &remote.key_id)?;
                report.joined = true;
            }
        }
    }

    // 2. 拉取：云端比本地新的记录
    let remote = read_manifest(op, vault).await?;
    let local = vault.manifest()?;
    for (key, rv) in &remote {
        if local.get(key).is_none_or(|lv| rv > lv) {
            if let Some(bytes) = read_optional(op, &record_path(key)).await? {
                if vault.import(&bytes)? {
                    report.pulled += 1;
                }
            }
        }
    }

    // 3. 推送：合并后本地比云端新的记录
    let local = vault.manifest()?;
    for (key, lv) in &local {
        if remote.get(key).is_none_or(|rv| lv > rv) {
            if let Some(bytes) = vault.export(key)? {
                op.write(&record_path(key), bytes).await?;
                report.pushed += 1;
            }
        }
    }

    // 4. 清理 30 天前的墓碑（本地与云端）
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        .saturating_sub(30 * 24 * 3600 * 1000);
    let purged = vault.purge_tombstones(cutoff)?;
    for key in &purged {
        let _ = op.delete(&record_path(key)).await;
    }
    report.purged = purged.len();

    // 5. 写回清单：重新读取云端清单并取并集，降低多台设备同时同步时互相覆盖的影响
    let mut merged = read_manifest(op, vault).await?;
    for (k, v) in vault.manifest()? {
        let newer = merged.get(&k).is_none_or(|m| &v > m);
        if newer {
            merged.insert(k, v);
        }
    }
    for key in &purged {
        merged.remove(key);
    }
    op.write(MANIFEST_FILE, vault.seal(&serde_json::to_vec(&merged)?)?)
        .await?;

    Ok(report)
}

/// 修改主密码：用新密码重新包装同一把数据密钥并覆盖云端 vault.json。
///
/// 数据本身不需要重新加密；已加入的其他设备继续使用同一把数据密钥，不受影响，
/// 只有之后新加入的设备需要输入新密码。
pub async fn change_password(op: &Operator, vault: &mut Vault, new_password: &str) -> Result<()> {
    if new_password.chars().count() < 8 {
        return Err(SyncError::Config("主密码至少 8 个字符".into()));
    }
    let local_id: Option<String> = vault.meta_get(META_KEY_ID)?;
    let bytes = read_optional(op, VAULT_FILE)
        .await?
        .ok_or_else(|| SyncError::Config("云端还没有同步库，请先同步一次".into()))?;
    let remote: WrappedKey = serde_json::from_slice(&bytes)?;
    if local_id.as_deref() != Some(remote.key_id.as_str()) {
        return Err(SyncError::NeedPassword { first_time: false });
    }
    let wrapped = vault.wrap_key(&remote.key_id, new_password)?;
    op.write(VAULT_FILE, serde_json::to_vec_pretty(&wrapped)?)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vornel_vault::{MemoryKeyStore, Vault};

    fn temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("vornel-sync-{}", uuid::Uuid::new_v4()))
    }

    fn snippet(text: &str) -> vornel_core::Snippet {
        vornel_core::Snippet {
            id: uuid::Uuid::new_v4(),
            name: text.into(),
            command: text.into(),
            run: false,
        }
    }

    #[tokio::test]
    async fn two_devices_sync_through_folder() {
        init_tls();
        let cloud = temp_dir();
        let cfg = BackendConfig::Folder {
            path: cloud.to_string_lossy().into_owned(),
        };
        let op = cfg.operator().unwrap();

        let keys_a = MemoryKeyStore::default();
        let keys_b = MemoryKeyStore::default();
        let mut a = Vault::open(&temp_dir().join("a.db"), &keys_a).unwrap();
        let mut b = Vault::open(&temp_dir().join("b.db"), &keys_b).unwrap();

        let s1 = snippet("from-a");
        a.put(&s1).unwrap();
        let s2 = snippet("from-b");
        b.put(&s2).unwrap();

        // A 创建同步库
        assert!(matches!(
            sync(&op, &mut a, &keys_a, None).await,
            Err(SyncError::NeedPassword { first_time: true })
        ));
        let r = sync(&op, &mut a, &keys_a, Some("pw")).await.unwrap();
        assert!(r.created);
        assert_eq!(r.pushed, 1);

        // B 用错误密码加入失败，用正确密码加入成功，且两边数据合并
        assert!(sync(&op, &mut b, &keys_b, Some("bad")).await.is_err());
        let r = sync(&op, &mut b, &keys_b, Some("pw")).await.unwrap();
        assert!(r.joined);
        assert_eq!(r.pulled, 1);
        assert_eq!(b.list::<vornel_core::Snippet>().unwrap().len(), 2);

        // A 再同步一次即可拿到 B 的数据，且不再需要密码
        let r = sync(&op, &mut a, &keys_a, None).await.unwrap();
        assert_eq!(r.pulled, 1);
        assert_eq!(a.list::<vornel_core::Snippet>().unwrap().len(), 2);

        // 删除同步到另一台设备
        a.delete::<vornel_core::Snippet>(s2.id).unwrap();
        sync(&op, &mut a, &keys_a, None).await.unwrap();
        sync(&op, &mut b, &keys_b, None).await.unwrap();
        assert_eq!(b.list::<vornel_core::Snippet>().unwrap().len(), 1);

        // 修改主密码：已加入的设备不受影响；新设备必须用新密码
        change_password(&op, &mut a, "new-password").await.unwrap();
        sync(&op, &mut b, &keys_b, None).await.unwrap();
        let keys_c = MemoryKeyStore::default();
        let mut c = Vault::open(&temp_dir().join("c.db"), &keys_c).unwrap();
        assert!(sync(&op, &mut c, &keys_c, Some("pw")).await.is_err());
        let r = sync(&op, &mut c, &keys_c, Some("new-password"))
            .await
            .unwrap();
        assert!(r.joined);
        assert_eq!(c.list::<vornel_core::Snippet>().unwrap().len(), 1);

        // 云端文件中不含明文
        let raw = std::fs::read(
            cloud
                .join("vornel")
                .join(record_path(&vornel_vault::record_key("snippet", s1.id))),
        )
        .unwrap();
        assert!(!raw.windows(6).any(|w| w == b"from-a"));
    }
}
