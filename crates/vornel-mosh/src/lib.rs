//! Mosh 客户端（mosh 1.x 线协议），不依赖任何 GPL 代码，按协议规范独立实现：
//!
//! - 加密：AES-128-OCB，12 字节 nonce = 4 个 0 字节 + 8 字节大端序号；序号最高位为方向（1 = 发往客户端）
//! - 包：2 字节时间戳 + 2 字节回显时间戳 + 分片
//! - 分片：8 字节 id + 2 字节片号（最高位表示最后一片）+ 内容；拼接后 zlib 解压得到 Instruction
//! - 状态同步：客户端发送“从服务器已确认的状态到最新状态”的增量；服务器发送终端画面的增量
//!
//! 简化：不在本地维护终端模拟器。服务器的增量只有在基准状态等于本地当前状态时才应用，
//! 否则忽略并继续确认当前状态，服务器会改用正确的基准重发。

mod proto;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aes::Aes128;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use ocb3::aead::{Aead, KeyInit};
use ocb3::Ocb3;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

pub use proto::UserEvent;
use proto::{decode_host_bytes, encode_user_message, Instruction, PROTOCOL_VERSION};

/// 通过 SSH 启动 mosh-server 的命令（找不到 en_US.UTF-8 时退回 C.UTF-8）。
pub const SERVER_COMMAND: &str = "mosh-server new -s -c 256 -l LANG=en_US.UTF-8 -l LC_ALL=en_US.UTF-8 2>&1 || mosh-server new -s -c 256 -l LANG=C.UTF-8 -l LC_ALL=C.UTF-8 2>&1";

#[derive(Debug, thiserror::Error)]
pub enum MoshError {
    #[error("网络错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("Mosh 密钥无效")]
    BadKey,
    #[error("没有在输出中找到 MOSH CONNECT，服务器上可能没有安装 mosh-server：{0}")]
    NoConnectLine(String),
}

/// 从 `mosh-server new` 的输出里取出端口和密钥。
pub fn parse_connect(output: &str) -> Option<(u16, String)> {
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("MOSH CONNECT ") {
            let mut parts = rest.split_whitespace();
            let port = parts.next()?.parse().ok()?;
            let key = parts.next()?.to_string();
            return Some((port, key));
        }
    }
    None
}

const TO_CLIENT: u64 = 1 << 63;
const FINAL: u16 = 0x8000;
const MTU: usize = 1000;
const SEND_INTERVAL: Duration = Duration::from_millis(20);
const ACK_DELAY: Duration = Duration::from_millis(100);
const HEARTBEAT: Duration = Duration::from_secs(3);
const RETRANSMIT: Duration = Duration::from_millis(500);

struct Crypto {
    cipher: Ocb3<Aes128>,
}

fn nonce(n: u64) -> [u8; 12] {
    let mut a = [0u8; 12];
    a[4..].copy_from_slice(&n.to_be_bytes());
    a
}

impl Crypto {
    fn new(key_b64: &str) -> Result<Self, MoshError> {
        let key = STANDARD_NO_PAD
            .decode(key_b64.trim().trim_end_matches('='))
            .map_err(|_| MoshError::BadKey)?;
        if key.len() != 16 {
            return Err(MoshError::BadKey);
        }
        Ok(Self {
            cipher: Ocb3::<Aes128>::new_from_slice(&key).map_err(|_| MoshError::BadKey)?,
        })
    }

    fn seal(&self, seq: u64, plaintext: &[u8]) -> Vec<u8> {
        let mut out = seq.to_be_bytes().to_vec();
        if let Ok(ct) = self.cipher.encrypt(&nonce(seq).into(), plaintext) {
            out.extend_from_slice(&ct);
        }
        out
    }

    fn open(&self, packet: &[u8]) -> Option<(u64, Vec<u8>)> {
        let seq = u64::from_be_bytes(packet.get(..8)?.try_into().ok()?);
        if seq & TO_CLIENT == 0 {
            return None;
        }
        let pt = self.cipher.decrypt(&nonce(seq).into(), &packet[8..]).ok()?;
        Some((seq, pt))
    }
}

fn now16() -> u16 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u16)
        .unwrap_or(0)
}

fn compress(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

fn decompress(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

/// 把一条 Instruction 切成若干分片。
fn fragments(id: u64, inst: &Instruction) -> Vec<Vec<u8>> {
    let payload = compress(&inst.encode());
    let chunks: Vec<&[u8]> = if payload.is_empty() {
        vec![&[][..]]
    } else {
        payload.chunks(MTU).collect()
    };
    let last = chunks.len() - 1;
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let mut f = id.to_be_bytes().to_vec();
            let num = i as u16 | if i == last { FINAL } else { 0 };
            f.extend_from_slice(&num.to_be_bytes());
            f.extend_from_slice(c);
            f
        })
        .collect()
}

/// 分片重组：同一 id 的分片收齐后返回解压后的 Instruction。
#[derive(Default)]
struct Assembler {
    id: Option<u64>,
    parts: BTreeMap<u16, Vec<u8>>,
    total: Option<u16>,
}

impl Assembler {
    fn add(&mut self, frag: &[u8]) -> Option<Instruction> {
        let id = u64::from_be_bytes(frag.get(..8)?.try_into().ok()?);
        let num = u16::from_be_bytes(frag.get(8..10)?.try_into().ok()?);
        let body = &frag[10..];
        if self.id != Some(id) {
            self.id = Some(id);
            self.parts.clear();
            self.total = None;
        }
        let index = num & !FINAL;
        if num & FINAL != 0 {
            self.total = Some(index + 1);
        }
        self.parts.insert(index, body.to_vec());
        let total = self.total?;
        if self.parts.len() as u16 != total {
            return None;
        }
        let joined: Vec<u8> = std::mem::take(&mut self.parts)
            .into_values()
            .flatten()
            .collect();
        self.id = None;
        self.total = None;
        Instruction::decode(&decompress(&joined)?)
    }
}

enum Cmd {
    Event(UserEvent),
}

/// 一个 Mosh 会话；丢弃时停止。
pub struct MoshSession {
    tx: mpsc::UnboundedSender<Cmd>,
}

impl MoshSession {
    pub fn write(&self, data: Vec<u8>) {
        let _ = self.tx.send(Cmd::Event(UserEvent::Keys(data)));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.tx.send(Cmd::Event(UserEvent::Resize(cols, rows)));
    }
}

struct State {
    crypto: Crypto,
    seq: u64,
    frag_id: u64,
    /// 本地用户事件：状态号 → 事件
    events: BTreeMap<u64, UserEvent>,
    latest: u64,
    /// 服务器已确认的客户端状态
    server_ack: u64,
    /// 已应用的服务器状态
    remote: u64,
    need_ack: Option<Instant>,
    last_send: Instant,
    last_ts: Option<(u16, Instant)>,
}

impl State {
    fn instruction(&self) -> Instruction {
        let pending: Vec<UserEvent> = self
            .events
            .range(self.server_ack + 1..)
            .map(|(_, e)| e.clone())
            .collect();
        Instruction {
            protocol_version: PROTOCOL_VERSION,
            old_num: self.server_ack,
            new_num: self.latest,
            ack_num: self.remote,
            throwaway_num: self.server_ack,
            diff: if pending.is_empty() {
                Vec::new()
            } else {
                encode_user_message(&pending)
            },
        }
    }

    async fn send(&mut self, sock: &UdpSocket) {
        let inst = self.instruction();
        self.frag_id += 1;
        for frag in fragments(self.frag_id, &inst) {
            let reply = match self.last_ts {
                Some((ts, at)) => ts.wrapping_add(at.elapsed().as_millis() as u16),
                None => u16::MAX,
            };
            let mut pt = Vec::with_capacity(frag.len() + 4);
            pt.extend_from_slice(&now16().to_be_bytes());
            pt.extend_from_slice(&reply.to_be_bytes());
            pt.extend_from_slice(&frag);
            let packet = self.crypto.seal(self.seq, &pt);
            self.seq += 1;
            let _ = sock.send(&packet).await;
        }
        self.last_send = Instant::now();
        self.need_ack = None;
    }

    fn push(&mut self, e: UserEvent) {
        self.latest += 1;
        self.events.insert(self.latest, e);
    }
}

/// 连接 mosh-server（UDP）。`key` 为 MOSH CONNECT 输出的 22 字符 base64 密钥。
pub async fn connect<O, E>(
    server: SocketAddr,
    key: &str,
    cols: u16,
    rows: u16,
    mut on_output: O,
    on_exit: E,
) -> Result<MoshSession, MoshError>
where
    O: FnMut(Vec<u8>) + Send + 'static,
    E: FnOnce(Option<String>) + Send + 'static,
{
    let crypto = Crypto::new(key)?;
    let bind: SocketAddr = if server.is_ipv6() {
        "[::]:0".parse().expect("合法地址")
    } else {
        "0.0.0.0:0".parse().expect("合法地址")
    };
    let sock = UdpSocket::bind(bind).await?;
    sock.connect(server).await?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Cmd>();
    let mut st = State {
        crypto,
        seq: 0,
        frag_id: 0,
        events: BTreeMap::new(),
        latest: 0,
        server_ack: 0,
        remote: 0,
        need_ack: None,
        last_send: Instant::now(),
        last_ts: None,
    };
    // 第一个事件：告诉服务器终端尺寸
    st.push(UserEvent::Resize(cols, rows));

    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        let mut asm = Assembler::default();
        let mut tick = tokio::time::interval(SEND_INTERVAL);
        let mut dirty = true;
        let reason = loop {
            tokio::select! {
                r = sock.recv(&mut buf) => {
                    let n = match r { Ok(n) => n, Err(_) => continue };
                    let Some((_seq, pt)) = st.crypto.open(&buf[..n]) else { continue };
                    if pt.len() < 4 { continue }
                    let ts = u16::from_be_bytes([pt[0], pt[1]]);
                    st.last_ts = Some((ts, Instant::now()));
                    let Some(inst) = asm.add(&pt[4..]) else { continue };
                    if inst.protocol_version != PROTOCOL_VERSION { continue }

                    if inst.ack_num > st.server_ack && inst.ack_num <= st.latest {
                        st.server_ack = inst.ack_num;
                        let keep = st.events.split_off(&(st.server_ack + 1));
                        st.events = keep;
                    }
                    // 服务器关闭：new_num 为全 1
                    if inst.new_num == u64::MAX {
                        st.remote = u64::MAX;
                        st.send(&sock).await;
                        break None;
                    }
                    if inst.new_num > st.remote && inst.old_num == st.remote {
                        if !inst.diff.is_empty() {
                            let out = decode_host_bytes(&inst.diff);
                            if !out.is_empty() {
                                on_output(out);
                            }
                        }
                        st.remote = inst.new_num;
                    }
                    if st.need_ack.is_none() {
                        st.need_ack = Some(Instant::now());
                    }
                }
                cmd = rx.recv() => match cmd {
                    None => break None,
                    Some(Cmd::Event(e)) => {
                        st.push(e);
                        dirty = true;
                    }
                },
                _ = tick.tick() => {
                    let unacked = st.server_ack < st.latest;
                    let due = dirty
                        || st.need_ack.is_some_and(|t| t.elapsed() >= ACK_DELAY)
                        || (unacked && st.last_send.elapsed() >= RETRANSMIT)
                        || st.last_send.elapsed() >= HEARTBEAT;
                    if due {
                        st.send(&sock).await;
                        dirty = false;
                    }
                }
            }
        };
        on_exit(reason);
    });

    Ok(MoshSession { tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_connect_line() {
        let out = "\r\nMOSH CONNECT 60001 4NeCCgvZFe2RnPgrcU1PQw\r\n\r\nmosh-server (mosh 1.4.0) [build mosh 1.4.0]\r\n";
        assert_eq!(
            parse_connect(out),
            Some((60001, "4NeCCgvZFe2RnPgrcU1PQw".to_string()))
        );
        assert_eq!(parse_connect("command not found"), None);
    }

    #[test]
    fn crypto_roundtrip_with_direction_bit() {
        let c = Crypto::new("4NeCCgvZFe2RnPgrcU1PQw").unwrap();
        let sealed = c.seal(TO_CLIENT | 7, b"payload");
        assert_eq!(c.open(&sealed), Some((TO_CLIENT | 7, b"payload".to_vec())));
        // 发往服务器方向的包不会被客户端接受
        assert_eq!(c.open(&c.seal(7, b"x")), None);
    }

    #[test]
    fn bad_key_is_rejected() {
        assert!(Crypto::new("short").is_err());
    }

    #[test]
    fn fragment_and_reassemble() {
        let inst = Instruction {
            protocol_version: 2,
            old_num: 1,
            new_num: 2,
            ack_num: 3,
            throwaway_num: 1,
            // 随机性强的数据压缩后仍然很大，迫使拆成多片
            diff: (0..20_000u32)
                .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
                .collect(),
        };
        let frags = fragments(42, &inst);
        assert!(frags.len() > 1);
        let mut asm = Assembler::default();
        let mut result = None;
        // 乱序到达
        for f in frags.iter().rev() {
            result = asm.add(f).or(result);
        }
        assert_eq!(result, Some(inst));
    }

    /// 端到端测试：需要本机安装 mosh-server（CI 中通过 apt 安装后运行）。
    #[tokio::test]
    #[ignore]
    async fn end_to_end_with_real_mosh_server() {
        let out = tokio::process::Command::new("mosh-server")
            .args([
                "new",
                "-i",
                "127.0.0.1",
                "-c",
                "256",
                "-l",
                "LANG=C.UTF-8",
                "-l",
                "LC_ALL=C.UTF-8",
                "--",
                "/bin/sh",
            ])
            .output()
            .await
            .expect("运行 mosh-server");
        let text = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        let (port, key) = parse_connect(&text).expect("MOSH CONNECT");

        let (otx, mut orx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (etx, mut erx) = mpsc::unbounded_channel::<Option<String>>();
        let session = connect(
            format!("127.0.0.1:{port}").parse().unwrap(),
            &key,
            80,
            24,
            move |d| {
                let _ = otx.send(d);
            },
            move |r| {
                let _ = etx.send(r);
            },
        )
        .await
        .unwrap();

        tokio::time::sleep(Duration::from_millis(500)).await;
        session.write(b"echo vornel-$((40+2))\r".to_vec());

        let mut screen = Vec::new();
        let found = tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(chunk) = orx.recv().await {
                screen.extend(chunk);
                if screen.windows(9).any(|w| w == b"vornel-42") {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        assert!(
            found,
            "没有收到命令输出：{}",
            String::from_utf8_lossy(&screen)
        );

        session.write(b"exit\r".to_vec());
        let closed = tokio::time::timeout(Duration::from_secs(15), erx.recv()).await;
        assert!(closed.is_ok(), "shell 退出后会话没有结束");
    }
}
