# adobe-mcp-rs セットアップ手順（Codex MCP設定込み）

- 最終更新: 2026-07-15
- 対象OS: Windows / macOS
- 対象: Rust版 `ae-mcp` / `pr-mcp` / `ps-mcp` / `ai-mcp` / `id-mcp` を Codex のカスタムMCPサーバーとして使う

## 1. 前提

1. 操作対象の Adobe host。repository の manifest 上の最小 version は Premiere Pro UXP 25.6、Premiere Pro CEP fallback 24.0、Photoshop UXP 23.3、Illustrator CEP 24.0。InDesignはfile I/Oを使うため18.5+をPoC対象とし、After Effects は 2022 以降を推奨
2. Rust（stable）と Cargo
3. After Effects は Startup ExtendScript bridge、Premiere Pro は UXP（CEP fallback あり）、Photoshop は UXP、Illustrator は CEP / ExtendScript bridge、InDesign は UXP Startup Script を利用できること
4. Codex CLI もしくは Codex IDE Extension が利用可能であること

現在の状態は After Effects が **Primary**、Premiere Pro / Photoshop / Illustrator / InDesign が **Experimental** です。Experimental host は binary と最小 MCP surface を実装済みですが、実機 E2E、配布、runtime compatibility、broker / service の同等性が未完成です。詳しい基準と制約は [Adobe host support status and roadmap](adobe-host-roadmap.md) を参照してください。

## 2. Rustバイナリをビルド

リポジトリルートで実行:

```bash
cargo build --release -p ae-mcp
cargo build --release -p pr-mcp
cargo build --release -p ps-mcp
cargo build --release -p ai-mcp
cargo build --release -p id-mcp
```

生成物:
- Windows: `target/release/ae-mcp.exe`
- macOS: `target/release/ae-mcp`
- Premiere Pro: `target/release/pr-mcp(.exe)`
- Photoshop: `target/release/ps-mcp(.exe)`
- Illustrator: `target/release/ai-mcp(.exe)`
- InDesign: `target/release/id-mcp(.exe)`

## 3. After Effects headless bridgeを導入（npm不要）

このプロジェクトはAE連携に `mcp-bridge-startup.jsx` と `mcp-bridge-auto.jsx` のExtendScript runtimeを使います。Startup bootstrapがruntimeを直接評価するため、workspaceやpanelのopen状態に依存しません。

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
- `src/scripts/mcp-bridge-startup.jsx` をAfter EffectsのStartupに配置
  - Windows: `C:\Program Files\Adobe\Adobe After Effects <VERSION>\Support Files\Scripts\Startup\`
  - macOS: `/Applications/Adobe After Effects <VERSION>/Scripts/Startup/`
- `src/scripts/mcp-bridge-shutdown.jsx` をAfter EffectsのShutdownに配置
  - Windows: `C:\Program Files\Adobe\Adobe After Effects <VERSION>\Support Files\Scripts\Shutdown\`
  - macOS: `/Applications/Adobe After Effects <VERSION>/Scripts/Shutdown/`

After Effects 側で:
1. Scripting & Expressions の「Allow Scripts to Write Files and Access Network」を有効化
2. 再起動

通常運用でpanelを開く操作や`Auto-run commands`は不要です。lifecycle APIと障害確認は [After Effects bridge lifecycle](after-effects-bridge-lifecycle.md) を参照してください。

別 terminal で AE broker を起動します。`serve-stdio` からの instance routing、待機、retained result 取得にはこの process が必要です。

```powershell
.\target\release\ae-mcp.exe serve-daemon
```

常駐化する場合は「8. host 別 daemon の常駐化」を参照してください。

## 4. CodexにMCPサーバーを登録（推奨: CLI）

OpenAI公式ドキュメントの `codex mcp add ... -- <stdio command>` 形式に合わせます。

## 4.1 Windows例

```powershell
codex mcp add aftereffects -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\ae-mcp.exe" serve-stdio
codex mcp add premiere -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\pr-mcp.exe" serve-stdio
codex mcp add photoshop -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\ps-mcp.exe" serve-stdio
codex mcp add illustrator -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\ai-mcp.exe" serve-stdio
codex mcp add indesign -- "C:\Users\<YOU>\path\adobe-mcp-rs\target\release\id-mcp.exe" serve-stdio
```

## 4.2 macOS例

```bash
codex mcp add aftereffects -- /Users/<YOU>/path/adobe-mcp-rs/target/release/ae-mcp serve-stdio
codex mcp add premiere -- /Users/<YOU>/path/adobe-mcp-rs/target/release/pr-mcp serve-stdio
codex mcp add photoshop -- /Users/<YOU>/path/adobe-mcp-rs/target/release/ps-mcp serve-stdio
codex mcp add illustrator -- /Users/<YOU>/path/adobe-mcp-rs/target/release/ai-mcp serve-stdio
codex mcp add indesign -- /Users/<YOU>/path/adobe-mcp-rs/target/release/id-mcp serve-stdio
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

[mcp_servers.indesign]
command = "C:\\Users\\<YOU>\\path\\adobe-mcp-rs\\target\\release\\id-mcp.exe"
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

[mcp_servers.indesign]
command = "/Users/<YOU>/path/adobe-mcp-rs/target/release/id-mcp"
args = ["serve-stdio"]
cwd = "/Users/<YOU>/path/adobe-mcp-rs"
startup_timeout_sec = 20
tool_timeout_sec = 120
enabled = true
```

## 6. After Effectsの公開MCP surface

`tools/list` が公開するAfter Effects Toolは次の9個です。

- `run-jsx`
- `run-jsx-file`
- `get-jsx-result`
- `list-ae-instances`
- `get-results`
- `get-help`
- `save-frame-png`
- `cleanup-preview-folder`
- `run-bridge-test`

`run-script`や個別のcomposition / effect / render queue / project操作Toolは、旧client向けの非公開互換dispatchです。旧Toolを直接呼ぶと実行結果とともに公開置換先が案内されますが、新しい設定・Prompt・確認手順では使用しません。`run-script`のallowlistと非同期direct-file動作は、`run-jsx`の明示的な`mode: "unsafe"`および同期daemon broker契約とは安全境界・完了条件が異なるため、現時点では再公開しません。

公開ToolのAE実行、`aftereffects://compositions` Resourceの読み取り、Promptが案内する操作は`serve-daemon` brokerを経由します。MCP Prompt自体は操作を実行せず、公開Toolまたは公開Resourceを使う手順を返します。

全Tool / Prompt / Resourceと非公開互換名の対応は [After Effects MCP public surface](after-effects-mcp-surface.md) を参照してください。

`run-jsx-file` は絶対パス、canonical allowed root、host別拡張子、UTF-8、サイズを検証します。`mode = "trusted"` は設定した path と SHA-256 の一致が必須です。設定例と旧configからの移行方法は [run-jsx-file の信頼ポリシー](script-file-security.md) を参照してください。`mode = "unsafe"` は sandbox を意味しません。

## 7. 動作確認（最短）

1. After Effects を起動
2. `ae-mcp serve-daemon` を起動
3. Codex 側で `aftereffects` サーバーが有効であることを確認
4. Codex から `list-ae-instances` を実行し、対象 instance、`extendscript-startup` runtime、versionを確認
5. `run-bridge-test` を実行し、bridge 結果が返ることを確認

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
- InDesign は `~/Documents/id-mcp-bridge/` を使います
  - `id_command.json`
  - `id_mcp_result.json`

root 直下の command / result は compatibility 用です。複数 instance の routing では `instances/<instanceId>/heartbeat.json` と host 別 command / result、retained result では `registry/<requestId>.json` も使います。

### 7.1 Premiere Pro / Photoshop / InDesign の UXP bridge と Illustrator CEP bridge

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

InDesign:

1. `src/indesign/uxp/mcp-bridge-indesign.idjs`をInDesignの`Scripts/Startup Scripts`へ配置する
2. InDesignを再起動する。panelやAuto-run toggleは不要
3. `id-mcp serve-daemon`を起動して`list-indesign-instances`と`run-bridge-test`を確認する
4. raw `run-script`は実機未検証PoCとして扱い、[InDesign MCP PoC](indesign-mcp.md)のE2E gateを通す

Premiere Pro / Photoshop / Illustrator も MCP stdio server から host 別 `serve-daemon` broker を経由します。host panel の準備後、対応する daemon を起動してください。

```powershell
.\target\release\pr-mcp.exe serve-daemon # 127.0.0.1:47656
.\target\release\ps-mcp.exe serve-daemon # 127.0.0.1:47657
.\target\release\ai-mcp.exe serve-daemon # 127.0.0.1:47658
.\target\release\id-mcp.exe serve-daemon # 127.0.0.1:47659
```

daemon 未起動時は stdio tool が接続エラーと起動コマンドを返します。timeout 後も `requestId` を `get-jsx-result` または `get-results` に渡すと、後から完了した結果を回収できます。root 直下の command/result file と `bridge` CLI は互換・診断用途です。

### 7.2 LLM運用時の推奨プロンプト（重要）

LLMがMCPを自動実行する場合は、まず非対話モードを前提にしてください。

- 通常（自動実行）:
  - 公開`run-jsx`には`interactive`引数がないため、dialogを開かないJSXを書く
  - pathは`code`または`args`内へ絶対pathで明示する
  - 公開`save-frame-png`では`suppressDialogs=true`（既定）を維持する
  - project lifecycleをJSXで実装する場合、保存先とclose方針をcode内で明示する
- ユーザーに操作を渡す場合のみ:
  - dialogを含む`run-jsx`を明示的なunsafe handoffとして扱う
  - 自動処理の継続を前提にしない

プロンプト例（LLM向け運用ルール）:

```text
After Effects MCP を使う際は、通常は non-interactive で実行すること。
- 公開 run-jsx に interactive 引数はない。dialog を開かない JSX を書く
- path は code または args 内へ絶対 path で明示する
- save-frame-png は suppressDialogs=true を維持する
- project lifecycle を実装するときは保存先と close 方針を code 内で明示する

ユーザー操作に引き継ぐときだけ、dialog を含む run-jsx を明示的な unsafe handoff として扱う。
```

## 8. host 別 daemon の常駐化

各 host broker を常駐させる場合、Windows は対象 binary の `autostart`、macOS は launchd を操作する `service` を使います。Windows Service は実装しておらず、Windows 版 CLI は `service` を公開しません。macOS 版 CLI は逆に `autostart` を公開しません。複数 host を利用する場合は binary ごとに登録します。既定 address は AE `127.0.0.1:47655`、Premiere `:47656`、Photoshop `:47657`、Illustrator `:47658`、InDesign `:47659` です。

### 8.1 Windows

```powershell
.\target\release\<host>-mcp.exe autostart install
.\target\release\<host>-mcp.exe autostart status
.\target\release\<host>-mcp.exe autostart start
.\target\release\<host>-mcp.exe autostart stop
.\target\release\<host>-mcp.exe autostart uninstall
```

- `install`: 現在のユーザーの `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` へ次回ログイン用コマンドを登録または更新する。daemon は起動しない
- `start`: daemon を即時起動する。同じ実行ファイルの daemon が稼働中なら二重起動せず `already running` を返す
- `status`: Run key の登録コマンドと PID ファイルを検証する。exe の移動後に登録が古い場合は `outdated` を表示する
- `stop`: PID ファイルに記録された実行ファイルと実プロセスを照合して停止する。移動前の exe が稼働中でも対象を取り違えない
- `uninstall`: Run key のみ削除する。稼働中 daemon も止める場合は先に `stop` を実行する

exe を移動・更新した後に旧 daemon が残っている場合、`start` は二重起動を避けるため失敗します。`stop`、`install`、`start` の順に実行してください。stale または壊れた PID ファイルは `start` / `stop` 時に除去されます。

Windows MSI は初回インストール時に autostart を勝手に有効化しません。ユーザーが既に登録済みの場合だけ upgrade 時に新しいインストール先へ Run key を修復し、通常 uninstall 時にはアンインストールを実行した現在ユーザーの daemon を停止して登録を削除します。別ユーザーの HKCU 登録は各ユーザーで `autostart stop` と `autostart uninstall` を実行してください。

### 8.2 macOS

```bash
./target/release/<host>-mcp service install
./target/release/<host>-mcp service status
./target/release/<host>-mcp service start
./target/release/<host>-mcp service stop
./target/release/<host>-mcp service uninstall
```

`service` はユーザーの `~/Library/LaunchAgents` に launchd plist を配置し、`install|start|status|stop|uninstall` を操作します。

## 9. よくあるトラブル

1. `get-results` が stale warning を返す
- AEのStartup bootstrapが未配置・未読込、またはfile/network accessが無効な可能性があります。

2. Codexからサーバーが見えない
- `codex mcp list` で登録状態確認
- `~/.codex/config.toml` の `command` 絶対パスを確認
- バイナリ再ビルド後はパスが変わっていないか確認

3. パネルは動いているのに結果が返らない
- `~/Documents/ae-mcp-bridge/ae_command.json` の `status` が `pending/running/completed/error` のどこで止まっているか確認

## 10. 参考（公式）

- Codex MCP設定: <https://developers.openai.com/codex/mcp>
- Docs MCP（Codex設定例）: <https://developers.openai.com/learn/docs-mcp>
