<p align="center">
  <img src="assets/usagedeck-banner.png" alt="UsageDeck 标志" width="560">
</p>

<h1 align="center">UsageDeck</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-TW.md">繁體中文</a> · 简体中文 · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a>
</p>

<p align="center">
  <b>把你付费订阅的每一款 AI 编程工具，收进同一个面板。</b>
</p>

<p align="center">
  <a href="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml"><img src="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml/badge.svg" alt="CI 状态"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases/latest"><img src="https://img.shields.io/github/v/release/lamchun1110/UsageDeck" alt="最新版本"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases"><img src="https://img.shields.io/github/downloads/lamchun1110/UsageDeck/total" alt="总下载次数"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT 许可证"></a>
</p>

UsageDeck 是一款面向 Windows、Linux 和 macOS 的开源、隐私优先桌面仪表板，可集中追踪 13 款
AI 编程助手的用量限额、重置时间、Token 历史和预估支出。

它常驻于系统托盘或菜单栏，并复用电脑上已有的登录凭据。所有功能都在本地运行——无需注册
UsageDeck 账户，也没有后端、分析或遥测。

## 从 OpenQuota 过来？

UsageDeck 最初是 OpenQuota 的 fork，现在已是一个独立项目。你的数据会跟着你走：首次启动时，
UsageDeck 会自动从现有的 OpenQuota 安装中迁移设置、用量历史、价格缓存和 Antigravity 本地数据，
保存在系统凭据存储中的 API 密钥也会移到 UsageDeck 自己的条目下。旧位置的数据一律不会被删除。
存放在 `~/.config/openquota/{kimi,minimax,zai}.json` 的密钥仍会继续读取；新密钥则存放在
`~/.config/usagedeck/`。OpenRouter 会继续读取其旧路径 `~/.config/openrouter/key.json`。

## 安装

从[最新发布](https://github.com/lamchun1110/UsageDeck/releases/latest)下载对应平台的安装文件：

| 平台    | 文件                                   | 说明                                           |
| ------- | -------------------------------------- | ---------------------------------------------- |
| Windows | `_x64-setup.exe` 或 `_arm64-setup.exe` | x64 与 ARM64                                   |
| macOS   | `_universal.dmg`                       | Universal，带 Developer ID 签名并经 Apple 公证 |
| Linux   | `.AppImage` 或 `.deb`                  | x64 与 ARM64，附 GPG 分离签名                  |

应用会自动更新。更新包使用项目自己的更新器密钥做了加密签名，这与操作系统的包签名是两回事。

### 发布签名

- **macOS：** 当仓库变量 `ENABLE_MACOS_NATIVE_SIGNING` 设为 `true` 时，正式发布版会使用 Apple Developer ID 证书签名并经 Apple 公证。已签名版本的 bundle ID 为 `com.lamchun1110.usagedeck`。发布工作流在发布前会验证代码签名、Gatekeeper 评估、公证票据与强化运行时（Hardened Runtime）。
- **Linux：** 每个 `.AppImage` 和 `.deb` 都有对应的 ASCII 装甲分离签名文件 `<file>.asc`。发布内容还包含 `SHA256SUMS`、它的 GPG 签名副本 `SHA256SUMS.asc`，以及验证所需的公钥 `usagedeck-gpg-public.asc`。

验证 Linux 下载：

```bash
gpg --import usagedeck-gpg-public.asc
gpg --verify UsageDeck.AppImage.asc UsageDeck.AppImage
# 请将上面的示例文件名替换为发布页中的实际文件名。
```

> [!IMPORTANT]
> Windows 安装包目前未做 Authenticode 签名，SmartScreen 可能会要求你确认。请只从本仓库的
> 发布页面下载 UsageDeck。每个版本的发布说明中都会注明该版本的确切签名状态。

## 可追踪的服务

| 服务                                              | 凭据     | 追踪内容                                                           |
| ------------------------------------------------- | -------- | ------------------------------------------------------------------ |
| **[Claude Code](docs/providers/claude.md)**       | 本地     | 多账户、会话与每周限额、各模型用量、Token 历史、预估支出           |
| **[Codex](docs/providers/codex.md)**              | 本地     | 会话与每周限额、速率限制重置、点数、Token 历史、模型分布、预估支出 |
| **[Command Code](docs/providers/commandcode.md)** | 本地     | 会话、每周与每月限额，以及额外点数                                 |
| **[Cursor](docs/providers/cursor.md)**            | 本地     | 总用量、Auto 与 API 用量、点数、Token 历史、预估支出               |
| **[Antigravity](docs/providers/antigravity.md)**  | 本地     | Gemini 与 Claude 共享的额度池                                      |
| **[Copilot](docs/providers/copilot.md)**          | 本地     | 高级请求、额外用量、聊天与补全限额、组织计费                       |
| **[Devin](docs/providers/devin.md)**              | 本地     | 每日与每周限额、重置时间、额外用量余额                             |
| **[Grok](docs/providers/grok.md)**                | 本地     | 每周配额、额外用量状态、Token 历史、预估支出                       |
| **[OpenCode](docs/providers/opencode.md)**        | 本地     | Go 的会话、每周与每月支出上限，以及本地用量历史                    |
| **[OpenRouter](docs/providers/openrouter.md)**    | API 密钥 | 点数余额与每日、每周、每月支出                                     |
| **[Z.ai](docs/providers/zai.md)**                 | API 密钥 | GLM Coding Plan 的会话、每周与网页搜索额度                         |
| **[Kimi](docs/providers/kimi.md)**                | API 密钥 | Kimi Code 的会话与每周额度，域名可自选                             |
| **[MiniMax](docs/providers/minimax.md)**          | API 密钥 | Token Plan 的会话与每周额度                                        |

标注**本地**的服务会直接复用你的 CLI 或编辑器已有的登录状态，无需任何配置。标注
**API 密钥**的服务则需要你在“自定义”中粘贴一次密钥；密钥会直接存入操作系统的凭据存储，
而不是配置文件。Codex 的订阅限额需要 ChatGPT 登录，仅使用 API 密钥时不会显示。

## 日常使用

- **托盘弹出面板或浮动窗口。** 看一眼就关，或者把面板常驻在第二块屏幕上。
- **固定重要指标。** 可以把任意指标提升到系统托盘或 macOS 菜单栏显示。
- **已用或剩余。** 按你习惯的方式显示额度。
- **消耗节奏。** 在额度真正见底之前，提前告诉你按目前的消耗速度能否撑到下次重置。
- **历史记录。** 今天、昨天，以及过去 30 天的 Token 用量与预估支出。
- **提前提醒。** 可选的桌面通知：额度快用完、所剩不多，以及按当前速度会在重置前用光时。
- **布局随你安排。** 重新排列服务与指标、隐藏某些行、折叠区块。
- **外观随你调整。** 浅色、深色或跟随系统，五种强调色、紧凑密度，以及 12 或 24 小时制。
- **分享卡片。** 把任意服务的面板复制为图片，直接粘贴即可分享。
- **说你的语言。** English、繁體中文、简体中文、日本語、한국어，或跟随系统设置。
- **不打扰你。** 开机自启、全局快捷键、跟随系统主题。

一切都在你的电脑上运行。没有账户、没有后端、没有分析、也没有遥测。

## 从源码构建

你需要 Node.js 22 及以上版本、pnpm 11.11.0、稳定版 Rust 工具链，以及所用平台的
[Tauri 2 前置要求](https://v2.tauri.app/start/prerequisites/)。

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm tauri dev
```

提交 Pull Request 之前，请先跑一遍完整检查——格式化、Lint、类型、契约测试与两套测试套件：

```sh
corepack pnpm verify
```

为当前平台打包：

```sh
corepack pnpm build:installer             # Windows
corepack pnpm build:linux                 # Linux
corepack pnpm tauri build --bundles dmg   # macOS
```

发布与签名要求见 [docs/releasing.md](docs/releasing.md)。

## 参与贡献

欢迎提交 Issue 和 Pull Request——开始之前请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。安全问题请
按照 [SECURITY.md](SECURITY.md) 私下报告，不要开公开 Issue。

## 渊源

最早是 Robin Ebers 为 macOS 打造的 [OpenUsage](https://github.com/robinebers/openusage)；随后
deviffyy 的 [OpenQuota](https://github.com/deviffyy/OpenQuota) 用 Tauri 将这个想法重写为支持
Windows、Linux 和 macOS 的跨平台应用。UsageDeck 从该项目的 fork 起步，如今已成长为拥有自己品牌、
发布基础设施和路线图的独立产品。最初的设计与早期绝大部分代码的功劳属于这两个项目——感谢他们。

## 许可证

[MIT](LICENSE)
