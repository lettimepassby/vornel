//! 混合逻辑时钟（HLC）：物理毫秒 + 逻辑计数 + 设备 id。
//!
//! 可以全序比较，且保证同一设备上单调递增；不同设备的时钟偏差不会导致“新写入比旧写入小”。

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hlc {
    pub wall: u64,
    pub counter: u32,
    pub node: String,
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.wall, self.counter, &self.node).cmp(&(other.wall, other.counter, &other.node))
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 定宽数字保证字符串排序与数值排序一致
        write!(f, "{:013}.{:06}.{}", self.wall, self.counter, self.node)
    }
}

impl FromStr for Hlc {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(3, '.');
        let wall = parts.next().and_then(|p| p.parse().ok());
        let counter = parts.next().and_then(|p| p.parse().ok());
        let node = parts.next();
        match (wall, counter, node) {
            (Some(wall), Some(counter), Some(node)) => Ok(Hlc {
                wall,
                counter,
                node: node.to_string(),
            }),
            _ => Err(format!("无效的 HLC：{s}")),
        }
    }
}

impl Serialize for Hlc {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Hlc {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// 本设备的时钟。
#[derive(Debug)]
pub struct Clock {
    last: Hlc,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Clock {
    pub fn new(node: impl Into<String>) -> Self {
        Self {
            last: Hlc {
                wall: 0,
                counter: 0,
                node: node.into(),
            },
        }
    }

    pub fn node(&self) -> &str {
        &self.last.node
    }

    /// 产生一个新的、严格大于之前所有值的时间戳。
    pub fn tick(&mut self) -> Hlc {
        let now = now_ms();
        if now > self.last.wall {
            self.last.wall = now;
            self.last.counter = 0;
        } else {
            self.last.counter += 1;
        }
        self.last.clone()
    }

    /// 收到其他设备的时间戳后推进本地时钟，保证之后的写入排在它后面。
    pub fn observe(&mut self, remote: &Hlc) {
        if remote.wall > self.last.wall
            || (remote.wall == self.last.wall && remote.counter > self.last.counter)
        {
            self.last.wall = remote.wall;
            self.last.counter = remote.counter;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_is_monotonic() {
        let mut c = Clock::new("a");
        let mut prev = c.tick();
        for _ in 0..1000 {
            let next = c.tick();
            assert!(next > prev);
            prev = next;
        }
    }

    #[test]
    fn observe_moves_forward() {
        let mut c = Clock::new("a");
        let far = Hlc {
            wall: now_ms() + 60_000,
            counter: 5,
            node: "b".into(),
        };
        c.observe(&far);
        assert!(c.tick() > far);
    }

    #[test]
    fn string_roundtrip_and_order() {
        let a = Hlc {
            wall: 5,
            counter: 2,
            node: "x".into(),
        };
        let b = Hlc {
            wall: 12,
            counter: 0,
            node: "a".into(),
        };
        assert_eq!(a.to_string().parse::<Hlc>().unwrap(), a);
        assert!(a < b);
        assert!(a.to_string() < b.to_string());
    }
}
