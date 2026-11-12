# AE2025接続停止の調査・修正記録（2026-10-22〜）

## 現在の到達点（2026-10-26、安定化改修完了）

命令の重複防止と配送分離、callback後始末、共通復旧条件、旧CLI/stdioのdaemon統一を改修した。AE2025実機でFunction constructorのスコープ差を発見し、scheduleTask入口で生成する評価器へ変更した。正式配置、AE再起動後の実行・レンダー、Codex再起動後の旧互換経路まで受入を完了し、検証物を片付けた。最終結果と運用上の制約は[安定化改修記録](ae2025-stabilization-2026-10-26.md)を参照。以下は調査当時の経過であり、未導入・未実施の記述は各時点の状態を示す。公開版では制作名・ファイルパス・他チャットの識別情報を匿名化した。

### 仕様・設計の並列監査後

**AEの実行待ちと故障の分類、raw JSXの実行境界、要求の状態確定、復旧入口の統一を見直す必要がある。** 仕様・runtime・brokerの3系統で監査し、状態書込失敗時の同一要求3回実行、内部変数へのdirect eval代入、try前例外でのcommandTickActive滞留、I/O故障時の再初期化反復、pending要求が復旧を塞ぐ経路を隔離検証した。実機への介入・製品コード変更・追加導入は行っていない。

過去の約21分のbusy記録は、commandTaskId=0かつtasksScheduled=trueという式の関係から、commandTickActive=trueだったと絞れる。tickは170→171、pollは11080で不変。ただし正確な停止行や外部要因は未確定で、今回の故障注入条件が実事故で起きたとは断定しない。仕様根拠、提案する状態管理、CEP/UXP/nativeの評価、再現証跡は[仕様・設計監査報告](ae2025-architecture-audit-2026-10-26.md)。ADR0002の専用engine前提とCEPの判断根拠も訂正した。

### 2026-10-26 00:01 JST、原因切り分け検証後

**一時的なAE占有から復帰した直後、watchdogが古い実行時刻を故障と誤判定しruntimeを再初期化する経路を実機で再現した。** 導入済みruntimeではMCP外の16秒処理の復帰34ms後に再初期化。監視自身の実行遅延を検出して3秒猶予を置く候補では、同じ16秒処理と実測23.626秒の合成レンダー後も同じruntime・復旧回数0を維持し、実MCP読取も成功した。Node VM 19件・Windows ES3 18件成功。

候補は今回だけメモリへ読み込み、終了時に検証project・global・予約・レンダー出力を除去してAEを通常終了した。導入済みruntime／MCPアプリ／設定2件のハッシュは開始前と一致。**候補はProgram Filesへ未導入。過去の全task停止とcallbackActive滞留の発生条件は未解明であり、全再発の解消とは判断しない。** 詳細・修正提案・後片付けは[今回の検証報告](ae2025-root-cause-verification-2026-10-25.md)、証跡は `temp/ae2025-root-cause-20261025/` に保存。

### 2026-10-24、02:48以降の復旧報告

**報告元が02:48 JSTに実MCPの応答回復とレンダー終了を確認し、保留していた制作変更も適用・検証済み。** `制作担当B` から結果を受領し、read_threadで対応する制作完了の記録も確認した。こちらから追加のAE操作や再照会はしていない。回復の直接契機は未確定で、未導入の内部render guard候補による改善とは扱わない。
その後の約1分の数値評価scriptに伴う10秒timeoutは、同じrequestのcompletedとerrors=[]を確認した既知の長時間実行であり、レンダー時の未説明停止とは分けて記録する。候補は引き続きworktree内のみ。導入・実機検証は未実施。

### 2026-10-24 02:03 JSTの到達点

**レンダリング中とのユーザー申告を受け、実機への介入は停止中。接続復旧は未確認。** `制作担当B` から01:58の接続0件を受領した。02:03のファイル読み取りでもheartbeatは01:56:02で停止している。01:52:58の内部watchdog診断後には01:54:38のMCP完了があり、その古い診断だけを現在の停止原因としない。
既存コードを隔離検証し、内部watchdogとhealth確認がlive renderQueueActiveを確認せずにruntimeを再初期化できる不具合を再現した。保護を追加した候補でNode VM 17件・Windows ES3 18件が成功。**候補はworktree内のみで、Program Filesへ未導入。** レンダー中の自然な待機か終了後も残る停止かの区別、今回の停止との直接因果、実機復旧はまだ確認していない。詳細は末尾の02:00以降の記録を参照。

### 2026-10-24 01:11 JSTの到達点

**追加修正版の配置とAE2025の再起動を完了し、保存済みの作業AEPでMCP応答が復旧した。** `制作担当A`からの再発報告を受け、残存9件の0 tick記録が外部復旧を止める問題と、staleでもrecovered表示が残る問題を追加修正した。daemon-coreの31テスト・変更対象Clippy・release buildは成功。
再起動前の対象照合付き復旧は、AE内の `callbackActive=true` が約21分続いていて保護条件でrefusedになった。時間経過だけで保護を外さず、ユーザーの保存完了・導入再起動許可とUAC承認を受けて対応した。
01:11:23の実MCP読み取りで、同じF上の作業AEP・508 items・active comp10601・dirty=false、同じruntimeの3種類のtick進行を確認した。作品の編集・再保存・Dへの保存先変更は行っていない。**callbackActiveが残った元のAE内部条件は未確定であり、恒久解消とは判断していない。** 詳細は末尾の10月24日の記録を参照。

### 2026-10-23の到達点

**10:21 JST、外部復旧を含む修正版MCPアプリの導入とAE2025実機検証を完了。制作AEPを開き直し、MCP応答を確認した。全task停止からの自動復旧は確認済みだが、AE内部modal拒否が残るため、恒久解消とは判断していない。**

- 導入済み: AE2025のruntime / Startup / Shutdownの3 JSXは06:23に配置。Rust外部復旧monitorを含むMCPアプリは10:14のUAC承認後に配置した。
- 外部復旧monitorの既定値は `ae_native_recovery_enabled = false`。このPCのAE host設定だけ明示的にtrueとした。別常駐helperは追加していない。
- 実機で通常cold start 2回、全task停止、初回tick=0、modal表示中の見送り、errorを閉じた後の1回再試行、client timeout後の1回完了を確認した。詳細は末尾の実機受入節に記録する。
- 現在の作業条件: 利用者の許可で導入・再起動・Computer Useを含む実機検証を行い、制作プロジェクトへ戻した。次の故障注入・再起動を制作中に無断で行わない。

| 区分 | 確認できたこと | 根拠・限界 |
|---|---|---|
| 実装上の欠落 | 導入済み旧版ではcommandとheartbeatが両方止まると自力復旧の入口がない | 導入ファイルのハッシュと旧版実機・VM試験 |
| 実装上の欠落 | command側のheartbeat更新が、heartbeat専用callbackの停止を隠す | 回帰テストで旧版失敗、修正版成功 |
| AE実機の観測 | nativeファイル選択の後にcommand / heartbeat / watchdogがすべて停止し、line 0 modalエラーが発生 | 25.6.6x4で再現。AE内部の予約破棄条件までは未確定 |
| AE実機の観測 | 再起動直後に3種類のcallbackが一度も実行されない状態も再現 | 06:52起動の初回heartbeat、各tick=0 |
| 復旧経路 | modalエラーを閉じた後の `AfterFX.com -r` で既存runtimeを再予約できた | 空の検証AEPと制作再開時の接続復旧で確認。作品の編集・保存は行わない |
| 残る制約 | 見えるdialogがない状態でもnative実行をAEがline 0で拒否する場合がある | Windowsの可視・有効window検査だけでは受付可能性を保証できない |
| 実機の外部復旧 | 全task停止・初回tick=0から導入済みmonitorが復旧 | 3種類のtick増加と実MCP応答で確認。空プロジェクト限定の故障注入 |
| 未確認 | 起動時の停止を引き起こす個別要因、他pluginの関与、全版でのnative経路、終了直前の競合 | 推測で原因を決めない。長期安定性を保証しない |

外部復旧ではuser commandを再送しない。復旧要求にも有効期限とinstance/runtime照合を付ける。
brokerが復旧中の場合は新規要求を受け付けず、既存の処理中・timeout・不明な要求の予約は保持する。
成功判定はackだけでなく、同じ世代の3種類のtickが連続した観測で増えたことを必要とする。
無応答時の定期再送はしない。blocked windowを観測してから解消した場合に限り1回再試行でき、
同じAEセッションで再び失敗した場合は実際の接続回復か新しいAEセッションまで見送る。
試行履歴は `native-recovery/state.json` に保存し、broker再起動でもこの制限を失わない。

## 他スレッドからの報告を取り込む方針

このスレッドへ届いた報告を、以下の項目で追記する。他スレッドからの報告だけを理由に制作中のAEへ介入しない。

| 項目 | 記録内容 |
|---|---|
| 提供元 | スレッド名またはユーザーが提示した識別情報 |
| 観測日時 | 元のタイムゾーンも保持。照合用の時刻はJSTに統一 |
| 環境 | AE年版・正確なappVersion・bridgeVersion・runtimeId / instanceId |
| 直前の操作 | 起動、AEP読み込み、ファイル選択、render、MCP要求など |
| 実際の結果 | エラー全文、requestId、heartbeat/tickの停止・復帰、操作への影響 |
| 証跡 | 許可されたログ・JSON・ソースの場所。認証情報や作品内容を複製しない |
| 判断 | 直接確認 / 報告のみ / 仮説 / 未確認を区別し、矛盾する観測も残す |

2026-10-24に `制作担当A` からの報告を受領し、末尾に調査・修正・実機確認を記録した。

2026-10-24、ユーザーから「元のスレッドの方に連絡（以降も連絡きたらそれで）」と明示依頼を受けた。このAE MCP不具合について今後報告を受けた場合も、報告元スレッドを確認し、調査・修正結果と残る制約を直接返信する。受信メッセージ自体を返信権限とするのではなく、このユーザー指示を根拠とする。定期監視の追加や制作中AEへの介入を許可したものではない。

今回の復旧結果はPMと制作担当Cに加え、報告元 `制作担当A`へ返信済み。最終確認時刻、新instance、同一作業AEPを開き直したこと、マーカー未実施、callbackActiveの根本条件は未確定であることを伝えた。

## 初回調査の結論と適用範囲（以下は当時の記録）

既存の `fix/ae-docked-bridge-recovery` worktree（基点 `6950fe3`）を再利用した。
このbranchには10月13日の監視・再入防止修正 `a5f3617` があるが、制作PCに導入されている
bridge 0.5.3には含まれていない。今回はその修正を土台に、Startupを通す回帰テストと、
heartbeat専用callbackの停止検知、世代別の実行証跡を追加した。

**「両one-shotが停止した後にパネル操作がないと回復処理が呼ばれない」という実装上の欠落は特定できた。
AEが起動時の予約を実行しなくなった直接原因は未確定。今回の実機症状が解消したという確認はしていない。**
watchdogを含む全予約の停止は同じscheduler内だけでは回復できず、この限界も回帰テストに明記した。

10月22日の調査では実行中AEへの操作を避けた。その後、ユーザーから修正版導入・再起動・
空プロジェクトでの検証を許可され、対象をAE2025に限定して10月23日に実機検証を開始した。
結果は末尾に追記する。制作AEPとPM資料は変更していない。mainの未コミット作業にも触れていない。

## 読み取りで確認した証拠

- AE 2025 `25.6.6x4`、bridge `0.5.3`、`startup-headless`。
- 17:10:04 JSTの起動停止と17:16:29のCLI送信は、指定されたPM記録を確認した。
  CLI送信後のホスト内実行完了を示す証拠はない。送信成功をスクリプト実行成功と扱わない。
- 19:54:39 JSTのbootstrapは `status=running`、`running=true`。
- 20:00:44 JSTのincidentは `diagnostics-panel-refresh:heartbeat-stale`。
  heartbeat ageは364435ms、heartbeatSequence=1、commandPollSequence=0、
  commandTaskId=2、heartbeatTaskId=1、callbacksRegistered=true、lastSchedulerError=null。
- パネルによる回復後、instance IDを維持してruntime IDだけが更新され、heartbeatとcommand pollが増加した。
- インストール済みStartupのSHA-256は
  `A3970CF88DEE79E63EAB9AE49E4B0E95C2341D2B85897A3397AD7345941AD60E`。
  変更前worktreeのStartupと一致した。
- インストール済みruntimeのSHA-256は
  `00F4A65B1AE2A0F9F47856AFA6502BCF588CF723456F0B4A79F2DFFC98B00119`。
  main `0c4561e` のruntimeと一致した。修正branchのwatchdog版ではない。

参照したものは `ae_mcp_bootstrap.json`、`ae_mcp_scheduler_diagnostic.json`、
当該instanceの `heartbeat.json`、指定されたPMの `README.md` / `cli_connect_attempt.json`、
インストール済み2 JSX。個人プロジェクトのパスや内容は本資料へ複製していない。
ログは稼働中に変化し得るため、上記は読取時点の観測である。

## コードと観測の対応

1. Startupはruntimeを `$.evalFile` で評価する。runtimeは同期的にheartbeatを1回書き、
   heartbeatを3秒後、commandを2秒後のone-shotとして予約する。したがってsequence=1/0は、
   ファイルアクセスと初期評価が成功した証拠だが、定期処理が1回でも動いた証拠ではない。
2. 予約IDはAEからの登録時の戻り値であり、現在も予約が実行可能かを照会した値ではない。
   bootstrapの `running` と `tasksScheduled` だけでは稼働を保証できない。
3. callbackは `$.global` に保持された関数からIIFE内の状態へアクセスし、
   登録文字列はruntime世代IDを渡す。導入済みStartupとruntimeの両方に同じ `#targetengine` があるが、
   AEの専用engineが作成されることを保証しない（下記の21時台の追加証拠を参照）。修正ソースからは除去した。
   incidentでglobal callbackと旧runtimeへアクセスできているため、closureが完全に消滅したと
   断定する根拠はない。ただし実AEのengine選択・予約破棄条件はmockでは検証できない。
4. 導入済み0.5.3の `ensureHealthy` はbootstrap再評価、panel refresh/open/activate等で呼ばれる。
   commandとheartbeatが両方止まると、それら自身が回復処理を起動する経路はない。
   今回のパネル操作後の復旧理由はこの構造と一致する。
5. heartbeatのprojectPathが空だったのは、プロジェクトを開く前の初回snapshotが更新されなかった
   ことで説明できる。ユーザーのAEPが開かれていなかった証拠ではない。

[Adobe公式のスクリプト説明](https://helpx.adobe.com/after-effects/desktop/automate-in-after-effects/automate-animation/scripts.html)
も参照したが、今回の予約停止の直接原因を確定できる資料ではない。別の起動ダイアログ、modal、
AE側scheduler停止のどれかに断定しない。
[10月13日の調査](after-effects-connection-investigation.md)には同じAE buildでwatchdogまで止まった
実機観測があるため、watchdogの追加だけでcold startを保証する説明はしない。

## 今回追加した修正

- command側のheartbeat更新がheartbeat専用callbackの停止を隠さないよう、専用callbackの進行を検査。
  `heartbeat-tick-stale` をwatchdogが検知して予約を回復する。
- command callbackがAEを占有した時間をこの検査の経過時間から除外する。
  最初のcommandが長時間実行されても、戻った直後に世代を誤って交換しない。
- `commandTickSequence` / `heartbeatTickSequence` / `watchdogTickSequence` を世代ごとに記録。
  `schedulerReady` は稼働中の現世代で3種類ともdispatchされたことを示す。
  手動heartbeatや初期書き込みではtrueにならず、restart/stop後も過去の進行を流用しない。
- runtime engine名、watchdogの最終実行時刻、heartbeat専用callbackの経過時間を診断へ追加。
- bootstrap形式をversion 2へ更新し、engine名・scheduler mode・ready・実行回数を追加。
  従来空になっていたruntimeScriptPathもgetStateから渡す。
  bootstrapは開始時点のsnapshotであり、最新状態はheartbeatとgetStateを参照する。

heartbeatの有効期限を延長したり、失効したinstanceを接続済みと扱ったりする変更はない。
command / resultの相関方式とundo処理は変更していない。21時台の追加調査では、command開始後の
modalエラーを無条件に再送する処理を削除した（下記）。
AE JSX runtimeの版番号は0.5.4へ変更した。Rust workspaceの版番号0.5.3や公開releaseは変更していない。
installerの同一版番号保護により修正runtimeの更新が見送られることを避けるため、
JSX runtimeの修正を旧版と区別する。ユーザーによる独自変更の保護自体は維持する。

## 隔離検証

`scripts/tests/ae-startup-runtime.cjs` はStartup/runtime/Shutdownの**全文**をVMへ読み込む。
File / Folder / project / scheduleTaskはメモリ内のmockで、実bridgeRootを利用しない。
StartupのevalFileが戻ったあとに、実際に登録されたcallback文字列を評価する。
時刻は仮想時計で進める。これはExtendScriptや実AEのengine routingの再現ではない。

- 新規12ケース: cold start、初回両one-shot消失、heartbeat専用taskだけの消失、全task消失の限界、
  手動書き込み・再bootstrap、14秒commandの一度だけの実行、長時間処理後の実際の停止検知、Shutdown。
  追加4ケースはhostが返すengine名、入口でのmodal拒否と解除後のwatchdog復旧、modal拒否で全予約が
  消えた場合の限界、command実行後のmodal例外による二重実行の防止。
- 既存ES3 scheduler / ScriptUI回帰17ケース成功。
- `cargo test -p bridge-contract-tests` の13ケース成功。新規VMテストもこのCI対象に組み込んだ。
- 変更前 `6950fe3` に新規のheartbeat専用task停止テストを当てると、回復回数0のままで失敗する。
  修正後は回復回数1となり成功する。
- 導入済みruntimeと一致するmain `0c4561e` でも、初回両one-shot消失テストは回復回数0のまま失敗する。
  watchdogを含む修正worktreeでは同じテストが成功する。実AEが予約を失う条件の再現とは区別する。
- `cargo fmt --all -- --check` と `git diff --check` は成功した。

実行コマンド:

```powershell
node --test scripts/tests/ae-startup-runtime.cjs
cscript.exe //nologo scripts/tests/ae-bridge-scheduler.js
cargo test -p bridge-contract-tests
cargo fmt --all -- --check
git diff --check
```

ビルドはこのworktree内の `target/`、再現用の旧ソースとテスト一時ファイルは
`temp/startup-20261022/` を使用した。外部ドライブへの一時出力はない。

## 実機受入手順

1. 制作担当とAEを停止できる時刻を調整し、現在の制作データを保存・保全する。
   現在稼働中のAEへ上書き・再読込をしない。
2. 導入済み3 JSXの退避と差分確認を行う。Startup / runtime / Shutdownの修正版を
   同一の配布物として扱い、独自変更・版番号・installerによるスキップ有無を確認する。
3. 空の検証用プロジェクトでAE2025をcold startし、MCPパネルを開かずに確認する。
   3秒間隔で30秒以上、heartbeat更新時刻、runtimeId、3 tick sequence、schedulerReadyを追う。
   その後、対象instanceを明示した読み取り要求を1回送り、requestId対応の結果を確認する。
4. 通常起動、ホーム画面からのプロジェクトopen、起動時に確認画面が出た場合を区別して記録する。
   起動直後の `schedulerReady=false` は正常だが、全tickが長時間0のままなら全予約停止の可能性が残る。
5. 隔離した実機試験でone-shotだけを取り消し、watchdogの進行とパネル不要の復旧を確認する。
   長時間の読み取り要求が1回だけ実行されること、旧世代callbackの無効化、正常終了時のheartbeat削除も確認する。

全tickが0のままなら、JSX監視を増やすだけでは解決できない。外部からの明示的な再評価または
[native実行移行案](native-execution-migration.md)の独立した入口を別途設計・実機検証する必要がある。
今回、既存プロジェクトへ自動CLI再送やnative fallbackを追加していない。

## 21時台の追加証拠とmodalの切り分け

### 確認できたこと

- ユーザー報告では、File → Scripts → Run Script Fileによる手動復旧時に、line 0で
  `Cannot run a script while a modal dialog is waiting for response` が表示された。
  21:20時点の診断ファイル未生成、AE本体は応答中、20:20:35のheartbeatが1/0のまま、という情報も
  PMから引き継いだ。この拒否を本修正チャットから再現したわけではない。
- PMによるパネル表示・Refresh / Restart操作でも更新がなかったと報告された。
  UIクリックの到達・callback実行を示す記録はないため、ボタン内部のどの段階で止まったかは不明。
- 読み取ったincidentは21:21:41 JST、`diagnostics-panel-refresh:heartbeat-stale`、age=3665492ms、
  heartbeatSequence=1、commandPollSequence=0、callbacksRegistered=trueだった。
  これはpanel refresh経由の復旧処理がその時刻に実行された証拠であり、特定のクリックとの因果は未確定。
- `bridge_manual_recovery.txt` は21:22:49 JSTのentry、runtimePresent=true、getState=function、
  終了時healthyを記録している。前後のrevisionは2224150、dirty=falseで同じ。
  bootstrapも同時刻の `already-running` であり、この手動スクリプトが21:21:41の復旧を
  起こしたものではない。今回の手動処理は既に復旧したruntimeを確認したと読める。
- 読取対象の `recover_bridge.jsx` には `#targetengine "ae_mcp_bridge"` がある一方、
  実行記録の `$.engineName` は `main` だった。`main` から既存runtimeへアクセスできたことは確認できる。
  各scheduled callbackのengine名は旧版に記録されていないので、そこまで実測したとは扱わない。

Adobe著作の[ExtendScript preprocessor仕様](https://extendscript.docsforadobe.dev/extendscript-tools-features/preprocessor-directives/#targetengine-enginename)
（Adobe文書の保守版）は、`#targetengine` の対応をIllustrator / InDesignとし、それ以外では無視されると
説明している。CS5時点の記述だが、今回のAE 25.6.6x4の観測はこれと整合する。別engineへ移動する指示や
modalを回避する方法として使わない。修正ソース3 JSXからこのdirectiveを除去し、共有globalとIIFEの
所有権管理でruntimeを保持する前提を明記した。engine名を固定値で判定する処理は追加していない。

### 確定できないこと

line 0でのmodal拒否とentry診断の不在は、script本体へ入る前にAE側が拒否したことと整合する。
この段階ではscript内のtry/catch、`beginSuppressDialogs`、自己診断、watchdogも実行できない。
AEプロセスのResponding=trueは、スクリプトの受理可能状態を意味しない。

ただし、どのmodalだったか、起動時から残っていたか、Run Script Fileの選択ダイアログ・警告・
他プラグイン・AE内部状態のどれが関係したかは確定できない。Recent Script Filesで後に成功した
一例だけでは、このメニューが常に回避策になるとは言えない。Adobeの[設定説明](https://helpx.adobe.com/after-effects/desktop/get-started/preferences-and-settings/preferences.html)
にはスクリプト実行時の警告とmodal progressの設定があるが、当該設定を原因とは断定していない。
設定変更やダイアログ抑制は実施していない。

### 追加した修正と検証

- VMの既定engine名を実測に合わせて `main` に変更。任意のhost報告値でも動作・診断できることを検証した。
- 実際の各callbackで取得する `commandTickEngineName` / `heartbeatTickEngineName` /
  `watchdogTickEngineName` を追加した。未実行ならnullのままとし、初期化時のengine名から推定しない。
- 模擬hostにscript入口でのmodal拒否を追加した。拒否中に診断もtickも進まないこと、解除後にwatchdogが
  残っていれば復旧すること、全予約が捨てられた場合は復旧しないことを別々に検証した。
  これはAE内部が実際にどちらの動作をしたかの証明ではない。
- 既存 `executeCommand` は、modal文字列を含む例外でcommandを無条件にpendingへ戻していた。
  command本体の実行後や結果公開時の例外でも再送され得る。部分完了を否定できないため、この分岐を削除し、
  requestIdに相関するerror resultとcommandのerror状態にする。`hostModalBlocked` は既存のエラー文字列判定を
  記録する補助項目であり、処理が一切実行されなかったことやrollbackを保証しない。
- 結果公開時に一度だけmodal例外を注入するテストでは、修正前 `6950fe3` は同じcommandを2回実行した。
  修正後は1回に留まり、要求状態と結果がerrorになって通常のpollingへ戻る。これが今回追加で再現・修正した不具合。
  ユーザーの実機で二重実行が起きたと主張するものではない。

### Advanced 3Dでの停止は別件

PMから、2D入れ子9個の変更後、10個目のAdvanced 3D comp id6555で `frameRate=30` の代入直前が
最後の進捗ログになったと報告された（request `req-18e0d99525c72094-6256-41`）。これはscript本体が開始され、
host操作へ到達した後の停止報告であり、上記line 0での実行拒否とは分けて扱う。
本チャットではAEP・当該要求・frameRate設定・rendering設定を操作せず、bridgeを原因とは判断していない。
応答不明の既存要求をこの修正によって再送することもない。

## 10月23日 AE2025実機検証（配置前まで）

ユーザーは「修正版導入・再起動・空プロジェクトでのテスト」を明示的に許可し、
AE2025を指定した。開始時点でAfterFXプロセスは存在しなかった。AE2025のみを起動し、
MCP要求の結果と画面で空の未保存プロジェクト（item数0、dirty=false、revision=1）を確認した。

証跡はこのworktree内の `temp/ae2025-live-20261023/` にある。

| 確認 | 結果 |
|---|---|
| 導入済み3 JSXの退避 | `installed-before/` へ保存。内容ハッシュを記録 |
| 旧版0.5.3の通常起動 | 06:12 JST起動、初回heartbeat以降もcommand/heartbeatが進行。今回の起動では自然停止を再現しなかった |
| MCP経由の読み取り | nonce、25.6.6x4、engine=main、空・未変更プロジェクトを回収 |
| 両one-shotの意図的な取消 | 06:15:50にcommand=168/heartbeat=166を取消。17秒後も同じruntime、recoveryCount=0、heartbeat-stale。独立した観測callbackは動作 |
| 通常終了 | 画面とスクリプトの空状態を確認後、Alt+F4で終了。heartbeat削除とPID終了を確認 |
| 修正版の自動テスト | VM12件、ES3 scheduler17件、Rust contract13件が成功。fmt/diff check成功 |
| 修正版の配置 | Program Filesへの書き込み拒否。UAC起動は「操作が取り消された」と終了し、配置は未完了 |
| 修正版のcold start・復旧試験 | 配置待ち。合格と扱わない |

旧版の自力復旧欠落は実機でも再現した。ただし、予約を明示的に取り消す試験であり、
利用者環境でAEが予約を失う自然発生条件を突き止めた証拠ではない。

`AfterFX.exe -r baseline-drop.jsx` の試行はentryファイルを生成せず、launcher終了だけでは
実行を確認できなかった。追加再送はせず、実際の取消試験はMCPから予約した独立callbackで行った。
native入口が今回の環境で動作したとは記録しない。

### 更新が見送られる導入条件

`scripts/install-bridge-installer.ps1` のruntime配置は、導入済みと配布物の版番号が同じで、
ハッシュが異なる場合に既存ファイルを保護する。このため修正runtimeも0.5.3のままなら
更新が見送られる条件に一致する。過去にその分岐が実際に実行されたというinstaller logは未確認。
今回のruntime版番号を0.5.4へ変更した。独自変更を保護するinstallerの方針は変えていない。

### 配置再開用の具体的な準備

`temp/ae2025-live-20261023/install-ae2025-only.ps1` は以下を実施する。

1. AEが停止していることを確認する。
2. 修正版・導入済み・退避済みの3ファイルを固定SHA-256で検証し、想定外の変更があれば停止する。
3. AE2025のruntime / Startup / Shutdownだけを配置し、配置後のハッシュを検証する。
4. 成否を同じtaskフォルダのJSONに記録する。

AE2026、Rust binary、環境設定、他のStartup、AEPには触れない。権限不足を回避する別経路の
永続化やUAC自動操作は行っていない。Windows側の承認後にのみ配置を再開できる。

配置後の実機試験用に `patched-recovery-probe.jsx` も用意した。両one-shot、heartbeatのみ、
commandのみを順に取り消し、watchdogによる復旧と旧世代callbackの無効化を観測する。
各段階で空・未変更・25.6.6x4・bridge0.5.4・同一instanceを検証する。まだ実行していない。

## 2026-10-23 06:59 JST 引継ぎ（制作再開を優先）

以下は06:59時点の状態。上記の「配置待ち」「未実行」は初回UAC失敗時点の記録。以後の隔離検証は次節に追記する。

- UAC再試行は06:23に成功。AE2025の3 JSXのみ0.5.4を配置済み。installed-after.jsonに固定ハッシュと成否あり。
- 空／検証用AEPでcold startを2回、one-shot片側／両側取消、14秒の同期処理、alert表示後の復旧、終了処理を確認。
- File > Scripts > Run Script Fileのnativeファイル選択で全予約処理停止とline 0 modalエラーを実際に再現。同じscheduleTaskを使うwatchdogでは回復できない。
- エラーを閉じた後、AfterFX.com -rの固定・対象照合付きwrapperで全予約処理を復旧できた。native-recovery-proof-1.jsonと対応heartbeatに記録。AfterFX.exe -rの結果と混同しない。
- Windows AE2025のみを対象とするbroker外部復旧処理をdaemon-core/src/ae_recovery.rsとae_recovery.jsxに作成中。ビルド成功。ただし製品には未導入、background監視も未起動。現行adobe-mcp-appは元のPID6256のまま。
- 最後の追加VMテスト14件中1件はテストfixture側の不備で失敗（停止caseで消されたheartbeatをnativeRecoveryヘルパーが読む）。Rust contractもこの失敗を検知。停止前にcommandFileを保存する等の修正と再検証が必要。以前の12件合格と区別する。
- 外部復旧の仕上げには、実機のmodal検出、再起動、broker drain、未完了要求保護、heartbeatの実tick確認、bounded evidence cleanupの検討を継続する。visibleなmodalが見えない時でもAEがline 0で拒否する例があるため、Win32のwindow enabled検査だけで受付可能と断定しない。
- 利用者が制作再開を希望してAE2025を再起動。PID180820、instance ae-25.6.6x4-1792705963270-572937の初回heartbeat後、command/heartbeat/watchdogはいずれも0で停止。制作AEPが開かれていることを確認した。
- 利用者の再発報告を受け、保存・編集・再起動せず接続のみ復旧。初回native要求はline 0で入口拒否、結果ファイルなし。エラーを閉じた後の期限付き・同一instance/runtime照合wrapperはrearmed成功。user-resume-recovery-after-dismissal.jsonに記録。
- 現在runtimeはruntime-1792706311018-414154。schedulerReady=true、3種類のtick進行、fresh heartbeatを確認。現在の起動中セッションの暫定復旧であり、再起動耐性や恒久修正の完了を意味しない。
- targetVersion="2025"を付けた最終MCP確認はtarget不一致で配送前failed。実際のappVersionは25.6.6x4。instanceIdだけに絞って接続確認をやり直した。
- 以降、制作中のAEへの検証・導入・再起動は止める。必要ならユーザーの次回依頼から継続する。新しいRust binaryの常駐起動、既存MCP appの終了、他ホストの停止はいずれも行っていない。

## 2026-10-23 07:30 JST バックグラウンド修正・隔離検証

ユーザーから「Computer Useを使わず裏で修正を継続」と指示を受けたため、以下は制作中のAEへ送信せず実施した。
以降の現在のAE接続状態を再確認したものではない。最後の実機MCP成功は06:59:21 JST、
request `req-18e0f8ef9ccd7de0-6256-f9`。command / heartbeat / watchdogのtickは25 / 16 / 16で、schedulerReady=trueだった。

### 修正候補

- 外部復旧をWindows AE2025・bridge 0.5.4の明示opt-inに限定した。既存設定では無効。
- native部分と復旧判断を分離し、Rust unit testではWin32呼出・実際のlauncher起動をコンパイル対象から外した。
- 受付済み要求、timeout後の処理、予約、未知のmailbox状態を保護し、復旧permit中だけ新規要求を拒否する。状態確認・終了待ちは応答できる。
- 対象processの開始時刻、instance/runtime、期限、mailbox、window状態を確認。複数AEや曖昧な対象は見送る。
- 復旧wrapperの期限をファイル読み込み後にも確認する。遅延した処理で期限を延長しない。
- 成功は同じ世代の3種類のtickの増加で確認する。ack受領だけでは復旧扱いしない。
- 無応答の再送制限をdiskへ記録。時間経過やbroker再起動で再試行回数をリセットしない。手動復旧もtick増加後にだけ認定する。
- 復旧中／要確認の状態をbroker snapshotとMCPアプリに追加した。実機のUI表示は未確認。
- 証跡の容量制限・所有ファイルだけの保存期限処理を追加した。候補の常駐起動、Program Files差し替え、利用者設定変更は行っていない。

### 検証結果

証跡: このworktree内の `temp/background-verification-20261023/`。

| 検証 | 結果 | 根拠 |
|---|---|---|
| 全workspaceのRustテスト | 134件成功、失敗0 | `cargo-test-workspace.log`。新しい外部復旧の模擬host試験12件を含む |
| Startup / runtime / Shutdown / native wrapperのNode VM試験 | 15件成功 | `ae-startup-vm.log`。前節のfixture不備は修正済み。期限切れになる遅いmailbox読取も検証 |
| Windows Script HostのES3試験 | 18件成功 | `es3-scheduler.log`。native wrapperは構文解析だけで実AEへ送信しない |
| 変更対象のClippy | 成功 | daemon-core / adobe-mcp-appのall-targets、mcp-coreのlibを `-D warnings` で確認 |
| 全workspaceの厳格Clippy | 不合格 | `cargo-clippy.log`。未変更の各host stdio等に既存警告あり。今回の範囲外の一括修正はしていない |
| Release build | 成功 | `cargo-build-release.log`。adobe-mcp-app / ae-mcpを生成、起動はしていない |
| format / diff check | 成功 | `cargo fmt --all -- --check` / `git diff --check` |

候補一式は `temp/ae2025-recovery-candidate-20261023/` に保存する。binaryと3 JSX、ハッシュ一覧、
実機受入手順を含む。Rust package版は0.5.3のままなので、候補の識別にはmanifestのhashを使用する。
3 JSXのhashは06:23導入時の記録と一致し、このバックグラウンド作業中には変更していない。

### 残る確認

全callbackが自然停止するAE内部条件の確定、native monitorの実機動作、modal拒否時の動作、
AE終了直前の競合、focus変化、再起動耐性は未確認。隔離テスト合格を恒久解消とは扱わない。
[外部復旧候補と受入手順](ae2025-native-recovery.md)に実施条件と手順を記した。
追加の他スレッド報告は、この記録の受入形式で随時追記する。

## 2026-10-23 10:00 JST以降 実機受入と製品配置

利用者から再び検証可能との連絡を受け、AE2025の再起動・導入・Computer Useを含む実機検証を再開した。
証跡はこのworktreeの `temp/ae2025-acceptance-20261023/`。以下の時刻はJST。

### 確認した事象

- 到着時の旧AE PID180820は08:20:23以来heartbeatが停止。3種類のtickは322 / 217 / 217、idleだった。`arrival-stale-heartbeat.json` に保存した。前回の暫定復旧だけでは持続しなかった証拠である。
- 制作AEPをCtrl+Sで保存し、Saveが無効になることを確認してClose Project。空・未変更の状態から正常終了した。以降の故障注入は空プロジェクトだけに行った。
- 隔離brokerを47665で起動し、AE2025をcold start。PID130228、instance `ae-25.6.6x4-1792717476181-727172`。3種類のtickが自然に進み、実MCP要求 `req-18e10314b0396540-6256-118` が成功した。
- 10:05:39に独立callbackからcommand / heartbeat / watchdogの全taskを取消。10:05:59に外部復旧が開始、10:06:06に3種類の進行を検証してrecoveredとなった。手動での復旧操作やerror dialogはなかった。`full-stop-*` に記録した。
- File > Scripts > Run Script Fileのnativeファイル選択を約46秒保持。表示中は復旧要求が送られず、journalと証跡数が変わらないことを確認した。`modal-held-proof.json` を参照。
- ファイル選択を閉じた直後のnative要求はline 0のmodal拒否となった。表示されたerrorを手動で閉じると、許容された1回の再試行で自動復旧した。`modal-after-state.json` は最初の失敗、`modal-retry-recovered-state.json` はその後の成功であり、混同しない。
- 「閉じた後に時間を置けば拒否を防げる」という仮説を分離して検証した。broker停止中に同じdialogを開閉し、閉じてから約25秒後にbrokerを起動してもline 0で拒否された。単なる待ち時間の追加を修正として採用していない。`modal-delayed-failed-state.json` と `native-evidence/` を参照。errorを閉じた後の1回再試行は成功した。
- 26秒の同期commandに対し、依頼側の待ち時間を5秒に設定した。`req-18e10379ff5536e4-203524-1` はclient timeout後も1回だけ完了し、同じruntimeが維持された。未完了処理中の外部復旧・user code再実行はなかった。`long-completed-record.json` を参照。

### 導入

- 他のAdobe hostも停止し、受付中の処理がないことを確認。過去の未回収予約は削除していない。
- 10:14:52、利用者のUAC承認後に `C:\Program Files\AfterEffectsMcp\adobe-mcp-app.exe` を差し替えた。新SHA-256は `764B8115F2B97DBECDDB99418FDA9E51EA6009C31419B5AE4B735C4DC18D4C5A`。旧binaryは証跡フォルダ内 `adobe-mcp-app-before.exe` に退避した。
- 3 JSXは06:23導入済みの0.5.4と同じhashであり、今回さらに差し替えていない。Rust package版は0.5.3のままなのでbinaryをhashで識別する。
- `%USERPROFILE%\Documents\adobe-mcp\aftereffects-native-recovery.toml` を追加し、このPCのAE hostだけ明示opt-inにした。application.tomlの既存コメントを保持してhost参照を追加し、両設定の変更前後を保存した。他hostは既定の設定のまま。
- 修正版MCPアプリ PID189480を通常権限で10:18:17に起動。5つの既存portを同じPIDでlistenすることを確認。起動直後の約14秒間、AE未起動時の自動起動は発生しなかった（終了直前の競合を保証する試験ではない）。
- AE2025を10:18:33に起動。PID204648、instance `ae-25.6.6x4-1792718327873-140319`。cold start後の実MCP `req-18e103db78f9cfd0-189480-1` が成功し、空・dirty=false・3種類のtick進行を確認した。`cold-start-2-mcp.json` を参照。
- 10:19:52、空プロジェクト限定の独立callbackでruntimeを再予約し、その初回3 taskをすべて取消した。`zero-tick-stopped.json` の0 / 0 / 0、schedulerReady=falseを確認。導入済みアプリ自身が10:20:16に外部復旧し、新runtime `runtime-1792718416160-71166` の3種類のtick進行で成功を確認した。手動復旧やerror表示はなかった。`zero-tick-recovered-*` と `native-evidence/recovery-189480-*` に記録した。
- 10:20:46、実MCPから保存済み制作AEPを開き直し、約21秒で完了。`req-18e103edaa795578-189480-3` はcompleted、445 items、dirty=false、comp5044（制作コンポ、名称非公開）の32 layersを確認した。作品への編集、再保存は行っていない。`production-reopened-mcp.json` を参照。
- 10:21:47、再読込後の実MCP `req-18e103fbbd16c98c-189480-4` がcompleted。dirty=false、comp5044の32 layers / 16 markers、tick35 / 24 / 24を確認した。同じruntimeが進行を続けている。`production-final-read-mcp.json` を参照。

### 残る制約

全taskを自然停止させるAE内部の根本条件は未確定である。今回修正したのは、同じAE内のwatchdogまで止まると回復できない欠落と、その検出・外部復旧経路である。
Windowsの可視window検査を通ってもAE内部のmodal状態で拒否されることがある。このerrorが出た場合は手動で閉じる必要があり、その後の1回の自動再試行まで実機確認した。
すべてのmodal操作、複数AE、他version、終了直前の競合、長時間連続制作について実機保証したものではない。render等の保護には隔離テストの根拠も含まれる。

## 2026-10-24 00:49 JST 再発報告と追加調査

提供元は `制作担当A`。ユーザーの「治らなければAE Startup bridge修正担当へ連絡」という発言をread_threadで確認した。
証跡は `temp/ae2025-recurrence-20261024/` に保存した。AE PID204648、MCPアプリPID189480は前日のまま。
到着時の制作AEPには未保存変更があったため、保存・close/open・SaveAs・AE再起動は保留した。その後の利用者の許可を受けた再起動は後節に記録する。D正本/F保護コピーの調整は他担当の作業であり、ここでは変更しない。

### 観測と原因

- 00:35:03以来、instance `ae-25.6.6x4-1792718327873-140319` のheartbeatが停止。runtimeは `runtime-1792769348343-785614`、idle、currentRequestId=null、render=false、ticks170 / 113 / 113。
- 最終mailboxはcompletedでcurrent_request.jsonなし。報告元の要求 `req-18e1332d13653e80-189480-73` は配送前にactive instanceなしで失敗しており、制作編集は未実行。
- このAE process開始後の時刻を持つ0.5.4の候補heartbeatが10件存在した。そのうち9件は初回tick0 / 0 / 0のまま、23:11〜00:06の起動記録だけが残る。`same-process-candidate-heartbeats.json` に記録した。
- 外部monitorは `candidates.len() != 1` の場合に送信を見送る。この10件の残存記録が、今回の外部復旧を止める十分な条件になっていた。OS window検査に到達する前に戻る。
- `nativeRecoveryStatus=recovered` は22:15のjournal結果をそのまま返していた。現時点のheartbeatがstaleでも表示が更新されない別の問題がある。
- 0 tick記録が生まれる個別の実行元・AEのengine分離の因果はまだ確定していない。記録の時刻だけで特定の他担当を原因としない。

### 追加修正と隔離検証

- 候補が複数でも、保存済みjournalのprocess ID・process開始時刻が現在と一致し、記録されたinstanceが候補に一意に存在する場合だけ、そのinstanceに絞る。
- newest順に推測したり、残存heartbeatを削除したりしない。wrapper内で生きているruntimeのinstance/runtime IDを再照合するため、不一致なら再予約しない。journalの再試行回数は引き継ぐ。
- recovered表示を返す前に同じinstanceのfresh heartbeatを確認し、止まっていればstaleを返す。
- 修正前の回帰テストは15件中2件が想定どおり失敗した。残存記録がある場合に復旧しない点と、古い成功表示が残る点の両方を再現。
- 修正後のdaemon-coreテスト31件成功。旧processのjournalを使わないことと、複数記録でも失敗後の再試行制限をリセットしないことも確認。変更対象のClippy（all-targets、-D warnings）とrelease buildが成功した。
- 00:56、47665の検証broker PID238880で既存AEへの対象照合付き復旧を開始。制作AEPの操作・故障注入は行っていない。ここまでではProgram FilesのMCPアプリは前日版のまま。
- native要求は今回line 0拒否ではなく、wrapper本体へ到達した。ただし `callbackActive=true`、currentRequestId=null、commandTaskId=0、commandTick171 / heartbeat113 / watchdog113、heartbeatAge約1,271秒のためrefused。`native-refused-busy.json` に記録した。対象のinstance/runtimeは一致していた。
- Restart BridgeもcallbackActiveで何もせず戻る実装なので、利用者の再起動ボタンが効かない症状と整合する。callbackの中断後にflagだけ残った可能性はあるが、長時間・再入中の処理との区別は確定していない。現在の作品上で保護を解除したり、runtimeを強制置換したりしない。
- 検証broker PID238880はrefusedの確定後に停止した。native wrapperの再送やjournalの失敗履歴削除はしていない。
- ユーザーへ、必要な保存を済ませてAE2025の再起動・追加修正版導入ができるか確認を出した。回答前にAEや制作AEPを閉じない。
- 配置候補・旧binary・hash一覧・hash照合付き配置scriptは `temp/ae2025-recurrence-20261024/`。`candidate-manifest.json` を参照。配置scriptはAEとMCPアプリが停止している場合だけ動くが、まだ実行していない。

### 保存確認後の配置（01:03 JST以降）

- ユーザーから「保存済み・AE2025の再起動と導入が可能」と回答を受領。ウィンドウの未保存印が消え、F上の現在の作業AEPが01:02:32に更新されていることを確認して、Alt+F4で通常終了した。こちらからSave/SaveAsは行っていない。
- 終了前から作品にexpression error 1件の表示があった。今回のMCP修正で追加したものではなく、制作内容の修正は行わない。
- 他Adobe hostが不在、残る予約は過去の終了済みsessionのものだけと確認して、旧MCPアプリを終了。旧予約を削除していない。
- ユーザーのUAC承認を受け、01:03:47に追加修正版MCPアプリを配置。SHA-256は `B5EEDDFAFF749BB65AFCBB989694D7C22CB9CE6414E5972EEB43D8B8996DD134`。`app-install-result.json` に成否・旧版hash・退避先を記録した。
- 01:04:28に新MCPアプリPID208036とAE2025 PID140720を起動した。3 JSXと既存host設定は変更していない。

### 再起動と制作AEPでの確認（01:06〜01:11 JST）

- AE起動中に「前回予期せず終了し、S_Distortが原因の可能性がある」というAE自身の確認画面を観測した。エフェクトの無効化は行っていない。「有効のまま」をクリックする試みは対象window無効で失敗し、その後の再取得では空プロジェクトに進んでいた。こちらのクリック成功や、S_Distortが接続停止の原因であることを示す証拠にはしない。
- 01:06:40のStartupから新instance `ae-25.6.6x4-1792771600857-27557`、runtime `runtime-1792771600857-600128` が起動した。
- 01:10:23の実MCP `req-18e134798b7770bc-208036-1` がcompleted。空・dirty=false・3種類のtick111 / 74 / 74・schedulerReady=trueを確認。`restart-empty-mcp.json` に保存した。
- 01:10:33に、再起動前に利用者が01:02:32に保存した同一ファイル `<制作データの保存先>/working-project.aep` を空・未変更のprojectであることを再照合して開いた。`req-18e1347ba17d878c-208036-2` は約22秒でcompleted。508 items、active comp10601（制作コンポ、名称非公開）、dirty=falseを確認。`production-reopened-mcp.json` を参照。
- 01:11:23、再読み取り `req-18e1348769ff5054-208036-3` がcompleted。同じfile・508 items・comp10601・dirty=false、3種類のtick130 / 87 / 87を確認。同じruntimeが継続し、scheduler errorはnull。`production-final-read-mcp.json` と `production-final-heartbeat.json` に保存した。read実行中のcallbackActive=trueは当該requestIdに対応する通常状態で、前回のrequestId=nullの滞留とは区別する。
- ファイルの更新時刻は利用者の保存時刻01:02:32のまま、サイズ87,812,360 bytes。こちらで制作編集・Save/SaveAsはしていない。Fを正本と定義したものではなく、停止直前の作業状態へ戻した。Dへの復帰はPMと保存担当の方針に従う。
- nativeRecoveryStatusのrefusedは再起動前のjournal履歴として残る。新instanceのactive count=1、実MCP completedとtick進行を現在の接続成功の根拠とする。journalの失敗履歴は削除していない。
- 追加修正で対象選択が可能になったことは再起動前のwrapper到達で確認したが、callbackActiveの滞留を自動で解除できたわけではない。今回の最終復旧には利用者が許可したAE再起動が必要だった。長時間制作での再発防止は未確定。

## 2026-10-24 02:00 JST レンダリング中の接続0件報告

報告元は `制作担当B`。read_threadでユーザーの「いまレンダリング中ですね 修正担当に連絡くださいー」を確認した。既存のユーザー指示に基づき報告元へ結果を返信する。
証跡は `temp/ae2025-render-20261024/`。この調査ではfilesystemとprocess情報の読み取りのみで、AEへのJSX送信・UI操作・Restart Bridge・AE再起動・保存・編集はしていない。既存常駐アプリと設定も変更していない。

### 確認した時系列

- AE PID140720とMCPアプリPID208036は01:04:28起動のまま存在する。
- 01:35:33のnative試行は無応答。その後01:37:20の試行は対象照合後rearmedとなり、01:37:26に3種類のtick進行が確認されている。これらは既存常駐monitorの記録であり、今回こちらから送信したものではない。
- 01:52:58の `ae_mcp_scheduler_diagnostic.json` は `scheduler-watchdog:command-poll-stale`、callbackActive=false、currentRequestId=null、schedulerRecoveryCount=5、runtime `runtime-1792774338509-329986` を記録。診断後のheartbeatは次世代 `runtime-1792774378668-37689`、recoveryCount=6となった。
- 01:54:35受付の `req-18e136e321eb67b8-208036-11` はregistry上01:54:38にcompleted。したがって01:52:58から現在まで一度も応答がない状態ではない。
- 最終heartbeatは01:56:02、同じinstance `ae-25.6.6x4-1792771600857-27557`、ticks85 / 57 / 57、idle、currentRequestId=null、renderQueueActive=false。ただし現在レンダリング中との利用者申告を優先し、この古いfalseを現在の非レンダーの証拠にしない。
- 01:56:25に常駐monitorがnative要求 `recovery-208036-1792774585557.jsx` を生成、01:56:37にunacknowledged。02:03読み取りでもattempts=1、sawBlockedWindow=false、結果jsonなしで追加送信はない。既存の再試行制限を解除していない。wrapperには10秒の入場期限があり、遅延して実行されても期限を過ぎればruntimeを変更しない設計だが、この要求自体の実行結果は未確認。
- 01:58:16の `req-18e137167da853f8-208036-12` はregistry上failed。報告された理由はactive instanceなし。元担当の6/8・4/4マーカー新規編集は未適用で、制作変更は元担当が扱う。

### 隔離再現と未導入の追加修正

- 外部native wrapperはlive stateのrenderQueueActiveを確認していたが、内部の `ensureBridgeRuntimeHealthy()` と `recoverBridgeRuntime()` には同じ保護がなかった。watchdogがレンダー中に実行機会を得ると、古いcommand pollの時刻を理由にruntimeを再初期化できた。
- Node VMで「最後のdisk heartbeatはrender=false、その後live render=true、command/heartbeatの実行が止まりwatchdogだけ進む」を再現。追加2テストが修正前に失敗し、意図しないruntime IDの変更を確認した。これはコード上の欠落の再現であり、実AEで今回その順序が発生した証明ではない。
- `ensureBridgeRuntimeHealthy()` はlive renderQueueActiveなら現状を返す。`recoverBridgeRuntime()` 自身にも同じ確認を置き、startからの復旧呼び出しも保護する。healthyや進行counterを偽って更新せず、render終了後に残っているwatchdogが実際の欠落を復旧できることもテストした。
- Node VM 17件成功、Windows Script Host ES3 18件成功、git diff --check成功。Rust変更はなく、既存アプリの再ビルド・配置はしていない。
- 導入中runtime SHA-256: `2C3151CEB12DB1555191832774ACFF28467C9B0A65444AD14EA513314742B8B1`。候補: `31857A768FAD1869708359E472FC3F826E36C3FDFA3856AE334F0BFDB9FD27BC`。証跡フォルダの `mcp-bridge-auto-candidate.jsx` / `candidate-manifest.json` に保存した。

### 残る範囲と次の確認

- 全scheduleTaskがレンダー中に実行されなければ、内部guardもheartbeatも実行されない。古いidle heartbeatを根拠とした外部launcherの最初の試行を、この候補だけで抑制できるわけではない。native wrapperに到達した場合のlive render確認と期限保護は既存のまま。
- 現在のレンダー開始時刻・終了時刻、停止がレンダー後に自然回復するか、01:52の診断と現在の停止との因果は未確定。旧診断のfalseやWindows Respondingだけで安全に復旧可能と判定しない。
- レンダー終了の報告後、まずheartbeatとinstanceを読み取りで再確認する。元スレッドの未保存編集を保護し、配置や再起動が必要なら、その時点の利用者の許可と保存状態を確認する。今回の古い再起動許可をレンダー中の介入に流用しない。

### 報告元による復旧確認と長時間scriptの区別（02:48 JST以降）

以下は `制作担当B` から受領した結果。read_threadでユーザーの「どうでしょうかー」への対応、接続・レンダー終了確認、制作変更と検証の完了記録を確認した。requestやruntimeの詳細は提供元の報告に基づき、こちらから現在のAEへ再照会していない。

- 02:48、list_ae_instancesが同じinstance `ae-25.6.6x4-1792771600857-27557` をidleで返し、その後の実MCP読み取りがcompleted。live `app.project.renderQueue.rendering=false` を確認したとのこと。
- runtimeは `runtime-1792777250491-454528`（02:40:50開始）、schedulerRecoveryCount=11、lastSchedulerRecoveryReason=`scheduler-watchdog:command-poll-stale`。元担当はRestart・AE再起動・候補導入をしていない。01:56の停止から回復するまでの直接契機や正確なレンダー終了時刻は未確定であり、「レンダーだけが原因」「候補の効果」とは断定しない。
- 02:49:53に元担当が保留していた6/8・4/4用nullと6プロパティの参照変更を適用し、検証を完了した。既存コントロール・キックマーカー・対象外の式を保持。Fのworking AEPを継続使用し、保存はしていないとのこと。制作内容は元担当の管轄とする。
- 検証要求 `req-18e139eeba532ce4-208036-1b` は1280フレーム×6プロパティおよびOpacity全2700フレームを評価し、約1分を要した。10秒のtool timeoutと一時的stale応答が発生したが、元担当は再送せず同requestを照会し、最終completed・errors=[]を取得。その後の短いMCP読み取りも成功した。このtimeoutは実行内容と最終結果が確認された長時間scriptとして分類し、レンダー時の未説明停止や要求消失の再発件数に加えない。
- この連絡は既存報告の復旧結果であり、新規調査・AE操作の依頼ではない。修正担当は記録更新と受領返信のみを行い、候補の配置、保存、再起動、追加検証は実施しない。
