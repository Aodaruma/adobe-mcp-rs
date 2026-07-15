# 共通 raw script 契約 v1

After Effects / Premiere Pro / Photoshop / Illustrator / InDesign は、canonical Tool として `run-script`、`run-script-file`、`get-script-result`、`get-capabilities`、`cancel-script-request` を公開します。After Effects等の `run-jsx` / `run-jsx-file` / `get-jsx-result` は互換aliasとして維持します。従来 `run-script` がallowlist操作だったhostでは、`script` fieldを持つ旧payloadも引き続き受理し、`code` fieldを持つ新payloadとdispatch時に区別します。

## 境界

- canonical inline sourceは256 KiB、file sourceは既定1 MiB、structured inputとJSON resultは各1 MiB。
- `timeoutMs` の既定上限は600,000 ms。timeoutは待機上限で、host codeの停止を意味しません。
- retentionは既定3,600秒、設定上限86,400秒。request registryにはsource本文ではなくhash、size、risk report、instance、state、期限を残します。
- result上限を超えた場合、binaryや巨大JSONを埋め込まず、artifact path、size、SHA-256、MIME typeを返します。
- cancellationはqueue中なら停止できます。dispatch後は協調的で、host codeが完了して結果を返す場合があります。

`get-capabilities` は上記上限、runtime、active instance、permission、guardのdeployment状態をschemaVersion 1で返します。

## risk policy

既定の`analyze`はbest-effort lexical reportを監査へ付けますが、実行を止めません。`raw`は内容検査を省略します。`block-destructive`と`confirm-destructive`は設定で明示的に有効化しない限り使用できません。いずれもsandbox、安全性の証明、完全なparserではありません。

`preflightOnly: true`はreportを返して実行しません。本物のdry-runやhost stateの複製ではありません。

### external approval token prototype

`confirm-destructive`のtoken形式は次です。MCP serverは検証だけを行い、発行Toolを公開しません。

```text
v1.<base64url(JSON claims)>.<hex HMAC-SHA256("v1." + payload)>
```

claimsは`version`、`hostId`、`targetInstanceId`、`sourceSha256`、`risk: "destructive"`、`expiresAtUnix`、16文字以上の`nonce`を持ちます。最大TTLは10分で、利用時にbridge rootの`approval-replay`へmarkerを排他的に作るためsingle-useです。共有鍵は`script_contract.approval_hmac_secret_env`が指す環境変数から読み、32 bytes以上を要求します。このprototypeは外部UI/policy serviceとの契約確認用であり、OS secret store、署名者の認証、ACL hardeningを代替しません。

## result envelope

request recordは既存の`status` / `hostInstance`等を維持しつつ、`state`、`hostId`、`instanceId`、`runtime`、`risk`、`audit`を追加します。これにより旧clientを壊さず、5 hostで同じfieldを読めます。
