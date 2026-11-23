//! 加密原语：XChaCha20-Poly1305 加密记录，Argon2id 从主密码派生密钥包装数据密钥（DEK）。

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::VaultError;

const NONCE_LEN: usize = 24;

/// 32 字节的数据密钥，离开作用域时清零。
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct Dek([u8; 32]);

impl Dek {
    pub fn generate() -> Result<Self, VaultError> {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).map_err(|e| VaultError::Crypto(e.to_string()))?;
        Ok(Self(k))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VaultError> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| VaultError::Crypto("数据密钥长度错误".into()))?;
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// 加密：输出 = nonce(24) ‖ 密文+标签
    pub fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
        seal_with(&self.0, plaintext)
    }

    pub fn open(&self, data: &[u8]) -> Result<Vec<u8>, VaultError> {
        open_with(&self.0, data)
    }
}

impl std::fmt::Debug for Dek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dek(***)")
    }
}

fn seal_with(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|e| VaultError::Crypto(e.to_string()))?;
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|e| VaultError::Crypto(e.to_string()))?;
    let ct = cipher
        .encrypt(&XNonce::from(nonce), plaintext)
        .map_err(|_| VaultError::Crypto("加密失败".into()))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn open_with(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, VaultError> {
    if data.len() < NONCE_LEN + 16 {
        return Err(VaultError::Crypto("密文过短".into()));
    }
    let (nonce, ct) = data.split_at(NONCE_LEN);
    let nonce: [u8; NONCE_LEN] = nonce.try_into().expect("长度已检查");
    let cipher =
        XChaCha20Poly1305::new_from_slice(key).map_err(|e| VaultError::Crypto(e.to_string()))?;
    cipher
        .decrypt(&XNonce::from(nonce), ct)
        .map_err(|_| VaultError::WrongKey)
}

/// 用主密码包装后的数据密钥，可以安全地放在云端（`vault.json`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WrappedKey {
    pub version: u32,
    /// 这把数据密钥的标识，用来判断本机是否已经在用同一把钥匙
    pub key_id: String,
    pub kdf: KdfParams,
    /// base64(nonce ‖ 密文)
    pub wrapped: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    pub algorithm: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    /// base64
    pub salt: String,
}

fn derive(password: &str, kdf: &KdfParams) -> Result<[u8; 32], VaultError> {
    if kdf.algorithm != "argon2id" {
        return Err(VaultError::Crypto(format!(
            "不支持的密钥派生算法：{}",
            kdf.algorithm
        )));
    }
    let salt = B64
        .decode(&kdf.salt)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let params = Params::new(kdf.memory_kib, kdf.iterations, kdf.parallelism, Some(32))
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), &salt, &mut out)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    Ok(out)
}

/// 用主密码包装数据密钥。
pub fn wrap_key(dek: &Dek, key_id: &str, password: &str) -> Result<WrappedKey, VaultError> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| VaultError::Crypto(e.to_string()))?;
    let kdf = KdfParams {
        algorithm: "argon2id".into(),
        // 64 MiB、3 轮：在手机上约 0.5 秒，足以抵抗离线暴力破解
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
        salt: B64.encode(salt),
    };
    let mut kek = derive(password, &kdf)?;
    let wrapped = seal_with(&kek, dek.as_bytes());
    kek.zeroize();
    Ok(WrappedKey {
        version: 1,
        key_id: key_id.to_string(),
        kdf,
        wrapped: B64.encode(wrapped?),
    })
}

/// 用主密码解开数据密钥；密码错误时返回 [`VaultError::WrongPassword`]。
pub fn unwrap_key(w: &WrappedKey, password: &str) -> Result<Dek, VaultError> {
    let mut kek = derive(password, &w.kdf)?;
    let data = B64
        .decode(&w.wrapped)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let res = open_with(&kek, &data);
    kek.zeroize();
    match res {
        Ok(bytes) => Dek::from_bytes(&bytes),
        Err(VaultError::WrongKey) => Err(VaultError::WrongPassword),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let k = Dek::generate().unwrap();
        let ct = k.seal(b"hello").unwrap();
        assert_ne!(&ct[NONCE_LEN..], b"hello");
        assert_eq!(k.open(&ct).unwrap(), b"hello");
    }

    #[test]
    fn wrong_key_fails() {
        let a = Dek::generate().unwrap();
        let b = Dek::generate().unwrap();
        let ct = a.seal(b"x").unwrap();
        assert!(matches!(b.open(&ct), Err(VaultError::WrongKey)));
    }

    #[test]
    fn wrap_unwrap() {
        let dek = Dek::generate().unwrap();
        let w = wrap_key(&dek, "id1", "correct horse").unwrap();
        assert_eq!(unwrap_key(&w, "correct horse").unwrap(), dek);
        assert!(matches!(
            unwrap_key(&w, "wrong"),
            Err(VaultError::WrongPassword)
        ));
    }
}
