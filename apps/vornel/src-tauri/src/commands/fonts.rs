//! 中文字体增强包（ARCHITECTURE.md 第 8 节）：按需下载、sha256 校验、通过 `vornel-font://` 提供给界面。

use std::borrow::Cow;
use std::path::PathBuf;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::http::{Request, Response, StatusCode};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext};

use super::session::Progress;
use super::{err, CmdResult};

const BASE: &str = "https://github.com/lettimepassby/vornel/releases/download/fonts-v1/";

pub struct AddonFile {
    pub name: &'static str,
    pub weight: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub struct Addon {
    pub id: &'static str,
    pub family: &'static str,
    /// `terminal`：终端字体；`cjk`：界面中文字体
    pub role: &'static str,
    pub files: &'static [AddonFile],
}

/// 校验值由 .github/workflows/fonts.yml 生成（见 fonts-v1 Release 的 manifest.json）。
pub const ADDONS: &[Addon] = &[
    Addon {
        id: "maple-mono-cn",
        family: "Maple Mono CN",
        role: "terminal",
        files: &[
            AddonFile {
                name: "MapleMono-CN-Regular.woff2",
                weight: "400",
                sha256: "MAPLE_REGULAR_SHA256",
                size: 0,
            },
            AddonFile {
                name: "MapleMono-CN-Bold.woff2",
                weight: "700",
                sha256: "MAPLE_BOLD_SHA256",
                size: 0,
            },
        ],
    },
    Addon {
        id: "noto-sans-sc",
        family: "Noto Sans SC",
        role: "cjk",
        files: &[AddonFile {
            name: "NotoSansSC[wght].woff2",
            weight: "100 900",
            sha256: "NOTO_SHA256",
            size: 0,
        }],
    },
    Addon {
        id: "lxgw-wenkai-screen",
        family: "LXGW WenKai Screen",
        role: "cjk",
        files: &[AddonFile {
            name: "LXGWWenKaiScreen.woff2",
            weight: "400",
            sha256: "LXGW_SHA256",
            size: 0,
        }],
    },
];

fn fonts_dir<R: Runtime>(app: &AppHandle<R>) -> CmdResult<PathBuf> {
    Ok(app.path().app_data_dir().map_err(err)?.join("fonts"))
}

fn installed(dir: &std::path::Path, addon: &Addon) -> bool {
    addon
        .files
        .iter()
        .all(|f| dir.join(addon.id).join(f.name).is_file())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddonFileView {
    pub name: String,
    pub weight: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddonView {
    pub id: String,
    pub family: String,
    pub role: String,
    pub size: u64,
    pub installed: bool,
    pub files: Vec<AddonFileView>,
}

#[tauri::command]
pub async fn font_addons(app: AppHandle) -> CmdResult<Vec<AddonView>> {
    let dir = fonts_dir(&app)?;
    Ok(ADDONS
        .iter()
        .map(|a| AddonView {
            id: a.id.into(),
            family: a.family.into(),
            role: a.role.into(),
            size: a.files.iter().map(|f| f.size).sum(),
            installed: installed(&dir, a),
            files: a
                .files
                .iter()
                .map(|f| AddonFileView {
                    name: f.name.into(),
                    weight: f.weight.into(),
                })
                .collect(),
        })
        .collect())
}

/// 下载一个增强包；每个文件下载后校验 sha256，不一致则删除并报错。
#[tauri::command]
pub async fn font_addon_install(
    app: AppHandle,
    id: String,
    on_progress: Channel<Progress>,
) -> CmdResult<()> {
    let addon = ADDONS
        .iter()
        .find(|a| a.id == id)
        .ok_or("未知的字体增强包")?;
    let dir = fonts_dir(&app)?.join(addon.id);
    tokio::fs::create_dir_all(&dir).await.map_err(err)?;

    let total: u64 = addon.files.iter().map(|f| f.size).sum();
    let mut done = 0u64;
    let client = reqwest::Client::new();
    for f in addon.files {
        let url = format!("{BASE}{}", f.name.replace('[', "%5B").replace(']', "%5D"));
        let mut resp = client
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("下载失败：{e}"))?;
        let mut hasher = Sha256::new();
        let mut bytes = Vec::with_capacity(f.size as usize);
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("下载失败：{e}"))? {
            hasher.update(&chunk);
            bytes.extend_from_slice(&chunk);
            done += chunk.len() as u64;
            let _ = on_progress.send(Progress {
                done,
                total: total.max(done),
            });
        }
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if digest != f.sha256 {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(format!("{} 校验失败，文件可能被篡改，已删除", f.name));
        }
        tokio::fs::write(dir.join(f.name), bytes)
            .await
            .map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn font_addon_remove(app: AppHandle, id: String) -> CmdResult<()> {
    let addon = ADDONS
        .iter()
        .find(|a| a.id == id)
        .ok_or("未知的字体增强包")?;
    let dir = fonts_dir(&app)?.join(addon.id);
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(err(e)),
    }
}

/// `vornel-font://localhost/<id>/<file>`：只允许读取已登记增强包中的文件。
pub fn protocol<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let path = percent_decode(request.uri().path().trim_start_matches('/'));
    let mut parts = path.splitn(2, '/');
    let (id, name) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let allowed = ADDONS
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.files.iter().find(|f| f.name == name));
    let body = match (allowed, fonts_dir(ctx.app_handle())) {
        (Some(f), Ok(dir)) => std::fs::read(dir.join(id).join(f.name)).ok(),
        _ => None,
    };
    match body {
        Some(bytes) => Response::builder()
            .header("Content-Type", "font/woff2")
            .header("Access-Control-Allow-Origin", "*")
            .body(Cow::Owned(bytes))
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[]))),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Cow::Borrowed(&[][..]))
            .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[]))),
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_brackets() {
        assert_eq!(percent_decode("a%5Bwght%5D.woff2"), "a[wght].woff2");
    }

    #[test]
    fn hashes_are_filled() {
        for a in ADDONS {
            for f in a.files {
                assert_eq!(f.sha256.len(), 64, "{} 的校验值未填写", f.name);
                assert!(f.size > 0);
            }
        }
    }
}
