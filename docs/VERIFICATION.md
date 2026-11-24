# 功能验收清单

逐项对照 [ARCHITECTURE.md](../ARCHITECTURE.md) 与 [DESIGN.md](../DESIGN.md)，列出每项功能由哪个自动化测试验证。所有测试都在 GitHub Actions 上运行（本地不编译）。

| 工作流 | 内容 |
|---|---|
| `ci.yml` → 前端检查 | `svelte-check` 类型检查 + 前端构建 |
| `ci.yml` → Rust 库（测试） | 全部单元测试；真实 `mosh-server`；真实 `sshd`（`crates/vornel-ssh/tests/sshd.rs`） |
| `ci.yml` → Tauri 应用（检查） | clippy + 应用内单元测试（导入等） |
| `e2e.yml` | 真实服务集成测试（xrdp、x11vnc、telnetd、虚拟串口、S3、WebDAV）+ tauri-driver 驱动真实应用的界面测试（`e2e/run.mjs`、`e2e/protocols.mjs`、`e2e/design.mjs`） |
| `updater-e2e.yml` | 自动更新：旧版本升级到新版本；错误签名与被篡改的安装包被拒绝 |
| `ios.yml` | iOS 模拟器编译、启动并截图 |
| `macos-shot.yml` | 真实 macOS 上启动打包好的应用并截图（标题栏 / 红绿灯） |
| `build.yml` | 发布：macOS（Apple 芯片 / Intel）、Windows 打包签名并生成更新清单 |

## ARCHITECTURE.md

### 技术选型（第 2 节）

| 功能 | 验证 |
|---|---|
| Tauri 2 + Svelte 5 + xterm.js（WebGL） | `ci.yml` 构建；`e2e/run.mjs` 在真实应用里读写终端 |
| russh SSH / SFTP | `sshd.rs`：`shell_and_exec`、`sftp_roundtrip`；`run.mjs`：输入密码连接、执行命令、SFTP 双栏 |
| IronRDP | `vornel-remote/tests/servers.rs::rdp_xrdp_connects_and_streams_frames`；`run.mjs`：RDP 画布出现真实画面 |
| vnc-rs | `servers.rs` VNC 用例（含错误密码）；`protocols.mjs`：VNC 画面与颜色通道 |
| OpenDAL 同步 | 见第 3 节 |
| keyring 存数据密钥 | `vornel-vault` 单元测试（`MemoryKeyStore` / 文件回退）；真实钥匙串由应用启动路径覆盖 |
| rusqlite + Argon2id + XChaCha20-Poly1305 | `vornel-vault`：`put_list_delete_reopen`、`wrong_key_fails` |

### 云同步（第 3 节）

| 功能 | 验证 |
|---|---|
| 主密码 → KEK → DEK 包装；改密码只重包 DEK | `vornel-vault`、`vornel-sync` 单元测试（`change_password`） |
| 云端格式 vault.json / manifest.bin / records/ | `protocols.mjs`：同步目录里逐个检查文件，且不含任何明文主机名、地址、密码 |
| HLC + 字段级 LWW + 墓碑 | `vornel-vault`：`concurrent_edits_merge_per_field`、`tombstone_wins_over_older_edits`、`delete_then_newer_edit_resurrects` |
| 增量同步（拉 manifest → 对比 → 合并 → 推送） | `vornel-sync::two_devices_sync_through_folder`；`backends.rs` 在 S3 / WebDAV 上同样两台设备互相同步 |
| 后端：本地文件夹 | 单元测试 + `protocols.mjs` 界面全流程 |
| 后端：S3 / R2 / MinIO | `backends.rs::sync_via_minio_s3`、`s3_probe_and_friendly_errors`（真实 S3 服务） |
| 后端：WebDAV | `backends.rs::sync_via_webdav`（真实 WebDAV 服务） |
| 后端：Google Drive / OneDrive / Dropbox | OAuth PKCE 回环流程与 state 校验：`oauth.rs` 单元测试（模拟授权服务器）。**真实服务需要用户注册 OAuth Client ID，见下方“需要用户提供”** |
| 同步范围：私钥默认不同步、密码可选同步 | `vornel-vault::local_only_records_are_not_synced` |
| 保存时连接测试、可读的错误提示 | `backends.rs::s3_probe_and_friendly_errors`；`protocols.mjs`：无写权限目录报错并可复制 |

### 需要提前知道的坑（第 4 节）

| 功能 | 验证 |
|---|---|
| iOS 回到前台自动重连、Mosh 可恢复 | `vornel-mosh` 真实 mosh-server 测试；自动重连逻辑在 `TerminalPane`（iOS 上的前后台切换需真机，未自动化） |
| RDP 帧用二进制 Channel、只传变化区域 | `servers.rs` 校验 8 字节头 + RGBA 脏矩形帧 |
| 不引入 GPL 依赖 | `ci.yml` 依赖许可检查：扫描全部 Rust 与 npm 依赖（mosh 为自研实现） |
| known_hosts 严格校验并同步 | `sshd.rs::host_key_trust_on_first_use_and_change_detection`；`run.mjs` 已知主机页 |
| SSH Agent 认证与转发 | `sshd.rs::agent_auth_and_forwarding`（远端 `ssh-add -l` 能看到本机密钥） |
| FIDO / 硬件密钥 | 通过系统 Agent 使用，路径同上 |

### 里程碑（第 7 节）

| 阶段 | 验证 |
|---|---|
| M1 SSH 终端、主机管理、密钥管理 | `run.mjs`：新建主机、密码连接、执行命令、生成 ed25519；`sshd.rs::wrong_key_is_rejected` |
| M2 SFTP、端口转发、跳板机 | `sshd.rs`：`sftp_roundtrip`、`local_and_dynamic_forwarding`、`remote_forwarding`、`jump_host_proxy_jump`；`run.mjs`：界面启动本地转发并经转发读到 SSH 横幅 |
| M3 加密 Vault 与同步 | 见第 3 节 |
| M4 iOS | `ios.yml` 模拟器启动截图。**签名 IPA 需要用户的证书，见下方** |
| M5 RDP、VNC、Mosh、Telnet、串口 | `servers.rs`、`vornel-mosh` 测试、`vornel-term/tests/real.rs`（真实 telnetd、socat 虚拟串口）；`protocols.mjs` 界面连接 Telnet / VNC / Mosh |
| 自动更新 | `updater-e2e.yml` |
| 主机页（分组卡片、系统 logo、快速连接 `ssh user@host`、按最近连接排序） | `run.mjs`：快速连接、自动识别系统（Ubuntu logo）、最近连接排在最前、主机名不被截断 |
| 新建主机时填写密码 / 查看已保存的密码 | `run.mjs`：填写密码后连接不再询问；编辑时点“显示密码”读出正确密码 |
| 分组：下拉选择、新建、外观（logo / 底色 / 图片）、改名 | `run.mjs`：新建分组、设置 Debian logo 与底色、改名后主机跟随 |
| 右键菜单（主机、分组、空白处、终端、标签、输入框）与原生剪贴板 | `run.mjs`：终端右键粘贴并执行、标签复制会话 / 关闭、主机卡片复制地址（读回系统剪贴板） |
| 导入数据（从其他客户端迁移） | `import.rs` 单元测试：主机、密码、密钥（解开口令）、跳板机引用、已知主机、重复导入跳过 |

### 字体包（第 8 节）

| 功能 | 验证 |
|---|---|
| 内置 6 套字体包 | `design.mjs`：从 DESIGN.md 读出 6 套，逐个切换并确认字体真实加载 |
| 中文增强包按需下载、sha256 校验 | `fonts.yml` 发布 `fonts-v1`；`ci.yml` 逐个下载发布的字体文件并核对应用里固定的 sha256；应用安装时同样校验（`commands/fonts.rs`） |

## DESIGN.md

`e2e/design.mjs` 直接解析 DESIGN.md 的表格与真实运行的应用对照，规范与实现任一边改动不一致都会失败。

| 规范 | 验证 |
|---|---|
| 2.1 Logo 几何 | `design.mjs`：应用图标与界面 Logo 的折线、光标矩形 |
| 3.1 颜色令牌（Ink / Paper 两套） | `design.mjs`：逐个令牌比对 |
| 3.3 对比度 | `design.mjs`：正文 / 次要文字 ≥ 4.5:1，状态色 ≥ 3:1（两套主题） |
| 4.2 字体包 | `design.mjs` |
| 5.3 桌面骨架（顶栏 46、红绿灯居中） | `design.mjs` 尺寸检查；`macos-shot.yml` 真机截图 |
| 6 组件尺寸（侧边栏条目 29、状态栏 27、输入框 32、对话框 440） | `design.mjs` |
| 6 对话框 Esc 关闭 | `design.mjs` |
| 8 可访问性：纯图标按钮有 aria-label | `design.mjs`（主界面与对话框） |
| 8 终端 role="application" + 屏幕阅读器模式 | `run.mjs` 通过 `div[role="application"]` 定位终端；`design.mjs` 检查开关 |
| 8 ⌘+ / ⌘− 字号缩放 | `design.mjs` |
| 沉浸式终端、选中即复制 | `run.mjs`：连接后侧边栏隐藏、终端占满宽度；拖选后写入剪贴板 |

## 需要用户提供（无法在 CI 中完成）

| 项目 | 需要什么 | 现状 |
|---|---|---|
| iOS 签名 IPA | 仓库 Secrets：`APPLE_DEVELOPMENT_TEAM`、`IOS_CERTIFICATE`、`IOS_CERTIFICATE_PASSWORD`、`IOS_MOBILE_PROVISION` | `ios.yml` 已写好签名步骤，配置后自动产出 IPA |
| Google Drive / OneDrive / Dropbox 真实同步 | 各平台注册的“桌面应用” OAuth Client ID | 授权流程已用模拟服务器测试；同步逻辑与其他后端共用同一套（OpenDAL） |
