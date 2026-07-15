# ADR 0001: host-neutral daemon broker

- Status: Accepted
- Date: 2026-07-15

## Context

After Effects had a localhost TCP request broker, while Premiere Pro, Photoshop, and Illustrator exposed a `serve-daemon` command that only maintained a PID file and heartbeat log. Their MCP stdio servers operated the file bridge directly. The same command therefore had different operational meanings and different recovery behavior.

## Decision

`daemon-core` owns one broker implementation shared by all four host binaries.

- Protocol: newline-delimited JSON over a host-specific loopback TCP address.
- Operations: `ping`, `listInstances`, `runCommand`, `getResult`, `latestResult`.
- Scheduling: FIFO per `instanceId`; separate instances use separate workers; `globalExclusive` takes a write lock that excludes all normal jobs for that host daemon.
- Results: requests are retained by `requestId` in the existing bridge registry. A client timeout does not cancel the worker, so `getResult` can recover a later result.
- Startup: bind the listener before publishing `daemon.pid`. Bind failure reports the host and address and suggests that another daemon may already be running.
- Configuration: each `HostSpec` provides a distinct default port. An explicit `daemon_addr` remains supported.

| Host | Default address |
|---|---|
| After Effects | `127.0.0.1:47655` |
| Premiere Pro | `127.0.0.1:47656` |
| Photoshop | `127.0.0.1:47657` |
| Illustrator | `127.0.0.1:47658` |

The binary-specific `bridge run-script` and `bridge get-results` commands, plus root-level command/result files, remain as direct file-bridge compatibility and diagnostic paths. MCP stdio execution for Premiere Pro, Photoshop, and Illustrator uses the daemon. Existing After Effects legacy dispatch entries that intentionally queue compatibility files are not the common transport contract and are tracked separately from this ADR.

## Consequences

- `serve-daemon` now means request broker on every supported host.
- Each configured MCP host needs its own daemon running.
- Failure and recovery instructions are consistent: inspect `health` for `daemon_addr`, start `<binary> serve-daemon`, then retry or recover a timed-out request with its `requestId`.
- The file bridge remains the host-process boundary; this decision does not replace UXP, CEP, or ExtendScript transport inside Adobe applications.
