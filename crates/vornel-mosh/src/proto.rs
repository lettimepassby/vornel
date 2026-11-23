//! 手写的最小 protobuf 编解码：只覆盖 Mosh 传输层与用户 / 主机消息用到的字段。

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub protocol_version: u32,
    pub old_num: u64,
    pub new_num: u64,
    pub ack_num: u64,
    pub throwaway_num: u64,
    pub diff: Vec<u8>,
}

pub const PROTOCOL_VERSION: u32 = 2;

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_key(out: &mut Vec<u8>, field: u32, wire: u8) {
    put_varint(out, ((field as u64) << 3) | wire as u64);
}

fn put_uint(out: &mut Vec<u8>, field: u32, v: u64) {
    put_key(out, field, 0);
    put_varint(out, v);
}

fn put_bytes(out: &mut Vec<u8>, field: u32, v: &[u8]) {
    put_key(out, field, 2);
    put_varint(out, v.len() as u64);
    out.extend_from_slice(v);
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = *self.buf.get(self.pos)?;
            self.pos += 1;
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }

    /// 下一个字段：(字段号, 值)。遇到不支持的线类型或数据截断时返回 None。
    fn next(&mut self) -> Option<(u32, Value<'a>)> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        match key & 7 {
            0 => Some((field, Value::Varint(self.varint()?))),
            2 => {
                let len = self.varint()? as usize;
                let end = self.pos.checked_add(len)?;
                let bytes = self.buf.get(self.pos..end)?;
                self.pos = end;
                Some((field, Value::Bytes(bytes)))
            }
            1 => {
                self.pos += 8;
                Some((field, Value::Varint(0)))
            }
            5 => {
                self.pos += 4;
                Some((field, Value::Varint(0)))
            }
            _ => None,
        }
    }
}

impl Instruction {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.diff.len() + 32);
        put_uint(&mut out, 1, self.protocol_version as u64);
        put_uint(&mut out, 2, self.old_num);
        put_uint(&mut out, 3, self.new_num);
        put_uint(&mut out, 4, self.ack_num);
        put_uint(&mut out, 5, self.throwaway_num);
        put_bytes(&mut out, 6, &self.diff);
        out
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        let mut r = Reader::new(buf);
        let mut i = Instruction::default();
        while let Some((field, v)) = r.next() {
            match (field, v) {
                (1, Value::Varint(x)) => i.protocol_version = x as u32,
                (2, Value::Varint(x)) => i.old_num = x,
                (3, Value::Varint(x)) => i.new_num = x,
                (4, Value::Varint(x)) => i.ack_num = x,
                (5, Value::Varint(x)) => i.throwaway_num = x,
                (6, Value::Bytes(b)) => i.diff = b.to_vec(),
                _ => {}
            }
        }
        Some(i)
    }
}

/// 客户端事件（UserMessage 里的一条 Instruction）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserEvent {
    Keys(Vec<u8>),
    Resize(u16, u16),
}

/// 编码 ClientBuffers.UserMessage：repeated Instruction(1)，扩展字段 keystroke(2) / resize(3)。
pub fn encode_user_message(events: &[UserEvent]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in events {
        let mut inst = Vec::new();
        match e {
            UserEvent::Keys(k) => {
                let mut ks = Vec::new();
                put_bytes(&mut ks, 4, k);
                put_bytes(&mut inst, 2, &ks);
            }
            UserEvent::Resize(w, h) => {
                let mut rs = Vec::new();
                put_uint(&mut rs, 5, *w as u64);
                put_uint(&mut rs, 6, *h as u64);
                put_bytes(&mut inst, 3, &rs);
            }
        }
        put_bytes(&mut out, 1, &inst);
    }
    out
}

/// 从 HostBuffers.HostMessage 中取出终端输出（hostbytes(2).hoststring(4)），按顺序拼接。
pub fn decode_host_bytes(buf: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut r = Reader::new(buf);
    while let Some((field, v)) = r.next() {
        if let (1, Value::Bytes(inst)) = (field, v) {
            let mut ri = Reader::new(inst);
            while let Some((f2, v2)) = ri.next() {
                if let (2, Value::Bytes(hb)) = (f2, v2) {
                    let mut rh = Reader::new(hb);
                    while let Some((f3, v3)) = rh.next() {
                        if let (4, Value::Bytes(s)) = (f3, v3) {
                            out.extend_from_slice(s);
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip_large() {
        let i = Instruction {
            protocol_version: 2,
            old_num: 0,
            new_num: u64::MAX,
            ack_num: 300,
            throwaway_num: 1,
            diff: b"abc".to_vec(),
        };
        assert_eq!(Instruction::decode(&i.encode()).unwrap(), i);
    }

    #[test]
    fn user_message_bytes() {
        // Instruction{ keystroke: Keystroke{ keys: "a" } }
        let enc = encode_user_message(&[UserEvent::Keys(b"a".to_vec())]);
        assert_eq!(enc, vec![0x0a, 0x05, 0x12, 0x03, 0x22, 0x01, b'a']);
    }

    #[test]
    fn host_bytes_are_extracted_in_order() {
        let mut hb1 = Vec::new();
        put_bytes(&mut hb1, 4, b"hello ");
        let mut inst1 = Vec::new();
        put_bytes(&mut inst1, 2, &hb1);
        let mut hb2 = Vec::new();
        put_bytes(&mut hb2, 4, b"world");
        let mut inst2 = Vec::new();
        put_bytes(&mut inst2, 2, &hb2);
        let mut resize = Vec::new();
        put_uint(&mut resize, 5, 80);
        let mut inst3 = Vec::new();
        put_bytes(&mut inst3, 3, &resize);
        let mut msg = Vec::new();
        put_bytes(&mut msg, 1, &inst1);
        put_bytes(&mut msg, 1, &inst3);
        put_bytes(&mut msg, 1, &inst2);
        assert_eq!(decode_host_bytes(&msg), b"hello world");
    }

    #[test]
    fn truncated_input_does_not_panic() {
        assert!(Instruction::decode(&[0x32, 0x10, 1, 2]).is_some());
        assert!(decode_host_bytes(&[0x0a, 0xff]).is_empty());
    }
}
