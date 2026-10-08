# Adobe MCP Beta — Windows x64 / 0.5.3 beta 1

この配布物は開発ブランチのベータ版です。公開MCPツールは現在の構成を維持しています。
Adobe純正製品ではありません。After Effectsが主な検証対象で、他の4アプリはExperimentalです。

## インストール・旧版からの更新

1. Adobe側の実行中の処理が完了してから、Adobeアプリを保存して終了してください。
2. 通知領域のAdobe MCPを右クリックし、Quit Adobe MCPで終了してください。
   旧daemonを使っている場合は、各CLIの `autostart stop` で停止してください。
3. `adobe-mcp-rs-windows-x86_64.msi` を実行し、Windowsの管理者確認に応じてインストールします。
4. セットアップ結果がメモ帳に開きます。失敗や保留があれば内容を確認し、
   スタートメニューの「Adobe MCP - Repair setup」でユーザー設定を再実行できます。
5. AdobeアプリとCodexを再起動してください。通知領域の「Mc」から接続状況を確認できます。

旧MSI製品（Adobe MCP (Rust)、同一UpgradeCode）は新MSIに置き換わります。
互換性のため、既定の配置先は `C:\Program Files\AfterEffectsMcp` です。
手動導入された `%LOCALAPPDATA%\Programs\AfterEffectsMcp` / `AdobeMcp` の既知の実行ファイルは、
ユーザー設定が更新された後にバックアップへ退避します。その他のファイルは残します。
旧ログイン起動は統合アプリに移行します。新規導入時のログイン起動はメニューから選択できます。

Codexの既存5サーバーのcommandが既知の旧配置先を指していれば新配置先に更新します。
承認ポリシー・ツール権限・引数・それ以外のサーバー設定は保持します。
独自の配置先やラッパーを使った設定は自動変更しません。必要なら各commandを更新してください。
不足している5サーバーの設定は追加します。

ユーザー設定と旧実行ファイルのバックアップ:
`Documents\adobe-mcp\upgrade-backup-日時`（restore-map.jsonlで元の配置先を確認できます）
Adobeのmachine bridgeのバックアップ:
`C:\ProgramData\AfterEffectsMcp\bridge-backups`
バックアップには個人の設定が含まれるため、他のテスターには配布しないでください。

## Adobe側のbridge

- Photoshop: UXP、23.3以上。Premiere Pro: UXP、25.6以上。
  Creative Cloud付属のUPIAで既存pluginを更新します。自動導入できない場合は、
  配置先のphotoshop-mcp-bridge.ccx / premiere-mcp-bridge.ccxを開いて導入してください。
  対応バージョンではAdobe起動時に接続を受け付けます。ボタン操作は不要です。
- Premiereの旧CEPパネルはUXP導入成功後に移行案内だけのパネルへ置き換え、二重受付を止めます。
- Illustrator: CEP、24.0以上。現在のユーザーにbridgeを配置し、CEP debug設定を有効にします。
- After Effects: 起動・終了スクリプトとruntimeを配置します。スクリプトのファイル／ネットワークアクセスを有効にしてください。
  同じバージョンで個別変更されたruntimeは保護します。保存先はセットアップ記録に出力されます。
- InDesign: UXP Startup Script、18.5以上の実験対応。既存のユーザープロファイルに配置します。
  プロファイルがない場合はInDesignを一度起動して終了し、Repair setupを実行してください。

## アイコンと動作

カラーのMcは、有効なサーバーすべてが受付可能であることを示します。
Adobeが未起動で接続待ちの場合もカラーです。Adobe内の処理成功を保証する表示ではありません。
全サーバー停止、起動失敗、終了待ちはグレーです。詳しい接続状況は各Adobeのサブメニューにあります。
個別の停止と終了は処理完了を待ちます。処理が残っている場合に強制終了しないでください。

## ベータ版の制約・報告

- Windows x64向けです。macOS用インストーラーはこの配布物に含みません。
- MSI・exeは未署名です。Windowsの署名確認で発行元が不明と表示されます。
- 検証PCのPhotoshop 26.11.7で終了時クラッシュが再現しています。bridgeとの因果関係は未確定です。
  Photoshopの成果物は終了前に保存し、この症状もベータ検証対象としてください。
- Adobeの各バージョン、スリープ復帰、長時間連続稼働は網羅的には確認できていません。
- Adobe MCPのアンインストールはWindowsの「インストールされているアプリ」から行えます。
  Adobe内のplugin/Startup scripts、Codex設定、バックアップは作品・設定保護のため残ります。
  UXP pluginの削除はCreative Cloudから行ってください。

不具合報告には、Windows/Adobeのバージョン、実行した操作、期待結果、実際の結果を添えてください。
トレイのOpen diagnosticsで診断を保存できます。ログや診断は共有前に作品名・パスなどを確認してください。
ユーザー側の導入記録: `Documents\adobe-mcp\installer\install-report.json`
machine側の導入記録: `C:\ProgramData\AfterEffectsMcp\install-report.json`

## ZIP版

ZIPは展開したフォルダーを保持して使う開発者向けの配布物です。通常はMSIを使ってください。
ZIPを展開しただけでは旧MSIの更新やAdobe側への配置は行われません。
