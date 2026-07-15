use anyhow::{anyhow, Context, Result};
use mcp_core::{host_spec_by_id, AppConfig, HostSpec, ScriptFileAudit};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static ATOMIC_WRITE_COUNTER: AtomicU64 = AtomicU64::new(1);
static ATOMIC_REPLACE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
static REQUEST_RECORD_UPDATE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> =
    OnceLock::new();
const BROKER_LOCK_STALE_SECONDS: u64 = 86_400;
const ATOMIC_TEMP_STALE_SECONDS: u64 = 3_600;
const ATOMIC_REPLACE_RETRIES: usize = 50;
const JSON_READ_RETRIES: usize = 8;
const FILE_RETRY_INTERVAL: Duration = Duration::from_millis(10);
const JSON_READ_RETRY_INTERVAL: Duration = Duration::from_millis(25);

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<ScriptFileAudit>,
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
        read_json_text_with_retry(path)
            .with_context(|| format!("failed to read valid result JSON: {}", path.display()))
    }

    pub fn read_results_with_stale_warning(&self, stale_threshold: Duration) -> Result<String> {
        if let Some(record) = self.latest_request_record()? {
            return serde_json::to_string_pretty(&record.to_value())
                .with_context(|| "failed to serialize latest request record");
        }

        let path = &self.cfg.bridge.result_file;
        if !path.exists() {
            return json_text(&serde_json::json!({
                "error": "No results file found. Please run a script in the host application first."
            }));
        }

        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to stat file: {}", path.display()))?;
        let modified = metadata
            .modified()
            .unwrap_or_else(|_| SystemTime::now() - stale_threshold);
        let content = read_json_text_with_retry(path)
            .with_context(|| format!("failed to read valid result JSON: {}", path.display()))?;

        if let Ok(age) = SystemTime::now().duration_since(modified) {
            if age > stale_threshold {
                return json_text(&serde_json::json!({
                    "warning": "Result file appears to be stale (not recently updated).",
                    "message": "This could indicate the host application is not properly writing results or the MCP Bridge panel is not running.",
                    "ageSeconds": age.as_secs(),
                    "originalContent": content
                }));
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
                if let Some((content, value)) = try_read_json_text(&self.cfg.bridge.result_file)? {
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
            let mut instance = match read_json_file_with_retry::<HostInstance>(&heartbeat_path) {
                Ok(value) => value,
                Err(error) => {
                    inactive_instances.push(instance_discovery_issue(
                        folder_name,
                        &heartbeat_path,
                        format!("failed to read or parse heartbeat.json: {error}"),
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
        self.prepare_request_with_audit(command, retention_seconds, host_instance, None)
    }

    pub fn prepare_request_with_audit(
        &self,
        command: &str,
        retention_seconds: u64,
        host_instance: Option<HostInstance>,
        audit: Option<ScriptFileAudit>,
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
            audit,
        };
        self.write_request_record(&record)?;
        Ok(BridgeRunOutcome {
            record,
            registry_path,
        })
    }

    pub fn mark_request_timeout(&self, request_id: &str, message: String) -> Result<RequestRecord> {
        self.update_request_record(request_id, |record| {
            if is_terminal_request_status(&record.status) {
                return false;
            }
            record.status = "timeout".to_string();
            record.updated_at = chrono_like_timestamp();
            record.message = Some(message);
            true
        })
    }

    pub fn mark_request_failed(&self, request_id: &str, message: String) -> Result<RequestRecord> {
        self.update_request_record(request_id, |record| {
            if is_terminal_request_status(&record.status) {
                return false;
            }
            record.status = "failed".to_string();
            record.updated_at = chrono_like_timestamp();
            record.message = Some(message);
            true
        })
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
                record = self.read_request_record(request_id)?;
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

        self.write_instance_waiting_result(&instance, request_id)?;
        self.write_current_request(&instance, request_id, command)?;
        self.write_instance_command_file(&instance, request_id, command, args)?;

        record.status = "running".to_string();
        record.updated_at = chrono_like_timestamp();
        self.write_request_record(&record)?;

        loop {
            if let Some((raw, parsed)) =
                self.try_read_instance_result(&instance, request_id, Some(command))?
            {
                record.status = result_record_status(&parsed).to_string();
                record.updated_at = chrono_like_timestamp();
                record.result = Some(parsed);
                record.result_raw = Some(raw);
                record.message = None;
                self.write_request_record(&record)?;
                record = self.read_request_record(request_id)?;
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
                record = self.read_request_record(request_id)?;
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
                record = self.read_request_record(request_id)?;
                self.clear_current_request_if_matches(&instance, request_id)?;
            } else if self.is_instance_stale(&instance)? {
                record.status = "lost".to_string();
                record.updated_at = chrono_like_timestamp();
                record.message = Some(format!(
                    "The target {} instance heartbeat is stale; the request may have been lost.",
                    self.host.display_name
                ));
                self.write_request_record(&record)?;
                record = self.read_request_record(request_id)?;
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
        let current = read_json_file_with_retry(&path)
            .with_context(|| format!("failed to read current request: {}", path.display()))?;
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
        let Some((raw, parsed)) = try_read_json_text(&path)? else {
            return Ok(None);
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
        validate_request_id(&record.request_id)?;
        let path = self.registry_path(&record.request_id);
        let update_lock = request_record_update_lock(&path)?;
        let _update_guard = update_lock
            .lock()
            .map_err(|_| anyhow!("request record update lock is poisoned"))?;

        if path.exists() {
            let current: RequestRecord = read_json_file_with_retry(&path)
                .with_context(|| format!("failed to read request registry: {}", path.display()))?;
            if should_preserve_current_record(&current, record) {
                return Ok(());
            }
        }
        write_json_file(&path, record)
    }

    fn update_request_record<F>(&self, request_id: &str, update: F) -> Result<RequestRecord>
    where
        F: FnOnce(&mut RequestRecord) -> bool,
    {
        validate_request_id(request_id)?;
        let path = self.registry_path(request_id);
        let update_lock = request_record_update_lock(&path)?;
        let _update_guard = update_lock
            .lock()
            .map_err(|_| anyhow!("request record update lock is poisoned"))?;
        let mut record: RequestRecord = read_json_file_with_retry(&path)
            .with_context(|| format!("failed to read request registry: {}", path.display()))?;
        if update(&mut record) {
            write_json_file(&path, &record)?;
        }
        Ok(record)
    }

    fn read_request_record(&self, request_id: &str) -> Result<RequestRecord> {
        validate_request_id(request_id)?;
        let path = self.registry_path(request_id);
        read_json_file_with_retry(&path)
            .with_context(|| format!("failed to read request registry: {}", path.display()))
    }

    fn cleanup_registry(&self) -> Result<()> {
        let dir = self.registry_dir();
        if !dir.exists() {
            return Ok(());
        }
        cleanup_stale_atomic_temp_files(
            &dir,
            None,
            Duration::from_secs(ATOMIC_TEMP_STALE_SECONDS),
        )?;
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

fn request_record_update_lock(path: &Path) -> Result<Arc<Mutex<()>>> {
    let mut locks = REQUEST_RECORD_UPDATE_LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| anyhow!("request record lock registry is poisoned"))?;
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    Ok(lock)
}

fn is_terminal_request_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "lost" | "cancelled")
}

// A client timeout is recoverable, but a completed/error result is final.
// Slow atomic I/O may let worker intermediate writes arrive after the outer
// timeout marker, so those writes must not move the registry backwards.
fn should_preserve_current_record(current: &RequestRecord, proposed: &RequestRecord) -> bool {
    is_terminal_request_status(&current.status)
        || (current.status == "timeout"
            && matches!(
                proposed.status.as_str(),
                "queued" | "dispatched" | "running"
            ))
}

pub fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(value).with_context(|| "failed to serialize JSON")?;
    write_atomic_text_file(path, raw.as_bytes())
        .with_context(|| format!("failed to atomically write file: {}", path.display()))
}

fn write_atomic_text_file(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory: {}", parent.display()))?;

    let target_name = path
        .file_name()
        .ok_or_else(|| anyhow!("atomic write target has no file name: {}", path.display()))?
        .to_string_lossy()
        .into_owned();
    cleanup_stale_atomic_temp_files(
        parent,
        Some(&target_name),
        Duration::from_secs(ATOMIC_TEMP_STALE_SECONDS),
    )?;

    let (temp_path, mut temp_file) = create_atomic_temp_file(parent, &target_name)?;
    let mut temp_guard = AtomicTempGuard::new(temp_path.clone());
    temp_file
        .write_all(contents)
        .with_context(|| format!("failed to write temporary file: {}", temp_path.display()))?;
    temp_file
        .flush()
        .with_context(|| format!("failed to flush temporary file: {}", temp_path.display()))?;
    temp_file
        .sync_all()
        .with_context(|| format!("failed to sync temporary file: {}", temp_path.display()))?;
    drop(temp_file);

    atomic_replace_with_retry(&temp_path, path)?;
    temp_guard.disarm();
    sync_parent_directory(parent)?;
    Ok(())
}

fn create_atomic_temp_file(parent: &Path, target_name: &str) -> Result<(PathBuf, File)> {
    for _ in 0..32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_nanos();
        let counter = ATOMIC_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".{target_name}.tmp-{}-{nanos:x}-{counter:x}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to create atomic temporary file: {}", path.display())
                });
            }
        }
    }
    Err(anyhow!(
        "failed to allocate a unique atomic temporary file for {target_name}"
    ))
}

fn atomic_replace_with_retry(source: &Path, destination: &Path) -> Result<()> {
    let destination_lock = {
        let mut locks = ATOMIC_REPLACE_LOCKS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|_| anyhow!("atomic replace lock registry is poisoned"))?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(destination).and_then(Weak::upgrade) {
            lock
        } else {
            let lock = Arc::new(Mutex::new(()));
            locks.insert(destination.to_path_buf(), Arc::downgrade(&lock));
            lock
        }
    };
    let _replace_guard = destination_lock
        .lock()
        .map_err(|_| anyhow!("atomic replace lock is poisoned"))?;
    let mut last_error = None;
    for attempt in 0..ATOMIC_REPLACE_RETRIES {
        match atomic_replace(source, destination) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let retryable = is_retryable_atomic_replace_error(&error);
                last_error = Some(error);
                if retryable && attempt + 1 < ATOMIC_REPLACE_RETRIES {
                    thread::sleep(FILE_RETRY_INTERVAL);
                } else {
                    break;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("atomic replace failed"))).with_context(
        || {
            format!(
                "failed to replace {} with {}",
                destination.display(),
                source.display()
            )
        },
    )
}

fn is_retryable_atomic_replace_error(error: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
        matches!(error.raw_os_error(), Some(5 | 32 | 33))
    }
    #[cfg(not(windows))]
    {
        matches!(
            error.kind(),
            ErrorKind::PermissionDenied | ErrorKind::WouldBlock | ErrorKind::Interrupted
        )
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> Result<()> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("failed to sync directory: {}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> Result<()> {
    Ok(())
}

fn cleanup_stale_atomic_temp_files(
    directory: &Path,
    target_name: Option<&str>,
    stale_after: Duration,
) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    let target_prefix = target_name.map(|name| format!(".{name}.tmp-"));
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to scan atomic temp files: {}", directory.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let is_atomic_temp = target_prefix
            .as_deref()
            .map(|prefix| file_name.starts_with(prefix))
            .unwrap_or_else(|| file_name.starts_with('.') && file_name.contains(".tmp-"));
        if !is_atomic_temp {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::now());
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_else(|_| Duration::from_secs(0));
        if age >= stale_after {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

fn read_json_text_with_retry(path: &Path) -> Result<String> {
    let mut last_error = String::new();
    for attempt in 0..JSON_READ_RETRIES {
        match fs::read_to_string(path) {
            Ok(raw) if !raw.trim().is_empty() => match serde_json::from_str::<Value>(&raw) {
                Ok(_) => return Ok(raw),
                Err(error) => last_error = format!("invalid JSON: {error}"),
            },
            Ok(_) => last_error = "file is empty".to_string(),
            Err(error) => last_error = error.to_string(),
        }
        if attempt + 1 < JSON_READ_RETRIES {
            thread::sleep(JSON_READ_RETRY_INTERVAL);
        }
    }
    Err(anyhow!(
        "failed to read complete JSON after {JSON_READ_RETRIES} attempts: {last_error}"
    ))
}

fn read_json_file_with_retry<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut last_error = String::new();
    for attempt in 0..JSON_READ_RETRIES {
        match fs::read_to_string(path) {
            Ok(raw) if !raw.trim().is_empty() => match serde_json::from_str::<T>(&raw) {
                Ok(value) => return Ok(value),
                Err(error) => last_error = format!("invalid JSON: {error}"),
            },
            Ok(_) => last_error = "file is empty".to_string(),
            Err(error) => last_error = error.to_string(),
        }
        if attempt + 1 < JSON_READ_RETRIES {
            thread::sleep(JSON_READ_RETRY_INTERVAL);
        }
    }
    Err(anyhow!(
        "failed to read complete JSON after {JSON_READ_RETRIES} attempts: {last_error}"
    ))
}

fn try_read_json_text(path: &Path) -> Result<Option<(String, Value)>> {
    let raw = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::PermissionDenied | ErrorKind::WouldBlock
            ) =>
        {
            return Ok(None);
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read JSON: {}", path.display()));
        }
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Ok(None);
    };
    Ok(Some((raw, value)))
}

struct AtomicTempGuard {
    path: PathBuf,
    armed: bool,
}

impl AtomicTempGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for AtomicTempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
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
    use std::sync::Arc;
    use tempfile::tempdir;

    static ATOMIC_STRESS_TEST_LOCK: Mutex<()> = Mutex::new(());

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
    fn request_registry_retains_script_audit_metadata() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg).expect("client");
        let audit = ScriptFileAudit {
            host_id: "aftereffects".to_string(),
            mode: "unsafe".to_string(),
            source_path: "C:/scripts/test.jsx".to_string(),
            source_sha256: "a".repeat(64),
            source_size_bytes: 12,
        };
        let prepared = bridge
            .prepare_request_with_audit("executeJsx", 60, None, Some(audit.clone()))
            .expect("prepare request");

        let raw = fs::read_to_string(&prepared.registry_path).expect("registry file");
        let record: RequestRecord = serde_json::from_str(&raw).expect("registry record");
        assert_eq!(record.audit, Some(audit));
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
    fn atomic_write_replaces_an_existing_destination() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("command.json");
        write_json_file(&path, &json!({ "status": "pending", "revision": 1 }))
            .expect("first write");
        write_json_file(&path, &json!({ "status": "running", "revision": 2 }))
            .expect("replace existing destination");

        let value: Value = read_json_file_with_retry(&path).expect("replacement");
        assert_eq!(value.get("status").and_then(Value::as_str), Some("running"));
        assert_eq!(value.get("revision").and_then(Value::as_u64), Some(2));
    }

    #[test]
    fn atomic_writes_remain_valid_during_parallel_updates() {
        let _stress_guard = ATOMIC_STRESS_TEST_LOCK.lock().expect("stress test lock");
        let dir = tempdir().expect("tempdir");
        let path = Arc::new(dir.path().join("heartbeat.json"));
        write_json_file(&path, &json!({ "writer": 0, "sequence": 0 })).expect("initial");

        let mut writers = Vec::new();
        for writer in 1..=4 {
            let path = Arc::clone(&path);
            writers.push(thread::spawn(move || {
                for sequence in 0..40 {
                    write_json_file(
                        &path,
                        &json!({
                            "writer": writer,
                            "sequence": sequence,
                            "payload": "x".repeat(16 * 1024)
                        }),
                    )
                    .expect("parallel atomic write");
                }
            }));
        }

        for _ in 0..200 {
            let raw = read_json_text_with_retry(&path).expect("complete JSON");
            let value: Value = serde_json::from_str(&raw).expect("parse");
            assert!(value.get("writer").and_then(Value::as_u64).is_some());
        }
        for writer in writers {
            writer.join().expect("writer thread");
        }

        let prefix = ".heartbeat.json.tmp-";
        let residues = fs::read_dir(dir.path())
            .expect("scan")
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
            .count();
        assert_eq!(residues, 0);
    }

    #[test]
    fn json_reader_retries_a_transient_partial_write() {
        let _stress_guard = ATOMIC_STRESS_TEST_LOCK.lock().expect("stress test lock");
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("result.json");
        fs::write(&path, r#"{"status":"#).expect("partial write");
        let writer_path = path.clone();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            write_json_file(&writer_path, &json!({ "status": "completed" }))
                .expect("complete write");
        });

        let value: Value = read_json_file_with_retry(&path).expect("reader recovers");
        assert_eq!(
            value.get("status").and_then(Value::as_str),
            Some("completed")
        );
        writer.join().expect("writer thread");
    }

    #[test]
    fn stale_atomic_residue_is_cleaned_without_touching_target() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("command.json");
        let residue = dir.path().join(".command.json.tmp-crashed-writer");
        write_json_file(&target, &json!({ "status": "pending" })).expect("target");
        fs::write(&residue, r#"{"status":"#).expect("residue");

        cleanup_stale_atomic_temp_files(dir.path(), Some("command.json"), Duration::ZERO)
            .expect("cleanup");

        assert!(!residue.exists());
        let value: Value = read_json_file_with_retry(&target).expect("target remains valid");
        assert_eq!(value.get("status").and_then(Value::as_str), Some("pending"));
    }

    #[test]
    fn failed_replace_keeps_the_previous_valid_file() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("registry.json");
        write_json_file(&target, &json!({ "status": "queued" })).expect("target");

        let error = atomic_replace_with_retry(&dir.path().join("missing.tmp"), &target)
            .expect_err("replace must fail");
        assert!(error.to_string().contains("failed to replace"));
        let value: Value = read_json_file_with_retry(&target).expect("old JSON remains");
        assert_eq!(value.get("status").and_then(Value::as_str), Some("queued"));
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
    fn timeout_record_recovers_from_a_late_result() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg.clone()).expect("client");
        let instance = write_test_instance(&cfg, "late-result");
        let prepared = bridge
            .prepare_request("ping", 60, Some(instance.clone()))
            .expect("prepare");
        let request_id = prepared.record.request_id;
        let timed_out = bridge
            .mark_request_timeout(&request_id, "outer timeout".to_string())
            .expect("timeout");
        assert_eq!(timed_out.status, "timeout");
        let mut late_intermediate = timed_out.clone();
        late_intermediate.status = "dispatched".to_string();
        late_intermediate.updated_at = chrono_like_timestamp();
        bridge
            .write_request_record(&late_intermediate)
            .expect("late intermediate update");
        assert_eq!(
            bridge
                .read_request_record(&request_id)
                .expect("preserved timeout")
                .status,
            "timeout"
        );

        write_json_file(
            &PathBuf::from(instance.result_file),
            &json!({
                "status": "success",
                "_commandExecuted": "ping",
                "_requestId": request_id
            }),
        )
        .expect("late result");
        let recovered = bridge.get_request_record(&request_id).expect("recover");
        assert_eq!(recovered.status, "completed");
        assert!(recovered.result.is_some());
    }

    #[test]
    fn terminal_result_wins_race_with_outer_timeout_marker() {
        let (cfg, _guard) = test_config();
        let bridge = BridgeClient::new(cfg.clone()).expect("client");
        let instance = write_test_instance(&cfg, "terminal-race");

        for (result_status, expected_status) in [("success", "completed"), ("error", "failed")] {
            for _ in 0..12 {
                let prepared = bridge
                    .prepare_request("ping", 60, Some(instance.clone()))
                    .expect("prepare");
                let request_id = prepared.record.request_id;
                write_json_file(
                    &PathBuf::from(&instance.result_file),
                    &json!({
                        "status": result_status,
                        "_commandExecuted": "ping",
                        "_requestId": request_id.clone()
                    }),
                )
                .expect("host result");

                let start = Arc::new(std::sync::Barrier::new(3));
                let result_bridge = bridge.clone();
                let result_id = request_id.clone();
                let result_start = Arc::clone(&start);
                let result_reader = thread::spawn(move || {
                    result_start.wait();
                    result_bridge
                        .get_request_record(&result_id)
                        .expect("result transition")
                });
                let timeout_bridge = bridge.clone();
                let timeout_id = request_id.clone();
                let timeout_start = Arc::clone(&start);
                let timeout_marker = thread::spawn(move || {
                    timeout_start.wait();
                    timeout_bridge
                        .mark_request_timeout(&timeout_id, "outer timeout".to_string())
                        .expect("timeout marker")
                });
                start.wait();
                let _ = result_reader.join().expect("result reader");
                let _ = timeout_marker.join().expect("timeout marker");

                let final_record = bridge
                    .get_request_record(&request_id)
                    .expect("final record");
                assert_eq!(final_record.status, expected_status);
                assert!(final_record.result.is_some());
                let late_timeout = bridge
                    .mark_request_timeout(&request_id, "later timeout".to_string())
                    .expect("late timeout marker");
                assert_eq!(late_timeout.status, expected_status);
            }
        }
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
