# After Effects bridge lifecycle

repository実装ではAfter Effects bridgeを `Scripts/Startup/mcp-bridge-startup.jsx` からheadlessに開始する。ScriptUI panelや `Auto-run commands` checkboxには依存しない。Windows版After Effects 2026（26.2.1x2）では、panelを閉じた通常のcold-start、daemon先行・後発、raw JSX、retained result、daemon再起動後の再接続、正常終了時のheartbeat削除を実機確認済み。

## Installed files

- `Scripts/Startup/mcp-bridge-startup.jsx`: lifecycle bootstrap
- `Scripts/Shutdown/mcp-bridge-shutdown.jsx`: normal終了時のtask停止とheartbeat削除
- `Scripts/ScriptUI Panels/mcp-bridge-auto.jsx`: command runtime。bootstrapから直接評価される

3ファイルを配置した後、After Effectsを再起動する。`Allow Scripts to Write Files and Access Network` は引き続き有効にする必要がある。

## Lifecycle API

ExtendScript consoleまたは `File > Scripts > Run Script File` から同じtarget engineへアクセスするdiagnostic scriptで、次を利用できる。

```jsx
aeMcpBridgeGetState();
aeMcpBridgeStop();
aeMcpBridgeStart();
aeMcpBridgeRestart();
```

`aeMcpBridgeGetState()` はrunning状態、instance ID、runtime generation ID、command/heartbeat task IDを返す。restartではinstance IDを維持し、runtime generation IDを更新する。

## Recovery behavior

- file bridgeなのでdaemonとの常時socket接続は持たない。AEとdaemonの起動順に関係なく、daemonは更新中のheartbeatを再検出する。
- runtime再評価時は旧taskをcancelする。旧callbackが残ってもgeneration ID不一致でcommand処理を行わない。
- workspaceがScriptUI panelを復元した場合、panelは既存のStartup-owned runtimeへ診断UIとしてattachする。panelを閉じてもheadless runtimeは停止しない。
- headless modeでは権限警告dialogを出さない。heartbeatが無い場合はfile/network access設定、Startup/runtimeの配置、bootstrap stateを確認する。
- `$.global.__adobeMcpBridgeBootstrapState` に `running`、`runtime-not-found`、`runtime-api-missing`、`error` などのbootstrap状態が残る。
- 同じbootstrap snapshotを `~/Documents/ae-mcp-bridge/ae_mcp_bootstrap.json` に保存する。継続中の稼働状態は各instanceの `heartbeat.json` を正とする。
- Windows版ExtendScriptの `File.rename` は既存fileを置換できない。heartbeatはfile名を常に存在させるためin-place更新し、Rust readerのpartial JSON retryと組み合わせる。command/resultは同一directoryのtemporary fileとbackup renameで公開する。

## Windows AE 2026 live result（2026-07-15）

| Check | Result |
|---|---|
| panelを閉じた通常起動 | `startup-headless`、bridge `0.4.4` を自動開始 |
| heartbeat安定性 | 32秒間に4秒間隔で8回確認し、全回存在・更新、backup residue 0 |
| raw JSX | `run-jsx` がAE 26.2.1x2で成功し、引数・runtime metadataを返却 |
| retained result | `get-jsx-result` で同じ `requestId` の完了結果を回収 |
| daemon再接続 | AEを維持したままdaemonを停止・再起動し、同じruntimeへ再度raw JSX成功 |
| normal shutdown | AE終了直後にShutdown scriptがheartbeatを削除 |

実機試験では、workspace panelとStartup runtimeが同じtarget engineのglobal変数を上書きする問題、heartbeat置換中にfileが一時消失する問題、`File.rename` 後に `File.name` が変化してresult publish先を誤る問題を検出し修正した。

## Limitations

- Windows AE 2023–2025、macOS、sleep/resume、modal dialog、複数version同時起動は未検証。ADR 0002のmatrixで実機確認する。
- After Effectsがscript実行中に長時間占有される操作ではscheduled taskの実行が遅れる。
- CEP/invisible extensionはprimary方式に採用していない。理由と再評価条件は [ADR 0002](adr/0002-after-effects-headless-startup-bridge.md) を参照する。
