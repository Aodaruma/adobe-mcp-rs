# AGENTS.md

このファイルは、`adobe-mcp-rs` の現状と運用注意点を、作業エージェント向けに簡潔にまとめたものです。

## 現状サマリ（2026-06-25）

- repository 名は Adobe アプリ横断の `adobe-mcp-rs` へ変更済み。
- 主要実装は Rust バイナリ `ae-mcp`（`serve-stdio` / `serve-daemon` / `service` / `bridge`）で稼働。
- Premiere Pro 向けに `pr-mcp` と UXP bridge（CEP fallback あり）が追加済み。ただし `pr-mcp serve-daemon` は AE broker と同等ではないため実験的扱い。
- Photoshop / Illustrator は未実装。`docs/adobe-host-roadmap.md` に今後の方針を記載。
- npm/TypeScript サーバー実装は削除済み（`package.json` / `src/index.ts` 等は廃止）。
- AE 連携は `mcp-bridge-auto.jsx` 経由（`~/Documents/ae-mcp-bridge` の command/result ファイル）。
- `applyEffect` / `applyEffectTemplate` は ExtendScript 互換化済み（`Object.keys` 非依存）。
- ターゲット指定は `compId/layerId`、`compName/layerName`、`compIndex/layerIndex` をサポート。
- 追加済み機能:
  - `list-supported-effects` / `mcp_aftereffects_listSupportedEffects`
  - `describe-effect` / `mcp_aftereffects_describeEffect`
  - `run-script` allowlist に `listSupportedEffects` / `describeEffect` を追加

## 実装済みのエフェクト関連仕様

- `smooth-gradient` テンプレート追加済み。
- Ramp 系はフォールバック実装済み（`ADBE Ramp` -> `Ramp` -> `ADBE 4ColorGradient` 系）。
- `describe-effect` は指定レイヤー上で一時適用してパラメータ情報を返し、終了時に削除する。
- `list-supported-effects` は既知カタログをプローブして利用可否を返す（全エフェクト列挙ではない）。

## 運用上の注意

- リポジトリコンテナ直下は `.repo.git/`（bare repo）、`main/`（main worktree）、`worktrees/`（Issue別worktree）の構成。通常の編集・Git操作は `main/` または `worktrees/<name>/` で行う。詳細は `docs/worktree.md`。
- AE 側で `Window > mcp-bridge-auto.jsx` を開き、`Auto-run commands` を ON にすること。
- `ae_command.json` が `pending` のままなら、パネル未起動・Auto-run OFF・AE再読込漏れを疑うこと。
- `getLayerInfo`（ブリッジ版）は「アクティブコンポ」前提。アクティブでないと `No active composition` を返す。
- 外部プラグイン系は表示名と matchName が一致しない場合がある。
  - 例: Glow は環境により `ADBE Glow` ではなく `ADBE Glo2` になる。
  - 不明時は `describe-effect` を先に使って matchName/プロパティを確認する。

## 推奨の確認コマンド

```powershell
cargo build --release -p ae-mcp
.\target\release\ae-mcp.exe health
.\target\release\ae-mcp.exe bridge run-script --script listCompositions --parameters '{}'
.\target\release\ae-mcp.exe bridge get-results
```

## ドキュメント

- セットアップ: `docs/setup-codex-mcp.md`
- 開発段階: `docs/development-stages.md`
- 移行仕様: `docs/specification-rust-migration.md`
