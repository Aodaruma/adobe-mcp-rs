import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { setImmediate as tick } from "node:timers/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../../src/photoshop/uxp/mcp-bridge-photoshop/js/main.js", import.meta.url), "utf8");

// Only expose documented UXP fs methods; Node-only synchronous methods must not mask regressions.
function boot({ command = false, renameFailures = 0, writeFailure = false, resultFailure = false } = {}) {
  const root = "/home/test/Documents/ps-mcp-bridge";
  const instance = `${root}/instances/test-instance`;
  const files = new Map();
  const dirs = new Set();
  const elements = new Map();
  const published = [];
  const intervals = [];
  const delays = [];
  const stale = `${instance}/.heartbeat.json.tmp-old`;
  const recent = `${instance}/.heartbeat.json.tmp-recent`;
  files.set(stale, "stale");
  files.set(recent, "recent");
  if (command) files.set(`${instance}/ps_command.json`, JSON.stringify({
    command: "ping", requestId: "request-43", status: "pending", args: {}
  }));
  const fs = {
    lstatSync(file) {
      if (!files.has(file) && !dirs.has(file)) throw new Error("ENOENT");
      return { mtimeMs: file === stale ? Date.now() - 7200000 : Date.now() };
    },
    readFileSync(file) { if (!files.has(file)) throw new Error("ENOENT"); return files.get(file); },
    async mkdir(dir) { dirs.add(dir); },
    async readdir(dir) { return [...files.keys()].filter(f => path.posix.dirname(f) === dir).map(f => path.posix.basename(f)); },
    async writeFile(file, value, options) {
      await tick();
      if (writeFailure || (resultFailure && file.includes(".ps_mcp_result.json.tmp-"))) throw new Error("write denied");
      if (options.flag === "wx" && files.has(file)) throw new Error("EEXIST");
      files.set(file, value);
    },
    async rename(from, to) {
      await tick();
      if (renameFailures-- > 0) throw new Error("rename denied");
      if (!files.has(from)) throw new Error("ENOENT");
      files.set(to, files.get(from));
      files.delete(from);
      published.push({ path: to, value: JSON.parse(files.get(to)) });
    },
    async unlink(file) { await tick(); files.delete(file); }
  };
  const document = {
    readyState: "complete",
    getElementById(id) {
      if (!elements.has(id)) elements.set(id, { textContent: "", addEventListener() {} });
      return elements.get(id);
    }
  };
  const window = {
    localStorage: { getItem: () => "test-instance" },
    addEventListener() {},
    setInterval(callback) { intervals.push(callback); return intervals.length; },
    clearInterval() {},
    setTimeout(callback, delay) { delays.push(delay); return setTimeout(callback, 0); }
  };
  vm.runInNewContext(source, {
    document, window,
    require(name) {
      return { fs, os: { platform: () => "darwin", homedir: () => "/home/test" },
        photoshop: { app: { version: "27.11.0", documents: [] } },
        uxp: { host: { name: "Photoshop", version: "27.11.0" } } }[name];
    }
  });
  return { root, instance, files, elements, published, intervals, delays, stale, recent };
}

async function settled(harness) {
  for (let i = 0; i < 200; i++) {
    if (harness.intervals.length) return;
    await tick();
    await new Promise(resolve => setTimeout(resolve, 1));
  }
  assert.fail("Bridge startup did not settle");
}

test("startup publishes valid heartbeat through the UXP fs API and removes only stale temporary files", async () => {
  const h = boot();
  await settled(h);
  const heartbeat = JSON.parse(h.files.get(`${h.instance}/heartbeat.json`));
  assert.equal(heartbeat.hostId, "photoshop");
  assert.equal(heartbeat.instanceId, "test-instance");
  assert.equal(heartbeat.commandFile, `${h.instance}/ps_command.json`);
  assert.ok(!h.files.has(h.stale));
  assert.ok(h.files.has(h.recent));
});

test("command completion follows result publication, preserving request correlation and compatibility mirror", async () => {
  const h = boot({ command: true });
  await settled(h);
  const resultPath = `${h.instance}/ps_mcp_result.json`;
  const result = JSON.parse(h.files.get(resultPath));
  assert.equal(result.status, "success");
  assert.equal(result._requestId, "request-43");
  assert.deepEqual(JSON.parse(h.files.get(`${h.root}/ps_mcp_result.json`)), result);
  const resultIndex = h.published.findIndex(p => p.path === resultPath);
  const runningIndex = h.published.findIndex(p => p.path.endsWith("/ps_command.json") && p.value.status === "running");
  const completedIndex = h.published.findIndex(p => p.path.endsWith("/ps_command.json") && p.value.status === "completed");
  assert.ok(runningIndex >= 0 && resultIndex > runningIndex && completedIndex > resultIndex);
});

test("temporary rename failures yield to timers and recover without leaving new temporary files", async () => {
  const h = boot({ renameFailures: 2 });
  await settled(h);
  assert.ok(h.files.has(`${h.instance}/heartbeat.json`));
  assert.equal(h.delays.length, 2);
  assert.deepEqual([...h.files.keys()].filter(p => p.includes(".tmp-")), [h.recent]);
});

for (const options of [{ renameFailures: 10 }, { writeFailure: true }]) {
  test(`publication failure surfaces in the panel and cleans temporary output: ${JSON.stringify(options)}`, async () => {
    const h = boot(options);
    await settled(h);
    assert.ok(!h.files.has(`${h.instance}/heartbeat.json`));
    assert.deepEqual([...h.files.keys()].filter(p => p.includes(".tmp-")), [h.recent]);
    assert.ok([...h.elements.values()].some(e => /denied/.test(e.textContent)));
  });
}

test("failed result publication never marks the command completed", async () => {
  const h = boot({ command: true, resultFailure: true });
  await settled(h);
  assert.equal(JSON.parse(h.files.get(`${h.instance}/ps_command.json`)).status, "running");
  assert.ok(!h.files.has(`${h.instance}/ps_mcp_result.json`));
  assert.ok(!h.published.some(p => p.path.endsWith("/ps_command.json") && p.value.status === "completed"));
});
