# after-effects-mcp-rs 開発ステージ計画（Rust移行）

- 作成日: 2026-03-23
- 参照仕様: `docs/specification-rust-migration.md`

## 1. 全体方針

- 段階的移行（Big Bangを回避）
- 互換優先（現行クライアント影響を最小化）
- 各ステージで「リリース可能」な区切りを作る

## 2. マイルストーン一覧

| Stage | 名称 | 目的 | 主成果物 | 完了条件（Exit Criteria） |
|---|---|---|---|---|
| 0 | Baseline固定 | TS現行の挙動を固定化 | 互換テスト項目、現行仕様スナップショット | 主要ツールの入出力サンプルが揃う |
| 1 | Rust土台構築 | Rustプロジェクト骨格を作る | Cargo workspace、CLI、ログ、設定読み込み | `serve-stdio`が起動しヘルス応答 |
| 2 | ブリッジ層移植 | JSONファイル連携をRust化 | command/result I/O、ポーリング、タイムアウト | `run-script` + `get-results`が動作 |
| 3 | MCP互換移植 | 現行機能をRustに寄せる | resource/prompt/tool一式 | 主要ツール互換テストが通過 |
| 4 | 常駐化 | OS別の常駐運用を実装 | daemonモード、Windows autostart / macOS launchd制御CLI | Win/macで手動起動成功 |
| 5 | インストーラ化 | 配布/導入を自動化 | Win/macインストーラ、アンインストーラ | 1コマンド導入・解除が成功 |
| 6 | 署名/公証とRC | 配布品質を上げる | 署名・公証パイプライン、RC版 | セキュリティ警告なしで配布可能 |
| 7 | GAリリース | 本番移行完了 | v1.0.0、移行ガイド、運用Runbook | TS版代替として運用可能 |

## 3. ステージ詳細

## Stage 0: Baseline固定（1週間）

### タスク
1. 現行TS版のMCP公開要素（tools/resources/prompts）を棚卸し。
2. 各主要ツールの成功/失敗レスポンスをJSONサンプル化。
3. AE未起動、パネル未起動、結果stale等の異常系を記録。

### 成果物
1. 互換性チェックリスト
2. ゴールデンレスポンス（fixtures）

### 完了条件
1. Rust版で比較可能な入出力基準が確定している。

## Stage 1: Rust土台構築（1週間）

### タスク
1. Cargo workspace作成（`ae-mcp`, `mcp-core`, `bridge-core`）。
2. `clap`でCLI構築（`serve-stdio`, `--version`, `--health`）。
3. `tracing`でログ初期化、`config.toml`読み込み導入。

### 成果物
1. 起動可能な最小Rustバイナリ
2. 開発用設定ファイルテンプレート

### 完了条件
1. Windows/macOS双方で`serve-stdio`起動確認。

## Stage 2: ブリッジ層移植（2週間）

### タスク
1. `ae_command.json`書き込みと状態遷移管理。
2. `ae_mcp_result.json`監視、タイムアウト、鮮度判定。
3. エラーハンドリング（未起動/不正JSON/破損ファイル）。

### 成果物
1. `bridge-core`実装
2. ブリッジ層Unit/Integrationテスト

### 完了条件
1. `run-script`と`get-results`相当機能がRustで実行可能。

## Stage 3: MCP互換移植（2週間）

### タスク
1. 現行resource/prompt/toolをRust実装。
2. 既存tool名・引数互換維持（破壊変更禁止）。
3. `get-help`/effects系ヘルプ出力を移植。

### 成果物
1. Rust版MCP API
2. 互換テスト結果レポート

### 完了条件
1. Stage 0のゴールデンレスポンス比較で重大差分なし。

## Stage 4: OS別常駐化（2週間）

### タスク
1. daemon実行モード追加。
2. Windows `autostart install|uninstall|start|stop|status` と macOS `service install|uninstall|start|stop|status` を実装。
3. Windows current-user Run key / PID管理と macOS launchd の最小登録動作検証。

### 成果物
1. サービス制御CLI
2. OS別運用手順ドラフト

### 完了条件
1. 再起動後もサービスが自動復帰（設定に応じて）。

## Stage 5: インストーラ化（2週間）

### タスク
1. Windows用インストーラ（MSI）作成。
2. macOS用インストーラ（pkg等）作成。
3. インストール時サービス登録、アンインストール時解除。

### 成果物
1. Win/macインストーラ成果物
2. インストーラE2Eテスト手順

### 完了条件
1. クリーン環境で導入から稼働まで手作業なしで完了。

## Stage 6: 署名/公証とRC（1-2週間）

### タスク
1. Windowsコード署名導入。
2. macOS署名 + Notarization導入。
3. `v1.0.0-rc`公開とフィードバック反映。

### 成果物
1. 署名済みRCビルド
2. 既知課題リスト

### 完了条件
1. 配布時のセキュリティ警告が許容範囲内。

## Stage 7: GAリリース（1週間）

### タスク
1. ドキュメント最終更新（README/移行ガイド/Runbook）。
2. TS版からRust版への移行手順確定。
3. `v1.0.0`タグ・リリースノート作成。

### 成果物
1. 本番リリース一式
2. 運用引き継ぎ資料

### 完了条件
1. 既存利用者がRust版へ移行完了できる。

## 4. 並行実施の推奨

1. Stage 2開始時点でCI/CD雛形（クロスOSビルド）を先行着手。
2. Stage 4開始時点でインストーラPoCを並行開始。
3. Stage 5中に署名/公証の権限・証明書準備を先行完了。

## 5. ステージゲート（必須チェック）

1. 機能ゲート:
   - 重大バグ（P1）が0件
2. 互換ゲート:
   - 主要ツール互換率100%
3. 配布ゲート:
   - Win/macの成果物がCIで再現可能
4. 運用ゲート:
   - ログ/障害調査手順が文書化済み

## 6. 初回実装の優先順位（推奨）

1. `run-script` / `get-results` / `create-composition`
2. `setLayerKeyframe` / `setLayerExpression`
3. `apply-effect` / `apply-effect-template`
4. 補助ツール（`test-animation`, `run-bridge-test`, ヘルプ系）
