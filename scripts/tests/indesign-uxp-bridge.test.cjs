const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const vm = require("node:vm");
const { test } = require("node:test");

const source = fs.readFileSync(process.env.BRIDGE_SOURCE || path.resolve(__dirname, "../../src/indesign/uxp/mcp-bridge-indesign.idjs"), "utf8");

async function waitFor(predicate) {
  const deadline = Date.now() + 2000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error("Bridge operation did not finish");
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

async function startBridge(t, hooks = {}) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "indesign-uxp-contract-"));
  const root = path.join(home, "Documents", "id-mcp-bridge");
  const timers = new Map();
  const listeners = new Map();
  let nextTimer = 0;
  const logs = [];
  const callbackOperation = (operation, callback) => {
    Promise.resolve().then(operation).then(() => callback(null), callback);
  };
  const uxpFs = {
    lstatSync: fs.lstatSync,
    readdirSync: fs.readdirSync,
    readFileSync: fs.readFileSync,
    writeFileSync: fs.writeFileSync,
    mkdir(file, options, callback) {
      callbackOperation(() => fs.promises.mkdir(file, options), callback);
    },
    rename(from, to, callback) {
      callbackOperation(async () => {
        if (hooks.rename) await hooks.rename(from, to);
        await fs.promises.rename(from, to);
        if (hooks.afterRename) await hooks.afterRename(from, to);
      }, callback);
    },
    unlink(file, callback) {
      callbackOperation(async () => {
        if (hooks.unlink) await hooks.unlink(file);
        await fs.promises.unlink(file);
      }, callback);
    },
  };
  const app = {
    documents: [], version: "21.4.1",
    addEventListener(name, callback) { listeners.set(name, callback); },
    removeEventListener(name) { listeners.delete(name); },
  };
  const context = {
    console: { error: (...args) => logs.push(args.map(String).join(" ")) },
    setTimeout, clearTimeout,
    setInterval(callback, interval) {
      const id = ++nextTimer;
      timers.set(id, { callback, interval });
      return id;
    },
    clearInterval(id) { timers.delete(id); },
    require(name) {
      if (name === "fs") return uxpFs;
      if (name === "os") return { homedir: () => home, platform: os.platform };
      if (name === "indesign") return { app };
      throw new Error("Unexpected module: " + name);
    },
  };
  const running = vm.runInNewContext("(async () => {\n" + source + "\n})()", context);
  running.catch((error) => logs.push(String(error)));
  const quit = () => listeners.get("beforeQuit")?.();
  t.after(async () => {
    quit();
    await Promise.race([running, new Promise((resolve) => setTimeout(resolve, 100))]);
    fs.rmSync(home, { recursive: true, force: true });
  });
  await waitFor(() => listeners.has("beforeQuit") || logs.length > 0);
  assert.deepEqual(logs, []);
  const instanceRoot = path.join(root, "instances", fs.readdirSync(path.join(root, "instances"))[0]);
  const read = (file) => JSON.parse(fs.readFileSync(file, "utf8"));
  return {
    instanceRoot, read, running, timers, quit,
    heartbeat: path.join(instanceRoot, "heartbeat.json"),
    command: path.join(instanceRoot, "id_command.json"),
    result: path.join(instanceRoot, "id_mcp_result.json"),
    tickHeartbeat() { [...timers.values()].find((timer) => timer.interval === 2000).callback(); },
    async ping(requestId = "cleanup-contract") {
      fs.writeFileSync(this.command, JSON.stringify({ command: "ping", args: {}, requestId, status: "pending" }));
      [...timers.values()].find((timer) => timer.interval === 250).callback();
      await waitFor(() => (fs.existsSync(this.result) && this.read(this.result)._requestId === requestId) || logs.length > 0);
      return logs;
    },
  };
}

test("supported UXP APIs publish heartbeat and complete a ping request", async (t) => {
  const bridge = await startBridge(t);
  assert.equal(bridge.read(bridge.heartbeat).hostId, "indesign");
  assert.deepEqual(await bridge.ping(), []);
  assert.equal(bridge.read(bridge.result)._requestId, "cleanup-contract");
  assert.equal(bridge.read(bridge.result).status, "success");
});

test("stale temporary files are deleted without deleting recent or unrelated files", async (t) => {
  let deleted = false;
  const bridge = await startBridge(t, { unlink(file) { if (file.endsWith(".heartbeat.json.tmp-old")) deleted = true; } });
  const old = path.join(bridge.instanceRoot, ".heartbeat.json.tmp-old");
  const recent = path.join(bridge.instanceRoot, ".heartbeat.json.tmp-recent");
  const other = path.join(bridge.instanceRoot, "keep.txt");
  for (const file of [old, recent, other]) fs.writeFileSync(file, "keep");
  const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
  fs.utimesSync(old, past, past);
  bridge.tickHeartbeat();
  await waitFor(() => deleted && !fs.existsSync(old));
  assert.equal(fs.readFileSync(recent, "utf8"), "keep");
  assert.equal(fs.readFileSync(other, "utf8"), "keep");
});

test("failed result publication retains the previous result and removes its temporary file", async (t) => {
  const bridge = await startBridge(t, { rename(_from, to) {
    if (path.basename(to) === "id_mcp_result.json") throw new Error("publication denied");
  } });
  fs.writeFileSync(bridge.result, JSON.stringify({ previous: true }));
  assert.match((await bridge.ping()).join("\n"), /publication denied/);
  assert.deepEqual(bridge.read(bridge.result), { previous: true });
  assert.equal(bridge.read(bridge.command).status, "running");
  assert.deepEqual(fs.readdirSync(bridge.instanceRoot).filter((name) => name.startsWith(".id_mcp_result.json.tmp-")), []);
});

test("shutdown stops polling and removes the published heartbeat", async (t) => {
  const bridge = await startBridge(t);
  bridge.quit();
  assert.equal(bridge.timers.size, 0);
  await bridge.running;
  assert.equal(fs.existsSync(bridge.heartbeat), false);
});

test("shutdown removes a heartbeat published by an already pending rename", async (t) => {
  let release;
  let blocked = false;
  let hold = false;
  const gate = new Promise((resolve) => { release = resolve; });
  t.after(() => release());
  const bridge = await startBridge(t, { async rename(_from, to) {
    if (hold && path.basename(to) === "heartbeat.json") { blocked = true; await gate; }
  } });
  hold = true;
  bridge.tickHeartbeat();
  await waitFor(() => blocked);
  bridge.quit();
  release();
  await bridge.running;
  assert.equal(fs.existsSync(bridge.heartbeat), false);
});

test("the startup script remains pending until asynchronous shutdown deletion completes", async (t) => {
  let release;
  let blocked = false;
  const gate = new Promise((resolve) => { release = resolve; });
  t.after(() => release());
  const bridge = await startBridge(t, { async unlink(file) {
    if (path.basename(file) === "heartbeat.json") { blocked = true; await gate; }
  } });
  let finished = false;
  bridge.running.then(() => { finished = true; });
  bridge.quit();
  await waitFor(() => blocked);
  assert.equal(finished, false);
  release();
  await bridge.running;
  assert.equal(fs.existsSync(bridge.heartbeat), false);
});

test("the next request survives publication of its predecessor result", async (t) => {
  let bridge;
  let submitted = false;
  bridge = await startBridge(t, { afterRename(_from, to) {
    if (bridge && to === bridge.result && !submitted) {
      submitted = true;
      fs.writeFileSync(bridge.command, JSON.stringify({ command: "ping", args: {}, requestId: "successor", status: "pending" }));
    }
  } });
  assert.deepEqual(await bridge.ping(), []);
  await waitFor(() => bridge.read(bridge.heartbeat).status === "completed");
  assert.equal(submitted, true);
  assert.equal(bridge.read(bridge.command).requestId, "successor");
  assert.equal(bridge.read(bridge.command).status, "pending");
  assert.deepEqual(await bridge.ping("successor"), []);
  assert.equal(bridge.read(bridge.result)._requestId, "successor");
});
