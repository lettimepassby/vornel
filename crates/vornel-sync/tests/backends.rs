//! 集成测试：通过真实的 S3（MinIO）与 WebDAV（rclone serve webdav）服务在两台“设备”间同步。

use vornel_core::Snippet;
use vornel_sync::{init_tls, sync, BackendConfig};
use vornel_vault::{MemoryKeyStore, Vault};

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("缺少环境变量 {k}"))
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join(format!("vornel-backend-{}", uuid::Uuid::new_v4()))
        .join(name)
}

fn snippet(t: &str) -> Snippet {
    Snippet {
        id: uuid::Uuid::new_v4(),
        name: t.into(),
        command: t.into(),
        run: false,
    }
}

async fn two_devices(cfg: BackendConfig) {
    init_tls();
    let op = cfg.operator().unwrap();
    let (ka, kb) = (MemoryKeyStore::default(), MemoryKeyStore::default());
    let mut a = Vault::open(&tmp("a.db"), &ka).unwrap();
    let mut b = Vault::open(&tmp("b.db"), &kb).unwrap();
    a.put(&snippet("from-a")).unwrap();
    b.put(&snippet("from-b")).unwrap();

    let r = sync(&op, &mut a, &ka, Some("master-pw-1")).await.unwrap();
    assert!(r.created);
    let r = sync(&op, &mut b, &kb, Some("master-pw-1")).await.unwrap();
    assert!(r.joined);
    sync(&op, &mut a, &ka, None).await.unwrap();
    assert_eq!(a.list::<Snippet>().unwrap().len(), 2);
    assert_eq!(b.list::<Snippet>().unwrap().len(), 2);

    // 云端只有密文
    let manifest = op.read("manifest.bin").await.unwrap().to_vec();
    assert!(!manifest.windows(6).any(|w| w == b"from-a"));
}

#[tokio::test]
#[ignore]
async fn sync_via_minio_s3() {
    two_devices(BackendConfig::S3 {
        endpoint: env("VORNEL_TEST_S3_ENDPOINT"),
        bucket: env("VORNEL_TEST_S3_BUCKET"),
        region: "us-east-1".into(),
        access_key_id: env("VORNEL_TEST_S3_KEY"),
        secret_access_key: env("VORNEL_TEST_S3_SECRET"),
        root: format!("run-{}", uuid::Uuid::new_v4()),
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn sync_via_webdav() {
    two_devices(BackendConfig::Webdav {
        endpoint: env("VORNEL_TEST_DAV_URL"),
        username: env("VORNEL_TEST_DAV_USER"),
        password: env("VORNEL_TEST_DAV_PASS"),
        root: format!("run-{}", uuid::Uuid::new_v4()),
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn s3_probe_and_friendly_errors() {
    init_tls();
    let cfg = |secret: String, bucket: String| BackendConfig::S3 {
        endpoint: env("VORNEL_TEST_S3_ENDPOINT"),
        bucket,
        region: "us-east-1".into(),
        access_key_id: env("VORNEL_TEST_S3_KEY"),
        secret_access_key: secret,
        root: format!("probe-{}", uuid::Uuid::new_v4()),
    };
    // 正确配置：读写删都通过，且不留下测试文件
    let good = cfg(env("VORNEL_TEST_S3_SECRET"), env("VORNEL_TEST_S3_BUCKET"));
    let op = good.operator().unwrap();
    vornel_sync::probe(&op).await.unwrap();
    assert!(!op.exists(".vornel-probe").await.unwrap());

    // 错误的 Secret：提示签名校验失败
    let bad = cfg("wrong-secret-0000".into(), env("VORNEL_TEST_S3_BUCKET"));
    let msg = vornel_sync::probe(&bad.operator().unwrap())
        .await
        .unwrap_err()
        .to_string();
    println!("{msg}");
    assert!(msg.contains("Secret Access Key"), "{msg}");

    // 不存在的存储桶
    let nb = cfg(env("VORNEL_TEST_S3_SECRET"), "no-such-bucket-vornel".into());
    let msg = vornel_sync::probe(&nb.operator().unwrap())
        .await
        .unwrap_err()
        .to_string();
    println!("{msg}");
    assert!(msg.contains("存储桶不存在"), "{msg}");
}
