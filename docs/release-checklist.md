# GAリリースチェックリスト（Stage 7）

- 最終更新: 2026-07-15

## 1. 事前確認

1. `cargo check` が成功
2. `cargo test` が成功
3. AEの公開MCP操作を手動確認（`list-ae-instances`, `run-bridge-test`, `run-jsx`, `get-results`）
4. Windows/macOS のインストーラ生成確認
5. 4 binary の OS 別 help を確認（Windows は `autostart` のみ、macOS は `service` のみ）
6. Windows は各 binary で `autostart install|start|status|stop|uninstall`、macOS は `service install|start|status|stop|uninstall` を確認
7. unsupported なサブコマンドと起動・停止未完了が成功終了しないことを確認

## 2. 署名・公証

1. Windows署名済み（`.exe` / `.msi`）
2. macOS署名+Notarization済み（`.pkg`）
3. 検証コマンド結果を保存

## 3. ドキュメント

1. セットアップ手順更新
2. 移行ガイド更新
3. Runbook更新
4. 既知制約の明記
5. setup / Runbook / installer E2E が Windows autostart と macOS launchd を同じ意味で案内している

## 4. リリース実施

1. `vX.Y.Z` タグ作成
2. CI完了確認（installer-build / rc-release）
3. アーティファクト公開
4. リリースノート公開
5. Windows MSI の初回 install が autostart を暗黙に有効化しないこと、upgrade が既存 Run key を修復すること、通常 uninstall が daemon と Run key を除去することを確認

## 5. リリース後

1. 初期ユーザーの導入可否確認
2. 重大不具合（P1）監視
3. Hotfix要否判断
