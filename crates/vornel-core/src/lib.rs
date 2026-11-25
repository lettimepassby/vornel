//! Vornel 的数据模型。
//!
//! 所有需要保存、同步的数据都实现 [`Record`]，由 `vornel-vault` 加密存储、由 `vornel-sync` 同步。

mod history;
mod model;
mod stats;

pub use history::HistoryEntry;
pub use model::*;
pub use stats::{parse_stats, ServerStats, STATS_COMMAND};

/// 命令历史的整理与记录（命令联想）。
pub mod command_history {
    pub use crate::history::{normalize, record, remove, COMMAND_MAX, HISTORY_LIMIT};
}

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
