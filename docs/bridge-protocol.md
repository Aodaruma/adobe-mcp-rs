# Bridge protocol v1

この文書は Adobe ホスト共通の file bridge schema と、旧 After Effects schema からの移行方法を定義します。

## HostSpec

ホスト固有の静的設定は `mcp-core::HostSpec` に集約します。現在は After Effects、Premiere Pro、Photoshop、Illustrator の4ホストを `HOST_SPECS` で定義しています。

新しいホストを追加する場合は、次の順で実装します。

1. `mcp-core` に `HostSpec` 定数を追加し、`HOST_SPECS` に登録する
2. バイナリで `AppConfig::load_for_host` を使用する
3. bridge が後述の `heartbeat.json` を書き出す
4. host 固有 MCP tool から共通の `hostInstance` schema をそのまま返す

`HostSpec` は host id、表示名、binary名、bridge root、command/result file名、instance tool名、primary runtime、bridge起動案内を保持します。設定ファイルで bridge path を明示した場合、その値は従来どおり優先されます。

例えば InDesign 用の雛形は次の形です（この例自体は InDesign 実装を追加しません）。

```rust
pub const INDESIGN_HOST: HostSpec = HostSpec {
    id: "indesign",
    display_name: "InDesign",
    binary_name: "id-mcp",
    bridge_root_name: "id-mcp-bridge",
    command_file_name: "id_command.json",
    result_file_name: "id_mcp_result.json",
    instance_tool_name: "list-indesign-instances",
    bridge_runtime: "uxp-script",
    bridge_setup_hint: "Install and enable the InDesign MCP startup script.",
};
```

## ディレクトリ

```text
~/Documents/<host>-mcp-bridge/
  instances/<instanceId>/
    heartbeat.json
    command.json（既存ホストでは互換ファイル名を使用）
    result.json（既存ホストでは互換ファイル名を使用）
    current_request.json
  registry/<requestId>.json
```

既存ホストの directory 名と command/result file名は変更しません。

## heartbeat.json

protocol v1 の共通項目は次のとおりです。

```json
{
  "protocolVersion": 1,
  "instanceId": "ps-1234",
  "hostId": "photoshop",
  "appName": "Photoshop",
  "appVersion": "27.0",
  "displayName": "Photoshop 27.0",
  "bridgeRuntime": "uxp",
  "capabilities": ["run-jsx", "documents.list", "layers.list"],
  "status": "idle",
  "currentRequestId": null,
  "bridgeRoot": ".../ps-mcp-bridge",
  "commandFile": ".../instances/ps-1234/ps_command.json",
  "resultFile": ".../instances/ps-1234/ps_mcp_result.json",
  "lastHeartbeatAt": "2026-06-25T00:00:00Z",
  "updatedAt": "2026-06-25T00:00:00Z"
}
```

`capabilities` は bridge が実際に処理できる機能の識別子です。未知の capability は無視します。`bridgeRuntime` は `uxp`、`cep-extendscript`、`extendscript-scriptui` など実行環境を示します。

## request record

新規に書き出す request record は対象インスタンスを `hostInstance` で返します。

```json
{
  "requestId": "req-...",
  "command": "ping",
  "status": "completed",
  "hostInstance": {
    "protocolVersion": 1,
    "instanceId": "ps-1234",
    "hostId": "photoshop",
    "bridgeRuntime": "uxp"
  }
}
```

`premiereInstance`、`photoshopInstance`、`illustratorInstance` のような host 別キーは生成しません。

## 後方互換性

- `protocolVersion`、`hostId`、`bridgeRuntime`、`capabilities`、`updatedAt` がない旧 heartbeat は protocol v1 として読み込み、判明する値を `HostSpec` と `lastHeartbeatAt` から補完します。
- request record の旧 `aeInstance` は読み込み時に `hostInstance` として扱います。再出力時は `hostInstance` のみを生成します。
- Rust API の旧 `AeInstance` は非推奨 type alias として残し、新規コードは `HostInstance` を使用します。
- bridge root と既存 command/result file名、設定ファイルで指定した path は変更しません。
- schema の追加項目を理解しない旧 bridge は、その項目を無視して動作できます。

この互換読み取りは移行期間中に維持します。将来 protocol version を上げる場合は、書き出し変更より先に旧・新両方を読める実装をリリースします。
