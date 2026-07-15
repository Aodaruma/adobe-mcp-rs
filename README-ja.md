# Adobe MCP Servers (Rust)

LLM から Adobe アプリをローカル自動操作するための、Rust 製 MCP サーバー群とホスト側ブリッジのプロジェクトです。

この repository は、After Effects 専用の `after-effects-mcp-rs` から、Adobe アプリ横断の `adobe-mcp-rs` へ育てる前提でリネームされています。現時点では After Effects 実装が最も進んでおり、Premiere Pro / Photoshop / Illustrator は実験的な実装が入っています。

- English: [README.md](README.md)

## 現状

最終コード照合日: 2026-07-15

| 対象アプリ | バイナリ | bridge runtime | 状態 | 現在の境界 |
|---|---|---|---|---|
| After Effects | `ae-mcp` | ScriptUI / ExtendScript JSX | **Primary** | panel、Auto-run、`serve-daemon` broker が必要 |
| Premiere Pro | `pr-mcp` | UXP 25.6+、CEP / ExtendScript 24.0+ fallback | **Experimental** | sequence / export の初期 surface。daemon は request broker ではない |
| Photoshop | `ps-mcp` | UXP 23.3+（API v2） | **Experimental** | 汎用実行と document / layer 読み取りの初期 surface |
| Illustrator | `ai-mcp` | CEP / ExtendScript 24.0+（CSXS 10） | **Experimental** | document / artboard / layer / export の初期 surface。runtime 配布の検証が必要 |

**Primary** は既定の運用経路が実装済み、**Experimental** は binary、bridge、最小 MCP surface はあるものの、実機 E2E、配布、runtime compatibility、broker / service の同等性のいずれかが未完成、**Planned** は利用可能な binary と bridge の組がまだない状態です。詳しい基準、runtime 制約、検証方法は [host 状態の source of truth](docs/adobe-host-roadmap.md) を参照してください。

## 現在の構成

workspace は、共通 Rust crate とアプリ別バイナリに分かれています。

| Path | 役割 |
|---|---|
| `crates/ae-mcp` | After Effects CLI、MCP stdio server、daemon、bridge command |
| `crates/pr-mcp` | Premiere Pro CLI、MCP stdio server |
| `crates/ps-mcp` | Photoshop CLI、MCP stdio server |
| `crates/ai-core` | Illustrator tool spec、prompt、allowlist script |
| `crates/ai-mcp` | Illustrator CLI、MCP stdio server |
| `crates/mcp-core` | 共通 config、MCP tool/prompt spec、bridge path default |
| `crates/bridge-core` | file bridge client、instance discovery、request registry、result retention |
| `crates/platform-service` | Windows/macOS service と autostart helper |
| `crates/pr-core` | Premiere Pro tool spec、prompt、allowlist script |
| `crates/ps-core` | Photoshop tool spec、help text、allowlist script |
| `src/scripts` | After Effects JSX bridge と helper script |
| `src/premiere/uxp` | Premiere Pro UXP bridge panel |
| `src/premiere/cep` | 旧 Premiere Pro CEP bridge fallback |
| `src/photoshop/uxp` | Photoshop UXP bridge panel |
| `src/illustrator/cep` | Illustrator CEP / ExtendScript bridge panel |

After Effects は `ae-mcp serve-daemon` をローカル broker として使います。bridge panel は `~/Documents/ae-mcp-bridge/instances/<instanceId>/` に登録され、MCP 呼び出しは対象 instance へ routing されます。結果は `requestId` で保持されます。

Premiere Pro は `~/Documents/pr-mcp-bridge` 以下で同じ file bridge パターンを使っています。UXP bridge が本線で、CEP bridge は fallback です。MCP stdio server が file bridge を直接操作します。

Photoshop は `~/Documents/ps-mcp-bridge` 以下で同じ file bridge パターンを使います。UXP bridge は任意 UXP code 実行と、document/layer 読み取り用の小さな allowlist script を提供します。MCP stdio server が file bridge を直接操作します。

Illustrator は `~/Documents/ai-mcp-bridge` 以下で同じ file bridge パターンを使います。CEP panel 上の ExtendScript で任意 JSX/ExtendScript と document/artboard/layer 読み取り用の allowlist script を提供します。Premiere Pro / Photoshop / Illustrator の `serve-daemon` は PID file と heartbeat log を維持するだけで request を仲介せず、通常の MCP 操作には不要です。Windows installer は、After Effects ScriptUI panel、Premiere / Photoshop UXP panel、Illustrator CEP panel、Codex MCP config を Custom Setup から配置できます。

## セットアップ

前提:

- Rust stable / Cargo
- 操作対象の Adobe アプリ
- UXP bridge を使う場合は Adobe UXP Developer Tool と、必要に応じて host 側 Developer Mode

全バイナリを build:

```powershell
cargo build --release
```

個別 build:

```powershell
cargo build --release -p ae-mcp
cargo build --release -p pr-mcp
cargo build --release -p ps-mcp
cargo build --release -p ai-mcp
```

### After Effects

bridge panel を配置します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-bridge.ps1
```

```bash
bash ./scripts/install-bridge.sh
```

After Effects 側:

1. `Allow Scripts to Write Files and Access Network` を有効化
2. After Effects を再起動
3. `Window > mcp-bridge-auto.jsx` を開く
4. `Auto-run commands` を ON

broker を起動:

```powershell
.\target\release\ae-mcp.exe serve-daemon
```

MCP server を登録:

```powershell
codex mcp add aftereffects -- "<ABSOLUTE_PATH>\target\release\ae-mcp.exe" serve-stdio
```

### Premiere Pro

build:

```powershell
cargo build --release -p pr-mcp
```

Adobe UXP Developer Tool で `src/premiere/uxp/mcp-bridge-premiere` を読み込み、Premiere Pro で `Window > UXP Plugins > Premiere MCP Bridge` を開いて `Auto-run commands` を ON にします。

MCP server を登録:

```powershell
codex mcp add premiere -- "<ABSOLUTE_PATH>\target\release\pr-mcp.exe" serve-stdio
```

### Photoshop

build:

```powershell
cargo build --release -p ps-mcp
```

Adobe UXP Developer Tool で `src/photoshop/uxp/mcp-bridge-photoshop` を読み込み、Photoshop の Plugins menu から `Photoshop MCP Bridge` を開いて `Auto-run commands` を ON にします。

MCP server を登録:

```powershell
codex mcp add photoshop -- "<ABSOLUTE_PATH>\target\release\ps-mcp.exe" serve-stdio
```

### Illustrator

build:

```powershell
cargo build --release -p ai-mcp
```

`src/illustrator/cep/mcp-bridge-illustrator` を CEP extensions directory に配置し、Illustrator で `Window > Extensions > Illustrator MCP Bridge` を開いて `Auto-run commands` を ON にします。

MCP server を登録:

```powershell
codex mcp add illustrator -- "<ABSOLUTE_PATH>\target\release\ai-mcp.exe" serve-stdio
```

## クイック確認

After Effects:

```powershell
.\target\release\ae-mcp.exe health
.\target\release\ae-mcp.exe serve-daemon
```

別 terminal:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-ae-instances","arguments":{}}}' | .\target\release\ae-mcp.exe serve-stdio
```

Premiere Pro:

```powershell
.\target\release\pr-mcp.exe health
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-premiere-instances","arguments":{}}}' | .\target\release\pr-mcp.exe serve-stdio
```

Photoshop:

```powershell
.\target\release\ps-mcp.exe health
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-photoshop-instances","arguments":{}}}' | .\target\release\ps-mcp.exe serve-stdio
```

Illustrator:

```powershell
.\target\release\ai-mcp.exe health
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-illustrator-instances","arguments":{}}}' | .\target\release\ai-mcp.exe serve-stdio
```

## MCP tool surface

After Effects:

- `run-jsx`
- `run-jsx-file`
- `get-jsx-result`
- `list-ae-instances`
- `get-results`
- `get-help`
- `save-frame-png`
- `cleanup-preview-folder`
- `run-bridge-test`

Premiere Pro:

- `run-jsx`
- `run-jsx-file`
- `run-script`
- `get-jsx-result`
- `get-results`
- `get-help`
- `list-premiere-instances`
- `run-bridge-test`

Photoshop:

- `run-jsx`
- `run-jsx-file`
- `run-script`
- `get-jsx-result`
- `get-results`
- `get-help`
- `list-photoshop-instances`
- `run-bridge-test`

Illustrator:

- `run-jsx`
- `run-jsx-file`
- `run-script`
- `get-jsx-result`
- `get-results`
- `get-help`
- `list-illustrator-instances`
- `run-bridge-test`

任意 code 実行では `mode: "unsafe"` と短い `description` を明示します。host 側 JavaScript/JSX は強い権限を持つため、MCP 呼び出し上も明示的に扱う方針です。

## 今後の拡張方針

次は、After Effects 前提を各アプリへコピーするのではなく、host 対応を明示的な設計要素にします。

1. host 名、bridge root、tool 名、実行ファイル名、help text、installer 挙動をまとめる host adapter 層を切り出す。
2. `heartbeat.json`、command/result file、instance metadata、capabilities、retained request record の bridge protocol を共通化する。
3. Premiere Pro を After Effects と同等の broker model に寄せるか、direct file-bridge として明確に分離する。
4. Photoshop UXP bridge は書き込み操作、modal execution policy、installer E2E を強化する。
5. Illustrator CEP bridge は export coverage、現行バージョンでの runtime 検証、署名、installer E2E を強化する。UXP は公開 host support が明確になるまで optional 扱いにする。

詳細は [docs/adobe-host-roadmap.md](docs/adobe-host-roadmap.md) にまとめています。

## Worktree 運用

repository container は bare Git data、main checkout、Issue 別 worktree を分けます。

```text
Documents/GitHub/adobe-mcp-rs/
  .repo.git/          # central bare repository
  main/               # main worktree
  worktrees/          # Issue / feature worktrees
```

よく使う command:

```powershell
cd .\main
git worktree list
git worktree add ..\worktrees\issue-123 -b codex/issue-123 main
git worktree remove ..\worktrees\issue-123
```

local 運用メモは [docs/worktree.md](docs/worktree.md) を参照してください。

## ドキュメント

- [Adobe host roadmap](docs/adobe-host-roadmap.md)
- [Worktree workflow](docs/worktree.md)
- [Codex MCP setup](docs/setup-codex-mcp.md)
- [Operations runbook](docs/operations-runbook.md)
- [Installer E2E guide](docs/installer-e2e.md)
- [Release checklist](docs/release-checklist.md)
- [Rust migration specification](docs/specification-rust-migration.md)
- [TS to Rust migration guide](docs/migration-guide-ts-to-rust.md)

## ライセンス

MIT License。詳細は [LICENSE](LICENSE) を参照してください。
