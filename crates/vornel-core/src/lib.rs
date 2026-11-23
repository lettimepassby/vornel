//! Vornel 的数据模型与本地存储。
//!
//! 目前主机列表以 JSON 文件保存在应用数据目录；端到端加密的 Vault 与云同步在 M3 实现。

mod host;
mod store;

pub use host::{AuthKind, Host, Protocol};
pub use store::{HostStore, StoreError};
