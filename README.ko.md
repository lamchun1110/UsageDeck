<p align="center">
  <img src="assets/usagedeck-banner.png" alt="UsageDeck 로고" width="560">
</p>

<h1 align="center">UsageDeck</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a> · 한국어
</p>

<p align="center">
  <b>구독 중인 모든 AI 코딩 도구의 잔여 한도를 하나의 패널에서.</b>
</p>

<p align="center">
  <a href="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml"><img src="https://github.com/lamchun1110/UsageDeck/actions/workflows/ci.yml/badge.svg" alt="CI 상태"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases/latest"><img src="https://img.shields.io/github/v/release/lamchun1110/UsageDeck" alt="최신 릴리스"></a>
  <a href="https://github.com/lamchun1110/UsageDeck/releases"><img src="https://img.shields.io/github/downloads/lamchun1110/UsageDeck/total" alt="총 다운로드 수"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT 라이선스"></a>
</p>

코딩 어시스턴트 열세 개, 결제 대시보드 열세 개, 그리고 "한도를 모두 사용했습니다"라는 말의 열세 가지 표현. UsageDeck는 이미 컴퓨터에 저장된 인증 정보를 그대로 읽어, 정말 중요한 세 가지 질문에만 답하는 작은 데스크톱 앱입니다. 얼마나 남았는지, 언제 초기화되는지, 지금까지 얼마를 썼는지.

트레이나 메뉴 막대에 상주합니다. 만들 계정도, 로그인할 것도 없습니다.

## OpenQuota에서 오신 분들

UsageDeck은 OpenQuota 포크로 시작해 지금은 독립 프로젝트가 되었습니다. 데이터도 함께 가져옵니다: 첫 실행 시 UsageDeck은 기존 OpenQuota 설치에서 설정·사용 기록·가격 캐시를 자동으로 옮기고, OS 자격 증명 저장소에 저장된 API 키는 UsageDeck 고유 항목으로 이전합니다. 기존 위치에서 무언가를 삭제하지는 않습니다. `~/.config/openquota/<provider>.json`에 저장된 키도 계속 읽으며, 새 키는 `~/.config/usagedeck/`에 저장됩니다.

## 설치

[최신 릴리스](https://github.com/lamchun1110/UsageDeck/releases/latest)에서 사용 중인 플랫폼에 맞는 파일을 내려받으세요.

| 플랫폼  | 파일                                     | 비고                                  |
| ------- | ---------------------------------------- | ------------------------------------- |
| Windows | `_x64-setup.exe` 또는 `_arm64-setup.exe` | x64 및 ARM64                          |
| macOS   | `_universal.dmg`                         | Apple Silicon 및 Intel, macOS 11 이상 |
| Linux   | `.AppImage` 또는 `.deb`                  | x64 및 ARM64                          |

앱은 스스로 업데이트합니다. 업데이트 페이로드는 프로젝트 자체의 업데이터 키로 서명되며, 이는 운영 체제의 패키지 서명과는 별개입니다.

> [!IMPORTANT]
> 이 빌드는 네이티브 서명이 기본적으로 꺼져 있습니다. Windows 설치 프로그램은 Authenticode 서명이 되지 않고, macOS 앱은 애드혹 서명만 있으며 Apple 공증도 받지 않았습니다. 따라서 SmartScreen이나 Gatekeeper가 확인을 요구할 수 있고, macOS에서는 '개인 정보 보호 및 보안'에서 수동으로 허용해야 할 수 있습니다. 반드시 이 저장소의 릴리스 페이지에서만 내려받으세요. 각 릴리스 노트에 해당 릴리스의 정확한 서명 상태가 명시되어 있습니다.

## 추적하는 항목

| 서비스                                            | 인증 정보 | 확인할 수 있는 내용                                            |
| ------------------------------------------------- | --------- | -------------------------------------------------------------- |
| **[Claude Code](docs/providers/claude.md)**       | 로컬      | 다중 계정, 세션·주간 한도, 모델별 사용량, 토큰 기록, 예상 비용 |
| **[Codex](docs/providers/codex.md)**              | 로컬      | 세션·주간 한도, 크레딧, 토큰 기록, 모델 분포, 예상 비용        |
| **[Command Code](docs/providers/commandcode.md)** | 로컬      | 세션·주간·월간 한도 및 추가 크레딧                             |
| **[Cursor](docs/providers/cursor.md)**            | 로컬      | 전체·Auto·API 사용량, 크레딧, 토큰 기록, 예상 비용             |
| **[Antigravity](docs/providers/antigravity.md)**  | 로컬      | Gemini와 Claude가 공유하는 할당량                              |
| **[Copilot](docs/providers/copilot.md)**          | 로컬      | 프리미엄 요청, 추가 사용량, 채팅·완성 한도, 조직 청구          |
| **[Devin](docs/providers/devin.md)**              | 로컬      | 일간·주간 한도, 초기화 시간, 추가 사용량 잔액                  |
| **[Grok](docs/providers/grok.md)**                | 로컬      | 주간 할당량, 추가 사용량 상태, 토큰 기록, 예상 비용            |
| **[OpenCode](docs/providers/opencode.md)**        | 로컬      | Go의 세션·주간·월간 지출 한도 및 로컬 사용 기록                |
| **[OpenRouter](docs/providers/openrouter.md)**    | API 키    | 크레딧 잔액과 일간·주간·월간 지출                              |
| **[Z.ai](docs/providers/zai.md)**                 | API 키    | GLM Coding Plan의 세션·주간·웹 검색 한도                       |
| **[Kimi](docs/providers/kimi.md)**                | API 키    | Kimi Code의 세션·주간 한도 (도메인 선택 가능)                  |
| **[MiniMax](docs/providers/minimax.md)**          | API 키    | Token Plan의 세션·주간 한도                                    |

**로컬** 서비스는 CLI나 편집기가 이미 만들어 둔 로그인을 그대로 사용하므로 따로 설정할 것이 없습니다. **API 키** 서비스는 '사용자 지정'에서 키를 한 번 붙여 넣어야 하며, 키는 설정 파일이 아니라 운영 체제의 자격 증명 저장소에 바로 저장됩니다. Codex의 구독 한도는 ChatGPT 로그인이 필요하며 API 키만 사용하는 환경에서는 표시되지 않습니다.

## 사용 경험

- **트레이 팝업 또는 플로팅 창.** 잠깐 확인하고 닫거나, 보조 모니터에 계속 띄워 두세요.
- **중요한 값 고정.** 어떤 지표든 트레이나 macOS 메뉴 막대로 올릴 수 있습니다.
- **사용량 또는 잔여량.** 익숙한 방식으로 표시하세요.
- **소진 속도.** 한도가 실제로 바닥나기 전에, 지금 속도로 다음 초기화까지 버틸 수 있는지 알려 줍니다.
- **기록.** 오늘, 어제, 그리고 최근 30일의 토큰 사용량과 예상 비용.
- **원하는 대로 배치.** 서비스와 지표 순서 변경, 행 숨기기, 섹션 접기.
- **방해하지 않음.** 로그인 시 실행, 전역 단축키, 시스템 테마 따르기.

모든 동작은 사용자의 컴퓨터에서 이루어집니다. 계정도, 백엔드도, 분석도, 텔레메트리도 없습니다.

## 소스에서 빌드하기

Node.js 22 이상, pnpm 11.11.0, 안정 버전 Rust 툴체인, 그리고 사용하는 플랫폼의 [Tauri 2 사전 요구 사항](https://v2.tauri.app/start/prerequisites/)이 필요합니다.

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm tauri dev
```

풀 리퀘스트를 보내기 전에 포매팅, 린트, 타입, 계약, 두 테스트 스위트를 모두 포함한 전체 검사를 실행하세요.

```sh
corepack pnpm verify
```

현재 플랫폼용 패키지 만들기:

```sh
corepack pnpm build:installer             # Windows
corepack pnpm build:linux                 # Linux
corepack pnpm tauri build --bundles dmg   # macOS
```

릴리스 및 서명 요구 사항은 [docs/releasing.md](docs/releasing.md)에 있습니다.

## 기여하기

Issue와 Pull Request를 환영합니다. 먼저 [CONTRIBUTING.md](CONTRIBUTING.md)를 읽어 주세요. 보안 문제는 공개 Issue 대신 [SECURITY.md](SECURITY.md)의 안내에 따라 비공개로 신고해 주세요.

## 계보

먼저 Robin Ebers가 macOS용으로 만든 [OpenUsage](https://github.com/robinebers/openusage)가 있었고, 이어서 deviffyy의 [OpenQuota](https://github.com/deviffyy/OpenQuota)가 그 아이디어를 Tauri 기반의 Windows, Linux, macOS 지원 앱으로 다시 만들었습니다. UsageDeck은 해당 프로젝트의 포크로 시작해 자체적인 정체성·릴리스 인프라·로드맵을 갖춘 독립 제품으로 성장했습니다. 최초 디자인과 초기 코드 대부분의 공로는 이 두 프로젝트에 있습니다. 감사합니다.

## 라이선스

[MIT](LICENSE)
