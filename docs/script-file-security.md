# `run-jsx-file` の信頼ポリシー

`run-jsx-file` は、Rust 側でファイルを検証してから Adobe host へ送ります。この検証は誤操作防止と監査のためのガードであり、sandbox ではありません。特に `mode = "unsafe"` は「隔離実行」を意味せず、スクリプトは対象 Adobe host と同じ権限で動作します。

## 実行モード

| mode | 用途 | 許可条件 |
|---|---|---|
| `unsafe` | ユーザーまたは LLM が用意したファイル | canonical path が `script_files.allowed_roots` のいずれかの配下 |
| `trusted` | 同梱または配布時に固定した既知スクリプト | canonical path と SHA-256 が `script_files.trusted_scripts` の同じ entry に完全一致 |

両 mode とも、入力 path は絶対パスでなければなりません。Rust は path と設定 root を `canonicalize` してから比較するため、`..`、symlink、Windows junction が許可 root の外を指す場合は拒否します。通常ファイル、サイズ上限、UTF-8、host 別拡張子も検証します。

| host | 拡張子 |
|---|---|
| After Effects / Illustrator | `.jsx` |
| Premiere Pro / Photoshop | `.js`, `.jsx` |
| InDesign | `.idjs` |

`.jsxbin` は UTF-8 source として監査できないため対象外です。

## 設定

host ごとの TOML config に以下を追加します。root と trusted script path は、存在する directory / file の絶対パスを指定してください。

```toml
[script_files]
allowed_roots = [
  'C:\Users\me\Documents\Adobe Scripts',
  'D:\projects\motion\scripts',
]
max_bytes = 1048576

[[script_files.trusted_scripts]]
path = 'C:\Program Files\adobe-mcp-rs\scripts\approved.jsx'
sha256 = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
```

`trusted` は trusted entry の path と hash の両方を要求し、`allowed_roots` だけでは許可されません。逆に `unsafe` は trusted entry を参照せず、allowed root 配下かだけを判定します。

### 旧設定からの移行

`script_files` section が存在しない旧 config は引き続き読み込めます。その場合に限り、MCP process の起動時 current working directory を既定の `allowed_roots` とします。以前のように任意の絶対パスを実行することはできません。

運用では current working directory に依存しないよう、host ごとの config に `[script_files]` と必要最小限の `allowed_roots` を明示してください。`[script_files]` を明示して `allowed_roots` を空にすると、`unsafe` な file 実行を無効化できます。

## 監査情報

検証後の次の情報を daemon request registry に保存し、初回応答と `get-jsx-result` の retained record で確認できます。

- `hostId`
- `mode`
- canonicalized `sourcePath`
- `sourceSha256`
- `sourceSizeBytes`

hash と path は実行した source の追跡に使えますが、コードの安全性を証明するものではありません。`run-jsx` に直接渡した文字列 code も従来どおり `mode = "unsafe"` のみで、sandbox 化されません。
