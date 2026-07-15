//! Reusable mock Adobe host for bridge protocol contract tests.
//!
//! The fixture exercises the real TCP daemon and file bridge. It deliberately
//! depends only on [`mcp_core::HostSpec`], so every current host and a future
//! InDesign host can run the same contract without copying test code.

use anyhow::{anyhow, Context, Result};
use bridge_core::{write_json_file, CommandFile, HostInstance};
use mcp_core::{AppConfig, BridgePaths, HostSpec};
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

const POLL_INTERVAL: Duration = Duration::from_millis(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(20);

/// Response behavior applied to the next command observed by a mock host.
#[derive(Debug, Clone)]
pub enum ResponsePlan {
    Success {
        after: Duration,
        value: Value,
    },
    /// Write deliberately incomplete JSON, then replace it with a valid result.
    PartialJsonThenSuccess {
        invalid_for: Duration,
        value: Value,
    },
    /// Observe the command but leave it unanswered.
    Ignore,
}

impl ResponsePlan {
    pub fn success(value: Value) -> Self {
        Self::Success {
            after: Duration::ZERO,
            value,
        }
    }

    pub fn delayed(after: Duration, value: Value) -> Self {
        Self::Success { after, value }
    }
}

/// One isolated bridge root for a single host contract scenario.
pub struct ProtocolFixture {
    _temp_dir: TempDir,
    host: HostSpec,
    config: AppConfig,
}

impl ProtocolFixture {
    pub fn new(host: HostSpec) -> Result<Self> {
        let temp_dir = tempfile::tempdir().context("failed to create protocol fixture")?;
        let root = temp_dir.path().join(host.bridge_root_name);
        let config = AppConfig {
            host_id: host.id.to_string(),
            bridge: BridgePaths {
                root_dir: root.clone(),
                command_file: root.join(host.command_file_name),
                result_file: root.join(host.result_file_name),
            },
            poll_interval_ms: POLL_INTERVAL.as_millis() as u64,
            result_timeout_ms: 500,
            result_retention_seconds: 60,
            result_retention_max_seconds: 3_600,
            instance_heartbeat_stale_ms: 120,
            daemon_addr: "127.0.0.1:0".to_string(),
            log_level: "error".to_string(),
            script_files: Default::default(),
        };
        Ok(Self {
            _temp_dir: temp_dir,
            host,
            config,
        })
    }

    pub fn host(&self) -> HostSpec {
        self.host
    }

    pub fn bridge_root(&self) -> &Path {
        &self.config.bridge.root_dir
    }

    /// Start a fresh daemon endpoint over the existing bridge root.
    ///
    /// Calling this twice models a client reconnecting to a restarted broker;
    /// retained request state is shared through the registry directory.
    pub fn start_daemon(&self) -> Result<DaemonEndpoint> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .context("failed to reserve an ephemeral daemon port")?;
        let mut config = self.config.clone();
        config.daemon_addr = listener.local_addr()?.to_string();
        let server_config = config.clone();
        thread::spawn(move || {
            let _ = daemon_core::run_daemon_server_with_listener(server_config, listener);
        });

        let endpoint = DaemonEndpoint { config };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if endpoint.call(json!({ "op": "ping" }), 100).is_ok() {
                return Ok(endpoint);
            }
            if Instant::now() >= deadline {
                return Err(anyhow!("daemon did not accept requests before deadline"));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub fn start_mock_host(&self, instance_id: &str) -> Result<MockHost> {
        MockHost::start(&self.config, self.host, instance_id)
    }

    pub fn instance_dir(&self, instance_id: &str) -> PathBuf {
        self.bridge_root().join("instances").join(instance_id)
    }

    pub fn write_malformed_heartbeat(&self, instance_id: &str) -> Result<()> {
        let dir = self.instance_dir(instance_id);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("heartbeat.json"), br#"{"instanceId":"#)?;
        Ok(())
    }
}

/// Client endpoint for one daemon listener.
#[derive(Debug, Clone)]
pub struct DaemonEndpoint {
    config: AppConfig,
}

impl DaemonEndpoint {
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn call(&self, request: Value, timeout_ms: u64) -> Result<Value> {
        daemon_core::call_daemon(&self.config, request, timeout_ms)
    }

    /// Send one daemon request in explicit chunks and return the raw envelope.
    pub fn send_raw_chunks(&self, chunks: &[(&str, Duration)]) -> Result<Value> {
        let mut stream = TcpStream::connect(&self.config.daemon_addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        for (chunk, pause_after) in chunks {
            stream.write_all(chunk.as_bytes())?;
            stream.flush()?;
            if !pause_after.is_zero() {
                thread::sleep(*pause_after);
            }
        }
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        serde_json::from_str(&line).context("daemon returned an invalid response envelope")
    }
}

/// A host-neutral mock panel/runtime that maintains a heartbeat and consumes
/// the instance-specific command file.
pub struct MockHost {
    instance: HostInstance,
    plans: Arc<Mutex<VecDeque<ResponsePlan>>>,
    observed: Arc<Mutex<Vec<CommandFile>>>,
    heartbeat_enabled: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl MockHost {
    fn start(config: &AppConfig, host: HostSpec, instance_id: &str) -> Result<Self> {
        let dir = config.bridge.root_dir.join("instances").join(instance_id);
        fs::create_dir_all(&dir)?;
        let instance = HostInstance {
            protocol_version: 1,
            instance_id: instance_id.to_string(),
            host_id: host.id.to_string(),
            app_name: Some(host.display_name.to_string()),
            app_version: Some("contract-test".to_string()),
            display_name: Some(format!("{} mock", host.display_name)),
            project_path: None,
            status: Some("idle".to_string()),
            current_request_id: None,
            bridge_runtime: host.bridge_runtime.to_string(),
            lifecycle_mode: None,
            runtime_id: None,
            runtime_started_at: None,
            capabilities: vec!["ping".to_string(), "contract-test".to_string()],
            bridge_root: config.bridge.root_dir.display().to_string(),
            command_file: dir.join(host.command_file_name).display().to_string(),
            result_file: dir.join(host.result_file_name).display().to_string(),
            last_heartbeat_at: timestamp(),
            updated_at: Some(timestamp()),
            heartbeat_path: Some(dir.join("heartbeat.json").display().to_string()),
        };
        write_json_file(&dir.join("heartbeat.json"), &instance)?;

        let plans = Arc::new(Mutex::new(VecDeque::new()));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let heartbeat_enabled = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));

        let worker_instance = instance.clone();
        let worker_plans = Arc::clone(&plans);
        let worker_observed = Arc::clone(&observed);
        let worker_heartbeat_enabled = Arc::clone(&heartbeat_enabled);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let heartbeat_path = PathBuf::from(
                worker_instance
                    .heartbeat_path
                    .as_deref()
                    .expect("mock heartbeat path"),
            );
            let command_path = PathBuf::from(&worker_instance.command_file);
            let result_path = PathBuf::from(&worker_instance.result_file);
            let mut seen = HashSet::new();
            let mut next_heartbeat = Instant::now();
            let mut response_workers = Vec::new();

            while !worker_stop.load(Ordering::SeqCst) {
                if worker_heartbeat_enabled.load(Ordering::SeqCst)
                    && Instant::now() >= next_heartbeat
                {
                    let mut heartbeat = worker_instance.clone();
                    heartbeat.last_heartbeat_at = timestamp();
                    heartbeat.updated_at = Some(heartbeat.last_heartbeat_at.clone());
                    let _ = write_json_file(&heartbeat_path, &heartbeat);
                    next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;
                }

                if let Ok(raw) = fs::read_to_string(&command_path) {
                    if let Ok(command) = serde_json::from_str::<CommandFile>(&raw) {
                        if let Some(request_id) = command.request_id.as_deref() {
                            if seen.insert(request_id.to_string()) {
                                worker_observed.lock().unwrap().push(command.clone());
                                let plan =
                                    worker_plans.lock().unwrap().pop_front().unwrap_or_else(|| {
                                        ResponsePlan::success(json!({ "pong": true }))
                                    });
                                if !matches!(plan, ResponsePlan::Ignore) {
                                    let response_instance = worker_instance.clone();
                                    let response_path = result_path.clone();
                                    response_workers.push(thread::spawn(move || {
                                        respond(response_path, response_instance, command, plan);
                                    }));
                                }
                            }
                        }
                    }
                }

                thread::sleep(POLL_INTERVAL);
            }

            for response_worker in response_workers {
                let _ = response_worker.join();
            }
        });

        Ok(Self {
            instance,
            plans,
            observed,
            heartbeat_enabled,
            stop,
            worker: Some(worker),
        })
    }

    pub fn instance(&self) -> &HostInstance {
        &self.instance
    }

    pub fn enqueue(&self, plan: ResponsePlan) {
        self.plans.lock().unwrap().push_back(plan);
    }

    pub fn observed_commands(&self) -> Vec<CommandFile> {
        self.observed.lock().unwrap().clone()
    }

    pub fn pause_heartbeat(&self) {
        self.heartbeat_enabled.store(false, Ordering::SeqCst);
    }

    pub fn resume_heartbeat(&self) {
        self.heartbeat_enabled.store(true, Ordering::SeqCst);
    }
}

impl Drop for MockHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn respond(result_path: PathBuf, instance: HostInstance, command: CommandFile, plan: ResponsePlan) {
    let value = match plan {
        ResponsePlan::Success { after, value } => {
            thread::sleep(after);
            value
        }
        ResponsePlan::PartialJsonThenSuccess { invalid_for, value } => {
            let _ = fs::write(&result_path, br#"{"status":"success"#);
            thread::sleep(invalid_for);
            value
        }
        ResponsePlan::Ignore => return,
    };
    let _ = write_json_file(
        &result_path,
        &json!({
            "status": "success",
            "result": value,
            "_requestId": command.request_id,
            "_commandExecuted": command.command,
            "_hostInstance": {
                "instanceId": instance.instance_id,
                "hostId": instance.host_id,
                "appVersion": instance.app_version,
                "bridgeRuntime": instance.bridge_runtime
            }
        }),
    );
}

fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}
