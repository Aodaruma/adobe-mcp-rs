# after-effects-mcp-rs Rust移行仕様書（v0.1）

- 作成日: 2026-03-23
- 対象リポジトリ: `after-effects-mcp-rs`
- 文書目的: TypeScript実装のAfter Effects MCP ServerをRustへ移行し、Windows/macOS向けバイナリ配布とインストーラ経由のデーモン登録までを仕様化する。

## 1. 背景と課題

- 現状実装はNode.js/TypeScriptベースであり、After Effects連携は`ae_command.json`/`ae_mcp_result.json`によるファイルブリッジ方式。
- リポジトリ名はRust移行を前提としているが、現時点ではRust実装が未着手。
- 目標は以下の3点:
1. MCPサーバー本体のRust移行
2. Win/mac向け単体バイナリ配布
3. インストーラからのサービス（デーモン）登録

## 2. プロダクト目標

## 2.1 必達目標（Must）

1. 既存MCP機能の実行互換（実用上同等）
2. Windows/macOSでのネイティブバイナリ提供
3. インストーラ実行でサービス登録・起動・停止・アンインストールが可能
4. 障害時のログ/状態確認手段を備える

## 2.2 品質目標（Should）

1. 単一バイナリ配布（依存ランタイム最小化）
2. コアロジックのクロスプラットフォーム共通化
3. 後方互換を意識したAPI移行

## 3. スコープ

- Rust製MCPサーバー実装
- 既存ブリッジ（AE ScriptUI Panel + JSONファイル）との互換維持
- サービス実行モード（daemon/service）
- インストーラ（Windows/macOS）
- CI/CDでのマルチOSビルドとアーティファクト配布

## 4. 非スコープ（初期）

- Linux向け配布
- After Effects側UI/UXの大幅改修
- SaaS型のクラウド中継
- 認証付きリモート公開（インターネット公開前提）

## 5. 現行互換要件

## 5.1 MCP公開要素の互換

現行の公開契約は [After Effects MCP public surface](after-effects-mcp-surface.md) に記載した9 Tool、1 Resource、6 Promptとする。旧TS実装の名前は削除せず、`tools/list`へ出さない非公開互換dispatchとして受理し、公開置換先を返す。

- Resources:
1. `compositions` (`aftereffects://compositions`)

- Prompts:
1. `list-compositions`
2. `analyze-composition`
3. `create-composition`
4. `save-preview-png`
5. `render-queue-setup`
6. `cleanup-preview-folder`

- 公開Tools: `run-jsx`, `run-jsx-file`, `get-jsx-result`, `list-ae-instances`, `get-results`, `get-help`, `save-frame-png`, `cleanup-preview-folder`, `run-bridge-test`
- 旧TS Tool名を含む非公開互換dispatchの完全な一覧は公開surface文書を参照

## 5.2 ファイルブリッジ契約の互換

- コマンドファイル: `~/Documents/ae-mcp-bridge/ae_command.json`
- 結果ファイル: `~/Documents/ae-mcp-bridge/ae_mcp_result.json`

- `ae_command.json`必須項目:
1. `command` (string)
2. `args` (object)
3. `timestamp` (ISO8601 string)
4. `status` (`pending` | `running` | `completed` | `error`)

- `ae_mcp_result.json`推奨項目:
1. `status` (`success` | `error` | `waiting` など)
2. `message`
3. `_responseTimestamp`
4. `_commandExecuted`

## 6. 要件定義

## 6.1 機能要件

1. MCPサーバーとして`stdio`モードで起動できること（既存クライアント互換）。
2. デーモンモードで常駐実行できること（OSサービス登録対象）。
3. 既存TS版の主要ツール群をRustで同等実装すること。
4. コマンド投入・結果取得・タイムアウト・再試行・鮮度判定を実装すること。
5. `--health`等のヘルスチェックCLIを提供すること。
6. `install`/`uninstall`/`start`/`stop`系のサービス管理CLIを提供すること。

## 6.2 非機能要件

1. 安定性:
   - ブリッジ未起動時にクラッシュせず、説明可能なエラーを返す。
2. 性能:
   - 通常コマンド（既存の軽量操作）で体感遅延がTS版以下。
3. 可観測性:
   - 構造化ログ（JSONまたはテキスト）をOS別既定パスへ出力。
4. 保守性:
   - コア処理・MCP公開層・OSサービス層を分離。
5. セキュリティ:
   - 任意スクリプト実行を無制限に許可しない（allowlist維持）。

## 7. アーキテクチャ方針

## 7.1 コンポーネント

1. `ae-mcp`（CLI/Binary）
   - サブコマンド:
     - `serve-stdio`
     - `serve-daemon`
     - Windows: `autostart install|uninstall|start|stop|status`
     - macOS: `service install|uninstall|start|stop|status`
     - `bridge health|tail|clear`
2. `mcp-core`
   - MCPリソース/ツール/プロンプト定義
3. `bridge-core`
   - JSONファイル読み書き、ポーリング、タイムアウト、結果検証
4. `platform-service`
   - Windows current-user autostart / PID 管理と macOS launchd の抽象化

## 7.2 実行モード

1. `stdio`モード:
   - MCPクライアントが都度プロセス起動する互換モード。
2. `daemon`モード:
   - Windows はユーザー別 autostart、macOS は launchd で常駐し、ローカルIPCまたはHTTP/SSEで受け付ける拡張モード。

注: 初期リリースでは`stdio`互換を優先し、daemon公開インターフェースは内部APIとして開始してよい。

## 8. インストーラ仕様

## 8.1 Windows

1. インストーラが`ae-mcp`バイナリを配置。
2. Windows Service は実装せず、現在ユーザーの Run key と PID ファイルで管理する。
3. 初回インストールでは autostart を暗黙に有効化せず、既存 opt-in 登録だけを upgrade 時に修復する。
4. 通常アンインストール時に登録済み daemon の停止と Run key 削除を実施。

## 8.2 macOS

1. インストーラが`ae-mcp`を配置（例: `/usr/local/bin`または`/Library/Application Support/...`）。
2. `launchd`用plistを配置して常駐登録。
3. ユーザー単位（LaunchAgent）とシステム単位（LaunchDaemon）のどちらかを初期方針で固定する。
   - 推奨: まずはユーザー単位を先行実装（権限要件が低い）。
4. アンインストール時に`launchctl`解除とplist削除を実施。

## 8.3 署名と配布

1. Windows: コード署名を推奨（SmartScreen考慮）。
2. macOS: Developer ID署名およびNotarizationをリリース要件化。

## 9. 技術選定（採用候補）

## 9.1 言語・基盤

- Rust stable
- 非同期ランタイム: `tokio`
- シリアライズ: `serde` / `serde_json`
- CLI: `clap`
- ログ: `tracing` / `tracing-subscriber`
- エラー: `thiserror` / `anyhow`

## 9.2 MCP実装

- 第一候補: 公式Rust SDK（`modelcontextprotocol/rust-sdk`, crate: `rmcp`）
- 方針:
1. プロトコル更新が速いため、導入時点のバージョンを固定
2. MCP仕様の日付版を明示して互換管理

## 9.3 常駐管理

- Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` と PID / exe パス照合
- macOS: ユーザー単位 launchd LaunchAgent

## 9.4 配布/インストーラ

- 第一候補: `cargo-dist`（複数OSの配布物/インストーラ生成をCI連携）
- Windows MSI詳細設定: `cargo-wix`系設定（`cargo-dist`経由含む）

## 10. 設定・データパス仕様（初版）

- 設定ファイル（案）:
  - Windows: `%ProgramData%/after-effects-mcp/config.toml`
  - macOS: `/Library/Application Support/after-effects-mcp/config.toml`
- ログ（案）:
  - Windows: `%ProgramData%/after-effects-mcp/logs/`
  - macOS: `/Library/Logs/after-effects-mcp/`
- ブリッジファイル:
  - 既存互換として`~/Documents/ae-mcp-bridge/`を維持

## 11. エラーハンドリング方針

1. ブリッジ未起動:
   - `status=error` と「AEパネル未起動」を返却
2. stale結果:
   - 最終更新時刻を含む警告を返却
3. タイムアウト:
   - コマンド名・待機時間を含むエラー
4. 不明コマンド:
   - allowlistの候補を含むエラー

## 12. テスト戦略

1. Unit:
   - JSON契約、タイムアウト、状態遷移
2. Integration:
   - 模擬ブリッジ（fake writer/reader）との往復
3. E2E:
   - AE実機で主要ツールの最小成功ケース
4. Installer:
   - Win/macでインストール、起動、再起動、アンインストール

## 13. 受け入れ基準（Definition of Done）

1. 既存主要MCP操作の互換テストが緑
2. Windows/macOSのリリースバイナリがCIで生成される
3. インストーラでサービス登録/解除が成功する
4. 障害時ログから原因追跡できる
5. READMEに導入/運用手順が反映される

## 14. リスクと対策

1. MCP仕様更新による追従コスト
   - 対策: 仕様バージョン固定 + 定期更新ウィンドウ設定
2. AE側スクリプト環境差異（バージョン差）
   - 対策: AE 2022/2024/2026で最小回帰テスト
3. OSサービス権限差
   - 対策: ユーザー単位運用を先行し、管理者モードは段階導入
4. macOS配布の署名・公証運用負荷
   - 対策: CIジョブに署名/公証パイプラインを明文化

## 15. 参考リンク（2026-03-23確認）

- MCP Versioning: <https://modelcontextprotocol.io/specification/versioning>
- MCP Specification (2025-06-18): <https://modelcontextprotocol.io/specification/2025-06-18/index>
- Official Rust MCP SDK (`rmcp`): <https://github.com/modelcontextprotocol/rust-sdk>
- `service-manager` crate: <https://docs.rs/service-manager/latest/service_manager/>
- `windows-service` crate: <https://docs.rs/windows-service/latest/windows_service/>
- cargo-dist docs: <https://axodotdev.github.io/cargo-dist/>
- cargo-dist MSI guide: <https://axodotdev.github.io/cargo-dist/book/installers/msi.html>
- Apple launchd作成ガイド: <https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html>
- Apple notarization（カスタムフロー）: <https://developer.apple.com/documentation/security/customizing-the-notarization-workflow>
- Microsoft `sc.exe create`: <https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/sc-create>
