# 署名・公証・タグリリース運用ガイド（Stage 6）

- 最終更新: 2026-07-16
- 対象: `v*` タグでの RC / GA リリース運用

## 1. 概要

Stage 6 では、配布物の信頼性向上のため以下を実施します。

1. Windows: Authenticode 署名
2. macOS: Developer ID署名 + Notarization + staple
3. RC / GAタグでのCI自動化

関連ワークフロー:

- `.github/workflows/rc-release.yml`

## 2. Windows 署名

## 2.1 ローカル実行

```powershell
.\scripts\sign-windows.ps1 -ArtifactDir .\dist\windows -PfxPath <path-to-pfx> -PfxPassword <password>
```

必要条件:

1. `signtool.exe` が利用可能
2. PFX証明書とパスワード

## 2.2 CIシークレット

1. `WIN_SIGN_PFX_BASE64` (PFXをbase64化した文字列)
2. `WIN_SIGN_PFX_PASSWORD`
3. 任意: `WIN_SIGN_TIMESTAMP_URL`

## 3. macOS 署名・公証

macOS release経路は、署名対象と証明書種別を次の順序で固定する。

1. `aarch64-apple-darwin`と`x86_64-apple-darwin`を結合した5つのuniversal2 binaryを作る
2. 5 binaryをそれぞれDeveloper ID Applicationでhardened runtime・timestamp付き署名し、`codesign --verify --strict`で検証する
3. 署名済みbinaryからtar archiveとcomponent pkgを作る
4. `productbuild`でDeveloper ID Installer署名・timestamp付きの最終pkgを作り、`pkgutil --check-signature`で検証する
5. 最終pkgだけを`notarytool submit --wait`へ送り、ticketをstapleして`stapler validate`する

Developer ID ApplicationとDeveloper ID Installerは用途が異なる。1つのidentityを兼用せず、完全なidentity名を別々に指定する。

## 3.1 ローカル実行

```bash
MACOS_SIGNING_MODE=release \
MAC_APPLICATION_IDENTITY="Developer ID Application: Example Org (TEAMID)" \
MAC_INSTALLER_IDENTITY="Developer ID Installer: Example Org (TEAMID)" \
REQUIRE_PKG=true \
./scripts/package-macos.sh ./dist/macos

APPLE_ID="<apple-id>" \
APPLE_TEAM_ID="<team-id>" \
APPLE_APP_SPECIFIC_PASSWORD="<app-password>" \
MAC_INSTALLER_IDENTITY="Developer ID Installer: Example Org (TEAMID)" \
./scripts/notarize-macos.sh ./dist/macos
```

必要条件:

1. Xcode Command Line Toolsの`codesign`、`pkgbuild`、`productbuild`、`pkgutil`、`xcrun`
2. keychain内のDeveloper ID Application identity
3. keychain内のDeveloper ID Installer identity
4. Apple notarization用資格情報

証明書がない開発環境では、次のようにunsigned経路を明示する。これはuniversal2生成とpkg payload検証に利用できる。通常の一般配布に使用する最終形態ではないが、後述の一時的なunsignedタグリリース例外では警告と識別子を付けて公開できる。

```bash
MACOS_SIGNING_MODE=unsigned REQUIRE_PKG=true \
  ./scripts/package-macos.sh ./dist/macos
```

## 3.2 CIシークレット

必須（notarization実行時）:

1. `MAC_APPLICATION_IDENTITY`
2. `MAC_INSTALLER_IDENTITY`
3. `APPLE_ID`
4. `APPLE_TEAM_ID`
5. `APPLE_APP_SPECIFIC_PASSWORD`

macOS signed releaseで必須:

1. `MAC_CERT_P12_BASE64`
2. `MAC_CERT_PASSWORD`

任意:

1. `MAC_KEYCHAIN_PASSWORD`。未指定時はworkflow内の固定された一時値を使う

`MAC_CERT_P12_BASE64`は、上記2つのidentityと秘密鍵をCI keychainへimportできるPKCS #12 bundleとする。

## 4. タグリリースの署名モード

`RC Release` workflowは、タグ実行時にWindowsとmacOSの署名モードをOSごとに判定する。`WIN_SIGN_TIMESTAMP_URL`と`MAC_KEYCHAIN_PASSWORD`は任意値のため、状態判定には含めない。

Windowsは`WIN_SIGN_PFX_BASE64`と`WIN_SIGN_PFX_PASSWORD`の2 secretを次のように扱う。

1. **signed**: 2 secretがすべて設定済み。Authenticode署名を実行する。
2. **unsigned**: 2 secretがすべて未設定。一時的な例外としてunsigned成果物を生成する。
3. **invalid**: 1 secretだけが設定済み。ビルド前に失敗させる。

macOSは`MAC_CERT_P12_BASE64`、`MAC_CERT_PASSWORD`、2つのidentity、3つの公証資格情報の合計7 secretを同じ原則で扱う。

1. **release**: 7 secretがすべて設定済み。Developer ID署名と公証を実行する。
2. **unsigned**: 7 secretがすべて未設定。一時的な例外としてunsigned成果物を生成する。
3. **invalid**: 1〜6 secretだけが設定済み。ビルド前に失敗させる。

それぞれのOSでsecretグループが不完全な場合のみ無効とする。そのため、一方のOSで必要なsecretがすべて設定済みで、もう一方のOSですべて未設定の場合は、署名済み成果物と`-unsigned`成果物が同じリリースに含まれる。

unsigned modeと判定されたOSでは、公開するすべての成果物名に`-unsigned`を付ける。例:

- `adobe-mcp-rs-windows-x86_64-unsigned.zip`
- `adobe-mcp-rs-windows-x86_64-unsigned.msi`
- `adobe-mcp-rs-macos-universal-unsigned.tar.gz`
- `adobe-mcp-rs-macos-universal-unsigned.pkg`

リリースノートにもunsigned配布であることを明記し、macOSではGatekeeper、WindowsではMicrosoft Defender SmartScreenの警告や実行ブロックが発生し得ることを利用者へ案内する。この例外はv0.5.1の早期公開に適用し、Developer ID / Authenticodeによる通常の署名配布への移行はIssue #31で追跡する。unsigned成果物は、一般配布の最終状態ではない。

## 5. RC / GA リリース手順

1. RCでは`vX.Y.Z-rcN`、GAでは`vX.Y.Z`タグを作成してpush
2. `RC Release` workflow を確認
3. 生成物（Windows/macOS）をダウンロード
4. signed modeでは署名/公証が完了したことを確認
5. いずれかのOSがunsigned modeの場合は、該当OSの成果物名にある`-unsigned`、リリースノートの警告、Issue #31への参照を確認

## 6. 注意事項

1. `notarize-macos.sh`はbinaryやpkgを署名しない。署名済み最終pkgの公証、staple、validate、`spctl --assess --type install`のみを行う。
2. 実identityを使う前に`security find-identity -v`でApplication/Installerの両identityを確認する。
3. 公証submission出力はartifact directoryの`notarytool-submit.json`へ保存される。失敗時はそこにあるsubmission IDを`xcrun notarytool log`へ渡して詳細を取得する。
4. 本番リリース前に5 binaryのAuthority、pkgのDeveloper ID Installer certificate、`xcrun stapler validate`、`spctl --assess --type install`結果を保存する。
5. macOS証明書運用は組織ポリシーに合わせてkeychainの扱いを固定化する。
