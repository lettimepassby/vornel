//! Vornel 的数据模型。
//!
//! 所有需要保存、同步的数据都实现 [`Record`]，由 `vornel-vault` 加密存储、由 `vornel-sync` 同步。

mod model;
mod stats;

pub use model::*;
pub use stats::{parse_stats, ServerStats, STATS_COMMAND};

use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

/// 可以存进 Vault 的一条记录。
pub trait Record: Serialize + DeserializeOwned + Clone + Send + 'static {
    /// 记录类型名，例如 `host`。
    const KIND: &'static str;

    fn id(&self) -> Uuid;

    /// 是否只保存在本机、不参与云同步（例如默认不同步的私钥）。
    fn local_only(&self) -> bool {
        false
    }
}
