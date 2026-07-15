# Bridge protocol v1

この文書は Adobe ホスト共通の file bridge schema と、旧 After Effects schema からの移行方法を定義します。

## HostSpec

ホスト固有の静的設定は `mcp-core::HostSpec` に集約します。現在は After Effects、Premiere Pro、Photoshop、Illustrator、InDesign の5ホストを `HOST_SPECS` で定義しています。

新しいホストを追加する場合は、次の順で実装します。

1. `mcp-core` に `HostSpec` 定数を追加し、`HOST_SPECS` に登録する
2. バイナリで `AppConfig::load_for_host` を使用する
3. bridge が後述の `heartbeat.json` を書き出す
4. host 固有 MCP tool から共通の `hostInstance` schema をそのまま返す

`HostSpec` は host id、表示名、binary名、bridge root、command/result file名、instance tool名、primary runtime、bridge起動案内、daemon既定portを保持します。設定ファイルで bridge path や `daemon_addr` を明示した場合、その値は従来どおり優先されます。

InDesign は次の `HostSpec` で登録されています。

```rust
pub const INDESIGN_HOST: HostSpec = HostSpec {
    id: "indesign",
    display_name: "InDesign",
    binary_name: "id-mcp",
    bridge_root_name: "id-mcp-bridge",
    command_file_name: "id_command.json",
    result_file_name: "id_mcp_result.json",
    instance_tool_name: "list-indesign-instances",
    bridge_runtime: "uxp-startup-script",
    bridge_setup_hint: "Install mcp-bridge-indesign.idjs into the InDesign Startup Scripts folder.",
    daemon_port: 47659,
};
```

## daemon broker

5 host の `serve-daemon` は `daemon-core` の同一 protocol を実装します。localhost TCP の1行JSONで次の operationを受け付けます。

- `ping`
- `listInstances`
- `runCommand`
- `getResult`
- `latestResult`

同一 `instanceId` は FIFO、別 instance は並列です。`runCommand.globalExclusive=true` はその host daemon の全 instance に対する排他を取得します。client timeout 後も worker は継続し、`requestId` を `getResult` へ渡すと完了結果を回収できます。詳細は [ADR 0001](adr/0001-host-neutral-daemon-broker.md) を参照してください。

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

## JSON fileの原子更新

`command`、`result`、`heartbeat`、`current_request`、`registry/<requestId>.json` は、次のwriter規約に従います。

1. 最終fileと同じdirectoryに、`.＜最終file名＞.tmp-＜process/session固有suffix＞` を排他的に作成する
2. 完成したJSON全体を書き、flushする
3. 最終pathへ同一filesystem内のreplace/renameで公開する
4. 成功・失敗を問わず自分の一時fileを片付ける

Rust writerはfileを `sync_all` してから公開します。Windowsでは既存fileを先に削除せず、`MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` を使用します。共有違反などで置換できない場合は短時間再試行し、最終的に失敗しても旧fileを残します。macOS/Linuxでは同一directoryの `rename` 後にdirectoryも同期します。同一processから同じtargetへの更新は直列化し、複数process間では最後に成功した置換を採用します。

Node.js filesystemを使えるUXP bridgeは、排他的な一時fileへ書き、`fsyncSync`、`renameSync` の順で公開します。ExtendScript/CEP bridgeは `File.close()` をflush境界とし、同一directoryの直接renameを最初に試します。既存宛先をrenameで上書きできないhostでは、旧fileを `.＜最終file名＞.bak-...` に退避してから一時fileを公開し、公開失敗時は旧fileを復元します。このlegacy fallbackに限り最終pathが短時間存在しない場合があります。

readerは `.tmp-*` と `.bak-*` を列挙対象にせず、最終pathだけを読みます。最終pathの短時間の欠落、共有違反、空file、不完全JSONは「まだ更新中」として再試行します。polling readerはそのpollを未到着として扱い、次のpollへ進みます。1時間以上残った対象file用の `.tmp-*` / `.bak-*` は次回のwrite時に削除し、Rustのregistry cleanupはregistry directoryの古い `.tmp-*` も削除します。

最終fileを直接truncateして書くwriterはprotocol v1非準拠です。新しいJSX/UXP/CEP bridgeを追加する場合も、この命名・flush・公開・cleanup規約を使用してください。

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

`capabilities` は bridge が実際に処理できる機能の識別子です。未知の capability は無視します。`bridgeRuntime` は `uxp`、`cep-extendscript`、`extendscript-startup`、`extendscript-scriptui` など実行環境を示します。

host固有の観測情報は追加可能です。AE Startup runtimeは `lifecycleMode`、`runtimeId`、`runtimeStartedAt` を追加し、poller generationと再初期化を診断できるようにします。protocol readerは未知の追加項目を無視します。

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
