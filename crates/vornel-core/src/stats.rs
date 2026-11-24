//! 服务器资源（终端底部的资源栏）：通过已有的 SSH 连接执行一条只读命令，解析 /proc 与 df 的输出。
//!
//! 只读取内核已经统计好的数据，开销与执行一次 `cat /proc/loadavg` 相当。

use serde::Serialize;

/// 在远端执行的命令。每段之间用 `@@` 行分隔，任何一段失败都不影响其他段。
pub const STATS_COMMAND: &str =
    "head -1 /proc/stat 2>/dev/null; echo @@; cat /proc/meminfo 2>/dev/null; echo @@; \
cat /proc/loadavg 2>/dev/null; echo @@; cat /proc/net/dev 2>/dev/null; echo @@; \
df -kP / 2>/dev/null | tail -1; echo @@; cat /proc/uptime 2>/dev/null; echo @@; nproc 2>/dev/null";

/// 一次采样。CPU 与网络是累计值，界面用两次采样的差值算出使用率和速率。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStats {
    /// CPU 累计时间片（全部 / 空闲含 iowait）
    pub cpu_total: u64,
    pub cpu_idle: u64,
    pub cores: u32,
    /// 内存（字节）
    pub mem_total: u64,
    pub mem_available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub load: [f32; 3],
    /// 网络累计字节（不含 lo）
    pub net_rx: u64,
    pub net_tx: u64,
    /// 根分区（字节）
    pub disk_total: u64,
    pub disk_used: u64,
    pub uptime_secs: u64,
}

/// 解析 [`STATS_COMMAND`] 的输出；不是 Linux（没有 /proc）时返回 None。
pub fn parse_stats(text: &str) -> Option<ServerStats> {
    let parts: Vec<&str> = text.split("@@").map(str::trim).collect();
    let part = |i: usize| parts.get(i).copied().unwrap_or("");
    let mut s = ServerStats::default();

    // /proc/stat 第一行：cpu user nice system idle iowait irq softirq steal …
    let cpu: Vec<u64> = part(0)
        .strip_prefix("cpu")?
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    if cpu.len() < 4 {
        return None;
    }
    s.cpu_total = cpu.iter().take(8).sum();
    s.cpu_idle = cpu[3] + cpu.get(4).copied().unwrap_or(0);

    for line in part(1).lines() {
        let mut it = line.split_whitespace();
        let (Some(k), Some(v)) = (it.next(), it.next()) else {
            continue;
        };
        let kb: u64 = v.parse().unwrap_or(0) * 1024;
        match k {
            "MemTotal:" => s.mem_total = kb,
            "MemAvailable:" => s.mem_available = kb,
            "SwapTotal:" => s.swap_total = kb,
            "SwapFree:" => s.swap_free = kb,
            _ => {}
        }
    }
    if s.mem_total == 0 {
        return None;
    }

    let load: Vec<f32> = part(2)
        .split_whitespace()
        .take(3)
        .filter_map(|v| v.parse().ok())
        .collect();
    if load.len() == 3 {
        s.load = [load[0], load[1], load[2]];
    }

    // /proc/net/dev：iface: rx_bytes … (第 9 列) tx_bytes
    for line in part(3).lines() {
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        if iface.trim() == "lo" {
            continue;
        }
        let cols: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        if cols.len() >= 9 {
            s.net_rx += cols[0];
            s.net_tx += cols[8];
        }
    }

    // df -kP：文件系统 1024-blocks 已用 可用 容量 挂载点
    let df: Vec<&str> = part(4).split_whitespace().collect();
    if df.len() >= 4 {
        s.disk_total = df[1].parse::<u64>().unwrap_or(0) * 1024;
        s.disk_used = df[2].parse::<u64>().unwrap_or(0) * 1024;
    }

    s.uptime_secs = part(5)
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v as u64)
        .unwrap_or(0);
    s.cores = part(6).trim().parse().unwrap_or(1).max(1);
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_output() {
        let out = "cpu  100 5 50 800 20 0 5 0 0 0\n@@\nMemTotal:  2048000 kB\nMemFree: 100 kB\nMemAvailable: 1024000 kB\nSwapTotal: 1000 kB\nSwapFree: 500 kB\n@@\n0.52 0.40 0.30 1/200 1234\n@@\nInter-|   Receive\n face |bytes packets\n    lo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\n  eth0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0\n@@\n/dev/sda1 10000 4000 6000 40% /\n@@\n3600.5 7000.1\n@@\n4\n";
        let s = parse_stats(out).unwrap();
        assert_eq!(s.cpu_total, 980);
        assert_eq!(s.cpu_idle, 820);
        assert_eq!(s.mem_total, 2048000 * 1024);
        assert_eq!(s.mem_available, 1024000 * 1024);
        assert_eq!(s.swap_free, 500 * 1024);
        assert_eq!(s.load, [0.52, 0.40, 0.30]);
        assert_eq!((s.net_rx, s.net_tx), (1000, 2000));
        assert_eq!((s.disk_total, s.disk_used), (10000 * 1024, 4000 * 1024));
        assert_eq!(s.uptime_secs, 3600);
        assert_eq!(s.cores, 4);
    }

    #[test]
    fn non_linux_returns_none() {
        assert!(parse_stats("@@\n@@\n@@\n@@\n/dev/disk1 1 1 1 1% /\n@@\n@@\n8").is_none());
    }
}
