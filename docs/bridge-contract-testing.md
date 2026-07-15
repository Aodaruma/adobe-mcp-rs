# Bridge protocol の契約テストと実機 smoke test

## 自動 contract harness

`crates/bridge-contract-tests` は、実 Adobe アプリの代わりに mock host を起動し、実際の TCP daemon と instance 別 file bridge を通す E2E fixture です。
期待する schema と原子更新規約は [Bridge protocol v1](bridge-protocol.md) を source of truth とします。

```powershell
cargo test -p bridge-contract-tests
```

テストは `mcp_core::HOST_SPECS` を走査するため、After Effects / Premiere Pro / Photoshop / Illustrator / InDesign が同じ契約を使います。InDesign もhost固有のfixtureを複製せず同じmatrixに入ります。

共通 fixture が検証する項目は次の通りです。

- host-first / daemon-first の両方で heartbeat を発見できる
- `requestId` 付き command と対応する result を取り違えない
- timeout 後の late result を retained registry から回収できる
- 新しい daemon endpoint へ再接続しても retained result を回収できる
- stale / malformed heartbeat を active instance に含めず、理由を返す
- heartbeat 復帰後に同じ instance を再発見できる
- partial file JSON を途中結果として採用しない
- chunk 分割された TCP JSON と malformed TCP JSON を安全に処理する
- 複数 instance がある場合は target 指定を要求し、指定先へ独立 routing する

mock の応答は `ResponsePlan`（即時成功、遅延成功、partial JSON 後の成功、無応答）で組み立てます。host 別 bridge 実装の protocol test はこの fixture を再利用し、別仕様を作らないでください。

## 実 Adobe host smoke checklist

自動 harness は Adobe DOM や panel lifecycle を代替しません。release 前に、対象 OS と host version ごとに次を実行します。

1. build commit、OS、host version、bridge runtime/version、daemon version を記録する。
2. host を先に起動し、panel/extension の auto-run または InDesign Startup Script を有効にしてから daemon を起動する。
3. `list-*-instances` で `protocolVersion`、`hostId`、`instanceId`、`bridgeRuntime`、`capabilities` を確認する。
4. `run-bridge-test` を実行し、command/result の `requestId` と対象 instance が一致することを確認する。
5. host 固有の read-only 操作を1件実行する。
   - After Effects: composition 一覧
   - Premiere Pro: project / sequence 一覧
   - Photoshop: document 一覧
   - Illustrator: document / artboard 一覧
   - InDesign: document / page / story 一覧
6. daemon を先に起動した状態でも host/panel を後から起動し、instance 発見と `run-bridge-test` を繰り返す。
7. bridge pollingを一時停止して短いtimeoutのrequestを発行し、復帰後に`get-jsx-result` / `get-results`でlate resultを回収する。AEは`aeMcpBridgeStop()` / `aeMcpBridgeStart()`を使う。
8. timeout request を保持したまま daemon を再起動し、同じ `requestId` を回収できることを確認する。
9. panel を閉じる、または host を終了し、stale threshold 後に `inactiveInstances` へ移ることを確認する。再起動後は active に戻ることを確認する。
10. 複数 version/instance を同時起動できる場合、target 無指定が曖昧性 error、target 指定が正しい instance へ届くことを確認する。
11. OS の sleep/resume と host の modal dialog 後に heartbeat と request 処理が復帰することを確認する。
12. host log、daemon log、対象 heartbeat/command/result、retained registry を artifact として保存する。script 本文や個人パス等の機密情報は除去する。

live bridge root の JSON を手で破損させる試験は行いません。malformed / partial JSON は自動 harness または専用の一時 bridge root で検証します。

## 結果の保存形式

1回の host/OS/version の組み合わせを1 JSONにします。保存先の推奨形は次の通りです。

```text
artifacts/adobe-host-smoke/<YYYY-MM-DD>/<hostId>/<os>-<appVersion>.json
```

JSON は `docs/bridge-smoke-result.schema.json` に従います。`checks[].id` には checklist の安定した名前（例: `heartbeat.discovery`、`request.late-result`）を使い、失敗時は `notes` と `evidencePaths` を必須にします。log や screenshot は JSON からの相対パスで保存します。

`status: "skip"` は環境上実行不能な場合だけ使用し、理由を `notes` に記録します。全必須 check が `pass` になるまで、その host/OS/version を実機確認済みにしません。
