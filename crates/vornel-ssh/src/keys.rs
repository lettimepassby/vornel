//! 密钥串：生成 ed25519 密钥、导入 OpenSSH 私钥、计算指纹。

use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{decode_secret_key, HashAlg, PrivateKey};

use crate::SshError;

/// 生成或导入后得到的密钥信息。
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub algorithm: String,
    pub public_key: String,
    pub private_key: String,
    pub fingerprint: String,
    pub encrypted: bool,
}

fn info(key: &PrivateKey, private_pem: String, encrypted: bool) -> Result<KeyInfo, SshError> {
    let public = key.public_key();
    Ok(KeyInfo {
        algorithm: public.algorithm().as_str().to_string(),
        public_key: public
            .to_openssh()
            .map_err(|e| SshError::Other(e.to_string()))?,
        private_key: private_pem,
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        encrypted,
    })
}

/// 生成一把新的 ed25519 密钥。`comment` 会写进公钥末尾。
pub fn generate_ed25519(comment: &str) -> Result<KeyInfo, SshError> {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| SshError::Other(e.to_string()))?;
    let pair = Ed25519Keypair::from_seed(&seed);
    let key = PrivateKey::new(KeypairData::Ed25519(pair), comment)
        .map_err(|e| SshError::Other(e.to_string()))?;
    let pem = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| SshError::Other(e.to_string()))?
        .to_string();
    info(&key, pem, false)
}

/// 导入 OpenSSH / PEM 私钥。带口令的私钥需要提供口令来读取公钥，
/// 但保存的仍是原始（加密的）私钥文本。
pub fn import(pem: &str, passphrase: Option<&str>) -> Result<KeyInfo, SshError> {
    let pem = pem.trim().to_string() + "\n";
    match decode_secret_key(&pem, None) {
        Ok(key) => info(&key, pem, false),
        Err(russh::keys::Error::KeyIsEncrypted) => {
            let pass = passphrase.ok_or(SshError::NeedPassphrase)?;
            let key = decode_secret_key(&pem, Some(pass))?;
            info(&key, pem, true)
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_reimport() {
        let k = generate_ed25519("test@vornel").unwrap();
        assert_eq!(k.algorithm, "ssh-ed25519");
        assert!(k.public_key.starts_with("ssh-ed25519 "));
        assert!(k.public_key.ends_with("test@vornel"));
        assert!(k.fingerprint.starts_with("SHA256:"));
        let again = import(&k.private_key, None).unwrap();
        assert_eq!(again.fingerprint, k.fingerprint);
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(import("not a key", None).is_err());
    }
}
