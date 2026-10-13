# MCP公開Toolの整理案

更新日: 2026-10-13。基準コミット: `91d2b1c`（ベータ版0.5.3）。本書は次の実装のための仕様案であり、この文書の追加によって公開Toolや既存クライアントの動作は変わらない。

## 目的と前提

LLMが選ぶToolを少数の共通契約に整理し、Adobeの個別操作はスクリプトとrecipeで組み立てられるようにする。host別のruntime差、結果回収、接続診断を明確にする。

ベータ版は現状の公開面のまま配布した。今回まずAE接続障害の修正を優先し、ここに記載する全hostの公開面変更は後続タスクで行う。[native移行案](native-execution-migration.md)と共通の契約にするが、transport変更とTool削減は別々にレビューできる単位へ分割する。

## 現状と既存文書の読み方

- 共通の`run-script`、`run-script-file`、`get-script-result`、`get-capabilities`、`cancel-script-request`はすでに実装されている。
- `run-jsx`等の旧名も公開され、AEでは`PUBLIC_TOOL_NAMES`が16件を返す。主な問題は共通機能の不足だけでなく、重複した入口とhostごとの説明差である。
- `run-script`は現在、`code`を持つraw実行と、`script`を持つ旧allowlist要求を区別して受理する。
- `runtime`とcapabilityは現行bridgeに結びついている。native／UXP併存時の選択契約は別途必要になる。
- AEの`run-bridge-test`は単純なpingではなく、既存コンポジションへエフェクトを適用する実装を含む。読み取り専用の疎通確認として案内しない。

現行契約の確認には[script contract](script-contract.md)、`crates/mcp-core/src/lib.rs`と`script_contract.rs`、各hostのTool定義を使う。

`mcp-jsx-tool-restructure.md`は2026-05時点の提案であり、`after-effects-mcp-surface.md`の旧一覧も共通Tool追加前の記述を含む。「run-scriptを再公開しない」といった記述を現行動作の保証として扱わない。実装時はこれらの説明、README、Prompt、Resource、helpを同時に更新する。

## 通常公開する共通Tool案

| Tool | 役割 | 設計上の要点 |
|---|---|---|
| `run-script` | inline codeを実行 | runtime、入力、実行目的、対象と待機上限を明示 |
| `run-script-file` | ローカルのscript fileを実行 | allowed root／trusted path・hash、サイズ検証を維持 |
| `get-script-result` | requestIdの状態・結果を回収 | timeout後も同じ要求を追跡 |
| `list-<host>-instances` | 対象アプリを列挙 | hostごとの既存名を維持。起動と利用可否を区別 |
| `get-capabilities` | runtime、経路、制約を取得 | 対象version／instanceで実際に使える機能を返す |
| `test-connection` | 副作用のないnonce付き実行で疎通確認 | 作品を変更せず、busy時はキュー／待機状態を示す |
| `cancel-script-request` | 要求のキャンセルを要求 | queuedと配送済みで保証を分ける |
| `get-help` | 契約・回復手順・recipeを取得 | 既定の回答は短く、必要な詳細を指定して取得 |

`list-<host>-instances`の既存例は`list-ae-instances`、`list-photoshop-instances`。一覧名の統一だけを目的に破壊的な改名はしない。

AEの`save-frame-png`は専用Toolとして残す候補とする。画像artifactを扱い、利用頻度も高いためである。`cleanup-preview-folder`は所有する出力の管理用として維持し、通常公開か診断・管理profileかを実装時に決定する。削除対象を広げない。

## script入力とruntime

共通契約で実装済みのfieldと上限を維持する。例示のfield名やschemaの細部は、実装時のcontract versionと照合する。

```json
{
  "code": "({ version: String(app.version), value: input.value })",
  "input": { "value": 42 },
  "runtime": "extendscript",
  "mode": "unsafe",
  "description": "アプリのversionと入力値を取得",
  "declaredEffects": ["read"],
  "riskPolicy": "analyze",
  "targetInstanceId": "selected-instance",
  "timeoutMs": 10000,
  "resultRetentionSeconds": 3600
}
```

上記は将来の論理runtime指定を含む仕様例であり、現行の全hostへそのまま送れる保証ではない。現在のruntime識別子とのalias／移行規則を定める。

- 論理runtime（ExtendScript／UXP）とtransport（native／WebSocket／file bridge）を分ける。
- `auto`を許す場合も、既存コードの言語環境を勝手に変更しない。特にPhotoshopのUXPコードをnative JSXへ流さない。
- `input`を標準とし、既存`args`は互換で受ける。両方指定時の優先順位またはエラーをschemaに定義する。
- inlineの`mode: unsafe`、fileの`trusted`条件、source hash、risk reportは維持する。任意コードのsandboxを意味する名前へ変えない。
- inline 256 KiB、file既定1 MiB、入力・結果各1 MiB、待機上限600秒、保持既定1時間・設定上限24時間を既存契約として扱う。変更時は別途互換性を確認する。
- `preflightOnly`は静的な検査であり、Adobeでのdry-runではない。

## 結果と接続状態

要求の状態とアプリの状態を分ける。`queued`／`running`／`completed`／`failed`／`unknown`等の要求状態を保持し、Adobeプロセスの存在だけから要求成功を推測しない。

応答には`requestId`、対象host／instance／runtime、利用経路、現在の状態、結果またはエラー、次に確認すべき操作を含める。既存`status`／`state`等を突然削除せず追加fieldで移行する。runtimeやtransportの追加field名は実装時にcontractへ固定する。

- timeout時: 実行が継続し得ることと`get-script-result`で追跡することを返す。
- heartbeat停止時: 「bridgeを起動してください」だけにせず、既知の実行中要求や応答未確認を示す。
- 実行可否不明時: 重複実行を避け、同じ要求を別経路へ自動再投入しない。
- queued cancel: 配送前なら実行を防ぐ。配送済みcancel: 強制停止を保証しない。

`test-connection`には二つの確認結果を分けて載せる。brokerへ到達したことと、対象Adobe内でnonceを返せたことは別である。単にTCPが開いているだけでAdobe接続成功としない。アプリ未起動時に黙って起動したり、コンポジション・レイヤー・エフェクトを作ったりしない。

## 互換移行

| 現在の入口 | 移行後の扱い |
|---|---|
| `run-jsx` / `run-jsx-file` | `run-script`系の互換alias。通常の`tools/list`では重複表示を減らす |
| `get-jsx-result` / `get-results` | 既存payloadを受理し、`get-script-result`へ案内。最新結果取得の意味は個別に維持 |
| `run-script`の`script` payload | 既存allowlist要求として維持。rawの`code`と混同しない |
| `mcp_aftereffects_*`等 | 非公開互換dispatchを当面維持 |
| `run-bridge-test` | 既存動作を無断で別物にせず、診断用途へ移し`test-connection`を案内 |
| `configure-bridge-diagnostics` | 設定変更を伴う管理機能として扱う。通常の疎通確認から呼ばない |
| `get-bridge-diagnostics` | 診断profile／アプリ内診断から利用できるよう維持 |

通常profileは共通Tool中心、legacy／diagnosticsの表示はopt-inとする案を推奨する。設定のキー名と切替方式は未確定である。Toolを一覧から隠すことと、既存の呼び出しを拒否することを分ける。aliasの削除は告知したbreaking releaseまで行わない。

`run-script`で`code`と`script`を同時指定する曖昧な要求は検証エラーにする方針とし、既存テストで互換性を確認する。allowlist専用の新しい公開Toolを増やすことは初期段階の必須条件にしない。

Prompt／Resourceも同じbroker・routing・結果契約に合わせる。内部で旧direct-file経路へ抜ける入口を残す場合は、その制約を明記する。Toolを減らしても、一部のResource経由で排他や監査が抜けないようにする。

## 説明とrecipe

各Toolの説明は「何を渡すか」「何が返るか」「待ちきれない場合どうするか」を中心に短くする。全host共通の長い注意書きを毎回重複させない。

`get-help`またはResourceに、文書列挙、選択内容取得、複数操作のまとめ実行、export、preview、timeout後の回収をruntime別recipeとして置く。UXPの非同期・modal要件とExtendScriptの同期処理を混ぜたサンプルにしない。

専用Toolの追加は、artifact管理、ライフサイクル、brokerでの排他／復旧など、単純なDOM wrapper以上の価値がある場合に限定する。[capability matrix](capability-matrix.md)の判断基準を引き継ぐ。

## 実装順序と受入条件

1. **公開面の棚卸し**: 5 hostの`tools/list`、schema、旧payload、Prompt／Resourceを記録し、実装と文書の差を解消する。
2. **副作用のない疎通確認**: `test-connection`を追加し、既存の変更を伴うbridge testと分離する。
3. **通常profileの整理**: canonicalを既定表示にし、旧名のdispatchとopt-in表示を維持する。
4. **runtime／状態の整理**: native移行案と整合するcapability、接続状態、結果envelopeを実装する。
5. **説明と移行ガイド**: help、recipe、README、既存設計書、クライアント設定例を揃える。

受入試験:

- 全hostの公開名・schemaのsnapshotと、旧名・旧payloadの互換試験。
- 同じ対象へcanonicalとaliasで要求しても、キュー・監査・結果回収を共有すること。
- `test-connection`前後で文書数・コンポジション数・dirty状態が変わらないこと。
- アプリ未起動、複数version、長時間処理、modal、timeout、終了、再接続時の説明が実際の状態と一致すること。
- `get-script-result`で遅れて返った結果を回収でき、再実行しないこと。
- bridgeとnativeの両方で契約が成立し、非対応runtimeは明確に拒否すること。
- Codex CLI等のMCP clientで最小の実行・結果回収を確認すること。モデル応答だけで実行成功とせずnonceとrequestIdを照合すること。

今回確定していない事項は、profile設定名、alias廃止時期、cleanupの既定表示、runtime aliasの厳密な規則、macOSのnative対応時期である。これらは実装PRで提案と検証結果を揃えて決める。
