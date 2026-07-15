# インストーラ E2E 手順（Stage 5）

- 最終更新: 2026-07-15
- 対象: Rust版 `ae-mcp` / `pr-mcp` / `ps-mcp` / `ai-mcp` の Windows/macOS インストーラ検証

## 1. 目的

1. クリーン環境で導入から起動確認までを再現する
2. インストーラ導入後に Windows は `autostart`、macOS は launchd `service` が動作することを確認する
3. AEブリッジとMCPの最小往復が成立することを確認する

## 2. 生成物

## 2.1 Windows

- `adobe-mcp-rs-windows-x86_64.zip`
- `adobe-mcp-rs-windows-x86_64.msi`

生成コマンド:

```powershell
.\scripts\package-windows.ps1 -OutputDir .\dist\windows -RequireMsi
```

## 2.2 macOS

- `adobe-mcp-rs-macos-universal.tar.gz`
- `adobe-mcp-rs-macos-universal.pkg`

生成コマンド:

```bash
REQUIRE_PKG=true ./scripts/package-macos.sh ./dist/macos
```

## 3. E2E 検証チェックリスト

## 3.1 インストール

1. インストーラ実行（MSI/pkg）
2. Windows MSI の場合、Custom Setup 画面で host bridge feature を選択できる
3. Windows MSI の場合、インストール中に別の PowerShell ウィンドウが表示されない
4. Windows MSI の場合、`C:\ProgramData\AfterEffectsMcp\install-report.json` で host integration の結果一覧を確認できる
5. 4 host の MCP バイナリが所定の場所へ配置される
6. 4 binary の `--help` が実行できる
7. Windows help は `autostart` を含み `service` を含まない。macOS help は `service` を含み `autostart` を含まない

## 3.2 Windows autostart / macOS service

各 host binary で次を確認する。

1. Windows: 初期 `autostart status` は `not installed` を返す
2. Windows: `autostart install` 後、現在ユーザーの Run key が現在の絶対 exe パスと `serve-daemon` を保持する
3. Windows: `autostart start` を2回実行し、2回目は新規 process を作らず `already running` を返す
4. Windows: `autostart stop` 後に `not running`、`autostart uninstall` 後に `not installed` になる
5. Windows: stale PID は除去され、別 exe の生存 PID は勝手に除去・上書きされず `start` が失敗する
6. macOS: `service install`、`start`、`status`、`stop`、`uninstall` を順に実行する

## 3.3 Windows MSI の install / upgrade / uninstall

1. 初回 install では4 hostとも Run key が新規作成されない
2. 任意の host で `autostart install` し、MSI upgrade 後も opt-in が維持され、登録値が新しいインストール先を指す
3. MSI upgrade 中の旧製品削除では Run key が一時削除されない
4. 通常 uninstall では、アンインストールを実行した現在ユーザーについて登録済み daemon が停止し、4 host の既知 Run key が削除される
5. installer log に repair / stop / remove の結果が記録される

## 3.4 MCP + AE ブリッジ

1. MSI/pkg で導入した場合、`mcp-bridge-auto.jsx` が検出済み AE に自動配置されることを確認
2. ポータブル版（zip/tar.gz）の場合は `mcp-bridge-auto.jsx` を手動配置
3. AEで `Window > mcp-bridge-auto.jsx` を開く
4. `Auto-run commands` をON
5. Codexで公開Tool `list-ae-instances` を実行し、対象instanceを確認
6. 公開Tool `run-bridge-test` を実行し、daemon broker経由の結果JSONを取得

## 4. 失敗時の確認ポイント

1. Windowsで daemon が起動しない:
   - `ae-mcp autostart status` を確認
   - 必要なら `ae-mcp autostart install` と `ae-mcp autostart start` を再実行
2. macOSでpkg生成失敗:
   - `pkgbuild --version` を確認
3. AE結果が返らない:
   - `~/Documents/ae-mcp-bridge/ae_command.json` の `status` を確認
4. Windows MSI で host integration の実行結果が確認できない:
   - `C:\ProgramData\AfterEffectsMcp\install-bridge-installer.log` を確認
   - `C:\ProgramData\AfterEffectsMcp\install-report.json` を確認

## 5. CI

- GitHub Actions: `.github/workflows/installer-build.yml`
- 実行方法:
1. `workflow_dispatch` で手動実行
2. `v*` タグPushで自動実行
