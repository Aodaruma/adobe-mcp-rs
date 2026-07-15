# After Effects bridge lifecycle

After Effects bridgeは `Scripts/Startup/mcp-bridge-startup.jsx` からheadlessに開始する。ScriptUI panelを開くことや `Auto-run commands` checkboxは通常運用では不要。

## Installed files

- `Scripts/Startup/mcp-bridge-startup.jsx`: lifecycle bootstrap
- `Scripts/Shutdown/mcp-bridge-shutdown.jsx`: normal終了時のtask停止とheartbeat削除
- `Scripts/ScriptUI Panels/mcp-bridge-auto.jsx`: command runtime。bootstrapから直接評価される

両方を配置した後、After Effectsを再起動する。`Allow Scripts to Write Files and Access Network` は引き続き有効にする必要がある。

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
- headless modeでは権限警告dialogを出さない。heartbeatが無い場合はfile/network access設定、Startup/runtimeの配置、bootstrap stateを確認する。
- `$.global.__adobeMcpBridgeBootstrapState` に `running`、`runtime-not-found`、`runtime-api-missing`、`error` などのbootstrap状態が残る。

## Limitations

- AEの実プロセス上でのcold start、sleep/resume、modal dialog、複数version同時起動は自動testでは代替できない。ADR 0002のmatrixで実機確認する。
- After Effectsがscript実行中に長時間占有される操作ではscheduled taskの実行が遅れる。
- CEP/invisible extensionはprimary方式に採用していない。理由と再評価条件は [ADR 0002](adr/0002-after-effects-headless-startup-bridge.md) を参照する。
