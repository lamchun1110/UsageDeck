<p align="center">
  <img src="assets/usagedeck-banner.png" alt="UsageDeck ロゴ" width="560">
</p>

<h1 align="center">UsageDeck</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.zh-CN.md">简体中文</a> · 日本語 · <a href="README.ko.md">한국어</a>
</p>

<p align="center">
  <b>契約している AI コーディングツールの残量を、ひとつのパネルに。</b>
</p>

<p align="center">
  <a href="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml"><img src="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml/badge.svg" alt="CI ステータス"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases/latest"><img src="https://img.shields.io/github/v/release/lamchun1110/UsageDeck" alt="最新リリース"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases"><img src="https://img.shields.io/github/downloads/lamchun1110/UsageDeck/total" alt="総ダウンロード数"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT ライセンス"></a>
</p>

UsageDeck は、Windows、Linux、macOS に対応したオープンソースかつプライバシー重視のデスクトップダッシュボードです。13 種類の AI コーディングアシスタントの使用上限、リセット時刻、トークン履歴、推定費用を一か所で確認できます。

トレイまたはメニューバーに常駐し、マシンに保存済みの認証情報を再利用します。すべてローカルで動作し、UsageDeck アカウント、バックエンド、分析、テレメトリはありません。

使用量の更新は、設定または認証した第三者のサービスに直接接続します。費用推定に使う公開モデル価格は GitHub、models.dev、openrouter.ai から約 1 日ごとに取得し、この通信に認証情報や使用履歴は含みません。Session Kickstart を有効にすると CLI が小さなプロンプトを送信し、プロバイダーの利用枠を消費します。Codex のリセット利用には確認が必要です。詳しくは[プライバシーポリシー](https://usagedeck.app/privacy/)をご覧ください。

表示できる指標はプロバイダーとアカウントのプランによって異なります。

## OpenQuota からの移行

UsageDeck は OpenQuota のフォークから独立したプロジェクトです。初回起動時に、既存の OpenQuota から設定・使用履歴・価格キャッシュ・Antigravity のローカルデータをコピーし、元のファイルは保持します。API キーは OS の資格情報ストアの `UsageDeck` エントリーへ移し、新しいキーの保存に成功した場合のみ古い資格情報エントリーを削除します。移行に失敗したキーは「カスタマイズ」で再登録してください。`~/.config/openquota/{kimi,minimax,zai}.json` と `~/.config/usagedeck/{kimi,minimax,zai}.json` は外部キーの読み込み元として引き続き使えます。アプリで保存したキーは OS の資格情報ストアに入ります。OpenRouter は `~/.config/openrouter/key.json` も読み込みます。

## インストール

[最新リリース](https://github.com/lamchun1110/UsageDeck/releases/latest)からお使いのプラットフォーム向けのファイルを取得してください。

| プラットフォーム | ファイル                                   | 備考                                  |
| ---------------- | ------------------------------------------ | ------------------------------------- |
| Windows          | `_x64-setup.exe` または `_arm64-setup.exe` | x64 と ARM64                          |
| macOS            | `_universal.dmg`                           | Intel と Apple Silicon、macOS 11 以降 |
| Linux            | `.AppImage`、`.deb` または `.rpm`          | x64 と ARM64                          |

UsageDeck は更新を自動確認できます。Windows、macOS、Linux AppImage はアプリ内から更新をインストールできます。Linux の `.deb` と `.rpm` はリリースページを開くので、新しいパッケージをダウンロードしてインストールしてください。更新ファイルはプロジェクトのアップデーター鍵で署名し、OS のネイティブ署名とは別に検証します。

### リリース署名

- **Windows:** Authenticode 署名の有無はリリースの設定によります。対象リリースの署名情報を確認してください。未署名のインストーラーでは SmartScreen が警告する場合があります。
- **macOS:** Developer ID 署名と Apple 公証はリリースごとに有効化します。無効の場合はアドホック署名で、公証はありません。
- **Linux:** GPG 署名を有効にしたリリースには `<file>.asc`、`SHA256SUMS`、クリア署名文書 `SHA256SUMS.asc`、公開鍵 `usagedeck-gpg-public.asc` が含まれます。RPM には埋め込み署名も付きます。

Linux ダウンロードを検証するには:

```bash
gpg --import usagedeck-gpg-public.asc
gpg --verify UsageDeck.AppImage.asc UsageDeck.AppImage
# 上のファイル名は、リリースページの実際のファイル名に置き換えてください。
```

> [!IMPORTANT]
> このリポジトリのリリースページからダウンロードし、対象リリースの署名情報と提供ファイルを確認してください。ネイティブ署名のないビルドでは Windows SmartScreen や macOS Gatekeeper が警告する場合があります。検証コマンドは [docs/releasing.md](docs/releasing.md) にあります。

## 取得できる情報

| プロバイダー                                      | 認証情報 | 取得内容                                                                                  |
| ------------------------------------------------- | -------- | ----------------------------------------------------------------------------------------- |
| **[Claude Code](docs/providers/claude.md)**       | ローカル | 複数アカウント、セッションと週次の上限、ボーナスリセット数と有効期限、トークン履歴、費用  |
| **[Codex](docs/providers/codex.md)**              | ローカル | セッションと週次の上限、リセットクレジットと有効期限、トークン履歴、モデル内訳、費用      |
| **[Command Code](docs/providers/commandcode.md)** | ローカル | セッション・週次・月次の上限と追加クレジット                                              |
| **[Cursor](docs/providers/cursor.md)**            | ローカル | 全体・Auto・API の使用量、クレジット、トークン履歴、費用                                  |
| **[Antigravity](docs/providers/antigravity.md)**  | ローカル | Gemini と Claude で共有されるクォータ                                                     |
| **[Copilot](docs/providers/copilot.md)**          | ローカル | プレミアムリクエストまたは AI クレジット、追加使用量、チャットと補完の上限、組織課金      |
| **[Devin](docs/providers/devin.md)**              | ローカル | 日次と週次の上限、リセット時刻、追加使用量の残高                                          |
| **[Grok](docs/providers/grok.md)**                | ローカル | 週次の割当、追加使用量の状態、トークン履歴、費用                                          |
| **[OpenCode](docs/providers/opencode.md)**        | ローカル | 複数の Go アカウント、セッション・週次・月次の上限、ローカル使用履歴と推定費用            |
| **[OpenRouter](docs/providers/openrouter.md)**    | API キー | クレジット、残高、日次・週次・月次の費用、キー上限                                        |
| **[Z.ai](docs/providers/zai.md)**                 | API キー | GLM Coding Plan のセッション・週次・ウェブ検索の上限、個人 ZCode リセットカードと有効期限 |
| **[Kimi](docs/providers/kimi.md)**                | API キー | Kimi Code のセッションと週次の上限（ドメインを選択可能）                                  |
| **[MiniMax](docs/providers/minimax.md)**          | API キー | Token Plan のセッションと週次の上限                                                       |

**ローカル**のプロバイダーは、CLI やエディターが作成済みのログインをそのまま利用するため、設定は不要です。**API キー**のプロバイダーは「カスタマイズ」で一度キーを貼り付ける必要があります。キーは設定ファイルではなく OS の資格情報ストアに直接保存されます。Codex のサブスクリプション上限には ChatGPT のログインが必要で、API キーのみの環境では表示されません。

## 使い心地

- **複数アカウント。** Claude のプロファイル、OpenCode Go のデータディレクトリ、OpenRouter・Z.ai・Kimi・MiniMax の名前付き API キーアカウントを個別カードで表示します。名前付き API アカウントは再起動せず追加できます。
- **リセットの期限通知。** Claude、Codex、個人 ZCode の利用可能数と既知の有効期限を表示し、1・24・48・168 時間前に通知できます。リセット行が非表示でも通知します。Z.ai には同じ個人 ZCode アカウントのログインと API キーが必要です。
- **Session Kickstart。** 任意で、ローリング利用枠の期限後に小さな CLI プロンプトを実行し、新しい枠を開始します。各プロンプトは利用枠を消費し、組み込みまたはカスタムコマンドを使えます。
- **Linux のトレイ対応。** 利用可能なシステムトレイがない場合は独立ウィンドウを使います。
- **トレイのポップアップ、またはフローティングウィンドウ。** さっと確認して閉じるか、サブディスプレイに開いたままにするか。
- **重要な値をピン留め。** 対応するメトリクスをトレイや macOS のメニューバーに表示できます。
- **使用量と残量。** 考えやすいほうで表示できます。
- **ペース配分。** 上限に達してしまう前に、現在のペースで次のリセットまで持つかどうかを知らせます。
- **履歴。** 今日・昨日・直近 30 日のトークン使用量と概算費用。
- **手遅れになる前に通知。** 残量が少なくなったとき、かなり際どいとき、現在のペースではリセット前に使い切るときに、デスクトップ通知を出せます（任意）。
- **自由なレイアウト。** プロバイダーとメトリクスの並べ替え、行の非表示、セクションの折りたたみ。
- **見た目も自由に。** ライト・ダーク・システム追従、5 種類のアクセントカラー、コンパクト表示、12 / 24 時間表記。
- **カードを共有。** 任意のプロバイダーのパネルを画像としてコピーし、そのまま貼り付けられます。
- **あなたの言語で。** English・繁體中文・简体中文・日本語・한국어、またはシステムの設定に追従。
- **邪魔をしない。** ログイン時に起動、グローバルショートカット、システムテーマへの追従。

データは端末に保存します。UsageDeck アカウント、プロジェクト運営のバックエンド、分析、テレメトリはありません。使用量と価格の更新は第三者のサービスに直接接続します。

アカウント設定、リセット通知、更新については[使い方ガイド（英語）](docs/usage.md)をご覧ください。

## ソースからのビルド

Node.js 24 以降、pnpm 11.11.0、rustup でインストールした Rust、そしてお使いのプラットフォーム向けの [Tauri 2 の前提条件](https://v2.tauri.app/start/prerequisites/)が必要です。

Rust のバージョン、Clippy、rustfmt は [rust-toolchain.toml](rust-toolchain.toml) で固定され、rustup が自動選択します。

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm tauri dev
```

プルリクエストを送る前に、フォーマット・Lint・型・コントラクト・両方のテストを含む全チェックを実行してください。

```sh
corepack pnpm verify
```

現在のプラットフォーム向けのパッケージング:

```sh
corepack pnpm build:installer             # Windows
corepack pnpm build:linux                 # Linux
corepack pnpm tauri build --bundles dmg   # macOS
```

リリースと署名の要件は [docs/releasing.md](docs/releasing.md) にあります。

## コントリビューション

Issue と Pull Request を歓迎します。まず [CONTRIBUTING.md](CONTRIBUTING.md) をお読みください。セキュリティ上の問題は公開 Issue ではなく、[SECURITY.md](SECURITY.md) の手順に従って非公開で報告してください。

## 系譜

はじめに Robin Ebers による macOS 向けの [OpenUsage](https://github.com/robinebers/openusage) がありました。続いて deviffyy の [OpenQuota](https://github.com/deviffyy/OpenQuota) が、その発想を Tauri による Windows・Linux・macOS 対応アプリとして作り直しました。UsageDeck は同プロジェクトのフォークとして始まり、独自のアイデンティティ・リリース基盤・ロードマップを持つ独立製品へと成長しました。オリジナルのデザインと初期コードの大部分の功績は、この 2 つのプロジェクトに帰属します。ありがとうございます。

## ライセンス

[MIT](LICENSE)
