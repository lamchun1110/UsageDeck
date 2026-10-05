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

用量更新會直接連線到你設定或登入的第三方服務。預估支出使用的公開模型價格目錄約每天從 GitHub、models.dev 與 openrouter.ai 更新一次，這些請求不包含憑證或用量資料。啟用 Session Kickstart 後，CLI 會送出小型提示並使用服務額度；兌換 Codex 重設點數需要確認。詳見[隱私權政策](https://usagedeck.app/privacy/)。

各服務可提供的指標取決於帳戶與方案。

## 從 OpenQuota 過來？

UsageDeck 原本是 OpenQuota 的分支，現在已是獨立專案。首次啟動時，會從既有的 OpenQuota 安裝複製設定、用量歷史、價格快取與 Antigravity 本機資料，並保留來源檔案。API 金鑰會移到系統憑證儲存區的 `UsageDeck` 條目；只有確認新金鑰已成功儲存後，才會刪除舊的憑證條目。若轉移失敗，請在「自訂」中重新加入金鑰。`~/.config/openquota/{kimi,minimax,zai}.json` 與 `~/.config/usagedeck/{kimi,minimax,zai}.json` 仍可作為外部金鑰來源；透過應用程式新增的金鑰一律存入系統憑證儲存區。OpenRouter 也接受 `~/.config/openrouter/key.json`。

## 安裝

從[最新發布](https://github.com/lamchun1110/UsageDeck/releases/latest)下載對應平台的安裝檔：

| 平台    | 檔案                                   | 說明                                  |
| ------- | -------------------------------------- | ------------------------------------- |
| Windows | `_x64-setup.exe` 或 `_arm64-setup.exe` | x64 與 ARM64                          |
| macOS   | `_universal.dmg`                       | Intel 與 Apple Silicon；macOS 11 以上 |
| Linux   | `.AppImage`、`.deb` 或 `.rpm`          | x64 與 ARM64                          |

UsageDeck 可自動檢查更新。Windows、macOS 與 Linux AppImage 可在應用程式內安裝更新；Linux `.deb` 與 `.rpm` 會開啟發布頁面，讓你下載並安裝新版套件。更新包使用專案的更新器金鑰簽署，與作業系統原生簽署分開驗證。

### 發布簽署

- **Windows：** 是否使用 Authenticode 簽署取決於該次發布設定；請查看所選版本的簽署資訊。未簽署的安裝程式可能觸發 SmartScreen。
- **macOS：** Developer ID 簽署與 Apple 公證依發布設定啟用；未啟用時為臨時簽署且未公證。
- **Linux：** 啟用 GPG 簽署的發布會提供 `<file>.asc`、`SHA256SUMS`、內嵌簽署文件 `SHA256SUMS.asc` 與公鑰 `usagedeck-gpg-public.asc`；RPM 另有內嵌簽署。

驗證 Linux 下載：

```bash
gpg --import usagedeck-gpg-public.asc
gpg --verify UsageDeck.AppImage.asc UsageDeck.AppImage
# 請將上方的範例檔名替換為發布頁中的實際檔名。
```

> [!IMPORTANT]
> 請從本儲存庫的發布頁下載，並查看所選版本的簽署資訊與實際提供的簽署檔。未做原生簽署的版本可能觸發 Windows SmartScreen 或 macOS Gatekeeper。驗證指令見 [docs/releasing.md](docs/releasing.md)。

## 可追蹤的服務

| 服務                                              | 憑證來源 | 追蹤內容                                                                |
| ------------------------------------------------- | -------- | ----------------------------------------------------------------------- |
| **[Claude Code](docs/providers/claude.md)**       | 本機     | 多帳戶、工作階段與每週限制、額外重設次數與到期日、Token 歷史、預估支出  |
| **[Codex](docs/providers/codex.md)**              | 本機     | 工作階段與每週限制、重設點數與到期日、Token 歷史、模型分佈、預估支出    |
| **[Command Code](docs/providers/commandcode.md)** | 本機     | 工作階段、每週與每月限制，以及額外點數                                  |
| **[Cursor](docs/providers/cursor.md)**            | 本機     | 總用量、Auto 與 API 用量、點數、Token 歷史、預估支出                    |
| **[Antigravity](docs/providers/antigravity.md)**  | 本機     | Gemini 與 Claude 共用的額度池                                           |
| **[Copilot](docs/providers/copilot.md)**          | 本機     | 進階要求或 AI 點數、額外用量、聊天與補全額度、組織帳務                  |
| **[Devin](docs/providers/devin.md)**              | 本機     | 每日與每週限制、重設時間、額外用量餘額                                  |
| **[Grok](docs/providers/grok.md)**                | 本機     | 每週配額、額外用量狀態、Token 歷史、預估支出                            |
| **[OpenCode](docs/providers/opencode.md)**        | 本機     | 多個 Go 帳戶、工作階段與每週及每月額度、本機用量與預估支出              |
| **[OpenRouter](docs/providers/openrouter.md)**    | API 金鑰 | 點數、餘額、每日與每週及每月支出、金鑰限額                              |
| **[Z.ai](docs/providers/zai.md)**                 | API 金鑰 | GLM Coding Plan 工作階段、每週與網頁搜尋額度；個人 ZCode 重設卡與到期日 |
| **[Kimi](docs/providers/kimi.md)**                | API 金鑰 | Kimi Code 的工作階段與每週額度，可自選網域                              |
| **[MiniMax](docs/providers/minimax.md)**          | API 金鑰 | Token Plan 的工作階段與每週額度                                         |

標示**本機**的服務會沿用你的 CLI 或編輯器既有的登入狀態，無須額外設定。標示 **API 金鑰**的服務則需要你在「自訂」中貼上一次金鑰；金鑰會直接存入作業系統的憑證儲存區，而不是設定檔。Codex 的訂閱限制需要 ChatGPT 登入，僅使用 API 金鑰的情況下不會顯示。

## 日常使用

- **多帳戶。** Claude 設定檔、OpenCode Go 資料目錄，以及 OpenRouter、Z.ai、Kimi、MiniMax 的具名 API 金鑰帳戶各有獨立卡片。具名 API 帳戶新增後立即可用，無須重新啟動。
- **重設點數到期提醒。** 查看 Claude、Codex 與個人 ZCode 的可用次數和已知到期日，並選擇提前 1、24、48 或 168 小時通知；隱藏重設列也能提醒。Z.ai 需要相符的個人 ZCode 登入與 API 金鑰。
- **Session Kickstart。** 可選擇在滾動用量視窗到期後執行小型 CLI 提示來開始新視窗。每次提示都會使用服務額度，可使用內建或自訂指令。
- **Linux 系統列備援。** 桌面環境沒有可用的系統列時，自動改用獨立視窗。
- **系統列彈出視窗或浮動視窗。** 看一眼就關閉，或把面板留在第二個螢幕上。
- **釘選重要項目。** 把支援釘選的指標提升到系統列或 macOS 選單列顯示。
- **已用或剩餘。** 依照你習慣的思考方式切換。
- **用量步調。** 在額度真的不夠之前，先告訴你目前的消耗速度能不能撐到下次重設。
- **歷史紀錄。** 今天、昨天，以及過去 30 天的 Token 用量與預估支出。
- **提前提醒。** 可選的桌面通知：額度快用完、所剩不多，以及照目前速度會在重設前用光時。
- **版面隨你安排。** 重新排序服務與指標、隱藏列、收合區塊。
- **外觀隨你調整。** 淺色、深色或跟隨系統，五種強調色、精簡密度，以及 12 或 24 小時制。
- **分享卡片。** 把任一服務的面板複製成圖片，直接貼上就能分享。
- **說你的語言。** English、繁體中文、简体中文、日本語、한국어，或跟隨系統設定。
- **不打擾你。** 開機自動啟動、全域快速鍵、跟隨系統主題。

資料儲存在你的電腦上，沒有 UsageDeck 帳號、專案營運的後端、分析或遙測。用量更新與價格目錄會直接連線至第三方服務。

帳戶設定、重設提醒與更新方式見[使用指南（英文）](docs/usage.md)。

## 從原始碼建置

你需要 Node.js 24 以上、pnpm 11.11.0、透過 rustup 安裝的 Rust，以及對應平台的 [Tauri 2 環境需求](https://v2.tauri.app/start/prerequisites/)。

Rust 版本、Clippy 與 rustfmt 由 [rust-toolchain.toml](rust-toolchain.toml) 固定；rustup 會自動選用。

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
