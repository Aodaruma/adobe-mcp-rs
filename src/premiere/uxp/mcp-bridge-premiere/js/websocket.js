// Kept identical in both packaged UXP plugins (checked by the runtime tests).
// Only bootstrap settings touch disk; commands, results and heartbeat use WS.
module.exports = function createTransport(options) {
  var socket = null;
  var timer = null;
  var stopped = true;
  var connecting = false;
  var welcomed = false;
  var generation = 0;
  var retryAt = 0;
  var backoff = 500;
  var lastSeen = 0;
  var lastHeartbeat = 0;
  var maxResultBytes = 1024 * 1024;
  var active = null;
  var pending = Object.create(null);
  var seen = Object.create(null);
  var session = 'uxp-' + Date.now() + '-' + Math.random().toString(36).slice(2);

  function status(value, message) { options.status(value, message); }
  function send(packet) {
    if (!socket || socket.readyState !== 1 || !welcomed) { return false; }
    packet.sessionId = session;
    socket.send(JSON.stringify(packet));
    return true;
  }
  function disconnect() {
    generation += 1;
    var previous = socket;
    socket = null;
    welcomed = false;
    connecting = false;
    if (previous) { try { previous.close(); } catch (_e) {} }
    retryAt = Date.now() + backoff + Math.floor(Math.random() * 250);
    backoff = Math.min(backoff * 2, 10000);
    if (!stopped) { status('disconnected', 'Waiting to reconnect to Adobe MCP.'); }
  }
  function flushResults() {
    Object.keys(pending).forEach(function (id) { send(pending[id]); });
  }
  async function execute(packet) {
    var id = packet.requestId;
    if (typeof id !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(id) || typeof packet.command !== 'string') {
      throw new Error('Invalid command envelope');
    }
    if (seen[id]) {
      // Replayed delivery must never execute twice, including after an ACK.
      if (pending[id]) { send(pending[id]); }
      return;
    }
    if (active || Object.keys(pending).length) { throw new Error('Unexpected concurrent command'); }
    seen[id] = true;
    active = id;
    status('running', 'Executing: ' + packet.command);
    // Even if the receipt cannot be delivered, this command has been accepted.
    try { send({ type: 'received', requestId: id }); } catch (_e) { disconnect(); }
    var result;
    try {
      if (!options.enabled()) { throw new Error('Command reception is paused.'); }
      result = await options.execute(packet.command, packet.args || {}, id);
    } catch (error) {
      result = { status: 'error', message: String(error.message || error) };
    }
    result._requestId = id;
    result._commandExecuted = packet.command;
    result._responseTimestamp = new Date().toISOString();
    try {
      var text = JSON.stringify(result);
      var bytes = encodeURIComponent(text).replace(/%[0-9A-F]{2}/gi, 'x').length;
      if (bytes > maxResultBytes) { throw new Error('Result exceeds maxResultBytes; return artifact metadata instead.'); }
    } catch (error) {
      result = { status: 'error', message: String(error.message || error), _requestId: id, _commandExecuted: packet.command };
    }
    pending[id] = { type: 'result', requestId: id, result: result };
    active = null;
    status('waiting', 'Result retained until the application acknowledges it.');
    try { send(pending[id]); } catch (_e) { disconnect(); }
  }
  async function connect() {
    if (stopped || connecting || socket) { return; }
    connecting = true;
    var current = generation;
    try {
      var config = options.config();
      if (!config || config.transport !== 'websocket' || config.protocolVersion !== 1 ||
          config.hostId !== options.hostId || !/^[0-9a-f]{64}$/i.test(config.token || '') ||
          !/^ws:\/\/localhost:[0-9]{1,5}\/uxp$/.test(config.url || '')) {
        throw new Error('Start Adobe MCP with a 127.0.0.1 listener to create connection.json.');
      }
      var instance = await options.instance();
      if (stopped || current !== generation) { return; }
      var connection = new options.WebSocket(config.url);
      socket = connection;
      lastSeen = Date.now();
      connection.onopen = function () {
        if (socket !== connection || stopped) { return; }
        instance.currentRequestId = active;
        connection.send(JSON.stringify({ type: 'hello', protocolVersion: 1, token: config.token, sessionId: session, instance: instance }));
      };
      connection.onmessage = function (event) {
        if (socket !== connection || stopped) { return; }
        lastSeen = Date.now();
        try {
          var packet = JSON.parse(event.data);
          if (packet.type === 'welcome') {
            if (packet.protocolVersion !== 1) { throw new Error('Unsupported protocol'); }
            welcomed = true;
            connecting = false;
            backoff = 500;
            maxResultBytes = packet.maxResultBytes || maxResultBytes;
            status(active ? 'running' : 'connected', 'WebSocket connected. Commands are received automatically.');
            flushResults();
          } else if (packet.type === 'command' && welcomed && packet.sessionId === session) {
            execute(packet).catch(function () { disconnect(); });
          } else if (packet.type === 'resultAck' && packet.sessionId === session) {
            delete pending[packet.requestId];
            status(active ? 'running' : 'connected', 'Result saved by the application.');
          }
        } catch (_e) { disconnect(); }
      };
      connection.onclose = connection.onerror = function () {
        if (socket === connection) { disconnect(); }
      };
    } catch (error) {
      disconnect();
      status('waiting', String(error.message || error));
    }
  }
  function tick() {
    if (stopped) { return; }
    if (socket && Date.now() - lastSeen > (welcomed ? 15000 : 5000)) { disconnect(); }
    if (!socket && Date.now() >= retryAt) { connect(); }
    if (welcomed && Date.now() - lastHeartbeat >= 3000) {
      lastHeartbeat = Date.now();
      try {
        send({ type: 'heartbeat', status: active ? 'running' : (options.enabled() ? 'idle' : 'paused'), currentRequestId: active });
        // Also retry an unacknowledged result on a healthy connection.
        flushResults();
      } catch (_e) { disconnect(); }
    }
  }
  return {
    start: function () {
      if (!stopped) { return; }
      stopped = false;
      timer = options.setInterval(tick, 1000);
      tick();
    },
    stop: function () {
      stopped = true;
      if (timer) { options.clearInterval(timer); timer = null; }
      disconnect();
    }
  };
};
