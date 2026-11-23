//! Vornel 的加密本地存储。
//!
//! - 每条记录序列化为 JSON 后用数据密钥（DEK）XChaCha20-Poly1305 加密，存进 SQLite。
//! - DEK 保存在系统钥匙串；开启云同步时用主密码（Argon2id）包装后上传。
//! - 记录按字段带 HLC 时间戳，合并时逐字段“最后写入者胜”，删除留墓碑。

mod crypto;
mod hlc;
mod keystore;
mod record;

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;
use vornel_core::Record;

pub use crypto::{unwrap_key, wrap_key, Dek, KdfParams, WrappedKey};
pub use hlc::{Clock, Hlc};
pub use keystore::{KeyStore, MemoryKeyStore, SystemKeyStore};
pub use record::{record_key, StoredRecord};

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("数据库错误：{0}")]
    Db(#[from] rusqlite::Error),
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("数据格式错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("加密错误：{0}")]
    Crypto(String),
    #[error("数据密钥不匹配，无法解密")]
    WrongKey,
    #[error("主密码不正确")]
    WrongPassword,
    #[error("本机的数据密钥丢失，无法打开已有的 Vault")]
    KeyMissing,
}

pub type Result<T> = std::result::Result<T, VaultError>;

const CHECK_PLAINTEXT: &[u8] = b"vornel-vault-v1";

pub struct Vault {
    conn: Connection,
    dek: Dek,
    clock: Clock,
}

impl Vault {
    /// 打开（或创建）Vault。首次创建时生成新的数据密钥并存进 `keys`。
    pub fn open(path: &Path, keys: &dyn KeyStore) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS records (
                 key        TEXT PRIMARY KEY,
                 kind       TEXT NOT NULL,
                 version    TEXT NOT NULL,
                 deleted    INTEGER NOT NULL,
                 local_only INTEGER NOT NULL,
                 body       BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS records_kind ON records(kind);
             CREATE TABLE IF NOT EXISTS meta (
                 key   TEXT PRIMARY KEY,
                 value BLOB NOT NULL
             );",
        )?;

        let node: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'node'", [], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .optional()?
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        let node = match node {
            Some(n) => n,
            None => {
                let n = Uuid::new_v4().simple().to_string()[..12].to_string();
                conn.execute(
                    "INSERT INTO meta (key, value) VALUES ('node', ?1)",
                    params![n.as_bytes()],
                )?;
                n
            }
        };

        let check: Option<Vec<u8>> = conn
            .query_row("SELECT value FROM meta WHERE key = 'check'", [], |r| {
                r.get(0)
            })
            .optional()?;

        let dek = match (keys.load()?, check) {
            (Some(dek), Some(check)) => {
                if dek.open(&check)? != CHECK_PLAINTEXT {
                    return Err(VaultError::WrongKey);
                }
                dek
            }
            (Some(dek), None) => {
                conn.execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES ('check', ?1)",
                    params![dek.seal(CHECK_PLAINTEXT)?],
                )?;
                dek
            }
            (None, Some(_)) => return Err(VaultError::KeyMissing),
            (None, None) => {
                let dek = Dek::generate()?;
                keys.save(&dek)?;
                conn.execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES ('check', ?1)",
                    params![dek.seal(CHECK_PLAINTEXT)?],
                )?;
                dek
            }
        };

        let mut vault = Self {
            conn,
            dek,
            clock: Clock::new(node),
        };
        // 让本地时钟不落后于已存储的任何时间戳
        let max: Option<String> = vault
            .conn
            .query_row("SELECT MAX(version) FROM records", [], |r| r.get(0))
            .optional()?
            .flatten();
        if let Some(Ok(h)) = max.map(|s| s.parse::<Hlc>()) {
            vault.clock.observe(&h);
        }
        Ok(vault)
    }

    /// 本机数据丢失密钥时的最后手段：删除数据库重新开始。
    pub fn reset(path: &Path, keys: &dyn KeyStore) -> Result<()> {
        for suffix in ["", "-wal", "-shm"] {
            let p = path.with_file_name(format!(
                "{}{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                suffix
            ));
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        keys.clear()
    }

    pub fn node(&self) -> &str {
        self.clock.node()
    }

    // ------------------------------------------------------------ 类型化读写

    pub fn list<T: Record>(&self) -> Result<Vec<T>> {
        let mut stmt = self
            .conn
            .prepare("SELECT body FROM records WHERE kind = ?1 AND deleted = 0")?;
        let rows = stmt.query_map(params![T::KIND], |r| r.get::<_, Vec<u8>>(0))?;
        let mut out = Vec::new();
        for body in rows {
            let rec = self.open_record(&body?)?;
            out.push(from_record(&rec)?);
        }
        Ok(out)
    }

    pub fn get<T: Record>(&self, id: Uuid) -> Result<Option<T>> {
        match self.load_raw(&record_key(T::KIND, id))? {
            Some(rec) if !rec.is_deleted() => Ok(Some(from_record(&rec)?)),
            _ => Ok(None),
        }
    }

    /// 新增或修改一条记录。只有变化的字段会更新时间戳。
    pub fn put<T: Record>(&mut self, value: &T) -> Result<()> {
        let key = record_key(T::KIND, value.id());
        let mut rec = self
            .load_raw(&key)?
            .unwrap_or_else(|| StoredRecord::new(T::KIND, value.id()));
        let local_only = value.local_only();
        let changed = rec.update(serde_json::to_value(value)?, &mut self.clock);
        if changed || rec.local_only != local_only {
            rec.local_only = local_only;
            self.store_raw(&rec)?;
        }
        Ok(())
    }

    /// 删除（留墓碑以便同步到其他设备）。
    pub fn delete<T: Record>(&mut self, id: Uuid) -> Result<bool> {
        let key = record_key(T::KIND, id);
        match self.load_raw(&key)? {
            Some(mut rec) if !rec.is_deleted() => {
                rec.mark_deleted(&mut self.clock);
                self.store_raw(&rec)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    // ------------------------------------------------------------ 同步用的原始接口

    /// 参与同步的记录：键 → 版本。
    pub fn manifest(&self) -> Result<BTreeMap<String, Hlc>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, version FROM records WHERE local_only = 0")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            if let Ok(h) = v.parse() {
                out.insert(k, h);
            }
        }
        Ok(out)
    }

    /// 导出一条记录的密文（与云端格式相同）。
    pub fn export(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT body FROM records WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// 导入一条来自其他设备的记录并合并。返回本地是否发生变化。
    pub fn import(&mut self, sealed: &[u8]) -> Result<bool> {
        let incoming = self.open_record(sealed)?;
        self.clock.observe(&incoming.version());
        let key = incoming.key();
        let merged = match self.load_raw(&key)? {
            Some(mut local) => {
                if !local.merge(&incoming) {
                    return Ok(false);
                }
                local
            }
            None => incoming,
        };
        self.store_raw(&merged)?;
        Ok(true)
    }

    /// 彻底清除早于 `before` 的墓碑，返回被清除的键。
    pub fn purge_tombstones(&mut self, before_wall_ms: u64) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, version FROM records WHERE deleted = 1")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        drop(stmt);
        let mut purged = Vec::new();
        for (key, version) in rows {
            if version
                .parse::<Hlc>()
                .is_ok_and(|h| h.wall < before_wall_ms)
            {
                self.conn
                    .execute("DELETE FROM records WHERE key = ?1", params![key])?;
                purged.push(key);
            }
        }
        Ok(purged)
    }

    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        self.dek.seal(plaintext)
    }

    pub fn open_sealed(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.dek.open(data)
    }

    // ------------------------------------------------------------ 密钥管理

    /// 用主密码包装当前数据密钥（上传到云端的 vault.json）。
    pub fn wrap_key(&self, key_id: &str, password: &str) -> Result<WrappedKey> {
        wrap_key(&self.dek, key_id, password)
    }

    /// 换用另一把数据密钥（加入已有的同步库时），本地所有数据用新密钥重新加密。
    pub fn rekey(&mut self, new_dek: Dek, keys: &dyn KeyStore) -> Result<()> {
        if new_dek == self.dek {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("SELECT key, body FROM records")?;
            let rows: Vec<(String, Vec<u8>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?;
            for (key, body) in rows {
                let plain = self.dek.open(&body)?;
                tx.execute(
                    "UPDATE records SET body = ?1 WHERE key = ?2",
                    params![new_dek.seal(&plain)?, key],
                )?;
            }
            let mut stmt = tx.prepare("SELECT key, value FROM meta WHERE key LIKE 's:%'")?;
            let rows: Vec<(String, Vec<u8>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?;
            for (key, value) in rows {
                let plain = self.dek.open(&value)?;
                tx.execute(
                    "UPDATE meta SET value = ?1 WHERE key = ?2",
                    params![new_dek.seal(&plain)?, key],
                )?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('check', ?1)",
                params![new_dek.seal(CHECK_PLAINTEXT)?],
            )?;
        }
        tx.commit()?;
        keys.save(&new_dek)?;
        self.dek = new_dek;
        Ok(())
    }

    // ------------------------------------------------------------ 加密的本机设置

    /// 读取一项只存在本机、加密保存的设置（例如同步后端的凭据）。
    pub fn meta_get<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let value: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![format!("s:{name}")],
                |r| r.get(0),
            )
            .optional()?;
        match value {
            Some(v) => Ok(Some(serde_json::from_slice(&self.dek.open(&v)?)?)),
            None => Ok(None),
        }
    }

    pub fn meta_set<T: Serialize>(&mut self, name: &str, value: &T) -> Result<()> {
        let sealed = self.dek.seal(&serde_json::to_vec(value)?)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![format!("s:{name}"), sealed],
        )?;
        Ok(())
    }

    pub fn meta_remove(&mut self, name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![format!("s:{name}")],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------ 内部

    fn open_record(&self, sealed: &[u8]) -> Result<StoredRecord> {
        Ok(serde_json::from_slice(&self.dek.open(sealed)?)?)
    }

    fn load_raw(&self, key: &str) -> Result<Option<StoredRecord>> {
        let body: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT body FROM records WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        body.map(|b| self.open_record(&b)).transpose()
    }

    fn store_raw(&self, rec: &StoredRecord) -> Result<()> {
        let body = self.dek.seal(&serde_json::to_vec(rec)?)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO records (key, kind, version, deleted, local_only, body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                rec.key(),
                rec.kind,
                rec.version().to_string(),
                rec.is_deleted() as i64,
                rec.local_only as i64,
                body
            ],
        )?;
        Ok(())
    }
}

fn from_record<T: Record>(rec: &StoredRecord) -> Result<T> {
    Ok(serde_json::from_value(rec.to_value())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vornel_core::{Host, Key, Snippet};

    fn temp_db() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("vornel-vault-{}", Uuid::new_v4()))
            .join("vault.db")
    }

    fn snippet(name: &str) -> Snippet {
        Snippet {
            id: Uuid::new_v4(),
            name: name.into(),
            command: "uptime".into(),
            run: true,
        }
    }

    #[test]
    fn put_list_delete_reopen() {
        let path = temp_db();
        let keys = MemoryKeyStore::default();
        let s = snippet("a");
        {
            let mut v = Vault::open(&path, &keys).unwrap();
            v.put(&s).unwrap();
            v.put(&snippet("b")).unwrap();
            assert_eq!(v.list::<Snippet>().unwrap().len(), 2);
            assert!(v.delete::<Snippet>(s.id).unwrap());
            assert_eq!(v.list::<Snippet>().unwrap().len(), 1);
            assert!(v.get::<Snippet>(s.id).unwrap().is_none());
        }
        let v = Vault::open(&path, &keys).unwrap();
        assert_eq!(v.list::<Snippet>().unwrap().len(), 1);
        // 墓碑仍在清单里，才能同步到其他设备
        assert!(v
            .manifest()
            .unwrap()
            .contains_key(&record_key("snippet", s.id)));
    }

    #[test]
    fn data_is_encrypted_at_rest() {
        let path = temp_db();
        let keys = MemoryKeyStore::default();
        let mut v = Vault::open(&path, &keys).unwrap();
        v.put(&Snippet {
            command: "super-secret-command".into(),
            ..snippet("x")
        })
        .unwrap();
        drop(v);
        let raw = std::fs::read(&path).unwrap();
        let wal = std::fs::read(path.with_file_name("vault.db-wal")).unwrap_or_default();
        let needle = b"super-secret-command";
        assert!(!raw.windows(needle.len()).any(|w| w == needle));
        assert!(!wal.windows(needle.len()).any(|w| w == needle));
    }

    #[test]
    fn missing_key_is_detected() {
        let path = temp_db();
        let keys = MemoryKeyStore::default();
        Vault::open(&path, &keys).unwrap();
        let other = MemoryKeyStore::default();
        assert!(matches!(
            Vault::open(&path, &other),
            Err(VaultError::KeyMissing)
        ));
    }

    #[test]
    fn local_only_records_are_not_synced() {
        let path = temp_db();
        let keys = MemoryKeyStore::default();
        let mut v = Vault::open(&path, &keys).unwrap();
        let key = Key {
            id: Uuid::new_v4(),
            name: "k".into(),
            algorithm: "ssh-ed25519".into(),
            public_key: "p".into(),
            private_key: "s".into(),
            encrypted: false,
            fingerprint: "f".into(),
            sync: false,
        };
        v.put(&key).unwrap();
        assert!(v.manifest().unwrap().is_empty());
        v.put(&Key { sync: true, ..key }).unwrap();
        assert_eq!(v.manifest().unwrap().len(), 1);
    }

    #[test]
    fn export_import_between_devices() {
        let keys = MemoryKeyStore::default();
        let mut a = Vault::open(&temp_db(), &keys).unwrap();
        let mut b = Vault::open(&temp_db(), &keys).unwrap(); // 同一把数据密钥
        let host: Host = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000009","name":"web","address":"10.0.0.1","username":"root"}"#,
        )
        .unwrap();
        a.put(&host).unwrap();
        let key = record_key("host", host.id);
        assert!(b.import(&a.export(&key).unwrap().unwrap()).unwrap());
        assert_eq!(b.get::<Host>(host.id).unwrap().unwrap().name, "web");
        // 再导入一次不会产生变化
        assert!(!b.import(&a.export(&key).unwrap().unwrap()).unwrap());
    }

    #[test]
    fn rekey_keeps_data() {
        let path = temp_db();
        let keys = MemoryKeyStore::default();
        let mut v = Vault::open(&path, &keys).unwrap();
        let s = snippet("x");
        v.put(&s).unwrap();
        v.meta_set("cfg", &"value").unwrap();
        v.rekey(Dek::generate().unwrap(), &keys).unwrap();
        drop(v);
        let v = Vault::open(&path, &keys).unwrap();
        assert_eq!(v.get::<Snippet>(s.id).unwrap().unwrap().name, "x");
        assert_eq!(v.meta_get::<String>("cfg").unwrap().unwrap(), "value");
    }
}
