//! Recover idle AE 2025 schedulers outside scheduleTask, which can stop in its entirety.
//! It requires an existing AE process and never dispatches user code or releases uncertain requests.
use super::*;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::time::SystemTime;
#[cfg(not(test))]
mod windows;
#[cfg(not(test))]
use windows::WindowsHost;

#[cfg(not(test))]
pub(super) use windows::track_host_exit;

fn supports_process_lifetime_tracking(instance: &HostInstance) -> bool {
    instance.host_id == "aftereffects"
        && instance.bridge_runtime == "extendscript-startup"
        && instance.lifecycle_mode.as_deref() == Some("startup-headless")
        && instance
            .app_version
            .as_deref()
            .is_some_and(|v| v.starts_with("25."))
}

fn instance_belongs_to_process(instance: &HostInstance, process: &AeProcess, now_ms: u64) -> bool {
    if !supports_process_lifetime_tracking(instance) {
        return false;
    }
    // Runtime timestamps have only second precision. The instance ID also
    // carries its creation millisecond, avoiding a same-second restart match.
    let mut parts = instance.instance_id.rsplitn(3, '-');
    let nonce = parts.next();
    let started = parts.next().and_then(|s| s.parse::<u64>().ok());
    let prefix = parts.next();
    let expected_prefix = format!("ae-{}", instance.app_version.as_deref().unwrap_or_default());
    nonce.is_some_and(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit()))
        && prefix == Some(expected_prefix.as_str())
        && started.is_some_and(|t| t >= process.started_ms && t <= now_ms)
}

#[cfg(test)]
mod process_lifetime_tests;

#[cfg(not(test))]
const CHECK_INTERVAL: Duration = Duration::from_secs(3);
const STALE_AFTER: Duration = Duration::from_secs(20);
const ACK_TIMEOUT_MS: u64 = 12_000;
const WRAPPER_LIFETIME_MS: u64 = 10_000;
const MAX_EVIDENCE_BYTES: u64 = 128 * 1024;

#[cfg(not(test))]
pub(super) fn start(state: &Arc<DaemonState>) -> Result<()> {
    if state.cfg.host_id != "aftereffects" || !state.cfg.ae_native_recovery_enabled {
        return Ok(());
    }
    let weak = Arc::downgrade(state);
    thread::Builder::new()
        .name("ae-native-recovery".into())
        .spawn(move || {
            let mut monitor = Monitor::default();
            let mut host = WindowsHost;
            let mut last_error = None;
            loop {
                thread::sleep(CHECK_INTERVAL);
                let Some(state) = weak.upgrade() else { break };
                if !state
                    .lifecycle
                    .lock()
                    .expect("lifecycle mutex poisoned")
                    .accepting
                {
                    break;
                }
                match monitor.poll(&state, &mut host) {
                    Ok(()) => last_error = None,
                    Err(error) => {
                        let message = format!("{error:#}");
                        if last_error.as_ref() != Some(&message) {
                            tracing::warn!("AE native scheduler recovery: {message}");
                        }
                        last_error = Some(message);
                    }
                }
            }
        })?;
    Ok(())
}

// A permit excludes new commands while keeping status and drain queries responsive.
struct RecoveryPermit(Arc<Mutex<Lifecycle>>);
impl RecoveryPermit {
    fn acquire(state: &DaemonState, pending_mailbox_bound: bool) -> Option<Self> {
        let mut lifecycle = state.lifecycle.try_lock().ok()?;
        if !lifecycle.accepting
            || (lifecycle.pending_jobs != 0 && !pending_mailbox_bound)
            || lifecycle.recovering
        {
            return None;
        }
        lifecycle.recovering = true;
        lifecycle.pending_jobs += 1;
        Some(Self(Arc::clone(&state.lifecycle)))
    }
}
impl Drop for RecoveryPermit {
    fn drop(&mut self) {
        let mut lifecycle = self.0.lock().expect("lifecycle mutex poisoned");
        lifecycle.recovering = false;
        lifecycle.pending_jobs -= 1;
    }
}

// Tests inject a fake host; they never inspect windows or launch a real AE process.
trait NativeHost {
    fn process(&mut self) -> Result<Option<AeProcess>>;
    fn windows_ready(&mut self, pid: u32) -> bool;
    fn launch(&mut self, executable: &Path, script: &Path) -> Result<()>;
    fn now_ms(&self) -> u64;
    fn sleep(&mut self, duration: Duration);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    process_id: u32,
    process_started_ms: u64,
    instance_id: String,
    runtime_id: String,
    attempts: u32,
    expires_at_ms: u64,
    saw_blocked_window: bool,
    outcome: String,
}
impl Journal {
    fn same_session(&self, process: &AeProcess, instance: &HostInstance) -> bool {
        self.process_id == process.pid
            && self.process_started_ms == process.started_ms
            && self.instance_id == instance.instance_id
    }
    fn may_attempt(&self, now: u64) -> bool {
        self.outcome == "recovered"
            || (self.attempts == 1
                && self.saw_blocked_window
                && self.outcome == "unacknowledged"
                && now > self.expires_at_ms)
    }
}

#[derive(Clone)]
struct Progress {
    instance: String,
    runtime: String,
    ticks: [u64; 3],
}
impl Progress {
    fn read(value: &Value) -> Option<Self> {
        if value["schedulerReady"] != true {
            return None;
        }
        let ticks = [
            value["commandTickSequence"].as_u64()?,
            value["heartbeatTickSequence"].as_u64()?,
            value["watchdogTickSequence"].as_u64()?,
        ];
        if ticks.contains(&0) {
            return None;
        }
        Some(Self {
            instance: value["instanceId"].as_str()?.to_owned(),
            runtime: value["runtimeId"].as_str()?.to_owned(),
            ticks,
        })
    }
    fn advances(&self, previous: &Self) -> bool {
        self.instance == previous.instance
            && self.runtime == previous.runtime
            && self
                .ticks
                .iter()
                .zip(previous.ticks)
                .all(|(now, before)| *now > before)
    }
}

#[derive(Default)]
struct Monitor {
    progress: Option<Progress>,
}

fn read_evidence(path: &Path) -> Result<Value> {
    read_evidence_limited(path, MAX_EVIDENCE_BYTES)
}

fn read_evidence_limited(path: &Path, limit: u64) -> Result<Value> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    anyhow::ensure!(
        file.metadata()?.len() <= limit,
        "recovery evidence is oversized"
    );
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= limit,
        "recovery evidence grew beyond limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn read_journal(path: &Path) -> Result<Option<Journal>> {
    match read_evidence(path) {
        Ok(value) => Ok(Some(serde_json::from_value(value)?)),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(error) => {
            Err(error.context("recovery journal is unreadable; automatic attempts are paused"))
        }
    }
}
fn save_journal(path: &Path, journal: &Journal) -> Result<()> {
    bridge_core::write_atomic_text_file(path, serde_json::to_string_pretty(journal)?.as_bytes())
}

pub(super) fn status(cfg: &AppConfig, bridge: &BridgeClient) -> Option<String> {
    if cfg.host_id != "aftereffects" || !cfg.ae_native_recovery_enabled {
        return None;
    }
    match read_journal(&cfg.bridge.root_dir.join("native-recovery/state.json")) {
        Ok(journal) => journal.map(|j| {
            if j.outcome != "recovered" {
                return j.outcome;
            }
            // A durable success record is history, not current connection health.
            match bridge.discover_instances(Duration::from_millis(cfg.instance_heartbeat_stale_ms))
            {
                Ok(report) => match report
                    .instances
                    .iter()
                    .find(|i| i.instance_id == j.instance_id)
                {
                    Some(instance) => {
                        let progress = instance
                            .heartbeat_path
                            .as_ref()
                            .and_then(|p| read_evidence(Path::new(p)).ok())
                            .and_then(|value| Progress::read(&value));
                        if progress.is_some_and(|p| p.instance == j.instance_id) {
                            "recovered".into()
                        } else {
                            "awaiting-progress".into()
                        }
                    }
                    None => "stale".into(),
                },
                Err(_) => "unknown-heartbeat".into(),
            }
        }),
        Err(_) => Some("unreadable-journal".into()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AeProcess {
    pid: u32,
    started_ms: u64,
    executable: PathBuf,
}

fn candidate(value: &Value, process: &AeProcess) -> bool {
    let started = value["runtimeStartedAt"].as_str().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.timestamp_millis())
            .or_else(|| {
                // ExtendScript emits a full weekday ("Thursday"), unlike RFC 2822.
                let (_, rest) = s.split_once(", ")?;
                chrono::NaiveDateTime::parse_from_str(rest, "%d %b %Y %H:%M:%S GMT")
                    .ok()
                    .map(|t| t.and_utc().timestamp_millis())
            })
    });
    value["hostId"] == "aftereffects"
        && value["status"] == "idle"
        && value["currentRequestId"].is_null()
        && value["renderQueueActive"] == false
        && value["bridgeVersion"] == "0.5.5"
        && value["appVersion"]
            .as_str()
            .is_some_and(|s| s.starts_with("25."))
        && value["runtimeId"].as_str().is_some_and(|s| !s.is_empty())
        && started.is_some_and(|t| t >= process.started_ms as i64 - 1000)
}

fn mailbox_admission(
    folder: &Path,
    command_file: &Path,
    max_command_bytes: u64,
) -> Option<Option<String>> {
    let reservation_path = folder.join("current_request.json");
    let command = match read_evidence_limited(command_file, max_command_bytes) {
        Ok(value) => value,
        Err(_) if !command_file.exists() && !reservation_path.exists() => return Some(None),
        Err(_) => return None,
    };
    if matches!(command["status"].as_str(), Some("completed" | "error"))
        && !reservation_path.exists()
    {
        return Some(None);
    }
    // A broker can reserve and queue a command immediately before AE loses all
    // scheduled tasks. Only that correlated, still-pending mailbox may be rearmed.
    // No request is executed here and no reservation is removed.
    let id = command["requestId"].as_str()?;
    if command["status"] != "pending"
        || id.is_empty()
        || id.len() > 160
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let reservation = read_evidence(&reservation_path).ok()?;
    (reservation["requestId"] == id
        && reservation["command"] == command["command"]
        && command["command"].as_str().is_some_and(|c| !c.is_empty())
        && reservation["status"] == "running")
        .then(|| Some(id.to_owned()))
}

impl Monitor {
    fn poll(&mut self, state: &DaemonState, host: &mut impl NativeHost) -> Result<()> {
        let report = state.bridge.discover_instances(STALE_AFTER)?;
        let output = state.cfg.bridge.root_dir.join("native-recovery");
        let journal_file = output.join("state.json");
        let mut journal = read_journal(&journal_file)?;
        if !report.instances.is_empty() {
            // A manual repair resets the retry budget only after observed progress.
            if let Some(ref mut record) = journal {
                if record.outcome != "recovered" {
                    if let Some(instance) = report
                        .instances
                        .iter()
                        .find(|i| i.instance_id == record.instance_id)
                    {
                        if let Some(path) = &instance.heartbeat_path {
                            if let Ok(value) = read_evidence(Path::new(path)) {
                                if let Some(next) = Progress::read(&value) {
                                    if self.progress.as_ref().is_some_and(|old| next.advances(old))
                                    {
                                        if let Ok(_lock) = recovery_lock(&output) {
                                            // Do not overwrite another broker's newer attempt.
                                            if let Some(mut latest) = read_journal(&journal_file)? {
                                                if latest.instance_id == record.instance_id
                                                    && latest.expires_at_ms == record.expires_at_ms
                                                {
                                                    latest.outcome = "recovered".into();
                                                    latest.attempts = 0;
                                                    save_journal(&journal_file, &latest)?;
                                                }
                                            }
                                        }
                                    }
                                    self.progress = Some(next);
                                }
                            }
                        }
                    }
                }
            }
            return Ok(());
        }
        self.progress = None;
        // Inspect OS state only if this root contains a supported stale runtime.
        let mut candidates = Vec::new();
        for issue in report.inactive_instances {
            if issue
                .age_ms
                .is_none_or(|age| age < STALE_AFTER.as_millis() as u64)
            {
                continue;
            }
            let path = PathBuf::from(&issue.heartbeat_path);
            let Ok(value) = read_evidence(&path) else {
                continue;
            };
            if value["bridgeVersion"] == "0.5.5" && value["hostId"] == "aftereffects" {
                candidates.push((path, value));
            }
        }
        if candidates.is_empty() {
            return Ok(());
        }
        let Some(process) = host.process()? else {
            return Ok(());
        };
        candidates.retain(|(_, value)| candidate(value, &process));
        if candidates.len() > 1 {
            // Re-running Startup can leave zero-tick records in another AE engine.
            // Keep the process/instance identity from our persisted attempt rather
            // than guessing from the newest filename or deleting those records.
            // The wrapper still verifies the exact live runtime before touching it,
            // and the same journal below retains its retry budget.
            if let Some(record) = journal.as_ref().filter(|j| {
                j.process_id == process.pid && j.process_started_ms == process.started_ms
            }) {
                candidates.retain(|(_, value)| value["instanceId"] == record.instance_id);
            }
        }
        if candidates.len() != 1 {
            return Ok(());
        }
        let (heartbeat, value) = candidates.remove(0);
        let instance: HostInstance = serde_json::from_value(value)?;
        let folder = heartbeat.parent().context("missing heartbeat folder")?;
        if fs::canonicalize(folder.parent().context("missing instances folder")?)?
            != fs::canonicalize(state.cfg.bridge.root_dir.join("instances"))?
            || fs::canonicalize(&instance.bridge_root)?
                != fs::canonicalize(&state.cfg.bridge.root_dir)?
            || folder.file_name().and_then(|s| s.to_str()) != Some(&instance.instance_id)
            || Path::new(&instance.command_file) != folder.join("ae_command.json")
        {
            return Ok(());
        }
        let max_command_bytes = state
            .cfg
            .script_contract
            .max_input_bytes
            .saturating_add(4096);
        let Some(pending_request) =
            mailbox_admission(folder, Path::new(&instance.command_file), max_command_bytes)
        else {
            return Ok(());
        };
        let launcher = process.executable.with_file_name("AfterFX.com");
        if !launcher.is_file() {
            return Ok(());
        }
        let Some(_permit) = RecoveryPermit::acquire(state, pending_request.is_some()) else {
            return Ok(());
        };
        fs::create_dir_all(&output)?;
        let Ok(_lock) = recovery_lock(&output) else {
            return Ok(());
        };
        // Another broker may have attempted recovery since the initial discovery.
        journal = read_journal(&journal_file)?.filter(|j| j.same_session(&process, &instance));
        if !host.windows_ready(process.pid) {
            if let Some(ref mut record) = journal {
                if record.outcome == "unacknowledged" && !record.saw_blocked_window {
                    record.saw_blocked_window = true;
                    save_journal(&journal_file, record)?;
                }
            }
            return Ok(());
        }
        let now = host.now_ms();
        if journal.as_ref().is_some_and(|j| !j.may_attempt(now)) {
            return Ok(());
        }
        if host.process()?.as_ref() != Some(&process)
            || !host.windows_ready(process.pid)
            || mailbox_admission(folder, Path::new(&instance.command_file), max_command_bytes)
                != Some(pending_request.clone())
        {
            return Ok(());
        }
        let latest = read_evidence(&heartbeat)?;
        if latest["runtimeId"].as_str() != instance.runtime_id.as_deref()
            || !candidate(&latest, &process)
            || fs::metadata(&heartbeat)?
                .modified()?
                .elapsed()
                .unwrap_or_default()
                < STALE_AFTER
        {
            return Ok(());
        }
        let token = format!("{}-{now}", std::process::id());
        let script = output.join(format!("recovery-{token}.jsx"));
        let result = output.join(format!("recovery-{token}.json"));
        let config = json!({
            "nonce": token, "instanceId": instance.instance_id, "runtimeId": instance.runtime_id,
            "appVersion": instance.app_version, "expiresAtMs": now + WRAPPER_LIFETIME_MS,
            "commandFile": instance.command_file, "reservationFile": folder.join("current_request.json"),
            "pendingRequestId": pending_request,
            "resultFile": result,
        });
        fs::write(&script, recovery_script(&config))?;
        let mut record = Journal {
            process_id: process.pid,
            process_started_ms: process.started_ms,
            instance_id: instance.instance_id.clone(),
            runtime_id: instance.runtime_id.clone().unwrap(),
            attempts: journal.as_ref().map_or(1, |j| j.attempts + 1),
            expires_at_ms: now + WRAPPER_LIFETIME_MS,
            saw_blocked_window: false,
            outcome: "launched".into(),
        };
        // Persist before launching. A broker restart cannot reset this retry budget.
        save_journal(&journal_file, &record)?;
        if let Err(error) = host.launch(&launcher, &script) {
            record.outcome = "launch-failed".into();
            save_journal(&journal_file, &record)?;
            return Err(error);
        }
        let mut observed: Option<Progress> = None;
        let mut acknowledged = false;
        // An OS clock adjustment must not turn a bounded recovery into an infinite wait.
        for _ in 0..(ACK_TIMEOUT_MS / 500) {
            if !host.windows_ready(process.pid) {
                record.saw_blocked_window = true;
            }
            if let Ok(reply) = read_evidence(&result) {
                if reply["nonce"] == config["nonce"] {
                    acknowledged = true;
                    if !matches!(
                        reply["outcome"].as_str(),
                        Some("rearmed" | "unchanged" | "deferred")
                    ) {
                        record.outcome = "refused".into();
                        save_journal(&journal_file, &record)?;
                        return Ok(());
                    }
                    if let Ok(value) = read_evidence(&heartbeat) {
                        if value["instanceId"] == config["instanceId"]
                            && (value["runtimeId"] == reply["after"]["runtimeId"]
                                || (reply["outcome"] == "deferred"
                                    && reply["before"]["runtimeId"] == config["runtimeId"]
                                    && reply["after"]["recoveryProbeTaskId"].as_u64().unwrap_or(0)
                                        > 0))
                        {
                            if let Some(next) = Progress::read(&value) {
                                if observed.as_ref().is_some_and(|old| next.advances(old)) {
                                    record.outcome = "recovered".into();
                                    record.attempts = 0;
                                    save_journal(&journal_file, &record)?;
                                    cleanup_evidence(&output)?;
                                    info!(
                                        "AE scheduler progress verified instance={} evidence={}",
                                        instance.instance_id,
                                        result.display()
                                    );
                                    return Ok(());
                                }
                                if observed
                                    .as_ref()
                                    .is_none_or(|old| old.runtime != next.runtime)
                                {
                                    observed = Some(next);
                                }
                            }
                        }
                    }
                }
            }
            host.sleep(Duration::from_millis(500));
        }
        record.outcome = if acknowledged {
            "no-scheduled-progress"
        } else {
            "unacknowledged"
        }
        .into();
        save_journal(&journal_file, &record)?;
        // No periodic resend. Only one observed blocked-to-ready transition permits
        // a second attempt, then real progress or a new AE session is required.
        tracing::warn!("AE scheduler recovery needs attention: {}", record.outcome);
        Ok(())
    }
}

fn recovery_lock(folder: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(folder.join("recovery.lock"))
}

fn cleanup_evidence(folder: &Path) -> Result<()> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(stem) = name
            .strip_suffix(".jsx")
            .or_else(|| name.strip_suffix(".json"))
        else {
            continue;
        };
        let Some((pid, timestamp)) = stem
            .strip_prefix("recovery-")
            .and_then(|s| s.split_once('-'))
        else {
            continue;
        };
        if pid.is_empty()
            || timestamp.is_empty()
            || !pid
                .bytes()
                .chain(timestamp.bytes())
                .all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        files.push((modified, entry.path()));
    }
    files.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (modified, path) in files.into_iter().skip(64) {
        if modified.elapsed().unwrap_or_default() > Duration::from_secs(86400) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn recovery_script(config: &Value) -> String {
    let config = config
        .to_string()
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    include_str!("ae_recovery.jsx").replace("/*CONFIG*/", &config)
}

#[cfg(test)]
mod tests;
