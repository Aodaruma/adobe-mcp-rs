# After Effects 接続停止の調査（2026-10-13）

## 対象と結論

render PC `mimoid_render` / `accou` の作業ツリー
`worktrees/fix-ae-docked-bridge-recovery`、基点
`91d2b1ce73fe433af03df2f7c6b20d433ce4a658`（beta 0.5.3）で調査した。
修正ソースは同名のローカル worktree へ回収した。制作に使用中の primary PC の AE・MCP は
操作していない。remote の GitHub account は使用せず、bridge のインストールや Adobe 設定の
書き換えも行っていない。これは配布済み 0.5.3 の更新インストールではない。

実装で確認できた不具合は、実行中の one-shot callback の task ID が 0 になる正常な期間を
停止と判定すること、command 側だけの停止を heartbeat の新しさで隠すこと、パネル操作が
必要な回復経路、古い runtime を保持する診断パネル、fallback パネル終了による runtime 停止である。
これらを AE bridge 内で修正した。長時間処理を再実行する変更や、heartbeat の有効期限を
長くして接続扱いする変更は入れていない。

**修正した部分停止からの復旧は、render の AE2025 実機で成功した。** command と heartbeat の
予約を意図的に取り消しても、パネル操作なしで監視タスクが復旧させ、その後の 14 秒の JSX は
一度だけ実行され、同じ runtime のまま完了した。

ただし、ユーザー環境で起きた永続切断の原因を一つに確定したわけではない。実際のドッキング操作は
未検証であり、AE が監視タスクも含むすべての scheduled callback を実行しない場合は本修正だけでは
回復できない。その状態も起動時ダイアログを挟んだ実機試行で観測した。以下で両者を区別する。

## 提供された primary PC の証拠

以下は前段の読み取り専用調査の観測であり、今回 primary PC で再測定した結果ではない。

- AE 2026 `26.3x87`、アプリ・bridge 0.5.3。5 broker ポートの所有者は同じ
  `adobe-mcp-app`。重複 daemon が原因という証拠はない。
- 2026-10-13 13:26:13〜13:33:12 JST の `executeJsx` は proxy 連番の再接続と
  `app.project.save` を実行し、正常終了した。実行中の heartbeat は `running` / 同じ
  requestId で凍結し、10 秒の broker 閾値から除外された。終了後は同じ runtime 世代で heartbeat が
  再開し、回復回数は 0。これは busy と heartbeat stale の混同を確認する証拠であり、scheduler の
  永続停止の証拠ではない。
- 08:17:59 JST の診断は `task-id-missing`、`running=true`、commandTaskId=0、
  heartbeatTaskId=3、age=14516ms、pollSequence=1、heartbeatSequence=4、
  callbacksRegistered=true、lastSchedulerError=null、renderQueueActive=false。
  08:17:42 にも heartbeat stale による回復があった。callback 実行中の ID=0 と整合するが、
  この snapshot だけでは本当に task が消失したかを判定できない。
- 12:38 の Windows Application Hang による別 AE プロセス終了は、原因不明。
- インストール済み runtime は repository と完全同一ではない。`executeJsx` の undo group を
  明示的に閉じず AE の暗黙終了を使う独自変更がある。上記の長時間要求は `undoGroup=false` であり、
  この差分で当該事象を説明できない。本パッチは undo 処理を変更していない。primary PC への適用時は
  独自変更を保持して差分を取り込む必要がある。

## 確認できたコード上の問題

1. command / heartbeat callback は開始時に自身の task ID を 0 にし、`finally` で再予約する。
   旧 health は両 ID の非ゼロを必須とするため、callback 内で診断 UI や lifecycle 操作が再入すると
   正常な実行中を task 消失と判定する。回復で世代を更新すると、まだ戻っていない旧 callback の
   `finally` は旧世代の再予約を行わなくなる。実際の callback 消失とは区別できず、不要な世代変更と
   loop 再予約を起こす。これだけで永続停止になることは実機確認できていない。
2. health は command poll の進行を見ていない。command の one-shot が失われても heartbeat が
   生きていれば正常と判定される。逆に両 one-shot が失われると、それらだけで自己回復できない。
3. docked パネルは timer watchdog ではない。refresh / open / activate で health を確認するだけなので、
   ドックされたまま放置しても回復は起動しない。
4. 診断パネルは作成時の runtime を closure に保持している。runtime の差し替え後も古い closure の
   restart を呼び、global callback を上書きし得る。
5. Startup runtime の診断パネルは閉じても runtime を止めないが、ScriptUI fallback パネルは
   `onClose` で heartbeat と command loop を停止していた。

scheduler 消失の発生条件（AE の modal、dock 操作、render、エンジン寿命、AE 自体の不具合）は
まだ仮説である。今回、これらの条件で実機停止を再現したという主張はしない。

## 修正の動作

- callback の実行中フラグ、`isChecking`、`currentRequestId` を busy 判定に使用する。
  自身の ID=0 は callback 実行中なら正常。refresh・start・restart・stop・watchdog・runtime 再評価は
  実行中の世代を差し替えない。通常の要求は元の callback で一度だけ実行され、終了時に再予約する。
- 同じ callback の再入を拒否する。scheduledRuntimeId と global runtime の所有者を確認し、旧世代・
  差し替え済み runtime の callback を無効化する。callback 登録は型だけでなく関数 identity を確認する。
- one-shot command / heartbeat に加えて、3 秒周期の独立した repeating watchdog を設置する。
  アイドル時の task ID 消失、callback 上書き、heartbeat 停止、command poll 停止を検知し、
  task を取り消して新世代を予約する。回復そのものは command を直接実行しない。
- command の最終 poll と callback 完了時刻の両方で進行を判定する。長時間スクリプトが戻った直後を
  command stall と誤判定しない。既存の stale 閾値（bridge 側 12 秒、broker 側既定 10 秒）は変更しない。
- 診断パネルの操作ごとに現在の global runtime を取得する。実行中は `BUSY` と表示する。
  headless と fallback の双方で、閉じる・再度開く操作は process 所有 runtime を停止・交換しない。
  手動の Auto-run 停止は、パネルを閉じても維持する。一時的な権限ロックでこの選択を上書きしない。
  開き直した診断パネルから現在の runtime の Auto-run を再開できる。
  明示的な lifecycle stop はアイドル時に利用できる。

Rust の dispatch / request reservation の本体は変更していない。stale heartbeat の instance は引き続き
active instance から除外される。`running` という古い記録だけで接続を保証できないためである。
dispatch 済み要求は `unknown` として予約を保持し、同じ requestId の遅延 result を回収する。
stale を理由に予約解除、再送、native 実行への切り替えは行わない。

## 検証

### ソースを実行する回帰ハーネス

`scripts/tests/ae-bridge-scheduler.js` は repository の JSX から scheduler と panel attachment の
実コードを読み込む。AE の scheduleTask / ScriptUI を小さな ES3 mock に置き換える。
Windows の既存 `cscript.exe` で動作し、Node でも実行できる。新しいソフトウェアは導入していない。
Windows CI に同じテストを追加した。

```powershell
cscript.exe //nologo scripts/tests/ae-bridge-scheduler.js
cargo test -p bridge-core -p daemon-core -p ae-mcp
cargo test -p bridge-contract-tests
cargo fmt --all -- --check
git diff --check
```

- scheduler / ScriptUI ハーネス: 17 ケース成功。
  実行中の stale heartbeat と ID=0、refresh / restart / stop / watchdog / callback 再入、
  heartbeat callback と再予約中の再入、手動要求実行中の予約、遅い callback の正常復帰、両 one-shot 消失後の
  ボタン不要の回復、heartbeat が新しい状態で command だけ停止、通常の例外、再予約例外、
  起動時の one-shot 予約失敗、callback identity 不一致、旧 runtime callback、実行中の headless 再読込、stop 後の遅延 callback、
  docked mock パネルの runtime 差し替え、パネル終了、終了後の手動 pause 維持と再表示時の再開を含む。
- Rust: `ae-mcp` 11、`bridge-core` 22、`daemon-core` 16、計 49 テスト成功。
  新規の `stale_running_heartbeat_is_inactive_but_never_releases_uncertain_request` は、
  heartbeat ファイルが存在するが期限切れの `running` instance を discovery が active 扱いせず、
  request が unknown / busy のままで、遅延 result によってのみ予約を解放することを検証する。
  既存の heartbeat ファイル消失・timeout・broker 再接続・exclusive / FIFO テストも成功。
- bridge contract: 12 テスト成功（内部の UXP lifecycle テストも成功）。旧 scheduler 名と
  repeating heartbeat 全面禁止を前提とした検査は、command / heartbeat は one-shot、watchdog のみ
  repeating という新しい構成に合わせて更新した。
- render の PATH に Node がなかったため、既存ローカル Node v24.15.0 の実行ファイルを task 内
  `temp/render-investigation-20261013/node-runtime/` にコピーし、検証用プロセスだけの PATH に追加した。
  ソフトウェアのインストールやユーザー共通の PATH 変更は行っていない。
- rustfmt と差分空白検査は成功。成果物とテスト一時ディレクトリは作業ツリー内の `target/` と
  `temp/render-investigation-20261013/` に置いた。

これらは模擬 scheduler / transport の検証であり、実際の AE が同じタイミングで callback を再入させる、
または watchdog を維持することの実機証明ではない。ハーネスの経過時間は状態の時刻を進めて再現している。

### render PC の Session 0 試行

AE2025 の `AfterFX.exe` / `AfterFX.com` の実行ファイル version は `25.6.6`。
SSH の PowerShell は Session 0、explorer の対話セッションとは異なる。起動前に AE / アプリは稼働していなかった。
この作業で作成した AE プロセスだけを対象とした。

通常起動を行い、同じローカル PC で次を試行した。

```text
AfterFX.com -r "<worktree>/temp/render-investigation-20261013/bootstrap-fixture.jsx"
```

fixture は空の新規起動プロジェクトで version / item count / nonce を記録し、作業ツリー内にコピーした
runtime を headless 評価する。コピーでは bridge root のみ task 内 `bridge/` に差し替える。
グローバル Adobe prefs、Scripts/Startup、ScriptUI Panels、既存 bridge を変更していない。
`run-bridge-test`、エフェクト適用、プロジェクト保存、ユーザー作品への操作は行っていない。

Session 0 で AE と com launcher のプロセスは残ったが、entry nonce、runtime state、heartbeat の出力は
得られなかった。com の stdout / stderr も空だった。`Responding=true` は GUI / transport 応答の証明にはならない。
fixture 自体が実行されたことを確認できず、理由を license dialog、初回起動、session 制約のどれかに断定できない。
最終的に作業所有 PID だけを停止した。この試行だけでは nonce round-trip や scheduler の合否を判断できない。

証跡は task 内の `ae-session0-observation.json`、`ae-process-id.txt`、`ae-com-process-id.txt`、
`afterfx-com.stdout.txt`、`afterfx-com.stderr.txt`、fixture JSX、テスト log に残した。

### 対話 Session 1 での追加検証

task 所有の一時 Scheduled Task から、14:01:45 JST に AE2025 を対話 Session 1 へ起動した。
検証対象はこの作業が起動した PID のみ。空・未保存・item 数 0 のプロジェクトで実行した。
入力待ち画面はその PID のウィンドウを確認して閉じ、Crash Repair は通常起動の Continue、
Adobe アンケートは Cancel を選択した。Safe Mode、設定リセット、アンケート送信は行っていない。

起動完了前の native 呼び出しでは Scripting plug-in に関する警告があり、modal が残る段階の
追加呼び出しは「modal dialog is waiting for response」と拒否された。後者は native 経路でも
modal を迂回できない実例であり、launcher が終了したことだけを実行成功としてはいけない。

14:25:37 の native 読み取りでは、runtime の global 関数と task ID は残っていたが、command は
一度も poll せず、heartbeat と watchdog も約 822 秒進んでいなかった。一方、その時点で新しく
予約した独立の scheduleTask は 1 秒後に実行された。起動時の dialog／script error を挟んだ
旧予約全体の停止を観測したが、どの出来事が直接の原因かは未確定である。

起動完了後に runtime を読み直して、14:26:10〜14:26:52 JST に次を検証した。

| 操作 | 実機結果 |
|---|---|
| `AfterFX.exe -r` で外部から JSX を実行 | AE `25.6.6x4`、nonce、item 数 0 を回収。同じ起動済み PID |
| command / heartbeat の予約だけを `cancelTask` | watchdog が約 12 秒後に `command-poll-stale` を検知し、自動復旧。パネル操作なし |
| 復旧後、command file に nonce 付き 14 秒 JSX を投入 | 一度だけ実行され、requestId に対応する成功結果を回収 |
| JSX 内で heartbeat が 14 秒古い状態の `ensureHealthy` | `callback-in-flight` として世代を維持。誤再起動・追加回復なし |
| 処理完了後の状態 | healthy、requestId 解放、poll / heartbeat 再開。item 数は 0 のまま |

停止前の runtime は `runtime-1791869170938-76782`、watchdog 復旧後は
`runtime-1791869185940-877655`。回復回数は 0→1 になり、14 秒の処理後も 1 のままだった。
`real-verification.json` の `passed`、`recoveredWithoutPanel`、`exactlyOnce` はすべて true。
`physicalDockVerified` は false である。

続いて実際の ScriptUI palette を起動・終了し、閉じたあとに新しい nonce 付き要求を投入した。
14:33:11 JST の `panel-verification.json` は成功を記録し、同じ runtime の受信・heartbeat 継続と
item 数 0 を確認した。最初の fixture は必須 description を省略したため入力検証で拒否されたが、
fixture を訂正して成功した。再表示後の Auto-run 再開 UI はその後に追加し、ハーネスで検証した。

証跡と fixture は remote task 内の `temp/render-interactive-20261013/`、主要な JSON と fixture の
ローカル回収先は `temp/render-interactive-evidence-20261013/`。installed Startup / shutdown hook、
実ドッキング操作、AE2026、制作データの編集・保存、MCP client から broker を含めた全経路の試験とは区別する。

検証終了後は空・未保存プロジェクトと callback 非実行中を再確認し、作業所有 AE を通常終了した。
14:36:33 JST に PID の終了と一時 Scheduled Task の削除を確認した。起動で生成された
`HAP-log.txt` / `NotchLC-log.txt` も task 内へ移し、source に含めていない。

## 限界と native fallback の判断

今回の root-cause bridge 修正は、callback の誤判定と部分的な scheduler 消失を安全に扱える。
watchdog を含む全 scheduled callback の消失、AE の hang、blocking render / modal 中には自力回復できない。
全 callback を失った場合は、AE がスクリプトを受け付けるようになってからパネル refresh または
bootstrap 再評価で回復できる。OS process の死を stale と区別する独立 process probe は今回追加していない。
broker/UI の disconnected 表示を busy 専用表示へ全面変更する範囲も含めていない。

AE-only native direct execution は**本パッチでは製品実装しない**。まず bridge の具体的な不具合を
修正し、部分停止の復旧と長時間処理の安全性を実機で確認できたためである。ただし全 callback 停止を
bridge 内だけで回復する保証はできない。これが安定稼働時にも残る場合は、監視機構をさらに
重ねるより、[native 移行案](native-execution-migration.md)の AE adapter を次の優先作業とする。

native 入口自体は対話 Session 1 の `AfterFX.exe -r` でも成立した。2026-10-09 の primary PC PoC は
同じ AE2025 `25.6.6x4` の既起動 process に `AfterFX.com -r` が成功したもの。
ただし bridge 未導入状態、複数 version の routing は未確認であり、modal 中の native 呼び出しは
今回も拒否された。native 化を AE 自体の busy / modal / hang の解決策として扱わない。

native を追加するなら、AE-only・明示的な選択・対象 process/version の検証・task 別 JSON 応答・
requestId による result 保持を先に実装・検証する必要がある。dispatch 済みで unknown の要求を native で
再実行してはいけない。`AfterFX.exe -r` と `.com -r` を等価として扱う、他 host に一般化する、
未文書化 injection を使うことは避ける。

次の受入確認は、修正版 bridge と primary PC の undo 独自変更を両立させ、実ドック状態と
installed Startup の起動で同じ fixture を行う。通常作業中の再発、正常終了・強制終了も
requestId / runtimeId / sequence / recovery count と対比する。配布時には版番号と installer の
改変済みファイル保護を確認し、同じ 0.5.3 のまま上書きしたつもりで旧 runtime が残ることを防ぐ。
