# ADR 0002: After Effects bridgeをStartup ExtendScriptで自動起動する

- Status: accepted（Windows AE 2025/2026部分確認済み、実機matrix完了まではpreview）
- Date: 2026-07-15
- Issue: #22

2026-10-26追記: 実行系の前提と代替方式の根拠を[仕様・設計監査](../ae2025-architecture-audit-2026-10-26.md)で再評価した。本ADRのStartup方式を直ちに置換したものではないが、専用engineによる隔離や時間内応答は前提としない。

## Context

従来の `mcp-bridge-auto.jsx` は、ScriptUI panelをWindowメニューから開き、`Auto-run commands` checkboxをONにしている間だけcommandをpollしていた。installerのStartup loaderもpanelのmenu commandを遅延実行する方式だったため、workspace復元、panel close、menu command名、UI controlの寿命にbridge lifecycleが引きずられていた。

目的はJSXを全廃することではない。After Effects DOMを操作するExtendScriptは維持しつつ、次を実現する。

- AE起動時にworkspace/panel操作なしでbridgeを開始する
- daemon先行・AE先行のどちらでもheartbeatによって相互発見できる
- script再評価時にpollerを重複させない
- 明示的な停止・再初期化手段を持つ
- bridge protocol v1と既存command/result pathを維持する

## Decision

`Scripts/Startup/mcp-bridge-startup.jsx` をprimary bootstrapとして採用する。bootstrapはAEが選ぶ共有スクリプト実行環境で `ScriptUI Panels/mcp-bridge-auto.jsx` をheadless modeとして直接評価する。AE2025の実測は `main` であり、`#targetengine` による専用実行系や独立threadを前提としない。menu commandやWindow生成は行わない。`Scripts/Shutdown/mcp-bridge-shutdown.jsx` はnormal終了時にtaskを停止し、heartbeatを削除する。異常終了時はdaemonのstale判定へfallbackするが、stale自体はprocess終了や予約消失の証明ではない。

runtimeは `$.global.__adobeMcpBridgeRuntime` にsingleton lifecycle APIを公開する。

- `aeMcpBridgeStart()`
- `aeMcpBridgeStop()`
- `aeMcpBridgeRestart()`
- `aeMcpBridgeGetState()`

各 `scheduleTask` はruntime generation IDを引数に含める。再評価時は以前のtask IDをcancelし、cancelに失敗した古いcallbackもgeneration不一致としてno-opにする。heartbeatには `lifecycleMode`、`runtimeId`、`runtimeStartedAt` を追加するが、protocol version、instance folder、command/result schemaは変更しない。

runtime本体は評価ごとのIIFE scopeへ隔離する。workspaceがScriptUI panelを復元した場合は、新runtimeでStartup runtimeを置き換えず、既存runtimeへ診断panelとしてattachする。このpanelのcloseはStartup-owned taskを停止しない。

Windows版ExtendScriptでは既存fileへの `File.rename` が失敗し、rename後に `File` objectのname/pathも変化する。heartbeatはin-place更新とreader retryを採用し、command/resultのatomic fallbackでは元のtarget nameをrename前に保持する。

ファイル書き込み権限が無い場合、headless bootstrapはdialogを出さずpollingを継続する。設定が有効になった後のtickで自動復帰する。設定変更後にAE再起動する手順は引き続き推奨する。

## Why not CEP now

当初は、CEP 12 CookbookのInvisible HTML Extensions対応表にAfter Effectsがないことと、通常Panelのworkspace依存lifecycleを理由に、cold-start改善の確証がないと判断した。2026-10-26の再調査では、Adobe公式Samples READMEだけでなく、CEP12のInvisible sample manifestにも `AEFT`・`Custom`・`AutoVisible=false` が含まれると確認した。対応表の不掲載を「AEは不可視CEP非対応」の根拠にはしない。ただし記載のStartOn eventがAE2025でpanel非依存のcold startを保証する契約・実測は未確認である。

また、CEP化してもAE DOM操作はhost側ExtendScriptへ委譲するため、JSX全廃にはならない。CEF/Node runtime、署名・配布、host version互換という追加面も増える。このためCEP skeletonは現時点では追加せず、Startup方式の実機matrixを優先する。

CEPは通信側の寿命を分離する比較候補として、AE2025の最小PoCで再評価できる。一方でhost側ExtendScriptはmain threadで実行されるため、DOM実行の待ちを解消する保証にはならない。native AEGPもhost側adapterの比較候補とするが、SDKの契約と実機受入なしに常時実行・確実な復旧を保証しない。方式選択に先立ち、要求の確定・重複実行防止・状態分類・復旧入口の契約を整理する。

## Evidence

- Adobe Help: [Scripts in After Effects](https://helpx.adobe.com/after-effects/using/scripts.html)
  - AE起動時のScripts読込、ScriptUI PanelsとWindowメニューの関係、file/network access設定を説明している。
- After Effects Scripting Guide: [Loading and running scripts](https://ae-scripting.docsforadobe.dev/introduction/overview/)
  - `Scripts/Startup` はplugin初期化後にalphabetical orderで自動実行され、同一sessionのglobal environmentが維持される。
- After Effects Scripting Guide: [Application.scheduleTask / cancelTask](https://ae-scripting.docsforadobe.dev/general/application/)
  - repeating task IDの取得とcancelを定義している。
- Adobe CEP: [CEP 12 HTML Extension Cookbook](https://github.com/Adobe-CEP/CEP-Resources/blob/master/CEP_12.x/Documentation/CEP%2012%20HTML%20Extension%20Cookbook.md)
  - AEのCEP対応、Panel lifecycle、Invisible HTML Extensionsの条件とhost対応表を掲載している。
- Adobe CEP: [CEP12 Invisible sampleのmanifest](https://raw.githubusercontent.com/Adobe-CEP/CEP-Resources/master/CEP_12.x/Samples/CEP_HTML_Invisible_Extension-12.0/CSXS/manifest.xml)
  - AEFTを対象に含む。Cookbookの表との不一致と、AE2025のcold-start保証が未確認である点を分ける。
- Adobe: [JavaScript Tools Guide CC、235ページ](https://github.com/Adobe-CEP/CEP-Resources/blob/master/ExtendScript-Toolkit/Guides/JavaScript%20Tools%20Guide%20CC.pdf)
  - 古い原典で#targetengineの対応アプリと制限を説明する。今回のAE2025 main実測と合わせて、専用engineによる隔離を前提から外した。

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

2026-07-15〜16にWindows版After Effects 2026（26.2.1x2）で、panelなしcold start、両起動順、workspace変更、runtime restart、modal復帰、normal/forced shutdownを確認した。After Effects 2025（25.6.5x3）へ現行Startup JSXを`afterfx -r`で評価し、2026との同時起動、曖昧性guard、instance別routingも確認した。詳細は [After Effects bridge lifecycle](../after-effects-bridge-lifecycle.md) を参照する。AE 2023/2024、2025のinstaller cold start、macOS、sleep/resume、Windows MSIの実upgrade/uninstallは未確認のためIssue #22は継続する。
