<p align="center">
  <img src="apps/vornel/app-icon.svg" width="112" alt="Vornel">
</p>

<h1 align="center">Vornel</h1>

<p align="center">小巧、快速的跨平台 SSH / SFTP / RDP 客户端，支持端到端加密的云同步。</p>

---

## 功能

- **协议**：SSH、Mosh、Telnet、串口、RDP、VNC
- **SSH**：密码 / 密钥串 / 私钥文件 / 系统 SSH Agent 认证，跳板机，Agent 转发，主机密钥首次信任与变化检测
- **SFTP**：本机与远端双栏文件管理，上传、下载、重命名、删除、新建文件夹
- **端口转发**：本地（-L）、远程（-R）、SOCKS5 动态（-D）
- **密钥串**：生成 ed25519 密钥、导入私钥；私钥只保存在本机加密 Vault 中
- **代码片段**：一键发送到当前终端
- **端到端加密同步**：本地文件夹（iCloud Drive / Syncthing）、S3 / R2 / MinIO、WebDAV、Google Drive、OneDrive、Dropbox
- **外观**：深色 Ink / 浅色 Paper 主题，6 套字体包，可下载的中文增强包
- **平台**：macOS、Windows、iOS

| 里程碑 | 内容 | 状态 |
|---|---|---|
| M1 | 桌面端 SSH 终端、主机管理、密钥管理 | ✅ |
| M2 | SFTP 双栏、端口转发、跳板机 | ✅ |
| M3 | 加密 Vault 与云同步 | ✅ |
| M4 | iOS | ✅ |
| M5 | RDP、VNC、Mosh、Telnet、串口 | ✅ |

已知限制见 [ARCHITECTURE.md 7.2](ARCHITECTURE.md#72-已知限制)。

## 技术栈

- **核心**：Rust（[russh](https://github.com/Eugeny/russh)、tokio），三端共用
- **界面**：[Tauri 2](https://tauri.app) + Svelte 5 + [xterm.js](https://xtermjs.org)（WebGL 渲染）
- 详见 [ARCHITECTURE.md](ARCHITECTURE.md) 与 [DESIGN.md](DESIGN.md)

## 下载

在 [Releases](https://github.com/lettimepassby/vornel/releases) 页面下载安装包。macOS 版本暂未签名，首次打开请在 Finder 中右键应用并选择“打开”。

## 构建

所有构建都在 GitHub Actions 上完成：

- 每次推送到 `main` 会运行 **CI**（前端类型检查、Rust 格式 / Clippy / 测试）。
- 在 Actions 页面手动运行 **Build** 工作流，可以得到 macOS 与 Windows 的安装包。
- 推送 `v*` 标签会自动打包、正式发布 Release，并上传应用内更新清单（见下方“发布新版本”）。
- **iOS** 工作流在每次推送到 `main` 时构建模拟器版本，验证能否编译。

### 发布新版本

macOS 与 Windows 版内置自动更新：启动时检查 GitHub 上最新 Release 的 `latest.json`，发现新版本会在窗口顶部提示，也可以在“设置 → 关于与开源许可”里手动检查。iOS 版通过 App Store 更新。

1. **改版本号**：`node scripts/bump-version.mjs 0.2.0`。脚本会同时修改 `Cargo.toml`、`apps/vornel/src-tauri/tauri.conf.json`、`apps/vornel/package.json`，并刷新 `Cargo.lock` 与 `package-lock.json`。
2. **提交**：`git commit -am "版本 0.2.0"`，推送到 `main`。
3. **打标签并推送**：`git tag v0.2.0 && git push origin v0.2.0`。
4. **Actions 自动完成**：Build 工作流打包 macOS 与 Windows 安装包，用 Secret 中的私钥给更新包签名，正式发布 Release，并生成更新清单 `latest.json`。
5. **已安装的应用自动提示更新**：用户点“更新并重启”即可完成下载、校验签名、安装与重启。

更新包签名需要仓库 Secret `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，对应的公钥写在 `tauri.conf.json` 的 `plugins.updater.pubkey`。私钥一旦丢失，已安装的版本将无法再验证新的更新包，只能让用户手动下载安装，请妥善备份。

### iOS 签名

要得到可安装的 IPA（TestFlight / App Store），在仓库的 **Settings → Secrets and variables → Actions** 中添加：

| Secret | 内容 |
|---|---|
| `APPLE_DEVELOPMENT_TEAM` | 开发者团队 ID（10 位） |
| `IOS_CERTIFICATE` | 分发证书 `.p12` 文件的 base64（`base64 -i cert.p12`） |
| `IOS_CERTIFICATE_PASSWORD` | 导出 `.p12` 时设置的密码 |
| `IOS_MOBILE_PROVISION` | App Store 描述文件 `.mobileprovision` 的 base64，Bundle ID 为 `com.lettimepassby.vornel` |

配置后，iOS 工作流会额外构建签名的 IPA，作为 Actions 产物上传。

本地开发（可选）：

```bash
cd apps/vornel
npm install
npx tauri icon app-icon.svg
npm run tauri dev
```

## 目录结构

```
crates/vornel-core   数据模型与本地存储
crates/vornel-ssh    SSH 连接、主机密钥校验、交互式 Shell
apps/vornel          Tauri 应用（src-tauri 为 Rust 端，src 为 Svelte 前端）
design               设计稿与字体包清单
```

## 许可

本项目采用 [PolyForm Noncommercial 1.0.0](LICENSE.md) 许可：

- ✅ 个人、学习、研究、非营利组织等**非商业用途**可以免费使用、修改和分发。
- ❌ **禁止商业用途**。如需商业授权，请联系作者。

注意：这是“源码公开”许可，不属于 OSI 定义的开源许可。

内置字体均采用 SIL Open Font License 1.1，各自的许可随字体包一起分发。
