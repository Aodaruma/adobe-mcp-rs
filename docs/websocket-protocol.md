# UXP WebSocket protocol v1

Photoshop / Premiere UXPの直接通信。既存のMCP stdio→TCP brokerは変更しない。
brokerと同じloopbackポートでHTTP Upgradeを受け付ける。既定はPhotoshop 47657、Premiere 47656。
サーバーは `127.0.0.1` にbind、UXP接続先は `ws://localhost:PORT/uxp`。

## Bootstrapと認証

hostのbridge rootの `connection.json` に `transport: "websocket"`、`url`、
`protocolVersion: 1`、`hostId`、OS乱数による256bit `token` を保存する。
これは接続設定のみ。command/result/heartbeatファイルはUXP側に生成しない。
設定とtokenは同じOSユーザーが読み取れる。tokenをログやGitへ含めない。
manifestではHTTP Upgradeを含め `ws://localhost:PORT` / `http://localhost:PORT` のみ許可する。
UXPコードでも接続先をlocalhostのみに制限する。独自ポートの場合はmanifestも更新する。

最初のJSONメッセージは次の形。

```json
{"type":"hello","protocolVersion":1,"token":"<local secret>","sessionId":"uxp-<runtime id>","instance":{"hostId":"photoshop","instanceId":"ps-uxp-...","bridgeRuntime":"uxp","appVersion":"26.11.7","bridgeRoot":"","commandFile":"","resultFile":"","lastHeartbeatAt":""}}
```

`instance` は既存HostInstance schema。アプリ側が `lifecycleMode: "websocket"`、
`runtimeId: sessionId`、自分のbridge rootを設定し、file pathは空にする。
instanceIdはplugin設定に保存、sessionIdはpluginロードごとに生成し再接続では維持する。
ホスト不一致・認証失敗・古いsessionからの結果を拒否する。

## メッセージ

| 方向 | type | 主なフィールド／役割 |
| --- | --- | --- |
| app→UXP | welcome | protocolVersion, maxResultBytes。認証完了 |
| app→UXP | command | sessionId, requestId, command, args |
| UXP→app | received | sessionId, requestId。受信通知 |
| UXP→app | result | sessionId, requestId, result |
| app→UXP | resultAck | sessionId, requestId。永続保存済み |
| UXP→app | heartbeat | sessionId, status, currentRequestId。3秒間隔 |
| app→UXP | pong | heartbeatへの応答 |

resultは既存schemaの `status`、`_requestId`、`_commandExecuted` を含む。
1メッセージ上限16MiB、結果は設定 `max_result_bytes` 以下。大きな結果はartifact metadataを返す。
新規instanceからの要求実行は既存のinstance別FIFO・host内global排他を通す。
queued cancellationはdispatch前に原子的に確認する。送信済みcommandの強制停止はしない。

## 切断・再接続・再起動

1. アプリはdispatch前にrequestと `dispatchedAt` をregistryへatomic write・syncする。
2. UXPは同じrequestIdを二度実行しない。結果をACKまでメモリに保持する。
3. アプリはhost/instance/session/request/commandを照合し、結果を保存・syncしてからACKする。
4. UXPは0.5秒から最大10秒のbackoff（jitter付き）で再接続する。再送するのは未ACKの結果だけ。
5. 切断中もworkerはFIFOと排他を保持する。client timeoutは実行の終了ではない。
6. アプリ再起動時は未解決のdispatch記録を読み、そのhostで新しいcommandの受付を止める。
   同じUXP sessionから結果を回収できれば通常受付に戻る。未dispatchの旧queueは失敗として記録する。

plugin終了・Adobe終了でUXPメモリを失った場合、結果の自動回収はできない。
結果不明のcommandは再送しない。元の処理が終わったことと成果物の状態を確認して運用上の復旧を判断する。
送信前に切断した場合も、送信したかを確定できない境界では安全側に倒して `unknown` を保持する。
同一instanceで元sessionの実行が未解決なら、新sessionが接続してもその実行を置換しない。

ファイル方式は `connection.json` の `transport: "file"` で明示選択しpluginを再ロードする。
実行中にWebSocketからファイルへ自動フォールバックしない。
