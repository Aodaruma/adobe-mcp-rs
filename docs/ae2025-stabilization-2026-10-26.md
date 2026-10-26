# AE2025 MCP 現行構成の安定化

## 目標と到達点

利用者の指定により、「大規模改修前の現行構成でAE2025のMCPを安定して利用できる状態」をgoalに設定した。ExtendScript・ファイル通信・Rust brokerを維持し、命令の実行境界、実行済み記録、復旧判断、受付経路を順に修正する。

2026-10-26 11:05 JST、**改修・正式導入・AE2025実機検証・Codex再起動後の接続更新・検証物の片付けまで完了**。10:48 JSTに再起動待ちとしてblockedにしたgoalの阻害条件は解消した。以下の各段階は実施順の記録であり、最終状態と制約は末尾を参照する。

## 第1段階：命令と実行状態の保護

- 命令を実行する前にrequestIdごとのclaimを保存する。保存失敗時は実行しない。
- 結果の保存・配送と命令本体を分離する。結果配送だけが失敗した場合は、計算済みの結果を再配送し、命令を再実行しない。
- claim後に処理が途切れ、結果が確定できない場合は結果不明として返す。自動的にやり直さない。
- 完了記録をruntimeの再読み込み後も参照し、同じrequestIdの再投入を抑止する。
- Windowsの結果ファイル置換と復元が両方失敗する故障注入で、初期候補には再読込み後の二重実行が残っていた。claimを一度だけ書く不変ファイルにし、結果を別ファイルへ分離してこの経路を塞いだ。結果が失われてもclaimから結果不明と判断し、再実行しない。
- receiptの書込み時に過去の全記録を列挙しない。初期候補は既存10,000件で1要求につき20,001 entryを走査した。修正ではreceiptディレクトリの全件走査を除いた。
- callback入口のメタデータ処理をtry/finally内へ移し、入口例外でもbusy flagを解放して次回を予約する。

### 実機で見つかったJavaScript実装差

Node VMで合格した `new Function("args", "mcp", "code", "return eval(code);")` は、AE25.6.6x4では生成元のscopeを引き継いだ。raw JSXから `typeof bridgeRuntimeRunning` はboolean、`typeof $.global.bridgeRuntimeRunning` はundefinedとなり、内部のローカル変数が見えていた。indirect eval、`$.global.eval` でも分離できなかった。

採用した修正では、AEが評価するscheduleTask文字列のトップレベルで評価関数を生成し、command callbackへ渡す。AE2025実機で内部変数3種類がundefinedとなり、args・mcp.log・app・$.engineNameは従来どおり利用できることを確認した。これはスコープの分離であり、unsafe JSXのhost/global権限を制限するsandboxではない。

実機では同じrequestIdをmailboxへ再投入しても実行回数は1回だった。14秒の命令はクライアント側でtimeoutになった後、再送せず同じrequestIdからcompletedを回収し、runtime IDも維持された。

## 第2段階：復旧条件の統一

- watchdog・start・restart・ensureHealthyの入口で、実行中、レンダー中、AE再開直後の猶予を共通に判定する。
- 外部入口がwatchdogより先に動いた場合も、保留中callbackに実行機会を与える。全予約を失った場合には、1個の確認用taskから改めて復旧を判断する。
- 診断パネルの状態更新は読取りだけにする。パネルを開いたこと自体でruntimeを再初期化しない。
- schedulerの進行とheartbeatファイルの書込み状態を区別する。ファイル保存障害だけではschedulerを再初期化しない。
- callbackActiveを「健全性が証明済み」とは扱わない。処理中と判断し、強制的なflag解除や再実行をしない。

実機ではMCP外の16秒停止の直後にensureHealthy・start・restartを連続して呼び、すべて同じruntime IDを保った。6.5秒後にhealthy=true、復旧回数0を確認した。第1・第2段階の証跡は `temp/ae-stabilization-20261026/` に保存した。

## 第3段階：brokerとの整合

AE MCP stdioの旧23経路とCLIの直接mailbox書込みをdaemon経由へ統一した。requestIdを返し、timeout後はそのIDで結果を取得する。daemon不在時に旧方式へ切り替えない。fake daemonで31互換経路とCLIの結果追跡・mailbox保護を確認し、ae-mcpの16テストが成功した。

さらに、brokerが要求を保存した直後にAEの全taskが停止した場合の行き詰まりを修正した。0.5.5のruntimeに限り、pendingのrequestIdとcommandがbroker予約と一致する場合だけ、予約を保持したままschedulerを再予約できる。running、不明、読取り不能、ID不一致は引き続き拒否する。wrapperは命令を直接実行しない。

AEプロセスの終了が確定しているのにworkerが永久待機する経路には、同一process HANDLEによる終了検出を追加した。単なるheartbeat停止・経過時間・OS照会失敗を終了と推定しない。実行後の結果が不明ならlostとして記録し、再送しない。キャンセル要求後の終了でも結果不明の説明を保持し、後から結果を回収できる。模擬テストでFIFO・全体排他・JobPermitの解放を確認し、bridge-core 26件、daemon-core 36件が成功した。Windows製品コードのcargo checkも成功した。実プロセスでの受入は未実施。

## セキュリティ監査と残る受入

最初の固定差分について正式Security Diff Scanを完了した。対象12製品source、新規の確定脆弱性0件。scan IDは `c755425a-90cc-47e5-9866-e9ef9008c538`。対象digestは `d304158d15316e35d2e2a887d5ce901f3498746f2ad1e75308b1c87d42a4ace9` で、実機で不適合だったnew Function案までのsnapshotである。後続の修正を監査済みと読み替えない。

監査報告はCodex標準の状態領域 `<CODEX_HOME>/state/plugins/codex-security/scans/fix-ae-docked-bridge-recovery/6950fe3aa4fd1aa1a456dd274230fb243ab26d86_20261026T003931Z_5iz7vvv0/report.md` に保存された。receipt結果本文の保持量、既存brokerの信頼境界、OS ACLは別の評価事項として記録した。

この監査時点で残していた統合テスト・差分レビュー・実機受入の結果は後述する。過去の長時間busy事故の正確な停止行と外部要因は依然未確定であり、今回の故障注入が当時発生したと断定しない。

結果本文とclaimの自動削除は今回追加しない。本文もinstance内に残るため、長期の容量管理は制約として残る。将来の掃除ではbrokerの結果受領確認と期限を使って本文だけを除去し、重複抑止の小さいclaimを保持する必要がある。期限だけでclaimを削除して再実行可能にしてはならない。

## 統合候補の自動検証

2026-10-26、導入前候補で以下を確認した。

- `cargo test --workspace --all-targets`: 154件成功。旧contract testの「パネルrefreshがensureHealthyを呼ぶ」という前提は、読取り専用の新仕様へ更新した。
- Node VM: 最終版41件成功。結果receiptのpublish/rollback二重故障、10,000記録での非走査、watchdog入口例外、構造化引数の状態保存、復旧probe早発火を追加した。
- Windows ES3: 18件成功。
- `cargo fmt --all -- --check`: 成功。
- `cargo clippy -p bridge-core -p daemon-core -p adobe-mcp-app --all-targets -- -D warnings`: 成功。workspace全体の厳格Clippyは、HEADにもあるae-mcpの既存helper・test、mcp-coreのtest、protocol contract testの警告で失敗する。今回追加したexit-probe helperの引数数警告には、既存APIとの対応を保つ局所的な例外理由を記載した。

復旧用のcommand読取上限はbrokerのcompact JSON入力上限に合わせ、command mailboxもcompact JSONで保存する。多数の小さい要素を含む入力で、pretty整形の増分が読取上限を超える問題を防いだ。deferred ACK直後に旧世代heartbeatが残る場合は、新世代を観測した時点で比較基準を更新する。いずれも回帰試験を追加した。

## 統合版の実機検証と追加修正

AE25.6.6x4の空プロジェクトとportable候補アプリで検証した。制作AEPは開いていない。

- command/heartbeat/watchdogの全予約を意図的に取り消したidle状態から、native監視が復旧し、3種類のcounterが再び進行した。
- 最初のpending故障注入では、復旧wrapperはdeferredを返したがprobeが再予約されず停止した。実機読取りでprobe ID=0、busy=false、render=false、error=null、命令実行回数0を確認した。
- 隔離fixtureで、probeを猶予期限の1・10・100ms前に配送すると同じ停止を再現できた。猶予期間内にprobeがなくなった場合、残り時間（最低50ms）で1個だけ再予約するよう修正した。実機で早発火した瞬間そのものは計測しておらず、原因候補との整合と修正後の受入を区別する。
- 修正版でpending要求 `req-18e1f066c4dfe0c4-282116-8` はtimeout後に再送せずcompletedを回収できた。native journalはrecovered、命令本体の実行回数は1回だった。
- 10,000要素の構造化引数を実行し、完了mailboxが140,910 bytes・改行なし・completedのcompact JSONであることを確認。その後のidle全予約取消でも自動復旧し、3counterが進行した。
- 停止した空AEを通常終了した際、待機要求 `req-18e1f00a04ce0b28-282116-5` はlost・結果不明・再送なしとなり、対応する予約ファイルが除去された。AE再起動後の新要求が完了し、旧workerの待機が全体排他を占有し続けないことを確認した。
- 検証globalと自作taskを片付け、portable試験用AEを通常終了した。Program Filesへの導入はこの時点では未実施。

実機証跡、導入前バックアップ、候補ハッシュ一覧、インストーラはworktree内の `temp/ae-stabilization-20261026/` に保存。Rust全体154件に加え、最終JSX変更後のcontract 13件、Node 41件、ES3 18件が成功した。最終release buildも成功し、2個のexe hashは直前の検証版と一致した。

正式監査の最終固定差分はscan `b6ed5133-dcda-470f-a870-76b3d9040b86`、digest `4d1f8ffc578680e814cac1590c999c3ed8a16dca0e67c2224f48c9906e2857af`。16 source、新規security findings 0件。報告は `<CODEX_HOME>/state/plugins/codex-security/scans/fix-ae-docked-bridge-recovery/6950fe3aa4fd1aa1a456dd274230fb243ab26d86_20261026T011904Z_mztq07js/report.md`。seal後のcompact statusとprobe再予約の2点は追加の読取りレビューで確認し、新たなsecurity blockerなし。最終runtime SHA256は `42220232754BE20961C0E6D974AD43184576411491E805D32DFE04B5F995C2A9`。

busy/render中にprobeが動いた場合、強制復旧や追加ループは行わない。全予約消失とレンダーが重なるケースでは、レンダー終了後に別の外部入口が必要になり得る。処理中の命令を再実行する復旧は今回の仕様に含めない。

## 正式配置と最終状態

2026-10-26 10:36:53 JSTにUAC付きインストーラが成功した。配置先は `C:/Program Files/AfterEffectsMcp/` の2 exeと、AE2025 Scripts内のruntime/Startup/Shutdownの3 JSX。5ファイルすべてが候補manifestのSHA256と一致する。バックアップ・変更前後のhash・復元手順を持つインストーラをworktree内に保持した。

- アプリ: `adobe-mcp-app.exe` SHA256 `809CA8C5D3A97724157F369E1FAA2AFD0AFB61A716FCF04825A433C881A3158C`。
- frontend: `ae-mcp.exe` SHA256 `9A985C6274866EFB7FDB3085224199059342924112C9FF071FFF414E0A81E5EF`。
- runtime 0.5.5: SHA256 `42220232754BE20961C0E6D974AD43184576411491E805D32DFE04B5F995C2A9`。

インストール先のアプリを通常権限で起動（PID 290644）し、AE2025も新規起動した（PID 292672、instance `ae-25.6.6x4-1792978693362-344879`）。MCPで読み取ったruntimeScriptPathはProgram Filesの正式配置先、versionは0.5.5だった。

新規コンポとテキストレイヤーを作成して30フレームを実レンダー。生成されたMP4をffprobeで読み、H.264・128×128・30フレームを確認した。検証後はキューとコンポを除去し、空プロジェクトへ戻した。正式配置した新CLIの `bridge run-script --script getProjectInfo` もbroker経由でcompletedとなり、requestIdを返した。

最終MCP確認: file=null、items=0、dirty=false、renderItems=0、検証globalはundefined。command/heartbeat/watchdog counterは159/106/106、schedulerHealthyとheartbeatFreshはtrue、runtime IDは起動時と同一だった。AEはこの空プロジェクト、MCPアプリは正式配置先から稼働している。候補portableプロセスは終了済み。

追加の画面経由モーダル試験は、Computer Useのwindow取得が `window id ... was not found` と `foreground window did not report a process id` で失敗したため実施していない。既に行った16秒の外部停止試験・隔離モーダル試験と区別する。

### 接続更新の経過

導入前に起動したCodexの `ae-mcp.exe serve-stdio` が5個残っている（観測PID 274676/277132/63392/286096/3708）。現在公開されるrun_jsx等のbroker経路は正式配置した新アプリに接続し、上記受入が成功している。一方、旧互換ツールのコードはプロセス再起動まで置き換わらない。クライアントを強制切断せず、利用者へCodexの再起動を依頼した。

10:47 JSTのPID 63392への45秒の終了待機はtimeout、10:48 JSTにも旧5プロセスのIDと開始時刻が一致した。確認の繰返しを避けるため、利用者による再起動と再開までblockedとした。確認証跡は `reconnect-wait-1.json` と `reconnect-wait-2.json` に保存した。

利用者のCodex再起動後、11:00 JSTに旧5プロセスの終了、新stdio PID 276628/278996（10:59起動）、配置5ファイルのhash一致を確認した。この時点ではMCPアプリが不在で、接続はdaemon不在エラーになった。検証時の通常Start-Process起動とCodexの実行jobとの関係が疑われるが、旧プロセス終了の直接原因を断定するイベント証跡はない。

11:02 JSTにWindowsのWin32_Process.Createから、既存ログイン起動登録と同じexe/config/data-dirを指定してMCPアプリを起動し直した（PID 263904、親WmiPrvSE.exe）。設定や起動登録は変更していない。別のコマンドからIsProcessInJobで、MCPアプリはjob未所属、Codexのコマンド実行プロセスはjob所属を確認した。MCPアプリがCodexの実行jobに含まれないことと、起動コマンド終了後も稼働することを検証した。

新stdio→新MCPアプリ→AE2025の経路で、instance一覧・capabilities・run_jsxが成功。request `req-18e1f1fe1ef224e4-263904-1` はcompleted、bridgeVersion=0.5.5、schedulerHealthy/heartbeatFresh=true、内部bindingはundefinedだった。AEのruntime IDは10:38起動時から変化していない。

さらに従来の `run-bridge-test` を空プロジェクト内の専用shape layerで実行した。request `req-18e1f2176327b444-263904-3` がbroker経由でcompletedとなり、Gaussian BlurとDrop Shadowの両方がsuccess。2個のeffectを実際に数えた後、専用コンポを除去して空プロジェクトへ戻した。最終結果はfile=null/items=0/dirty=false/renderItems=0/testGlobal=undefinedである。

11:03 JSTに旧実行ファイル `temp/ae-stabilization-20261026/retired/ae-mcp.exe`（4,400,640 bytes）を削除した。旧プロセス不存在と、導入前バックアップ・削除対象のhash一致を確認してから削除している。復元用バックアップと証跡は約16.3 MiBをworktree内に保持。プロジェクト外の新規一時領域は使用していない。検証スクリプトは自動起動されないタスクフォルダだけにあり、AEの検証global/task/コンポは残っていない。

証跡: `detached-app-launch.json`、`broker-job-membership.json`、`reconnected-final-state.json`、`reconnected-legacy-bridge-test.json`、`reconnected-cleanup-state.json`、`retired-cleanup.json`、`reconnection-complete.json`。

### 完了判定

| 要件 | 確認した根拠 |
| --- | --- |
| 現行構成を維持して実行境界と重複実行を保護 | 修正source、実機scope/重複要求、immutable claimと配送失敗の隔離回帰 |
| callback後始末と復旧判断を修正 | 例外・長時間停止・早発火の回帰、実機idle/pending復旧と実行回数1 |
| 状態分類と終了時の待機解除 | 終了HANDLEの回帰、実機lost/再送なし/予約解放、次の起動で要求完了 |
| 導入物と再起動後の経路を整合 | 5ファイルhash、旧stdio終了、新stdioと独立broker、新接続からの実要求完了 |
| 検証とセキュリティ確認 | Rust154、最終contract13、Node41、ES3 18、正式差分監査と最小delta追加レビュー |
| 検証物の片付けと制約の記録 | 空プロジェクト・検証global不存在・旧exe削除、容量/結果不明/busy-render時の制約を本書へ記録 |

残っているreceipt保持容量と、実行中のhost処理を強制復旧しない条件は既知の運用制約であり、今回の受入の未実施項目とは分ける。過去の全障害の単一原因や無期限の無障害動作を証明したものではない。
