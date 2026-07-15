# 運用 Runbook（Stage 7）

- 最終更新: 2026-07-15
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

`install` は現在のユーザーの Run key を登録するだけで、即時起動はしない。`uninstall` も登録削除だけなので、完全停止は `stop` の後に行う。`status` が `outdated` を返した場合は exe 移動または upgrade 後の登録ずれなので、`stop` → `install` → `start` の順で修復する。旧 exe のプロセスが生きている間は、安全のため新 daemon の `start` は失敗する。

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
- AEの `mcp-bridge-auto.jsx` を開く
- `Auto-run commands` を ON にする
- `list-ae-instances` / `list-premiere-instances` の `inactiveInstances` を確認し、`heartbeat is stale`、parse error、空の `instanceId` などの理由を見る

3. `method not found`（MCP）
- クライアントが `serve-stdio` で起動しているか確認
- 古いNode設定が残っていないか確認

4. panel / UXP を開いたまま host app を再起動した後に instance が見えない
- AE: `mcp-bridge-auto.jsx` を一度閉じて開き直し、heartbeat task のログを確認
- Premiere UXP: `Window > UXP Plugins > Premiere MCP Bridge` を開き、Instance 表示と `~/Documents/pr-mcp-bridge/instances/<instanceId>/heartbeat.json` の更新時刻を確認
- Premiere CEP fallback: `~/Documents/pr-mcp-bridge/instances/<instanceId>/heartbeat.json` が作成されているか確認

## 4. 監視ポイント

1. daemon 稼働状態（Windows: `<host>-mcp autostart status`、macOS: `<host>-mcp service status`）
2. 結果ファイル更新時刻
3. MCPクライアントの呼び出し失敗率

## 5. 障害時ログ採取

1. 実行コマンドと出力（stdout/stderr）
2. `ae_command.json` / `ae_mcp_result.json` の内容
3. AEバージョン、OSバージョン、実行ユーザー権限
