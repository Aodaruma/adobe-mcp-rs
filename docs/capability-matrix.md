# Adobe host capability matrix と raw-script-first 方針

- 最終更新: 2026-07-15
- 対象: After Effects / Premiere Pro / Photoshop / Illustrator / InDesign
- 関連 Issue: #20、#21、#22

この文書は、Adobe host ごとの機能差と、今後 MCP に何を公開するかの判断基準を定義します。host の実装済み状態は [Adobe host roadmap](adobe-host-roadmap.md)、bridge の共通契約は [bridge protocol](bridge-protocol.md) を source of truth とします。

## 結論

公開面は **raw-script-first** とします。LLM が host の JavaScript / JSX を直接組み立てられる場合、操作ごとの Tool を増やすより、少数の汎用 script Tool、実行規約、例、実行結果の回収手段を提供する方を優先します。

理由は次のとおりです。

- 複数の host 操作を一つの script にまとめられ、LLM が最短の実行経路を選びやすい。
- Adobe DOM / UXP API の追加を、Rust の Tool schema 追加を待たずに利用できる。
- 操作ごとの Tool 名や引数を大量に維持せず、host version 差を script 内で吸収できる。
- 読み取り、更新、export、結果整形を一回の host 実行にまとめ、往復と token 消費を減らせる。

ただし raw script は Adobe host と同じ権限で動きます。静的検知、確認、hash allowlist を追加しても sandbox にはなりません。任意 code を許可するかどうかは、MCP server の外側を含む運用ポリシーで決めます。

## 用語と公開方針

| 用語 | 意味 |
|---|---|
| raw script | LLM またはユーザーが入力した任意の JavaScript / JSX / `.idjs` source |
| structured input | raw script に JSON 値を `input` として渡す仕組み。操作 allowlist ではない |
| structured operation | `export-document` のように操作名と引数を固定した wrapper |
| trusted file | canonical path と SHA-256 が設定と完全一致する配布済み script |
| guard | 静的検知、risk 分類、確認などの事故防止層。安全性を保証する sandbox ではない |

基本方針:

1. host 操作は、まず raw script と structured input で表現する。
2. 実行結果は script 側で JSON serializable な値へ整形する。
3. よく使う操作は Tool 化より先に Resource、Prompt、recipe、sample script で教える。
4. structured operation は後述の追加基準を満たす場合だけ公開する。
5. bridge / daemon の状態、instance routing、result recovery は host script では代替できないため共通 Tool とする。

## Capability matrix

次の表は Adobe host 自体の能力と、この repository で採用する実行方法を合わせて示します。「実装済み」は repository 上の bridge を指し、実際の Adobe host で動作確認済みという意味ではありません。実機未検証の経路は明記します。

### Script runtime と入力

| Host | 基本 runtime | raw source の host 内評価 | raw file | structured input |
|---|---|---|---|---|
| After Effects | ExtendScript / ScriptUI（ES3 系 JSX） | **実装済み**。bridge の `eval` wrapper で実行 | **実装済み**。`.jsx`、path / size / UTF-8 / hash policy 適用 | **可**。JSON `input` を wrapper 引数として渡す |
| Premiere Pro | UXP 25.6+、CEP / ExtendScript fallback | **実装済み**。UXP plugin は `allowCodeGenerationFromStrings` が必要。CEP は JSX | **実装済み**。`.js` / `.jsx` | **可**。Promise を含む UXP code に JSON `input` を渡す |
| Photoshop | UXP API v2 | **実装済み**。UXP plugin の `new Function` wrapper。manifest permission が必要 | **実装済み**。`.js` / `.jsx` | **可**。`app`、`action.batchPlay`、`core.executeAsModal` helper と JSON `input` を渡す |
| Illustrator | CEP / ExtendScript | **実装済み**。CEP から ExtendScript `eval` | **実装済み**。`.jsx` | **可**。JSON `input` を JSX wrapper 引数として渡す |
| InDesign | UXP Startup Script `.idjs`（18.5+ PoC） | **repository実装済み・実機未検証**。`eval` / `Function` は使わず、raw文字列を `app.doScript(String, UXPSCRIPT, [], ENTIRE_SCRIPT)` へ渡す | **repository実装済み・実機未検証**。Rustでpath / size / UTF-8 / hashを検証後、sourceを同じString実行経路へ渡す | **repository実装済み・実機未検証**。JSONをwrapper sourceへliteralとして注入する |

InDesign だけは「raw-first = inline `eval`」ではありません。公式の UXP Script permission では文字列からの code generation が無効なため、repository PoCはInDesign DOMの `app.doScript` String入力を使います。`run-script-file`もhost側の一時fileを作らず、Rustで検証・監査したsourceを同じwrapperへ渡します。長時間動作するStartup Scriptからこの経路が利用できるかは実機E2E gateです。

### Host 操作

| Host | Read | Write | Export / render | 現在の主な不足 |
|---|---|---|---|---|
| After Effects | project / composition / layer / effect を取得可能 | composition、layer、keyframe、expression、effect、project を更新可能 | frame preview、render queue、MOGRT 等 | 一部 API は active composition や plugin の matchName に依存 |
| Premiere Pro | project / sequence / track / clip / marker を取得可能 | UXP DOM action / transaction、または CEP API で更新可能 | sequence export。media / codec API は version と host 状態に依存 | repository の allowlist は小さく、実機 version matrix が未確立 |
| Photoshop | document / layer / property を DOM / `batchPlay` で取得可能 | DOM / `batchPlay`。変更は `executeAsModal` が必要 | save / export 系 action を実行可能 | repository は読み取り中心。modal、write、export の E2E が不足 |
| Illustrator | document / artboard / layer / page item を取得可能 | DOM / action で vector、text、layer 等を更新可能 | PNG / JPEG / SVG / PDF 等 | 現行 host version、署名、配布の検証が不足 |
| InDesign | document / page / story / text / style / link 等を取得可能 | DOM で組版、style、page item、document を更新可能 | PDF / EPUB / image / package 等を host API から実行可能 | `id-mcp`、Startup bridge、document/page/story読取はrepository実装済み。Startup常駐とraw/write/exportは実機未検証 |

この表の「可能」は host API の能力です。すべてが現在の allowlist operation として実装済みという意味ではありません。raw-script-first では、host API に到達できれば個別 Tool の追加を待たずに利用できることを重視します。

### Undo、modal、filesystem

| Host | Undo / transaction | Modal / user interaction | Filesystem / network |
|---|---|---|---|
| After Effects | `app.beginUndoGroup` / `endUndoGroup` でまとめられるが、file、render、save、quit は完全 rollback できない | dialog suppression API はあるが、任意 plugin dialog や長時間 script の一律制御はできない | ExtendScript の file / network access は host preference の許可が必要 |
| Premiere Pro | UXP の `Project.executeTransaction` と `lockedAccess` を使用。すべての API が同じ transaction model とは限らない | UXP modal dialog は UI を block する。raw code は原則 non-interactive にする | UXP manifest の filesystem / network permission。CEP fallback は別の権限モデル |
| Photoshop | `executeAsModal` の execution context と history suspension を使用。すべての外部副作用は戻せない | document 変更は modal scope が必要。キャンセルは Promise 待機点など interruptible な code に限られる | UXP manifest permission と OS permission の積。`batchPlay` 自体は filesystem sandbox ではない |
| Illustrator | `app.undo()` / host undo stack の best effort。外部 file 操作は戻せない | ExtendScript dialog と host modal state は自動復旧を妨げうる | CEP / ExtendScript の host permission と OS permission |
| InDesign | `app.doScript` の `UndoModes` で script 全体を一つの transaction にできるが、export / filesystem は rollback 対象外 | script preference / dialog を明示し、通常は non-interactive にする | UXP Script は固定 permission、plugin は manifest permission。Startup Script 方式は権限が広い点も risk として扱う |

Undo は便利な回復手段ですが、安全境界や完全な transaction ではありません。save、overwrite、export、file deletion、network、application quit などは別の risk として扱います。

### Lifecycle、instance、payload、timeout

| Host | Startup / lifecycle | Multi-instance | Payload | Timeout / cancellation |
|---|---|---|---|---|
| After Effects | Startup / Shutdown JSXによるheadless runtimeとgeneration guardをrepository実装。cold-start、sleep/modal、複数versionは実機未検証 | **実装済み**。heartbeat、instance routing、instance別 FIFO | inline の統一上限は未実装。file は既定 1 MiB | daemon wait timeout と retained result は実装済み。timeout は実行停止を意味せず、実行中 JSX の強制停止は保証しない |
| Premiere Pro | UXP panel lifecycle、CEP fallback。panel の load / unload と再接続を実機確認する | **実装済み**。共通 broker 契約 | AE と同じ共通上限を Phase 1 で導入 | retained result は実装済み。Promise / transaction の協調的 cancellation に限定 |
| Photoshop | UXP `loadEvent: startup` を使用。modal 中と plugin reload 後の復旧を確認する | **実装済み**。共通 broker 契約 | AE と同じ共通上限を Phase 1 で導入 | retained result は実装済み。`executeAsModal` cancellation は code が interruptible な場合のみ |
| Illustrator | CEP panel lifecycle。host 起動時 load と panel 非表示時の動作を実機確認する | **実装済み**。共通 broker 契約 | AE と同じ共通上限を Phase 1 で導入 | retained result は実装済み。実行中 ExtendScript の強制停止は保証しない |
| InDesign | UXP Startup Scriptと未解決top-level Promiseによるpoll/heartbeatをrepository実装。実際の常駐・再接続は実機未検証 | **実装済み**。`HostSpec`、heartbeat、instance別 FIFO | inline/fileとも現在1 MiB上限。fileはRustで検証後にsourceとして転送 | **実装済み**。daemon timeoutとretained resultを共通化。`doScript`実行中の強制停止は前提にしない |

Payload の初期共通予算は次を提案します。値は host 実機 E2E で調整します。

- inline source: 256 KiB を既定上限とする。
- script file source: 現在どおり 1 MiB を既定上限とする。
- structured input / JSON result: それぞれ 1 MiB を既定上限候補とする。
- 画像、動画、project、巨大な descriptor 配列は JSON に埋め込まず、artifact path、size、SHA-256、MIME type を返す。
- `timeoutMs` は MCP 呼び出しの待機上限であり、host code の kill timeout ではない。timeout 後も `requestId` で結果を回収できるようにする。

## 共通公開 surface の案

### Capability ID

Tool 名の移行中も bridge capability は次の host-neutral ID で表します。

| Capability | 意味 |
|---|---|
| `script.execute.inline` | inline raw source を実行できる |
| `script.execute.file` | 検証済み script file を実行できる |
| `script.input.structured` | JSON input を code へ渡せる |
| `script.result.retained` | timeout 後に requestId で結果を回収できる |
| `script.guard.preflight` | 静的 risk report を実行前に返せる |
| `host.instances` | instance 列挙と target routing ができる |
| `host.undo.best-effort` | host undo / transaction wrapper を利用できる |

`script.execute.inline` の `inline` は「MCP request が source を直接持つ」という transport 上の意味です。InDesign PoCは受信sourceを `app.doScript(String)` のwrapperへ組み込んで実行します。

### Tool 名

長期的な共通 surface は次の6種を基本にします。

| Tool | 役割 |
|---|---|
| `run-script` | raw inline source + structured input を実行 |
| `run-script-file` | allowed root または trusted path/hash の script file を実行 |
| `get-script-result` | requestId で retained result を取得 |
| `list-<host>-instances` | host instance を列挙。複数 server を同時接続した際の識別のため host 名は残す |
| `get-capabilities` | runtime、bridge、permission、guard、payload 上限を動的に取得 |
| `run-bridge-test` | daemon から host までの疎通確認 |

現在の `run-jsx` / `run-jsx-file` / `get-jsx-result` は互換性を維持します。特に After Effects では既存の `run-script` が allowlist / direct-file 互換名として使われているため、すぐに名前を再利用しません。先に旧 allowlist dispatch を非公開の `run-operation` 相当へ整理し、その後 `run-script` を canonical 名として導入します。移行中の Prompt と Resource は advertised capability を見て canonical または互換 Tool を選びます。

`run-operation` は raw script より上位の便利 API ではなく、管理者が任意 code を無効化した環境向けの opt-in allowlist とします。

### `run-script` schema 案

```json
{
  "code": "async function main(input, mcp) { return { name: input.name }; }",
  "runtime": "auto",
  "input": { "name": "example" },
  "mode": "unsafe",
  "description": "Read the active document name",
  "declaredEffects": ["read"],
  "riskPolicy": "analyze",
  "preflightOnly": false,
  "targetInstanceId": "optional-instance-id",
  "timeoutMs": 10000,
  "confirmationToken": "optional-external-approval-token"
}
```

要点:

- inline code の `mode` は `unsafe` のみ。`unsafe` は sandbox ではなく、Adobe host の権限で実行することを明示する。
- `runtime` の許可値は `get-capabilities` が返す。例: `extendscript`、`uxp`、`uxp-file`。
- `input` と返却値は JSON serializable とし、code と業務データを分離する。
- `declaredEffects` は監査と確認 UI の材料であり、code の実際の副作用を保証しない。
- `preflightOnly: true` は静的 report を返して **実行しない**。host state を複製する本物の dry-run ではない。
- `confirmationToken` は LLM が同じ MCP 経由で自己発行できない設計にし、ユーザー UI / client policy が source hash、host instance、risk、期限へ署名して渡す。

返却 envelope 案:

```json
{
  "requestId": "01H...",
  "hostId": "photoshop",
  "instanceId": "ps-2026-1234",
  "runtime": "uxp",
  "state": "completed",
  "risk": {
    "level": "read",
    "detected": ["photoshop.documents.read"],
    "warnings": []
  },
  "result": { "name": "example.psd" },
  "audit": {
    "sourceSha256": "...",
    "sourceSizeBytes": 184,
    "mode": "unsafe"
  }
}
```

### `run-script-file` schema 案

```json
{
  "path": "C:/mcp-scripts/export.idjs",
  "mode": "trusted",
  "input": { "output": "C:/exports/document.pdf" },
  "description": "Export the active InDesign document",
  "riskPolicy": "confirm-destructive",
  "targetInstanceId": "optional-instance-id",
  "timeoutMs": 60000
}
```

`trusted` は path と SHA-256 の設定一致を要求します。caller が request 内で hash を指定するだけでは trusted になりません。詳細は [script file trust policy](script-file-security.md) を参照してください。

## Guard と risk policy

### 「safe mode」と呼ばない

削除 command の検知と block は有用ですが、任意 JavaScript / JSX の安全性を証明できません。そのため field 名や UI で `safe`、`sandboxed`、`restricted` と表示せず、挙動を表す `riskPolicy` を使います。

| `riskPolicy` | 動作 |
|---|---|
| `raw` | size / encoding / transport 検証と監査だけ行い、静的 risk で block しない |
| `analyze` | 静的 risk report を付けるが、そのまま実行する |
| `block-destructive` | scanner が destructive と検知した code を拒否する。未検知の副作用はあり得る |
| `confirm-destructive` | destructive と検知した場合、外部 approval token が一致したときだけ実行する |

`analyze` を推奨既定値とし、検知だけで挙動を変えません。`block-destructive` / `confirm-destructive` は deployment policy で明示的に有効化します。完全に raw な経路が必要な環境では `raw` を選べます。

### Risk 分類

| Level | 例 | 既定の扱い候補 |
|---|---|---|
| `read` | document / composition / layer / metadata の参照 | 実行、監査 |
| `reversible-write` | host undo にまとまりやすい property / text / layer 更新 | 実行、undo wrapper、監査 |
| `persistent-write` | save、save-as、export、overwrite、render、package | path と上書き条件を表示。必要に応じ確認 |
| `destructive` | document / layer / item / file deletion、close without save、quit、recursive mutation | block または外部確認 |
| `external` | shell、process launch、network upload、host 外 file write | permission 分離と外部確認 |
| `opaque` | `eval`、`new Function`、動的 module、数値 command ID、動的 action descriptor | high risk として扱う候補 |

削除検知は host ごとに少なくとも次を対象にします。

- JS / JSX の `delete`、`.remove()`、filesystem の `unlink` / `rm` / `deleteEntry`。
- Photoshop `batchPlay` の `_obj: "delete"`。
- close-without-save、application quit、project item / layer / page item の remove。
- export / save 時の既存 file overwrite。

### 静的検知の限界

次のような code は scanner を回避し得ます。

- `obj["re" + "move"]()` のような文字列連結と動的 property access。
- function alias、higher-order function、prototype 変更、別 file / module の間接呼び出し。
- `eval` / `new Function` / `app.doScript` で生成した二段目の code。
- `app.executeCommand(number)`、計算された Photoshop action descriptor、plugin 固有 API。
- 「全 item を選択して置換」のように、delete という単語を使わない実質的削除。
- network から取得した code や data によって初めて決まる操作。
- 難読化、encoding、圧縮、binary / `.jsxbin`。

したがって `block-destructive` が通ったことは「安全」を意味しません。scanner が理解できない code を `opaque` として上位 risk にすることはできますが、false positive と false negative の両方が残ります。

### 防御層

Guard は単独ではなく、次の層を組み合わせます。

1. **入力境界:** JSON schema、runtime enum、byte 上限、UTF-8、timeout 上限。
2. **source identity:** canonical path、allowed roots、trusted exact path + SHA-256、実行 source hash の監査。
3. **静的 preflight:** host 別 parser / token scanner、risk 分類、検知箇所の提示。
4. **外部確認:** source hash、instance、risk、出力 path、期限に束縛した approval token。
5. **host wrapper:** undo / transaction、non-interactive 規約、cooperative cancellation、結果 envelope。
6. **権限分離:** UXP manifest を最小権限にし、OS の低権限 user / ACL、host の file-network preference、daemon bind を分離。
7. **監査と復旧:** requestId、source hash、paths、risk report、result retention、artifact metadata、backup / versioning。

raw execution を必要としない運用では、管理者が inline code を無効化し、trusted file または allowlist operation だけを有効にできます。これは deployment policy であり、raw mode 自体を見かけ上安全にするものではありません。

## Structured Tool を追加する判断基準

次のいずれかを満たす場合だけ、操作ごとの Tool を追加します。

1. host script から扱えない daemon / installer / lifecycle / instance routing の操作である。
2. binary や巨大 artifact を JSON code/result に載せず転送・管理する必要がある。
3. OS 側の permission、path、confirmation を Rust 側で強制する必要がある。
4. request の idempotency、resume、cancel、long-running progress を broker が管理する必要がある。
5. host version 差を吸収する安定 wrapper があり、複数の主要 workflow で繰り返し使われ、token / latency 削減が測定できる。
6. 任意 code を無効化した管理環境に、狭い allowlist として提供する明確な需要がある。

追加しない例:

- `create-composition`、`rename-layer`、`set-text` のように短い script で完結し、複数操作との合成が重要なもの。
- host DOM の薄い一対一 wrapper。
- version ごとに引数や挙動が頻繁に変わり、結局 raw script fallback が必要なもの。

追加候補:

- `get-capabilities`、`list-<host>-instances`、`run-bridge-test`、retained result / cancellation。
- frame / thumbnail の artifact 作成と cleanup。
- render / export job の進捗・resume・artifact metadata を broker が管理する wrapper。
- policy 管理された file delete / cleanup。ただし対象 root と ownership marker を Rust が検証できる場合に限る。

Structured Tool を追加するときは、以下を checklist にします。

- raw script / recipe では不十分な理由を Issue に書いたか。
- host-neutral capability ID と host 固有差を分けたか。
- schema に `targetInstanceId`、`timeoutMs`、`description`、requestId recovery が必要か検討したか。
- read / write / persistent / destructive / external / opaque の risk を分類したか。
- undo、modal、filesystem、overwrite、cancel、timeout 後の実行継続を説明したか。
- payload と artifact の上限を決めたか。
- raw script と同じ監査 envelope を使うか、使えない理由を記録したか。
- 実機 version matrix と E2E fixture を追加したか。

## Phase roadmap

### Phase 0 — 設計の固定（この文書）

- raw-script-first と structured Tool 追加基準を合意する。
- capability ID、schema、risk 用語を source of truth にする。
- 現在の Tool 名は急に変更せず、互換 alias と migration を前提にする。

### Phase 1 — 共通 script contract

- 5 host の schema / result envelope を揃える。
- `get-capabilities` に runtime、inline/file、permission、payload、guard support を追加する。
- inline 256 KiB、input/result 1 MiB の暫定上限と oversized artifact 方針を実装・計測する。
- `preflightOnly` と `riskPolicy: raw | analyze` を先に実装し、検知結果を監査へ残す。
- `run-script` canonical 名への移行方法を決め、AE の旧 allowlist `run-script` と衝突しない migration を行う。

### Phase 2 — InDesign と AE lifecycle のrepository PoC（実装済み）

- **Issue #21:** `id-mcp`、`HostSpec`、UXP Startup Script、`app.doScript(String)` wrapper、JSON literal input、retained result、heartbeatをrepositoryへ実装した。
- **Issue #22:** AEのStartup / Shutdown JSXとheadless generation guardを実装した。CEPはAE invisible cold-startの保証が不足するため現時点では不採用とした。
- 両PoCともAdobe実機でのcold-start、再接続、modal/sleep、複数instance、raw実行を確認するまでproduction readyとは扱わない。

### Phase 3 — Guard policy

- host 別 static scanner と risk report を追加する。
- `block-destructive` を opt-in で提供し、UI と docs に非 sandbox であることを表示する。
- 外部 approval token の署名者、hash binding、TTL、replay 防止を定義してから `confirm-destructive` を実装する。
- OS user / ACL、UXP manifest、AE preference、daemon bind / authentication の hardening を行う。

### Phase 4 — 実機 parity と配布

- 5 host の read / write / export、restart、reconnect、multi-instance、modal、timeout、oversized payload を実機 matrix で確認する。
- Windows / macOS installer、署名、公証、upgrade / uninstall を host component ごとに検証する。
- 実測で効果が確認できた structured Tool だけを追加する。

## 公式資料

- [After Effects: Scripts](https://helpx.adobe.com/after-effects/using/scripts.html)
- [Premiere Pro UXP introduction](https://developer.adobe.com/premiere-pro/uxp/introduction/)
- [Premiere Pro Project API (`executeTransaction`)](https://developer.adobe.com/premiere-pro/uxp/ppro-reference/classes/project/)
- [Photoshop `batchPlay`](https://developer.adobe.com/photoshop/uxp/ps_reference/media/batchplay/)
- [Photoshop `executeAsModal`](https://developer.adobe.com/photoshop/uxp/2022/ps-reference/media/executeasmodal/)
- [Illustrator developer overview](https://developer.adobe.com/illustrator/)
- [InDesign UXP: Scripts and Plugins](https://developer.adobe.com/indesign/uxp/introduction/next-steps/script-and-plugin/)
- [InDesign UXP Script manifest permissions](https://developer.adobe.com/indesign/uxp/resources/fundamentals/manifest/)
- [InDesign Startup Scripts](https://developer.adobe.com/indesign/uxp/scripts/tutorials/tips-tricks/)
- [InDesign `Application.doScript`](https://developer.adobe.com/indesign/uxp/dom/api/a/application/)
- [InDesign script arguments](https://developer.adobe.com/indesign/uxp/scripts/tutorials/arguments/)
- [InDesign script result](https://developer.adobe.com/indesign/uxp/scripts/tutorials/script-result/)
