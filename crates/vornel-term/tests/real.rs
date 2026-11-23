//! 集成测试：真实的 telnetd（busybox）与虚拟串口对（socat）。

use std::io::{Read, Write};
use std::time::Duration;

use tokio::sync::mpsc;

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("缺少环境变量 {k}"))
}

async fn wait_for(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>, needle: &[u8]) -> Vec<u8> {
    let mut all = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(d) = rx.recv().await {
            all.extend(d);
            if all.windows(needle.len()).any(|w| w == needle) {
                break;
            }
        }
    })
    .await;
    all
}

#[tokio::test]
#[ignore]
async fn telnet_real_server_runs_command() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (etx, mut erx) = mpsc::unbounded_channel();
    let s = vornel_term::telnet::connect(
        "127.0.0.1",
        env("VORNEL_TEST_TELNET_PORT").parse().unwrap(),
        100,
        30,
        move |d| {
            let _ = tx.send(d);
        },
        move |r| {
            let _ = etx.send(r);
        },
    )
    .await
    .unwrap();
    // 真实的 telnetd 会要求登录
    if let (Ok(user), Ok(pass)) = (
        std::env::var("VORNEL_TEST_TELNET_USER"),
        std::env::var("VORNEL_TEST_TELNET_PASS"),
    ) {
        let out = wait_for(&mut rx, b"login:").await;
        assert!(
            out.windows(6).any(|w| w == b"login:"),
            "没有登录提示：{}",
            String::from_utf8_lossy(&out)
        );
        s.write(format!("{user}\r").into_bytes());
        let out = wait_for(&mut rx, b"assword:").await;
        assert!(
            out.windows(8).any(|w| w == b"assword:"),
            "没有密码提示：{}",
            String::from_utf8_lossy(&out)
        );
        s.write(format!("{pass}\r").into_bytes());
        tokio::time::sleep(Duration::from_secs(2)).await;
    } else {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    s.write(b"echo tel-$((6*7))\r".to_vec());
    let out = wait_for(&mut rx, b"tel-42").await;
    assert!(
        out.windows(6).any(|w| w == b"tel-42"),
        "输出：{}",
        String::from_utf8_lossy(&out)
    );
    s.write(b"exit\r".to_vec());
    let closed = tokio::time::timeout(Duration::from_secs(10), erx.recv()).await;
    assert!(closed.is_ok(), "退出后连接应关闭");
}

#[tokio::test]
#[ignore]
async fn serial_virtual_port_pair() {
    let a = env("VORNEL_TEST_SERIAL_A");
    let b = env("VORNEL_TEST_SERIAL_B");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let opts = vornel_term::serial::SerialOptions {
        path: a,
        baud_rate: 115_200,
        data_bits: 8,
        parity: "none".into(),
        stop_bits: 1,
        flow_control: "none".into(),
    };
    let session = vornel_term::serial::open(
        &opts,
        move |d| {
            let _ = tx.send(d);
        },
        |_| {},
    )
    .unwrap();

    let mut peer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&b)
        .unwrap();
    // 对端 → Vornel
    peer.write_all(b"ping-from-device").unwrap();
    let got = wait_for(&mut rx, b"ping-from-device").await;
    assert!(got.windows(16).any(|w| w == b"ping-from-device"));

    // Vornel → 对端
    session.write(b"pong".to_vec());
    let mut buf = [0u8; 4];
    let read = tokio::task::spawn_blocking(move || peer.read_exact(&mut buf).map(|_| buf))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&read, b"pong");
}
