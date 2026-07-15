# インストーラ E2E 手順（Stage 5）

- 最終更新: 2026-07-15
- 対象: Rust版 `ae-mcp` / `pr-mcp` / `ps-mcp` / `ai-mcp` / `id-mcp` の Windows/macOS インストーラ検証

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
2. Windows MSI の場合、Custom Setup 画面で5 hostのbridge featureを選択できる
3. Windows MSI の場合、インストール中に別の PowerShell ウィンドウが表示されない
4. Windows MSI の場合、`C:\ProgramData\AfterEffectsMcp\install-report.json` で host integration の結果一覧を確認できる
5. Windows ZIP/MSIに`id-mcp.exe`、`mcp-bridge-indesign.idjs`、`install-indesign-bridge.ps1`が収録される
6. macOS archive/pkgに`id-mcp`と`indesign/mcp-bridge-indesign.idjs`、専用installerが収録され、pkgが各`/Applications/Adobe InDesign YYYY/Scripts/Startup Scripts`へ固定bridgeを配置する
7. Windows/macOSとも、対象ユーザーのCodex設定に未登録のMCP serverだけを追加し、既存の同名tableを変更しない。設定ファイルがなければ作成する
8. 5 host の MCP バイナリが所定の場所へ配置される
9. 5 binary の `--help` が実行できる
10. Windows help は `autostart` を含み `service` を含まない。macOS help は `service` を含み `autostart` を含まない

## 3.2 Windows autostart / macOS service

各 host binary で次を確認する。

1. Windows: 初期 `autostart status` は `not installed` を返す
2. Windows: `autostart install` 後、現在ユーザーの Run key が現在の絶対 exe パスと `serve-daemon` を保持する
3. Windows: `autostart start` を2回実行し、2回目は新規 process を作らず `already running` を返す
4. Windows: `autostart stop` 後に `not running`、`autostart uninstall` 後に `not installed` になる
5. Windows: stale PID は除去され、別 exe の生存 PID は勝手に除去・上書きされず `start` が失敗する
6. macOS: `service install`、`start`、`status`、`stop`、`uninstall` を順に実行する

## 3.3 Windows MSI の install / upgrade / uninstall

2026-07-15のWindows実機では、WiX 5.0.2で0.4.4 ZIP/MSIを生成し、MSI Property/File/Feature tableとZIP entryを検査した。`ProductVersion=0.4.4.0`、全bridge featureのdefault `Level=1`、`id-mcp.exe`、`mcp-bridge-indesign.idjs`、`install-indesign-bridge.ps1`の収録は確認済み。別hostの実機検証と競合させないため、install/upgrade/uninstall matrix自体はmain統合後に実施する。

1. 初回 install では5 hostとも Run key が新規作成されない
2. 任意の host で `autostart install` し、MSI upgrade 後も opt-in が維持され、登録値が新しいインストール先を指す
3. MSI upgrade 中の旧製品削除では Run key が一時削除されない
4. `InDesignMcp`も初回installで暗黙登録されず、opt-in済みRun keyだけがupgradeで修復される
5. 通常 uninstall では、アンインストールを実行した現在ユーザーについて登録済み daemon が停止し、5 host の既知 Run key が削除される
6. installer log に repair / stop / remove の結果が記録される

## 3.4 InDesign Startup Script

1. 専用installerのdry-runが検出済みprofileまたは明示した`Scripts/Startup Scripts`だけを列挙する
2. install後に固定名`mcp-bridge-indesign.idjs`だけが追加・更新される
3. `-Remove` / `--remove`のdry-run後に削除を実行し、固定ファイルだけが消えて他のscriptと親directoryが残る
4. Windows generic installerは現在ユーザーの既存profileだけへ配置し、未検出時はskipをreportする
5. macOS pkgのroot postinstallは`~/Library/Preferences`を推測せず、検出した`/Applications/Adobe InDesign YYYY/Scripts/Startup Scripts`へ固定bridgeだけを配置する
6. InDesign実機での起動・heartbeat・MCP commandは別途manual E2E gateを通す。package生成成功だけで実機対応済みとはしない

## 3.5 Codex MCP設定

1. Codex設定がない状態でinstallし、対象ユーザー所有の`config.toml`が作成される
2. `mcp_servers.aftereffects`または`mcp_servers.indesign`の一方だけが既存の場合、既存tableがbyte単位で保持され、不足tableだけが追加される
3. 両tableが既存の場合、install前後で設定ファイルに差分がない
4. installerを再実行しても重複tableが増えない
5. Windowsでは現在ユーザー以外の`config.toml`を変更しない
6. macOS pkgでは有効なconsole userを特定できない場合、rootの設定を作らずskipを記録する

## 3.6 MCP + AE ブリッジ

1. MSI/pkgで導入した場合、runtimeが`ScriptUI Panels`、bootstrapが`Scripts/Startup`、cleanupが`Scripts/Shutdown`へ自動配置されることを確認
2. ポータブル版（zip/tar.gz）の場合は3つのJSXを手動配置
3. AEを再起動し、panelを開かずに公開Tool `list-ae-instances` を実行する
4. `bridgeRuntime: extendscript-startup`、`lifecycleMode: startup-headless`、heartbeat更新を確認
5. 公開Tool `run-bridge-test` を実行し、daemon broker経由の結果JSONを取得
6. daemon先行/AE先行、workspace reset、bootstrap再評価、stop/restartを [ADR 0002](adr/0002-after-effects-headless-startup-bridge.md) のmatrixで確認

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
