# adobe-mcp-rs セットアップ手順（Codex MCP設定込み）

- 最終更新: 2026-07-15
- 対象OS: Windows / macOS
- 対象: Rust版 `ae-mcp` / `pr-mcp` / `ps-mcp` / `ai-mcp` を Codex のカスタムMCPサーバーとして使う

## 1. 前提

1. 操作対象の Adobe host。repository の manifest 上の最小 version は Premiere Pro UXP 25.6、Premiere Pro CEP fallback 24.0、Photoshop UXP 23.3、Illustrator CEP 24.0。After Effects は 2022 以降を推奨
2. Rust（stable）と Cargo
3. After Effects は ScriptUI / ExtendScript の `mcp-bridge-auto.jsx`、Premiere Pro は UXP（CEP fallback あり）、Photoshop は UXP、Illustrator は CEP / ExtendScript の bridge panel を開けること
4. Codex CLI もしくは Codex IDE Extension が利用可能であること

現在の状態は After Effects が **Primary**、Premiere Pro / Photoshop / Illustrator が **Experimental** です。Experimental host は binary と最小 MCP surface を実装済みですが、実機 E2E、配布、runtime compatibility、broker / service の同等性が未完成です。詳しい基準と制約は [Adobe host support status and roadmap](adobe-host-roadmap.md) を参照してください。

## 2. Rustバイナリをビルド

リポジトリルートで実行:

```bash
cargo build --release -p ae-mcp
cargo build --release -p pr-mcp
cargo build --release -p ps-mcp
cargo build --release -p ai-mcp
```

生成物:
- Windows: `target/release/ae-mcp.exe`
- macOS: `target/release/ae-mcp`
- Premiere Pro: `target/release/pr-mcp(.exe)`
- Photoshop: `target/release/ps-mcp(.exe)`
- Illustrator: `target/release/ai-mcp(.exe)`

## 3. After Effectsブリッジパネルを導入（npm不要）

このプロジェクトは AE 連携に `mcp-bridge-auto.jsx`（ScriptUI Panel）を使います。

### 3.1 推奨（シェルスクリプト）

Windows（PowerShell）:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-bridge.ps1
```

`-AfterEffectsPath` 未指定時は、検出した `Adobe After Effects <YEAR>` すべてにインストールされます。

macOS（bash）:

```bash
bash ./scripts/install-bridge.sh
```

`--ae-path` 未指定時は、検出した `/Applications/Adobe After Effects <YEAR>` すべてにインストールされます。

インストール先を手動指定する場合:

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-bridge.ps1 -AfterEffectsPath "C:\Program Files\Adobe\Adobe After Effects 2026"
```

macOS:

```bash
bash ./scripts/install-bridge.sh --ae-path "/Applications/Adobe After Effects 2026"
```

### 3.2 手動配置（必要時）

- `src/scripts/mcp-bridge-auto.jsx` を After Effects の ScriptUI Panels に配置
  - Windows: `C:\Program Files\Adobe\Adobe After Effects <VERSION>\Support Files\Scripts\ScriptUI Panels\`
  - macOS: `/Applications/Adobe After Effects <VERSION>/Scripts/ScriptUI Panels/`

After Effects 側で:
1. Scripting & Expressions の「Allow Scripts to Write Files and Access Network」を有効化
2. 再起動
3. `Window > mcp-bridge-auto.jsx` を開く
4. `Auto-run commands` を ON

別 terminal で AE broker を起動します。`serve-stdio` からの instance routing、待機、retained result 取得にはこの process が必要です。

```powershell
.\target\release\ae-mcp.exe serve-daemon
```

常駐化する場合は「7. After Effects daemon の常駐化」を参照してください。

## 4. CodexにMCPサーバーを登録（推奨: CLI）

OpenAI公式ドキュメントの `codex mcp add ... -- <stdio command>` 形式に合わせます。

## 4.1 Windows例

```powershell
codex mcp add aftereffects -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\ae-mcp.exe" serve-stdio
codex mcp add premiere -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\pr-mcp.exe" serve-stdio
codex mcp add photoshop -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\ps-mcp.exe" serve-stdio
codex mcp add illustrator -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\ai-mcp.exe" serve-stdio
```

## 4.2 macOS例

```bash
codex mcp add aftereffects -- /Users/<YOU>/path/adobe-mcp-rs/target/release/ae-mcp serve-stdio
codex mcp add premiere -- /Users/<YOU>/path/adobe-mcp-rs/target/release/pr-mcp serve-stdio
codex mcp add photoshop -- /Users/<YOU>/path/adobe-mcp-rs/target/release/ps-mcp serve-stdio
codex mcp add illustrator -- /Users/<YOU>/path/adobe-mcp-rs/target/release/ai-mcp serve-stdio
```

登録確認:

```bash
codex mcp list
```

TUI上でも `/mcp` で有効サーバーを確認できます。

## 5. `~/.codex/config.toml` へ手動設定する場合

`codex mcp add` を使わず直接書く場合は、以下のように設定します。

### 5.1 Windows例

```toml
[mcp_servers.aftereffects]
command = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs\\target\\release\\ae-mcp.exe"
args = ["serve-stdio"]
cwd = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true

[mcp_servers.premiere]
command = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs\\target\\release\\pr-mcp.exe"
args = ["serve-stdio"]
cwd = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true

[mcp_servers.photoshop]
command = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs\\target\\release\\ps-mcp.exe"
args = ["serve-stdio"]
cwd = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true

[mcp_servers.illustrator]
command = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs\\target\\release\\ai-mcp.exe"
args = ["serve-stdio"]
cwd = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true
```

### 5.2 macOS例

```toml
[mcp_servers.aftereffects]
command = "/Users/<YOU>/path/adobe-mcp-rs/target/release/ae-mcp"
args = ["serve-stdio"]
cwd = "/Users/<YOU>/path/adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true

[mcp_servers.premiere]
command = "/Users/<YOU>/path/adobe-mcp-rs/target/release/pr-mcp"
args = ["serve-stdio"]
cwd = "/Users/<YOU>/path/adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true

[mcp_servers.photoshop]
command = "/Users/<YOU>/path/adobe-mcp-rs/target/release/ps-mcp"
args = ["serve-stdio"]
cwd = "/Users/<YOU>/path/adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true

[mcp_servers.illustrator]
command = "/Users/<YOU>/path/adobe-mcp-rs/target/release/ai-mcp"
args = ["serve-stdio"]
cwd = "/Users/<YOU>/path/adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true
```

## 6. 動作確認（最短）

1. After Effects を起動
2. `Window > mcp-bridge-auto.jsx` を開き、`Auto-run commands` を ON
3. `ae-mcp serve-daemon` を起動
4. Codex 側で `aftereffects` サーバーが有効であることを確認
5. Codex から `list-ae-instances` を実行し、対象 instance と version を確認
6. `run-bridge-test` を実行し、bridge 結果が返ることを確認

`health` は binary 起動と bridge root の表示だけを確認し、Adobe host 内での実行成功までは確認しません。

補足:
- ブリッジファイルは `~/Documents/ae-mcp-bridge/` に作成されます
  - `ae_command.json`
  - `ae_mcp_result.json`
- Premiere Pro は `~/Documents/pr-mcp-bridge/` を使います
  - `pr_command.json`
  - `pr_mcp_result.json`
- Photoshop は `~/Documents/ps-mcp-bridge/` を使います
  - `ps_command.json`
  - `ps_mcp_result.json`
- Illustrator は `~/Documents/ai-mcp-bridge/` を使います
  - `ai_command.json`
  - `ai_mcp_result.json`

root 直下の command / result は compatibility 用です。複数 instance の routing では `instances/<instanceId>/heartbeat.json` と host 別 command / result、retained result では `registry/<requestId>.json` も使います。

### 6.1 Premiere Pro / Photoshop の UXP bridge と Illustrator CEP bridge

Windows installer を使う場合は、MSI の Custom Setup 画面で After Effects / Premiere Pro / Photoshop / Illustrator の bridge component を選択できます。MSI 本体のファイル配置後、選択された host integration と Codex config の更新は非表示の custom action として実行されるため、通常は別の PowerShell ウィンドウは表示されません。

host integration の結果を確認したい場合は、`C:\ProgramData\AfterEffectsMcp\install-bridge-installer.log` と `C:\ProgramData\AfterEffectsMcp\install-report.json` を確認してください。MSI から host integration 起動処理まで到達しているか、どの component が配置または skipped になったかを確認できます。

Premiere Pro:

1. Premiere Pro 25.6+ では Developer Mode を有効にし、Adobe UXP Developer Tool で `src/premiere/uxp/mcp-bridge-premiere/manifest.json` を読み込む。24.x の CEP は fallback として扱う
2. Premiere Pro で `Window > UXP Plugins > Premiere MCP Bridge` を開く
3. `Auto-run commands` を ON

Photoshop:

1. Photoshop 23.3+ で Adobe UXP Developer Tool から `src/photoshop/uxp/mcp-bridge-photoshop/manifest.json` を読み込む
2. Photoshop の Plugins menu から `Photoshop MCP Bridge` を開く
3. `Auto-run commands` を ON

Illustrator:

1. Illustrator 24.0+ で `src/illustrator/cep/mcp-bridge-illustrator` を CEP extensions directory に配置。unsigned local extension は CEP debug mode が必要な場合がある
2. Illustrator で `Window > Extensions > Illustrator MCP Bridge` を開く
3. `Auto-run commands` を ON

Premiere Pro / Photoshop / Illustrator は MCP stdio server が file bridge を直接操作します。各 `serve-daemon` は現在 request broker ではなく、通常の MCP 操作には起動不要です。

### 6.2 LLM運用時の推奨プロンプト（重要）

LLMがMCPを自動実行する場合は、まず非対話モードを前提にしてください。

- 通常（自動実行）:
  - `interactive=false` を使う（既定）
  - `suppressDialogs=true` を維持
  - 保存系は必ず `saveAsPath/filePath/path` を渡す
  - `closeOption` は `SAVE_CHANGES` または `DO_NOT_SAVE_CHANGES` を使う
- ユーザーに操作を渡す場合のみ:
  - `interactive=true` を指定（ダイアログ表示を許可）
  - `PROMPT_TO_SAVE_CHANGES` や未保存時の Save As ダイアログを許可

プロンプト例（LLM向け運用ルール）:

```text
After Effects MCP を使う際は、通常は non-interactive で実行すること。
- interactive=false（default）
- suppressDialogs=true
- 保存が必要な操作では saveAsPath/filePath/path を必ず明示
- closeOption は SAVE_CHANGES または DO_NOT_SAVE_CHANGES を使い、PROMPT は使わない

ユーザー操作に引き継ぐときだけ interactive=true を使ってダイアログ表示を許可する。
```

## 7. After Effects daemon の常駐化

After Effects broker を常駐させる場合、Windows は `autostart`、macOS は `service` を使います。Premiere Pro / Photoshop / Illustrator の同名 command は現在 heartbeat process の管理に留まるため、この手順の対象外です。

### 7.1 Windows

```powershell
.\target\release\ae-mcp.exe autostart install
.\target\release\ae-mcp.exe autostart status
.\target\release\ae-mcp.exe autostart start
.\target\release\ae-mcp.exe autostart stop
.\target\release\ae-mcp.exe autostart uninstall
```

### 7.2 macOS

```bash
./target/release/ae-mcp service install
./target/release/ae-mcp service status
./target/release/ae-mcp service start
./target/release/ae-mcp service stop
./target/release/ae-mcp service uninstall
```

## 8. よくあるトラブル

1. `get-results` が stale warning を返す
- AE側パネルが閉じているか、`Auto-run commands` が OFF の可能性があります。

2. Codexからサーバーが見えない
- `codex mcp list` で登録状態確認
- `~/.codex/config.toml` の `command` 絶対パスを確認
- バイナリ再ビルド後はパスが変わっていないか確認

3. パネルは動いているのに結果が返らない
- `~/Documents/ae-mcp-bridge/ae_command.json` の `status` が `pending/running/completed/error` のどこで止まっているか確認

## 9. 参考（公式）

- Codex MCP設定: <https://developers.openai.com/codex/mcp>
- Docs MCP（Codex設定例）: <https://developers.openai.com/learn/docs-mcp>
