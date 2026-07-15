use anyhow::Result;
use bridge_contract_tests::{DaemonEndpoint, ProtocolFixture, ResponsePlan};
use mcp_core::{HostSpec, HOST_SPECS};
use serde_json::{json, Value};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn smoke_result_schema_and_example_are_valid_json() -> Result<()> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../docs/bridge-smoke-result.schema.json"
    ))?;
    let example: Value = serde_json::from_str(include_str!(
        "../../../docs/examples/bridge-smoke-result.example.json"
    ))?;
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(example["schemaVersion"], 1);
    Ok(())
}

#[test]
fn after_effects_startup_bridge_is_headless_and_generation_guarded() {
    let startup = include_str!("../../../src/scripts/mcp-bridge-startup.jsx");
    let shutdown = include_str!("../../../src/scripts/mcp-bridge-shutdown.jsx");
    let runtime = include_str!("../../../src/scripts/mcp-bridge-auto.jsx");
    let windows_installer = include_str!("../../../scripts/install-bridge.ps1");
    let packaged_installer = include_str!("../../../scripts/install-bridge-installer.ps1");
    let macos_installer = include_str!("../../../scripts/install-bridge.sh");

    assert!(startup.contains("$.evalFile(runtimeFile)"));
    assert!(startup.contains("headless: true"));
    assert!(startup.contains("delete $.global.__adobeMcpBridgeBootstrapConfig"));
    assert!(startup.contains("ae_mcp_bootstrap.json"));
    assert!(startup.contains("writeBootstrapDiagnostic(state)"));
    assert!(!startup.contains("findMenuCommandId"));
    assert!(!startup.contains("executeCommand"));
    assert!(!startup.contains("new Window"));

    assert!(runtime.contains("__adobeMcpBridgeRuntime"));
    assert!(runtime.contains("(function (thisObj)"));
    assert!(runtime.contains("aeMcpAttachExistingRuntime"));
    assert!(runtime.contains("Closing it must"));
    assert!(runtime.contains("writeHeartbeatTextFile(heartbeatFile"));
    assert!(runtime.contains("Rust reader's retry"));
    assert!(runtime.contains("var targetName = file.name;"));
    assert!(runtime.contains("tempFile.rename(targetName)"));
    assert!(runtime.contains("var publishedBackup = new File(backupPath)"));
    assert!(runtime.contains("__adobeMcpBridgeCommandTick('"));
    assert!(runtime.contains("scheduledRuntimeId !== bridgeRuntimeId"));
    assert!(runtime.contains("Neutralize callbacks left by the pre-generation runtime"));
    assert!(runtime.contains("aeMcpBridgeRestart"));
    assert!(runtime.contains("extendscript-startup"));
    assert!(shutdown.contains("removeHeartbeat: true"));
    assert!(shutdown.contains("after-effects-shutdown"));

    for installer in [windows_installer, packaged_installer, macos_installer] {
        assert!(installer.contains("mcp-bridge-startup.jsx"));
        assert!(installer.contains("mcp-bridge-shutdown.jsx"));
        assert!(installer.contains("mcp-bridge-auto.jsx"));
    }
}

#[test]
fn indesign_startup_bridge_uses_supported_uxp_file_apis() {
    let bridge = include_str!("../../../src/indesign/uxp/mcp-bridge-indesign.idjs");

    assert!(bridge.contains("fs.mkdir(path, { recursive: true }, (error) =>"));
    assert!(bridge.contains("fs.rename(sourcePath, destinationPath, (error) =>"));
    assert!(bridge.contains("destinationPath === bridgePaths().heartbeatFile"));
    assert!(bridge.contains("await ensureBridgeDirectories()"));
    assert!(bridge.contains("await writeHeartbeat("));
    assert!(bridge.contains("await renameFile(tempPath, path)"));
    assert!(bridge.contains("fileWriteTail.then(() => performAtomicWrite(path, text))"));
    assert!(bridge.contains("app.addEventListener(\"beforeQuit\", handleBeforeQuit)"));
    assert!(bridge.contains("app.removeEventListener(\"beforeQuit\", handleBeforeQuit)"));
    assert!(bridge.contains("clearInterval(pollIntervalId)"));
    assert!(bridge.contains("clearInterval(heartbeatIntervalId)"));
    assert!(bridge.contains("beforeQuit does not wait for promise continuations"));
    assert!(bridge.matches("removeHeartbeatFile();").count() >= 2);
    assert!(bridge.contains("return lifecyclePromise"));

    for unsupported in [
        "fs.mkdirSync",
        "fs.statSync",
        "fs.openSync",
        "fs.closeSync",
        "fs.renameSync",
    ] {
        assert!(
            !bridge.contains(unsupported),
            "InDesign UXP does not expose {unsupported}"
        );
    }
}

fn run_command(
    daemon: &DaemonEndpoint,
    target_instance_id: Option<&str>,
    sequence: u64,
    timeout_ms: u64,
) -> Result<Value> {
    let mut request = json!({
        "op": "runCommand",
        "command": "ping",
        "args": { "sequence": sequence },
        "timeoutMs": timeout_ms,
        "pollIntervalMs": 5,
        "retentionSeconds": 60
    });
    if let Some(instance_id) = target_instance_id {
        request["targetInstanceId"] = json!(instance_id);
    }
    daemon.call(request, timeout_ms.saturating_add(500))
}

fn assert_completed(value: &Value, host: HostSpec, instance_id: &str, sequence: u64) {
    assert_eq!(
        value.get("status").and_then(Value::as_str),
        Some("completed"),
        "{} returned {value}",
        host.id
    );
    assert_eq!(
        value
            .pointer("/result/result/sequence")
            .and_then(Value::as_u64),
        Some(sequence)
    );
    assert_eq!(
        value
            .pointer("/result/_hostInstance/hostId")
            .and_then(Value::as_str),
        Some(host.id)
    );
    assert_eq!(
        value
            .pointer("/result/_hostInstance/instanceId")
            .and_then(Value::as_str),
        Some(instance_id)
    );
}

#[test]
fn heartbeat_command_and_result_contract_supports_both_start_orders() -> Result<()> {
    for host in HOST_SPECS {
        // Host-first: a running panel must be discovered when the daemon starts later.
        let fixture = ProtocolFixture::new(*host)?;
        let mock = fixture.start_mock_host("host-first")?;
        mock.enqueue(ResponsePlan::success(json!({ "sequence": 1 })));
        let daemon = fixture.start_daemon()?;
        let instances = daemon.call(json!({ "op": "listInstances" }), 500)?;
        assert_eq!(instances["count"], 1, "{} host-first", host.id);
        assert_eq!(instances["instances"][0]["protocolVersion"], 1);
        assert_eq!(instances["instances"][0]["hostId"], host.id);
        assert_eq!(
            instances["instances"][0]["bridgeRuntime"],
            host.bridge_runtime
        );
        let result = run_command(&daemon, Some("host-first"), 1, 500)?;
        assert_completed(&result, *host, "host-first", 1);
        let commands = mock.observed_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "ping");
        assert_eq!(commands[0].args["sequence"], 1);
        assert!(commands[0].request_id.is_some());

        // Daemon-first: a panel that appears later must become routable.
        let fixture = ProtocolFixture::new(*host)?;
        let daemon = fixture.start_daemon()?;
        let empty = daemon.call(json!({ "op": "listInstances" }), 500)?;
        assert_eq!(empty["count"], 0, "{} daemon-first initial", host.id);
        let mock = fixture.start_mock_host("daemon-first")?;
        mock.enqueue(ResponsePlan::success(json!({ "sequence": 2 })));
        let result = run_command(&daemon, Some("daemon-first"), 2, 500)?;
        assert_completed(&result, *host, "daemon-first", 2);
    }
    Ok(())
}

#[test]
fn timeout_late_result_survives_a_new_daemon_connection() -> Result<()> {
    for host in HOST_SPECS {
        let fixture = ProtocolFixture::new(*host)?;
        let mock = fixture.start_mock_host("retained")?;
        mock.enqueue(ResponsePlan::delayed(
            Duration::from_millis(140),
            json!({ "sequence": 10 }),
        ));
        let first_daemon = fixture.start_daemon()?;
        let timed_out = run_command(&first_daemon, Some("retained"), 10, 30)?;
        assert_eq!(timed_out["status"], "timeout", "{} timeout", host.id);
        let request_id = timed_out["requestId"]
            .as_str()
            .expect("timeout has requestId")
            .to_string();

        // A fresh daemon listener models client reconnect/broker restart. The
        // new process view must recover the file-backed request registry.
        let reconnected = fixture.start_daemon()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let retained =
                reconnected.call(json!({ "op": "getResult", "requestId": request_id }), 500)?;
            if retained["status"] == "completed" {
                assert_completed(&retained, *host, "retained", 10);
                assert_eq!(retained["requestId"], request_id);
                break;
            }
            assert_eq!(retained["status"], "timeout", "{} retained", host.id);
            assert!(Instant::now() < deadline, "{} late result missing", host.id);
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

#[test]
fn stale_and_malformed_heartbeats_are_reported_and_reconnect() -> Result<()> {
    for host in HOST_SPECS {
        let fixture = ProtocolFixture::new(*host)?.with_heartbeat_stale_ms(120);
        let daemon = fixture.start_daemon()?;
        let mock = fixture.start_mock_host("stale")?;
        fixture.write_malformed_heartbeat("malformed")?;
        mock.enqueue(ResponsePlan::Ignore);

        let timed_out = run_command(&daemon, Some("stale"), 20, 30)?;
        assert_eq!(timed_out["status"], "timeout");
        let request_id = timed_out["requestId"]
            .as_str()
            .expect("timeout has requestId")
            .to_string();
        mock.pause_heartbeat();
        thread::sleep(Duration::from_millis(180));

        let report = daemon.call(json!({ "op": "listInstances" }), 1_000)?;
        assert_eq!(report["count"], 0, "{} stale count", host.id);
        let reasons: Vec<_> = report["inactiveInstances"]
            .as_array()
            .expect("inactiveInstances array")
            .iter()
            .filter_map(|entry| entry["reason"].as_str())
            .collect();
        assert!(
            reasons.iter().any(|reason| *reason == "heartbeat is stale"),
            "{} reasons: {reasons:?}",
            host.id
        );
        assert!(
            reasons
                .iter()
                .any(|reason| reason.contains("failed to read or parse heartbeat.json")),
            "{} reasons: {reasons:?}",
            host.id
        );
        let lost = daemon.call(json!({ "op": "getResult", "requestId": request_id }), 500)?;
        assert_eq!(lost["status"], "lost", "{} lost request", host.id);

        mock.resume_heartbeat();
        thread::sleep(Duration::from_millis(40));
        let reconnected = daemon.call(json!({ "op": "listInstances" }), 1_000)?;
        assert_eq!(reconnected["count"], 1, "{} reconnect", host.id);
        mock.enqueue(ResponsePlan::success(json!({ "sequence": 21 })));
        let next = run_command(&daemon, Some("stale"), 21, 500)?;
        assert_completed(&next, *host, "stale", 21);
    }
    Ok(())
}

#[test]
fn partial_file_json_and_chunked_tcp_json_do_not_break_the_protocol() -> Result<()> {
    for host in HOST_SPECS {
        let fixture = ProtocolFixture::new(*host)?;
        let mock = fixture.start_mock_host("partial")?;
        mock.enqueue(ResponsePlan::PartialJsonThenSuccess {
            invalid_for: Duration::from_millis(60),
            value: json!({ "sequence": 30 }),
        });
        let daemon = fixture.start_daemon()?;
        let result = run_command(&daemon, Some("partial"), 30, 500)?;
        assert_completed(&result, *host, "partial", 30);

        let chunked = daemon.send_raw_chunks(&[
            (r#"{"op":"pi"#, Duration::from_millis(20)),
            ("ng\"}\n", Duration::ZERO),
        ])?;
        assert_eq!(chunked["ok"], true, "{} chunked TCP", host.id);
        assert_eq!(chunked["value"]["hostId"], host.id);

        let malformed = daemon.send_raw_chunks(&[("{not-json}\n", Duration::ZERO)])?;
        assert_eq!(malformed["ok"], false, "{} malformed TCP", host.id);
        assert!(malformed["error"]
            .as_str()
            .unwrap_or_default()
            .contains("invalid daemon request"));
    }
    Ok(())
}

#[test]
fn multiple_instances_require_targeting_and_route_independently() -> Result<()> {
    for host in HOST_SPECS {
        let fixture = ProtocolFixture::new(*host)?;
        let first = fixture.start_mock_host("instance-a")?;
        let second = fixture.start_mock_host("instance-b")?;
        first.enqueue(ResponsePlan::success(json!({ "sequence": 41 })));
        second.enqueue(ResponsePlan::success(json!({ "sequence": 42 })));
        let daemon = fixture.start_daemon()?;

        let ambiguous = run_command(&daemon, None, 40, 500)?;
        assert_eq!(ambiguous["status"], "failed", "{} ambiguous", host.id);
        assert!(ambiguous["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Multiple active"));

        let first_result = run_command(&daemon, Some("instance-a"), 41, 500)?;
        let second_result = run_command(&daemon, Some("instance-b"), 42, 500)?;
        assert_completed(&first_result, *host, "instance-a", 41);
        assert_completed(&second_result, *host, "instance-b", 42);
        assert_eq!(first.observed_commands().len(), 1);
        assert_eq!(second.observed_commands().len(), 1);
    }
    Ok(())
}
