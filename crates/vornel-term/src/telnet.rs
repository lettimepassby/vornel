//! Telnet 客户端：处理 IAC 协商（回显、抑制继续、终端类型、窗口大小），其余字节直接交给终端。

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::{Cmd, TermError, TermSession};

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;

const OPT_ECHO: u8 = 1;
const OPT_SGA: u8 = 3;
const OPT_TTYPE: u8 = 24;
const OPT_NAWS: u8 = 31;

const TTYPE_IS: u8 = 0;
const TTYPE_SEND: u8 = 1;

#[derive(Default)]
enum State {
    #[default]
    Data,
    Iac,
    Verb(u8),
    Sub(Vec<u8>),
    SubIac(Vec<u8>),
}

/// 协议状态机：输入服务器字节，输出给终端的数据和需要回复服务器的字节。
#[derive(Default)]
pub(crate) struct Parser {
    state: State,
    pub naws: bool,
}

pub(crate) struct Parsed {
    pub data: Vec<u8>,
    pub reply: Vec<u8>,
    /// 服务器刚同意了 NAWS，需要立刻发送窗口大小
    pub send_size: bool,
}

impl Parser {
    pub fn feed(&mut self, input: &[u8]) -> Parsed {
        let mut out = Parsed {
            data: Vec::with_capacity(input.len()),
            reply: Vec::new(),
            send_size: false,
        };
        for &b in input {
            self.state = match std::mem::take(&mut self.state) {
                State::Data => {
                    if b == IAC {
                        State::Iac
                    } else {
                        out.data.push(b);
                        State::Data
                    }
                }
                State::Iac => match b {
                    IAC => {
                        out.data.push(IAC);
                        State::Data
                    }
                    WILL | WONT | DO | DONT => State::Verb(b),
                    SB => State::Sub(Vec::new()),
                    _ => State::Data,
                },
                State::Verb(verb) => {
                    self.negotiate(verb, b, &mut out);
                    State::Data
                }
                State::Sub(mut buf) => {
                    if b == IAC {
                        State::SubIac(buf)
                    } else {
                        buf.push(b);
                        State::Sub(buf)
                    }
                }
                State::SubIac(mut buf) => match b {
                    SE => {
                        if buf.first() == Some(&OPT_TTYPE) && buf.get(1) == Some(&TTYPE_SEND) {
                            out.reply.extend_from_slice(&[IAC, SB, OPT_TTYPE, TTYPE_IS]);
                            out.reply.extend_from_slice(b"XTERM-256COLOR");
                            out.reply.extend_from_slice(&[IAC, SE]);
                        }
                        State::Data
                    }
                    IAC => {
                        buf.push(IAC);
                        State::Sub(buf)
                    }
                    _ => State::Data,
                },
            };
        }
        out
    }

    fn negotiate(&mut self, verb: u8, opt: u8, out: &mut Parsed) {
        match verb {
            WILL => {
                // 服务器愿意回显 / 抑制继续：同意；其他一律拒绝
                let ok = matches!(opt, OPT_ECHO | OPT_SGA);
                out.reply
                    .extend_from_slice(&[IAC, if ok { DO } else { DONT }, opt]);
            }
            DO => match opt {
                OPT_NAWS => {
                    out.reply.extend_from_slice(&[IAC, WILL, OPT_NAWS]);
                    self.naws = true;
                    out.send_size = true;
                }
                OPT_TTYPE | OPT_SGA => out.reply.extend_from_slice(&[IAC, WILL, opt]),
                _ => out.reply.extend_from_slice(&[IAC, WONT, opt]),
            },
            DONT => {
                if opt == OPT_NAWS {
                    self.naws = false;
                }
                out.reply.extend_from_slice(&[IAC, WONT, opt]);
            }
            _ => {}
        }
    }
}

fn naws(cols: u16, rows: u16) -> Vec<u8> {
    let mut v = vec![IAC, SB, OPT_NAWS];
    for b in cols.to_be_bytes().into_iter().chain(rows.to_be_bytes()) {
        v.push(b);
        if b == IAC {
            v.push(IAC);
        }
    }
    v.extend_from_slice(&[IAC, SE]);
    v
}

/// 用户输入 → 线路格式：0xFF 转义为 IAC IAC；单独的 CR 按 RFC 854 发送为 CR NUL。
fn escape_input(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 4);
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        out.push(b);
        if b == IAC {
            out.push(IAC);
        } else if b == b'\r' && data.get(i + 1) != Some(&b'\n') {
            out.push(0);
        }
        i += 1;
    }
    out
}

pub async fn connect<O, E>(
    host: &str,
    port: u16,
    cols: u16,
    rows: u16,
    mut on_output: O,
    on_exit: E,
) -> Result<TermSession, TermError>
where
    O: FnMut(Vec<u8>) + Send + 'static,
    E: FnOnce(Option<String>) + Send + 'static,
{
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        TcpStream::connect((host, port)),
    )
    .await
    .map_err(|_| {
        TermError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "连接超时",
        ))
    })??;
    let (mut rd, mut wr) = stream.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Cmd>();

    tokio::spawn(async move {
        let mut parser = Parser::default();
        let mut size = (cols, rows);
        let mut buf = vec![0u8; 16 * 1024];
        let reason = loop {
            tokio::select! {
                n = rd.read(&mut buf) => match n {
                    Ok(0) => break None,
                    Ok(n) => {
                        let p = parser.feed(&buf[..n]);
                        if !p.reply.is_empty() && wr.write_all(&p.reply).await.is_err() {
                            break Some("发送失败".into());
                        }
                        if p.send_size {
                            let _ = wr.write_all(&naws(size.0, size.1)).await;
                        }
                        if !p.data.is_empty() {
                            on_output(p.data);
                        }
                    }
                    Err(e) => break Some(e.to_string()),
                },
                cmd = rx.recv() => match cmd {
                    None => break None,
                    Some(Cmd::Write(d)) => {
                        if wr.write_all(&escape_input(&d)).await.is_err() {
                            break Some("发送失败".into());
                        }
                    }
                    Some(Cmd::Resize(c, r)) => {
                        size = (c, r);
                        if parser.naws {
                            let _ = wr.write_all(&naws(c, r)).await;
                        }
                    }
                },
            }
        };
        on_exit(reason);
    });

    Ok(TermSession { tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_data_passes_through() {
        let mut p = Parser::default();
        let r = p.feed(b"hello");
        assert_eq!(r.data, b"hello");
        assert!(r.reply.is_empty());
    }

    #[test]
    fn negotiation_split_across_chunks() {
        let mut p = Parser::default();
        let a = p.feed(&[b'a', IAC, DO]);
        let b = p.feed(&[OPT_NAWS, b'b']);
        assert_eq!(a.data, b"a");
        assert_eq!(b.data, b"b");
        assert_eq!(b.reply, vec![IAC, WILL, OPT_NAWS]);
        assert!(b.send_size && p.naws);
    }

    #[test]
    fn terminal_type_subnegotiation() {
        let mut p = Parser::default();
        let r = p.feed(&[IAC, SB, OPT_TTYPE, TTYPE_SEND, IAC, SE]);
        assert!(r.reply.starts_with(&[IAC, SB, OPT_TTYPE, TTYPE_IS]));
        assert!(r.reply.windows(5).any(|w| w == b"XTERM"));
    }

    #[test]
    fn escaped_iac_in_data() {
        let mut p = Parser::default();
        assert_eq!(p.feed(&[IAC, IAC]).data, vec![IAC]);
    }

    #[test]
    fn rejects_unknown_options() {
        let mut p = Parser::default();
        assert_eq!(p.feed(&[IAC, DO, 99]).reply, vec![IAC, WONT, 99]);
        assert_eq!(p.feed(&[IAC, WILL, 99]).reply, vec![IAC, DONT, 99]);
    }

    #[test]
    fn input_escaping() {
        assert_eq!(escape_input(b"a\r"), b"a\r\0");
        assert_eq!(escape_input(b"a\r\n"), b"a\r\n");
        assert_eq!(escape_input(&[IAC]), vec![IAC, IAC]);
    }

    #[test]
    fn naws_escapes_255() {
        assert_eq!(
            naws(255, 24),
            vec![IAC, SB, OPT_NAWS, 0, 255, 255, 0, 24, IAC, SE]
        );
    }

    #[tokio::test]
    async fn end_to_end_with_local_server() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            s.write_all(&[IAC, DO, OPT_NAWS]).await.unwrap();
            s.write_all(b"login: ").await.unwrap();
            let mut buf = [0u8; 64];
            let mut got = Vec::new();
            while !got.windows(2).any(|w| w == b"\r\0") {
                let n = s.read(&mut buf).await.unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            // 收到了 WILL NAWS、窗口大小和用户输入
            assert!(got.windows(3).any(|w| w == [IAC, WILL, OPT_NAWS]));
            assert!(got.windows(4).any(|w| w == [0, 80, 0, 24]));
            assert!(got.windows(4).any(|w| w == b"root"));
            s.write_all(b"ok").await.unwrap();
        });

        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let session = connect(
            "127.0.0.1",
            port,
            80,
            24,
            move |d| {
                let _ = out_tx.send(d);
            },
            |_| {},
        )
        .await
        .unwrap();
        let mut all = Vec::new();
        while !all.windows(7).any(|w| w == b"login: ") {
            all.extend(out_rx.recv().await.unwrap());
        }
        session.write(b"root\r".to_vec());
        while !all.windows(2).any(|w| w == b"ok") {
            all.extend(out_rx.recv().await.unwrap());
        }
    }
}
