<p align="center">
  <img src="apps/vornel/app-icon.svg" width="112" alt="Vornel">
</p>

<h1 align="center">Vornel</h1>

<p align="center">小巧、快速的跨平台 SSH / SFTP / RDP 客户端，支持端到端加密的云同步。</p>

---

> **开发中**：目前处于 M1 阶段，桌面端可以保存主机、通过 SSH 连接并使用终端。

## 特性与路线图

| 里程碑 | 内容 | 状态 |
|---|---|---|
| M1 | 桌面端 SSH 终端、主机管理、字体包与深浅主题 | 🚧 进行中 |
| M2 | SFTP 双栏文件管理、端口转发、跳板机、密钥串、代码片段 | 计划中 |
| M3 | 端到端加密 Vault，同步到 S3 / Google Drive / WebDAV / 本地文件夹 | 计划中 |
| M4 | iOS 版本 | 计划中 |
| M5 | RDP、VNC、Mosh、Telnet、串口 | 计划中 |

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
- 推送 `v*` 标签会自动打包并创建草稿 Release。

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
