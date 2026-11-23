use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use vornel_core::HostStore;
use vornel_ssh::Shell;

pub struct AppState {
    pub hosts: Mutex<HostStore>,
    pub sessions: Mutex<HashMap<String, Arc<Shell>>>,
    pub known_hosts: PathBuf,
}

impl AppState {
    pub fn new(hosts: HostStore, known_hosts: PathBuf) -> Self {
        Self {
            hosts: Mutex::new(hosts),
            sessions: Mutex::new(HashMap::new()),
            known_hosts,
        }
    }

    /// 取出会话的共享引用；立刻释放锁，避免在网络 IO 期间阻塞其他会话。
    pub async fn session(&self, id: &str) -> Result<Arc<Shell>, String> {
        self.sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| "会话已关闭".to_string())
    }
}
