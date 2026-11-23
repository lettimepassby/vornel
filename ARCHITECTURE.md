# Vornel 技术架构

> 跨平台（Windows / macOS / iOS）源码公开的 SSH 客户端，支持 SSH、SFTP、RDP 等协议，可通过 Google Drive、S3 等后端做端到端加密的云同步。
> 设计目标：**小巧、高效、低占用**。

---

## 1. 总体思路

最先要定的是“核心用什么写”，界面框架其次：

- **Rust 写核心**：SSH、SFTP、RDP、同步、加密、凭据这些逻辑全部用 Rust 写一份，三端共用。
- **Tauri 2 做界面**：用系统自带的 WebView，安装包小，并且支持 iOS。
- **核心与界面解耦**：以后真要换成 Flutter 或原生 UI，核心不用重写。

```
┌──────────────────────────────────────────────────────────┐
│  UI 层：Tauri 2 + Svelte 5（不用 React，包更小）              │
│  ├─ 终端：xterm.js（WebGL 渲染器）                          │
│  ├─ RDP/VNC：<canvas>，只更新变化的区域（脏矩形）             │
│  └─ SFTP 文件管理、主机列表、设置                            │
├───────────────── Tauri IPC（二进制 Channel）────────────────┤
│  vornel-core（Rust，tokio 异步）                            │
│  ├─ vornel-ssh   russh + russh-sftp：跳板机、端口转发、Agent  │
│  ├─ vornel-rdp   IronRDP                                   │
│  ├─ vornel-vnc   vnc-rs（可选）                             │
│  ├─ vornel-vault 本地 SQLite + 加密                         │
│  └─ vornel-sync  OpenDAL：S3 / GDrive / OneDrive / WebDAV  │
└──────────────────────────────────────────────────────────┘
```

---

## 2. 技术选型

| 需求 | 方案 | 理由 |
|---|---|---|
| 小巧、低占用 | **Tauri 2** | 用系统自带的 WebView，安装包 5–15MB；Electron 通常 100MB 以上 |
| 前端框架 | **Svelte 5** | 编译时框架，运行时小、没有虚拟 DOM 的开销 |
| 终端渲染 | **xterm.js** + WebGL addon | 业界最成熟的 Web 终端，VS Code 也在用 |
| 支持 iOS | Tauri 2 / 纯 Rust 依赖 | 避开在 iOS 上交叉编译 OpenSSL、libssh2 的麻烦 |
| SSH / SFTP | **`russh`** + **`russh-sftp`** | 纯 Rust、异步，一条连接上可以开多个通道，支持端口转发 |
| RDP | **`IronRDP`**（Devolutions） | 纯 Rust，MIT/Apache 协议，有人在维护；他们自己的产品也在浏览器 canvas 里渲染它 |
| VNC | `vnc-rs` | 可选，后期再做 |
| 云同步 | **Apache `OpenDAL`** | 一套 API 覆盖 S3、R2、MinIO、Google Drive、OneDrive、Dropbox、WebDAV 和本地目录 |
| 凭据存储 | **`keyring`** | 走系统钥匙串：macOS/iOS 是 Keychain，Windows 是凭据管理器 |
| 本地数据 | **`rusqlite`** | 轻量、成熟 |
| 加密 | `argon2` + `chacha20poly1305` | Argon2id 派生密钥，XChaCha20-Poly1305 加密 |
| 异步运行时 | `tokio` | 生态最完整 |

---

## 3. 云同步设计

**不要直接同步 SQLite 文件。** 推荐用“端到端加密 + 按条记录存储”。

### 3.1 密钥体系

```
主密码 ──Argon2id──▶ 主密钥 (KEK)
                        │ 加密
                        ▼
                  数据密钥 (DEK，随机生成) ──▶ 加密所有记录
```

- 改主密码时只需重新加密 DEK，数据本身不用重加密。
- DEK 可以缓存在系统钥匙串里，这样每次启动不必输入主密码（可选）。

### 3.2 云端存储格式

```
vornel/
├─ vault.json            # 加密后的 DEK、KDF 参数、格式版本
├─ manifest.bin          # 加密的记录索引：{ id → 版本号/HLC }
└─ records/
   ├─ <uuid>.bin         # 每条主机/密钥/代码片段一个文件，内容是加密的 JSON
   └─ ...
```

云端只能看到一堆密文。

### 3.3 冲突合并

- 每条记录带 **HLC（混合逻辑时钟）** 时间戳。
- 冲突时按字段“最后写入者胜”（LWW）。
- 删除操作留一个**墓碑记录**，定期清理。
- 对 SSH 客户端来说这样就够了，**不需要上 CRDT**（Automerge 之类）。

### 3.4 增量同步流程

1. 拉取远端 `manifest`
2. 和本地对比，找出需要拉取和推送的记录
3. 下载、解密、合并
4. 上传本地变更，最后再写 `manifest`

### 3.5 各后端的注意点

| 后端 | 说明 |
|---|---|
| 本地文件夹 | 指向 iCloud Drive 或 Syncthing 的目录，几乎不用写代码就能用，**建议第一个做** |
| S3 / R2 / MinIO | 用户自己填 endpoint 和密钥 |
| Google Drive | 只申请 `drive.file` 或 appDataFolder 这类窄权限，避开 Google 对受限权限的严格审核；需要你自己注册 OAuth Client ID |
| OneDrive / Dropbox | 同样走 OAuth |
| WebDAV | 适合自建 Nextcloud 的用户 |

### 3.6 同步哪些数据

- ✅ 主机、分组、标签、代码片段、端口转发规则、known_hosts
- ⚠️ 私钥：**默认只放在本机**，用户主动开启后才加密同步
- ❌ 会话日志、缓存

---

## 4. 需要提前知道的坑

### iOS 后台限制
- App 切到后台约 30 秒后，系统会断开网络连接。
- 对策：支持 **Mosh**，或者做好自动重连和会话恢复。
- 端口转发只在前台有效；iOS 上没有串口。

### RDP 的性能
- 帧数据**不要用 JSON 传**。用 Tauri 2 的 `Channel` 或二进制响应，只传变化区域的压缩数据。
- 真遇到瓶颈，再考虑用 `wgpu` 在原生层直接渲染。

### 内存占用
- Windows 上的 WebView2 本身会占几十 MB 内存。这仍然比 Electron 好得多，但比不上纯原生。

### 许可证
- 本项目采用 **PolyForm Noncommercial 1.0.0**：源码公开，允许非商业用途的使用、修改和分发，禁止商业用途。它不属于 OSI 定义的开源许可。
- 作者本人作为版权所有者不受该许可约束，可以自行在 App Store 上架或另行商业授权。
- 依赖只能选用宽松许可（MIT / Apache-2.0 / BSD / ISC / OFL 等）。**不能引入 GPL / AGPL 依赖**：它们要求衍生作品以 GPL 发布，与“禁止商用”的条款冲突。
- 如果以后接受外部贡献，需要先加贡献者协议（CLA），否则无法对他人贡献的代码做商业授权。

### 安全细节
- known_hosts 要严格校验，并参与同步。
- 支持 FIDO/硬件密钥（`sk-ssh-ed25519`）会是一个加分项。
- 支持 SSH Agent 转发，以及调用系统 Agent（macOS ssh-agent、Windows OpenSSH Agent、1Password 等）。

---

## 5. 另外两个方案（不推荐作为首选）

| 方案 | 优点 | 缺点 | 适合场景 |
|---|---|---|---|
| **Rust 核心 + Flutter** | iOS 体验更成熟；RDP 可以用 `Texture` 零拷贝渲染，性能更好 | 安装包大一些（20–40MB）；终端组件 xterm.dart 不如 xterm.js 完善 | 最看重 iOS 体验 |
| **Rust 核心 + 各平台原生 UI**（SwiftUI + WinUI） | 性能和占用最好 | 要写两套 UI，工作量大约翻倍 | 项目成熟后再做 |

---

## 6. 目录结构

```
vornel/
├─ crates/
│  ├─ vornel-core/    # 数据模型、错误类型、事件总线
│  ├─ vornel-ssh/     # SSH、SFTP、转发、跳板机
│  ├─ vornel-rdp/     # IronRDP 封装
│  ├─ vornel-vault/   # SQLite + 加密
│  └─ vornel-sync/    # OpenDAL 后端 + 合并逻辑
├─ apps/
│  └─ vornel/         # Tauri 应用
│     ├─ src-tauri/   # Rust 端：命令、IPC 桥接
│     └─ src/         # 前端：Svelte 5 + xterm.js
├─ docs/
├─ Cargo.toml         # workspace
└─ LICENSE.md         # PolyForm Noncommercial 1.0.0
```

---

## 7. 里程碑

| 阶段 | 内容 |
|---|---|
| **M1** | 桌面端 SSH 终端、主机管理、密钥管理 |
| **M2** | SFTP 双栏文件管理、端口转发、跳板机 |
| **M3** | 加密 Vault 和同步（先做本地文件夹/S3，再做 Google Drive） |
| **M4** | iOS 版本 |
| **M5** | RDP，之后再加 VNC、Mosh、Telnet、串口 |

---

## 8. 字体包

字体组合做成应用内置的“字体包”，清单见 [`design/font-packs.json`](design/font-packs.json)。

### 8.1 分层

| 层 | 内容 | 体积 | 方式 |
|---|---|---|---|
| **内置** | 6 套字体包用到的全部英文字体（Geist、Inter、JetBrains Mono、Maple Mono 等），woff2 格式 | 合计约 1–2 MB | 随安装包分发，离线可用 |
| **增强包** | 中文字体：Maple Mono CN、思源黑体、霞鹜文楷 | 每个约 9–12 MB | 用户点了才下载 |
| **系统** | 苹方 / 微软雅黑 | 0 | 界面中文的默认字体 |

> 大小依据：拉丁字符子集的 woff2 每款约 30–70 KB；Maple Mono CN 全家族压缩包 134 MB，霞鹜文楷 Screen 单文件 24 MB。增强包只提供“常规 + 粗体”两个字重，所以体积是估算值，打包时以实测为准。

### 8.2 字体包的组成

每个字体包定义四个角色：

- `ui`：界面文字
- `mono`：主机名、路径、数字等标识
- `terminal`：终端，另外带行高、字间距、是否开启连字
- `cjk`：界面里的中文字体

用户可以在字体包的基础上单独覆盖某一项；桌面端还可以选本机已安装的字体，用 `fontdb` crate 列出系统字体。

### 8.3 实现要点

1. **内置字体**放在 `ui/src/assets/fonts/`，在 CSS 里用 `@font-face` 注册，再通过 `--f-ui`、`--f-mono`、`--f-term` 这几个 CSS 变量切换。
2. **增强包下载**：
   - 由 Rust 下载到 `app_data_dir/fonts/<id>/`，用 `sha2` 校验 sha256，校验不通过就删除。
   - 通过自定义协议（`vornel-font://`）把字体交给前端；前端用 `FontFace` API 注册。
3. **终端换字体必须按顺序来**：
   1. `await document.fonts.load(...)`，等字体真正加载完；
   2. 设置 `term.options.fontFamily`；
   3. WebGL 渲染器调用 `clearTextureAtlas()` 清掉旧字形缓存；
   4. 调用 `fit()` 重新计算行列数。

   否则 xterm.js 会按回退字体测量字符宽度，导致字符错位。
4. **同步**：只同步字体包 id 和覆盖项，不同步字体文件。新设备上缺少增强包时，先用系统字体回退，并提示用户下载。
5. **iOS**：同样走 WKWebView。使用蜂窝网络时，下载增强包前先征得用户确认。

### 8.4 许可

- 所有字体使用 OFL-1.1，在“关于与开源许可”页面附上每款字体的 OFL 全文。
- **不做子集化**，只把原版转换成 woff2。原因是 OFL 把子集化视为修改，而声明了保留字体名（RFN）的字体，修改后就不能再用原名发布。
- **不内置 MiSans**：它用的是小米的自有协议，需要单独评估能否再分发。
- 增强包放在本项目 GitHub Releases 的 `fonts-v1` 标签下，统一转成 woff2，只提供“常规 + 粗体”两个字重。不直接下载上游动辄一百多 MB 的完整压缩包。
