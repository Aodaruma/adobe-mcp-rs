use anyhow::{anyhow, Context, Result};
use mcp_core::{host_spec_by_id, AppConfig, HostSpec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
const BROKER_LOCK_STALE_SECONDS: u64 = 86_400;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommandStatus {
    Pending,
    Running,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandFile {
    pub command: String,
    #[serde(default)]
    pub args: Value,
    pub timestamp: String,
    pub status: CommandStatus,
    #[serde(default, rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaitingResult {
    pub status: String,
    pub message: String,
    pub timestamp: String,
    #[serde(default, rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostInstance {
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u32,
    pub instance_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bridge_runtime: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub bridge_root: String,
    pub command_file: String,
    pub result_file: String,
    pub last_heartbeat_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_path: Option<String>,
}

/// Rust API compatibility alias. New code should use [`HostInstance`].
#[deprecated(note = "use HostInstance")]
pub type AeInstance = HostInstance;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDiscoveryIssue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_name: Option<String>,
    pub heartbeat_path: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDiscoveryReport {
    pub instances: Vec<HostInstance>,
    pub inactive_instances: Vec<InstanceDiscoveryIssue>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BridgeTarget {
    pub instance_id: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BridgeRunOptions {
    pub target: BridgeTarget,
    pub timeout: Duration,
    pub poll_interval: Duration,
    pub retention_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestRecord {
    pub request_id: String,
    pub command: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    #[serde(default, alias = "aeInstance", skip_serializing_if = "Option::is_none")]
    pub host_instance: Option<HostInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_raw: Option<String>,
}

impl RequestRecord {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| {
            json!({
                "requestId": self.request_id,
                "status": self.status,
                "message": "failed to serialize request record"
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct BridgeRunOutcome {
    pub record: RequestRecord,
    pub registry_path: PathBuf,
}

impl BridgeRunOutcome {
    pub fn to_value(&self) -> Value {
        let mut value = self.record.to_value();
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "registryPath".to_string(),
                Value::String(self.registry_path.display().to_string()),
            );
        }
        value
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CurrentRequest {
    request_id: String,
    command: String,
    status: String,
    dispatched_at: String,
}

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("no result file found at {0}")]
    MissingResultFile(String),
    #[error("timed out waiting for bridge result{0}")]
    Timeout(String),
}

#[derive(Debug, Clone)]
pub struct BridgeClient {
    cfg: AppConfig,
    host: HostSpec,
}

impl BridgeClient {
    pub fn new(cfg: AppConfig) -> Result<Self> {
        let host = host_spec_by_id(&cfg.host_id)
            .ok_or_else(|| anyhow!("unsupported hostId in bridge config: {}", cfg.host_id))?;
        ensure_bridge_dir(&cfg)?;
        fs::create_dir_all(cfg.bridge.root_dir.join("instances")).with_context(|| {
            format!(
                "failed to create instances directory: {}",
                cfg.bridge.root_dir.join("instances").display()
            )
        })?;
        fs::create_dir_all(cfg.bridge.root_dir.join("registry")).with_context(|| {
            format!(
                "failed to create registry directory: {}",
                cfg.bridge.root_dir.join("registry").display()
            )
        })?;
        Ok(Self { cfg, host })
    }

    pub fn config(&self) -> &AppConfig {
        &self.cfg
    }

    /// Compatibility path for old queued commands. New MCP calls should use run_command_sync.
    pub fn write_command_file(&self, command: &str, args: Value) -> Result<()> {
        let payload = CommandFile {
            command: command.to_string(),
            args,
            timestamp: chrono_like_timestamp(),
            status: CommandStatus::Pending,
            request_id: None,
        };

        let active = self
            .list_active_instances(Duration::from_millis(self.cfg.instance_heartbeat_stale_ms))?;
        if active.len() == 1 {
            return write_json_file(&instance_command_path(&active[0]), &payload);
        }

        write_json_file(&self.cfg.bridge.command_file, &payload)
    }

    pub fn clear_results_file(&self) -> Result<()> {
        let payload = WaitingResult {
            status: "waiting".to_string(),
            message: "Waiting for new result from the host application...".to_string(),
            timestamp: chrono_like_timestamp(),
            request_id: None,
        };
        write_json_file(&self.cfg.bridge.result_file, &payload)
    }

    pub fn read_results_raw(&self) -> Result<String> {
        let path = &self.cfg.bridge.result_file;
        if !path.exists() {
            return Err(BridgeError::MissingResultFile(path.display().to_string()).into());
        }
        fs::read_to_string(path)
            .with_context(|| format!("failed to read result file: {}", path.display()))
    }

    pub fn read_results_with_stale_warning(&self, stale_threshold: Duration) -> Result<String> {
        if let Some(record) = self.latest_request_record()? {
            return serde_json::to_string_pretty(&record.to_value())
                .with_context(|| "failed to serialize latest request record");
        }

        let path = &self.cfg.bridge.result_file;
        if !path.exists() {
            return Ok(json_text(&serde_json::json!({
                "error": "No results file found. Please run a script in the host application first."
            }))?);
        }

        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to stat file: {}", path.display()))?;
        let modified = metadata
            .modified()
            .unwrap_or_else(|_| SystemTime::now() - stale_threshold);
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read result file: {}", path.display()))?;

        if let Ok(age) = SystemTime::now().duration_since(modified) {
            if age > stale_threshold {
                return Ok(json_text(&serde_json::json!({
                    "warning": "Result file appears to be stale (not recently updated).",
                    "message": "This could indicate the host application is not properly writing results or the MCP Bridge panel is not running.",
                    "ageSeconds": age.as_secs(),
                    "originalContent": content
                }))?);
            }
        }

        Ok(content)
    }

    pub fn wait_for_bridge_result(
        &self,
        expected_command: Option<&str>,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<String> {
        let start = SystemTime::now();
        loop {
            if self.cfg.bridge.result_file.exists() {
                let content = self.read_results_raw()?;
                if content.trim().is_empty() {
                    // continue polling
                } else if let Ok(value) = serde_json::from_str::<Value>(&content) {
                    let matched = expected_command
                        .map(|cmd| {
                            value.get("_commandExecuted").and_then(Value::as_str) == Some(cmd)
                        })
                        .unwrap_or(true);
                    if matched {
                        return Ok(content);
                    }
                }
            }

            if elapsed_since_system(start) >= timeout {
                let suffix = expected_command
                    .map(|x| format!(" for command '{x}'"))
                    .unwrap_or_default();
                return Err(BridgeError::Timeout(suffix).into());
            }

            thread::sleep(poll_interval);
        }
    }

    pub fn list_active_instances(&self, stale_threshold: Duration) -> Result<Vec<HostInstance>> {
        Ok(self.discover_instances(stale_threshold)?.instances)
    }

    pub fn discover_instances(&self, stale_threshold: Duration) -> Result<InstanceDiscoveryReport> {
        let mut instances = Vec::new();
        let mut inactive_instances = Vec::new();
        let dir = self.instances_dir();
        if !dir.exists() {
            return Ok(InstanceDiscoveryReport {
                instances,
                inactive_instances,
            });
        }

        for entry in fs::read_dir(&dir)
            .with_context(|| format!("failed to read instances directory: {}", dir.display()))?
        {
            let entry = entry?;
            let folder_name = entry
                .file_name()
                .to_str()
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            let heartbeat_path = entry.path().join("heartbeat.json");
            if !heartbeat_path.exists() {
                inactive_instances.push(instance_discovery_issue(
                    folder_name,
                    &heartbeat_path,
                    "missing heartbeat.json",
                    None,
                    None,
                ));
                continue;
            }
            let metadata = match fs::metadata(&heartbeat_path) {
                Ok(value) => value,
                Err(error) => {
                    inactive_instances.push(instance_discovery_issue(
                        folder_name,
                        &heartbeat_path,
                        format!("failed to stat heartbeat.json: {error}"),
                        None,
                        None,
                    ));
                    continue;
                }
            };
            let modified = metadata
                .modified()
                .unwrap_or_else(|_| SystemTime::now() - stale_threshold);
            let age = SystemTime::now()
                .duration_since(modified)
                .unwrap_or_else(|_| Duration::from_secs(0));
            let raw = match fs::read_to_string(&heartbeat_path) {
                Ok(value) => value,
                Err(error) => {
                    inactive_instances.push(instance_discovery_issue(
                        folder_name,
                        &heartbeat_path,
                        format!("failed to read heartbeat.json: {error}"),
                        Some(age),
                        None,
                    ));
                    continue;
                }
            };
            let mut instance = match serde_json::from_str::<HostInstance>(&raw) {
                Ok(value) => value,
                Err(error) => {
                    inactive_instances.push(instance_discovery_issue(
                        folder_name,
                        &heartbeat_path,
                        format!("failed to parse heartbeat.json: {error}"),
                        Some(age),
                        None,
                    ));
                    continue;
                }
            };
            if instance.host_id.is_empty() {
                instance.host_id = self.host.id.to_string();
            }
            if instance.bridge_runtime.is_empty() {
                instance.bridge_runtime = self.host.bridge_runtime.to_string();
            }
            if instance.updated_at.is_none() {
                instance.updated_at = Some(instance.last_heartbeat_at.clone());
            }
            let parsed_issue = instance_discovery_issue_from_instance(
                folder_name.clone(),
                &heartbeat_path,
                Some(age),
                &instance,
            );
            if age > stale_threshold {
                inactive_instances.push(InstanceDiscoveryIssue {
                    reason: "heartbeat is stale".to_string(),
                    ..parsed_issue
                });
                continue;
            }
            if instance.instance_id.trim().is_empty() {
                inactive_instances.push(InstanceDiscoveryIssue {
                    reason: "heartbeat has empty instanceId".to_string(),
                    ..parsed_issue
                });
                continue;
            }
            if instance.bridge_root.trim().is_empty()
                || instance.command_file.trim().is_empty()
                || instance.result_file.trim().is_empty()
            {
                inactive_instances.push(InstanceDiscoveryIssue {
                    reason: "heartbeat is missing bridgeRoot, commandFile, or resultFile"
                        .to_string(),
                    ..parsed_issue
                });
                continue;
            }
            instance.heartbeat_path = Some(heartbeat_path.display().to_string());
            instances.push(instance);
        }

        instances.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        inactive_instances.sort_by(|a, b| {
            a.folder_name
                .as_deref()
                .unwrap_or_default()
                .cmp(b.folder_name.as_deref().unwrap_or_default())
        });
        Ok(InstanceDiscoveryReport {
            instances,
            inactive_instances,
        })
    }

    pub fn run_command_sync(
        &self,
        command: &str,
        args: Value,
        options: BridgeRunOptions,
    ) -> Result<BridgeRunOutcome> {
        let prepared = self.prepare_request(command, options.retention_seconds, None)?;
        let registry_path = prepared.registry_path.clone();
        let mut record = prepared.record;

        let started = Instant::now();
        let _lock = match self.acquire_broker_lock(options.timeout, options.poll_interval) {
            Ok(lock) => lock,
            Err(error) => {
                record.status = "timeout".to_string();
                record.updated_at = chrono_like_timestamp();
                record.message = Some(error.to_string());
                self.write_request_record(&record)?;
                return Ok(BridgeRunOutcome {
                    record,
                    registry_path,
                });
            }
        };
        self.cleanup_registry()?;

        let instance = match self.resolve_target(&options.target) {
            Ok(instance) => instance,
            Err(error) => {
                record = self.mark_request_failed(&record.request_id, error.to_string())?;
                return Ok(BridgeRunOutcome {
                    record,
                    registry_path,
                });
            }
        };
        self.run_prepared_request_on_instance(
            &record.request_id,
            command,
            args,
            instance,
            options,
            Some(started),
        )
    }

    pub fn prepare_request(
        &self,
        command: &str,
        retention_seconds: u64,
        host_instance: Option<HostInstance>,
    ) -> Result<BridgeRunOutcome> {
        self.cleanup_registry()?;
        let request_id = generate_request_id();
        let created_at = chrono_like_timestamp();
        let registry_path = self.registry_path(&request_id);
        let record = RequestRecord {
            request_id: request_id.clone(),
            command: command.to_string(),
            status: "queued".to_string(),
            created_at: created_at.clone(),
            updated_at: created_at,
            expires_at: timestamp_after_seconds(retention_seconds),
            host_instance,
            message: None,
            result: None,
            result_raw: None,
        };
        self.write_request_record(&record)?;
        Ok(BridgeRunOutcome {
            record,
            registry_path,
        })
    }

    pub fn mark_request_timeout(&self, request_id: &str, message: String) -> Result<RequestRecord> {
        let mut record = self.read_request_record(request_id)?;
        record.status = "timeout".to_string();
        record.updated_at = chrono_like_timestamp();
        record.message = Some(message);
        self.write_request_record(&record)?;
        Ok(record)
    }

    pub fn mark_request_failed(&self, request_id: &str, message: String) -> Result<RequestRecord> {
        let mut record = self.read_request_record(request_id)?;
        record.status = "failed".to_string();
        record.updated_at = chrono_like_timestamp();
        record.message = Some(message);
        self.write_request_record(&record)?;
        Ok(record)
    }

    pub fn resolve_target(&self, target: &BridgeTarget) -> Result<HostInstance> {
        let active = self
            .list_active_instances(Duration::from_millis(self.cfg.instance_heartbeat_stale_ms))?;
        if active.is_empty() {
            return Err(anyhow!(
                "No active {} bridge instances were found. {}",
                self.host.display_name,
                self.host.bridge_setup_hint
            ));
        }

        if let Some(instance_id) = target.instance_id.as_deref() {
            return active
                .into_iter()
                .find(|instance| instance.instance_id == instance_id)
                .ok_or_else(|| anyhow!("targetInstanceId not found or stale: {instance_id}"));
        }

        let filtered = if let Some(version) = target.version.as_deref() {
            active
                .into_iter()
                .filter(|instance| instance_matches_version(instance, version))
                .collect::<Vec<_>>()
        } else {
            active
        };

        match filtered.len() {
            0 => Err(anyhow!(
                "No active {} instance matched the requested target",
                self.host.display_name
            )),
            1 => Ok(filtered.into_iter().next().expect("single instance")),
            _ => Err(anyhow!(
                "Multiple active {} instances matched. Specify targetInstanceId or targetVersion. Active instances: {}",
                self.host.display_name,
                serde_json::to_string(&filtered)?
            )),
        }
    }

    pub fn run_prepared_request_on_instance(
        &self,
        request_id: &str,
        command: &str,
        args: Value,
        instance: HostInstance,
        options: BridgeRunOptions,
        started_at: Option<Instant>,
    ) -> Result<BridgeRunOutcome> {
        let registry_path = self.registry_path(request_id);
        let mut record = self.read_request_record(request_id)?;
        let started = started_at.unwrap_or_else(Instant::now);
        record.host_instance = Some(instance.clone());
        record.updated_at = chrono_like_timestamp();
        self.write_request_record(&record)?;

        while self.is_instance_busy(&instance)? {
            if started.elapsed() >= options.timeout {
                record.status = "timeout".to_string();
                record.updated_at = chrono_like_timestamp();
                record.message = Some(
                    format!(
                        "Timed out while waiting for the target {} instance to become available. Use get-jsx-result with requestId to check later.",
                        self.host.display_name
                    ),
                );
                self.write_request_record(&record)?;
                return Ok(BridgeRunOutcome {
                    record,
                    registry_path,
                });
            }
            thread::sleep(options.poll_interval);
        }

        record.status = "dispatched".to_string();
        record.updated_at = chrono_like_timestamp();
        self.write_request_record(&record)?;

        self.write_instance_waiting_result(&instance, &request_id)?;
        self.write_current_request(&instance, &request_id, command)?;
        self.write_instance_command_file(&instance, &request_id, command, args)?;

        record.status = "running".to_string();
        record.updated_at = chrono_like_timestamp();
        self.write_request_record(&record)?;

        loop {
            if let Some((raw, parsed)) =
                self.try_read_instance_result(&instance, &request_id, Some(command))?
            {
                record.status = result_record_status(&parsed).to_string();
                record.updated_at = chrono_like_timestamp();
                record.result = Some(parsed);
                record.result_raw = Some(raw);
                record.message = None;
                self.write_request_record(&record)?;
                self.clear_current_request(&instance)?;
                return Ok(BridgeRunOutcome {
                    record,
                    registry_path,
                });
            }

            if started.elapsed() >= options.timeout {
                record.status = "timeout".to_string();
                record.updated_at = chrono_like_timestamp();
                record.message = Some(
                    format!(
                        "Timed out while waiting for {} to return a result. Use get-jsx-result with requestId to check the result later.",
                        self.host.display_name
                    ),
                );
                self.write_request_record(&record)?;
                return Ok(BridgeRunOutcome {
                    record,
                    registry_path,
                });
            }

            thread::sleep(options.poll_interval);
        }
    }

    pub fn get_request_record(&self, request_id: &str) -> Result<RequestRecord> {
        let mut record = self.read_request_record(request_id)?;
        if matches!(
            record.status.as_str(),
            "completed" | "failed" | "lost" | "cancelled"
        ) {
            return Ok(record);
        }

        if let Some(instance) = record.host_instance.clone() {
            if let Some((raw, parsed)) =
                self.try_read_instance_result(&instance, request_id, Some(&record.command))?
            {
                record.status = result_record_status(&parsed).to_string();
                record.updated_at = chrono_like_timestamp();
                record.result = Some(parsed);
                record.result_raw = Some(raw);
                record.message = None;
                self.write_request_record(&record)?;
                self.clear_current_request_if_matches(&instance, request_id)?;
            } else if self.is_instance_stale(&instance)? {
                record.status = "lost".to_string();
                record.updated_at = chrono_like_timestamp();
                record.message = Some(format!(
                    "The target {} instance heartbeat is stale; the request may have been lost.",
                    self.host.display_name
                ));
                self.write_request_record(&record)?;
                self.clear_current_request_if_matches(&instance, request_id)?;
            }
        }

        Ok(record)
    }

    pub fn latest_request_record(&self) -> Result<Option<RequestRecord>> {
        let dir = self.registry_dir();
        if !dir.exists() {
            return Ok(None);
        }
        let mut latest: Option<(SystemTime, String)> = None;
        for entry in fs::read_dir(&dir)
            .with_context(|| format!("failed to read registry directory: {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let request_id = path
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_string();
            if latest
                .as_ref()
                .map(|(current, _)| modified > *current)
                .unwrap_or(true)
            {
                latest = Some((modified, request_id));
            }
        }

        latest
            .map(|(_, request_id)| self.get_request_record(&request_id))
            .transpose()
    }

    fn instances_dir(&self) -> PathBuf {
        self.cfg.bridge.root_dir.join("instances")
    }

    fn registry_dir(&self) -> PathBuf {
        self.cfg.bridge.root_dir.join("registry")
    }

    fn registry_path(&self, request_id: &str) -> PathBuf {
        self.registry_dir().join(format!("{request_id}.json"))
    }

    fn broker_lock_path(&self) -> PathBuf {
        self.cfg.bridge.root_dir.join("broker.lock")
    }

    fn acquire_broker_lock(
        &self,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<BrokerLock> {
        let started = Instant::now();
        let path = self.broker_lock_path();
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    let payload = json!({
                        "pid": std::process::id(),
                        "createdAt": chrono_like_timestamp()
                    });
                    let _ = writeln!(file, "{}", serde_json::to_string(&payload)?);
                    return Ok(BrokerLock { path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    maybe_remove_stale_lock(&path)?;
                    if started.elapsed() >= timeout {
                        return Err(anyhow!(
                            "timed out waiting for broker lock; another MCP process may be running a command"
                        ));
                    }
                    thread::sleep(poll_interval);
                }
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("failed to create broker lock: {}", path.display())
                    });
                }
            }
        }
    }

    fn write_instance_command_file(
        &self,
        instance: &HostInstance,
        request_id: &str,
        command: &str,
        args: Value,
    ) -> Result<()> {
        let payload = CommandFile {
            command: command.to_string(),
            args,
            timestamp: chrono_like_timestamp(),
            status: CommandStatus::Pending,
            request_id: Some(request_id.to_string()),
        };
        write_json_file(&instance_command_path(instance), &payload)
    }

    fn write_instance_waiting_result(
        &self,
        instance: &HostInstance,
        request_id: &str,
    ) -> Result<()> {
        let payload = WaitingResult {
            status: "waiting".to_string(),
            message: format!("Waiting for new result from {}...", self.host.display_name),
            timestamp: chrono_like_timestamp(),
            request_id: Some(request_id.to_string()),
        };
        write_json_file(&instance_result_path(instance), &payload)
    }

    fn write_current_request(
        &self,
        instance: &HostInstance,
        request_id: &str,
        command: &str,
    ) -> Result<()> {
        let payload = CurrentRequest {
            request_id: request_id.to_string(),
            command: command.to_string(),
            status: "running".to_string(),
            dispatched_at: chrono_like_timestamp(),
        };
        write_json_file(&instance_current_request_path(instance), &payload)
    }

    fn read_current_request(&self, instance: &HostInstance) -> Result<Option<CurrentRequest>> {
        let path = instance_current_request_path(instance);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read current request: {}", path.display()))?;
        let current = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse current request: {}", path.display()))?;
        Ok(Some(current))
    }

    fn clear_current_request(&self, instance: &HostInstance) -> Result<()> {
        let path = instance_current_request_path(instance);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove current request: {}", path.display()))?;
        }
        Ok(())
    }

    fn clear_current_request_if_matches(
        &self,
        instance: &HostInstance,
        request_id: &str,
    ) -> Result<()> {
        if let Some(current) = self.read_current_request(instance)? {
            if current.request_id == request_id {
                self.clear_current_request(instance)?;
            }
        }
        Ok(())
    }

    fn is_instance_busy(&self, instance: &HostInstance) -> Result<bool> {
        let Some(current) = self.read_current_request(instance)? else {
            return Ok(false);
        };

        if let Some((raw, parsed)) =
            self.try_read_instance_result(instance, &current.request_id, Some(&current.command))?
        {
            if let Ok(mut record) = self.read_request_record(&current.request_id) {
                record.status = result_record_status(&parsed).to_string();
                record.updated_at = chrono_like_timestamp();
                record.result = Some(parsed);
                record.result_raw = Some(raw);
                record.message = None;
                let _ = self.write_request_record(&record);
            }
            self.clear_current_request(instance)?;
            return Ok(false);
        }

        if self.is_instance_stale(instance)? {
            if let Ok(mut record) = self.read_request_record(&current.request_id) {
                record.status = "lost".to_string();
                record.updated_at = chrono_like_timestamp();
                record.message = Some(format!(
                    "The target {} instance heartbeat is stale; the request may have been lost.",
                    self.host.display_name
                ));
                let _ = self.write_request_record(&record);
            }
            self.clear_current_request(instance)?;
            return Ok(false);
        }

        Ok(true)
    }

    fn is_instance_stale(&self, instance: &HostInstance) -> Result<bool> {
        let heartbeat_path = instance_heartbeat_path(instance);
        if !heartbeat_path.exists() {
            return Ok(true);
        }
        let modified = fs::metadata(&heartbeat_path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_else(|_| Duration::from_secs(0));
        Ok(age > Duration::from_millis(self.cfg.instance_heartbeat_stale_ms))
    }

    fn try_read_instance_result(
        &self,
        instance: &HostInstance,
        request_id: &str,
        expected_command: Option<&str>,
    ) -> Result<Option<(String, Value)>> {
        let path = instance_result_path(instance);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read instance result: {}", path.display()))?;
        if raw.trim().is_empty() {
            return Ok(None);
        }
        let parsed = match serde_json::from_str::<Value>(&raw) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let request_matches = parsed
            .get("_requestId")
            .and_then(Value::as_str)
            .map(|value| value == request_id)
            .unwrap_or(false);
        if !request_matches {
            return Ok(None);
        }
        if let Some(command) = expected_command {
            let command_matches = parsed
                .get("_commandExecuted")
                .and_then(Value::as_str)
                .map(|value| value == command)
                .unwrap_or(false);
            if !command_matches {
                return Ok(None);
            }
        }
        Ok(Some((raw, parsed)))
    }

    fn write_request_record(&self, record: &RequestRecord) -> Result<()> {
        write_json_file(&self.registry_path(&record.request_id), record)
    }

    fn read_request_record(&self, request_id: &str) -> Result<RequestRecord> {
        validate_request_id(request_id)?;
        let path = self.registry_path(request_id);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read request registry: {}", path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse request registry: {}", path.display()))
    }

    fn cleanup_registry(&self) -> Result<()> {
        let dir = self.registry_dir();
        if !dir.exists() {
            return Ok(());
        }
        let now = chrono::Utc::now();
        for entry in fs::read_dir(&dir)
            .with_context(|| format!("failed to read registry directory: {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let raw = match fs::read_to_string(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let value = match serde_json::from_str::<Value>(&raw) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Some(expires_at) = value.get("expiresAt").and_then(Value::as_str) else {
                continue;
            };
            let Ok(expires_at) = chrono::DateTime::parse_from_rfc3339(expires_at) else {
                continue;
            };
            if expires_at.with_timezone(&chrono::Utc) <= now {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
}

struct BrokerLock {
    path: PathBuf,
}

impl Drop for BrokerLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn ensure_bridge_dir(cfg: &AppConfig) -> Result<()> {
    fs::create_dir_all(&cfg.bridge.root_dir).with_context(|| {
        format!(
            "failed to create bridge directory: {}",
            cfg.bridge.root_dir.display()
        )
    })?;
    Ok(())
}

pub fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(value).with_context(|| "failed to serialize JSON")?;
    fs::write(path, raw).with_context(|| format!("failed to write file: {}", path.display()))?;
    Ok(())
}

fn instance_command_path(instance: &HostInstance) -> PathBuf {
    PathBuf::from(&instance.command_file)
}

fn instance_discovery_issue(
    folder_name: Option<String>,
    heartbeat_path: &Path,
    reason: impl Into<String>,
    age: Option<Duration>,
    instance: Option<&HostInstance>,
) -> InstanceDiscoveryIssue {
    let mut issue = instance
        .map(|value| {
            instance_discovery_issue_from_instance(folder_name.clone(), heartbeat_path, age, value)
        })
        .unwrap_or_else(|| InstanceDiscoveryIssue {
            instance_id: None,
            folder_name: folder_name.clone(),
            heartbeat_path: heartbeat_path.display().to_string(),
            reason: String::new(),
            age_ms: age.map(duration_millis_u64),
            last_heartbeat_at: None,
            app_name: None,
            app_version: None,
            status: None,
        });
    issue.reason = reason.into();
    issue
}

fn instance_discovery_issue_from_instance(
    folder_name: Option<String>,
    heartbeat_path: &Path,
    age: Option<Duration>,
    instance: &HostInstance,
) -> InstanceDiscoveryIssue {
    InstanceDiscoveryIssue {
        instance_id: Some(instance.instance_id.clone()),
        folder_name,
        heartbeat_path: heartbeat_path.display().to_string(),
        reason: String::new(),
        age_ms: age.map(duration_millis_u64),
        last_heartbeat_at: Some(instance.last_heartbeat_at.clone()),
        app_name: instance.app_name.clone(),
        app_version: instance.app_version.clone(),
        status: instance.status.clone(),
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn instance_result_path(instance: &HostInstance) -> PathBuf {
    PathBuf::from(&instance.result_file)
}

fn instance_heartbeat_path(instance: &HostInstance) -> PathBuf {
    instance
        .heartbeat_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(&instance.bridge_root)
                .join("instances")
                .join(&instance.instance_id)
                .join("heartbeat.json")
        })
}

fn instance_current_request_path(instance: &HostInstance) -> PathBuf {
    PathBuf::from(&instance.bridge_root)
        .join("instances")
        .join(&instance.instance_id)
        .join("current_request.json")
}

fn instance_matches_version(instance: &HostInstance, version: &str) -> bool {
    let needle = version.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    instance
        .app_version
        .as_deref()
        .map(|value| value.to_lowercase().contains(&needle))
        .unwrap_or(false)
        || instance
            .display_name
            .as_deref()
            .map(|value| value.to_lowercase().contains(&needle))
            .unwrap_or(false)
}

fn result_record_status(value: &Value) -> &'static str {
    if value.get("status").and_then(Value::as_str) == Some("error") {
        return "failed";
    }
    if value.get("success").and_then(Value::as_bool) == Some(false) {
        return "failed";
    }
    "completed"
}

fn default_protocol_version() -> u32 {
    1
}

fn maybe_remove_stale_lock(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    let modified = metadata.modified().unwrap_or(SystemTime::now());
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_else(|_| Duration::from_secs(0));
    if age > Duration::from_secs(BROKER_LOCK_STALE_SECONDS) {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove stale broker lock: {}", path.display()))?;
    }
    Ok(())
}

fn validate_request_id(request_id: &str) -> Result<()> {
    let valid = !request_id.is_empty()
        && request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid {
        return Err(anyhow!("invalid requestId"));
    }
    Ok(())
}

fn generate_request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req-{nanos:x}-{}-{counter:x}", std::process::id())
}

fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let datetime: chrono::DateTime<chrono::Utc> = now.into();
    datetime.to_rfc3339()
}

fn timestamp_after_seconds(seconds: u64) -> String {
    let seconds = i64::try_from(seconds).unwrap_or(i64::MAX / 2);
    (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339()
}

fn elapsed_since_system(start: SystemTime) -> Duration {
    SystemTime::now()
        .duration_since(start)
        .unwrap_or_else(|_| Duration::from_secs(0))
}

fn json_text(value: &Value) -> Result<String> {
    serde_json::to_string(value).with_context(|| "failed to serialize warning JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcp_core::BridgePaths;
    use tempfile::tempdir;

    fn test_config() -> (AppConfig, tempfile::TempDir) {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join("ae-mcp-bridge");
        (
            AppConfig {
                bridge: BridgePaths {
                    command_file: root.join("ae_command.json"),
                    result_file: root.join("ae_mcp_result.json"),
                    root_dir: root,
                },
                ..AppConfig::default()
            },
            dir,
        )
    }

    fn write_test_instance(cfg: &AppConfig, instance_id: &str) -> HostInstance {
        let dir = cfg.bridge.root_dir.join("instances").join(instance_id);
        fs::create_dir_all(&dir).expect("instance dir");
        let instance = HostInstance {
            protocol_version: 1,
            instance_id: instance_id.to_string(),
            host_id: cfg.host_id.clone(),
            app_name: Some("After Effects".to_string()),
            app_version: Some("25.0".to_string()),
            display_name: Some("Adobe After Effects 2025".to_string()),
            project_path: None,
            status: Some("idle".to_string()),
            current_request_id: None,
            bridge_runtime: "extendscript-scriptui".to_string(),
            capabilities: vec!["run-jsx".to_string()],
            bridge_root: cfg.bridge.root_dir.display().to_string(),
            command_file: dir.join("ae_command.json").display().to_string(),
            result_file: dir.join("ae_mcp_result.json").display().to_string(),
            last_heartbeat_at: chrono_like_timestamp(),
            updated_at: None,
            heartbeat_path: Some(dir.join("heartbeat.json").display().to_string()),
        };
        write_json_file(&dir.join("heartbeat.json"), &instance).expect("heartbeat");
        instance
    }

    #[test]
    fn write_command_file_creates_pending_command() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg.clone()).expect("client");
        bridge
            .write_command_file("listCompositions", serde_json::json!({}))
            .expect("write");

        let raw = fs::read_to_string(cfg.bridge.command_file).expect("read");
        let data: CommandFile = serde_json::from_str(&raw).expect("parse");
        assert_eq!(data.command, "listCompositions");
        assert_eq!(data.status, CommandStatus::Pending);
    }

    #[test]
    fn clear_results_writes_waiting_payload() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg.clone()).expect("client");
        bridge.clear_results_file().expect("clear");

        let raw = fs::read_to_string(cfg.bridge.result_file).expect("read");
        let data: WaitingResult = serde_json::from_str(&raw).expect("parse");
        assert_eq!(data.status, "waiting");
    }

    #[test]
    fn wait_for_bridge_result_matches_expected_command() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg.clone()).expect("client");
        bridge.clear_results_file().expect("clear");

        let result_path = cfg.bridge.result_file.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            let payload = serde_json::json!({
                "status": "success",
                "_commandExecuted": "listCompositions"
            });
            fs::write(
                result_path,
                serde_json::to_string(&payload).expect("serialize"),
            )
            .expect("write");
        });

        let raw = bridge
            .wait_for_bridge_result(
                Some("listCompositions"),
                Duration::from_secs(2),
                Duration::from_millis(100),
            )
            .expect("should get result");
        let value: Value = serde_json::from_str(&raw).expect("parse");
        assert_eq!(
            value.get("_commandExecuted").and_then(Value::as_str),
            Some("listCompositions")
        );
    }

    #[test]
    fn list_active_instances_reads_heartbeat() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg.clone()).expect("client");
        write_test_instance(&cfg, "ae-test");

        let instances = bridge
            .list_active_instances(Duration::from_secs(10))
            .expect("instances");
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].instance_id, "ae-test");
    }

    #[test]
    fn run_command_sync_dispatches_to_single_instance() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg.clone()).expect("client");
        let instance = write_test_instance(&cfg, "ae-test");
        let result_path = PathBuf::from(instance.result_file.clone());

        thread::spawn(move || {
            let command_path = PathBuf::from(instance.command_file.clone());
            let mut request_id = String::new();
            for _ in 0..20 {
                if command_path.exists() {
                    let raw = fs::read_to_string(&command_path).expect("read command");
                    let command: CommandFile = serde_json::from_str(&raw).expect("parse command");
                    request_id = command.request_id.unwrap_or_default();
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            let payload = serde_json::json!({
                "status": "success",
                "_commandExecuted": "listCompositions",
                "_requestId": request_id
            });
            fs::write(
                result_path,
                serde_json::to_string(&payload).expect("serialize"),
            )
            .expect("write result");
        });

        let outcome = bridge
            .run_command_sync(
                "listCompositions",
                json!({}),
                BridgeRunOptions {
                    target: BridgeTarget::default(),
                    timeout: Duration::from_secs(3),
                    poll_interval: Duration::from_millis(50),
                    retention_seconds: 60,
                },
            )
            .expect("run");
        assert_eq!(outcome.record.status, "completed");
        assert!(outcome.record.host_instance.is_some());
    }

    #[test]
    fn legacy_heartbeat_is_normalized_to_host_schema() {
        let (mut cfg, _guard) = test_config();
        cfg.host_id = "photoshop".to_string();
        let dir = cfg.bridge.root_dir.join("instances").join("ps-legacy");
        fs::create_dir_all(&dir).expect("instance dir");
        let now = chrono_like_timestamp();
        write_json_file(
            &dir.join("heartbeat.json"),
            &json!({
                "instanceId": "ps-legacy",
                "appName": "Photoshop",
                "appVersion": "26.0",
                "bridgeRoot": cfg.bridge.root_dir,
                "commandFile": dir.join("ps_command.json"),
                "resultFile": dir.join("ps_mcp_result.json"),
                "lastHeartbeatAt": now
            }),
        )
        .expect("legacy heartbeat");

        let bridge = BridgeClient::new(cfg).expect("client");
        let instances = bridge
            .list_active_instances(Duration::from_secs(10))
            .expect("instances");
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].protocol_version, 1);
        assert_eq!(instances[0].host_id, "photoshop");
        assert_eq!(instances[0].bridge_runtime, "uxp");
        assert!(instances[0].updated_at.is_some());
    }

    #[test]
    fn legacy_request_record_reads_ae_instance_and_writes_host_instance() {
        let legacy = json!({
            "requestId": "req-legacy",
            "command": "ping",
            "status": "completed",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:01Z",
            "expiresAt": "2026-01-01T01:00:00Z",
            "aeInstance": {
                "instanceId": "ae-legacy",
                "bridgeRoot": "bridge",
                "commandFile": "command.json",
                "resultFile": "result.json",
                "lastHeartbeatAt": "2026-01-01T00:00:00Z"
            }
        });
        let record: RequestRecord = serde_json::from_value(legacy).expect("legacy record");
        assert_eq!(
            record
                .host_instance
                .as_ref()
                .map(|value| value.instance_id.as_str()),
            Some("ae-legacy")
        );

        let value = record.to_value();
        assert!(value.get("hostInstance").is_some());
        assert!(value.get("aeInstance").is_none());
    }

    #[test]
    fn diagnostics_are_derived_from_each_host_spec() {
        for host in mcp_core::HOST_SPECS {
            let dir = tempdir().expect("tempdir");
            let root = dir.path().join(host.bridge_root_name);
            let cfg = AppConfig {
                host_id: host.id.to_string(),
                bridge: BridgePaths {
                    command_file: root.join(host.command_file_name),
                    result_file: root.join(host.result_file_name),
                    root_dir: root,
                },
                ..AppConfig::default()
            };
            let bridge = BridgeClient::new(cfg).expect("client");
            let error = bridge
                .resolve_target(&BridgeTarget::default())
                .expect_err("no heartbeat");
            let message = error.to_string();
            assert!(message.contains(host.display_name));
            assert!(message.contains(host.bridge_setup_hint));
            if host.id != "aftereffects" {
                assert!(!message.contains("After Effects"));
            }
        }
    }
}
