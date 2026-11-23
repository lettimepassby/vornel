use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, ChannelOpenHandle, Handle, Msg, Session};
use russh::keys::{HashAlg, PublicKey, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg, ChannelOpenFailure, ChannelStream, Disconnect};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::{agent, SshError};

/// 认证方式。
#[derive(Clone)]
pub enum Auth {
    Password(String),
    /// OpenSSH 私钥文本（来自密钥串或磁盘文件）
    Key {
        pem: String,
        passphrase: Option<String>,
    },
    /// 系统 SSH Agent
    Agent,
}

/// 一跳连接的目标。
#[derive(Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
    /// 已信任的主机公钥（OpenSSH 一行格式）
    pub known_keys: Vec<String>,
}

pub struct ConnectOptions {
    pub target: Endpoint,
    /// 跳板机（ProxyJump）
    pub jump: Option<Endpoint>,
    /// 是否转发本机 SSH Agent
    pub agent_forward: bool,
}

/// 连接过程中见到的主机公钥。
#[derive(Debug, Clone)]
pub struct HostKey {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    pub public_key: String,
    /// 首次见到（应当记录为已信任）
    pub new: bool,
}

type RemoteForwards = Arc<Mutex<HashMap<u32, (String, u16)>>>;

pub(crate) struct Handler {
    host: String,
    port: u16,
    known: Vec<PublicKey>,
    seen: Arc<Mutex<Option<HostKey>>>,
    remote_forwards: RemoteForwards,
}

impl client::Handler for Handler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_public_key.public_key();
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let algorithm = key.algorithm().as_str().to_string();

        let same_alg: Vec<&PublicKey> = self
            .known
            .iter()
            .filter(|k| k.algorithm() == key.algorithm())
            .collect();
        let new = if same_alg.is_empty() {
            true
        } else if same_alg.iter().any(|k| k.key_data() == key.key_data()) {
            false
        } else {
            return Err(SshError::HostKeyChanged {
                host: self.host.clone(),
                port: self.port,
                fingerprint,
            });
        };

        if let Ok(mut slot) = self.seen.lock() {
            *slot = Some(HostKey {
                host: self.host.clone(),
                port: self.port,
                algorithm,
                fingerprint,
                public_key: key.to_openssh().unwrap_or_default(),
                new,
            });
        }
        Ok(true)
    }

    /// 远程端口转发：服务器把连接转回来，按端口找到本地目标并桥接。
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let target = self
            .remote_forwards
            .lock()
            .ok()
            .and_then(|m| m.get(&connected_port).cloned());
        match target {
            Some((host, port)) => {
                reply.accept().await;
                tokio::spawn(async move {
                    if let Ok(mut tcp) = TcpStream::connect((host.as_str(), port)).await {
                        let mut stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
                    }
                });
            }
            None => {
                reply
                    .reject(ChannelOpenFailure::AdministrativelyProhibited)
                    .await
            }
        }
        Ok(())
    }

    /// Agent 转发：服务器上的程序请求签名时，桥接到本机 Agent。
    async fn server_channel_open_agent_forward(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        tokio::spawn(async move {
            let _ = agent::bridge(channel.into_stream()).await;
        });
        Ok(())
    }
}

/// 一条已认证的 SSH 连接。可以在上面开多个 Shell、SFTP 和端口转发。
pub struct Connection {
    handle: Handle<Handler>,
    remote_forwards: RemoteForwards,
    agent_forward: bool,
    /// 经由跳板机时，保持跳板机连接存活
    _jump: Option<Box<Connection>>,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

fn config() -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    })
}

fn new_handler(ep: &Endpoint) -> (Handler, Arc<Mutex<Option<HostKey>>>, RemoteForwards) {
    let seen = Arc::new(Mutex::new(None));
    let forwards: RemoteForwards = Arc::new(Mutex::new(HashMap::new()));
    let known = ep
        .known_keys
        .iter()
        .filter_map(|k| PublicKey::from_openssh(k).ok())
        .collect();
    (
        Handler {
            host: ep.host.clone(),
            port: ep.port,
            known,
            seen: seen.clone(),
            remote_forwards: forwards.clone(),
        },
        seen,
        forwards,
    )
}

fn take_seen(seen: &Arc<Mutex<Option<HostKey>>>) -> Option<HostKey> {
    seen.lock().ok().and_then(|mut s| s.take())
}

async fn authenticate(handle: &mut Handle<Handler>, ep: &Endpoint) -> Result<(), SshError> {
    let ok = match &ep.auth {
        Auth::Password(pw) => handle
            .authenticate_password(&ep.user, pw.clone())
            .await?
            .success(),
        Auth::Key { pem, passphrase } => {
            let key = match russh::keys::decode_secret_key(pem, passphrase.as_deref()) {
                Ok(k) => k,
                Err(russh::keys::Error::KeyIsEncrypted) => return Err(SshError::NeedPassphrase),
                Err(e) => return Err(e.into()),
            };
            let hash = handle.best_supported_rsa_hash().await?.flatten();
            handle
                .authenticate_publickey(
                    &ep.user,
                    russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await?
                .success()
        }
        Auth::Agent => agent::authenticate(handle, &ep.user).await?,
    };
    if ok {
        Ok(())
    } else {
        Err(SshError::AuthFailed)
    }
}

/// 建立连接（必要时经由跳板机）并认证。返回连接和途中见到的主机公钥（跳板机在前）。
pub async fn connect(opts: ConnectOptions) -> Result<(Connection, Vec<HostKey>), SshError> {
    let mut host_keys = Vec::new();

    let jump = match &opts.jump {
        Some(ep) => {
            let (handler, seen, forwards) = new_handler(ep);
            let mut handle = tokio::time::timeout(
                CONNECT_TIMEOUT,
                client::connect(config(), (ep.host.as_str(), ep.port), handler),
            )
            .await
            .map_err(|_| SshError::Timeout)??;
            host_keys.extend(take_seen(&seen));
            authenticate(&mut handle, ep).await?;
            Some(Box::new(Connection {
                handle,
                remote_forwards: forwards,
                agent_forward: false,
                _jump: None,
            }))
        }
        None => None,
    };

    let ep = &opts.target;
    let (handler, seen, forwards) = new_handler(ep);
    let mut handle = match &jump {
        Some(j) => {
            let stream = j.direct_tcpip(&ep.host, ep.port).await?;
            tokio::time::timeout(
                CONNECT_TIMEOUT,
                client::connect_stream(config(), stream, handler),
            )
            .await
            .map_err(|_| SshError::Timeout)??
        }
        None => tokio::time::timeout(
            CONNECT_TIMEOUT,
            client::connect(config(), (ep.host.as_str(), ep.port), handler),
        )
        .await
        .map_err(|_| SshError::Timeout)??,
    };
    host_keys.extend(take_seen(&seen));
    authenticate(&mut handle, ep).await?;

    Ok((
        Connection {
            handle,
            remote_forwards: forwards,
            agent_forward: opts.agent_forward,
            _jump: jump,
        },
        host_keys,
    ))
}

/// 正在运行的端口转发；`stop()` 或丢弃时停止监听。
pub struct ForwardHandle {
    task: Option<JoinHandle<()>>,
    remote: Option<(String, u32)>,
    pub bound_port: u16,
}

impl Drop for ForwardHandle {
    fn drop(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
}

impl Connection {
    pub(crate) async fn open_session(&self) -> Result<Channel<Msg>, SshError> {
        let channel = self.handle.channel_open_session().await?;
        if self.agent_forward {
            let _ = channel.agent_forward(false).await;
        }
        Ok(channel)
    }

    /// 打开一条到 `host:port` 的直连通道（ssh -W）。
    pub async fn direct_tcpip(
        &self,
        host: &str,
        port: u16,
    ) -> Result<ChannelStream<Msg>, SshError> {
        let channel = self
            .handle
            .channel_open_direct_tcpip(host, port as u32, "127.0.0.1", 0)
            .await?;
        Ok(channel.into_stream())
    }

    /// 执行一条命令并收集输出（用于启动 mosh-server 等）。
    pub async fn exec(&self, command: &str) -> Result<(Vec<u8>, Option<u32>), SshError> {
        let mut channel = self.open_session().await?;
        channel.exec(true, command).await?;
        let mut out = Vec::new();
        let mut code = None;
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    out.extend_from_slice(&data)
                }
                ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok((out, code))
    }

    /// 本地转发（ssh -L）：本机 `bind:port` → 远端可达的 `target:port`。
    pub async fn local_forward(
        self: &Arc<Self>,
        bind: &str,
        port: u16,
        target_host: String,
        target_port: u16,
    ) -> Result<ForwardHandle, SshError> {
        let listener = TcpListener::bind((bind, port)).await?;
        let bound_port = listener.local_addr()?.port();
        let conn = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            while let Ok((mut tcp, _)) = listener.accept().await {
                let Some(conn) = conn.upgrade() else { break };
                let host = target_host.clone();
                tokio::spawn(async move {
                    if let Ok(mut stream) = conn.direct_tcpip(&host, target_port).await {
                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
                    }
                });
            }
        });
        Ok(ForwardHandle {
            task: Some(task),
            remote: None,
            bound_port,
        })
    }

    /// 动态转发（ssh -D）：本机 SOCKS5 代理，经由远端访问任意地址。
    pub async fn dynamic_forward(
        self: &Arc<Self>,
        bind: &str,
        port: u16,
    ) -> Result<ForwardHandle, SshError> {
        let listener = TcpListener::bind((bind, port)).await?;
        let bound_port = listener.local_addr()?.port();
        let conn = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let Some(conn) = conn.upgrade() else { break };
                tokio::spawn(async move {
                    let _ = socks5(tcp, conn).await;
                });
            }
        });
        Ok(ForwardHandle {
            task: Some(task),
            remote: None,
            bound_port,
        })
    }

    /// 远程转发（ssh -R）：远端 `bind:port` → 本机可达的 `target:port`。
    pub async fn remote_forward(
        &self,
        bind: &str,
        port: u16,
        target_host: String,
        target_port: u16,
    ) -> Result<ForwardHandle, SshError> {
        let bound = self.handle.tcpip_forward(bind, port as u32).await?;
        let bound = if bound == 0 { port as u32 } else { bound };
        if let Ok(mut m) = self.remote_forwards.lock() {
            m.insert(bound, (target_host, target_port));
        }
        Ok(ForwardHandle {
            task: None,
            remote: Some((bind.to_string(), bound)),
            bound_port: bound as u16,
        })
    }

    /// 停止一个转发；远程转发需要通知服务器取消监听。
    pub async fn stop_forward(&self, mut fwd: ForwardHandle) {
        if let Some((bind, port)) = fwd.remote.take() {
            let _ = self.handle.cancel_tcpip_forward(bind, port).await;
            if let Ok(mut m) = self.remote_forwards.lock() {
                m.remove(&port);
            }
        }
        drop(fwd);
    }

    pub async fn disconnect(&self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
    }

    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }
}

/// 最小可用的 SOCKS5（RFC 1928）：无认证，仅 CONNECT。
async fn socks5(mut tcp: TcpStream, conn: Arc<Connection>) -> Result<(), SshError> {
    use tokio::io::AsyncReadExt;

    let mut head = [0u8; 2];
    tcp.read_exact(&mut head).await?;
    if head[0] != 5 {
        return Err(SshError::Other("不是 SOCKS5 请求".into()));
    }
    let mut methods = vec![0u8; head[1] as usize];
    tcp.read_exact(&mut methods).await?;
    tcp.write_all(&[5, 0]).await?;

    let mut req = [0u8; 4];
    tcp.read_exact(&mut req).await?;
    if req[1] != 1 {
        tcp.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        return Err(SshError::Other("只支持 CONNECT".into()));
    }
    let host = match req[3] {
        1 => {
            let mut ip = [0u8; 4];
            tcp.read_exact(&mut ip).await?;
            std::net::Ipv4Addr::from(ip).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            tcp.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            tcp.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        4 => {
            let mut ip = [0u8; 16];
            tcp.read_exact(&mut ip).await?;
            std::net::Ipv6Addr::from(ip).to_string()
        }
        _ => return Err(SshError::Other("未知的地址类型".into())),
    };
    let mut port = [0u8; 2];
    tcp.read_exact(&mut port).await?;
    let port = u16::from_be_bytes(port);

    match conn.direct_tcpip(&host, port).await {
        Ok(mut stream) => {
            tcp.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
            Ok(())
        }
        Err(e) => {
            tcp.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            Err(e)
        }
    }
}
