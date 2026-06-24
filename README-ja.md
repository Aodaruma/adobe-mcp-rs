# Adobe MCP Servers (Rust)

LLM から Adobe アプリをローカル自動操作するための、Rust 製 MCP サーバー群とホスト側ブリッジのプロジェクトです。

この repository は、After Effects 専用の `after-effects-mcp-rs` から、Adobe アプリ横断の `adobe-mcp-rs` へ育てる前提でリネームされています。現時点では After Effects 実装が最も進んでおり、Premiere Pro は実験的な実装が入り、Photoshop / Illustrator 対応は今後追加する段階です。

- English: [README.md](README.md)

## 現状

| 対象アプリ | バイナリ | ブリッジ | 状態 |
|---|---|---|---|
| After Effects | `ae-mcp` | ScriptUI / JSX panel | 主対応 |
| Premiere Pro | `pr-mcp` | UXP panel、CEP fallback | 実験的。tool surface はあるが install/release 周りは未整備 |
| Photoshop | planned `ps-mcp` | UXP plugin 優先 | 計画中 |
| Illustrator | planned `ai-mcp` | まず ExtendScript/CEP または native plugin。UXP は公開対応確認後 | 計画中 |

## 現在の構成

workspace は、共通 Rust crate とアプリ別バイナリに分かれています。

| Path | 役割 |
|---|---|
| `crates/ae-mcp` | After Effects CLI、MCP stdio server、daemon、bridge command |
| `crates/pr-mcp` | Premiere Pro CLI、MCP stdio server |
| `crates/mcp-core` | 共通 config、MCP tool/prompt spec、bridge path default |
| `crates/bridge-core` | file bridge client、instance discovery、request registry、result retention |
| `crates/platform-service` | Windows/macOS service と autostart helper |
| `crates/pr-core` | Premiere Pro tool spec、prompt、allowlist script |
| `src/scripts` | After Effects JSX bridge と helper script |
| `src/premiere/uxp` | Premiere Pro UXP bridge panel |
| `src/premiere/cep` | 旧 Premiere Pro CEP bridge fallback |

After Effects は `ae-mcp serve-daemon` をローカル broker として使います。bridge panel は `~/Documents/ae-mcp-bridge/instances/<instanceId>/` に登録され、MCP 呼び出しは対象 instance へ routing されます。結果は `requestId` で保持されます。

Premiere Pro は `~/Documents/pr-mcp-bridge` 以下で同じ file bridge パターンを使っています。UXP bridge が本線で、CEP bridge は fallback です。ただし `pr-mcp serve-daemon` はまだ After Effects の broker と同等ではないため、Premiere 側は実験的として扱います。

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

任意 code 実行では `mode: "unsafe"` と短い `description` を明示します。host 側 JavaScript/JSX は強い権限を持つため、MCP 呼び出し上も明示的に扱う方針です。

## 今後の拡張方針

次は、After Effects 前提を各アプリへコピーするのではなく、host 対応を明示的な設計要素にします。

1. host 名、bridge root、tool 名、実行ファイル名、help text、installer 挙動をまとめる host adapter 層を切り出す。
2. `heartbeat.json`、command/result file、instance metadata、capabilities、retained request record の bridge protocol を共通化する。
3. Premiere Pro を After Effects と同等の broker model に寄せるか、direct file-bridge として明確に分離する。
4. Photoshop は UXP bridge から着手し、Photoshop DOM と `batchPlay` を併用する。
5. Illustrator は現行バージョンで使える公開 API を確認する spike を先に行い、当面は ExtendScript/CEP または native plugin bridge を現実解にする。

詳細は [docs/adobe-host-roadmap.md](docs/adobe-host-roadmap.md) にまとめています。

## Worktree 運用

この checkout は linked worktree として仕立てています。想定 local layout は次の通りです。

```text
Documents/GitHub/
  adobe-mcp-rs.git/   # bare repository
  adobe-mcp-rs/       # main worktree
```

よく使う command:

```powershell
git worktree list
git worktree add ..\adobe-mcp-rs-photoshop -b codex/photoshop-support main
git worktree add ..\adobe-mcp-rs-illustrator -b codex/illustrator-support main
git worktree remove ..\adobe-mcp-rs-photoshop
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
