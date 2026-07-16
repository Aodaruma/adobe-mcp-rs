# GAリリースチェックリスト（Stage 7）

- 最終更新: 2026-07-16

## 1. 事前確認

1. `cargo check` が成功
2. `cargo test` が成功
3. AEの公開MCP操作を手動確認（`list-ae-instances`, `run-bridge-test`, `run-jsx`, `get-results`）
4. Windows/macOS のインストーラ生成確認
5. 5 binary の OS 別 help を確認（Windows は `autostart` のみ、macOS は `service` のみ）
6. Windows は各 binary で `autostart install|start|status|stop|uninstall`、macOS は `service install|start|status|stop|uninstall` を確認
7. unsupported なサブコマンドと起動・停止未完了が成功終了しないことを確認
8. macOS archiveのstageとpkg展開後payloadにある5 binaryが、すべて`arm64`と`x86_64`を含むuniversal2であることを`lipo -archs`で確認

## 2. 署名・公証

signed modeでは、次の1〜6を確認する。

1. Windows署名済み（`.exe` / `.msi`）
2. macOSの5 universal2 binaryがDeveloper ID Applicationで署名され、`codesign --verify --strict --verbose=2`に成功
3. 最終pkgが別のDeveloper ID Installer identityで署名され、`pkgutil --check-signature`に成功
4. 署名済み最終pkgの`notarytool submit --wait`、`stapler staple`、`stapler validate`、`spctl --assess --type install`に成功
5. Application/Installer identity、binary 5件、最終pkg、公証submission IDを含む検証結果を保存
6. signed modeではunsigned artifactが混入していないことを確認
7. 一時的なunsigned例外では、OSごとに必須署名secretがすべて未設定であることを確認。同じOSのsecretが一部だけ設定された状態ではリリースしない
8. unsigned成果物では、該当asset名の`-unsigned`、リリースノートのGatekeeper / Microsoft Defender SmartScreen警告、Issue #31への参照を確認

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
6. Windows/macOS artifactに`id-mcp`、InDesign Startup Script、専用installerが含まれ、macOS pkgがapplication側の`Scripts/Startup Scripts`へ固定bridgeを配置することを確認
7. Windows/macOS installerがCodex設定の不足MCP tableだけを追加し、既存tableを保持し、再実行しても重複しないことを確認
8. 専用InDesign installerのinstall/remove dry-runを確認し、固定bridge file以外を削除しないことを確認
9. unsigned modeでは、該当OSの全asset名が`-unsigned`で識別でき、リリースノートが未署名配布とOS警告を明記していることを確認

## 5. リリース後

1. 初期ユーザーの導入可否確認
2. 重大不具合（P1）監視
3. Hotfix要否判断
4. Issue #31でDeveloper ID / Authenticode署名への移行状況を追跡
