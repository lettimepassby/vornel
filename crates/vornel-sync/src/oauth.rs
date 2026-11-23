//! 桌面 / 移动端的 OAuth 2.0 授权（PKCE + 本机回环地址），用于 Google Drive、OneDrive、Dropbox。
//!
//! 按 ARCHITECTURE.md 3.5：只申请窄权限（Google `drive.file`、OneDrive App Folder），
//! Client ID 由用户在各平台自行注册后填入。

use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::{BackendConfig, Result, SyncError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Google,
    Microsoft,
    Dropbox,
}

struct Endpoints {
    auth: &'static str,
    token: &'static str,
    scope: &'static str,
    extra: &'static [(&'static str, &'static str)],
}

impl Provider {
    fn endpoints(self) -> Endpoints {
        match self {
            Provider::Google => Endpoints {
                auth: "https://accounts.google.com/o/oauth2/v2/auth",
                token: "https://oauth2.googleapis.com/token",
                scope: "https://www.googleapis.com/auth/drive.file",
                extra: &[("access_type", "offline"), ("prompt", "consent")],
            },
            Provider::Microsoft => Endpoints {
                auth: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
                token: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
                scope: "offline_access Files.ReadWrite.AppFolder",
                extra: &[],
            },
            Provider::Dropbox => Endpoints {
                auth: "https://www.dropbox.com/oauth2/authorize",
                token: "https://api.dropboxapi.com/oauth2/token",
                scope: "files.content.read files.content.write",
                extra: &[("token_access_type", "offline")],
            },
        }
    }
}

fn random_token(len: usize) -> Result<String> {
    let mut buf = vec![0u8; len];
    getrandom::fill(&mut buf).map_err(|e| SyncError::OAuth(e.to_string()))?;
    Ok(B64URL.encode(buf))
}

#[derive(Deserialize)]
struct TokenResponse {
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// 完成一次授权：打开浏览器 → 用户同意 → 浏览器跳回本机端口 → 用授权码换取 refresh token。
///
/// `open_url` 由调用方实现（Tauri 里用系统浏览器打开）。
pub async fn authorize(
    provider: Provider,
    client_id: &str,
    client_secret: &str,
    open_url: impl FnOnce(String),
) -> Result<BackendConfig> {
    if client_id.trim().is_empty() {
        return Err(SyncError::Config("请填写 Client ID".into()));
    }
    let ep = provider.endpoints();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| SyncError::OAuth(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| SyncError::OAuth(e.to_string()))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}/callback");

    let verifier = random_token(48)?;
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_token(16)?;

    let mut url = Url::parse(ep.auth).map_err(|e| SyncError::OAuth(e.to_string()))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", client_id.trim())
            .append_pair("redirect_uri", &redirect)
            .append_pair("response_type", "code")
            .append_pair("scope", ep.scope)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        for (k, v) in ep.extra {
            q.append_pair(k, v);
        }
    }
    open_url(url.to_string());

    let code = tokio::time::timeout(Duration::from_secs(300), wait_for_code(&listener, &state))
        .await
        .map_err(|_| SyncError::OAuth("等待授权超时".into()))??;

    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", client_id.trim().to_string()),
        ("code_verifier", verifier),
    ];
    if !client_secret.trim().is_empty() {
        form.push(("client_secret", client_secret.trim().to_string()));
    }
    let resp: TokenResponse = reqwest::Client::new()
        .post(ep.token)
        .form(&form)
        .send()
        .await
        .map_err(|e| SyncError::OAuth(e.to_string()))?
        .json()
        .await
        .map_err(|e| SyncError::OAuth(e.to_string()))?;
    if let Some(err) = resp.error {
        return Err(SyncError::OAuth(resp.error_description.unwrap_or(err)));
    }
    let refresh_token = resp
        .refresh_token
        .ok_or_else(|| SyncError::OAuth("服务没有返回 refresh token".into()))?;

    let client_id = client_id.trim().to_string();
    let client_secret = client_secret.trim().to_string();
    Ok(match provider {
        Provider::Google => BackendConfig::Gdrive {
            client_id,
            client_secret,
            refresh_token,
        },
        Provider::Microsoft => BackendConfig::OneDrive {
            client_id,
            client_secret,
            refresh_token,
        },
        Provider::Dropbox => BackendConfig::Dropbox {
            client_id,
            client_secret,
            refresh_token,
        },
    })
}

async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| SyncError::OAuth(e.to_string()))?;
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("/");
        let parsed = Url::parse(&format!("http://127.0.0.1{path}")).ok();
        let get = |k: &str| {
            parsed.as_ref().and_then(|u| {
                u.query_pairs()
                    .find(|(name, _)| name == k)
                    .map(|(_, v)| v.into_owned())
            })
        };

        let (ok, message) = match (get("code"), get("state"), get("error")) {
            (_, _, Some(err)) => (Err(SyncError::OAuth(err)), "授权被拒绝，可以关闭此页面。"),
            (Some(code), Some(s), _) if s == state => {
                (Ok(code), "授权完成，可以关闭此页面并回到 Vornel。")
            }
            (Some(_), _, _) => (
                Err(SyncError::OAuth("state 校验失败".into())),
                "授权校验失败，请回到 Vornel 重试。",
            ),
            // 浏览器可能先请求 favicon 等，忽略继续等
            _ => {
                let _ = stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await;
                continue;
            }
        };
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>Vornel</title><body style=\"font:16px system-ui;padding:48px;text-align:center\">{message}</body>"
        );
        let _ = stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            )
            .await;
        return ok;
    }
}
