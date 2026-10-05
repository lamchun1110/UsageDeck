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

用量更新会直接连接你配置或登录的第三方服务。预估支出使用的公开模型价格目录约每天从 GitHub、models.dev 和 openrouter.ai 更新一次，这些请求不包含凭据或用量数据。启用 Session Kickstart 后，CLI 会发送小型提示并使用服务额度；兑换 Codex 重置点数需要确认。详见[隐私政策](https://usagedeck.app/privacy/)。

各服务可提供的指标取决于账户和套餐。

## 从 OpenQuota 过来？

UsageDeck 最初是 OpenQuota 的 fork，现在已是独立项目。首次启动时，会从现有的 OpenQuota 安装复制设置、用量历史、价格缓存和 Antigravity 本地数据，并保留源文件。API 密钥会移到系统凭据存储的 `UsageDeck` 条目；只有确认新密钥已成功保存后，才会删除旧凭据条目。若转移失败，请在“自定义”中重新添加密钥。`~/.config/openquota/{kimi,minimax,zai}.json` 和 `~/.config/usagedeck/{kimi,minimax,zai}.json` 仍可作为外部密钥来源；通过应用添加的密钥一律保存在系统凭据存储中。OpenRouter 也接受 `~/.config/openrouter/key.json`。

## 安装

从[最新发布](https://github.com/lamchun1110/UsageDeck/releases/latest)下载对应平台的安装文件：

| 平台    | 文件                                   | 说明                                    |
| ------- | -------------------------------------- | --------------------------------------- |
| Windows | `_x64-setup.exe` 或 `_arm64-setup.exe` | x64 与 ARM64                            |
| macOS   | `_universal.dmg`                       | Intel 和 Apple Silicon；macOS 11 及以上 |
| Linux   | `.AppImage`、`.deb` 或 `.rpm`          | x64 和 ARM64                            |

UsageDeck 可自动检查更新。Windows、macOS 和 Linux AppImage 可在应用内安装更新；Linux `.deb` 和 `.rpm` 会打开发布页面，让你下载并安装新版软件包。更新包使用项目的更新器密钥签名，与操作系统原生签名分开验证。

### 发布签名

- **Windows：** 是否使用 Authenticode 签名取决于该次发布配置；请查看所选版本的签名信息。未签名的安装程序可能触发 SmartScreen。
- **macOS：** Developer ID 签名和 Apple 公证按发布配置启用；未启用时为临时签名且未公证。
- **Linux：** 启用 GPG 签名的发布会提供 `<file>.asc`、`SHA256SUMS`、内嵌签名文档 `SHA256SUMS.asc` 和公钥 `usagedeck-gpg-public.asc`；RPM 另有内嵌签名。

验证 Linux 下载：

```bash
gpg --import usagedeck-gpg-public.asc
gpg --verify UsageDeck.AppImage.asc UsageDeck.AppImage
# 请将上面的示例文件名替换为发布页中的实际文件名。
```

> [!IMPORTANT]
> 请从本仓库的发布页下载，并查看所选版本的签名信息及实际提供的签名文件。未做原生签名的版本可能触发 Windows SmartScreen 或 macOS Gatekeeper。验证命令见 [docs/releasing.md](docs/releasing.md)。

## 可追踪的服务

| 服务                                              | 凭据     | 追踪内容                                                            |
| ------------------------------------------------- | -------- | ------------------------------------------------------------------- |
| **[Claude Code](docs/providers/claude.md)**       | 本地     | 多账户、会话与每周限额、额外重置次数与到期日、Token 历史、预估支出  |
| **[Codex](docs/providers/codex.md)**              | 本地     | 会话与每周限额、重置点数与到期日、Token 历史、模型分布、预估支出    |
| **[Command Code](docs/providers/commandcode.md)** | 本地     | 会话、每周与每月限额，以及额外点数                                  |
| **[Cursor](docs/providers/cursor.md)**            | 本地     | 总用量、Auto 与 API 用量、点数、Token 历史、预估支出                |
| **[Antigravity](docs/providers/antigravity.md)**  | 本地     | Gemini 与 Claude 共享的额度池                                       |
| **[Copilot](docs/providers/copilot.md)**          | 本地     | 高级请求或 AI 点数、额外用量、聊天与补全限额、组织计费              |
| **[Devin](docs/providers/devin.md)**              | 本地     | 每日与每周限额、重置时间、额外用量余额                              |
| **[Grok](docs/providers/grok.md)**                | 本地     | 每周配额、额外用量状态、Token 历史、预估支出                        |
| **[OpenCode](docs/providers/opencode.md)**        | 本地     | 多个 Go 账户、会话与每周及每月额度、本地用量与预估支出              |
| **[OpenRouter](docs/providers/openrouter.md)**    | API 密钥 | 点数、余额、每日与每周及每月支出、密钥限额                          |
| **[Z.ai](docs/providers/zai.md)**                 | API 密钥 | GLM Coding Plan 会话、每周与网页搜索额度；个人 ZCode 重置卡与到期日 |
| **[Kimi](docs/providers/kimi.md)**                | API 密钥 | Kimi Code 的会话与每周额度，域名可自选                              |
| **[MiniMax](docs/providers/minimax.md)**          | API 密钥 | Token Plan 的会话与每周额度                                         |

标注**本地**的服务会直接复用你的 CLI 或编辑器已有的登录状态，无需任何配置。标注
**API 密钥**的服务则需要你在“自定义”中粘贴一次密钥；密钥会直接存入操作系统的凭据存储，
而不是配置文件。Codex 的订阅限额需要 ChatGPT 登录，仅使用 API 密钥时不会显示。

## 日常使用

- **多账户。** Claude 配置目录、OpenCode Go 数据目录，以及 OpenRouter、Z.ai、Kimi、MiniMax 的命名 API 密钥账户各有独立卡片。命名 API 账户添加后立即可用，无须重启。
- **重置点数到期提醒。** 查看 Claude、Codex 和个人 ZCode 的可用次数及已知到期日，并选择提前 1、24、48 或 168 小时通知；隐藏重置行也能提醒。Z.ai 需要匹配的个人 ZCode 登录和 API 密钥。
- **Session Kickstart。** 可选择在滚动用量窗口到期后执行小型 CLI 提示来开始新窗口。每次提示都会使用服务额度，可使用内置或自定义命令。
- **Linux 托盘备用方式。** 桌面环境没有可用系统托盘时，自动改用独立窗口。
- **托盘弹出面板或浮动窗口。** 看一眼就关，或者把面板常驻在第二块屏幕上。
- **固定重要指标。** 可以把支持固定的指标提升到系统托盘或 macOS 菜单栏显示。
- **已用或剩余。** 按你习惯的方式显示额度。
- **消耗节奏。** 在额度真正见底之前，提前告诉你按目前的消耗速度能否撑到下次重置。
- **历史记录。** 今天、昨天，以及过去 30 天的 Token 用量与预估支出。
- **提前提醒。** 可选的桌面通知：额度快用完、所剩不多，以及按当前速度会在重置前用光时。
- **布局随你安排。** 重新排列服务与指标、隐藏某些行、折叠区块。
- **外观随你调整。** 浅色、深色或跟随系统，五种强调色、紧凑密度，以及 12 或 24 小时制。
- **分享卡片。** 把任意服务的面板复制为图片，直接粘贴即可分享。
- **说你的语言。** English、繁體中文、简体中文、日本語、한국어，或跟随系统设置。
- **不打扰你。** 开机自启、全局快捷键、跟随系统主题。

数据保存在你的电脑上，没有 UsageDeck 账户、项目运营的后端、分析或遥测。用量更新与价格目录会直接连接第三方服务。

账户设置、重置提醒与更新方式见[使用指南（英文）](docs/usage.md)。

## 从源码构建

你需要 Node.js 24 及以上版本、pnpm 11.11.0、通过 rustup 安装的 Rust，以及所用平台的
[Tauri 2 前置要求](https://v2.tauri.app/start/prerequisites/)。

Rust 版本、Clippy 和 rustfmt 由 [rust-toolchain.toml](rust-toolchain.toml) 固定；rustup 会自动选择。

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
