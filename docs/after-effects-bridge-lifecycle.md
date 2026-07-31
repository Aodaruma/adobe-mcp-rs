# After Effects bridge lifecycle

repository実装ではAfter Effects bridgeを `Scripts/Startup/mcp-bridge-startup.jsx` からheadlessに開始する。ScriptUI panelや `Auto-run commands` checkboxには依存しない。Windows版After Effects 2026（26.2.1x2）では、panelを閉じたcold start、両起動順、workspace変更、runtime restart、modal復帰、複数version routing、正常・異常終了を実機確認済み。After Effects 2025（25.6.5x3）は現行Startup JSXをAdobe公式の`afterfx -r`で評価した範囲で確認済みだが、現行installerからのcold start確認は残る。

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
aeMcpBridgeEnsureHealthy();
aeMcpBridgeGetDiagnosticsConfig();
aeMcpBridgeSetDiagnosticsEnabled(true);
```

`aeMcpBridgeGetState()` はrunning状態、instance ID、runtime generation ID、command/heartbeat task IDを返す。restartではinstance IDを維持し、runtime generation IDを更新する。

`aeMcpBridgeGetState()` の `healthy` / `healthReason` / `heartbeatAgeMs` は、task IDが存在するだけでなく、実際にheartbeatが更新されているかを表す。`aeMcpBridgeEnsureHealthy()` はstale状態を検出した場合にruntime generationを再作成する。

## Diagnostic logging

詳細なdebug logは既定でOFF。ログ先は書き込み可能な `~/Documents/ae-mcp-bridge/ae_mcp_debug.log` とする。次のどの経路からでも手動で切り替えられる。

- diagnostics panelの `Write diagnostic debug log`
- MCP `configure-bridge-diagnostics` の `{ "enabled": true | false }`
- `~/Documents/ae-mcp-bridge/ae_mcp_debug_config.json`
- ExtendScript API `aeMcpBridgeSetDiagnosticsEnabled(true | false)`

MCP `get-bridge-diagnostics` は現在の設定、active / inactive instance、最新のscheduler incident、debug log末尾を返す。設定ファイルをMCPから変更した場合、稼働中runtimeは次回tickで再読込する。schedulerがすでにstaleなら、panel / Startup restart後に適用する。

verbose debug logがOFFでも、stale scheduler検出時の単発snapshot `ae_mcp_scheduler_diagnostic.json` は保存する。これは常時追記ログではなく、次回の原因切り分けに必要な最小情報だけを保持する。

## Recovery behavior

- file bridgeなのでdaemonとの常時socket接続は持たない。AEとdaemonの起動順に関係なく、daemonは更新中のheartbeatを再検出する。
- runtime再評価時は旧taskをcancelする。旧callbackが残ってもgeneration ID不一致でcommand処理を行わない。
- command pollingとheartbeatは `repeat=true` の反復taskではなく、完了後に次回を登録するone-shot taskを使う。modal dialogや長時間のhost処理でcallbackが遅延しても、復帰後に1回だけ実行して次回を再登録する。
- workspaceがScriptUI panelを復元した場合、panelは既存のStartup-owned runtimeへ診断UIとしてattachする。panelを閉じてもheadless runtimeは停止しない。
- diagnostics panelのopen / refresh / activateとStartup bootstrap再評価はheartbeat ageを検査する。task IDだけ残ったstale runtimeは自動的にrestartする。
- headless modeでは権限警告dialogを出さない。heartbeatが無い場合はfile/network access設定、Startup/runtimeの配置、bootstrap stateを確認する。
- `$.global.__adobeMcpBridgeBootstrapState` に `running`、`runtime-not-found`、`runtime-api-missing`、`error` などのbootstrap状態が残る。
- 同じbootstrap snapshotを `~/Documents/ae-mcp-bridge/ae_mcp_bootstrap.json` に保存する。継続中の稼働状態は各instanceの `heartbeat.json` を正とする。
- stale schedulerを検出した時点のruntime stateは `~/Documents/ae-mcp-bridge/ae_mcp_scheduler_diagnostic.json` に保存する。
- Windows版ExtendScriptの `File.rename` は既存fileを置換できない。heartbeatはfile名を常に存在させるためin-place更新し、Rust readerのpartial JSON retryと組み合わせる。command/resultは同一directoryのtemporary fileとbackup renameで公開する。

## Windows live result（2026-07-15〜16）

検証時のbinary/bridgeは`0.4.4`、daemonは`127.0.0.1:47655`。個人path、runtime ID、instance ID、request IDは記録から除外した。

| Check | Result |
|---|---|
| AE 2026 cold start | panel操作なしで`extendscript-startup` / `startup-headless` / bridge `0.4.4`を自動開始 |
| panel未表示・workspace変更 | diagnostics panel未表示のままDefaultからReviewへ変更後もheartbeat継続 |
| heartbeat安定性 | 32秒間に4秒間隔で8回確認し、全回存在・更新、backup residue 0 |
| raw JSX | `run-jsx` がAE 26.2.1x2で成功し、引数・runtime metadataを返却 |
| retained result | `get-jsx-result` で同じ `requestId` の完了結果を回収 |
| host-first / daemon-first | どちらもactive instanceを発見し、instance指定のraw JSX成功 |
| runtime restart | instance IDを維持してruntime IDとtask IDが更新され、5秒後も1 commandが1回だけ実行 |
| 複数version routing | AE 2025 / 2026を同時起動し、target無指定は曖昧性error、各instance ID指定は対応versionへ到達 |
| modal | 制御済み`alert`中はheartbeatがstaleになりrequestがtimeout。閉じた直後に同じruntime/taskでheartbeatが復帰し、retained result回収成功 |
| normal shutdown | AE終了直後にShutdown scriptがheartbeatを削除 |
| forced termination | heartbeatは残るが、10秒のstale threshold後にactive一覧から除外 |

### Version matrix

| Installed host | Result |
|---|---|
| AE 2023 / 23.6.9 | executableは存在するが、通常起動と`-r`起動のどちらもtargetable window/heartbeatが現れず、この環境では未検証。起動processは試験後に終了 |
| AE 2024 | installation directoryは残るが`AfterFX.exe`が無いためskip |
| AE 2025 / 25.6.5x3 | installed bridgeは旧`0.4.2`でpanelをworkspace復元した。現行Startup JSXを`afterfx -r`で評価後は`0.4.4` headlessへ移行し、AE 2026との同時routing成功。現行Shutdown JSXは未配置のため終了時heartbeat cleanupは未適用 |
| AE 2026 / 26.2.1x2 | installed 3 JSXが現行sourceとhash一致。cold startを含む上記matrix成功 |

### Package matrix

- 現branchからWindows ZIPを生成・展開し、3 JSXのSHA-256がsourceと一致し、同梱`ae-mcp.exe health`が成功した。
- WiX 7がPATHで先に見つかりOSMF EULAでMSI生成が止まったため、repositoryで検証済みの`.dotnet/tools/wix.exe`（5.0.2）を優先するようpackagerを修正した。`-RequireMsi`でMSI生成成功。
- `msiexec /a`のadministrative image展開は終了code 0、32 files、3 JSXのhash一致を確認した。
- 現sessionはstandard userで、実machine install / installed 0.4.2からのupgrade / uninstallにはUACが必要なため実行していない。release前に管理者sessionで`docs/installer-e2e.md` 3.3を完了する。

実機試験では、workspace panelとStartup runtimeが同じtarget engineのglobal変数を上書きする問題、heartbeat置換中にfileが一時消失する問題、`File.rename` 後に `File.name` が変化してresult publish先を誤る問題を検出し修正した。追加matrixでは、runtime APIが`running: true`でもbootstrap diagnosticだけ`false`を記録する問題を検出し、primitive/Boolean wrapperの両方を正規化して修正した。

## AE 2025 scheduler stale incident（2026-07-31）

AE 2025（25.6.5x3）のprocessとtarget engineは応答している一方、heartbeatとcommand pollingの両方が停止した。runtime stateには `running: true` と有効そうなtask IDが残っていたため、従来のdiagnostics panelは誤って `RUNNING` と表示した。daemon / stdio / file permission / bridge version不整合ではなく、AE内の反復 `scheduleTask` callbackが実行されなくなったことが直接原因だった。

停止時点ではscheduler event logが無効だったため、発火させた個別操作は確定できない。AEはmodal dialog待機中やrenderなどhostが制御を保持する処理中にscheduled scriptを実行できない。今回のAE 2025 processでは長時間のhost処理後も反復taskが復帰しなかった可能性が高い。

対策として次を実装した。

1. `repeat=true` を廃止し、one-shot callbackの完了後に次回taskを登録する。
2. `running` とtask IDだけでなくheartbeat age、callback登録、task IDを合わせてhealth判定する。
3. panel refresh / activateとStartup再評価でstale runtimeを再生成する。
4. heartbeat sequence、command poll sequence、last scheduler error、recovery履歴、render queue状態をheartbeatへ追加する。
5. stale検出時の直前stateを `ae_mcp_scheduler_diagnostic.json` に保存する。

## Limitations

- AE 2025は現行installerによるcold start、AE 2023はhost自体の起動、AE 2024は完全なinstallationで再確認が必要。
- sleep/resumeはPC全体と並行作業へ影響するため自動実行していない。専用release sessionで確認する。
- macOS実機はこのPCに無いため、AE 24/25/26、pkg/archive、launchdを未検証。
- Windows MSIの実install / upgrade / uninstallは管理者sessionで未検証。
- After Effectsがscript実行中に長時間占有される操作ではscheduled taskの実行が遅れる。
- CEP/invisible extensionはprimary方式に採用していない。理由と再評価条件は [ADR 0002](adr/0002-after-effects-headless-startup-bridge.md) を参照する。
