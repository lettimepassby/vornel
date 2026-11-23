use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::Host;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("读写主机列表失败：{0}")]
    Io(#[from] io::Error),
    #[error("主机列表文件格式错误：{0}")]
    Json(#[from] serde_json::Error),
}

/// 以 JSON 文件保存的主机列表。写入时先写临时文件再改名，避免写一半崩溃导致数据损坏。
#[derive(Debug)]
pub struct HostStore {
    path: PathBuf,
    hosts: Vec<Host>,
}

impl HostStore {
    /// 读取主机列表；文件不存在时返回空列表。
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        let hosts = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { path, hosts })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn list(&self) -> &[Host] {
        &self.hosts
    }

    pub fn get(&self, id: Uuid) -> Option<&Host> {
        self.hosts.iter().find(|h| h.id == id)
    }

    /// 新增或按 id 覆盖一台主机，并立即保存。
    pub fn upsert(&mut self, host: Host) -> Result<(), StoreError> {
        match self.hosts.iter_mut().find(|h| h.id == host.id) {
            Some(existing) => *existing = host,
            None => self.hosts.push(host),
        }
        self.save()
    }

    /// 删除一台主机并保存；返回是否确实删除了。
    pub fn remove(&mut self, id: Uuid) -> Result<bool, StoreError> {
        let before = self.hosts.len();
        self.hosts.retain(|h| h.id != id);
        let removed = self.hosts.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    fn save(&self) -> Result<(), StoreError> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.hosts)?)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthKind, Protocol};

    fn sample(name: &str) -> Host {
        Host {
            id: Uuid::new_v4(),
            name: name.into(),
            address: "127.0.0.1".into(),
            port: 22,
            username: "root".into(),
            group: Some("测试".into()),
            protocol: Protocol::Ssh,
            auth: AuthKind::Password,
        }
    }

    fn temp_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("vornel-test-{}", Uuid::new_v4()))
            .join("hosts.json")
    }

    #[test]
    fn missing_file_is_empty() {
        let store = HostStore::load(temp_path()).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn upsert_remove_roundtrip() {
        let path = temp_path();
        let mut store = HostStore::load(&path).unwrap();
        let mut a = sample("a");
        store.upsert(a.clone()).unwrap();
        store.upsert(sample("b")).unwrap();

        a.name = "a2".into();
        store.upsert(a.clone()).unwrap();

        let reloaded = HostStore::load(&path).unwrap();
        assert_eq!(reloaded.list().len(), 2);
        assert_eq!(reloaded.get(a.id).unwrap().name, "a2");

        let mut store = reloaded;
        assert!(store.remove(a.id).unwrap());
        assert!(!store.remove(a.id).unwrap());
        assert_eq!(HostStore::load(&path).unwrap().list().len(), 1);

        fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
