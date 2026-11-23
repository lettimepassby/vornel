use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use uuid::Uuid;
use vornel_ssh::{Connection, ForwardHandle, Sftp, Shell};
use vornel_vault::{SystemKeyStore, Vault};

/// 一个打开的终端会话。
pub enum Session {
    Ssh {
        conn: Arc<Connection>,
        shell: Shell,
        sftp: Mutex<Option<Arc<Sftp>>>,
    },
}

pub struct RunningForward {
    pub conn: Arc<Connection>,
    pub handle: ForwardHandle,
}

pub struct AppState {
    pub vault: Mutex<Option<Vault>>,
    /// Vault 打不开时的原因（例如本机数据密钥丢失）
    pub vault_error: Mutex<Option<String>>,
    pub vault_path: PathBuf,
    pub keys: SystemKeyStore,
    pub sessions: Mutex<HashMap<String, Arc<Session>>>,
    pub forwards: Mutex<HashMap<Uuid, RunningForward>>,
}

impl AppState {
    pub fn session_err() -> String {
        "会话已关闭".to_string()
    }

    pub async fn session(&self, id: &str) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(Self::session_err)
    }
}

/// 在持有 Vault 锁的情况下执行一段同步代码。
#[macro_export]
macro_rules! with_vault {
    ($state:expr, |$v:ident| $body:expr) => {{
        let mut guard = $state.vault.lock().await;
        let $v = guard.as_mut().ok_or_else(|| "Vault 未打开".to_string())?;
        $body
    }};
}
