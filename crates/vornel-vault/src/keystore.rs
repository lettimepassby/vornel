//! 数据密钥（DEK）的本机保存位置。
//!
//! 优先使用系统钥匙串（macOS Keychain / Windows 凭据管理器 / Linux Secret Service）；
//! 钥匙串不可用（iOS、无桌面环境的 Linux）时，退回到应用数据目录里的文件——
//! 在 iOS 上它位于应用沙盒内，其他应用无法读取。

use std::fs;
use std::path::PathBuf;

use crate::crypto::Dek;
use crate::VaultError;

pub trait KeyStore: Send + Sync {
    fn load(&self) -> Result<Option<Dek>, VaultError>;
    fn save(&self, dek: &Dek) -> Result<(), VaultError>;
    fn clear(&self) -> Result<(), VaultError>;
}

/// 只存在内存里（用于测试）。
#[derive(Default)]
pub struct MemoryKeyStore(std::sync::Mutex<Option<Dek>>);

impl KeyStore for MemoryKeyStore {
    fn load(&self) -> Result<Option<Dek>, VaultError> {
        Ok(self.0.lock().map_err(|_| poisoned())?.clone())
    }
    fn save(&self, dek: &Dek) -> Result<(), VaultError> {
        *self.0.lock().map_err(|_| poisoned())? = Some(dek.clone());
        Ok(())
    }
    fn clear(&self) -> Result<(), VaultError> {
        *self.0.lock().map_err(|_| poisoned())? = None;
        Ok(())
    }
}

fn poisoned() -> VaultError {
    VaultError::Crypto("密钥存储锁失效".into())
}

/// 系统钥匙串 + 文件回退。
pub struct SystemKeyStore {
    #[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
    service: String,
    fallback: PathBuf,
}

impl SystemKeyStore {
    pub fn new(service: impl Into<String>, fallback: impl Into<PathBuf>) -> Self {
        Self {
            service: service.into(),
            fallback: fallback.into(),
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn entry(&self) -> Option<keyring::Entry> {
        keyring::Entry::new(&self.service, "vault-dek").ok()
    }

    fn load_file(&self) -> Result<Option<Dek>, VaultError> {
        match fs::read(&self.fallback) {
            Ok(bytes) => Ok(Some(Dek::from_bytes(&bytes)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save_file(&self, dek: &Dek) -> Result<(), VaultError> {
        if let Some(dir) = self.fallback.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.fallback, dek.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.fallback, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

impl KeyStore for SystemKeyStore {
    fn load(&self) -> Result<Option<Dek>, VaultError> {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if let Some(entry) = self.entry() {
            if let Ok(secret) = entry.get_secret() {
                return Ok(Some(Dek::from_bytes(&secret)?));
            }
        }
        self.load_file()
    }

    fn save(&self, dek: &Dek) -> Result<(), VaultError> {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if let Some(entry) = self.entry() {
            if entry.set_secret(dek.as_bytes()).is_ok() && entry.get_secret().is_ok() {
                // 钥匙串可用，删掉可能残留的回退文件
                let _ = fs::remove_file(&self.fallback);
                return Ok(());
            }
        }
        self.save_file(dek)
    }

    fn clear(&self) -> Result<(), VaultError> {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if let Some(entry) = self.entry() {
            let _ = entry.delete_credential();
        }
        match fs::remove_file(&self.fallback) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
