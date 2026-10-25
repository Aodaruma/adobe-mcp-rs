const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const vm = require("node:vm");
const { test } = require("node:test");

const source = fs.readFileSync(path.resolve(__dirname, "../../src/premiere/uxp/mcp-bridge-premiere/js/main.js"), "utf8");

async function waitFor(predicate, failure) {
  const deadline = Date.now() + 2000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error(failure());
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

async function startBridge(t, hooks = {}) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "premiere-uxp-contract-"));
  const root = path.join(home, "Documents", "pr-mcp-bridge");
  const instanceRoot = path.join(root, "instances", "pr-uxp-contract");
  const elements = new Map();
  const timers = new Map();
  let nextTimer = 0;
  let entrypoints;
  const element = (id) => {
    if (!elements.has(id)) elements.set(id, {textContent: "", checked: true, addEventListener() {}});
    return elements.get(id);
  };
  // Adobe公式fs APIの公開面だけを渡し、Node.js専用の同期APIを混入させない。
  const uxpFs = {
    lstatSync: fs.lstatSync,
    readdirSync: fs.readdirSync,
    readFileSync: fs.readFileSync,
    writeFileSync: fs.writeFileSync,
    mkdir: (...args) => fs.promises.mkdir(...args),
    async writeFile(file, text, options) {
      if (hooks.write) await hooks.write(file, text);
      return fs.promises.writeFile(file, text, options);
    },
    async rename(from, to) {
      if (hooks.rename) await hooks.rename(from, to);
      await fs.promises.rename(from, to);
      if (hooks.afterRename) await hooks.afterRename(from, to);
    },
    unlink: (...args) => fs.promises.unlink(...args),
  };
  const context = {
    console,
    setTimeout,
    clearTimeout,
    document: {readyState: "complete", getElementById: element},
    window: {
      localStorage: {getItem: () => "pr-uxp-contract", setItem() {}},
      addEventListener() {},
      setTimeout,
      clearTimeout,
      setInterval(callback, interval) {const id = ++nextTimer; timers.set(id, {callback, interval}); return id;},
      clearInterval(id) {timers.delete(id);},
    },
    require(name) {
      if (name === "fs") return uxpFs;
      if (name === "os") return {homedir: () => home, platform: os.platform};
      if (name === "premierepro") return {Project: {getActiveProject: async () => null}};
      if (name === "uxp") return {host: {name: "Premiere Pro", version: "26.5.2"}, entrypoints: {setup(value) {entrypoints = value;}}};
      throw new Error("Unexpected module: " + name);
    },
  };
  vm.runInNewContext(source, context, {filename: "premiere-main.js"});
  t.after(async () => {
    entrypoints?.panels.mcpBridgePanel.destroy();
    await new Promise((resolve) => setTimeout(resolve, 30));
    fs.rmSync(home, {recursive: true, force: true});
  });
  await waitFor(() => [...timers.values()].some((timer) => timer.interval === 1000), () => element("log").textContent);
  assert.notEqual(element("status").textContent, "error", element("log").textContent);
  const read = (file) => JSON.parse(fs.readFileSync(file, "utf8"));
  return {
    root, instanceRoot, element, read,
    heartbeat: path.join(instanceRoot, "heartbeat.json"),
    command: path.join(instanceRoot, "pr_command.json"),
    result: path.join(instanceRoot, "pr_mcp_result.json"),
    async poll() {await [...timers.values()].find((timer) => timer.interval === 1000).callback();},
    enqueue(command = "ping", args = {}, requestId = "contract-request") {
      fs.writeFileSync(path.join(instanceRoot, "pr_command.json"), JSON.stringify({command, args, requestId, status: "pending"}));
    },
  };
}

test("UXP対応APIだけで起動し、探索に必要なハートビートを保存する", async (t) => {
  const bridge = await startBridge(t);
  const heartbeat = bridge.read(bridge.heartbeat);
  assert.equal(heartbeat.protocolVersion, 1);
  assert.equal(heartbeat.hostId, "premiere");
  assert.equal(heartbeat.instanceId, "pr-uxp-contract");
  assert.equal(heartbeat.commandFile, bridge.command);
  assert.equal(heartbeat.resultFile, bridge.result);
  assert.ok(Date.now() - Date.parse(heartbeat.lastHeartbeatAt) < 2000);
  assert.equal(bridge.element("status").textContent, "waiting");
});

test("pingの応答を個別・互換保存先へ公開する", async (t) => {
  const bridge = await startBridge(t);
  bridge.enqueue();
  await bridge.poll();
  const result = bridge.read(bridge.result);
  assert.equal(result.status, "success");
  assert.equal(result._requestId, "contract-request");
  assert.equal(result._commandExecuted, "ping");
  assert.deepEqual(bridge.read(path.join(bridge.root, "pr_mcp_result.json")), result);
  assert.equal(bridge.read(bridge.command).status, "completed");
  assert.equal(bridge.element("status").textContent, "completed");
});

test("互換結果の保存待ちでは個別応答を公開しない", async (t) => {
  let release;
  let blocked = false;
  const gate = new Promise((resolve) => {release = resolve;});
  const bridge = await startBridge(t, {async rename(_from, to) {
    if (path.basename(to) === "pr_mcp_result.json") {blocked = true; await gate;}
  }});
  t.after(() => release());
  bridge.enqueue();
  const pending = bridge.poll();
  await waitFor(() => blocked, () => "結果公開へ到達しませんでした");
  assert.equal(bridge.read(bridge.command).status, "running");
  assert.equal(bridge.element("status").textContent, "running");
  assert.equal(fs.existsSync(bridge.result), false);
  release();
  await pending;
  assert.equal(bridge.read(bridge.command).status, "completed");
});

for (const scope of ["compatibility", "instance"]) {
  test(`failed ${scope} publication preserves the destination and removes temporary files`, async (t) => {
    let failedResult;
    const bridge = await startBridge(t, { async rename(_from, to) {
      if (to === failedResult) throw new Error("result rename denied");
    } });
    const resultName = path.basename(bridge.result);
    failedResult = scope === "compatibility" ? path.join(bridge.root, resultName) : bridge.result;
    for (const file of new Set([failedResult, bridge.result])) fs.writeFileSync(file, JSON.stringify({ previous: true }));
    bridge.enqueue();
    await bridge.poll();
    assert.deepEqual(bridge.read(failedResult), { previous: true });
    assert.deepEqual(bridge.read(bridge.result), { previous: true });
    assert.equal(bridge.element("status").textContent, "error");
    assert.match(bridge.element("log").textContent, /result rename denied/);
    for (const parent of [bridge.root, bridge.instanceRoot]) {
      assert.deepEqual(fs.readdirSync(parent).filter((name) => name.startsWith("." + resultName + ".tmp-")), []);
    }
  });

  test(`failed ${scope} temporary write preserves the destination and cleans partial data`, async (t) => {
    let failedResult;
    const bridge = await startBridge(t, { async write(file) {
      if (failedResult && path.dirname(file) === path.dirname(failedResult) && path.basename(file).startsWith("." + path.basename(failedResult) + ".tmp-")) {
        await fs.promises.writeFile(file, "partial");
        throw new Error("result write denied");
      }
    } });
    const resultName = path.basename(bridge.result);
    failedResult = scope === "compatibility" ? path.join(bridge.root, resultName) : bridge.result;
    for (const file of new Set([failedResult, bridge.result])) fs.writeFileSync(file, JSON.stringify({ previous: true }));
    bridge.enqueue();
    await bridge.poll();
    assert.deepEqual(bridge.read(failedResult), { previous: true });
    assert.deepEqual(bridge.read(bridge.result), { previous: true });
    assert.equal(bridge.element("status").textContent, "error");
    assert.match(bridge.element("log").textContent, /result write denied/);
    for (const parent of [bridge.root, bridge.instanceRoot]) {
      assert.deepEqual(fs.readdirSync(parent).filter((name) => name.startsWith("." + resultName + ".tmp-")), []);
    }
  });
}

test("一時的な置換失敗から回復し、後続コマンドも保存できる", async (t) => {
  let attempts = 0;
  const bridge = await startBridge(t, {async rename(_from, to) {
    if (path.basename(to) === "pr_mcp_result.json" && attempts++ < 2) throw new Error("temporary busy");
  }});
  bridge.enqueue();
  await bridge.poll();
  assert.equal(bridge.read(bridge.command).status, "completed");
  bridge.enqueue("ping", {}, "next-request");
  await bridge.poll();
  assert.equal(bridge.read(bridge.result)._requestId, "next-request");
});

test("古い対象一時ファイルだけを清掃し、直近や無関係のファイルを保つ", async (t) => {
  const bridge = await startBridge(t);
  const old = path.join(bridge.instanceRoot, ".heartbeat.json.tmp-old");
  const recent = path.join(bridge.instanceRoot, ".heartbeat.json.tmp-recent");
  const other = path.join(bridge.instanceRoot, "keep.txt");
  for (const file of [old, recent, other]) fs.writeFileSync(file, "keep");
  const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
  fs.utimesSync(old, past, past);
  await bridge.poll();
  assert.equal(fs.existsSync(old), false);
  assert.equal(fs.readFileSync(recent, "utf8"), "keep");
  assert.equal(fs.readFileSync(other, "utf8"), "keep");
});

test("raw UXPからawaitしたファイル保存と日本語の結果を返せる", async (t) => {
  const bridge = await startBridge(t);
  bridge.enqueue("executeJsx", {mode: "unsafe", description: "bridge file contract", code: 'await bridge.writeJsonFile(bridge.joinPath(bridge.getBridgePaths().instanceRoot, "probe.json"), {text:"日本語"}); return {readBack: bridge.readJsonFile(bridge.joinPath(bridge.getBridgePaths().instanceRoot, "probe.json"))};'});
  await bridge.poll();
  assert.equal(bridge.read(bridge.result).result.readBack.text, "日本語");
  assert.equal(bridge.read(bridge.command).status, "completed");
});

test("overlapping writes to one destination preserve submission order", async (t) => {
  let release;
  let firstBlocked = false;
  let secondStarted = false;
  const gate = new Promise((resolve) => { release = resolve; });
  t.after(() => release());
  const bridge = await startBridge(t, { async write(file, text) {
    if (!path.basename(file).startsWith(".probe.json.tmp-")) return;
    if (JSON.parse(text).value === 1) { firstBlocked = true; await gate; }
    else secondStarted = true;
  } });
  bridge.enqueue("executeJsx", {
    mode: "unsafe", description: "ordered file publication",
    code: 'const file = bridge.joinPath(bridge.getBridgePaths().instanceRoot, "probe.json"); await Promise.all([bridge.writeJsonFile(file, {value:1}), bridge.writeJsonFile(file, {value:2})]); return bridge.readJsonFile(file);',
  });
  const pending = bridge.poll();
  await waitFor(() => firstBlocked, () => "First write did not start");
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(secondStarted, false);
  release();
  await pending;
  assert.equal(bridge.read(bridge.result).result.value, 2);
});

test("a request submitted when its predecessor result becomes visible is not overwritten", async (t) => {
  let bridge;
  let submitted = false;
  bridge = await startBridge(t, { afterRename(_from, to) {
    if (bridge && to === bridge.result && !submitted) {
      submitted = true;
      bridge.enqueue("ping", {}, "next-after-publication");
    }
  } });
  bridge.enqueue("ping", {}, "first-publication");
  await bridge.poll();
  assert.equal(submitted, true);
  assert.equal(bridge.read(bridge.command).requestId, "next-after-publication");
  assert.equal(bridge.read(bridge.command).status, "pending");
  await bridge.poll();
  assert.equal(bridge.read(bridge.result)._requestId, "next-after-publication");
  assert.equal(bridge.read(bridge.result).status, "success");
});

test("a legacy global request submitted after its predecessor result is preserved", async (t) => {
  let bridge;
  let commandFile;
  let resultFile;
  let submitted = false;
  const enqueue = (requestId) => fs.writeFileSync(commandFile, JSON.stringify({ command: "ping", args: {}, requestId, status: "pending" }));
  bridge = await startBridge(t, { afterRename(_from, to) {
    if (to === resultFile && !submitted) { submitted = true; enqueue("global-successor"); }
  } });
  commandFile = path.join(bridge.root, "pr_command.json");
  resultFile = path.join(bridge.root, "pr_mcp_result.json");
  enqueue("global-first");
  await bridge.poll();
  assert.equal(submitted, true);
  assert.equal(bridge.read(commandFile).requestId, "global-successor");
  assert.equal(bridge.read(commandFile).status, "pending");
  await bridge.poll();
  assert.equal(bridge.read(resultFile)._requestId, "global-successor");
  assert.equal(bridge.read(resultFile).status, "success");
});
