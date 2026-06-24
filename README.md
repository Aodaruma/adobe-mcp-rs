# Adobe MCP Servers (Rust)

Rust-based MCP servers and Adobe host bridge panels for LLM-driven local automation.

This repository was renamed from `after-effects-mcp-rs` to `adobe-mcp-rs` so the project can grow beyond After Effects. The current codebase contains the most complete implementation for After Effects, an experimental Premiere Pro path, and the shared Rust pieces needed to add Photoshop and Illustrator.

- Japanese: [README-ja.md](README-ja.md)

## Status

| Host app | Binary | Bridge | Status |
|---|---|---|---|
| After Effects | `ae-mcp` | ScriptUI / JSX panel | Primary supported path |
| Premiere Pro | `pr-mcp` | UXP panel, CEP fallback | Experimental; API surface exists, install/release path still needs hardening |
| Photoshop | planned `ps-mcp` | UXP plugin preferred | Planned |
| Illustrator | planned `ai-mcp` | ExtendScript/CEP or native plugin first; UXP only after public host support is confirmed | Planned |

## Current Architecture

The workspace is split into reusable Rust crates and host-specific binaries:

| Path | Role |
|---|---|
| `crates/ae-mcp` | After Effects CLI, MCP stdio server, daemon, and bridge commands |
| `crates/pr-mcp` | Premiere Pro CLI and MCP stdio server |
| `crates/mcp-core` | Shared config, MCP tool/prompt specs, bridge path defaults |
| `crates/bridge-core` | File bridge client, instance discovery, request registry, result retention |
| `crates/platform-service` | Windows/macOS service and autostart helpers |
| `crates/pr-core` | Premiere Pro tool specs, prompts, and allowlisted script names |
| `src/scripts` | After Effects JSX bridge and helper scripts |
| `src/premiere/uxp` | Premiere Pro UXP bridge panel |
| `src/premiere/cep` | Legacy Premiere Pro CEP bridge fallback |

After Effects uses `ae-mcp serve-daemon` as a local broker. Bridge panels register under `~/Documents/ae-mcp-bridge/instances/<instanceId>/`, and MCP calls are routed to a target instance with retained `requestId` results.

Premiere Pro currently reuses the same file-bridge pattern under `~/Documents/pr-mcp-bridge`. The UXP bridge is the intended path, while the CEP bridge remains a fallback. `pr-mcp serve-daemon` is not yet equivalent to the After Effects broker, so Premiere should still be treated as experimental.

## Setup

Prerequisites:

- Rust stable and Cargo
- The Adobe host app you want to automate
- For UXP bridges, Adobe UXP Developer Tool and host developer mode where required

Build all Rust binaries:

```powershell
cargo build --release
```

Build one host binary:

```powershell
cargo build --release -p ae-mcp
cargo build --release -p pr-mcp
```

### After Effects

Install the bridge panel:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-bridge.ps1
```

```bash
bash ./scripts/install-bridge.sh
```

In After Effects:

1. Enable `Allow Scripts to Write Files and Access Network`.
2. Restart After Effects.
3. Open `Window > mcp-bridge-auto.jsx`.
4. Enable `Auto-run commands`.

Run the broker:

```powershell
.\target\release\ae-mcp.exe serve-daemon
```

Register the MCP server:

```powershell
codex mcp add aftereffects -- "<ABSOLUTE_PATH>\target\release\ae-mcp.exe" serve-stdio
```

### Premiere Pro

Build the binary:

```powershell
cargo build --release -p pr-mcp
```

Load the UXP bridge from `src/premiere/uxp/mcp-bridge-premiere` with Adobe UXP Developer Tool, then open `Window > UXP Plugins > Premiere MCP Bridge` in Premiere Pro and enable `Auto-run commands`.

Register the MCP server:

```powershell
codex mcp add premiere -- "<ABSOLUTE_PATH>\target\release\pr-mcp.exe" serve-stdio
```

## Quick Validation

After Effects:

```powershell
.\target\release\ae-mcp.exe health
.\target\release\ae-mcp.exe serve-daemon
```

In another terminal:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-ae-instances","arguments":{}}}' | .\target\release\ae-mcp.exe serve-stdio
```

Premiere Pro:

```powershell
.\target\release\pr-mcp.exe health
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-premiere-instances","arguments":{}}}' | .\target\release\pr-mcp.exe serve-stdio
```

## MCP Tool Surface

After Effects currently exposes:

- `run-jsx`
- `run-jsx-file`
- `get-jsx-result`
- `list-ae-instances`
- `get-results`
- `get-help`
- `save-frame-png`
- `cleanup-preview-folder`
- `run-bridge-test`

Premiere Pro currently exposes:

- `run-jsx`
- `run-jsx-file`
- `run-script`
- `get-jsx-result`
- `get-results`
- `get-help`
- `list-premiere-instances`
- `run-bridge-test`

For arbitrary code execution, pass `mode: "unsafe"` and a short `description`. This is intentional: host-side JavaScript/JSX execution is powerful and should be explicit in MCP calls.

## Expansion Plan

The next step is to make host support a first-class concept instead of cloning the After Effects assumptions into every app.

1. Extract a small host adapter layer for host names, bridge root names, tool names, executable names, help text, and installer behavior.
2. Normalize the bridge protocol across hosts: `heartbeat.json`, command files, result files, instance metadata, capabilities, and retained request records.
3. Harden Premiere Pro to match the After Effects broker model or explicitly document it as direct file-bridge only.
4. Add Photoshop through a UXP bridge first, using the Photoshop DOM and `batchPlay` for operations that are not covered by the DOM.
5. Add Illustrator after a short spike that confirms the best bridge technology for current Illustrator versions. Treat ExtendScript/CEP or a native plugin bridge as the practical baseline until public UXP support is clear enough for third-party automation.

Detailed notes are in [docs/adobe-host-roadmap.md](docs/adobe-host-roadmap.md).

## Worktree Workflow

This checkout has been prepared as a linked worktree. The central bare repository is expected next to it:

```text
Documents/GitHub/
  adobe-mcp-rs.git/   # bare repository
  adobe-mcp-rs/       # main worktree
```

Useful commands:

```powershell
git worktree list
git worktree add ..\adobe-mcp-rs-photoshop -b codex/photoshop-support main
git worktree add ..\adobe-mcp-rs-illustrator -b codex/illustrator-support main
git worktree remove ..\adobe-mcp-rs-photoshop
```

See [docs/worktree.md](docs/worktree.md) for the local workflow notes.

## Docs

- [Adobe host roadmap](docs/adobe-host-roadmap.md)
- [Worktree workflow](docs/worktree.md)
- [Codex MCP setup](docs/setup-codex-mcp.md)
- [Operations runbook](docs/operations-runbook.md)
- [Installer E2E guide](docs/installer-e2e.md)
- [Release checklist](docs/release-checklist.md)
- [Rust migration specification](docs/specification-rust-migration.md)
- [TS to Rust migration guide](docs/migration-guide-ts-to-rust.md)

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).
