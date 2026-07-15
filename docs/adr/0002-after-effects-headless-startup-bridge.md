# ADR 0002: After Effects bridgeをStartup ExtendScriptで自動起動する

- Status: accepted（実機matrix完了まではpreview）
- Date: 2026-07-15
- Issue: #22

## Context

従来の `mcp-bridge-auto.jsx` は、ScriptUI panelをWindowメニューから開き、`Auto-run commands` checkboxをONにしている間だけcommandをpollしていた。installerのStartup loaderもpanelのmenu commandを遅延実行する方式だったため、workspace復元、panel close、menu command名、UI controlの寿命にbridge lifecycleが引きずられていた。

目的はJSXを全廃することではない。After Effects DOMを操作するExtendScriptは維持しつつ、次を実現する。

- AE起動時にworkspace/panel操作なしでbridgeを開始する
- daemon先行・AE先行のどちらでもheartbeatによって相互発見できる
- script再評価時にpollerを重複させない
- 明示的な停止・再初期化手段を持つ
- bridge protocol v1と既存command/result pathを維持する

## Decision

`Scripts/Startup/mcp-bridge-startup.jsx` をprimary bootstrapとして採用する。bootstrapは同じpersistent target engine内で `ScriptUI Panels/mcp-bridge-auto.jsx` をheadless modeとして直接評価する。menu commandやWindow生成は行わない。`Scripts/Shutdown/mcp-bridge-shutdown.jsx` はnormal終了時にtaskを停止し、heartbeatを削除する。異常終了時はdaemonのstale判定へfallbackする。

runtimeは `$.global.__adobeMcpBridgeRuntime` にsingleton lifecycle APIを公開する。

- `aeMcpBridgeStart()`
- `aeMcpBridgeStop()`
- `aeMcpBridgeRestart()`
- `aeMcpBridgeGetState()`

各 `scheduleTask` はruntime generation IDを引数に含める。再評価時は以前のtask IDをcancelし、cancelに失敗した古いcallbackもgeneration不一致としてno-opにする。heartbeatには `lifecycleMode`、`runtimeId`、`runtimeStartedAt` を追加するが、protocol version、instance folder、command/result schemaは変更しない。

ファイル書き込み権限が無い場合、headless bootstrapはdialogを出さずpollingを継続する。設定が有効になった後のtickで自動復帰する。設定変更後にAE再起動する手順は引き続き推奨する。

## Why not CEP now

CEP 12はAfter Effects自体を対応hostとして掲載している。一方、公式CookbookのInvisible HTML Extensions対応表にはAfter Effectsが含まれていない。通常のPanelは「終了時に開いていれば次回起動時に再度開く」というworkspace依存のlifecycleであり、今回解消したいcold-start問題を確実に改善すると判断できない。

また、CEP化してもAE DOM操作はhost側ExtendScriptへ委譲するため、JSX全廃にはならない。CEF/Node runtime、署名・配布、host version互換という追加面も増える。このためCEP skeletonは現時点では追加せず、Startup方式の実機matrixを優先する。

CEPは、AdobeがAEでのinvisible extension / `StartOn` の起動保証を明文化するか、Startup方式に再現可能な欠陥が見つかった場合に再評価する。native AEGPは、ExtendScriptでは実現不能なlifecycle要件が確認された場合だけ検討する。

## Evidence

- Adobe Help: [Scripts in After Effects](https://helpx.adobe.com/after-effects/using/scripts.html)
  - AE起動時のScripts読込、ScriptUI PanelsとWindowメニューの関係、file/network access設定を説明している。
- After Effects Scripting Guide: [Loading and running scripts](https://ae-scripting.docsforadobe.dev/introduction/overview/)
  - `Scripts/Startup` はplugin初期化後にalphabetical orderで自動実行され、同一sessionのglobal environmentが維持される。
- After Effects Scripting Guide: [Application.scheduleTask / cancelTask](https://ae-scripting.docsforadobe.dev/general/application/)
  - repeating task IDの取得とcancelを定義している。
- Adobe CEP: [CEP 12 HTML Extension Cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md)
  - AEのCEP対応、Panel lifecycle、Invisible HTML Extensionsの条件とhost対応表を掲載している。

## Verification matrix

実機ではWindows/macOSの各supported AE versionについて、次を記録する。

| Scenario | Expected |
|---|---|
| AE先行、daemon後起動 | panel操作なしでheartbeatが存在し、daemonがinstanceを発見する |
| daemon先行、AE後起動 | AE起動後にheartbeatが現れ、既存daemonからcommandを実行できる |
| workspace変更・reset | heartbeatとcommand pollingが継続する |
| startup script再評価 | task IDは各1個、同じcommandを一度だけ処理する |
| `aeMcpBridgeStop()` | task停止、heartbeat削除、stale後にinactiveになる |
| `aeMcpBridgeRestart()` | 新runtime IDでheartbeatとpollingが復帰する |
| file/network access無効→有効 | 無効時はcrash/dialog loopせず、有効化・再起動後に復帰する |
| modal dialog、sleep/resume | dialog終了・resume後にheartbeatとpollingが復帰する |
| AE終了・再起動 | 古いheartbeatはstale扱いになり、新instanceが発見される |

実機結果は `docs/bridge-contract-testing.md` のartifact schemaに従って保存する。
