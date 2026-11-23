//! 集成测试：连接真实的 RDP（xrdp）与 VNC（x11vnc）服务器。CI 的“真实服务集成测试”任务会准备环境变量。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use vornel_remote::rdp::{RdpOptions, ServerKey};
use vornel_remote::vnc::VncOptions;
use vornel_remote::{Event, Input};

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("缺少环境变量 {k}"))
}

async fn wait_frames(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>, min: usize) -> (usize, usize) {
    let mut frames = 0;
    let mut bytes = 0;
    let _ = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(f) = rx.recv().await {
            assert!(f.len() >= 8, "帧缺少头部");
            let w = u16::from_le_bytes([f[4], f[5]]) as usize;
            let h = u16::from_le_bytes([f[6], f[7]]) as usize;
            assert_eq!(f.len(), 8 + w * h * 4, "帧长度与宽高不符");
            frames += 1;
            bytes += f.len();
            if frames >= min {
                break;
            }
        }
    })
    .await;
    (frames, bytes)
}

#[tokio::test]
#[ignore]
async fn rdp_xrdp_connects_and_streams_frames() {
    let seen: Arc<Mutex<Option<ServerKey>>> = Arc::default();
    let seen2 = seen.clone();
    let (ftx, mut frx) = mpsc::unbounded_channel();
    let (etx, mut erx) = mpsc::unbounded_channel();
    let opts = || RdpOptions {
        host: "127.0.0.1".into(),
        port: env("VORNEL_TEST_RDP_PORT").parse().unwrap(),
        username: env("VORNEL_TEST_RDP_USER"),
        password: env("VORNEL_TEST_RDP_PASS"),
        domain: None,
        width: 1024,
        height: 768,
    };
    let session = vornel_remote::rdp::connect(
        opts(),
        move |k| {
            *seen2.lock().unwrap() = Some(k.clone());
            Ok(())
        },
        move |f| {
            let _ = ftx.send(f);
        },
        move |e| {
            let _ = etx.send(e);
        },
    )
    .await
    .expect("RDP 连接");

    let key = seen
        .lock()
        .unwrap()
        .clone()
        .expect("应当拿到服务器证书公钥");
    assert!(key.fingerprint.starts_with("SHA256:"));
    match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await {
        Ok(Some(Event::Resize { width, height })) => assert!(width > 0 && height > 0),
        other => panic!("应当先收到桌面尺寸，实际：{other:?}"),
    }
    let (frames, bytes) = wait_frames(&mut frx, 3).await;
    assert!(frames >= 1, "没有收到任何画面");
    println!("RDP：收到 {frames} 个脏矩形，共 {bytes} 字节");

    // 输入不应导致会话出错
    session.input(Input::Pointer {
        x: 10,
        y: 10,
        button: 0,
        down: false,
        moved: true,
    });
    session.input(Input::Key {
        code: "KeyA".into(),
        key: "a".into(),
        down: true,
    });
    session.input(Input::Key {
        code: "KeyA".into(),
        key: "a".into(),
        down: false,
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(session);

    // 证书指纹不一致时，应在发送凭据前拒绝
    let fp = key.fingerprint.clone();
    let r = vornel_remote::rdp::connect(
        opts(),
        move |k| {
            if k.fingerprint == fp {
                Err("指纹已变化（模拟）".into())
            } else {
                Ok(())
            }
        },
        |_| {},
        |_| {},
    )
    .await;
    assert!(r.is_err(), "拒绝证书后不应连接成功");
}

#[tokio::test]
#[ignore]
async fn vnc_x11vnc_connects_and_streams_frames() {
    let (ftx, mut frx) = mpsc::unbounded_channel();
    let (etx, mut erx) = mpsc::unbounded_channel();
    let session = vornel_remote::vnc::connect(
        VncOptions {
            host: "127.0.0.1".into(),
            port: env("VORNEL_TEST_VNC_PORT").parse().unwrap(),
            password: env("VORNEL_TEST_VNC_PASS"),
        },
        move |f| {
            let _ = ftx.send(f);
        },
        move |e| {
            let _ = etx.send(e);
        },
    )
    .await
    .expect("VNC 连接");
    match tokio::time::timeout(Duration::from_secs(10), erx.recv()).await {
        Ok(Some(Event::Resize { width, height })) => assert!(width > 0 && height > 0),
        other => panic!("应当先收到桌面尺寸，实际：{other:?}"),
    }
    let (frames, bytes) = wait_frames(&mut frx, 1).await;
    assert!(frames >= 1, "没有收到任何画面");
    println!("VNC：收到 {frames} 个脏矩形，共 {bytes} 字节");
    session.input(Input::Pointer {
        x: 5,
        y: 5,
        button: 0,
        down: true,
        moved: false,
    });
    session.input(Input::Pointer {
        x: 5,
        y: 5,
        button: 0,
        down: false,
        moved: false,
    });
    session.input(Input::Key {
        code: "KeyA".into(),
        key: "a".into(),
        down: true,
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[tokio::test]
#[ignore]
async fn vnc_wrong_password_fails() {
    let r = vornel_remote::vnc::connect(
        VncOptions {
            host: "127.0.0.1".into(),
            port: env("VORNEL_TEST_VNC_PORT").parse().unwrap(),
            password: "wrong-pw".into(),
        },
        |_| {},
        |_| {},
    )
    .await;
    assert!(r.is_err());
}
