use anyhow::{anyhow, Context, Result};
use bridge_core::{BridgeClient, BridgeRunOptions, BridgeTarget, HostInstance};
use mcp_core::AppConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
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
}

#[derive(Debug, Serialize)]
struct DaemonResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct DaemonState {
    cfg: AppConfig,
    bridge: BridgeClient,
    workers: Mutex<HashMap<String, mpsc::Sender<DaemonJob>>>,
    global_gate: Arc<RwLock<()>>,
}

struct DaemonJob {
    request_id: String,
    command: String,
    args: Value,
    instance: HostInstance,
    options: BridgeRunOptions,
    global_exclusive: bool,
    response_tx: mpsc::Sender<Result<Value, String>>,
}

pub fn run_daemon_server(cfg: AppConfig) -> Result<()> {
    let listener = TcpListener::bind(&cfg.daemon_addr)
        .with_context(|| format!("failed to bind daemon listener at {}", cfg.daemon_addr))?;
    let bridge = BridgeClient::new(cfg.clone())?;
    let state = Arc::new(DaemonState {
        cfg,
        bridge,
        workers: Mutex::new(HashMap::new()),
        global_gate: Arc::new(RwLock::new(())),
    });

    info!("serve-daemon listening on {}", state.cfg.daemon_addr);
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

fn handle_client(mut stream: TcpStream, state: Arc<DaemonState>) -> Result<()> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(stream.try_clone()?);
        reader
            .read_line(&mut line)
            .with_context(|| "failed to read daemon request")?;
    }

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

    let raw = serde_json::to_string(&response)?;
    stream.write_all(raw.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn handle_request(state: &Arc<DaemonState>, request: DaemonRequest) -> Result<Value> {
    match request.op.as_str() {
        "ping" => Ok(json!({ "status": "ok" })),
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
    validate_retention(&state.cfg, retention_seconds)?;

    let target = BridgeTarget {
        instance_id: request.target_instance_id.clone(),
        version: request.target_version.clone(),
    };

    let instance = match state.bridge.resolve_target(&target) {
        Ok(instance) => instance,
        Err(error) => {
            let prepared = state
                .bridge
                .prepare_request(&command, retention_seconds, None)?;
            let record = state
                .bridge
                .mark_request_failed(&prepared.record.request_id, error.to_string())?;
            return Ok(record.to_value());
        }
    };

    let prepared =
        state
            .bridge
            .prepare_request(&command, retention_seconds, Some(instance.clone()))?;
    let request_id = prepared.record.request_id.clone();
    let options = BridgeRunOptions {
        target,
        timeout: Duration::from_millis(timeout_ms),
        poll_interval: Duration::from_millis(poll_interval_ms),
        retention_seconds,
    };

    let worker = get_or_spawn_worker(state, &instance);
    let (response_tx, response_rx) = mpsc::channel();
    worker
        .send(DaemonJob {
            request_id: request_id.clone(),
            command,
            args: request.args,
            instance,
            options,
            global_exclusive: request.global_exclusive,
            response_tx,
        })
        .map_err(|_| anyhow!("failed to enqueue daemon job"))?;

    match response_rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(message)) => Err(anyhow!(message)),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let record = state.bridge.mark_request_timeout(
                &request_id,
                "Timed out while waiting for daemon queue/execution. Use get-jsx-result with requestId to check later."
                    .to_string(),
            )?;
            Ok(record.to_value())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(anyhow!("daemon worker disconnected")),
    }
}

fn get_or_spawn_worker(
    state: &Arc<DaemonState>,
    instance: &HostInstance,
) -> mpsc::Sender<DaemonJob> {
    let mut workers = state.workers.lock().expect("workers mutex poisoned");
    if let Some(sender) = workers.get(&instance.instance_id) {
        return sender.clone();
    }

    let (sender, receiver) = mpsc::channel::<DaemonJob>();
    workers.insert(instance.instance_id.clone(), sender.clone());

    let bridge = state.bridge.clone();
    let global_gate = Arc::clone(&state.global_gate);
    let instance_id = instance.instance_id.clone();
    thread::spawn(move || run_instance_worker(instance_id, bridge, global_gate, receiver));
    sender
}

fn run_instance_worker(
    instance_id: String,
    bridge: BridgeClient,
    global_gate: Arc<RwLock<()>>,
    receiver: mpsc::Receiver<DaemonJob>,
) {
    info!("daemon worker started for AE instance {instance_id}");
    for job in receiver {
        let result = if job.global_exclusive {
            let _gate = global_gate.write().expect("global gate poisoned");
            run_job(&bridge, job)
        } else {
            let _gate = global_gate.read().expect("global gate poisoned");
            run_job(&bridge, job)
        };
        if let Err(error) = result {
            error!("daemon worker job failed: {error}");
        }
    }
}

fn run_job(bridge: &BridgeClient, job: DaemonJob) -> Result<()> {
    let result = bridge.run_prepared_request_on_instance(
        &job.request_id,
        &job.command,
        job.args,
        job.instance,
        job.options,
        None,
    );
    let response = match result {
        Ok(outcome) => Ok(outcome.to_value()),
        Err(error) => {
            let message = error.to_string();
            let _ = bridge.mark_request_failed(&job.request_id, message.clone());
            Err(message)
        }
    };
    let _ = job.response_tx.send(response);
    Ok(())
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
