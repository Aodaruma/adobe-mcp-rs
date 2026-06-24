# Adobe host roadmap

この文書は、`adobe-mcp-rs` を After Effects / Premiere Pro から Photoshop / Illustrator へ広げるための作業メモです。

## 調査結果

| Host | 現状 | 判断 |
|---|---|---|
| After Effects | `ae-mcp`、ScriptUI JSX bridge、daemon broker、instance registry が実装済み | 既存機能の整理と broker の安定化を継続 |
| Premiere Pro | `pr-mcp`、UXP bridge、CEP fallback、allowlist script が実装済み | 実験的。daemon は AE と同等ではないため hardening が必要 |
| Photoshop | repository 内には未実装 | UXP plugin を第一候補にする。Photoshop DOM と `batchPlay` で実装可能範囲を広げる |
| Illustrator | `ai-mcp` と CEP / ExtendScript bridge の初期実装あり | 実験的。読み取り系と exportDocument から検証し、installer hardening は別作業 |

参考:

- [UXP for Adobe Photoshop](https://developer.adobe.com/photoshop/uxp/2022/)
- [Photoshop API reference](https://developer.adobe.com/photoshop/uxp/2022/ps-reference/)
- [Premiere Pro UXP API](https://developer.adobe.com/premiere-pro/uxp/)
- [Illustrator developer overview](https://developer.adobe.com/illustrator/)
- [UXP host version table](https://developer.adobe.com/xd/uxp/uxp/versions/)

## 共通化するもの

今の `mcp-core` / `bridge-core` には After Effects 由来の命名や前提が残っています。Photoshop / Illustrator を足す前に、次の host adapter を切り出します。

```rust
pub struct HostSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub binary_name: &'static str,
    pub bridge_root_name: &'static str,
    pub command_file_name: &'static str,
    pub result_file_name: &'static str,
    pub instance_tool_name: &'static str,
}
```

最初は trait より data struct を優先します。host ごとの差分が増えてから trait に分けた方が、既存 AE 実装を壊しにくいためです。

## Bridge protocol

host ごとに directory 名は分けつつ、中身の schema は揃えます。

```text
~/Documents/<host>-mcp-bridge/
  instances/<instanceId>/
    heartbeat.json
    command.json
    result.json
  registry/<requestId>.json
```

`heartbeat.json` には最低限この情報を入れます。

```json
{
  "protocolVersion": 1,
  "instanceId": "ps-...",
  "hostId": "photoshop",
  "appName": "Photoshop",
  "appVersion": "27.0",
  "bridgeRuntime": "uxp",
  "capabilities": ["run-code", "documents.list", "layers.list"],
  "updatedAt": "2026-06-25T00:00:00Z"
}
```

Premiere 側で `aeInstance` を text replace している箇所は、共通 schema 側を `hostInstance` に寄せて解消します。

## Photoshop plan

Photoshop は UXP の公式 document が整っているため、最初に着手しやすい host です。

Phase 1:

- `crates/ps-core` と `crates/ps-mcp` を追加
- `src/photoshop/uxp/mcp-bridge-photoshop` を追加
- `list-photoshop-instances`
- `run-code` / `run-code-file` または既存互換の `run-jsx` / `run-jsx-file`
- `get-result` / `get-results`
- `get-help`
- `run-script` allowlist: `ping`, `getAppInfo`, `listDocuments`, `getActiveDocument`, `listLayers`

Phase 2:

- document open/save/export
- layer selection, visibility, rename, group traversal
- `batchPlay` wrapper
- image export preview
- modal execution policy and error normalization

最初の public tool surface は小さく保ち、個別操作は allowlist script か UXP code helper に寄せます。

## Illustrator plan

Illustrator は Photoshop と同じ前提で UXP bridge を作り始めない方が安全です。公式 overview は HTML panel、C++ plugin、JavaScript scripting を案内している一方で、third-party UXP の公開範囲は事前確認が必要です。

Spike:

- 現行 Illustrator で UXP Developer Tool から third-party plugin を load できるか確認
- `require` 可能な Illustrator host API が公開されているか確認
- ExtendScript で file bridge panel 相当を作れるか確認
- CEP fallback のサポート状況を確認
- 必要なら native C++ plugin + localhost bridge の実現性を検討

Phase 1 実装済み:

- `crates/ai-core` と `crates/ai-mcp`
- `list-illustrator-instances`
- `run-script` allowlist: `ping`, `getAppInfo`, `listDocuments`, `getActiveDocument`, `listArtboards`, `listLayers`, `exportDocument`
- artboard / layer / selection / export の読み取り系を優先

残作業:

- CEP bridge の署名・debug mode・配置手順を installer に組み込む
- `exportDocument` の format/options を実機 Illustrator で検証する
- Illustrator DOM の selection / pageItem 操作を allowlist script として追加する

## Premiere hardening

Premiere は既に `pr-mcp` があるため、Photoshop/Illustrator より先に設計 debt を減らします。

- `serve-daemon` を AE と同等の broker にするか、削除して direct bridge として明記する
- result schema の `aeInstance` 由来をなくす
- UXP bridge の install 手順と package 生成を docs / scripts に入れる
- CEP fallback の位置づけを明確化する
- `run-script` helper を UXP bridge 側で test 可能にする

## Release / packaging

repository 名に合わせ、release artifact 名は `adobe-mcp-rs-*` に寄せます。ただし binary 名は host ごとのままにします。

- `ae-mcp`
- `pr-mcp`
- `ps-mcp`
- `ai-mcp`

installer は最終的に host 別 component を選べる形が望ましいです。初期段階では host ごとの portable archive を優先し、MSI/PKG 統合は後回しにします。
