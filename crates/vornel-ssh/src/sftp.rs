//! SFTP：目录浏览、上传、下载、重命名、删除、新建目录。

use std::path::Path;

use russh_sftp::client::SftpSession;
use serde::Serialize;
use tokio::fs::File as LocalFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::client::Connection;
use crate::SshError;

/// 目录里的一项（本地与远端共用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    /// Unix 秒
    pub modified: Option<u64>,
    /// 权限位，例如 0o644
    pub mode: Option<u32>,
}

pub struct Sftp {
    session: SftpSession,
}

const CHUNK: usize = 64 * 1024;

impl Connection {
    pub async fn open_sftp(&self) -> Result<Sftp, SshError> {
        let channel = self.open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        let session = SftpSession::new(channel.into_stream()).await?;
        Ok(Sftp { session })
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

impl Sftp {
    /// 解析为绝对路径（`.` 即家目录）。
    pub async fn canonicalize(&self, path: &str) -> Result<String, SshError> {
        Ok(self.session.canonicalize(path).await?)
    }

    pub async fn list(&self, path: &str) -> Result<Vec<FileEntry>, SshError> {
        let mut out: Vec<FileEntry> = self
            .session
            .read_dir(path)
            .await?
            .filter(|e| e.file_name() != "." && e.file_name() != "..")
            .map(|e| {
                let m = e.metadata();
                let ft = e.file_type();
                FileEntry {
                    name: e.file_name(),
                    is_dir: ft.is_dir(),
                    is_link: ft.is_symlink(),
                    size: m.size.unwrap_or(0),
                    modified: m.mtime.map(u64::from),
                    mode: m.permissions.map(|p| p & 0o7777),
                }
            })
            .collect();
        sort_entries(&mut out);
        Ok(out)
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), SshError> {
        Ok(self.session.create_dir(path).await?)
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), SshError> {
        Ok(self.session.rename(from, to).await?)
    }

    /// 删除文件或目录（目录递归删除）。
    pub async fn remove(&self, path: &str) -> Result<(), SshError> {
        let meta = self.session.symlink_metadata(path).await?;
        if meta.file_type().is_dir() {
            for e in self.list(path).await? {
                Box::pin(self.remove(&join(path, &e.name))).await?;
            }
            self.session.remove_dir(path).await?;
        } else {
            self.session.remove_file(path).await?;
        }
        Ok(())
    }

    /// 下载一个文件；`progress(已传输字节, 总字节)`。
    pub async fn download(
        &self,
        remote: &str,
        local: &Path,
        mut progress: impl FnMut(u64, u64) + Send,
    ) -> Result<(), SshError> {
        let total = self.session.metadata(remote).await?.size.unwrap_or(0);
        let mut src = self.session.open(remote).await?;
        let mut dst = LocalFile::create(local).await?;
        let mut buf = vec![0u8; CHUNK];
        let mut done = 0u64;
        loop {
            let n = src.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).await?;
            done += n as u64;
            progress(done, total);
        }
        dst.flush().await?;
        Ok(())
    }

    /// 上传一个文件；`progress(已传输字节, 总字节)`。
    pub async fn upload(
        &self,
        local: &Path,
        remote: &str,
        mut progress: impl FnMut(u64, u64) + Send,
    ) -> Result<(), SshError> {
        let mut src = LocalFile::open(local).await?;
        let total = src.metadata().await?.len();
        let mut dst = self.session.create(remote).await?;
        let mut buf = vec![0u8; CHUNK];
        let mut done = 0u64;
        loop {
            let n = src.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n]).await?;
            done += n as u64;
            progress(done, total);
        }
        dst.shutdown().await?;
        Ok(())
    }

    pub async fn close(&self) {
        let _ = self.session.close().await;
    }
}

pub fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// 列出本机目录（SFTP 双栏的左栏）。
pub async fn list_local(path: &Path) -> Result<Vec<FileEntry>, SshError> {
    let mut rd = tokio::fs::read_dir(path).await?;
    let mut out = Vec::new();
    while let Some(e) = rd.next_entry().await? {
        let meta = match e.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let ft = e.file_type().await?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            Some(meta.permissions().mode() & 0o7777)
        };
        #[cfg(not(unix))]
        let mode = None;
        out.push(FileEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            is_link: ft.is_symlink(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            mode,
        });
    }
    sort_entries(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_paths() {
        assert_eq!(join("/var", "log"), "/var/log");
        assert_eq!(join("/", "etc"), "/etc");
    }

    #[test]
    fn dirs_first_then_name() {
        let mk = |name: &str, is_dir| FileEntry {
            name: name.into(),
            is_dir,
            is_link: false,
            size: 0,
            modified: None,
            mode: None,
        };
        let mut v = vec![
            mk("b.txt", false),
            mk("Z", true),
            mk("a.txt", false),
            mk("a", true),
        ];
        sort_entries(&mut v);
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "Z", "a.txt", "b.txt"]);
    }

    #[tokio::test]
    async fn list_local_dir() {
        let dir = std::env::temp_dir().join(format!("vornel-sftp-{}", std::process::id()));
        tokio::fs::create_dir_all(dir.join("sub")).await.unwrap();
        tokio::fs::write(dir.join("f.txt"), b"hello").await.unwrap();
        let v = list_local(&dir).await.unwrap();
        assert_eq!(v[0].name, "sub");
        assert!(v[0].is_dir);
        assert_eq!(v[1].size, 5);
        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
