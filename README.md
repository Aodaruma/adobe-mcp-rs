# Adobe MCP Servers (Rust)

Rust-based MCP servers and Adobe host bridge panels for LLM-driven local automation.

This repository was renamed from `after-effects-mcp-rs` to `adobe-mcp-rs` so the project can grow beyond After Effects. The current codebase contains the most complete implementation for After Effects, plus experimental Premiere Pro, Photoshop, and Illustrator paths.

- Japanese: [README-ja.md](README-ja.md)

## Status

Last synchronized with the code on 2026-07-15.

| Host app | Binary | Bridge runtime | Status | Current boundary |
|---|---|---|---|---|
| After Effects | `ae-mcp` | ScriptUI / ExtendScript JSX | **Primary** | Requires the panel, Auto-run, and the `serve-daemon` broker |
| Premiere Pro | `pr-mcp` | UXP 25.6+, CEP / ExtendScript 24.0+ fallback | **Experimental** | Initial sequence/export surface; `serve-daemon` broker required |
| Photoshop | `ps-mcp` | UXP 23.3+ (API v2) | **Experimental** | Initial generic execution and read-only document/layer surface |
| Illustrator | `ai-mcp` | CEP / ExtendScript 24.0+ (CSXS 10) | **Experimental** | Initial document/artboard/layer/export surface; runtime packaging needs validation |

**Primary** means the default operational path is implemented. **Experimental** means a binary, bridge, and minimal MCP surface exist, but real-host E2E, packaging, runtime compatibility, or broker/service parity still needs hardening. **Planned** is reserved for hosts without a usable binary-and-bridge pair. See [the host status source of truth](docs/adobe-host-roadmap.md) for the full criteria, runtime constraints, and verification procedure.

## Current Architecture

The workspace is split into reusable Rust crates and host-specific binaries:

| Path | Role |
|---|---|
| `crates/ae-mcp` | After Effects CLI, MCP stdio server, daemon, and bridge commands |
| `crates/pr-mcp` | Premiere Pro CLI and MCP stdio server |
| `crates/ps-mcp` | Photoshop CLI and MCP stdio server |
| `crates/ai-core` | Illustrator tool specs, prompts, and allowlisted script names |
| `crates/ai-mcp` | Illustrator CLI and MCP stdio server |
| `crates/mcp-core` | Shared config, MCP tool/prompt specs, bridge path defaults |
| `crates/bridge-core` | File bridge client, instance discovery, request registry, result retention |
| `crates/daemon-core` | Shared TCP broker/client, per-instance scheduler, global-exclusive gate |
| `crates/platform-service` | Windows current-user autostart and macOS launchd helpers |
| `crates/pr-core` | Premiere Pro tool specs, prompts, and allowlisted script names |
| `crates/ps-core` | Photoshop tool specs, help text, and allowlisted script names |
| `src/scripts` | After Effects JSX bridge and helper scripts |
| `src/premiere/uxp` | Premiere Pro UXP bridge panel |
| `src/premiere/cep` | Legacy Premiere Pro CEP bridge fallback |
| `src/photoshop/uxp` | Photoshop UXP bridge panel |
| `src/illustrator/cep` | Illustrator CEP / ExtendScript bridge panel |

All four binaries use the same local TCP broker model: `serve-stdio` proxies normal MCP execution to `serve-daemon`, which routes file-bridge commands to `instances/<instanceId>/` and retains results by `requestId`. Jobs are FIFO within one instance, may run in parallel on separate instances, and can request a host-wide global-exclusive gate.

Default loopback addresses are host-specific: After Effects `127.0.0.1:47655`, Premiere Pro `:47656`, Photoshop `:47657`, and Illustrator `:47658`. `daemon_addr` in a host-specific config can override the default. The root command/result files and each binary's `bridge` CLI remain available only for compatibility and diagnostics; they are not the normal MCP transport. See [ADR 0001](docs/adr/0001-host-neutral-daemon-broker.md).

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
cargo build --release -p ps-mcp
cargo build --release -p ai-mcp
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

Start `pr-mcp serve-daemon` before registering or using the MCP server.

Register the MCP server:

```powershell
codex mcp add premiere -- "<ABSOLUTE_PATH>\target\release\pr-mcp.exe" serve-stdio
```

### Photoshop

Build the binary:

```powershell
cargo build --release -p ps-mcp
```

Load the UXP bridge from `src/photoshop/uxp/mcp-bridge-photoshop` with Adobe UXP Developer Tool, then open `Photoshop MCP Bridge` from the Photoshop Plugins menu and keep `Auto-run commands` enabled.

Start `ps-mcp serve-daemon` before registering or using the MCP server.

Register the MCP server:

```powershell
codex mcp add photoshop -- "<ABSOLUTE_PATH>\target\release\ps-mcp.exe" serve-stdio
```

### Illustrator

Build the binary:

```powershell
cargo build --release -p ai-mcp
```

Install or copy `src/illustrator/cep/mcp-bridge-illustrator` into a CEP extensions directory, then open `Window > Extensions > Illustrator MCP Bridge` in Illustrator and enable `Auto-run commands`.

Start `ai-mcp serve-daemon` before registering or using the MCP server.

Register the MCP server:

```powershell
codex mcp add illustrator -- "<ABSOLUTE_PATH>\target\release\ai-mcp.exe" serve-stdio
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
.\target\release\pr-mcp.exe serve-daemon
```

In another terminal:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-premiere-instances","arguments":{}}}' | .\target\release\pr-mcp.exe serve-stdio
```

Photoshop:

```powershell
.\target\release\ps-mcp.exe health
.\target\release\ps-mcp.exe serve-daemon
```

In another terminal:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-photoshop-instances","arguments":{}}}' | .\target\release\ps-mcp.exe serve-stdio
```

Illustrator:

```powershell
.\target\release\ai-mcp.exe health
.\target\release\ai-mcp.exe serve-daemon
```

In another terminal:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list-illustrator-instances","arguments":{}}}' | .\target\release\ai-mcp.exe serve-stdio
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

This nine-tool list is the complete After Effects public contract returned by `tools/list`. Historical names such as `run-script`, `create-composition`, effect helpers, render-queue helpers, and project lifecycle helpers remain accepted only by hidden compatibility dispatch. A legacy call includes a deprecation notice and its public replacement; prompts and setup instructions do not depend on hidden names.

`run-script` is intentionally not republished. Its allowlist remains useful for old clients, but its asynchronous direct-file behavior does not match the synchronous daemon-backed public contract. New host-specific operations should use explicit `run-jsx` calls (`mode: "unsafe"`). The `aftereffects://compositions` resource and every operation named by an After Effects prompt use the daemon broker; prompts themselves only return reusable instructions.

Premiere Pro currently exposes:

- `run-jsx`
- `run-jsx-file`
- `run-script`
- `get-jsx-result`
- `get-results`
- `get-help`
- `list-premiere-instances`
- `run-bridge-test`

Photoshop currently exposes:

- `run-jsx`
- `run-jsx-file`
- `run-script`
- `get-jsx-result`
- `get-results`
- `get-help`
- `list-photoshop-instances`
- `run-bridge-test`

Illustrator currently exposes:

- `run-jsx`
- `run-jsx-file`
- `run-script`
- `get-jsx-result`
- `get-results`
- `get-help`
- `list-illustrator-instances`
- `run-bridge-test`

For arbitrary code execution, pass `mode: "unsafe"` and a short `description`. `unsafe` does not mean sandboxed: host-side JavaScript/JSX runs with the Adobe host's authority. See the [run-jsx-file trust policy](docs/script-file-security.md) for allowed roots, trusted path/hash entries, extensions, migration behavior, and retained audit metadata.

## Expansion Plan

The next step is to make host support a first-class concept instead of cloning the After Effects assumptions into every app.

1. **Done:** Extract host metadata into `HostSpec`.
2. **Done:** Normalize the bridge protocol and retained request records across hosts.
3. **Done:** Share the `daemon-core` broker model across all four binaries; keep direct file bridge only for compatibility and diagnostics.
4. Harden the initial Photoshop UXP bridge with write operations, modal execution policies, and installer E2E coverage.
5. Harden the initial Illustrator CEP bridge with export coverage, current-version runtime validation, signing, and installer E2E coverage. Keep UXP optional until public host support is clear enough for third-party automation.

Detailed notes are in [docs/adobe-host-roadmap.md](docs/adobe-host-roadmap.md).

## Worktree Workflow

The repository container keeps the bare Git data, the main checkout, and issue worktrees separate:

```text
Documents/GitHub/adobe-mcp-rs/
  .repo.git/          # central bare repository
  main/               # main worktree
  worktrees/          # issue/feature worktrees
```

Useful commands:

```powershell
cd .\main
git worktree list
git worktree add ..\worktrees\issue-123 -b codex/issue-123 main
git worktree remove ..\worktrees\issue-123
```

See [docs/worktree.md](docs/worktree.md) for the local workflow notes.

## Docs

- [Adobe host roadmap](docs/adobe-host-roadmap.md)
- [After Effects MCP public surface](docs/after-effects-mcp-surface.md)
- [Worktree workflow](docs/worktree.md)
- [Codex MCP setup](docs/setup-codex-mcp.md)
- [Operations runbook](docs/operations-runbook.md)
- [Bridge contract and Adobe host smoke testing](docs/bridge-contract-testing.md)
- [Installer E2E guide](docs/installer-e2e.md)
- [Release checklist](docs/release-checklist.md)
- [Rust migration specification](docs/specification-rust-migration.md)
- [TS to Rust migration guide](docs/migration-guide-ts-to-rust.md)

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).
