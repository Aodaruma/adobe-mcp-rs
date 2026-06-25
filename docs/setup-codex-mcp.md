# adobe-mcp-rs セットアップ手順（Codex MCP設定込み）

- 最終更新: 2026-06-25
- 対象OS: Windows / macOS
- 対象: Rust版 `ae-mcp` / `pr-mcp` / `ps-mcp` / `ai-mcp` を Codex のカスタムMCPサーバーとして使う

## 1. 前提

1. Adobe After Effects（2022以降推奨）、Premiere Pro、Photoshop、Illustrator のうち操作対象のアプリ
2. Rust（stable）と Cargo
3. After Effects は `mcp-bridge-auto.jsx`、Premiere Pro / Photoshop は UXP bridge panel、Illustrator は CEP bridge panel を開けること
4. Codex CLI もしくは Codex IDE Extension が利用可能であること

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
3. Codex 側で `aftereffects` サーバーが有効であることを確認
4. Codex から `run-script` (`script=listCompositions`) を実行
5. 続けて `get-results` を実行し、JSON結果が返ることを確認

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

### 6.1 Premiere Pro / Photoshop の UXP bridge と Illustrator CEP bridge

Windows installer を使う場合は、MSI 本体のファイル配置後、host integration の事前確認ウィンドウで After Effects / Premiere Pro / Photoshop / Illustrator / Codex config の配置対象を選択できます。各項目には現在の登録バージョンと新しいバージョンが表示され、完了時にも結果一覧が表示されます。

Premiere Pro:

1. Adobe UXP Developer Tool で `src/premiere/uxp/mcp-bridge-premiere/manifest.json` を読み込む
2. Premiere Pro で `Window > UXP Plugins > Premiere MCP Bridge` を開く
3. `Auto-run commands` を ON

Photoshop:

1. Adobe UXP Developer Tool で `src/photoshop/uxp/mcp-bridge-photoshop/manifest.json` を読み込む
2. Photoshop の Plugins menu から `Photoshop MCP Bridge` を開く
3. `Auto-run commands` を ON

Illustrator:

1. `src/illustrator/cep/mcp-bridge-illustrator` を CEP extensions directory に配置
2. Illustrator で `Window > Extensions > Illustrator MCP Bridge` を開く
3. `Auto-run commands` を ON

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

## 7. daemon 常駐化を使う場合

Windows は `autostart`、macOS は `service` を使います。

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
