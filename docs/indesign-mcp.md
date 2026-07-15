# InDesign MCP PoC

## Status

InDesign support is **Experimental / Windows live-verified**. Windows 11、InDesign 2026 21.4.1.4（`Version 21.0-J/ja_JP`）では、Startup Scriptのcold start、daemon discovery、raw UXP、retained result、daemon再接続を確認済みです。macOS、複数version、sleep/modal、undo、normal shutdown cleanupは未完了です。

The minimum target is InDesign 18.5. InDesign supports UXP scripts from 18.0, while Adobe's current filesystem recipe calls out 18.5 or later. The repository bridge uses a UXP Startup Script (.idjs) rather than a panel and starts without an Auto-run checkbox on the verified Windows host.

## Architecture

~~~text
MCP client
  -> id-mcp serve-stdio
  -> id-mcp serve-daemon (127.0.0.1:47659)
  -> ~/Documents/id-mcp-bridge/instances/<instanceId>/
  -> mcp-bridge-indesign.idjs
  -> InDesign UXP DOM
~~~

mcp-core::INDESIGN_HOST, daemon-core, the retained request registry, script-file policy, and the cross-host contract fixture are shared with the other Adobe hosts. The startup script serializes writes and publishes protocol-v1 heartbeat, command, and result records by writing a same-directory temporary file and calling UXP's callback-based `fs.rename`. InDesign 2026 does not expose `mkdirSync`, `statSync`, `openSync`, `closeSync`, or `renameSync`.

## Why a Startup Script

Adobe documents that:

- InDesign 18.0 and later run .idjs UXP scripts: [UXP scripts](https://developer.adobe.com/indesign/uxp/scripts/).
- a script inside Scripts/Startup Scripts runs when InDesign starts: [Tips and tricks for UXP scripts](https://developer.adobe.com/indesign/uxp/scripts/tutorials/tips-tricks/).
- a long-running event script may keep its top-level Promise pending for the session: [InDesign events](https://developer.adobe.com/indesign/uxp/resources/recipes/indesign-events/).
- UXP scripts receive fixed localFileSystem: fullAccess and network permissions, while allowCodeGenerationFromStrings is false: [UXP manifest permissions for scripts](https://developer.adobe.com/indesign/uxp/resources/fundamentals/manifest/).
- Application.doScript accepts a File, String, or JavaScript Function and a ScriptLanguage: [Application API](https://developer.adobe.com/indesign/uxp/dom/api/a/application/).

Therefore run-script does **not** call eval or the Function constructor. It sends code to InDesign's documented app.doScript(..., ScriptLanguage.UXPSCRIPT, ...) host API. String input and `script.setResult` succeeded on InDesign 2026 21.4.1.4; other supported versions and macOS remain release gates.

## MCP surface

| Name | Purpose |
|---|---|
| run-script | Submit a synchronous UXP function body through app.doScript; requires mode: "unsafe" and description |
| run-script-file | Validate a local function-body source file in Rust, then submit its contents; supports unsafe and exact path/SHA-256 trusted modes |
| run-template | Run ping, app/document/page/story read templates |
| get-script-result | Read a retained result by requestId |
| get-results | Read a retained result by ID or the latest retained result |
| get-help | Return InDesign-specific setup and safety notes |
| list-indesign-instances | List protocol-v1 Startup Script instances |
| run-bridge-test | Execute ping through the daemon and file bridge |

The indesign://documents resource executes the read-only listDocuments template. Raw code receives a single args object and must synchronously return a JSON-serializable value. `run-script-file` does **not** execute a general top-level `.idjs` program: the file contents are inserted into the same function body as inline code. Therefore top-level `await`, module-style top-level control flow, and calling `require("uxp").script.setResult` from submitted code are unsupported. The bridge owns `script.setResult` after the function returns:

~~~javascript
const { app } = require("indesign");
return {
  version: app.version,
  documentCount: app.documents.length,
  requestedName: args.name || null,
};
~~~

unsafe is an acknowledgement, not a sandbox. UXP scripts have broad host, filesystem, and network authority. The bridge does not claim that keyword scanning can reliably classify deletion or other destructive behavior.

## Install

Build:

~~~powershell
cargo build --release -p id-mcp
~~~

The repository installer copies the Startup Script into existing InDesign preference profiles:

~~~powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-indesign-bridge.ps1
~~~

~~~bash
bash ./scripts/install-indesign-bridge.sh
~~~

Preview destinations before installation, or remove only the fixed bridge filename from detected/explicit Startup Scripts directories:

~~~powershell
.\scripts\install-indesign-bridge.ps1 -DryRun
.\scripts\install-indesign-bridge.ps1 -Remove -DryRun
.\scripts\install-indesign-bridge.ps1 -Remove
~~~

~~~bash
bash ./scripts/install-indesign-bridge.sh --dry-run
bash ./scripts/install-indesign-bridge.sh --remove --dry-run
bash ./scripts/install-indesign-bridge.sh --remove
~~~

Both dedicated installers accept only a directory ending in `Scripts/Startup Scripts` and only install or remove `mcp-bridge-indesign.idjs`; they never remove the directory or unrelated scripts. Pass `-Destination` / `--destination` when auto-detection cannot identify the verified profile.

Windows release ZIP/MSI artifacts include `id-mcp.exe`, the Startup Script, and `install-indesign-bridge.ps1`. The generic MSI helper creates the current-user Codex config when needed, adds only missing MCP server tables, and repairs an already opted-in `InDesignMcp` current-user autostart entry. Existing same-name MCP tables are left unchanged. It copies the Startup Script only into current-user InDesign preference profiles that actually exist.

macOS release archives/pkg include `id-mcp` and an InDesign bundle under `/usr/local/share/ae-mcp/indesign`. The pkg postinstall copies the fixed bridge into each detected `/Applications/Adobe InDesign YYYY/Scripts/Startup Scripts` directory without guessing a user preference profile. It also adds missing MCP server tables to the active console user's Codex config while preserving existing same-name tables. The bundled `install-indesign-bridge.sh` remains available for explicit user-profile installation when required.

Manual Windows destination:

~~~text
%APPDATA%\Adobe\InDesign\Version <VERSION>\<locale>\Scripts\Startup Scripts\mcp-bridge-indesign.idjs
~~~

Manual macOS destinations to verify for the installed version:

~~~text
~/Library/Preferences/Adobe InDesign/Version <VERSION>/<locale>/Scripts/Startup Scripts/mcp-bridge-indesign.idjs
/Applications/Adobe InDesign <YEAR>/Scripts/Startup Scripts/mcp-bridge-indesign.idjs
~~~

Start the broker and register the stdio server:

~~~powershell
.\target\release\id-mcp.exe serve-daemon
codex mcp add indesign -- "C:\absolute\path\target\release\id-mcp.exe" serve-stdio
~~~

Restart InDesign after installing or updating the Startup Script.

## Windows live result (2026-07-15)

Verified on Windows 11 x86_64, InDesign 2026 21.4.1.4, Japanese locale, bridge/id-mcp 0.4.4.

| Check | Result |
|---|---|
| preference-profile Startup Script cold start | pass |
| protocol-v1 heartbeat / instance discovery / ping | pass |
| daemon-first and host-first discovery | pass |
| `listDocuments` with no open document | pass (`[]`) |
| non-mutating raw `app.doScript(String, UXPSCRIPT)` | pass |
| raw temporary document creation, rectangle creation, no-save close | pass |
| short timeout followed by `get-script-result` | pass |
| daemon restart while InDesign remains open | pass |
| forced host termination followed by new instance discovery | pass |
| one-step undo for `UndoModes.ENTIRE_SCRIPT` | fail: Ctrl+Z and enabled Undo menu action did not remove the rectangle; `undoName` remained empty |
| normal shutdown heartbeat removal | fail: application exited, but heartbeat remained and was reported as stale after 10 seconds |

Creating a new document took longer than 5 seconds in one run. Callers that intentionally use a short timeout can recover the completed result through `get-script-result`.

## Manual real-host E2E gate

Record OS, InDesign version, UXP version, locale, commit, and bridge version.

1. Remove or rename any previous bridge script and start InDesign. Confirm no indesign heartbeat appears.
2. Install the .idjs, restart InDesign, and confirm ~/Documents/id-mcp-bridge/instances/<id>/heartbeat.json reports protocolVersion: 1, hostId: "indesign", and bridgeRuntime: "uxp-startup-script".
3. Start id-mcp serve-daemon; call list-indesign-instances and run-bridge-test.
4. With no document open, read indesign://documents; expect an empty list.
5. Open two documents and verify names, IDs, page/story counts, and the active-document template.
6. Run a non-mutating raw script through run-script; verify the returned value and corresponding requestId.
7. Run an allowed synchronous function-body file through run-script-file; verify path/hash/size audit metadata in the retained registry. Do not use a general top-level `.idjs`, top-level await, or script.setResult in this file.
8. Run a small mutation with a recognizable undo name. Confirm one undo restores the document.
9. Submit a delayed request with a short client timeout; recover it using get-script-result.
10. Start the daemon before InDesign and repeat discovery. Then restart InDesign and verify stale-instance reporting and rediscovery.
11. Test two installed InDesign versions if available; verify targetless ambiguity and targetInstanceId routing.
12. Test sleep/resume and a modal dialog; verify heartbeat and polling recover.

The following remain explicit PoC gates, not current guarantees:

- app.doScript String execution and script.setResult remain stable outside the verified Windows/InDesign 21.4.1.4 host; Promise-returning raw code is not currently supported.
- callback-based fs.rename replacement works on macOS and other supported InDesign UXP runtimes.
- the user preference Startup Scripts directory is honored on every supported macOS version/locale.
- one `UndoModes.ENTIRE_SCRIPT` request produces one usable host Undo step.
- normal host shutdown removes the heartbeat instead of relying on stale-instance detection.
- an unresolved top-level Promise remains alive without delaying or destabilizing other supported hosts.

Save results using docs/bridge-smoke-result.schema.json. Do not promote InDesign beyond Experimental until the required checks pass on both Windows and macOS.
