//! 端到端测试：连接真实的 sshd。
//!
//! 需要环境变量（CI 中由工作流准备）：
//! - `VORNEL_TEST_SSH_PORT`：sshd 端口
//! - `VORNEL_TEST_SSH_USER`：用户名
//! - `VORNEL_TEST_SSH_KEY`：私钥文件路径
//!
//! 运行：`cargo test -p vornel-ssh --test sshd -- --ignored`

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use vornel_ssh::{connect, Auth, ConnectOptions, Endpoint, SshError};

fn endpoint(known_keys: Vec<String>) -> Endpoint {
    let port = std::env::var("VORNEL_TEST_SSH_PORT")
        .expect("VORNEL_TEST_SSH_PORT")
        .parse()
        .unwrap();
    let user = std::env::var("VORNEL_TEST_SSH_USER").expect("VORNEL_TEST_SSH_USER");
    let pem =
        std::fs::read_to_string(std::env::var("VORNEL_TEST_SSH_KEY").expect("VORNEL_TEST_SSH_KEY"))
            .unwrap();
    Endpoint {
        host: "127.0.0.1".into(),
        port,
        user,
        auth: Auth::Key {
            pem,
            passphrase: None,
        },
        known_keys,
    }
}

fn opts(ep: Endpoint) -> ConnectOptions {
    ConnectOptions {
        target: ep,
        jump: None,
        agent_forward: false,
    }
}

#[tokio::test]
#[ignore]
async fn host_key_trust_on_first_use_and_change_detection() {
    let (conn, keys) = connect(opts(endpoint(vec![]))).await.unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys[0].new, "首次连接应标记为新主机");
    assert!(keys[0].fingerprint.starts_with("SHA256:"));
    let known = keys[0].public_key.clone();
    conn.disconnect().await;

    let (conn, keys) = connect(opts(endpoint(vec![known]))).await.unwrap();
    assert!(!keys[0].new, "已信任的主机不应再标记为新");
    conn.disconnect().await;

    // 同算法的另一把公钥 → 视为主机密钥变化，拒绝连接
    let fake = vornel_ssh::keys::generate_ed25519("fake")
        .unwrap()
        .public_key;
    match connect(opts(endpoint(vec![fake]))).await {
        Err(SshError::HostKeyChanged { .. }) => {}
        Err(e) => panic!("应当是主机密钥变化错误，实际：{e}"),
        Ok(_) => panic!("主机密钥变化时不应连接成功"),
    }
}

#[tokio::test]
#[ignore]
async fn shell_and_exec() {
    let (conn, _) = connect(opts(endpoint(vec![]))).await.unwrap();

    let (out, code) = conn.exec("echo exec-$((20+3))").await.unwrap();
    assert!(String::from_utf8_lossy(&out).contains("exec-23"));
    assert_eq!(code, Some(0));

    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let shell = conn
        .open_shell(
            80,
            24,
            move |d| {
                let _ = tx.send(d);
            },
            |_| {},
        )
        .await
        .unwrap();
    shell
        .write(b"echo shell-$((40+2))\n".to_vec())
        .await
        .unwrap();
    let mut all = Vec::new();
    let ok = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(d) = rx.recv().await {
            all.extend(d);
            if all.windows(8).any(|w| w == b"shell-42") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(ok, "Shell 输出：{}", String::from_utf8_lossy(&all));
    shell.resize(120, 40).await.unwrap();
    shell.close().await;
    conn.disconnect().await;
}

#[tokio::test]
#[ignore]
async fn sftp_roundtrip() {
    let (conn, _) = connect(opts(endpoint(vec![]))).await.unwrap();
    let sftp = conn.open_sftp().await.unwrap();
    let home = sftp.canonicalize(".").await.unwrap();
    let dir = format!("{home}/vornel-sftp-test");
    let _ = sftp.remove(&dir).await;
    sftp.mkdir(&dir).await.unwrap();

    let local_dir = std::env::temp_dir().join(format!("vornel-sftp-{}", std::process::id()));
    std::fs::create_dir_all(&local_dir).unwrap();
    let src = local_dir.join("upload.bin");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&src, &data).unwrap();

    let mut last = (0, 0);
    sftp.upload(&src, &format!("{dir}/upload.bin"), |d, t| last = (d, t))
        .await
        .unwrap();
    assert_eq!(last, (data.len() as u64, data.len() as u64));

    let entries = sftp.list(&dir).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, data.len() as u64);

    sftp.rename(&format!("{dir}/upload.bin"), &format!("{dir}/renamed.bin"))
        .await
        .unwrap();
    let dst = local_dir.join("renamed.bin");
    sftp.download(&format!("{dir}/renamed.bin"), &dst, |_, _| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), data);

    sftp.remove(&dir).await.unwrap();
    assert!(sftp.list(&dir).await.is_err());
    std::fs::remove_dir_all(&local_dir).ok();
    conn.disconnect().await;
}

async fn read_banner(mut s: TcpStream) -> String {
    let mut buf = [0u8; 64];
    let n = tokio::time::timeout(Duration::from_secs(10), s.read(&mut buf))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

#[tokio::test]
#[ignore]
async fn local_and_dynamic_forwarding() {
    let ep = endpoint(vec![]);
    let ssh_port = ep.port;
    let (conn, _) = connect(opts(ep)).await.unwrap();
    let conn = Arc::new(conn);

    // 本地转发：本机随机端口 → 远端 127.0.0.1:sshd，应读到 SSH 版本横幅
    let fwd = conn
        .local_forward("127.0.0.1", 0, "127.0.0.1".into(), ssh_port)
        .await
        .unwrap();
    let banner = read_banner(
        TcpStream::connect(("127.0.0.1", fwd.bound_port))
            .await
            .unwrap(),
    )
    .await;
    assert!(banner.starts_with("SSH-2.0"), "本地转发读到：{banner}");
    conn.stop_forward(fwd).await;

    // SOCKS5 动态转发
    let socks = conn.dynamic_forward("127.0.0.1", 0).await.unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", socks.bound_port))
        .await
        .unwrap();
    s.write_all(&[5, 1, 0]).await.unwrap();
    let mut reply = [0u8; 2];
    s.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [5, 0]);
    let mut req = vec![5, 1, 0, 1, 127, 0, 0, 1];
    req.extend_from_slice(&ssh_port.to_be_bytes());
    s.write_all(&req).await.unwrap();
    let mut resp = [0u8; 10];
    s.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp[1], 0, "SOCKS5 CONNECT 失败");
    let banner = read_banner(s).await;
    assert!(banner.starts_with("SSH-2.0"), "SOCKS5 读到：{banner}");
    conn.stop_forward(socks).await;
    conn.disconnect().await;
}

#[tokio::test]
#[ignore]
async fn wrong_key_is_rejected() {
    let mut ep = endpoint(vec![]);
    ep.auth = Auth::Key {
        pem: vornel_ssh::keys::generate_ed25519("other")
            .unwrap()
            .private_key,
        passphrase: None,
    };
    assert!(matches!(connect(opts(ep)).await, Err(SshError::AuthFailed)));
}

#[tokio::test]
#[ignore]
async fn jump_host_proxy_jump() {
    // 跳板机和目标都是同一台 sshd：目标连接经跳板机的 direct-tcpip 通道建立
    let (conn, keys) = connect(ConnectOptions {
        target: endpoint(vec![]),
        jump: Some(endpoint(vec![])),
        agent_forward: false,
    })
    .await
    .unwrap();
    assert_eq!(keys.len(), 2, "应分别校验跳板机与目标的主机密钥");
    let (out, code) = conn.exec("echo via-jump").await.unwrap();
    assert_eq!(code, Some(0));
    assert!(String::from_utf8_lossy(&out).contains("via-jump"));
    conn.disconnect().await;
}

#[tokio::test]
#[ignore]
async fn remote_forwarding() {
    // 本机起一个服务；远端（同一台机器）监听端口 → 经 SSH 回到本机服务
    let svc = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let svc_port = svc.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = svc.accept().await {
            let _ = s.write_all(b"hello-from-local\n").await;
        }
    });
    let (conn, _) = connect(opts(endpoint(vec![]))).await.unwrap();
    let conn = Arc::new(conn);
    let fwd = conn
        .remote_forward("127.0.0.1", 0, "127.0.0.1".into(), svc_port)
        .await
        .unwrap();
    assert_ne!(fwd.bound_port, 0);
    let got = read_banner(
        TcpStream::connect(("127.0.0.1", fwd.bound_port))
            .await
            .unwrap(),
    )
    .await;
    assert!(got.starts_with("hello-from-local"), "远程转发读到：{got}");
    conn.stop_forward(fwd).await;
    conn.disconnect().await;
}

#[tokio::test]
#[ignore]
async fn agent_auth_and_forwarding() {
    // 需要 SSH_AUTH_SOCK 指向已加载测试私钥的 ssh-agent
    let mut ep = endpoint(vec![]);
    ep.auth = Auth::Agent;
    let (conn, _) = connect(ConnectOptions {
        target: ep,
        jump: None,
        agent_forward: true,
    })
    .await
    .unwrap();
    // 远端通过转发过来的 Agent 能看到本机 Agent 里的密钥
    let (out, code) = conn.exec("ssh-add -l").await.unwrap();
    let out = String::from_utf8_lossy(&out);
    assert_eq!(code, Some(0), "远端 ssh-add -l 失败：{out}");
    assert!(out.contains("vornel-test"), "远端看不到转发的密钥：{out}");
    conn.disconnect().await;
}
