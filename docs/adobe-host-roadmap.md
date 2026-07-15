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
| After Effects | `ae-mcp` | ScriptUI panel / ExtendScript JSX | **Primary** | MCP の待機・instance routing は `ae-mcp serve-daemon` が必要。panel を開いて `Auto-run commands` を有効にする。公開 tool 一覧と legacy dispatch の差分が残る。 |
| Premiere Pro | `pr-mcp` | UXP panel（25.6+）、CEP / ExtendScript fallback（24.0+） | **Experimental** | `serve-daemon` は broker ではない。UXP の install/release と実機 version matrix が未確立。CEP は fallback。 |
| Photoshop | `ps-mcp` | UXP panel（23.3+、API v2） | **Experimental** | 読み取り中心の小さな allowlist。`serve-daemon` は broker ではない。modal policy、書き込み操作、配布・実機 E2E が未完成。 |
| Illustrator | `ai-mcp` | CEP panel / ExtendScript（24.0+、CSXS 10） | **Experimental** | unsigned CEP は debug mode が必要な場合がある。`serve-daemon` は broker ではない。現行 version の実機検証と署名・配布が未完成。 |

4 host とも Rust workspace の member、`serve-stdio`、`health`、直接 bridge 検証 command、instance registry / retained result のコードを持ちます。ただし daemon の意味は同じではありません。

- After Effects: localhost TCP broker が command routing、待機、request registry、result retention を担当する。
- Premiere Pro / Photoshop / Illustrator: MCP stdio server が file bridge を直接操作する。`serve-daemon` は現在 PID file と定期 log を維持する heartbeat process だけで、request を処理しない。

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
- MCP dispatch が受理する legacy / host-specific tool と `tools/list` で advertise される tool に差がある。文書では advertise 済みの tool を公開 surface として扱う。

### Premiere Pro — Experimental

Runtime と経路:

```text
MCP client -> pr-mcp serve-stdio -> ~/Documents/pr-mcp-bridge
           -> UXP panel (preferred) / CEP ExtendScript panel (fallback)
```

最小機能:

- 共通 8 tool: `run-jsx`, `run-jsx-file`, `run-script`, `get-jsx-result`, `get-results`, `get-help`, `list-premiere-instances`, `run-bridge-test`
- allowlist: `ping`, `getProjectInfo`, `listSequences`, `getActiveSequence`, `getSequenceInfo`, `setPlayheadTime`, `exportSequence`
- prompt: sequence 一覧、playhead 移動、sequence export

制約:

- UXP manifest は Premiere Pro 25.6.0+ を要求する。Developer Mode と UXP Developer Tool が必要になる場合がある。
- CEP manifest は Premiere Pro 24.0+ / CSXS 10 を対象とする fallback で、UXP と同じ release quality を保証しない。
- `pr-mcp serve-daemon`、`service`、`autostart` は request broker を提供しない。通常の MCP 操作には起動不要。

### Photoshop — Experimental

Runtime と経路:

```text
MCP client -> ps-mcp serve-stdio -> ~/Documents/ps-mcp-bridge
           -> Photoshop UXP panel
```

最小機能:

- 共通 8 tool: `run-jsx`, `run-jsx-file`, `run-script`, `get-jsx-result`, `get-results`, `get-help`, `list-photoshop-instances`, `run-bridge-test`
- allowlist: `ping`, `getAppInfo`, `listDocuments`, `getActiveDocument`, `listLayers`
- 任意 UXP code 実行は `mode: "unsafe"` と `description` の明示が必要

制約:

- UXP manifest は Photoshop 23.3.0+、API v2、`loadEvent: startup` を要求する。
- 現時点の allowlist は document / layer の読み取り中心で、公開 prompt はない。
- `ps-mcp serve-daemon`、`service`、`autostart` は request broker を提供しない。通常の MCP 操作には起動不要。
- document write / export、`batchPlay` wrapper、modal execution policy、error normalization は hardening 対象。

### Illustrator — Experimental

Runtime と経路:

```text
MCP client -> ai-mcp serve-stdio -> ~/Documents/ai-mcp-bridge
           -> CEP panel -> Illustrator ExtendScript
```

最小機能:

- 共通 8 tool: `run-jsx`, `run-jsx-file`, `run-script`, `get-jsx-result`, `get-results`, `get-help`, `list-illustrator-instances`, `run-bridge-test`
- allowlist: `ping`, `getAppInfo`, `listDocuments`, `getActiveDocument`, `listArtboards`, `listLayers`, `exportDocument`
- prompt: document 一覧、artboard 一覧、document export
- `exportDocument`: PNG24 / PNG8 / JPEG / SVG / PDF の初期実装

制約:

- CEP manifest は Illustrator 24.0+ / CSXS 10 を対象とする。local unsigned extension は CEP debug mode が必要な場合がある。
- `ai-mcp serve-daemon`、`service`、`autostart` は request broker を提供しない。通常の MCP 操作には起動不要。
- UXP を既定 runtime にはしない。third-party host support と配布経路が明確になるまで CEP / ExtendScript を baseline とする。

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
2. Premiere Pro / Photoshop / Illustrator の daemon を AE と同等の broker にするか、heartbeat command を廃止して direct file bridge と明記する。
3. host 共通の protocol / E2E fixture と Adobe 実機 test matrix を追加する。
4. Premiere Pro の UXP package、CEP fallback、installer の対応 version を実機で固定する。
5. Photoshop の書き込み・export・modal policy と Illustrator の export / packaging を hardening する。
6. Windows / macOS の host 別 component install、署名、公証、upgrade / uninstall を release gate に組み込む。

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
