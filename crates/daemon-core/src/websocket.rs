//! UXP initiates an authenticated loopback connection on the broker's TCP port.
//! The existing scheduler owns execution, including while a session is offline.
use super::*;
use bridge_core::{write_atomic_text_file, HostInstance};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::time::Instant;
use tungstenite::{Message, WebSocket};

const PROTOCOL: u32 = 1;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

struct Peer {
    instance: HostInstance,
    sender: Option<mpsc::Sender<Value>>,
    connection: u64,
    pending: Option<(String, mpsc::Sender<Value>)>,
}

#[derive(Default)]
struct HubState {
    peers: HashMap<String, Peer>,
    generation: u64,
    recovery: HashSet<String>,
}

pub(super) struct Hub {
    token: String,
    inner: Mutex<HubState>,
}

impl Hub {
    pub fn new(cfg: &AppConfig, bridge: &BridgeClient) -> Result<Self> {
        let address: std::net::SocketAddr = cfg
            .daemon_addr
            .parse()
            .context("UXP WebSocket daemon_addr must be a numeric loopback address")?;
        if address.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST) {
            return Err(anyhow!("UXP WebSocket requires a 127.0.0.1 listener"));
        }
        let path = cfg.bridge.root_dir.join("connection.json");
        let mut config: Value = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?).context("invalid UXP connection.json")?
        } else {
            json!({"transport": "websocket"})
        };
        if !config.is_object()
            || !matches!(config["transport"].as_str(), Some("websocket" | "file"))
        {
            return Err(anyhow!(
                "connection.json must be an object with transport 'websocket' or 'file'"
            ));
        }
        let token = match config["token"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            Some(value) => value.to_owned(),
            None => {
                let mut bytes = [0u8; 32];
                getrandom::fill(&mut bytes)
                    .map_err(|e| anyhow!("OS random generator failed: {e}"))?;
                bytes.iter().map(|b| format!("{b:02x}")).collect()
            }
        };
        config["token"] = json!(token);
        // UXP rejects IP literals in the manifest's domain allowlist. The server
        // still binds only IPv4 loopback; the plugin uses the localhost origin.
        config["url"] = json!(format!("ws://localhost:{}/uxp", address.port()));
        config["protocolVersion"] = json!(PROTOCOL);
        config["hostId"] = json!(cfg.host_id);
        // A bootstrap setting, not a command/result mailbox. Never log the token.
        write_atomic_text_file(&path, &serde_json::to_vec_pretty(&config)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        let mut inner = HubState::default();
        for record in bridge.unfinished_network_requests()? {
            if record.dispatched_at.is_some() {
                bridge.mark_network_disconnected(&record.request_id)?;
                inner.recovery.insert(record.request_id);
            } else {
                bridge.mark_request_failed(
                    &record.request_id,
                    "Runtime restarted before dispatch; command was not sent.".into(),
                )?;
            }
        }
        Ok(Self {
            token,
            inner: Mutex::new(inner),
        })
    }

    pub fn instances(&self) -> Vec<HostInstance> {
        self.inner
            .lock()
            .expect("WebSocket hub poisoned")
            .peers
            .values()
            .filter(|p| p.sender.is_some())
            .map(|p| p.instance.clone())
            .collect()
    }

    pub fn owns_instance(&self, id: &str) -> bool {
        self.inner
            .lock()
            .expect("WebSocket hub poisoned")
            .peers
            .contains_key(id)
    }

    pub fn recovering(&self) -> bool {
        self.recovery_count() > 0
    }

    pub fn recovery_count(&self) -> usize {
        self.inner
            .lock()
            .expect("WebSocket hub poisoned")
            .recovery
            .len()
    }

    pub fn execute(
        &self,
        bridge: &BridgeClient,
        instance: &HostInstance,
        request_id: &str,
        command: &str,
        args: Value,
    ) -> Result<Value> {
        let (done_tx, done_rx) = mpsc::channel();
        {
            let mut hub = self.inner.lock().expect("WebSocket hub poisoned");
            let peer = hub
                .peers
                .get_mut(&instance.instance_id)
                .ok_or_else(|| anyhow!("UXP instance disconnected before dispatch"))?;
            if peer.instance.runtime_id != instance.runtime_id
                || peer.pending.is_some()
                || peer.sender.is_none()
            {
                return Err(anyhow!(
                    "UXP session unavailable before dispatch; command was not sent"
                ));
            }
            let claimed = bridge.claim_network_request(request_id)?;
            if claimed.dispatched_at.is_none() {
                return Ok(claimed.to_value());
            }
            peer.pending = Some((request_id.to_string(), done_tx));
            let packet = json!({"type":"command", "sessionId":instance.runtime_id, "requestId":request_id, "command":command, "args":args});
            // From this point a delivery error is uncertain, never a failed job
            // that would release FIFO/global exclusion or trigger file fallback.
            if peer.sender.as_ref().unwrap().send(packet).is_err() {
                peer.sender = None;
                let _ = bridge.mark_network_disconnected(request_id);
            }
        }
        done_rx
            .recv()
            .map_err(|_| anyhow!("WebSocket result waiter disconnected"))
    }

    fn register(
        &self,
        state: &DaemonState,
        hello: Value,
        sender: mpsc::Sender<Value>,
    ) -> Result<(String, u64)> {
        if hello["type"] != "hello"
            || hello["protocolVersion"] != PROTOCOL
            || hello["token"].as_str() != Some(&self.token)
        {
            return Err(anyhow!("UXP authentication/protocol rejected"));
        }
        let mut instance: HostInstance =
            serde_json::from_value(hello["instance"].clone()).context("invalid UXP hello")?;
        let session = hello["sessionId"]
            .as_str()
            .filter(|s| valid_id(s))
            .ok_or_else(|| anyhow!("invalid UXP sessionId"))?;
        if instance.host_id != state.cfg.host_id
            || !valid_id(&instance.instance_id)
            || instance.bridge_runtime != "uxp"
        {
            return Err(anyhow!("invalid UXP host/instance identity"));
        }
        instance.lifecycle_mode = Some("websocket".into());
        instance.runtime_id = Some(session.into());
        instance.bridge_root = state.cfg.bridge.root_dir.display().to_string();
        instance.command_file.clear();
        instance.result_file.clear();
        instance.heartbeat_path = None;
        instance.last_heartbeat_at = chrono::Utc::now().to_rfc3339();
        let id = instance.instance_id.clone();
        let mut hub = self.inner.lock().expect("WebSocket hub poisoned");
        if let Some(peer) = hub.peers.get(&id) {
            if peer.instance.runtime_id != instance.runtime_id
                && (peer.sender.is_some() || peer.pending.is_some())
            {
                return Err(anyhow!(
                    "UXP instance is owned by another live or unresolved session"
                ));
            }
        }
        hub.generation += 1;
        let connection = hub.generation;
        let pending = hub.peers.remove(&id).and_then(|p| p.pending);
        hub.peers.insert(
            id.clone(),
            Peer {
                instance,
                sender: Some(sender),
                connection,
                pending,
            },
        );
        Ok((id, connection))
    }

    fn receive(&self, state: &DaemonState, id: &str, connection: u64, packet: Value) -> Result<()> {
        let mut hub = self.inner.lock().expect("WebSocket hub poisoned");
        let peer = hub
            .peers
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown UXP instance"))?;
        if peer.connection != connection
            || packet["sessionId"].as_str() != peer.instance.runtime_id.as_deref()
        {
            return Err(anyhow!("stale UXP connection/session"));
        }
        peer.instance.last_heartbeat_at = chrono::Utc::now().to_rfc3339();
        match packet["type"].as_str() {
            Some("heartbeat") => {
                peer.instance.status = packet["status"].as_str().map(str::to_string);
                peer.instance.current_request_id =
                    packet["currentRequestId"].as_str().map(str::to_string);
                peer.sender.as_ref().unwrap().send(json!({"type":"pong"}))?;
            }
            Some("received") => { /* dispatch was already durably recorded */ }
            Some("result") => {
                let request_id = packet["requestId"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing result requestId"))?;
                let record = state.bridge.complete_network_request(
                    &peer.instance,
                    request_id,
                    packet["result"].clone(),
                )?;
                // Enqueue ACK before waking the scheduler, preserving wire order.
                peer.sender.as_ref().unwrap().send(json!({"type":"resultAck", "requestId":request_id, "sessionId":peer.instance.runtime_id}))?;
                if peer
                    .pending
                    .as_ref()
                    .is_some_and(|(id, _)| id == request_id)
                {
                    let (_, done) = peer.pending.take().unwrap();
                    let _ = done.send(record.to_value());
                }
                hub.recovery.remove(request_id);
            }
            _ => return Err(anyhow!("unknown UXP message")),
        }
        Ok(())
    }

    fn disconnect(&self, bridge: &BridgeClient, id: &str, connection: u64) {
        let mut hub = self.inner.lock().expect("WebSocket hub poisoned");
        if let Some(peer) = hub.peers.get_mut(id) {
            if peer.connection == connection {
                peer.sender = None;
                if let Some((request_id, _)) = &peer.pending {
                    let _ = bridge.mark_network_disconnected(request_id);
                }
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn send(socket: &mut WebSocket<TcpStream>, value: Value) -> Result<()> {
    socket.send(Message::Text(serde_json::to_string(&value)?.into()))?;
    Ok(())
}

pub(super) fn serve(stream: TcpStream, state: Arc<DaemonState>) -> Result<()> {
    if !stream.peer_addr()?.ip().is_loopback() {
        return Err(anyhow!("UXP requires loopback"));
    }
    let hub = state
        .websocket
        .as_ref()
        .ok_or_else(|| anyhow!("WebSocket is unsupported for this host"))?;
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(16 * 1024 * 1024))
        .max_frame_size(Some(16 * 1024 * 1024));
    let mut socket = tungstenite::accept_with_config(stream, Some(config))?;
    let hello: Value = serde_json::from_str(socket.read()?.to_text()?)?;
    let (out_tx, out_rx) = mpsc::channel();
    let (id, connection) = hub.register(&state, hello, out_tx)?;
    let result = (|| -> Result<()> {
        send(
            &mut socket,
            json!({"type":"welcome", "protocolVersion":PROTOCOL, "maxResultBytes":state.cfg.script_contract.max_result_bytes}),
        )?;
        socket
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(50)))?;
        let mut last_seen = Instant::now();
        loop {
            {
                let status = state.lifecycle.lock().expect("lifecycle mutex poisoned");
                if !status.accepting && status.pending_jobs == 0 && !hub.recovering() {
                    return Ok(());
                }
            }
            // A replaced connection cannot dispatch or publish any messages.
            if hub
                .inner
                .lock()
                .expect("WebSocket hub poisoned")
                .peers
                .get(&id)
                .is_none_or(|p| p.connection != connection)
            {
                return Ok(());
            }
            for packet in out_rx.try_iter() {
                send(&mut socket, packet)?;
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    last_seen = Instant::now();
                    hub.receive(&state, &id, connection, serde_json::from_str(&text)?)?;
                }
                Ok(Message::Close(_)) => return Ok(()),
                Ok(_) => {
                    socket.flush()?;
                }
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    if last_seen.elapsed() > IDLE_TIMEOUT {
                        return Err(anyhow!("UXP heartbeat timed out"));
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    })();
    hub.disconnect(&state.bridge, &id, connection);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcp_core::BridgePaths;

    fn runtime() -> (ManagedDaemon, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let root = dir.path().to_path_buf();
        let cfg = AppConfig {
            host_id: "photoshop".into(),
            daemon_addr: listener.local_addr().unwrap().to_string(),
            bridge: BridgePaths {
                root_dir: root.clone(),
                command_file: root.join("ps_command.json"),
                result_file: root.join("ps_mcp_result.json"),
            },
            ..AppConfig::default()
        };
        (start_embedded_with_listener(cfg, listener).unwrap(), dir)
    }

    fn hello(state: &DaemonState, id: &str, session: &str) -> Value {
        json!({"type":"hello", "protocolVersion":1, "token":state.websocket.as_ref().unwrap().token,
        "sessionId":session, "instance":{
            "instanceId":id,"hostId":"photoshop","bridgeRuntime":"uxp", "appVersion":"26.0",
            "bridgeRoot":"", "commandFile":"", "resultFile":"", "lastHeartbeatAt":""
        }})
    }

    fn connect(state: &DaemonState, hello: Value) -> WebSocket<TcpStream> {
        let tcp = TcpStream::connect(&state.cfg.daemon_addr).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let (mut ws, _) =
            tungstenite::client(format!("ws://{}/uxp", state.cfg.daemon_addr), tcp).unwrap();
        send(&mut ws, hello).unwrap();
        ws
    }

    fn packet(ws: &mut WebSocket<TcpStream>) -> Value {
        serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap()
    }

    fn command(
        state: &Arc<DaemonState>,
        id: &str,
        exclusive: bool,
        timeout: u64,
    ) -> thread::JoinHandle<Value> {
        let state = Arc::clone(state);
        let id = id.to_string();
        thread::spawn(move || {
            handle_request(&state, serde_json::from_value(json!({
            "op":"runCommand", "command":"ping", "targetInstanceId":id, "timeoutMs":timeout, "globalExclusive":exclusive
        })).unwrap()).unwrap()
        })
    }

    fn finish(ws: &mut WebSocket<TcpStream>, request: &Value, session: &str) {
        send(ws, json!({"type":"result", "sessionId":session, "requestId":request["requestId"], "result":{
            "status":"success", "_requestId":request["requestId"], "_commandExecuted":request["command"], "message":"ok"
        }})).unwrap();
        assert_eq!(packet(ws)["type"], "resultAck");
    }

    #[test]
    fn websocket_auth_and_host_identity_are_required() {
        let (runtime, _dir) = runtime();
        for mutate in ["token", "host"] {
            let mut greeting = hello(&runtime.state, "ps-test", "session-a");
            if mutate == "token" {
                greeting["token"] = json!("wrong");
            } else {
                greeting["instance"]["hostId"] = json!("premiere");
            }
            let mut ws = connect(&runtime.state, greeting);
            assert!(ws.read().is_err());
        }
        assert!(runtime.state.instances().unwrap().instances.is_empty());
    }

    #[test]
    fn reconnect_recovers_result_without_resend_and_retains_exclusive_gate() {
        let (runtime, _dir) = runtime();
        let state = &runtime.state;
        let mut a = connect(state, hello(state, "ps-a", "session-a"));
        let mut b = connect(state, hello(state, "ps-b", "session-b"));
        assert_eq!(packet(&mut a)["type"], "welcome");
        assert_eq!(packet(&mut b)["type"], "welcome");
        let first = command(state, "ps-a", true, 100);
        let sent = packet(&mut a);
        assert_eq!(sent["type"], "command");
        assert_eq!(first.join().unwrap()["status"], "timeout");
        a.get_mut().shutdown(std::net::Shutdown::Both).unwrap();
        let second = command(state, "ps-b", false, 100);
        assert_eq!(
            second.join().unwrap()["status"],
            "timeout",
            "exclusive gate remains held while offline"
        );
        let request_id = sent["requestId"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.bridge.get_request_record(request_id).unwrap().status != "unknown" {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let mut resumed = connect(state, hello(state, "ps-a", "session-a"));
        assert_eq!(packet(&mut resumed)["type"], "welcome");
        finish(&mut resumed, &sent, "session-a"); // next frame must be ACK, never command redelivery
        assert_eq!(
            state.bridge.get_request_record(request_id).unwrap().status,
            "completed"
        );
        let second_sent = packet(&mut b);
        assert_eq!(second_sent["type"], "command");
        finish(&mut b, &second_sent, "session-b");
        finish(&mut resumed, &sent, "session-a"); // idempotent replay after a lost ACK
        assert!(!state.cfg.bridge.command_file.exists());
        assert!(!state.cfg.bridge.result_file.exists());
        runtime.begin_shutdown();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !runtime.is_finished() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn restart_recovers_only_original_session_and_blocks_new_dispatch() {
        let (runtime, _dir) = runtime();
        let state = &runtime.state;
        let mut ws = connect(state, hello(state, "ps-a", "session-a"));
        packet(&mut ws);
        let instance = state.instances().unwrap().instances.remove(0);
        let prepared = state
            .bridge
            .prepare_request("ping", 300, Some(instance.clone()))
            .unwrap();
        state
            .bridge
            .claim_network_request(&prepared.record.request_id)
            .unwrap();
        // Rebuild the hub from durable records, as after process loss.
        let recovered = make_state(state.cfg.clone()).unwrap();
        assert!(recovered.websocket.as_ref().unwrap().recovering());
        let request = json!({"op":"runCommand", "command":"ping"});
        assert!(handle_request(&recovered, serde_json::from_value(request).unwrap()).is_err());
        let mut wrong = instance.clone();
        wrong.runtime_id = Some("session-b".into());
        let result = json!({"status":"success", "_requestId":prepared.record.request_id, "_commandExecuted":"ping"});
        assert!(state
            .bridge
            .complete_network_request(&wrong, &prepared.record.request_id, result.clone())
            .is_err());
        let hub = recovered.websocket.as_ref().unwrap();
        let (tx, rx) = mpsc::channel();
        let (id, connection) = hub
            .register(&recovered, hello(&recovered, "ps-a", "session-a"), tx)
            .unwrap();
        hub.receive(&recovered, &id, connection, json!({"type":"result", "sessionId":"session-a", "requestId":prepared.record.request_id, "result":result})).unwrap();
        assert_eq!(rx.recv().unwrap()["type"], "resultAck");
        assert!(!hub.recovering());
    }
}
