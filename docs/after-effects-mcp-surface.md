# After Effects MCP public surface

- 最終更新: 2026-07-15
- 対象: `ae-mcp serve-stdio`

この文書をAfter Effects MCPの公開Tool、Resource、Promptと非公開互換dispatchの契約とします。実装上のsource of truthは`mcp-core::PUBLIC_TOOL_NAMES`、`mcp-core::LEGACY_TOOL_NAMES`、`tool_specs()`です。

## 公開Tool

`tools/list`は次の9 Toolだけを返します。通常のAE実行は`serve-stdio`から`serve-daemon`へ転送され、instance routing、FIFO、timeout後のresult retentionを共通brokerが処理します。

| Tool | 用途 |
|---|---|
| `run-jsx` | `mode: "unsafe"`を明示して文字列JSXを同期実行 |
| `run-jsx-file` | allowed root内の`unsafe`、またはpath/SHA-256 allowlist済みの`trusted`ローカルJSXファイルを同期実行 |
| `get-jsx-result` | `requestId`でretained resultを回収 |
| `list-ae-instances` | daemonが認識するAE instanceを列挙 |
| `get-results` | 最新または指定`requestId`のretained resultを回収 |
| `get-help` | この公開surfaceと運用上の注意を取得 |
| `save-frame-png` | 単一frameのPNG previewを保存 |
| `cleanup-preview-folder` | preview PNGを条件付きで削除 |
| `run-bridge-test` | daemon brokerからAE bridgeまでの最短疎通確認 |

`run-script`は再公開しません。allowlistは旧clientとの互換に有用ですが、現行実装は非同期direct-file互換経路であり、同期daemon brokerの公開契約と完了条件が異なります。また、`run-jsx`の明示的なunsafe境界とは別のtrusted script境界もまだ定義されていません。

## ResourceとPrompt

公開Resourceは`aftereffects://compositions`です。`resources/read`は`listCompositions`をdaemon brokerへ送り、Tool実行と同じinstance routing / FIFOに参加します。

公開Promptは次の6個です。Prompt自体は操作を実行せず、公開Toolまたは公開Resourceだけを使う手順を返します。

| Prompt | 案内する公開経路 |
|---|---|
| `list-compositions` | `aftereffects://compositions`、必要時のみ`run-jsx` |
| `analyze-composition` | `run-jsx` |
| `create-composition` | `run-jsx` |
| `save-preview-png` | `save-frame-png`、必要時`get-results` |
| `render-queue-setup` | `run-jsx` |
| `cleanup-preview-folder` | `cleanup-preview-folder` |

## 非公開互換dispatch

次の名前は`tools/list`へ出しません。旧clientからの呼出は互換目的で受理し、結果の先頭に非推奨案内と公開置換先を付けます。新しいPrompt、セットアップ、E2E手順では使用しません。

公開置換先`run-jsx`:

- `run-script`
- `create-composition`
- `setLayerKeyframe`
- `setLayerExpression`
- `test-animation`
- `apply-effect`
- `apply-effect-template`
- `list-supported-effects`
- `describe-effect`
- `render-queue-add`
- `render-queue-status`
- `render-queue-start`
- `render-queue-is-rendering`
- `set-current-time`
- `get-current-time`
- `set-work-area`
- `get-work-area`
- `get-composition-markers`
- `set-suppress-dialogs`
- `get-suppress-dialogs`
- `project-open`
- `project-close`
- `project-save`
- `project-save-as`
- `application-quit`
- `mcp_aftereffects_applyEffect`
- `mcp_aftereffects_applyEffectTemplate`
- `mcp_aftereffects_listSupportedEffects`
- `mcp_aftereffects_describeEffect`

公開置換先`get-help`:

- `mcp_aftereffects_get_effects_help`

互換dispatchの一部はroot command/result fileを使うため、公開surfaceと同じbroker保証を前提にできません。移行先の公開Toolでは`targetInstanceId` / `targetVersion`、`timeoutMs`、`requestId` recoveryを使用してください。

## 最短確認

1. Startup bridgeのheartbeatが更新されていることを確認する。
2. `ae-mcp serve-daemon`を起動する。
3. 公開Tool `list-ae-instances`で対象instanceを確認する。
4. 公開Tool `run-bridge-test`で結果JSONを確認する。

この確認には非公開互換dispatchを使用しません。
