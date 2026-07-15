# インストーラ E2E 手順（Stage 5）

- 最終更新: 2026-06-26
- 対象: Rust版 `ae-mcp` の Windows/macOS インストーラ検証

## 1. 目的

1. クリーン環境で導入から起動確認までを再現する
2. インストーラ導入後に `service` サブコマンドが動作することを確認する
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
5. `ae-mcp` バイナリが所定の場所へ配置される
6. `ae-mcp --help` が実行できる

## 3.2 Windows autostart / macOS service

1. Windows: `autostart install`
2. Windows: `autostart start`
3. Windows: `autostart status`
4. Windows: `autostart stop`
5. Windows: `autostart uninstall`
6. macOS: `service install`
7. macOS: `service start`
8. macOS: `service status`
9. macOS: `service stop`
10. macOS: `service uninstall`

## 3.3 MCP + AE ブリッジ

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
