# GAリリースチェックリスト（Stage 7）

- 最終更新: 2026-03-23

## 1. 事前確認

1. `cargo check` が成功
2. `cargo test` が成功
3. AEの公開MCP操作を手動確認（`list-ae-instances`, `run-bridge-test`, `run-jsx`, `get-results`）
4. Windows/macOS のインストーラ生成確認

## 2. 署名・公証

1. Windows署名済み（`.exe` / `.msi`）
2. macOS署名+Notarization済み（`.pkg`）
3. 検証コマンド結果を保存

## 3. ドキュメント

1. セットアップ手順更新
2. 移行ガイド更新
3. Runbook更新
4. 既知制約の明記

## 4. リリース実施

1. `vX.Y.Z` タグ作成
2. CI完了確認（installer-build / rc-release）
3. アーティファクト公開
4. リリースノート公開

## 5. リリース後

1. 初期ユーザーの導入可否確認
2. 重大不具合（P1）監視
3. Hotfix要否判断
