# InDesign MCP PoC

## Status

InDesign support is **Experimental / real-host unverified**. The Rust server, protocol-v1 bridge, contract tests, and installer path are implemented, but this repository has not yet executed them in a live InDesign process.

The minimum target is InDesign 18.5. InDesign supports UXP scripts from 18.0, while Adobe's current filesystem recipe calls out 18.5 or later. The repository bridge uses a UXP Startup Script (.idjs) rather than a panel and is designed to start with InDesign without an Auto-run checkbox; that persistence behavior remains a real-host PoC gate.

## Architecture

~~~text
MCP client
  -> id-mcp serve-stdio
  -> id-mcp serve-daemon (127.0.0.1:47659)
  -> ~/Documents/id-mcp-bridge/instances/<instanceId>/
  -> mcp-bridge-indesign.idjs
  -> InDesign UXP DOM
~~~

mcp-core::INDESIGN_HOST, daemon-core, the retained request registry, script-file policy, and the cross-host contract fixture are shared with the other Adobe hosts. The startup script writes protocol-v1 heartbeat, command, and result records with atomic temp-file replacement.

## Why a Startup Script

Adobe documents that:

- InDesign 18.0 and later run .idjs UXP scripts: [UXP scripts](https://developer.adobe.com/indesign/uxp/scripts/).
- a script inside Scripts/Startup Scripts runs when InDesign starts: [Tips and tricks for UXP scripts](https://developer.adobe.com/indesign/uxp/scripts/tutorials/tips-tricks/).
- a long-running event script may keep its top-level Promise pending for the session: [InDesign events](https://developer.adobe.com/indesign/uxp/resources/recipes/indesign-events/).
- UXP scripts receive fixed localFileSystem: fullAccess and network permissions, while allowCodeGenerationFromStrings is false: [UXP manifest permissions for scripts](https://developer.adobe.com/indesign/uxp/resources/fundamentals/manifest/).
- Application.doScript accepts a File, String, or JavaScript Function and a ScriptLanguage: [Application API](https://developer.adobe.com/indesign/uxp/dom/api/a/application/).

Therefore run-script does **not** call eval or the Function constructor. It sends code to InDesign's documented app.doScript(..., ScriptLanguage.UXPSCRIPT, ...) host API. Adobe documents the String input, but executing that input from this long-running Startup Script has not yet been verified in a live host. Until the manual PoC passes, raw execution must not be represented as production-verified.

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

Windows release ZIP/MSI artifacts include `id-mcp.exe`, the Startup Script, and `install-indesign-bridge.ps1`. The generic MSI helper updates an existing Codex config and repairs an already opted-in `InDesignMcp` current-user autostart entry. It copies the Startup Script only into current-user InDesign preference profiles that actually exist.

macOS release archives/pkg include `id-mcp` and an InDesign bundle under `/usr/local/share/ae-mcp/indesign`. The root pkg postinstall does not guess or modify a user preference profile. Run the bundled `install-indesign-bridge.sh --dry-run` as the target user, then pass an explicit verified destination if detection finds none.

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

The following are explicit PoC gates, not current guarantees:

- app.doScript String execution works from the long-running UXP Startup Script.
- script.setResult and the synchronous returned value are stable for the supported InDesign versions; Promise-returning raw code is not currently supported.
- fs.renameSync replacement works atomically on both Windows and macOS UXP runtimes.
- the user preference Startup Scripts directory is honored on every supported macOS version/locale.
- an unresolved top-level Promise remains alive without delaying or destabilizing host startup.

Save results using docs/bridge-smoke-result.schema.json. Do not promote InDesign beyond Experimental until the required checks pass on both Windows and macOS.
