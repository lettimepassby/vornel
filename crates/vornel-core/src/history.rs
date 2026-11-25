//! 命令历史：终端里执行过的命令，用于命令联想。
//!
//! 只保存在本机（加密的 Vault 本机设置里），不参与云同步。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 最多保留多少条（按最近使用淘汰）。
pub const HISTORY_LIMIT: usize = 3000;
/// 单条命令的最大长度（字符数），更长的大多是粘贴进来的脚本，不记录。
pub const COMMAND_MAX: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub command: String,
    /// 在哪台主机上执行的；同一条命令在不同主机上分别计数
    #[serde(default)]
    pub host_id: Option<Uuid>,
    pub count: u32,
    /// 最近一次执行的时间（Unix 毫秒）
    pub last_used: u64,
}

/// 整理一条命令：去掉行尾空白；空的、以空格开头的（与 bash 的 ignorespace 约定一致，
/// 用来执行不想被记住的命令）、含控制字符的、过长的都不记录。
pub fn normalize(command: &str) -> Option<&str> {
    if command.starts_with(' ') || command.starts_with('\t') {
        return None;
    }
    let c = command.trim_end();
    if c.is_empty() || c.chars().count() > COMMAND_MAX || c.chars().any(|ch| ch.is_control()) {
        return None;
    }
    Some(c)
}

/// 记录一次执行；返回是否记下了。
pub fn record(
    list: &mut Vec<HistoryEntry>,
    host_id: Option<Uuid>,
    command: &str,
    now: u64,
) -> bool {
    let Some(c) = normalize(command) else {
        return false;
    };
    if let Some(e) = list
        .iter_mut()
        .find(|e| e.host_id == host_id && e.command == c)
    {
        e.count = e.count.saturating_add(1);
        e.last_used = e.last_used.max(now);
    } else {
        list.push(HistoryEntry {
            command: c.to_string(),
            host_id,
            count: 1,
            last_used: now,
        });
    }
    if list.len() > HISTORY_LIMIT {
        list.sort_by(|a, b| b.last_used.cmp(&a.last_used));
        list.truncate(HISTORY_LIMIT);
    }
    true
}

/// 删除一条命令（所有主机上的记录）；返回删掉了几条。
pub fn remove(list: &mut Vec<HistoryEntry>, command: &str) -> usize {
    let before = list.len();
    list.retain(|e| e.command != command);
    before - list.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_counts() {
        let mut l = Vec::new();
        let h = Some(Uuid::new_v4());
        assert!(record(&mut l, h, "ls -la  ", 10));
        assert!(record(&mut l, h, "ls -la", 20));
        assert!(record(&mut l, None, "ls -la", 30));
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].command, "ls -la");
        assert_eq!(l[0].count, 2);
        assert_eq!(l[0].last_used, 20);
        assert_eq!(l[1].host_id, None);
    }

    #[test]
    fn skips_private_and_garbage() {
        let mut l = Vec::new();
        assert!(!record(&mut l, None, " export TOKEN=secret", 1));
        assert!(!record(&mut l, None, "   ", 1));
        assert!(!record(&mut l, None, "echo a\necho b", 1));
        assert!(!record(&mut l, None, "a\u{1b}[A", 1));
        assert!(!record(&mut l, None, &"x".repeat(COMMAND_MAX + 1), 1));
        assert!(record(&mut l, None, "echo 中文", 1));
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn evicts_least_recent() {
        let mut l = Vec::new();
        for i in 0..HISTORY_LIMIT as u64 + 5 {
            record(&mut l, None, &format!("cmd {i}"), i);
        }
        assert_eq!(l.len(), HISTORY_LIMIT);
        assert!(!l.iter().any(|e| e.command == "cmd 0"));
        assert!(l
            .iter()
            .any(|e| e.command == format!("cmd {}", HISTORY_LIMIT + 4)));
    }

    #[test]
    fn removes_everywhere() {
        let mut l = Vec::new();
        record(&mut l, None, "rm -rf ./build", 1);
        record(&mut l, Some(Uuid::new_v4()), "rm -rf ./build", 2);
        record(&mut l, None, "ls", 3);
        assert_eq!(remove(&mut l, "rm -rf ./build"), 2);
        assert_eq!(l.len(), 1);
    }
}
