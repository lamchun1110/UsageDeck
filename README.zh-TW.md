<p align="center">
  <img src="assets/usagedeck-banner.png" alt="UsageDeck 標誌" width="560">
</p>

<h1 align="center">UsageDeck</h1>

<p align="center">
  <a href="README.md">English</a> · 繁體中文 · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a>
</p>

<p align="center">
  <b>把你付費訂閱的每一項 AI 編程工具，收進同一個面板。</b>
</p>

<p align="center">
  <a href="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml"><img src="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml/badge.svg" alt="CI 狀態"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases/latest"><img src="https://img.shields.io/github/v/release/lamchun1110/UsageDeck" alt="最新版本"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases"><img src="https://img.shields.io/github/downloads/lamchun1110/UsageDeck/total" alt="總下載次數"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT 授權條款"></a>
</p>

UsageDeck 是一款適用於 Windows、Linux 與 macOS 的開源、隱私優先桌面儀表板，可集中追蹤 13 款 AI 編程助理的用量限制、重設時間、Token 歷史與預估支出。

它常駐於系統列或選單列，並重用電腦上既有的登入憑證。所有功能都在本機執行——無需註冊 UsageDeck 帳號，也沒有後端、分析或遙測。

## 從 OpenQuota 過來？

UsageDeck 原本是 OpenQuota 的分支，現在已是獨立的專案。你的資料會跟著你走：首次啟動時，UsageDeck 會自動從既有的 OpenQuota 安裝搬移設定、用量歷史、價格快取與 Antigravity 本機資料，存放在系統憑證儲存區的 API 金鑰也會移到 UsageDeck 自己的條目。舊位置的資料一律不會刪除。存放在 `~/.config/openquota/{kimi,minimax,zai}.json` 的金鑰仍會被讀取；新的金鑰則存放於 `~/.config/usagedeck/`。OpenRouter 會繼續讀取其舊路徑 `~/.config/openrouter/key.json`。

## 安裝

從[最新發布](https://github.com/lamchun1110/UsageDeck/releases/latest)下載對應平台的安裝檔：

| 平台    | 檔案                                   | 說明                                        |
| ------- | -------------------------------------- | ------------------------------------------- |
| Windows | `_x64-setup.exe` 或 `_arm64-setup.exe` | x64 與 ARM64                                |
| macOS   | `_universal.dmg`                       | Universal，Developer ID 簽署並經 Apple 公證 |
| Linux   | `.AppImage` 或 `.deb`                  | x64 與 ARM64，附 GPG 分離簽署               |

應用程式會自動更新。更新包使用專案自己的更新器金鑰做了加密簽署，這與作業系統的套件簽署是兩回事。

### 發布簽署

- **macOS：** 當儲存庫變數 `ENABLE_MACOS_NATIVE_SIGNING` 設為 `true` 時，正式發布版本會使用 Apple Developer ID 憑證簽署並經 Apple 公證。已簽署版本的 bundle ID 為 `com.lamchun1110.usagedeck`。發布流程在發布前會驗證程式碼簽署、Gatekeeper 評估、公證票證與強化執行階段（Hardened Runtime）。
- **Linux：** 每個 `.AppImage` 與 `.deb` 都有對應的 ASCII 裝甲分離簽署檔 `<file>.asc`。發布內容還包含 `SHA256SUMS`、其 GPG 簽署副本 `SHA256SUMS.asc`，以及驗證所需的公鑰 `usagedeck-gpg-public.asc`。

驗證 Linux 下載：

```bash
gpg --import usagedeck-gpg-public.asc
gpg --verify UsageDeck.AppImage.asc UsageDeck.AppImage
# 請將上方的範例檔名替換為發布頁中的實際檔名。
```

> [!IMPORTANT]
> Windows 安裝程式目前未做 Authenticode 簽署，SmartScreen 可能會要求你確認。請只從本儲存庫的發布頁面下載 UsageDeck。每個版本的發布說明都會註明該版本的正確簽署狀態。

## 可追蹤的服務

| 服務                                              | 憑證來源 | 追蹤內容                                                               |
| ------------------------------------------------- | -------- | ---------------------------------------------------------------------- |
| **[Claude Code](docs/providers/claude.md)**       | 本機     | 多帳戶、工作階段與每週限制、各模型用量、Token 歷史、預估支出           |
| **[Codex](docs/providers/codex.md)**              | 本機     | 工作階段與每週限制、速率限制重設、點數、Token 歷史、模型分佈、預估支出 |
| **[Command Code](docs/providers/commandcode.md)** | 本機     | 工作階段、每週與每月限制，以及額外點數                                 |
| **[Cursor](docs/providers/cursor.md)**            | 本機     | 總用量、Auto 與 API 用量、點數、Token 歷史、預估支出                   |
| **[Antigravity](docs/providers/antigravity.md)**  | 本機     | Gemini 與 Claude 共用的額度池                                          |
| **[Copilot](docs/providers/copilot.md)**          | 本機     | 進階要求、額外用量、聊天與補全額度、組織帳務                           |
| **[Devin](docs/providers/devin.md)**              | 本機     | 每日與每週限制、重設時間、額外用量餘額                                 |
| **[Grok](docs/providers/grok.md)**                | 本機     | 每週配額、額外用量狀態、Token 歷史、預估支出                           |
| **[OpenCode](docs/providers/opencode.md)**        | 本機     | Go 的工作階段、每週與每月支出上限，以及本機用量歷史                    |
| **[OpenRouter](docs/providers/openrouter.md)**    | API 金鑰 | 點數餘額與每日、每週、每月支出                                         |
| **[Z.ai](docs/providers/zai.md)**                 | API 金鑰 | GLM Coding Plan 的工作階段、每週與網頁搜尋額度                         |
| **[Kimi](docs/providers/kimi.md)**                | API 金鑰 | Kimi Code 的工作階段與每週額度，可自選網域                             |
| **[MiniMax](docs/providers/minimax.md)**          | API 金鑰 | Token Plan 的工作階段與每週額度                                        |

標示**本機**的服務會沿用你的 CLI 或編輯器既有的登入狀態，無須額外設定。標示 **API 金鑰**的服務則需要你在「自訂」中貼上一次金鑰；金鑰會直接存入作業系統的憑證儲存區，而不是設定檔。Codex 的訂閱限制需要 ChatGPT 登入，僅使用 API 金鑰的情況下不會顯示。

## 日常使用

- **系統列彈出視窗或浮動視窗。** 看一眼就關閉，或把面板留在第二個螢幕上。
- **釘選重要項目。** 把任何指標提升到系統列或 macOS 選單列顯示。
- **已用或剩餘。** 依照你習慣的思考方式切換。
- **用量步調。** 在額度真的不夠之前，先告訴你目前的消耗速度能不能撐到下次重設。
- **歷史紀錄。** 今天、昨天，以及過去 30 天的 Token 用量與預估支出。
- **提前提醒。** 可選的桌面通知：額度快用完、所剩不多，以及照目前速度會在重設前用光時。
- **版面隨你安排。** 重新排序服務與指標、隱藏列、收合區塊。
- **外觀隨你調整。** 淺色、深色或跟隨系統，五種強調色、精簡密度，以及 12 或 24 小時制。
- **分享卡片。** 把任一服務的面板複製成圖片，直接貼上就能分享。
- **說你的語言。** English、繁體中文、简体中文、日本語、한국어，或跟隨系統設定。
- **不打擾你。** 開機自動啟動、全域快速鍵、跟隨系統主題。

所有運算都在你的電腦上完成。沒有帳號、沒有後端伺服器、沒有分析、也沒有遙測。

## 從原始碼建置

你需要 Node.js 22 以上、pnpm 11.11.0、穩定版 Rust 工具鏈，以及對應平台的 [Tauri 2 環境需求](https://v2.tauri.app/start/prerequisites/)。

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm tauri dev
```

送出 Pull Request 前，請執行完整檢查——格式化、Lint、型別、契約與兩套測試：

```sh
corepack pnpm verify
```

為目前平台打包：

```sh
corepack pnpm build:installer             # Windows
corepack pnpm build:linux                 # Linux
corepack pnpm tauri build --bundles dmg   # macOS
```

發行與簽署需求請見 [docs/releasing.md](docs/releasing.md)。

## 參與貢獻

歡迎提交 Issue 與 Pull Request，開始前請先閱讀 [CONTRIBUTING.md](CONTRIBUTING.md)。安全性問題請依照 [SECURITY.md](SECURITY.md) 私下回報，不要開在公開 Issue。

## 淵源

最早是 Robin Ebers 為 macOS 打造的 [OpenUsage](https://github.com/robinebers/openusage)；接著 deviffyy 的 [OpenQuota](https://github.com/deviffyy/OpenQuota) 以 Tauri 將這個構想重建為跨平台（Windows、Linux、macOS）應用程式。UsageDeck 從該專案的分支起步，如今已成長為擁有自身品牌、發行基礎架構與路線圖的獨立產品。原始設計與早期絕大部分程式碼的功勞屬於這兩個專案——謝謝你們。

## 授權條款

[MIT](LICENSE)
