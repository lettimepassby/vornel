//! 存储格式：每条记录按字段保存值与时间戳，合并时逐字段“最后写入者胜”。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::hlc::{Clock, Hlc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRecord {
    pub id: Uuid,
    pub kind: String,
    pub fields: BTreeMap<String, Value>,
    pub clocks: BTreeMap<String, Hlc>,
    /// 墓碑：删除时间
    #[serde(default)]
    pub deleted: Option<Hlc>,
    #[serde(default)]
    pub local_only: bool,
}

impl StoredRecord {
    pub fn new(kind: &str, id: Uuid) -> Self {
        Self {
            id,
            kind: kind.to_string(),
            fields: BTreeMap::new(),
            clocks: BTreeMap::new(),
            deleted: None,
            local_only: false,
        }
    }

    /// 记录在 Vault 中的键：`kind:uuid`
    pub fn key(&self) -> String {
        record_key(&self.kind, self.id)
    }

    /// 记录的版本 = 所有字段时间戳与墓碑中的最大值
    pub fn version(&self) -> Hlc {
        self.clocks
            .values()
            .chain(self.deleted.iter())
            .max()
            .cloned()
            .unwrap_or(Hlc {
                wall: 0,
                counter: 0,
                node: String::new(),
            })
    }

    /// 删除时间晚于所有字段的最后修改时间，才算删除
    pub fn is_deleted(&self) -> bool {
        match &self.deleted {
            Some(d) => self.clocks.values().all(|c| c <= d),
            None => false,
        }
    }

    /// 写入新值：只有真正变化的字段才更新时间戳。
    pub fn update(&mut self, value: Value, clock: &mut Clock) -> bool {
        let Value::Object(obj) = value else {
            return false;
        };
        let mut changed = false;
        for (k, v) in obj {
            if self.fields.get(&k) != Some(&v) || self.is_deleted() {
                self.fields.insert(k.clone(), v);
                self.clocks.insert(k, clock.tick());
                changed = true;
            }
        }
        changed
    }

    pub fn mark_deleted(&mut self, clock: &mut Clock) {
        self.deleted = Some(clock.tick());
    }

    /// 合并另一份副本：逐字段取时间戳较大者；墓碑取较晚者。返回本副本是否改变。
    pub fn merge(&mut self, other: &StoredRecord) -> bool {
        let mut changed = false;
        for (k, oc) in &other.clocks {
            let newer = self.clocks.get(k).is_none_or(|c| oc > c);
            if newer {
                if let Some(v) = other.fields.get(k) {
                    self.fields.insert(k.clone(), v.clone());
                }
                self.clocks.insert(k.clone(), oc.clone());
                changed = true;
            }
        }
        if let Some(od) = &other.deleted {
            if self.deleted.as_ref().is_none_or(|d| od > d) {
                self.deleted = Some(od.clone());
                changed = true;
            }
        }
        changed
    }

    pub fn to_value(&self) -> Value {
        Value::Object(self.fields.clone().into_iter().collect::<Map<_, _>>())
    }
}

pub fn record_key(kind: &str, id: Uuid) -> String {
    format!("{kind}:{id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rec() -> StoredRecord {
        StoredRecord::new("host", Uuid::nil())
    }

    #[test]
    fn concurrent_edits_merge_per_field() {
        let mut a_clock = Clock::new("a");
        let mut b_clock = Clock::new("b");

        let mut base = rec();
        base.update(json!({"name": "web", "port": 22}), &mut a_clock);

        // 两台设备同时修改不同字段
        let mut a = base.clone();
        let mut b = base.clone();
        b_clock.observe(&base.version());
        a.update(json!({"name": "web-01", "port": 22}), &mut a_clock);
        b.update(json!({"name": "web", "port": 2222}), &mut b_clock);

        let mut merged_ab = a.clone();
        merged_ab.merge(&b);
        let mut merged_ba = b.clone();
        merged_ba.merge(&a);

        // 两个方向合并结果一致，且两边的修改都保留
        assert_eq!(merged_ab.fields, merged_ba.fields);
        assert_eq!(merged_ab.fields["name"], json!("web-01"));
        assert_eq!(merged_ab.fields["port"], json!(2222));
    }

    #[test]
    fn delete_then_newer_edit_resurrects() {
        let mut c = Clock::new("a");
        let mut r = rec();
        r.update(json!({"name": "x"}), &mut c);
        r.mark_deleted(&mut c);
        assert!(r.is_deleted());
        r.update(json!({"name": "x"}), &mut c);
        assert!(!r.is_deleted());
    }

    #[test]
    fn tombstone_wins_over_older_edits() {
        let mut c = Clock::new("a");
        let mut a = rec();
        a.update(json!({"name": "x"}), &mut c);
        let mut b = a.clone();
        b.mark_deleted(&mut c);
        a.merge(&b);
        assert!(a.is_deleted());
    }

    #[test]
    fn unchanged_fields_keep_their_clock() {
        let mut c = Clock::new("a");
        let mut r = rec();
        r.update(json!({"name": "x", "port": 1}), &mut c);
        let before = r.clocks["name"].clone();
        r.update(json!({"name": "x", "port": 2}), &mut c);
        assert_eq!(r.clocks["name"], before);
        assert!(r.clocks["port"] > before);
    }
}
