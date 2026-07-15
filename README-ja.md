# Adobe MCP Servers (Rust)

LLM から Adobe アプリをローカル自動操作するための、Rust 製 MCP サーバー群とホスト側ブリッジのプロジェクトです。

この repository は、After Effects 専用の `after-effects-mcp-rs` から、Adobe アプリ横断の `adobe-mcp-rs` へ育てる前提でリネームされています。現時点では After Effects 実装が最も進んでおり、Premiere Pro / Photoshop / Illustrator / InDesign は実験的な実装が入っています。

- English: [README.md](README.md)

## 現状

最終コード照合日: 2026-07-15

| 対象アプリ | バイナリ | bridge runtime | 状態 | 現在の境界 |
|---|---|---|---|---|
| After Effects | `ae-mcp` | Startup / ExtendScript JSX | **Primary** | headless Startup bootstrapと`serve-daemon` brokerを使用 |
| Premiere Pro | `pr-mcp` | UXP 25.6+、CEP / ExtendScript 24.0+ fallback | **Experimental** | sequence / export の初期 surface。`serve-daemon` broker が必要 |
| Photoshop | `ps-mcp` | UXP 23.3+（API v2） | **Experimental** | 汎用実行と document / layer 読み取りの初期 surface |
| Illustrator | `ai-mcp` | CEP / ExtendScript 24.0+（CSXS 10） | **Experimental** | document / artboard / layer / export の初期 surface。runtime 配布の検証が必要 |
| InDesign | `id-mcp` | UXP Startup Script 18.5+ PoC | **Experimental** | panel不要のraw `app.doScript`とdocument/page/story読み取り。実機検証が必要 |

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
| `crates/id-core` | InDesign raw-first Tool定義とallowlist読み取りtemplate |
| `crates/id-mcp` | InDesign CLI、MCP stdio server |
| `crates/mcp-core` | 共通 config、MCP tool/prompt spec、bridge path default |
| `crates/bridge-core` | file bridge client、instance discovery、request registry、result retention |
| `crates/daemon-core` | 共通 TCP broker/client、instance別 scheduler、global-exclusive gate |
| `crates/platform-service` | Windows のユーザー別 autostart と macOS launchd helper |
| `crates/pr-core` | Premiere Pro tool spec、prompt、allowlist script |
| `crates/ps-core` | Photoshop tool spec、help text、allowlist script |
| `src/scripts` | After Effects JSX bridge と helper script |
| `src/premiere/uxp` | Premiere Pro UXP bridge panel |
| `src/premiere/cep` | 旧 Premiere Pro CEP bridge fallback |
| `src/photoshop/uxp` | Photoshop UXP bridge panel |
| `src/illustrator/cep` | Illustrator CEP / ExtendScript bridge panel |
| `src/indesign/uxp` | InDesign UXP Startup Script bridge |

5 binary は同じローカル TCP broker model を使います。通常の MCP 実行は `serve-stdio` から `serve-daemon` へ proxy され、daemon が `instances/<instanceId>/` の file bridge へ routing し、結果を `requestId` で保持します。同一 instance は FIFO、別 instance は並列実行でき、必要な操作は host 全体の global-exclusive gate を取得します。

既定の loopback address は host 別です。After Effects は `127.0.0.1:47655`、Premiere Pro は `:47656`、Photoshop は `:47657`、Illustrator は `:47658`、InDesignは`:47659`です。host 別 config の `daemon_addr` で上書きできます。root 直下の command/result file と各 binary の `bridge` CLI は互換・診断用途に限って残し、通常 MCP transport には使いません。詳細は [ADR 0001](docs/adr/0001-host-neutral-daemon-broker.md) を参照してください。

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
cargo build --release -p id-mcp
```

### After Effects

headless bridge runtimeとStartup bootstrapを配置します。

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-bridge.ps1
```

```bash
bash ./scripts/install-bridge.sh
```

After Effects 側:

1. `Allow Scripts to Write Files and Access Network` を有効化
2. After Effects を再起動

repository上のbridgeは `Scripts/Startup` からheadless起動し、panelや`Auto-run commands`に依存しない設計です。対応AE versionでの実際のcold-startは未検証なので、[After Effects bridge lifecycle](docs/after-effects-bridge-lifecycle.md) の実機gateを確認してください。

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

MCP server の登録・利用前に `pr-mcp serve-daemon` を起動します。

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

MCP server の登録・利用前に `ps-mcp serve-daemon` を起動します。

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

MCP server の登録・利用前に `ai-mcp serve-daemon` を起動します。

MCP server を登録:

```powershell
codex mcp add illustrator -- "<ABSOLUTE_PATH>\target\release\ai-mcp.exe" serve-stdio
```

### InDesign

`id-mcp`をbuildし、`src/indesign/uxp/mcp-bridge-indesign.idjs`を対象versionの`Scripts/Startup Scripts`へ配置してInDesignを再起動し、`id-mcp serve-daemon`を起動します。repository上はpanelやAuto-run toggleを使わない設計ですが、Startup Scriptの常駐は実機PoC gateです。

raw-firstの`run-script`は`eval`/`Function`ではなく、InDesignが公開する`app.doScript`のString入力を使います。現時点では実機未検証のPoCなので、運用前に[InDesign MCP PoCとE2E gate](docs/indesign-mcp.md)を確認してください。

```powershell
codex mcp add indesign -- "<ABSOLUTE_PATH>\target\release\id-mcp.exe" serve-stdio
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
.\target\release\pr-mcp.exe serve-daemon
```

別 terminal:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-premiere-instances","arguments":{}}}' | .\target\release\pr-mcp.exe serve-stdio
```

Photoshop:

```powershell
.\target\release\ps-mcp.exe health
.\target\release\ps-mcp.exe serve-daemon
```

別 terminal:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-photoshop-instances","arguments":{}}}' | .\target\release\ps-mcp.exe serve-stdio
```

Illustrator:

```powershell
.\target\release\ai-mcp.exe health
.\target\release\ai-mcp.exe serve-daemon
```

別 terminal:

```powershell
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

この9 Toolが `tools/list` に返る After Effects の公開契約すべてです。`run-script`、`create-composition`、effect / render queue / project lifecycle の旧Tool名は、非公開の互換dispatchとしてのみ受理します。旧Tool呼出時は非推奨の案内と公開置換先を返し、Promptとセットアップ手順は非公開名へ依存しません。

`run-script` は意図的に再公開しません。allowlist は旧clientとの互換に有用ですが、非同期direct-file動作が同期daemon brokerの公開契約と一致しないためです。新規のhost固有操作は `mode: "unsafe"` を明示した `run-jsx` を使います。`aftereffects://compositions` ResourceとAfter Effects Promptが案内する全操作はdaemon broker経路を使い、Prompt自体は再利用可能な手順だけを返します。

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

任意 code 実行では `mode: "unsafe"` と短い `description` を明示します。`unsafe` は sandbox を意味せず、host 側 JavaScript/JSX は Adobe host と同じ強い権限で動作します。`run-jsx-file` の allowed root、trusted path/hash、拡張子、監査情報は [run-jsx-file の信頼ポリシー](docs/script-file-security.md) を参照してください。

## 今後の拡張方針

公開 API は **raw-script-first** とします。LLM が JavaScript / JSX を直接組み立てられる操作は、操作ごとの Tool を増やすより `run-script` / `run-jsx` 系、structured input、recipe、結果回収を優先します。静的な削除検知や確認は事故防止には有用ですが sandbox にはならないため、`safe mode` とは呼ばず risk policy として扱います。host 別の比較、schema 案、guard の限界、Tool 追加基準は [capability matrix](docs/capability-matrix.md) を参照してください。

1. **完了:** host metadata を `HostSpec` に集約する。
2. **完了:** `heartbeat.json`、command/result file、instance metadata、capabilities、retained request record を共通化する。
3. **完了:** 5 binary で `daemon-core` の broker model を共有し、direct file bridge は互換・診断用途に限定する。
4. 共通 script contract、capability report、payload 上限、非 sandbox の risk preflight を段階導入する。
5. InDesign UXP Startup Script PoC と AE の自動起動・再接続 PoC を実機で検証・強化する。
6. Photoshop UXP bridge の write / modal / export と、Illustrator CEP bridge の export / packaging を実機で強化する。

詳細は [docs/adobe-host-roadmap.md](docs/adobe-host-roadmap.md) にまとめています。After Effectsの公開Tool、Resource、Prompt、非公開互換dispatchの正確な一覧は [docs/after-effects-mcp-surface.md](docs/after-effects-mcp-surface.md) を参照してください。

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
- [Adobe host capability matrix / raw-script-first policy](docs/capability-matrix.md)
- [Worktree workflow](docs/worktree.md)
- [Codex MCP setup](docs/setup-codex-mcp.md)
- [Operations runbook](docs/operations-runbook.md)
- [Bridge contract / 実機 smoke test](docs/bridge-contract-testing.md)
- [InDesign MCP PoCとE2E gate](docs/indesign-mcp.md)
- [Installer E2E guide](docs/installer-e2e.md)
- [Release checklist](docs/release-checklist.md)
- [Rust migration specification](docs/specification-rust-migration.md)
- [TS to Rust migration guide](docs/migration-guide-ts-to-rust.md)

## ライセンス

MIT License。詳細は [LICENSE](LICENSE) を参照してください。
