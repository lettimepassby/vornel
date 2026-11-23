use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 一台已保存的远端主机。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: Uuid,
    /// 显示名称，例如 `prod-web-01`。
    pub name: String,
    /// 主机名或 IP。
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    /// 分组名，例如“生产环境”；为空表示未分组。
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub auth: AuthKind,
}

fn default_port() -> u16 {
    22
}

/// 连接协议。目前只实现了 SSH，其余为后续里程碑预留。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Ssh,
    Sftp,
    Rdp,
    Telnet,
}

/// 认证方式。密码不落盘：每次连接时由用户输入（加密 Vault 在 M3 实现）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthKind {
    #[default]
    Password,
    KeyFile {
        path: String,
    },
}

impl Host {
    /// 校验用户输入，返回第一条错误信息。
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("名称不能为空".into());
        }
        if self.address.trim().is_empty() {
            return Err("地址不能为空".into());
        }
        if self.username.trim().is_empty() {
            return Err("用户名不能为空".into());
        }
        if self.port == 0 {
            return Err("端口必须在 1–65535 之间".into());
        }
        if let AuthKind::KeyFile { path } = &self.auth {
            if path.trim().is_empty() {
                return Err("请填写私钥文件路径".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_matches_frontend() {
        let host = Host {
            id: Uuid::nil(),
            name: "web".into(),
            address: "10.0.0.1".into(),
            port: 22,
            username: "deploy".into(),
            group: None,
            protocol: Protocol::Ssh,
            auth: AuthKind::KeyFile {
                path: "~/.ssh/id_ed25519".into(),
            },
        };
        let v = serde_json::to_value(&host).unwrap();
        assert_eq!(v["auth"]["type"], "keyFile");
        assert_eq!(v["auth"]["path"], "~/.ssh/id_ed25519");
        assert_eq!(v["protocol"], "ssh");
        assert!(v.get("username").is_some());
    }

    #[test]
    fn missing_optional_fields_use_defaults() {
        let host: Host = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000000","name":"a","address":"b","username":"c"}"#,
        )
        .unwrap();
        assert_eq!(host.port, 22);
        assert_eq!(host.auth, AuthKind::Password);
        assert_eq!(host.protocol, Protocol::Ssh);
    }

    #[test]
    fn validate_rejects_empty_fields() {
        let mut host: Host = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000000","name":"a","address":"b","username":"c"}"#,
        )
        .unwrap();
        assert!(host.validate().is_ok());
        host.address = " ".into();
        assert!(host.validate().is_err());
    }
}
