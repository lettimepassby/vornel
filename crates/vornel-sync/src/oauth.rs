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

/// OAuth 服务端点（测试时可指向本机模拟服务）。
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub scope: String,
    pub extra: Vec<(String, String)>,
}

impl Provider {
    pub fn endpoints(self) -> Endpoints {
        match self {
            Provider::Google => Endpoints {
                auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
                token: "https://oauth2.googleapis.com/token".into(),
                scope: "https://www.googleapis.com/auth/drive.file".into(),
                extra: vec![
                    ("access_type".into(), "offline".into()),
                    ("prompt".into(), "consent".into()),
                ],
            },
            Provider::Microsoft => Endpoints {
                auth: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
                token: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                scope: "offline_access Files.ReadWrite.AppFolder".into(),
                extra: vec![],
            },
            Provider::Dropbox => Endpoints {
                auth: "https://www.dropbox.com/oauth2/authorize".into(),
                token: "https://api.dropboxapi.com/oauth2/token".into(),
                scope: "files.content.read files.content.write".into(),
                extra: vec![("token_access_type".into(), "offline".into())],
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
    authorize_at(
        provider,
        provider.endpoints(),
        client_id,
        client_secret,
        open_url,
    )
    .await
}

/// 与 [`authorize`] 相同，但使用指定的端点。
pub async fn authorize_at(
    provider: Provider,
    ep: Endpoints,
    client_id: &str,
    client_secret: &str,
    open_url: impl FnOnce(String),
) -> Result<BackendConfig> {
    if client_id.trim().is_empty() {
        return Err(SyncError::Config("请填写 Client ID".into()));
    }
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

    let mut url = Url::parse(&ep.auth).map_err(|e| SyncError::OAuth(e.to_string()))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", client_id.trim())
            .append_pair("redirect_uri", &redirect)
            .append_pair("response_type", "code")
            .append_pair("scope", &ep.scope)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        for (k, v) in &ep.extra {
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
        .post(&ep.token)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 完整走一遍：生成授权链接 → 模拟浏览器回调本机端口 → 用授权码换取 refresh token（模拟令牌服务）。
    #[tokio::test]
    async fn loopback_pkce_flow_against_mock_server() {
        crate::init_tls();
        // 模拟令牌服务：校验 PKCE（sha256(verifier) == challenge）后返回 refresh token
        let token_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let token_port = token_listener.local_addr().unwrap().port();
        let challenge_slot = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let challenge_for_server = challenge_slot.clone();
        tokio::spawn(async move {
            let (mut s, _) = token_listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let mut req = Vec::new();
            loop {
                let n = s.read(&mut buf).await.unwrap();
                req.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&req);
                if let Some(i) = text.find("\r\n\r\n") {
                    let len: usize = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if req.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&req).into_owned();
            let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
            let form: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(body.as_bytes())
                    .into_owned()
                    .collect();
            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
            let ok = form.get("code").map(String::as_str) == Some("the-code")
                && B64URL.encode(Sha256::digest(verifier.as_bytes()))
                    == *challenge_for_server.lock().unwrap()
                && form.get("client_id").map(String::as_str) == Some("cid");
            let json = if ok {
                r#"{"refresh_token":"rt-123"}"#
            } else {
                r#"{"error":"invalid_grant"}"#
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            );
            s.write_all(resp.as_bytes()).await.unwrap();
        });

        let ep = Endpoints {
            auth: "https://auth.example/authorize".into(),
            token: format!("http://127.0.0.1:{token_port}/token"),
            scope: "files".into(),
            extra: vec![],
        };
        let challenge_for_browser = challenge_slot.clone();
        let cfg = authorize_at(Provider::Google, ep, "cid", "", move |auth_url| {
            // 模拟浏览器：用户同意后跳回 redirect_uri
            let u = Url::parse(&auth_url).unwrap();
            let get = |k: &str| {
                u.query_pairs()
                    .find(|(n, _)| n == k)
                    .unwrap()
                    .1
                    .into_owned()
            };
            assert_eq!(get("code_challenge_method"), "S256");
            *challenge_for_browser.lock().unwrap() = get("code_challenge");
            let redirect = format!(
                "{}?code=the-code&state={}",
                get("redirect_uri"),
                get("state")
            );
            tokio::spawn(async move {
                let u = Url::parse(&redirect).unwrap();
                let mut s =
                    tokio::net::TcpStream::connect((u.host_str().unwrap(), u.port().unwrap()))
                        .await
                        .unwrap();
                let req = format!(
                    "GET {}?{} HTTP/1.1\r\nHost: x\r\n\r\n",
                    u.path(),
                    u.query().unwrap()
                );
                s.write_all(req.as_bytes()).await.unwrap();
                let mut resp = Vec::new();
                let _ = s.read_to_end(&mut resp).await;
                assert!(String::from_utf8_lossy(&resp).contains("授权完成"));
            });
        })
        .await
        .unwrap();
        match cfg {
            BackendConfig::Gdrive {
                refresh_token,
                client_id,
                ..
            } => {
                assert_eq!(refresh_token, "rt-123");
                assert_eq!(client_id, "cid");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn wrong_state_is_rejected() {
        let ep = Endpoints {
            auth: "https://auth.example/authorize".into(),
            token: "http://127.0.0.1:9/token".into(),
            scope: "files".into(),
            extra: vec![],
        };
        let r = authorize_at(Provider::Dropbox, ep, "cid", "", |auth_url| {
            let u = Url::parse(&auth_url).unwrap();
            let redirect = u
                .query_pairs()
                .find(|(n, _)| n == "redirect_uri")
                .unwrap()
                .1
                .into_owned();
            tokio::spawn(async move {
                let u = Url::parse(&redirect).unwrap();
                let mut s =
                    tokio::net::TcpStream::connect((u.host_str().unwrap(), u.port().unwrap()))
                        .await
                        .unwrap();
                s.write_all(b"GET /callback?code=x&state=forged HTTP/1.1\r\n\r\n")
                    .await
                    .unwrap();
            });
        })
        .await;
        assert!(matches!(r, Err(SyncError::OAuth(_))));
    }
}
