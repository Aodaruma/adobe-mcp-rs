use anyhow::{anyhow, Context, Result};
use bridge_core::{BridgeClient, BridgeRunOptions, BridgeTarget};
use mcp_core::{host_spec_by_id, AppConfig, ScriptFileAudit};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use tracing::{error, info};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DaemonRequest {
    op: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Value,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    target_instance_id: Option<String>,
    #[serde(default)]
    target_version: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    poll_interval_ms: Option<u64>,
    #[serde(default)]
    retention_seconds: Option<u64>,
    #[serde(default)]
    global_exclusive: bool,
    #[serde(default)]
    audit: Option<ScriptFileAudit>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

type ScheduledTask = Box<dyn FnOnce() -> Result<Value, String> + Send>;

struct ScheduledJob {
    task: ScheduledTask,
    global_exclusive: bool,
    response_tx: mpsc::Sender<Result<Value, String>>,
}

#[derive(Clone, Default)]
struct InstanceScheduler {
    workers: Arc<Mutex<HashMap<String, mpsc::Sender<ScheduledJob>>>>,
    global_gate: Arc<RwLock<()>>,
}

impl InstanceScheduler {
    fn submit(
        &self,
        instance_id: &str,
        global_exclusive: bool,
        task: ScheduledTask,
    ) -> Result<mpsc::Receiver<Result<Value, String>>> {
        let worker = self.worker(instance_id);
        let (response_tx, response_rx) = mpsc::channel();
        worker
            .send(ScheduledJob {
                task,
                global_exclusive,
                response_tx,
            })
            .map_err(|_| anyhow!("failed to enqueue daemon job"))?;
        Ok(response_rx)
    }

    fn worker(&self, instance_id: &str) -> mpsc::Sender<ScheduledJob> {
        let mut workers = self.workers.lock().expect("workers mutex poisoned");
        if let Some(sender) = workers.get(instance_id) {
            return sender.clone();
        }

        let (sender, receiver) = mpsc::channel();
        workers.insert(instance_id.to_string(), sender.clone());
        let gate = Arc::clone(&self.global_gate);
        let worker_instance_id = instance_id.to_string();
        thread::spawn(move || {
            info!("daemon worker started for instance {worker_instance_id}");
            for job in receiver {
                let result = if job.global_exclusive {
                    let _guard = gate.write().expect("global gate poisoned");
                    (job.task)()
                } else {
                    let _guard = gate.read().expect("global gate poisoned");
                    (job.task)()
                };
                let _ = job.response_tx.send(result);
            }
        });
        sender
    }
}

struct DaemonState {
    cfg: AppConfig,
    bridge: BridgeClient,
    scheduler: InstanceScheduler,
}

/// Run a host-neutral local TCP request broker.
///
/// The listener is bound before the PID file is published, so a second daemon
/// produces a useful bind diagnostic without overwriting the active PID file.
pub fn run_daemon_server(cfg: AppConfig) -> Result<()> {
    let listener = bind_listener(&cfg)?;
    let _pid_file = DaemonPidFile::create(cfg.bridge.root_dir.join("daemon.pid"))?;
    run_daemon_with_listener(cfg, listener)
}

fn bind_listener(cfg: &AppConfig) -> Result<TcpListener> {
    TcpListener::bind(&cfg.daemon_addr).with_context(|| {
        format!(
            "failed to bind {} daemon at {}; another daemon may already be running or the address is unavailable",
            cfg.host_id, cfg.daemon_addr
        )
    })
}

fn run_daemon_with_listener(cfg: AppConfig, listener: TcpListener) -> Result<()> {
    let bridge = BridgeClient::new(cfg.clone())?;
    let state = Arc::new(DaemonState {
        cfg,
        bridge,
        scheduler: InstanceScheduler::default(),
    });

    info!(
        "{} serve-daemon listening on {}",
        state.cfg.host_id, state.cfg.daemon_addr
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    if let Err(error) = handle_client(stream, state) {
                        error!("daemon client error: {error}");
                    }
                });
            }
            Err(error) => error!("daemon accept error: {error}"),
        }
    }
    Ok(())
}

/// Run the daemon on an already-bound listener.
///
/// This entry point lets protocol contract tests reserve an ephemeral port
/// without a bind race. Production binaries should normally use
/// [`run_daemon_server`].
#[doc(hidden)]
pub fn run_daemon_server_with_listener(cfg: AppConfig, listener: TcpListener) -> Result<()> {
    run_daemon_with_listener(cfg, listener)
}

fn handle_client(mut stream: TcpStream, state: Arc<DaemonState>) -> Result<()> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?)
        .read_line(&mut line)
        .with_context(|| "failed to read daemon request")?;

    let response = match serde_json::from_str::<DaemonRequest>(line.trim()) {
        Ok(request) => match handle_request(&state, request) {
            Ok(value) => DaemonResponse {
                ok: true,
                value: Some(value),
                error: None,
            },
            Err(error) => DaemonResponse {
                ok: false,
                value: None,
                error: Some(error.to_string()),
            },
        },
        Err(error) => DaemonResponse {
            ok: false,
            value: None,
            error: Some(format!("invalid daemon request: {error}")),
        },
    };

    stream.write_all(serde_json::to_string(&response)?.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn handle_request(state: &Arc<DaemonState>, request: DaemonRequest) -> Result<Value> {
    match request.op.as_str() {
        "ping" => Ok(json!({
            "status": "ok",
            "hostId": state.cfg.host_id,
            "daemonAddr": state.cfg.daemon_addr
        })),
        "listInstances" => {
            let report = state
                .bridge
                .discover_instances(Duration::from_millis(state.cfg.instance_heartbeat_stale_ms))?;
            let count = report.instances.len();
            Ok(json!({
                "instances": report.instances,
                "inactiveInstances": report.inactive_instances,
                "count": count,
                "staleThresholdMs": state.cfg.instance_heartbeat_stale_ms
            }))
        }
        "getResult" => {
            let request_id = request
                .request_id
                .as_deref()
                .ok_or_else(|| anyhow!("requestId is required"))?;
            Ok(state.bridge.get_request_record(request_id)?.to_value())
        }
        "latestResult" => Ok(state
            .bridge
            .latest_request_record()?
            .map(|record| record.to_value())
            .unwrap_or_else(
                || json!({ "status": "empty", "message": "No retained request result." }),
            )),
        "runCommand" => handle_run_command(state, request),
        other => Err(anyhow!("unknown daemon op: {other}")),
    }
}

fn handle_run_command(state: &Arc<DaemonState>, request: DaemonRequest) -> Result<Value> {
    let command = request
        .command
        .clone()
        .ok_or_else(|| anyhow!("command is required"))?;
    let timeout_ms = request.timeout_ms.unwrap_or(state.cfg.result_timeout_ms);
    let poll_interval_ms = request
        .poll_interval_ms
        .unwrap_or(state.cfg.poll_interval_ms);
    let retention_seconds = request
        .retention_seconds
        .unwrap_or(state.cfg.result_retention_seconds);
    if timeout_ms == 0 {
        return Err(anyhow!("timeoutMs must be greater than 0"));
    }
    if poll_interval_ms == 0 {
        return Err(anyhow!("pollIntervalMs must be greater than 0"));
    }
    validate_retention(&state.cfg, retention_seconds)?;

    let target = BridgeTarget {
        instance_id: request.target_instance_id.clone(),
        version: request.target_version.clone(),
    };
    let instance = match state.bridge.resolve_target(&target) {
        Ok(instance) => instance,
        Err(error) => {
            let prepared = state.bridge.prepare_request_with_audit(
                &command,
                retention_seconds,
                None,
                request.audit.clone(),
            )?;
            return Ok(state
                .bridge
                .mark_request_failed(&prepared.record.request_id, error.to_string())?
                .to_value());
        }
    };

    let prepared = state.bridge.prepare_request_with_audit(
        &command,
        retention_seconds,
        Some(instance.clone()),
        request.audit.clone(),
    )?;
    let request_id = prepared.record.request_id.clone();
    let instance_id = instance.instance_id.clone();
    let options = BridgeRunOptions {
        target,
        timeout: Duration::from_millis(timeout_ms),
        poll_interval: Duration::from_millis(poll_interval_ms),
        retention_seconds,
    };

    let bridge = state.bridge.clone();
    let task_request_id = request_id.clone();
    let task_command = command;
    let task = Box::new(move || {
        match bridge.run_prepared_request_on_instance(
            &task_request_id,
            &task_command,
            request.args,
            instance,
            options,
            None,
        ) {
            Ok(outcome) => Ok(outcome.to_value()),
            Err(error) => {
                let message = error.to_string();
                let _ = bridge.mark_request_failed(&task_request_id, message.clone());
                Err(message)
            }
        }
    });
    let response_rx = state
        .scheduler
        .submit(&instance_id, request.global_exclusive, task)?;

    match response_rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(message)) => Err(anyhow!(message)),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(state
            .bridge
            .mark_request_timeout(
                &request_id,
                "Timed out while waiting for daemon queue/execution. Use get-result with requestId to check later."
                    .to_string(),
            )?
            .to_value()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow!("daemon worker disconnected")),
    }
}

fn validate_retention(cfg: &AppConfig, retention_seconds: u64) -> Result<()> {
    if retention_seconds == 0 {
        return Err(anyhow!("resultRetentionSeconds must be greater than 0"));
    }
    if retention_seconds > cfg.result_retention_max_seconds {
        return Err(anyhow!(
            "resultRetentionSeconds exceeds the configured maximum: {} > {}",
            retention_seconds,
            cfg.result_retention_max_seconds
        ));
    }
    Ok(())
}

/// Send one newline-delimited JSON request to the host's daemon.
pub fn call_daemon(cfg: &AppConfig, request: Value, timeout_ms: u64) -> Result<Value> {
    let host = host_spec_by_id(&cfg.host_id)
        .ok_or_else(|| anyhow!("unsupported hostId in daemon config: {}", cfg.host_id))?;
    let connect_timeout = Duration::from_millis(timeout_ms.clamp(100, 2_000));
    let addresses = cfg
        .daemon_addr
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve daemon address {}", cfg.daemon_addr))?;
    let mut last_error = None;
    let mut stream = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, connect_timeout) {
            Ok(value) => {
                stream = Some(value);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let mut stream = stream.with_context(|| {
        format!(
            "failed to connect to {} daemon at {} ({}). Start it with `{} serve-daemon`, or configure daemon_addr for this host",
            host.display_name,
            cfg.daemon_addr,
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "address did not resolve to a socket".to_string()),
            host.binary_name
        )
    })?;
    let io_timeout = Duration::from_millis(timeout_ms.saturating_add(2_000).max(5_000));
    stream
        .set_read_timeout(Some(io_timeout))
        .with_context(|| "failed to set daemon read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .with_context(|| "failed to set daemon write timeout")?;

    stream.write_all(serde_json::to_string(&request)?.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .with_context(|| "failed to read daemon response")?;
    if line.trim().is_empty() {
        return Err(anyhow!("daemon returned an empty response"));
    }
    let response: DaemonResponse =
        serde_json::from_str(line.trim()).with_context(|| "failed to parse daemon response")?;
    if response.ok {
        response
            .value
            .ok_or_else(|| anyhow!("daemon response did not include value"))
    } else {
        Err(anyhow!(
            "{}",
            response
                .error
                .unwrap_or_else(|| "daemon request failed".to_string())
        ))
    }
}

struct DaemonPidFile {
    path: PathBuf,
    pid: u32,
}

impl DaemonPidFile {
    fn create(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let pid = std::process::id();
        let exe = std::env::current_exe()?;
        fs::write(&path, format!("{pid}\n{}\n", exe.display()))?;
        Ok(Self { path, pid })
    }
}

impl Drop for DaemonPidFile {
    fn drop(&mut self) {
        let Ok(raw) = fs::read_to_string(&self.path) else {
            return;
        };
        let expected_pid = self.pid.to_string();
        if raw.lines().next().map(str::trim) == Some(expected_pid.as_str()) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridge_core::{CommandFile, HostInstance};
    use mcp_core::{BridgePaths, HOST_SPECS};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Barrier;
    use tempfile::tempdir;

    fn test_config() -> (AppConfig, tempfile::TempDir, TcpListener) {
        let dir = tempdir().unwrap();
        let root = dir.path().join("ae-mcp-bridge");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let cfg = AppConfig {
            bridge: BridgePaths {
                root_dir: root.clone(),
                command_file: root.join("ae_command.json"),
                result_file: root.join("ae_mcp_result.json"),
            },
            daemon_addr: listener.local_addr().unwrap().to_string(),
            poll_interval_ms: 5,
            result_timeout_ms: 100,
            ..AppConfig::default()
        };
        (cfg, dir, listener)
    }

    fn write_instance(cfg: &AppConfig, instance_id: &str) -> HostInstance {
        let dir = cfg.bridge.root_dir.join("instances").join(instance_id);
        fs::create_dir_all(&dir).unwrap();
        let instance = HostInstance {
            protocol_version: 1,
            instance_id: instance_id.to_string(),
            host_id: cfg.host_id.clone(),
            app_name: Some("After Effects".to_string()),
            app_version: Some("25.0".to_string()),
            display_name: Some("Test host".to_string()),
            project_path: None,
            status: Some("idle".to_string()),
            current_request_id: None,
            bridge_runtime: "test".to_string(),
            lifecycle_mode: None,
            runtime_id: None,
            runtime_started_at: None,
            capabilities: vec!["ping".to_string()],
            bridge_root: cfg.bridge.root_dir.display().to_string(),
            command_file: dir.join("ae_command.json").display().to_string(),
            result_file: dir.join("ae_mcp_result.json").display().to_string(),
            last_heartbeat_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: None,
            heartbeat_path: Some(dir.join("heartbeat.json").display().to_string()),
        };
        fs::write(
            dir.join("heartbeat.json"),
            serde_json::to_vec(&instance).unwrap(),
        )
        .unwrap();
        instance
    }

    fn wait_for_command(path: &PathBuf, previous_request_id: Option<&str>) -> CommandFile {
        for _ in 0..400 {
            if let Ok(raw) = fs::read_to_string(path) {
                if let Ok(command) = serde_json::from_str::<CommandFile>(&raw) {
                    let is_new = command.request_id.as_deref() != previous_request_id;
                    if is_new && command.request_id.is_some() {
                        return command;
                    }
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out waiting for command at {}", path.display());
    }

    fn write_success(path: &PathBuf, command: &CommandFile) {
        fs::write(
            path,
            serde_json::to_vec(&json!({
                "status": "success",
                "_commandExecuted": command.command,
                "_requestId": command.request_id
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn run_test_command(
        cfg: &AppConfig,
        instance_id: &str,
        sequence: u64,
        global_exclusive: bool,
    ) -> Value {
        call_daemon(
            cfg,
            json!({
                "op": "runCommand",
                "command": "ping",
                "args": { "sequence": sequence },
                "targetInstanceId": instance_id,
                "timeoutMs": 2_000,
                "pollIntervalMs": 5,
                "retentionSeconds": 60,
                "globalExclusive": global_exclusive
            }),
            2_000,
        )
        .unwrap()
    }

    #[test]
    fn same_instance_jobs_are_fifo() {
        let scheduler = InstanceScheduler::default();
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut receivers = Vec::new();
        for value in 1..=3 {
            let order = Arc::clone(&order);
            receivers.push(
                scheduler
                    .submit(
                        "same",
                        false,
                        Box::new(move || {
                            order.lock().unwrap().push(value);
                            Ok(json!(value))
                        }),
                    )
                    .unwrap(),
            );
        }
        for receiver in receivers {
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn separate_instances_execute_in_parallel() {
        let scheduler = InstanceScheduler::default();
        let barrier = Arc::new(Barrier::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut receivers = Vec::new();
        for instance in ["a", "b"] {
            let barrier = Arc::clone(&barrier);
            let active = Arc::clone(&active);
            let max_active = Arc::clone(&max_active);
            receivers.push(
                scheduler
                    .submit(
                        instance,
                        false,
                        Box::new(move || {
                            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                            max_active.fetch_max(now, Ordering::SeqCst);
                            barrier.wait();
                            active.fetch_sub(1, Ordering::SeqCst);
                            Ok(json!(null))
                        }),
                    )
                    .unwrap(),
            );
        }
        for receiver in receivers {
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap();
        }
        assert_eq!(max_active.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn global_exclusive_blocks_other_instances() {
        let scheduler = InstanceScheduler::default();
        let release = Arc::new(Barrier::new(2));
        let exclusive_started = Arc::new(AtomicBool::new(false));
        let other_started = Arc::new(AtomicBool::new(false));

        let release_task = Arc::clone(&release);
        let exclusive_flag = Arc::clone(&exclusive_started);
        let exclusive = scheduler
            .submit(
                "a",
                true,
                Box::new(move || {
                    exclusive_flag.store(true, Ordering::SeqCst);
                    release_task.wait();
                    Ok(json!(null))
                }),
            )
            .unwrap();
        while !exclusive_started.load(Ordering::SeqCst) {
            thread::yield_now();
        }

        let other_flag = Arc::clone(&other_started);
        let other = scheduler
            .submit(
                "b",
                false,
                Box::new(move || {
                    other_flag.store(true, Ordering::SeqCst);
                    Ok(json!(null))
                }),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        assert!(!other_started.load(Ordering::SeqCst));
        release.wait();
        exclusive
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        other.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert!(other_started.load(Ordering::SeqCst));
    }

    #[test]
    fn protocol_supports_all_operations_and_timeout_recovery() {
        let (cfg, _dir, listener) = test_config();
        let instance = write_instance(&cfg, "test-instance");
        let server_cfg = cfg.clone();
        thread::spawn(move || run_daemon_with_listener(server_cfg, listener).unwrap());

        let ping = call_daemon(&cfg, json!({ "op": "ping" }), 100).unwrap();
        assert_eq!(
            ping.get("hostId").and_then(Value::as_str),
            Some("aftereffects")
        );
        let instances = call_daemon(&cfg, json!({ "op": "listInstances" }), 100).unwrap();
        assert_eq!(instances.get("count").and_then(Value::as_u64), Some(1));

        let result_path = PathBuf::from(instance.result_file);
        let command_path = PathBuf::from(instance.command_file);
        thread::spawn(move || {
            let request_id = loop {
                if let Ok(raw) = fs::read_to_string(&command_path) {
                    if let Ok(command) = serde_json::from_str::<CommandFile>(&raw) {
                        if let Some(request_id) = command.request_id {
                            break request_id;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(5));
            };
            thread::sleep(Duration::from_millis(80));
            bridge_core::write_json_file(
                &result_path,
                &json!({
                    "status": "success",
                    "_commandExecuted": "ping",
                    "_requestId": request_id
                }),
            )
            .unwrap();
        });

        let timed_out = call_daemon(
            &cfg,
            json!({
                "op": "runCommand",
                "command": "ping",
                "args": {},
                "timeoutMs": 20,
                "pollIntervalMs": 5,
                "retentionSeconds": 60,
                "audit": {
                    "hostId": "aftereffects",
                    "mode": "unsafe",
                    "sourcePath": "C:/scripts/test.jsx",
                    "sourceSha256": "a".repeat(64),
                    "sourceSizeBytes": 12
                }
            }),
            100,
        )
        .unwrap();
        assert_eq!(
            timed_out.get("status").and_then(Value::as_str),
            Some("timeout")
        );
        assert_eq!(
            timed_out
                .pointer("/audit/sourcePath")
                .and_then(Value::as_str),
            Some("C:/scripts/test.jsx")
        );
        let request_id = timed_out
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let recovered = loop {
            let value = call_daemon(
                &cfg,
                json!({ "op": "getResult", "requestId": request_id }),
                200,
            )
            .unwrap();
            if matches!(
                value.get("status").and_then(Value::as_str),
                Some("completed" | "failed" | "lost" | "cancelled")
            ) {
                break value;
            }
            assert_eq!(
                value.get("status").and_then(Value::as_str),
                Some("timeout"),
                "outer timeout must not be downgraded to a worker intermediate state"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "request did not reach a terminal state: {value}"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            recovered.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            recovered
                .pointer("/audit/sourceSha256")
                .and_then(Value::as_str),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        let latest = call_daemon(&cfg, json!({ "op": "latestResult" }), 100).unwrap();
        assert_eq!(latest.get("requestId"), recovered.get("requestId"));
    }

    #[test]
    fn duplicate_listener_error_identifies_host_and_address() {
        let (cfg, _dir, _listener) = test_config();
        let error = bind_listener(&cfg).expect_err("address is already bound");
        let message = error.to_string();
        assert!(message.contains(&cfg.host_id));
        assert!(message.contains(&cfg.daemon_addr));
        assert!(message.contains("another daemon may already be running"));
    }

    #[test]
    fn tcp_requests_are_fifo_within_one_instance() {
        let (cfg, _dir, listener) = test_config();
        let instance = write_instance(&cfg, "fifo");
        let server_cfg = cfg.clone();
        thread::spawn(move || run_daemon_with_listener(server_cfg, listener).unwrap());

        let command_path = PathBuf::from(instance.command_file);
        let result_path = PathBuf::from(instance.result_file);
        let (first_seen_tx, first_seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (order_tx, order_rx) = mpsc::channel();
        let host = thread::spawn(move || {
            let first = wait_for_command(&command_path, None);
            order_tx
                .send(first.args.get("sequence").and_then(Value::as_u64).unwrap())
                .unwrap();
            first_seen_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            write_success(&result_path, &first);

            let second = wait_for_command(&command_path, first.request_id.as_deref());
            order_tx
                .send(second.args.get("sequence").and_then(Value::as_u64).unwrap())
                .unwrap();
            write_success(&result_path, &second);
        });

        let first_cfg = cfg.clone();
        let first = thread::spawn(move || run_test_command(&first_cfg, "fifo", 1, false));
        first_seen_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let second_cfg = cfg.clone();
        let second = thread::spawn(move || run_test_command(&second_cfg, "fifo", 2, false));
        thread::sleep(Duration::from_millis(40));
        assert_eq!(order_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 1);
        assert!(
            order_rx.try_recv().is_err(),
            "second job started before first completed"
        );
        release_tx.send(()).unwrap();

        assert_eq!(
            first.join().unwrap().get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            second.join().unwrap().get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(order_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 2);
        host.join().unwrap();
    }

    #[test]
    fn tcp_requests_run_in_parallel_on_separate_instances() {
        let (cfg, _dir, listener) = test_config();
        let first_instance = write_instance(&cfg, "parallel-a");
        let second_instance = write_instance(&cfg, "parallel-b");
        let server_cfg = cfg.clone();
        thread::spawn(move || run_daemon_with_listener(server_cfg, listener).unwrap());

        let (seen_tx, seen_rx) = mpsc::channel();
        let (release_a_tx, release_a_rx) = mpsc::channel();
        let (release_b_tx, release_b_rx) = mpsc::channel();
        for (instance, release_rx) in [
            (first_instance, release_a_rx),
            (second_instance, release_b_rx),
        ] {
            let seen_tx = seen_tx.clone();
            thread::spawn(move || {
                let command = wait_for_command(&PathBuf::from(&instance.command_file), None);
                seen_tx.send(instance.instance_id).unwrap();
                release_rx.recv().unwrap();
                write_success(&PathBuf::from(instance.result_file), &command);
            });
        }

        let first_cfg = cfg.clone();
        let first = thread::spawn(move || run_test_command(&first_cfg, "parallel-a", 1, false));
        let second_cfg = cfg.clone();
        let second = thread::spawn(move || run_test_command(&second_cfg, "parallel-b", 2, false));
        let mut seen = vec![
            seen_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            seen_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        ];
        seen.sort();
        assert_eq!(seen, vec!["parallel-a", "parallel-b"]);
        release_a_tx.send(()).unwrap();
        release_b_tx.send(()).unwrap();
        assert_eq!(
            first.join().unwrap().get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            second.join().unwrap().get("status").and_then(Value::as_str),
            Some("completed")
        );
    }

    #[test]
    fn tcp_global_exclusive_blocks_other_instances() {
        let (cfg, _dir, listener) = test_config();
        let exclusive_instance = write_instance(&cfg, "exclusive");
        let normal_instance = write_instance(&cfg, "normal");
        let server_cfg = cfg.clone();
        thread::spawn(move || run_daemon_with_listener(server_cfg, listener).unwrap());

        let (exclusive_seen_tx, exclusive_seen_rx) = mpsc::channel();
        let (normal_seen_tx, normal_seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        thread::spawn(move || {
            let command = wait_for_command(&PathBuf::from(&exclusive_instance.command_file), None);
            exclusive_seen_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            write_success(&PathBuf::from(exclusive_instance.result_file), &command);
        });
        thread::spawn(move || {
            let command = wait_for_command(&PathBuf::from(&normal_instance.command_file), None);
            normal_seen_tx.send(()).unwrap();
            write_success(&PathBuf::from(normal_instance.result_file), &command);
        });

        let exclusive_cfg = cfg.clone();
        let exclusive =
            thread::spawn(move || run_test_command(&exclusive_cfg, "exclusive", 1, true));
        exclusive_seen_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        let normal_cfg = cfg.clone();
        let normal = thread::spawn(move || run_test_command(&normal_cfg, "normal", 2, false));
        assert!(
            normal_seen_rx
                .recv_timeout(Duration::from_millis(80))
                .is_err(),
            "normal job started while global-exclusive job was running"
        );
        release_tx.send(()).unwrap();
        normal_seen_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            exclusive
                .join()
                .unwrap()
                .get("status")
                .and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            normal.join().unwrap().get("status").and_then(Value::as_str),
            Some("completed")
        );
    }

    #[test]
    fn unavailable_daemon_error_is_host_specific() {
        for host in HOST_SPECS {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let unavailable_addr = listener.local_addr().unwrap().to_string();
            drop(listener);
            let mut cfg = AppConfig::load_for_host(None, *host).unwrap();
            cfg.daemon_addr = unavailable_addr.clone();
            let error = call_daemon(&cfg, json!({ "op": "ping" }), 20).unwrap_err();
            let message = error.to_string();
            assert!(message.contains(host.display_name));
            assert!(message.contains(host.binary_name));
            assert!(message.contains(&unavailable_addr));
            assert!(message.contains("serve-daemon"));
        }
    }
}
