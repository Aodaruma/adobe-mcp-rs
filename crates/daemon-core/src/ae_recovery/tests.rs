use super::*;

#[derive(Clone, Copy, PartialEq)]
enum Reply {
    Progress,
    DeferredProgress,
    DeferredNoProgress,
    NoProgress,
    NoAck,
    WrongNonce,
    Refused,
    LaunchError,
}

struct FakeHost {
    process: Option<AeProcess>,
    process_reads: usize,
    replace_process: bool,
    ready: bool,
    block_after_launch: bool,
    now: u64,
    launches: usize,
    reply: Reply,
    heartbeat: PathBuf,
    config: Option<Value>,
}
impl NativeHost for FakeHost {
    fn process(&mut self) -> Result<Option<AeProcess>> {
        self.process_reads += 1;
        if self.replace_process && self.process_reads > 1 {
            return Ok(None);
        }
        Ok(self.process.clone())
    }
    fn windows_ready(&mut self, _pid: u32) -> bool {
        self.ready && !(self.block_after_launch && self.launches > 0)
    }
    fn now_ms(&self) -> u64 {
        self.now
    }
    fn sleep(&mut self, duration: Duration) {
        self.now += duration.as_millis() as u64;
        if matches!(self.reply, Reply::Progress | Reply::DeferredProgress) && self.launches > 0 {
            let mut heartbeat = read_evidence(&self.heartbeat).unwrap();
            heartbeat["runtimeId"] = json!("recovered-runtime");
            heartbeat["schedulerReady"] = json!(true);
            for key in [
                "commandTickSequence",
                "heartbeatTickSequence",
                "watchdogTickSequence",
            ] {
                heartbeat[key] = json!(heartbeat[key].as_u64().unwrap_or(0) + 1);
            }
            fs::write(&self.heartbeat, heartbeat.to_string()).unwrap();
        }
    }
    fn launch(&mut self, executable: &Path, script: &Path) -> Result<()> {
        assert_eq!(executable.file_name().unwrap(), "AfterFX.com");
        self.launches += 1;
        if self.reply == Reply::LaunchError {
            anyhow::bail!("simulated launcher failure");
        }
        let source = fs::read_to_string(script)?;
        assert!(!source.contains("app.project"));
        let config: Value = serde_json::from_str(
            source
                .rsplit_once("})(")
                .unwrap()
                .1
                .trim()
                .strip_suffix(");")
                .unwrap(),
        )?;
        self.config = Some(config.clone());
        if self.reply != Reply::NoAck {
            let nonce = if self.reply == Reply::WrongNonce {
                json!("not-this-attempt")
            } else {
                config["nonce"].clone()
            };
            let deferred = matches!(
                self.reply,
                Reply::DeferredProgress | Reply::DeferredNoProgress
            );
            let outcome = if self.reply == Reply::Refused {
                "refused"
            } else if deferred {
                "deferred"
            } else {
                "rearmed"
            };
            fs::write(config["resultFile"].as_str().unwrap(), json!({
                    "nonce": nonce, "outcome": outcome,
                    "before": { "runtimeId": config["runtimeId"] },
                    "after": { "runtimeId": if deferred { "original-runtime" } else { "recovered-runtime" },
                        "recoveryProbeTaskId": if deferred { 123 } else { 0 } }
                }).to_string())?;
        }
        Ok(())
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    state: Arc<DaemonState>,
    host: FakeHost,
}
impl Fixture {
    fn new(reply: Reply) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bridge");
        let cfg = AppConfig {
            bridge: mcp_core::BridgePaths {
                root_dir: root.clone(),
                command_file: root.join("ae_command.json"),
                result_file: root.join("ae_mcp_result.json"),
            },
            ae_native_recovery_enabled: true,
            ..AppConfig::default()
        };
        let state = make_state(cfg).unwrap();
        let folder = root.join("instances/fixture-ae");
        fs::create_dir_all(&folder).unwrap();
        let heartbeat = folder.join("heartbeat.json");
        fs::write(&heartbeat, json!({
                "instanceId":"fixture-ae", "hostId":"aftereffects", "appVersion":"25.6.6x4",
                "bridgeVersion":"0.5.5", "bridgeRuntime":"extendscript-startup", "lifecycleMode":"startup-headless",
                "status":"idle", "currentRequestId":null, "renderQueueActive":false,
                "runtimeId":"original-runtime", "runtimeStartedAt":"2026-10-22T21:28:36Z",
                "bridgeRoot":root, "commandFile":folder.join("ae_command.json"), "resultFile":folder.join("ae_mcp_result.json"),
                "lastHeartbeatAt":"2026-10-22T21:28:36Z", "schedulerReady":false,
                "commandTickSequence":0, "heartbeatTickSequence":0, "watchdogTickSequence":0
            }).to_string()).unwrap();
        stale_file(&heartbeat);
        fs::write(dir.path().join("AfterFX.com"), b"fake, never executed").unwrap();
        let host = FakeHost {
            process: Some(AeProcess {
                pid: 42,
                started_ms: 1_792_704_500_000,
                executable: dir.path().join("AfterFX.exe"),
            }),
            process_reads: 0,
            replace_process: false,
            ready: true,
            block_after_launch: false,
            now: 1_792_705_000_000,
            launches: 0,
            reply,
            heartbeat,
            config: None,
        };
        Self {
            _dir: dir,
            state,
            host,
        }
    }
    fn journal(&self) -> Journal {
        read_journal(
            &self
                .state
                .cfg
                .bridge
                .root_dir
                .join("native-recovery/state.json"),
        )
        .unwrap()
        .unwrap()
    }
    fn poll(&mut self, monitor: &mut Monitor) {
        monitor.poll(&self.state, &mut self.host).unwrap();
    }
}
fn stale_file(path: &Path) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(60)))
        .unwrap();
}

#[test]
fn monitor_verifies_tick_progress_and_never_creates_a_user_command() {
    let mut f = Fixture::new(Reply::Progress);
    f.poll(&mut Monitor::default());
    assert_eq!(f.host.launches, 1);
    assert_eq!(f.journal().outcome, "recovered");
    assert_eq!(f.journal().attempts, 0);
    assert!(!f.host.heartbeat.with_file_name("ae_command.json").exists());
    let status = f.state.lifecycle.lock().unwrap();
    assert!(!status.recovering);
    assert_eq!(status.pending_jobs, 0);
}

#[test]
fn deferred_recovery_requires_observed_progress_after_the_probe_restarts_the_runtime() {
    for (reply, expected) in [
        (Reply::DeferredProgress, "recovered"),
        (Reply::DeferredNoProgress, "no-scheduled-progress"),
    ] {
        let mut f = Fixture::new(reply);
        f.poll(&mut Monitor::default());
        assert_eq!(f.host.launches, 1);
        assert_eq!(f.journal().outcome, expected);
        assert!(!f.host.heartbeat.with_file_name("ae_command.json").exists());
    }
}

#[test]
fn deferred_recovery_resets_its_baseline_when_old_ready_heartbeat_precedes_new_generation() {
    let mut f = Fixture::new(Reply::DeferredProgress);
    let mut heartbeat = read_evidence(&f.host.heartbeat).unwrap();
    heartbeat["schedulerReady"] = json!(true);
    for key in [
        "commandTickSequence",
        "heartbeatTickSequence",
        "watchdogTickSequence",
    ] {
        heartbeat[key] = json!(20);
    }
    fs::write(&f.host.heartbeat, heartbeat.to_string()).unwrap();
    stale_file(&f.host.heartbeat);
    f.poll(&mut Monitor::default());
    assert_eq!(f.journal().outcome, "recovered");
    assert_eq!(f.host.launches, 1);
}

#[test]
fn acknowledgement_without_progress_is_not_recovery_and_is_not_retried() {
    let mut f = Fixture::new(Reply::NoProgress);
    f.poll(&mut Monitor::default());
    assert_eq!(f.journal().outcome, "no-scheduled-progress");
    f.host.now += 60_000;
    f.poll(&mut Monitor::default());
    assert_eq!(
        f.host.launches, 1,
        "new monitor/broker must retain the failure budget"
    );
}

#[test]
fn no_ack_never_retries_on_time_alone_and_allows_only_one_observed_dismissal_retry() {
    let mut f = Fixture::new(Reply::NoAck);
    f.poll(&mut Monitor::default());
    assert_eq!(f.journal().outcome, "unacknowledged");
    for _ in 0..3 {
        f.host.now += 60_000;
        f.poll(&mut Monitor::default());
    }
    assert_eq!(f.host.launches, 1);
    f.host.ready = false;
    f.poll(&mut Monitor::default());
    assert!(f.journal().saw_blocked_window);
    f.host.ready = true;
    f.poll(&mut Monitor::default());
    assert_eq!(f.host.launches, 2);
    for _ in 0..3 {
        f.host.ready = false;
        f.poll(&mut Monitor::default());
        f.host.ready = true;
        f.host.now += 60_000;
        f.poll(&mut Monitor::default());
    }
    assert_eq!(
        f.host.launches, 2,
        "repeated dialog cycles must not cause an alert loop"
    );
}

#[test]
fn visible_modal_is_never_sent_a_native_script() {
    let mut f = Fixture::new(Reply::Progress);
    f.host.ready = false;
    f.poll(&mut Monitor::default());
    assert_eq!(f.host.launches, 0);
    f.host.ready = true;
    f.poll(&mut Monitor::default());
    assert_eq!(f.journal().outcome, "recovered");
}

#[test]
fn busy_draining_or_reserved_instances_are_never_recovered() {
    for reason in [
        "accepted-job",
        "draining",
        "reservation",
        "pending",
        "running",
    ] {
        let mut f = Fixture::new(Reply::Progress);
        let mut permit = None;
        match reason {
            "accepted-job" => permit = Some(JobPermit::acquire(&f.state.lifecycle).unwrap()),
            "draining" => f.state.lifecycle.lock().unwrap().accepting = false,
            "reservation" => fs::write(
                f.host.heartbeat.with_file_name("current_request.json"),
                "uncertain",
            )
            .unwrap(),
            status => fs::write(
                f.host.heartbeat.with_file_name("ae_command.json"),
                json!({"status":status}).to_string(),
            )
            .unwrap(),
        }
        f.host.now += 3_600_000;
        f.poll(&mut Monitor::default());
        assert_eq!(f.host.launches, 0, "{reason}");
        drop(permit);
        if reason == "reservation" {
            assert!(f
                .host
                .heartbeat
                .with_file_name("current_request.json")
                .exists());
        }
    }
}

#[test]
fn recovery_permit_preserves_drain_and_admission_without_holding_the_status_mutex() {
    let f = Fixture::new(Reply::Progress);
    let permit = RecoveryPermit::acquire(&f.state, false).unwrap();
    assert!(f.state.lifecycle.try_lock().is_ok());
    assert!(JobPermit::acquire(&f.state.lifecycle).is_err());
    assert!(RecoveryPermit::acquire(&f.state, false).is_none());
    f.state.lifecycle.lock().unwrap().accepting = false;
    drop(permit);
    let status = f.state.lifecycle.lock().unwrap();
    assert!(!status.accepting);
    assert!(!status.recovering);
    assert_eq!(status.pending_jobs, 0);
}

#[test]
fn absent_or_changed_process_and_ambiguous_heartbeats_prevent_launch() {
    for reason in ["absent", "changed", "ambiguous"] {
        let mut f = Fixture::new(Reply::Progress);
        match reason {
            "absent" => f.host.process = None,
            "changed" => f.host.replace_process = true,
            _ => {
                let folder = f
                    .host
                    .heartbeat
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("other");
                fs::create_dir_all(&folder).unwrap();
                let mut value = read_evidence(&f.host.heartbeat).unwrap();
                value["instanceId"] = json!("other");
                value["commandFile"] = json!(folder.join("ae_command.json"));
                value["resultFile"] = json!(folder.join("ae_mcp_result.json"));
                fs::write(folder.join("heartbeat.json"), value.to_string()).unwrap();
                stale_file(&folder.join("heartbeat.json"));
            }
        }
        f.poll(&mut Monitor::default());
        assert_eq!(f.host.launches, 0, "{reason}");
    }
}

#[test]
fn wrong_nonce_refusal_launch_failure_and_corrupt_journal_fail_closed() {
    for reply in [Reply::WrongNonce, Reply::Refused, Reply::LaunchError] {
        let mut f = Fixture::new(reply);
        let result = Monitor::default().poll(&f.state, &mut f.host);
        assert_eq!(result.is_err(), reply == Reply::LaunchError);
        assert_ne!(f.journal().outcome, "recovered");
        f.poll(&mut Monitor::default());
        assert_eq!(f.host.launches, 1);
        assert_eq!(f.state.lifecycle.lock().unwrap().pending_jobs, 0);
    }
    let mut f = Fixture::new(Reply::Progress);
    let folder = f.state.cfg.bridge.root_dir.join("native-recovery");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("state.json"), "{").unwrap();
    assert!(Monitor::default().poll(&f.state, &mut f.host).is_err());
    assert_eq!(f.host.launches, 0);
}

fn add_stale_duplicate(f: &Fixture) {
    let folder = f.state.cfg.bridge.root_dir.join("instances/duplicate");
    fs::create_dir_all(&folder).unwrap();
    let mut value = read_evidence(&f.host.heartbeat).unwrap();
    value["instanceId"] = json!("duplicate");
    value["runtimeId"] = json!("duplicate-runtime");
    value["commandFile"] = json!(folder.join("ae_command.json"));
    value["resultFile"] = json!(folder.join("ae_mcp_result.json"));
    value["schedulerReady"] = json!(false);
    for key in [
        "commandTickSequence",
        "heartbeatTickSequence",
        "watchdogTickSequence",
    ] {
        value[key] = json!(0);
    }
    fs::write(folder.join("heartbeat.json"), value.to_string()).unwrap();
    stale_file(&folder.join("heartbeat.json"));
}

#[test]
fn previously_verified_session_recovers_despite_abandoned_bootstrap_records() {
    let mut f = Fixture::new(Reply::Progress);
    f.poll(&mut Monitor::default());
    stale_file(&f.host.heartbeat);
    add_stale_duplicate(&f);
    f.host.now += 60_000;
    f.poll(&mut Monitor::default());
    assert_eq!(f.host.launches, 2);
    assert_eq!(f.journal().instance_id, "fixture-ae");
    assert_eq!(f.journal().outcome, "recovered");
    assert!(f
        .state
        .cfg
        .bridge
        .root_dir
        .join("instances/duplicate/heartbeat.json")
        .exists());
}

#[test]
fn duplicate_resolution_never_uses_a_previous_process_or_resets_a_failed_attempt() {
    for change in ["pid", "started", "budget"] {
        let mut f = Fixture::new(Reply::NoAck);
        f.poll(&mut Monitor::default());
        let mut journal = f.journal();
        match change {
            "pid" => journal.process_id += 1,
            "started" => journal.process_started_ms += 1,
            _ => {}
        }
        save_journal(
            &f.state
                .cfg
                .bridge
                .root_dir
                .join("native-recovery/state.json"),
            &journal,
        )
        .unwrap();
        add_stale_duplicate(&f);
        f.host.now += 60_000;
        f.poll(&mut Monitor::default());
        assert_eq!(f.host.launches, 1, "{change}");
    }
}

#[test]
fn recovered_status_expires_when_the_verified_heartbeat_stops() {
    let mut f = Fixture::new(Reply::Progress);
    f.poll(&mut Monitor::default());
    assert_eq!(
        f.state.native_recovery_status().as_deref(),
        Some("recovered")
    );
    stale_file(&f.host.heartbeat);
    assert_eq!(f.state.native_recovery_status().as_deref(), Some("stale"));
    let mut value = read_evidence(&f.host.heartbeat).unwrap();
    value["schedulerReady"] = json!(false);
    value["runtimeId"] = json!("fresh-zero-tick-runtime");
    fs::write(&f.host.heartbeat, value.to_string()).unwrap();
    assert_eq!(
        f.state.native_recovery_status().as_deref(),
        Some("awaiting-progress")
    );
}

#[test]
fn manual_repair_resets_budget_only_after_all_three_counters_advance() {
    let mut f = Fixture::new(Reply::NoAck);
    let mut monitor = Monitor::default();
    f.poll(&mut monitor);
    let mut value = read_evidence(&f.host.heartbeat).unwrap();
    value["schedulerReady"] = json!(true);
    for key in [
        "commandTickSequence",
        "heartbeatTickSequence",
        "watchdogTickSequence",
    ] {
        value[key] = json!(1);
    }
    fs::write(&f.host.heartbeat, value.to_string()).unwrap();
    f.poll(&mut monitor);
    f.poll(&mut monitor);
    assert_ne!(f.journal().outcome, "recovered");
    for key in [
        "commandTickSequence",
        "heartbeatTickSequence",
        "watchdogTickSequence",
    ] {
        value[key] = json!(2);
    }
    fs::write(&f.host.heartbeat, value.to_string()).unwrap();
    f.poll(&mut monitor);
    assert_eq!(f.journal().outcome, "recovered");
}

#[test]
fn wrapper_escapes_es3_line_separators_and_cleanup_only_removes_old_owned_files() {
    let script = recovery_script(&json!({"resultFile":"a\u{2028}b\u{2029}c"}));
    assert!(!script.contains('\u{2028}'));
    assert!(script.contains("\\u2028"));
    let dir = tempfile::tempdir().unwrap();
    for n in 0..70 {
        let path = dir.path().join(format!("recovery-1-{n}.jsx"));
        fs::write(&path, "expired wrapper").unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::now() - Duration::from_secs(90000 + n)),
            )
            .unwrap();
    }
    fs::write(dir.path().join("recovery-personal-notes.jsx"), "keep").unwrap();
    fs::write(dir.path().join("state.json"), "keep").unwrap();
    cleanup_evidence(dir.path()).unwrap();
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 66);
    assert_eq!(
        fs::read_to_string(dir.path().join("recovery-personal-notes.jsx")).unwrap(),
        "keep"
    );
}

#[test]
fn candidates_exclude_other_versions_old_sessions_rendering_and_uncertain_requests() {
    let process = AeProcess {
        pid: 1,
        started_ms: 1_792_704_500_000,
        executable: "AfterFX.exe".into(),
    };
    let value = json!({"hostId":"aftereffects","status":"idle","currentRequestId":null,
            "renderQueueActive":false,"bridgeVersion":"0.5.5","appVersion":"25.6.6x4",
            "runtimeId":"runtime-new","runtimeStartedAt":"Thursday, 22 Oct 2026 21:28:36 GMT"});
    assert!(candidate(&value, &process));
    for (field, invalid) in [
        ("status", json!("running")),
        ("currentRequestId", json!("unresolved")),
        ("renderQueueActive", json!(true)),
        ("appVersion", json!("26.0")),
        ("bridgeVersion", json!("0.5.3")),
        ("runtimeId", Value::Null),
        (
            "runtimeStartedAt",
            json!("Thursday, 22 Oct 2026 21:20:00 GMT"),
        ),
    ] {
        let mut changed = value.clone();
        changed[field] = invalid;
        assert!(!candidate(&changed, &process), "{field}");
    }
}

#[test]
fn reservation_and_uncertain_mailbox_always_prevent_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("ae_command.json");
    assert_eq!(mailbox_admission(dir.path(), &command, 1048576), Some(None));
    for status in ["pending", "running", "unknown"] {
        fs::write(&command, json!({"status":status}).to_string()).unwrap();
        assert_eq!(mailbox_admission(dir.path(), &command, 1048576), None);
    }
    fs::write(&command, r#"{"status":"completed"}"#).unwrap();
    assert_eq!(mailbox_admission(dir.path(), &command, 1048576), Some(None));
    fs::write(dir.path().join("current_request.json"), "{}").unwrap();
    assert_eq!(mailbox_admission(dir.path(), &command, 1048576), None);
}

#[test]
fn a_correlated_pending_mailbox_can_rearm_without_releasing_its_worker_or_reservation() {
    let mut f = Fixture::new(Reply::Progress);
    let permit = JobPermit::acquire(&f.state.lifecycle).unwrap();
    let command = f.host.heartbeat.with_file_name("ae_command.json");
    let reservation = f.host.heartbeat.with_file_name("current_request.json");
    let command_bytes =
        json!({"status":"pending","requestId":"req-pending","command":"executeJsx"}).to_string();
    let reservation_bytes =
        json!({"status":"running","requestId":"req-pending","command":"executeJsx"}).to_string();
    fs::write(&command, &command_bytes).unwrap();
    fs::write(&reservation, &reservation_bytes).unwrap();
    f.poll(&mut Monitor::default());
    assert_eq!(f.host.launches, 1);
    assert_eq!(
        f.host.config.as_ref().unwrap()["pendingRequestId"],
        "req-pending"
    );
    assert_eq!(f.journal().outcome, "recovered");
    assert_eq!(fs::read_to_string(&command).unwrap(), command_bytes);
    assert_eq!(fs::read_to_string(&reservation).unwrap(), reservation_bytes);
    assert_eq!(f.state.lifecycle.lock().unwrap().pending_jobs, 1);
    drop(permit);
}

#[test]
fn pending_mailbox_admission_requires_matching_reservation_and_never_accepts_running() {
    for mode in [
        "missing",
        "different-id",
        "different-command",
        "running",
        "unsafe-id",
    ] {
        let f = Fixture::new(Reply::Progress);
        let folder = f.host.heartbeat.parent().unwrap();
        let command_file = folder.join("ae_command.json");
        let mut command =
            json!({"status":"pending","requestId":"req-pending","command":"executeJsx"});
        let mut reservation =
            json!({"status":"running","requestId":"req-pending","command":"executeJsx"});
        match mode {
            "different-id" => reservation["requestId"] = json!("different"),
            "different-command" => reservation["command"] = json!("other"),
            "running" => command["status"] = json!("running"),
            "unsafe-id" => {
                command["requestId"] = json!("../bad");
                reservation["requestId"] = json!("../bad");
            }
            _ => (),
        }
        fs::write(&command_file, command.to_string()).unwrap();
        if mode != "missing" {
            fs::write(folder.join("current_request.json"), reservation.to_string()).unwrap();
        }
        assert_eq!(
            mailbox_admission(folder, &command_file, 1048576),
            None,
            "{mode}"
        );
    }
}

#[test]
fn pending_mailbox_uses_the_configured_payload_bound_instead_of_the_small_diagnostic_limit() {
    let f = Fixture::new(Reply::Progress);
    let folder = f.host.heartbeat.parent().unwrap();
    let command = folder.join("ae_command.json");
    fs::write(
        &command,
        json!({"status":"pending","requestId":"large-request","command":"executeJsx",
        "args":{"code":"x".repeat(200_000)}})
        .to_string(),
    )
    .unwrap();
    fs::write(
        folder.join("current_request.json"),
        json!({"status":"running","requestId":"large-request","command":"executeJsx"}).to_string(),
    )
    .unwrap();
    assert_eq!(
        mailbox_admission(folder, &command, 1_048_576),
        Some(Some("large-request".into()))
    );
    assert_eq!(
        mailbox_admission(folder, &command, MAX_EVIDENCE_BYTES),
        None
    );
}
