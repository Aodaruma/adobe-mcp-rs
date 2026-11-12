# AE2025の外部復旧と受入記録

2026-10-26更新: bridge runtime 0.5.5を正式配置し、AE2025のidle/pending全予約停止からの復旧、プロセス終了時の待機解放、再起動後の接続と実レンダーを確認した。以下の動作表はこの最終版の仕様を示す。実装・検証の結果と制約は[安定化改修記録](ae2025-stabilization-2026-10-26.md)を参照。10月23〜24日の記述は当時の観測履歴として保持する。製品パッケージのバージョン0.5.3とbridge runtimeのバージョン0.5.5は別管理である。

2026-10-23にこのPCのAE2025へ導入し、実機で再起動・全task停止・初回tick=0からの復旧を確認した。
ファイル選択を閉じた後のAE内部modal拒否は残っており、その場合は表示されたエラーを手動で閉じる必要がある。
原因・実機結果の継続記録は[接続停止の調査記録](after-effects-startup-investigation-2026-10.md)を参照。

2026-10-24追記: 同一AE起動中の残存heartbeatが複数ある場合の候補選択と、古いrecovered表示を追加修正し、利用者の許可で01:03:47に導入した。実機の対象照合は成功したがcallbackActive保護で拒否されたため、保存完了後にAE2025を再起動した。01:11:23に同一の保存済み作業AEPでMCP応答・dirty=false・3種類のtick進行を確認。callbackActiveが残る元のAE内部条件は未確定であり、このケースの自動復旧まで確認したものではない。

## 対象と動作

2026-10-24 02:03の未導入修正: レンダー中の接続0件報告から、内部watchdog/health確認にlive renderQueueActiveの保護が不足していたことを隔離テストで確認し、worktreeのruntime JSXへ追加した。Node VM 17件・Windows ES3 18件成功。現在の制作レンダーには介入せず、配置・実機検証は保留している。外部launcherが古いidle heartbeatに基づき最初の試行をする点はこの候補では変えていない。詳細とhashは調査記録の02:00以降の節を参照。

02:48以降の報告元の確認では、候補未導入・手動RestartやAE再起動なしで実MCP応答が回復し、live render=falseを確認した。直接の回復契機は未確定。後続の約1分の数値評価によるtool timeoutは同じrequestのcompletedを確認した既知の長時間実行であり、レンダー時の未説明停止と分けて記録した。追加AE操作は行わない。

AE内のcommand / heartbeat / watchdogがすべて止まる場合、同じ `scheduleTask` に監視を増やしても復旧できない。
Adobe MCPアプリ内のAE brokerが監視し、起動済みのAE2025へ `AfterFX.com -r` で固定の復旧wrapperを送る。
user commandの実行経路・FIFO・requestId・結果保持は従来のまま。失敗したuser commandの再送はしない。

有効化はAE host設定のトップレベルに以下を明示した場合だけ。既定はfalse。

```toml
ae_native_recovery_enabled = true
```

この設定を用いた実機受入は、利用者が制作を止められる時間に実施した。
このPCでは `Documents/adobe-mcp/aftereffects-native-recovery.toml` で有効化し、
`application.toml` の `[hosts.aftereffects]` から参照する。製品の既定値はfalseのまま。
AE2026、他のAdobe host、macOSにはこのmonitorを適用しない。

| 条件 | 挙動 |
|---|---|
| heartbeatが20秒以内に更新 | native実行しない |
| AEが未起動／複数起動／別ログオンsession／実行ファイルがmajor 25以外 | native実行しない |
| 古い起動の残存heartbeat | native実行しない |
| 同一起動中の対象候補が複数 | journalのprocess ID・開始時刻が一致し、そのinstanceを一意に照合できる場合だけ選択。照合できなければ見送る。wrapper内のlive runtime照合と再試行制限は維持 |
| bridge 0.5.5以外／render中／currentRequestIdあり | native実行しない |
| brokerに受付済みの要求あり | 原則見送る。対象のpending mailboxとbroker予約のrequestId・commandが一致する場合だけ、予約を保持してschedulerの再予約を依頼できる |
| current_request.jsonが不明／mailboxがrunning・未知・読取不能・ID不一致 | 予約とmailboxを保持して見送る |
| AE windowがdisabled・native dialog表示中・応答不明 | windowを操作せず見送る |
| 上記を通過 | 同じprocessの開始時刻・instance・runtime・期限を照合して再予約を依頼 |
| wrapperからack受領 | これだけでは成功と扱わない |
| AEが動作を再開した直後 | 3秒猶予を置き、保留callbackの再開を待つ。全予約を失っていれば1個の確認taskで復旧判断を再実行 |
| 同じ世代のcommand / heartbeat / watchdogが連続観測で全部増加 | 復旧確認済みと記録 |
| 無応答 | 時間経過だけでは再送しない。blocked windowを観測後に解消した場合だけ、期限経過後に1回再試行できる |
| 2回失敗／ack後にtickが進まない／wrapperが拒否 | 実際の接続回復または新しいAEセッションまで自動試行を停止 |

復旧中は受付用mutexを長時間占有せず、専用permitで新規要求を拒否する。
MCPアプリの状態確認や終了要求は応答でき、drainは復旧処理の終了を待つ。
状態は復旧中に `Reconnecting: verifying AE scheduler`、試行停止後に接続がなければ
`Needs attention: AE recovery paused` と表示する。broker snapshotの `native_recovery_status` と
instance一覧の `nativeRecoveryStatus` に結果を返す。保存済みのrecoveredに対し、現在の同じinstanceがstaleなら `stale`、freshでもscheduler進行が未確認なら `awaiting-progress` を返す。既定で無効の場合は復旧処理を開始しない。
別brokerが同じbridge rootを使った場合も、OSのファイル共有lockで復旧送信を重複させない。

## 証跡

AE hostのbridge root内に `native-recovery/` を作る。

- `state.json`: 対象のprocess開始時刻・instance・試行回数・期限・結果。broker再起動後も再送制限を保持。
- `recovery-<pid>-<timestamp>.jsx`: 固定wrapperと対象照合値。user codeを含まない。
- `recovery-<pid>-<timestamp>.json`: nonce、再予約前後のbridge状態、拒否または結果。
- `recovery.lock`: 同一bridge root内のnative復旧を排他するOS共有lock。

実行証跡の読み取りは128 KiBまでに制限し、不正・途中・別nonceの結果を成功扱いしない。
正常復旧時、名前が所有形式に厳密一致する証跡のうち、最新64ファイルから外れており24時間以上経過したものだけを整理する。
他の名前・directory・symlinkは対象外。stateとlockは保持する。

## 保証していないこと

- Windowsのwindow検査でAE内部のmodal状態を完全には判別できない。実機では可視dialogがない状態からline 0で拒否された例がある。エラーdialogの自動操作は実装しない。
- process確認とlauncher起動はOS上で原子的ではない。直前にAEが終了する競合までCLIで防げるとは判断していない。AE未起動時に自動起動しないことは短時間観測したが、この競合の保証にはならない。
- native wrapperはAEを再起動せず作品にもアクセスしないが、全環境でエラー表示やfocus変化が起きないという保証はない。
- 一度の再予約成功は、再起動耐性・長時間安定性・他版対応の保証ではない。
- 元のAE自身が同期処理で占有されている場合、強制中断や別経路へのuser command再送はしない。

## 実機受入手順と未網羅範囲

制作データを保存してAE2025とMCPアプリを止められる時間に実施する。他hostの受付済み処理もdrainで保全する。
Computer Useや再起動を許可されたセッションでのみ操作し、以下を空の検証AEPで確認する。

1. 候補binaryと導入済み3 JSXのhashを照合し、既存binary・設定を退避する。候補版と旧版を同じportで同時起動しない。
2. 隔離した検証profileでmonitorを有効化する。制作中のプロジェクトへ故障注入しない。
3. 起動順を変えたcold start、初回tick=0の停止、全task停止からの外部復旧を確認する。
4. nativeファイル選択を開いたまま保持し、native要求を送らないことを確認する。閉じた後の再開とtick増加を確認する。
5. line 0拒否後に同じ要求を連続送信せず、手動でdialogを閉じた場合の最大1回の再試行を確認する。
6. 長時間command、client timeout、未回収結果、render中、broker終了待ちで誤って復旧しないことを確認する。
7. 複数AE・別version・AE終了直前・起動し直しで誤対象へ処理しないことを確認する。
8. 作品に編集を加えず、正常終了、heartbeat削除、再起動後の再接続を再確認する。

同等の修正を別環境へ導入する場合も上記手順で確認する。未完了requestの予約を削除して復旧したことにはしない。
このPCでの明示opt-inを他環境の無人有効化の根拠にはしない。

2026-10-23の実測では、全task停止、初回tick=0、通常cold start 2回、native dialog表示中の見送り、
modal拒否後の1回再試行、26秒処理の5秒client timeout後の完了を確認した。
dialogを閉じて25秒待っても最初のnative要求がline 0で拒否される再現があり、任意の待ち時間追加は採用していない。
複数AE・別version・render中・終了直前の競合等については隔離テストと実機試験を区別し、実機全項目合格とは扱わない。

10月23日の導入binaryのSHA-256は `764B8115F2B97DBECDDB99418FDA9E51EA6009C31419B5AE4B735C4DC18D4C5A`。
当日の証跡、設定の変更前後、旧binaryの退避は `temp/ae2025-acceptance-20261023/` に保存した。
10月24日の追加修正版は `B5EEDDFAFF749BB65AFCBB989694D7C22CB9CE6414E5972EEB43D8B8996DD134`。
追加調査・31件のdaemon-coreテスト・Clippy・build・導入と再起動後の実MCP証跡・前日版binaryは `temp/ae2025-recurrence-20261024/` に保存した。3 JSXとhost設定は10月24日には変更していない。
