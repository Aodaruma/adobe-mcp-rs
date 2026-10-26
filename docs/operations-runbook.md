# 運用 Runbook（Stage 7）

- 最終更新: 2026-09-05
- 対象: Rust版 `ae-mcp` / `pr-mcp` / `ps-mcp` / `ai-mcp` の日常運用

## 1. 基本コマンド

## 1.1 ヘルス確認

```bash
<host>-mcp health
```

## 1.2 MCP stdio起動

```bash
<host>-mcp serve-stdio
```

## 1.3 デーモン起動

```bash
<host>-mcp serve-daemon
```

既定 address は AE `127.0.0.1:47655`、Premiere `:47656`、Photoshop `:47657`、Illustrator `:47658`。`health` は実際に使用する `daemon_addr` を表示します。

## 1.4 Windows autostart 管理

```bash
<host>-mcp autostart install
<host>-mcp autostart start
<host>-mcp autostart status
<host>-mcp autostart stop
<host>-mcp autostart uninstall
```

`install` は現在のユーザーの Run key と非表示launcherを登録し、即時起動はしない。`uninstall` も登録削除だけなので、完全停止は `stop` の後に行う。`status` が `outdated` を返した場合は exe 移動または upgrade 後の登録ずれなので、実行中requestがないことを確認してから `stop` → `install` → `start` の順で修復する。旧 exe のプロセスが生きている間は、安全のため新 daemon の `start` は失敗する。

ログイン時にterminalが残る場合、Run keyが古い `ae-mcp.exe serve-daemon` の直接起動になっていないか確認する。現行の登録は `wscript.exe //B //NoLogo .../daemon-autostart.vbs` で、launcherがwindow style 0でdaemonを起動する。更新したbinaryで `autostart install` を実行して登録し直す。stdioのstdoutはMCP通信なので変更しない。

Windows 版 CLI に `service` は存在しない。`service` を案内している古い手順は使用しない。

## 1.5 macOS launchd service 管理

```bash
<host>-mcp service install
<host>-mcp service start
<host>-mcp service status
<host>-mcp service stop
<host>-mcp service uninstall
```

macOS 版 CLI に Windows 用 `autostart` は存在しない。

## 2. ブリッジファイル

配置先（`ae` は `pr` / `ps` / `ai` に読み替え）:

- `~/Documents/ae-mcp-bridge/ae_command.json`
- `~/Documents/ae-mcp-bridge/ae_mcp_result.json`

確認ポイント:

1. `ae_command.json.status` が `pending` で止まっていないか
2. `ae_mcp_result.json` の更新時刻が古くないか

## 3. 典型障害と一次対応

1. daemon に接続できない
- `<host>-mcp health` で host 別 `daemon_addr` を確認
- Windows は `<host>-mcp autostart status`、macOS は `<host>-mcp service status` で状態確認
- 必要なら OS 対応の `start` または `<host>-mcp serve-daemon` を実行
- `failed to bind ... another daemon may already be running` は同じ address の二重起動を示すため、既存 daemon を確認する
- Windows の `running from a different executable` は移動前 exe がまだ稼働中。`autostart stop` 後に `install` と `start` をやり直す

2. `get-results` が stale warning
- AEを再起動し、`Scripts/Startup/mcp-bridge-startup.jsx` と `Scripts/ScriptUI Panels/mcp-bridge-auto.jsx` の配置を確認する
- Scripting & Expressionsのfile/network access設定を確認する
- `$.global.__adobeMcpBridgeBootstrapState` と `aeMcpBridgeGetState()` を確認する
- `list-ae-instances` / `list-premiere-instances` の `inactiveInstances` を確認し、`heartbeat is stale`、parse error、空の `instanceId` などの理由を見る

3. `method not found`（MCP）
- クライアントが `serve-stdio` で起動しているか確認
- 古いNode設定が残っていないか確認

4. panel / UXP を開いたまま host app を再起動した後に instance が見えない
- AE: `aeMcpBridgeRestart()`を実行するかAEを再起動し、Startup bootstrap stateとheartbeat更新時刻を確認
- Premiere UXP: `Window > UXP Plugins > Premiere MCP Bridge` を開き、Instance 表示と `~/Documents/pr-mcp-bridge/instances/<instanceId>/heartbeat.json` の更新時刻を確認
- Premiere CEP fallback: `~/Documents/pr-mcp-bridge/instances/<instanceId>/heartbeat.json` が作成されているか確認

## 4. 監視ポイント

### timeout / unknown と安全な復旧

- `timeout`はclientが待機を終えた状態で、Adobe内の処理停止を意味しない。同じ`requestId`で`get-script-result`を確認し、変更操作を再送しない。
- heartbeat途絶時の`unknown`、互換読込の`lost`、`cancelRequested`も後着結果を回収できる。キャンセルは強制停止ではない。
- 未解決requestがある間は同一instanceの後続処理が待機する。global exclusiveが関係する場合は別instanceも待機する。結果が永久に返らなければ自動では解除しない。
- 復旧前に新規送信を止め、request / result / current_requestと診断ログを保全する。Adobe側の実行が終了したことを確認し、保存可能な作業は保存してhostを通常終了する。実行中のままdaemonだけを再起動しない。global排他はdaemonのメモリにあり、再起動をまたいで保証されない。
- host終了後にdaemonを再起動し、新しいhost instanceへ接続する。古いinstanceの未解決記録は履歴として残す。host終了確認前に`current_request.json`を削除して予約を解除しない。

### AEの更新とUndo確認

binaryをコピーしても稼働中のdaemon / stdioは旧版のまま。JSXを配置しても起動済みAEのruntimeは旧版のままなので、配置版と稼働版を区別する。daemonは`ping.version` / `processId`、AEは`ae_mcp_bootstrap.json.runtimeVersion`と新しいinstanceのheartbeatを照合する。

AE 0.5.1 bridgeは`undoGroup:false`を解釈しない。0.5.3には外側の`endUndoGroup()`を重ねない修正があるが、任意JSXのUndo安全性全体を保証するものではない。作業を保存して通常再起動した後、使い捨てprojectで単純編集、throw、user側Undo group、`undoGroup:false`、import、renderを分けて確認する。既存の`run-bridge-test`はeffectを変更するため、制作projectの疎通確認に使わない。

### 日常監視

1. daemon 稼働状態（Windows: `<host>-mcp autostart status`、macOS: `<host>-mcp service status`）
2. 結果ファイル更新時刻
3. MCPクライアントの呼び出し失敗率

## 5. 障害時ログ採取

1. 実行コマンドと出力（stdout/stderr）
2. `ae_command.json` / `ae_mcp_result.json` の内容
3. AEバージョン、OSバージョン、実行ユーザー権限
4. AEは必要な期間だけ `configure-bridge-diagnostics` の `enabled: true`、またはdiagnostics panelの `Write diagnostic debug log` を有効にする
5. `get-bridge-diagnostics` でinstance状態、`ae_mcp_scheduler_diagnostic.json`、debug log末尾を取得する

AEのverbose debug logは既定OFF。調査終了後は `configure-bridge-diagnostics` の `enabled: false` へ戻す。
