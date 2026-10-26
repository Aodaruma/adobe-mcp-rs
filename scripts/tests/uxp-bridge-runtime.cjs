const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

// Deliberately expose only the UXP fs API, not Node's larger synchronous API.
function loadBridge(host, prefix, network = false) {
  const files = new Map();
  files.set(`/home/Documents/${prefix}-mcp-bridge/connection.json`, JSON.stringify(network ? {
    transport: 'websocket', hostId: host, protocolVersion: 1, token: 'a'.repeat(64), url: 'ws://localhost:49161/uxp'
  } : { transport: 'file' }));
  const sockets = [];
  let executions = 0;
  let finishExecution;
  class FakeWebSocket {
    constructor(url) { this.url = url; this.readyState = 0; this.sent = []; sockets.push(this); }
    send(text) { assert.equal(this.readyState, 1); this.sent.push(JSON.parse(text)); }
    open() { this.readyState = 1; this.onopen(); }
    receive(packet) { this.onmessage({ data: JSON.stringify(packet) }); }
    close() { this.readyState = 3; if (this.onclose) this.onclose(); }
  }
  const dirs = new Set(['/home', '/home/Documents']);
  const elements = new Map();
  const timers = new Map();
  let hooks;
  let timerId = 0;
  let failRename = false;
  const fakeFs = {
    lstatSync(p) {
      if (!files.has(p) && !dirs.has(p)) throw new Error('ENOENT ' + p);
      return { mtime: new Date() };
    },
    async mkdir(p, callback) {
      assert.equal(callback, undefined, 'UXP mkdir has no Node options argument');
      assert(dirs.has(path.posix.dirname(p)), 'parent directory must exist');
      dirs.add(p);
    },
    readdirSync(p) { return [...files.keys()].filter(f => path.posix.dirname(f) === p).map(f => path.posix.basename(f)); },
    readFileSync(p) { if (!files.has(p)) throw new Error('ENOENT'); return files.get(p); },
    writeFileSync(p, text, options) {
      assert.equal(typeof p, 'string', 'UXP writeFileSync takes a path');
      assert(dirs.has(path.posix.dirname(p)));
      if (options.flag === 'wx' && files.has(p)) throw new Error('EEXIST');
      files.set(p, text);
    },
    async rename(from, to) {
      await Promise.resolve();
      if (failRename) throw new Error('EACCES simulated rename failure');
      assert(files.has(from));
      files.set(to, files.get(from));
      files.delete(from);
    },
    async unlink(p) { files.delete(p); }
  };
  const document = {
    readyState: 'complete',
    getElementById(id) {
      if (!elements.has(id)) elements.set(id, { textContent: '', addEventListener() {} });
      return elements.get(id);
    }
  };
  const window = {
    WebSocket: FakeWebSocket,
    addEventListener() {},
    setInterval(fn, ms) { const id = ++timerId; timers.set(id, { fn, ms }); return id; },
    clearInterval(id) { timers.delete(id); },
    setTimeout, clearTimeout,
    localStorage: { getItem() { return `${prefix}-uxp-test-instance`; } }
  };
  const modules = {
    fs: fakeFs, os: { platform: () => 'darwin', homedir: () => '/home' },
    uxp: { host: { name: host, version: '26.0' }, entrypoints: { setup(value) { hooks = value; } } },
    photoshop: { app: { documents: [], version: '26.0' } },
    premierepro: { app: {}, Project: { getActiveProject: async () => null } }
  };
  modules['./js/websocket.js'] = require('../../src/' + host + '/uxp/mcp-bridge-' + host + '/js/websocket.js');
  modules.probe = { run() { executions++; return new Promise(resolve => { finishExecution = resolve; }); } };
  const source = path.resolve(__dirname, `../../src/${host}/uxp/mcp-bridge-${host}/js/main.js`);
  vm.runInNewContext(fs.readFileSync(source, 'utf8'), { require: name => modules[name], window, document, setTimeout, clearTimeout, console }, { filename: source });
  const root = `/home/Documents/${prefix}-mcp-bridge`;
  const instance = `${root}/instances/${prefix}-uxp-test-instance`;
  return { files, timers, elements, root, instance, hooks, sockets,
    executions: () => executions, finishExecution: () => finishExecution('done'),
    setFailRename: value => { failRename = value; } };
}

async function settle() { for (let i = 0; i < 25; i++) await new Promise(resolve => setImmediate(resolve)); }

for (const [host, prefix] of [['photoshop', 'ps'], ['premiere', 'pr']]) {
  test(`${host}: WebSocket auto receive, reconnect, retained ACK and duplicate suppression`, async () => {
    const bridge = loadBridge(host, prefix, true);
    await settle();
    assert.equal(bridge.sockets.length, 1, bridge.elements.get('log').textContent);
    const ws = bridge.sockets[0];
    ws.open();
    const hello = ws.sent[0];
    assert.equal(hello.type, 'hello');
    ws.receive({ type: 'welcome', protocolVersion: 1 });
    const command = { type: 'command', sessionId: hello.sessionId, requestId: 'req-first', command: 'executeJsx', args: {
      mode: 'unsafe', description: 'Controlled transport test', code: "return await bridge.require('probe').run();"
    }};
    ws.receive(command);
    await settle();
    assert.equal(bridge.executions(), 1);
    ws.close();
    bridge.finishExecution();
    await settle();
    // The initial retry is bounded; exercise the actual retry tick after its deadline.
    await new Promise(resolve => setTimeout(resolve, 850));
    for (const timer of bridge.timers.values()) timer.fn();
    await settle();
    const resumed = bridge.sockets.at(-1);
    assert.notEqual(resumed, ws);
    resumed.open();
    assert.equal(resumed.sent[0].sessionId, hello.sessionId);
    resumed.receive({ type: 'welcome', protocolVersion: 1 });
    assert.equal(resumed.sent.at(-1).type, 'result');
    assert.equal(resumed.sent.at(-1).result.result, 'done');
    resumed.receive(command);
    assert.equal(bridge.executions(), 1);
    resumed.receive({ type: 'resultAck', sessionId: hello.sessionId, requestId: command.requestId });
    resumed.receive(command);
    assert.equal(bridge.executions(), 1, 'ACK does not permit repeated execution');
    bridge.hooks.panels.mcpBridgePanel.hide();
    await settle();
    assert.equal(bridge.sockets.at(-1), resumed, 'hidden panel retains its connection');
    assert.equal(bridge.files.size, 1, 'network transport must not write command/result/heartbeat files');
    bridge.hooks.plugin.destroy();
    assert.equal(bridge.timers.size, 0);
  });

  test(`${host}: auto receive, atomic replacement, and hidden panel lifecycle`, async () => {
    const bridge = loadBridge(host, prefix);
    await settle();
    const heartbeat = `${bridge.instance}/heartbeat.json`;
    assert(bridge.files.has(heartbeat), bridge.elements.get('log').textContent);
    const poll = [...bridge.timers.values()].find(t => t.ms === 1000);
    assert(poll, 'receives commands without a button');
    for (const id of ['first', 'second']) {
      bridge.files.set(`${bridge.instance}/${prefix}_command.json`, JSON.stringify({ command: 'ping', status: 'pending', requestId: id }));
      await poll.fn();
      const result = JSON.parse(bridge.files.get(`${bridge.instance}/${prefix}_mcp_result.json`));
      assert.equal(result.status, 'success');
      assert.equal(result._requestId, id);
    }
    bridge.hooks.panels.mcpBridgePanel.hide();
    await settle();
    assert([...bridge.timers.values()].some(t => t.ms === 1000), 'hidden/docked panel must keep receiving');
    const previous = bridge.files.get(heartbeat);
    bridge.setFailRename(true);
    await poll.fn();
    assert.equal(bridge.files.get(heartbeat), previous, 'failed replacement must preserve the previous file');
    assert(![...bridge.files.keys()].some(p => p.includes('.tmp-')), 'remove failed write residue');
    assert.match(bridge.elements.get('log').textContent, /EACCES/);
    bridge.hooks.plugin.destroy();
    assert.equal(bridge.timers.size, 0, 'plugin teardown owns timer cleanup');
  });
  test(`${host}: plugin teardown during startup does not resurrect polling`, async () => {
    const bridge = loadBridge(host, prefix);
    bridge.hooks.plugin.destroy();
    await settle();
    assert.equal(bridge.timers.size, 0);
  });
}

test('packaged UXP transports are identical and present in Windows installer', () => {
  const ps = fs.readFileSync(path.resolve(__dirname, '../../src/photoshop/uxp/mcp-bridge-photoshop/js/websocket.js'), 'utf8');
  const pr = fs.readFileSync(path.resolve(__dirname, '../../src/premiere/uxp/mcp-bridge-premiere/js/websocket.js'), 'utf8');
  assert.equal(ps, pr);
  const installer = fs.readFileSync(path.resolve(__dirname, '../package-windows.ps1'), 'utf8');
  assert.match(installer, /PhotoshopUxpWebSocketFile/);
  assert.match(installer, /PremiereUxpWebSocketFile/);
});
