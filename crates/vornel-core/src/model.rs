use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Record;

// ---------------------------------------------------------------- 主机

/// 一台已保存的远端主机。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: Uuid,
    /// 显示名称，例如 `prod-web-01`。
    pub name: String,
    /// 主机名或 IP；串口协议下是设备路径（如 `/dev/tty.usbserial`、`COM3`）。
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    /// 分组名；为空表示未分组。
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub protocol: Protocol,
    #[serde(default)]
    pub auth: AuthKind,
    /// 跳板机（另一台已保存主机的 id）。
    #[serde(default)]
    pub jump_host: Option<Uuid>,
    /// 是否转发本机 SSH Agent。
    #[serde(default)]
    pub agent_forward: bool,
    /// 是否在 Vault 中保存密码（密码本身存在 [`Secret`] 记录里）。
    #[serde(default)]
    pub save_password: bool,
    /// 串口参数（仅串口协议）。
    #[serde(default)]
    pub serial: Option<SerialConfig>,
    /// RDP 域（可选）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 远端操作系统（`ubuntu`、`debian`、`centos`、`macos`…），连接成功后自动识别，用于主机列表的图标。
    #[serde(default)]
    pub os: Option<String>,
    /// 最近一次连接成功的时间（Unix 毫秒），用于主机列表排序。
    #[serde(default)]
    pub last_connected: Option<u64>,
}

/// 从 `/etc/os-release` 与 `uname -s` 的输出中识别系统，返回小写的标识。
pub fn parse_os(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("ID=") {
            let id = v.trim().trim_matches(['"', '\'']).to_lowercase();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    let uname = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.contains('='))?;
    Some(
        match uname.to_lowercase().as_str() {
            "darwin" => "macos",
            "linux" => "linux",
            other => return Some(other.to_string()),
        }
        .to_string(),
    )
}

fn default_port() -> u16 {
    22
}

impl Record for Host {
    const KIND: &'static str = "host";
    fn id(&self) -> Uuid {
        self.id
    }
}

/// 连接协议。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Ssh,
    Mosh,
    Telnet,
    Serial,
    Rdp,
    Vnc,
}

impl Protocol {
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Ssh | Protocol::Mosh => 22,
            Protocol::Telnet => 23,
            Protocol::Serial => 0,
            Protocol::Rdp => 3389,
            Protocol::Vnc => 5900,
        }
    }
}

/// SSH 认证方式。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AuthKind {
    /// 密码（可选择保存到 Vault）。
    #[default]
    Password,
    /// 密钥串里的一把密钥。
    Key { key_id: Uuid },
    /// 本机磁盘上的私钥文件。
    KeyFile { path: String },
    /// 系统 SSH Agent（ssh-agent、Windows OpenSSH Agent、1Password 等）。
    Agent,
}

/// 串口参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialConfig {
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    /// `none` / `odd` / `even`
    #[serde(default = "default_parity")]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    /// `none` / `software` / `hardware`
    #[serde(default = "default_flow")]
    pub flow_control: String,
}

fn default_data_bits() -> u8 {
    8
}
fn default_parity() -> String {
    "none".into()
}
fn default_stop_bits() -> u8 {
    1
}
fn default_flow() -> String {
    "none".into()
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 115_200,
            data_bits: 8,
            parity: default_parity(),
            stop_bits: 1,
            flow_control: default_flow(),
        }
    }
}

impl Host {
    /// 校验用户输入，返回第一条错误信息。
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("名称不能为空".into());
        }
        if self.address.trim().is_empty() {
            return Err(if self.protocol == Protocol::Serial {
                "请填写串口设备".into()
            } else {
                "地址不能为空".into()
            });
        }
        let needs_user = matches!(self.protocol, Protocol::Ssh | Protocol::Mosh);
        if needs_user && self.username.trim().is_empty() {
            return Err("用户名不能为空".into());
        }
        if self.protocol != Protocol::Serial && self.port == 0 {
            return Err("端口必须在 1–65535 之间".into());
        }
        if let AuthKind::KeyFile { path } = &self.auth {
            if path.trim().is_empty() {
                return Err("请填写私钥文件路径".into());
            }
        }
        if self.jump_host == Some(self.id) {
            return Err("跳板机不能是自己".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- 密钥串

/// 密钥串中的一把 SSH 密钥。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub id: Uuid,
    pub name: String,
    /// 例如 `ssh-ed25519`
    pub algorithm: String,
    /// OpenSSH 公钥（一行）
    pub public_key: String,
    /// OpenSSH 私钥（PEM）；可能本身带口令加密
    pub private_key: String,
    /// 私钥是否带口令
    #[serde(default)]
    pub encrypted: bool,
    /// SHA256 指纹
    pub fingerprint: String,
    /// 是否参与云同步（默认否，见 ARCHITECTURE.md 3.6）
    #[serde(default)]
    pub sync: bool,
}

impl Record for Key {
    const KIND: &'static str = "key";
    fn id(&self) -> Uuid {
        self.id
    }
    fn local_only(&self) -> bool {
        !self.sync
    }
}

// ---------------------------------------------------------------- 保存的密码

/// 保存的主机密码，id 与主机相同。是否同步跟随设置“同步密码”。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Secret {
    pub id: Uuid,
    pub password: String,
    #[serde(default)]
    pub sync: bool,
}

impl Record for Secret {
    const KIND: &'static str = "secret";
    fn id(&self) -> Uuid {
        self.id
    }
    fn local_only(&self) -> bool {
        !self.sync
    }
}

// ---------------------------------------------------------------- 代码片段

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    /// 发送后是否自动回车执行
    #[serde(default)]
    pub run: bool,
}

impl Record for Snippet {
    const KIND: &'static str = "snippet";
    fn id(&self) -> Uuid {
        self.id
    }
}

// ---------------------------------------------------------------- 端口转发

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    /// 本地端口 → 远端目标（ssh -L）
    Local,
    /// 远端端口 → 本地目标（ssh -R）
    Remote,
    /// 本地 SOCKS5 代理（ssh -D）
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forward {
    pub id: Uuid,
    pub name: String,
    pub host_id: Uuid,
    pub kind: ForwardKind,
    #[serde(default = "default_bind")]
    pub bind_address: String,
    pub bind_port: u16,
    /// 目标地址（动态转发不需要）
    #[serde(default)]
    pub target_host: String,
    #[serde(default)]
    pub target_port: u16,
}

fn default_bind() -> String {
    "127.0.0.1".into()
}

impl Record for Forward {
    const KIND: &'static str = "forward";
    fn id(&self) -> Uuid {
        self.id
    }
}

impl Forward {
    pub fn validate(&self) -> Result<(), String> {
        if self.bind_port == 0 {
            return Err("请填写监听端口".into());
        }
        if self.kind != ForwardKind::Dynamic
            && (self.target_host.trim().is_empty() || self.target_port == 0)
        {
            return Err("请填写目标地址和端口".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- 已知主机

/// 已信任的主机公钥（替代 ~/.ssh/known_hosts，并参与同步）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub id: Uuid,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    /// OpenSSH 公钥（一行）
    pub public_key: String,
    /// 首次信任时间（Unix 秒）
    pub added_at: u64,
}

impl Record for KnownHost {
    const KIND: &'static str = "known_host";
    fn id(&self) -> Uuid {
        self.id
    }
}

// ---------------------------------------------------------------- 偏好设置

/// 跨设备同步的偏好设置（字体包、主题等），全局只有一条，id 固定为 nil。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    pub id: Uuid,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_pack")]
    pub font_pack: String,
    #[serde(default = "default_font_size")]
    pub terminal_font_size: u8,
    /// 单独覆盖字体包里的某一项：`ui` / `mono` / `terminal` / `cjk` → 字体名
    #[serde(default)]
    pub font_overrides: std::collections::BTreeMap<String, String>,
    /// 是否同步保存的密码
    #[serde(default)]
    pub sync_secrets: bool,
    /// 终端屏幕阅读器模式（xterm.js screenReaderMode）
    #[serde(default)]
    pub screen_reader: bool,
    /// 终端里选中文字后自动复制到剪贴板
    #[serde(default = "default_true")]
    pub copy_on_select: bool,
    /// 分组外观：分组名 → 图标 / 底色 / 自定义图片
    #[serde(default)]
    pub group_styles: std::collections::BTreeMap<String, GroupStyle>,
    /// 终端配色方案：`auto` 跟随界面主题，或 `dracula`、`nord` 等
    #[serde(default = "default_auto")]
    pub terminal_theme: String,
    /// 终端行高；None 表示使用字体包的默认值
    #[serde(default)]
    pub terminal_line_height: Option<f32>,
    /// 光标样式：`block` / `bar` / `underline`
    #[serde(default = "default_cursor")]
    pub cursor_style: String,
    #[serde(default = "default_true")]
    pub cursor_blink: bool,
}

fn default_auto() -> String {
    "auto".into()
}
fn default_cursor() -> String {
    "block".into()
}

/// 分组卡片的外观。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupStyle {
    /// 预设图标：`os:ubuntu` 这类系统 logo，或 `icon:cloud` 这类通用图标
    #[serde(default)]
    pub icon: Option<String>,
    /// 底色（CSS 颜色）
    #[serde(default)]
    pub color: Option<String>,
    /// 自定义图片（缩小后的 PNG data URL，不超过 64 KB）
    #[serde(default)]
    pub image: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "ink".into()
}
fn default_pack() -> String {
    "modern".into()
}
fn default_font_size() -> u8 {
    13
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            theme: default_theme(),
            font_pack: default_pack(),
            terminal_font_size: default_font_size(),
            font_overrides: Default::default(),
            sync_secrets: false,
            screen_reader: false,
            copy_on_select: true,
            group_styles: Default::default(),
            terminal_theme: default_auto(),
            terminal_line_height: None,
            cursor_style: default_cursor(),
            cursor_blink: true,
        }
    }
}

impl Record for Prefs {
    const KIND: &'static str = "prefs";
    fn id(&self) -> Uuid {
        Uuid::nil()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_os() {
        assert_eq!(
            parse_os("NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\nLinux\n").as_deref(),
            Some("ubuntu")
        );
        assert_eq!(parse_os("ID=\"rocky\"\n").as_deref(), Some("rocky"));
        assert_eq!(parse_os("Darwin\n").as_deref(), Some("macos"));
        assert_eq!(parse_os("FreeBSD\n").as_deref(), Some("freebsd"));
        assert_eq!(parse_os(""), None);
    }

    fn host() -> Host {
        serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000001","name":"a","address":"b","username":"c"}"#,
        )
        .unwrap()
    }

    #[test]
    fn host_defaults() {
        let h = host();
        assert_eq!(h.port, 22);
        assert_eq!(h.auth, AuthKind::Password);
        assert_eq!(h.protocol, Protocol::Ssh);
        assert!(h.validate().is_ok());
    }

    #[test]
    fn auth_json_shape() {
        let v = serde_json::to_value(AuthKind::Key {
            key_id: Uuid::nil(),
        })
        .unwrap();
        assert_eq!(v["type"], "key");
        assert!(v.get("keyId").is_some());
    }

    #[test]
    fn validate_rejects_self_jump() {
        let mut h = host();
        h.jump_host = Some(h.id);
        assert!(h.validate().is_err());
    }

    #[test]
    fn telnet_does_not_need_user() {
        let mut h = host();
        h.protocol = Protocol::Telnet;
        h.username.clear();
        assert!(h.validate().is_ok());
    }

    #[test]
    fn key_is_local_by_default() {
        let k: Key = serde_json::from_str(
            r#"{"id":"00000000-0000-0000-0000-000000000002","name":"k","algorithm":"ssh-ed25519","publicKey":"p","privateKey":"s","fingerprint":"f"}"#,
        )
        .unwrap();
        assert!(k.local_only());
    }
}
