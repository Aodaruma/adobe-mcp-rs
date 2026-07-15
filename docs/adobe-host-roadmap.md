# Adobe host support status and roadmap

- 最終更新: 2026-07-15
- 対象 revision の確認方法: 本文の「状態を更新するときの検証」を参照

この文書を host 対応状況の source of truth とします。README とセットアップ文書は概要と導入手順を示し、状態区分、runtime、最小機能、既知の制約はこの文書に合わせます。

## 状態区分

| 区分 | 判定基準 |
|---|---|
| **Primary** | Rust MCP server と host bridge が揃い、通常利用する routing / result 取得経路と主要操作が実装され、既定の運用手順として案内できる。すべての機能が GA 品質であることを意味しない。 |
| **Experimental** | Rust MCP server と host bridge の初期実装、build / unit test、最小 tool surface は揃っているが、実機 E2E、installer、runtime compatibility、broker / service のいずれかが Primary と同等でない。 |
| **Planned** | 設計または Issue はあるが、利用可能な Rust binary と host bridge の組がまだ揃っていない。 |

状態を上げる場合は、対象 OS / host version を記録した実機 E2E、導入・再起動後の再接続、instance routing、代表的な読み取り・書き込み操作を確認します。

## 現在の対応状況

| Host | Binary | Bridge runtime | 状態 | 主な制約 |
|---|---|---|---|---|
| After Effects | `ae-mcp` | ScriptUI panel / ExtendScript JSX | **Primary** | MCP の待機・instance routing は `ae-mcp serve-daemon` が必要。panel を開いて `Auto-run commands` を有効にする。 |
| Premiere Pro | `pr-mcp` | UXP panel（25.6+）、CEP / ExtendScript fallback（24.0+） | **Experimental** | 共通 broker を使用。UXP の install/release と実機 version matrix が未確立。CEP は fallback。 |
| Photoshop | `ps-mcp` | UXP panel（23.3+、API v2） | **Experimental** | 共通 broker を使用。modal policy、書き込み操作、配布・実機 E2E が未完成。 |
| Illustrator | `ai-mcp` | CEP panel / ExtendScript（24.0+、CSXS 10） | **Experimental** | 共通 broker を使用。現行 version の実機検証と署名・配布が未完成。 |
| InDesign | `id-mcp`（計画） | UXP Startup Script `.idjs`（計画） | **Planned** | Issue #21 で binary、Startup Script、raw `.idjs` 実行、daemon 接続を PoC する。 |

4 host とも `daemon-core` の localhost TCP broker を使用します。`serve-daemon` は command routing、待機、instance別FIFO、別instance並列、global exclusive、request registry、result retention を担当します。既定 port は AE `47655` / Premiere `47656` / Photoshop `47657` / Illustrator `47658` です。

## Host 別の実装範囲

### After Effects — Primary

Runtime と経路:

```text
MCP client -> ae-mcp serve-stdio -> ae-mcp serve-daemon
           -> ~/Documents/ae-mcp-bridge -> ScriptUI / ExtendScript panel
```

最小機能:

- MCP で公開される共通実行・運用 tool: `run-jsx`, `run-jsx-file`, `get-jsx-result`, `list-ae-instances`, `get-results`, `get-help`, `save-frame-png`, `cleanup-preview-folder`, `run-bridge-test`
- bridge / CLI allowlist: composition / layer の取得・作成・更新、keyframe / expression、effect / template、preview PNG、render queue、time / work area / marker、project open / save / close、dialog suppression
- `instances/<instanceId>/heartbeat.json` による複数 instance 検出と `requestId` result retention

制約:

- `mcp-bridge-auto.jsx` を ScriptUI Panels に配置し、After Effects の file / network access を許可する。
- panel を開いて `Auto-run commands` を有効にする必要がある。自動起動と daemon 再接続は今後の hardening 対象。
- `getLayerInfo` の bridge 実装は active composition が必要。
- `tools/list`でadvertiseする9 Toolだけが公開surface。legacy / host-specific名は非公開互換dispatchとして受理し、呼出時に公開置換先を返す。
- `run-script`はallowlistを持つが、非同期direct-file互換経路と同期daemon broker契約の安全境界・完了条件が異なるため公開しない。
- `aftereffects://compositions` ResourceとPromptが案内する実行経路はdaemon brokerを使用する。Prompt自体は実行せず、公開Tool / Resourceを案内する。

### Premiere Pro — Experimental

Runtime と経路:

```text
MCP client -> pr-mcp serve-stdio -> pr-mcp serve-daemon
           -> ~/Documents/pr-mcp-bridge
           -> UXP panel (preferred) / CEP ExtendScript panel (fallback)
```

最小機能:

- 共通 8 tool: `run-jsx`, `run-jsx-file`, `run-script`, `get-jsx-result`, `get-results`, `get-help`, `list-premiere-instances`, `run-bridge-test`
- allowlist: `ping`, `getProjectInfo`, `listSequences`, `getActiveSequence`, `getSequenceInfo`, `setPlayheadTime`, `exportSequence`
- prompt: sequence 一覧、playhead 移動、sequence export

制約:

- UXP manifest は Premiere Pro 25.6.0+ を要求する。Developer Mode と UXP Developer Tool が必要になる場合がある。
- CEP manifest は Premiere Pro 24.0+ / CSXS 10 を対象とする fallback で、UXP と同じ release quality を保証しない。
- `pr-mcp serve-daemon` の起動が通常の MCP 操作に必要。UXP / CEP 実機での broker E2E は release gate。

### Photoshop — Experimental

Runtime と経路:

```text
MCP client -> ps-mcp serve-stdio -> ps-mcp serve-daemon
           -> ~/Documents/ps-mcp-bridge
           -> Photoshop UXP panel
```

最小機能:

- 共通 8 tool: `run-jsx`, `run-jsx-file`, `run-script`, `get-jsx-result`, `get-results`, `get-help`, `list-photoshop-instances`, `run-bridge-test`
- allowlist: `ping`, `getAppInfo`, `listDocuments`, `getActiveDocument`, `listLayers`
- 任意 UXP code 実行は `mode: "unsafe"` と `description` の明示が必要

制約:

- UXP manifest は Photoshop 23.3.0+、API v2、`loadEvent: startup` を要求する。
- 現時点の allowlist は document / layer の読み取り中心で、公開 prompt はない。
- `ps-mcp serve-daemon` の起動が通常の MCP 操作に必要。UXP 実機での broker E2E は release gate。
- document write / export、`batchPlay` wrapper、modal execution policy、error normalization は hardening 対象。

### Illustrator — Experimental

Runtime と経路:

```text
MCP client -> ai-mcp serve-stdio -> ai-mcp serve-daemon
           -> ~/Documents/ai-mcp-bridge
           -> CEP panel -> Illustrator ExtendScript
```

最小機能:

- 共通 8 tool: `run-jsx`, `run-jsx-file`, `run-script`, `get-jsx-result`, `get-results`, `get-help`, `list-illustrator-instances`, `run-bridge-test`
- allowlist: `ping`, `getAppInfo`, `listDocuments`, `getActiveDocument`, `listArtboards`, `listLayers`, `exportDocument`
- prompt: document 一覧、artboard 一覧、document export
- `exportDocument`: PNG24 / PNG8 / JPEG / SVG / PDF の初期実装

制約:

- CEP manifest は Illustrator 24.0+ / CSXS 10 を対象とする。local unsigned extension は CEP debug mode が必要な場合がある。
- `ai-mcp serve-daemon` の起動が通常の MCP 操作に必要。CEP 実機での broker E2E は release gate。
- UXP を既定 runtime にはしない。third-party host support と配布経路が明確になるまで CEP / ExtendScript を baseline とする。

### InDesign — Planned

候補経路:

```text
MCP client -> id-mcp serve-stdio -> id-mcp serve-daemon
           -> InDesign UXP Startup Script (.idjs)
           -> audited temporary .idjs -> app.doScript(..., UXPSCRIPT)
```

設計方針:

- UXP Startup Script による host 起動時の自動起動と、未解決 Promise / event loop による bridge lifespan を Issue #21 で検証する。
- UXP Script は文字列からの code generation が無効なため、raw source は監査済み一時 `.idjs` にして `app.doScript` で実行する案を第一候補とする。
- structured input は `doScript` の `withArguments` / `script.args`、結果は `script.setResult` または bridge envelope で返す。
- `HostSpec`、共通 daemon、heartbeat、instance routing、retained result、script file policy は既存 host と同じ契約へ追加する。
- UXP plugin は permission と lifecycle の比較候補だが、操作ごとの panel Tool を既定 surface にはしない。

5 host の runtime、raw code、read / write / export、undo / modal / filesystem、lifecycle、payload、guard 方針は [capability matrix](capability-matrix.md) を参照してください。

## Bridge protocol の現状

現在は host ごとに root / command / result file 名が異なり、root 直下の compatibility file と instance ごとの file を併用します。

```text
~/Documents/<host>-mcp-bridge/
  <host>_command.json
  <host>_mcp_result.json
  instances/<instanceId>/
    heartbeat.json
    <host>_command.json
    <host>_mcp_result.json
  registry/<requestId>.json
```

`mcp-core` の `HostSpec` に4 hostのmetadataを集約し、`bridge-core` は `HostInstance` と `hostInstance` を共通schemaとして使用します。heartbeatは `protocolVersion`、`hostId`、`bridgeRuntime`、`capabilities` を持つprotocol v1へ移行済みです。旧heartbeatとrequest recordの `aeInstance` は読み取り互換を維持します。詳細は [bridge protocol](bridge-protocol.md) を参照してください。

## Roadmap

1. **完了:** host名、binary、bridge root、file名、instance tool名を `HostSpec` に集約し、protocol v1を導入する。
2. **完了:** 4 host の daemon を `daemon-core` の共通 broker に統一する。direct file bridge は互換・診断用途として残す。
3. **完了:** host 共通の protocol / E2E fixture を追加する。Adobe 実機 test matrix は継続する。
4. **設計済み:** [capability matrix](capability-matrix.md) で raw-script-first、共通 script contract、guard の非 sandbox 境界、structured Tool 追加基準を定義する。schema / guard 実装は段階導入する。
5. Issue #21 の InDesign Startup Script PoC と Issue #22 の AE lifecycle / reconnect PoC を並行する。
6. Premiere Pro の UXP package / CEP fallback、Photoshop の modal / write / export、Illustrator の export / packaging を実機で hardening する。
7. Windows / macOS の host 別 component install、署名、公証、upgrade / uninstall を release gate に組み込む。

## 状態を更新するときの検証

repository 上の静的確認:

```powershell
cargo test --workspace
cargo build --release -p ae-mcp -p pr-mcp -p ps-mcp -p ai-mcp
.\target\release\ae-mcp.exe health
.\target\release\pr-mcp.exe health
.\target\release\ps-mcp.exe health
.\target\release\ai-mcp.exe health
```

加えて、各 `*-core` の `tool_specs()` / allowlist、各 `mcp_stdio.rs` の dispatch、bridge manifest の host / minimum version、installer script の配置処理を照合します。2026-07-15 の文書同期ではこの repository-level verification を実施しています。

Primary / Experimental の release 判定では、実際の Adobe host で次も確認して記録します。

1. bridge を導入して panel を開き、heartbeat が更新される。
2. `list-*-instances` が対象 instance と version / runtime を返す。
3. `run-bridge-test` と allowlist の読み取り操作が成功する。
4. 代表的な書き込みまたは export が成功する。
5. host / panel / daemon の起動順変更、host 再起動、複数 instance、stale result を確認する。
6. OS、Adobe host version、bridge runtime、binary version、実施日を test record に残す。

## 参考

- [UXP for Adobe Photoshop](https://developer.adobe.com/photoshop/uxp/2022/)
- [Photoshop API reference](https://developer.adobe.com/photoshop/uxp/2022/ps-reference/)
- [Premiere Pro UXP API](https://developer.adobe.com/premiere-pro/uxp/)
- [Illustrator developer overview](https://developer.adobe.com/illustrator/)
- [UXP host version table](https://developer.adobe.com/xd/uxp/uxp/versions/)
