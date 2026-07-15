# AGENTS.md

このファイルは、`adobe-mcp-rs` の現状と運用注意点を、作業エージェント向けに簡潔にまとめたものです。

## 現状サマリ（2026-07-15）

- repository 名は Adobe アプリ横断の `adobe-mcp-rs` へ変更済み。
- **Primary** は After Effects。Rust バイナリ `ae-mcp`、ScriptUI / ExtendScript bridge、TCP daemon broker、instance routing / retained result が実装済み。
- **Experimental** は Premiere Pro / Photoshop / Illustrator。各 host の Rust バイナリと bridge の初期実装はあるが、実機 E2E、配布、daemon broker の同等性が未完了。
  - Premiere Pro: `pr-mcp` + UXP（25.6+）。CEP / ExtendScript（24.0+）は fallback。
  - Photoshop: `ps-mcp` + UXP（23.3+）。読み取り中心の小さな allowlist と任意 UXP code 実行。
  - Illustrator: `ai-mcp` + CEP / ExtendScript（24.0+）。document / artboard / layer 読み取りと export の初期実装。
- 4 host の `serve-daemon` は `daemon-core` の共通 TCP broker を使う。MCP stdio server は host 別 daemon を経由し、instance別FIFO・別instance並列・global exclusive・retained resultを共有する。
- 状態区分の基準、host 別 runtime / 最小機能 / 制約の source of truth は `docs/adobe-host-roadmap.md`。
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
cargo test --workspace
cargo build --release -p ae-mcp -p pr-mcp -p ps-mcp -p ai-mcp
.\target\release\ae-mcp.exe health
.\target\release\ae-mcp.exe bridge run-script --script listCompositions --parameters '{}'
.\target\release\ae-mcp.exe bridge get-results
```

`health` は binary、bridge root、host 別 daemon address の確認に留まり、Adobe host 内の実行確認にはならない。host panel の `Auto-run commands` を有効にし、対象 binary の `serve-daemon` を起動して `list-*-instances` と `run-bridge-test` まで確認する。

## ドキュメント

- セットアップ: `docs/setup-codex-mcp.md`
- 開発段階: `docs/development-stages.md`
- 移行仕様: `docs/specification-rust-migration.md`
