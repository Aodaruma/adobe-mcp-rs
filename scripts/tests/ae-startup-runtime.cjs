// Execute the complete Startup/runtime/Shutdown sources in an isolated VM.
// All host files and tasks are in memory; this never connects to After Effects.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

const scripts = path.resolve(__dirname, '../../src/scripts');
const startupPath = '/mock/Scripts/Startup/mcp-bridge-startup.jsx';
const runtimePath = '/mock/Scripts/ScriptUI Panels/mcp-bridge-auto.jsx';
const shutdownPath = '/mock/Scripts/Shutdown/mcp-bridge-shutdown.jsx';
const bridgeRoot = '/mock/Documents/ae-mcp-bridge';
const bootstrapPath = bridgeRoot + '/ae_mcp_bootstrap.json';
const incidentPath = bridgeRoot + '/ae_mcp_scheduler_diagnostic.json';
const sources = new Map([
    [startupPath, fs.readFileSync(path.join(scripts, 'mcp-bridge-startup.jsx'), 'utf8')],
    [runtimePath, fs.readFileSync(path.join(scripts, 'mcp-bridge-auto.jsx'), 'utf8')],
    [shutdownPath, fs.readFileSync(path.join(scripts, 'mcp-bridge-shutdown.jsx'), 'utf8')],
]);

function queueAuditCommand(f, id, code, args = {}) {
    const command = {status:'pending', command:'executeJsx', requestId:id,
        args:{mode:'unsafe', description:'Reliability regression', undoGroup:false, code, args}};
    f.files.set(f.heartbeat().commandFile, JSON.stringify(command));
    return command;
}

test('raw JSX has public arguments and host globals but no private runtime bindings', () => {
    const f = fixture();
    queueAuditCommand(f, 'lexical-boundary', 'heartbeatTickActive = true; ({value:args.value, logType:typeof mcp.log, privateState:typeof bridgeRuntimeRunning});', {value:42});
    f.advance(6000);
    assert.deepEqual(f.read(f.heartbeat().resultFile).result, {value:42,logType:'function',privateState:'undefined'});
    assert.equal(f.context.heartbeatTickActive, true, 'unsafe mode still has global access');
    assert.equal(f.state().callbackActive, false);
    assert.equal(f.state().healthy, true);
});

test('large structured args remain compact through running and terminal mailbox updates', () => {
    for (const fail of [false, true]) {
        const f = fixture();
        const args = {values:Array.from({length:10000}, () => ({a:[0,1,2]}))};
        const command = queueAuditCommand(f, fail ? 'compact-error' : 'compact-success',
            fail ? 'throw new Error("expected");' : 'args.values.length;', args);
        const writes = [];
        const write = f.context.File.prototype.write;
        f.context.File.prototype.write = function(text) {
            if (this.name.startsWith('.ae_command.json.tmp-')) writes.push(text);
            return write.call(this, text);
        };
        f.advance(6000);
        // User-code errors are returned normally by executeJsx; transport completes.
        const terminal = 'completed';
        assert(writes.some(text => JSON.parse(text).status === 'running'));
        assert(writes.some(text => JSON.parse(text).status === terminal));
        for (const text of writes) {
            assert.equal(text, JSON.stringify(JSON.parse(text)));
            assert(Buffer.byteLength(text) <= Buffer.byteLength(JSON.stringify(command)) + 4096);
        }
        assert.equal(f.read(f.heartbeat().commandFile).status, terminal);
        assert.equal(f.read(f.heartbeat().resultFile).status, fail ? 'error' : 'success');
    }
});

test('failure to persist a claim prevents execution until storage is writable', () => {
    const f = fixture();
    const write = f.context.File.prototype.write;
    let failing = true;
    f.context.File.prototype.write = function(text) {
        if (failing && this.fsName.includes('/request-receipts/')) throw new Error('claim write failed');
        return write.call(this, text);
    };
    queueAuditCommand(f, 'claim-write', '$.global.__runs = ($.global.__runs || 0) + 1;');
    f.advance(6000);
    assert.equal(f.context.__runs, undefined);
    failing = false;
    f.advance(6000);
    assert.equal(f.context.__runs, 1);
    assert.equal(f.read(f.heartbeat().resultFile).status, 'success');
});

test('failed running status cannot execute a request whose durable claim already exists', () => {
    const f = fixture();
    const write = f.context.File.prototype.write;
    f.context.File.prototype.write = function(text) {
        if (this.name.startsWith('.ae_command.json.tmp-')) throw new Error('status write failed');
        return write.call(this, text);
    };
    queueAuditCommand(f, 'running-write', '$.global.__runs = ($.global.__runs || 0) + 1;');
    f.advance(6000);
    assert.equal(f.context.__runs, undefined);
    assert.equal(f.read(f.heartbeat().resultFile).outcomeUnknown, true);
    assert.equal(f.state().callbackActive, false);
});

test('a failed completed status preserves success and never reexecutes a duplicate request', () => {
    const f = fixture();
    const write = f.context.File.prototype.write;
    f.context.File.prototype.write = function(text) {
        if (this.name.startsWith('.ae_command.json.tmp-') && JSON.parse(text).status === 'completed') throw new Error('completion write failed');
        return write.call(this, text);
    };
    const command = queueAuditCommand(f, 'completed-write', '$.global.__runs = ($.global.__runs || 0) + 1;');
    f.advance(6000);
    f.files.set(f.heartbeat().commandFile, JSON.stringify(command));
    f.advance(6000);
    assert.equal(f.context.__runs, 1);
    assert.equal(f.read(f.heartbeat().resultFile).status, 'success');
});

test('receipt survives runtime replacement and prevents replay of an older duplicate', () => {
    const f = fixture();
    const original = queueAuditCommand(f, 'first-receipt', '$.global.__runs = ($.global.__runs || 0) + 1;');
    f.advance(4000);
    queueAuditCommand(f, 'second-receipt', '"second";');
    f.advance(4000);
    f.context.$.global.__adobeMcpBridgeBootstrapConfig = {headless:true, source:'test-reload'};
    f.evaluate(runtimePath);
    f.files.set(f.heartbeat().commandFile, JSON.stringify(original));
    f.advance(6000);
    assert.equal(f.context.__runs, 1);
    assert.equal(f.read(f.heartbeat().resultFile)._requestId, 'first-receipt');
});

test('result delivery survives reload using the durable finished receipt', () => {
    const f = fixture();
    const write = f.context.File.prototype.write;
    let failing = true;
    f.context.File.prototype.write = function(text) {
        if (failing && this.name.startsWith('.ae_mcp_result.json.tmp-')) throw new Error('result write failed');
        return write.call(this, text);
    };
    queueAuditCommand(f, 'result-reload', '$.global.__runs = ($.global.__runs || 0) + 1;');
    f.advance(4000);
    assert.equal(f.context.__runs, 1);
    f.context.$.global.__adobeMcpBridgeBootstrapConfig = {headless:true, source:'test-reload'};
    f.evaluate(runtimePath);
    failing = false;
    f.advance(6000);
    assert.equal(f.context.__runs, 1);
    assert.equal(f.read(f.heartbeat().resultFile).status, 'success');
});

test('application quit waits for result delivery and does not repeat after a duplicate', () => {
    const f = fixture();
    let quits = 0;
    f.context.app.quit = () => { quits++; };
    f.context.CloseOptions = {DO_NOT_SAVE_CHANGES:0, SAVE_CHANGES:1, PROMPT_TO_SAVE_CHANGES:2};
    const write = f.context.File.prototype.write;
    let failOnce = true;
    f.context.File.prototype.write = function(text) {
        if (failOnce && this.name.startsWith('.ae_mcp_result.json.tmp-')) {
            failOnce = false;
            throw new Error('temporary result delivery failure');
        }
        return write.call(this, text);
    };
    const command = {status:'pending', command:'applicationQuit', requestId:'quit-after-delivery',
        args:{closeProject:false, suppressDialogs:false}};
    f.files.set(f.heartbeat().commandFile, JSON.stringify(command));
    f.advance(2000);
    assert.equal(quits, 0);
    f.advance(4000);
    assert.equal(f.read(f.heartbeat().resultFile).status, 'success');
    assert.equal(quits, 1);
    f.files.set(f.heartbeat().commandFile, JSON.stringify(command));
    f.advance(4000);
    assert.equal(quits, 1);
});

test('corrupt receipt and unsafe request identifiers cannot execute JSX', () => {
    for (const id of ['corrupt', '../escape']) {
        const f = fixture();
        if (id === 'corrupt') {
            const folder = path.posix.dirname(f.heartbeat().commandFile) + '/request-receipts';
            f.files.set(folder + '/corrupt.json', '{broken');
        }
        queueAuditCommand(f, id, '$.global.__runs = 1;');
        f.advance(6000);
        assert.equal(f.context.__runs, undefined);
    }
});

test('result receipt publication and rollback failure cannot erase the immutable claim', () => {
    const f = fixture();
    const id = 'receipt-rollback';
    const command = queueAuditCommand(f, id, '$.global.__runs=($.global.__runs||0)+1;');
    f.advance(2000);
    const folder = path.posix.dirname(f.heartbeat().commandFile) + '/request-receipts';
    const claim = f.files.get(folder+'/'+id+'.json');
    assert.equal(JSON.parse(claim).phase, 'claimed');
    f.files.set(f.heartbeat().commandFile, JSON.stringify(command));
    const rename = f.context.File.prototype.rename;
    f.context.File.prototype.rename = function(name) {
        if(this.parent.fsName === folder && name === id+'.result.json' &&
            (this.name.includes('.tmp-') || this.name.includes('.bak-'))) return false;
        return rename.call(this,name);
    };
    f.advance(2000);
    assert.equal(f.files.get(folder+'/'+id+'.json'), claim);
    assert.equal(f.files.has(folder+'/'+id+'.result.json'), false);
    assert([...f.files.keys()].some(p=>p.includes(id+'.result.json.bak-')));
    f.context.File.prototype.rename = rename;
    f.context.$.global.__adobeMcpBridgeBootstrapConfig={headless:true,source:'rollback-test'};
    f.evaluate(runtimePath);
    f.advance(6000);
    assert.equal(f.context.__runs, 1);
    assert.equal(f.read(f.heartbeat().resultFile).outcomeUnknown, true);
    assert.equal(f.files.get(folder+'/'+id+'.json'), claim);
});

test('receipt publication never scans all prior receipts on the host thread', () => {
    const f = fixture();
    const folder = path.posix.dirname(f.heartbeat().commandFile) + '/request-receipts';
    for(let i=0;i<10000;i++) f.files.set(folder+'/old-'+i+'.json','{}');
    const getFiles=f.context.Folder.prototype.getFiles;
    f.context.Folder.prototype.getFiles=function() {
        assert.notEqual(this.fsName,folder,'receipt writes must not enumerate old requests');
        return getFiles.call(this);
    };
    queueAuditCommand(f,'new-receipt','42;');
    f.advance(4000);
    assert.equal(f.read(f.heartbeat().resultFile).result,42);
});

test('watchdog catches failures in resume-probe scheduling and entry metadata', () => {
    const f=fixture(); f.advance(6000);
    const original=f.state().runtimeId;
    const schedule=f.context.app.scheduleTask;
    f.context.app.scheduleTask=(code,delay,repeat)=>{
        if(code.includes('RecoveryProbe')) throw new Error('probe unavailable');
        return schedule(code,delay,repeat);
    };
    f.elapse(16000);
    assert.doesNotThrow(()=>f.advance(0));
    assert.match(f.state().lastSchedulerError,/probe unavailable/);
    f.context.app.scheduleTask=schedule;
    f.advance(6000);
    const nativeString=vm.runInContext('String',f.context);
    f.context.String=value=>{if(value==='main')throw new Error('watchdog metadata failed');return nativeString(value);};
    assert.doesNotThrow(()=>f.context.__adobeMcpBridgeWatchdogTick(f.state().runtimeId));
    f.context.String=nativeString;
    assert.match(f.state().lastSchedulerError,/watchdog metadata failed/);
    f.advance(6000);
    assert.equal(f.state().runtimeId,original);
    assert.equal(f.state().healthy,true);
});

test('callback entry metadata exceptions release busy flags and rearm the next tick', () => {
    for (const tick of ['__adobeMcpBridgeCommandTick','__adobeMcpBridgeHeartbeatTick']) {
        const f = fixture();
        const nativeString = vm.runInContext('String', f.context);
        let failing = true;
        f.context.String = value => {
            if (failing && value === 'main') { failing=false; throw new Error('engine metadata failed'); }
            return nativeString(value);
        };
        f.context.$.global[tick](f.state().runtimeId, vm.runInContext('(function(args,mcp,code){return eval(code);})', f.context));
        f.context.String = nativeString;
        assert.equal(f.state().callbackActive, false);
        assert.match(f.state().lastSchedulerError, /engine metadata failed/);
        f.advance(6000);
        assert.equal(f.state().healthy, true);
        assert(f.state().commandPollSequence > 0);
    }
});

function fixture(options = {}) {
    const files = new Map(sources);
    const folders = new Set(['/mock/Documents']);
    const tasks = new Map();
    let now = Date.UTC(2026, 9, 22, 10, 54, 39);
    let nextTaskId = 0;
    let modal = false;
    const rejected = [];
    class Clock extends Date {
        constructor(...args) { super(...(args.length ? args : [now])); }
        static now() { return now; }
    }
    function Folder(value) {
        if (!(this instanceof Folder)) return new Folder(value);
        this.fsName = String(value);
    }
    Object.defineProperties(Folder.prototype, {
        exists: { get() { return folders.has(this.fsName); } },
        parent: { get() { return new Folder(path.posix.dirname(this.fsName)); } },
    });
    Folder.prototype.create = function () { folders.add(this.fsName); return true; };
    Folder.prototype.getFiles = function () {
        return [...files.keys()].filter(p => path.posix.dirname(p) === this.fsName).map(p => new File(p));
    };
    Folder.myDocuments = new Folder('/mock/Documents');
    function File(value) {
        if (!(this instanceof File)) return new File(value);
        this.fsName = String(value);
    }
    Object.defineProperties(File.prototype, {
        exists: { get() { return files.has(this.fsName); } },
        name: { get() { return path.posix.basename(this.fsName); } },
        parent: { get() { return new Folder(path.posix.dirname(this.fsName)); } },
        modified: { get() { return new Clock(); } },
    });
    File.prototype.open = function (mode) {
        if (mode === 'r' && !this.exists) return false;
        if (mode === 'w') files.set(this.fsName, '');
        this.opened = true;
        return true;
    };
    File.prototype.write = function (text) {
        assert(this.opened);
        files.set(this.fsName, (files.get(this.fsName) || '') + text);
        return true;
    };
    File.prototype.writeln = function (text) { return this.write(text + '\n'); };
    File.prototype.read = function () { assert(this.opened); return files.get(this.fsName); };
    File.prototype.close = function () { this.opened = false; return true; };
    File.prototype.remove = function () { return files.delete(this.fsName); };
    File.prototype.rename = function (name) {
        const destination = this.parent.fsName + '/' + name;
        // Match Windows ExtendScript: refuse to replace an existing file.
        if (!this.exists || files.has(destination)) return false;
        files.set(destination, files.get(this.fsName));
        files.delete(this.fsName);
        this.fsName = destination;
        return true;
    };
    const context = vm.createContext({
        Date: Clock, File, Folder,
        Window: function () { throw new Error('Startup must not create a panel'); },
        Panel: function () {},
        alert() { throw new Error('Startup must not show a dialog'); },
        app: {
            version: '25.6.6x4',
            project: { file: null, numItems: 0, renderQueue: { rendering: false } },
            scheduleTask(code, delay, repeat) {
                const id = ++nextTaskId;
                tasks.set(id, { code, delay, repeat, due: now + delay });
                return id;
            },
            cancelTask(id) { tasks.delete(id); },
        },
        $: { engineName: options.engineName || 'main', fileName: startupPath },
    });
    vm.runInContext('$.global = this;', context);
    function evaluate(filePath) {
        if (modal) throw new Error('Cannot run a script while a modal dialog is waiting for response.');
        const previous = context.$.fileName;
        context.$.fileName = filePath;
        try {
            // Node does not implement ExtendScript directives or engine routing.
            return vm.runInContext(files.get(filePath).replace(/^#.*$/gm, ''), context, { filename: filePath });
        } finally { context.$.fileName = previous; }
    }
    context.$.evalFile = file => evaluate(file.fsName);
    function advance(milliseconds) {
        const end = now + milliseconds;
        let iterations = 0;
        while (true) {
            const entry = [...tasks].filter(([, t]) => t.due <= end).sort((a, b) => a[1].due - b[1].due)[0];
            if (!entry) break;
            assert(++iterations < 1000, 'unexpected schedule loop');
            const [id, task] = entry;
            now = Math.max(now, task.due);
            if (task.repeat) task.due = now + task.delay;
            else tasks.delete(id);
            // Execute the exact registered string after evalFile has returned.
            if (modal) {
                rejected.push(task.code);
                if (options.dropRejectedTasks) tasks.delete(id);
                continue;
            }
            vm.runInContext(task.code, context);
        }
        now = Math.max(now, end);
    }
    evaluate(startupPath);
    const state = () => context.$.global.__adobeMcpBridgeRuntime.getState();
    const heartbeatPath = () => bridgeRoot + '/instances/' + state().instanceId + '/heartbeat.json';
    return {
        files, tasks, context, advance, evaluate, state, rejected,
        setModal: value => { modal = value; },
        read: name => JSON.parse(files.get(name)),
        heartbeat: () => JSON.parse(files.get(heartbeatPath())),
        elapse: milliseconds => { now += milliseconds; },
    };
}

test('heartbeat storage failure does not restart a progressing scheduler', () => {
    const f = fixture();
    const initial = f.state();
    const write = f.context.File.prototype.write;
    f.context.File.prototype.write = function(text) {
        if (this.name === 'heartbeat.json') throw new Error('heartbeat storage unavailable');
        return write.call(this, text);
    };
    f.advance(60000);
    assert.equal(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    assert.equal(f.state().schedulerHealthy, true);
    assert.equal(f.state().heartbeatFresh, false);
    assert.equal(f.state().healthy, false);
    assert(f.state().commandTickSequence > 20);
    f.context.File.prototype.write = write;
    f.advance(6000);
    assert.equal(f.state().healthy, true);
});

test('every recovery entrance defers after a host pause before the watchdog resumes', () => {
    for (const method of ['ensureHealthy', 'start', 'restart']) {
        const f = fixture();
        f.advance(6000);
        const initial = f.state();
        f.elapse(16000);
        f.context.__adobeMcpBridgeRuntime[method]({reason:'external-test'});
        assert.equal(f.state().runtimeId, initial.runtimeId, method);
        assert(f.state().recoveryProbeTaskId > 0, method);
        f.advance(6000);
        assert.equal(f.state().runtimeId, initial.runtimeId, method);
        assert.equal(f.state().healthy, true, method);
        assert.equal(f.tasks.size, 3, method);
    }
});

test('render guard covers explicit restart, stop, start and runtime replacement', () => {
    const f = fixture();
    f.advance(6000);
    const original = f.state();
    f.context.app.project.renderQueue.rendering = true;
    for (const method of ['restart', 'stop', 'start', 'ensureHealthy']) {
        f.context.__adobeMcpBridgeRuntime[method]();
        assert.equal(f.state().runtimeId, original.runtimeId, method);
        assert.equal(f.state().running, true, method);
    }
    f.context.$.global.__adobeMcpBridgeBootstrapConfig = {headless:true,source:'test-reload'};
    f.evaluate(runtimePath);
    assert.equal(f.state().runtimeId, original.runtimeId);
    assert.equal(f.tasks.size, 3);
});

test('repeated external recovery requests create one probe and recover total scheduler loss', () => {
    const f = fixture();
    f.advance(6000);
    const original = f.state();
    f.tasks.clear();
    f.elapse(16000);
    for(let i=0;i<10;i++) f.context.__adobeMcpBridgeRuntime.ensureHealthy();
    assert.equal(f.tasks.size, 1);
    const probe = f.tasks.get(f.state().recoveryProbeTaskId).code;
    f.advance(9000);
    assert.equal(f.state().schedulerRecoveryCount, 1);
    assert.notEqual(f.state().runtimeId, original.runtimeId);
    assert.equal(f.state().healthy, true);
    vm.runInContext(probe, f.context);
    assert.equal(f.tasks.size, 3, 'late old probe cannot multiply tasks');
});

test('an early resume probe remains scheduled until grace expires and executes a pending request once', () => {
    for (const earlyByMs of [1, 10, 100]) {
        const f = fixture();
        f.advance(6000);
        const initial = f.state();
        f.tasks.clear();
        f.elapse(16000);
        queueAuditCommand(f, 'early-probe-' + earlyByMs, '$.global.__runs = ($.global.__runs || 0) + 1;');
        f.context.__adobeMcpBridgeRuntime.ensureHealthy();
        const firstProbe = f.state().recoveryProbeTaskId;
        const code = f.tasks.get(firstProbe).code;
        f.tasks.delete(firstProbe);
        f.elapse(3000 - earlyByMs);
        vm.runInContext(code, f.context);
        assert.equal(f.state().runtimeId, initial.runtimeId, 'grace still applies');
        assert.equal(f.context.__runs, undefined, 'no dispatch before grace expires');
        assert.equal(f.tasks.size, 1, 'exactly one replacement observer');
        assert.notEqual(f.state().recoveryProbeTaskId, firstProbe);
        assert(f.state().recoveryProbeTaskId > 0);
        f.advance(9000);
        assert.equal(f.state().schedulerRecoveryCount, 1);
        assert.equal(f.state().healthy, true);
        assert.equal(f.context.__runs, 1);
        assert.equal(f.read(f.heartbeat().resultFile).status, 'success');
        assert.equal(f.tasks.size, 3);
    }
});

test('an early resume probe does not rearm while a render or command callback is active', () => {
    for (const mode of ['render', 'callback']) {
        const f = fixture();
        f.advance(6000);
        const initial = f.state();
        const commandCode = f.tasks.get(initial.commandTaskId).code;
        f.tasks.clear();
        f.elapse(16000);
        f.context.__adobeMcpBridgeRuntime.ensureHealthy();
        const probeId = f.state().recoveryProbeTaskId;
        const probeCode = f.tasks.get(probeId).code;
        f.elapse(2999);
        const fireProbe = () => {
            f.tasks.delete(probeId);
            vm.runInContext(probeCode, f.context);
            assert.equal(f.state().recoveryProbeTaskId, 0, mode);
            assert.equal(f.tasks.size, 0, 'busy/render deferral adds no polling loop');
        };
        if (mode === 'render') {
            f.context.app.project.renderQueue.rendering = true;
            fireProbe();
            f.advance(30000);
            assert.equal(f.tasks.size, 0);
        } else {
            f.context.__fireProbe = fireProbe;
            queueAuditCommand(f, 'busy-probe', '__fireProbe();');
            vm.runInContext(commandCode, f.context);
            assert.equal(f.read(f.heartbeat().resultFile).status, 'success');
        }
        assert.equal(f.state().runtimeId, initial.runtimeId);
        assert.equal(f.state().schedulerRecoveryCount, 0);
    }
});

test('command callback without a global-origin evaluator cannot claim a request', () => {
    const f = fixture();
    queueAuditCommand(f, 'no-evaluator', '$.global.__runs=1;');
    f.context.__adobeMcpBridgeCommandTick(f.state().runtimeId);
    assert.equal(f.context.__runs, undefined);
    assert.equal(f.read(f.heartbeat().commandFile).status, 'pending');
    assert.equal(f.state().callbackActive, false);
    f.advance(6000);
    assert.equal(f.context.__runs, 1);
});

test('cold Startup keeps callbacks alive after evalFile returns, with no panel', () => {
    const f = fixture();
    const initial = f.state();
    assert.equal(f.read(bootstrapPath).status, 'running');
    assert.equal(f.read(bootstrapPath).schedulerReady, false);
    assert.equal(f.read(bootstrapPath).runtimeScriptPath, runtimePath);
    assert.equal(f.read(bootstrapPath).runtimeEngineName, 'main');
    assert.equal(f.read(bootstrapPath).startupEngineName, 'main');
    assert.equal(initial.commandPollSequence, 0);
    assert.equal(initial.heartbeatSequence, 1);
    assert.equal(f.context.$.global.__adobeMcpBridgeBootstrapConfig, undefined);
    f.context.app.project.file = { fsName: '/mock/fixture.aep' };
    f.advance(9000);
    assert.equal(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerReady, true);
    assert.equal(f.state().commandTickSequence, 4);
    assert.equal(f.state().heartbeatTickSequence, 3);
    assert.equal(f.state().watchdogTickSequence, 3);
    assert.equal(f.state().commandTickEngineName, 'main');
    assert.equal(f.state().heartbeatTickEngineName, 'main');
    assert.equal(f.state().watchdogTickEngineName, 'main');
    assert.equal(f.heartbeat().projectPath, '/mock/fixture.aep');
    assert.equal(f.heartbeat().schedulerReady, true);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    assert.equal(f.tasks.size, 3);
});

test('lost first one-shots recover via watchdog without panel/bootstrap reevaluation', () => {
    const f = fixture();
    const initial = f.state();
    const oldCommand = f.tasks.get(initial.commandTaskId).code;
    const oldHeartbeat = f.tasks.get(initial.heartbeatTaskId).code;
    f.tasks.delete(initial.commandTaskId);
    f.tasks.delete(initial.heartbeatTaskId);
    f.advance(12000);
    assert.equal(f.state().schedulerRecoveryCount, 0, 'startup grace permits delayed callbacks');
    f.advance(6000);
    assert.equal(f.state().schedulerRecoveryCount, 1);
    assert.equal(f.state().instanceId, initial.instanceId);
    assert.notEqual(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerReady, true);
    const incident = f.read(incidentPath);
    assert.equal(incident.reason, 'scheduler-watchdog:command-poll-stale');
    assert.equal(incident.state.heartbeatSequence, 1);
    assert.equal(incident.state.commandPollSequence, 0);
    assert.equal(incident.state.heartbeatTickSequence, 0);
    assert(incident.state.watchdogTickSequence > 0);
    const polls = f.state().commandPollSequence;
    vm.runInContext(oldCommand, f.context);
    vm.runInContext(oldHeartbeat, f.context);
    assert.equal(f.state().commandPollSequence, polls, 'late old-generation callback is inert');
    assert.equal(f.tasks.size, 3, 'recovery does not multiply loops');
});

test('diagnostics preserve the host-reported engine name without requiring a named engine', () => {
    const f = fixture({ engineName: 'host-reported-engine' });
    f.advance(6000);
    assert.equal(f.state().runtimeEngineName, 'host-reported-engine');
    assert.equal(f.heartbeat().commandTickEngineName, 'host-reported-engine');
    assert.equal(f.heartbeat().heartbeatTickEngineName, 'host-reported-engine');
    assert.equal(f.heartbeat().watchdogTickEngineName, 'host-reported-engine');
    assert.equal(f.state().schedulerReady, true);
});

test('modal rejection before script entry writes no new evidence; surviving watchdog recovers after dismissal', () => {
    const f = fixture();
    const bootstrap = f.files.get(bootstrapPath);
    const initial = f.state();
    f.setModal(true);
    assert.throws(() => f.evaluate(startupPath), /modal dialog/);
    f.advance(18000);
    assert.equal(f.files.get(bootstrapPath), bootstrap);
    assert.equal(f.heartbeat().heartbeatSequence, 1);
    assert.equal(f.state().commandTickSequence, 0);
    assert.equal(f.state().heartbeatTickSequence, 0);
    assert.equal(f.state().watchdogTickSequence, 0);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    assert.equal(f.files.has(incidentPath), false);
    assert(f.rejected.length > 0, 'host gate rejected the registered strings before they ran');
    f.setModal(false);
    f.advance(9000);
    assert.equal(f.state().instanceId, initial.instanceId);
    assert.notEqual(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerRecoveryCount, 1);
    assert.equal(f.state().schedulerReady, true);
});

test('modal rejection that drops every reservation still requires an external entry after dismissal', () => {
    const f = fixture({ dropRejectedTasks: true });
    f.setModal(true);
    f.advance(18000);
    f.setModal(false);
    f.advance(18000);
    assert.equal(f.tasks.size, 0);
    assert.equal(f.state().watchdogTickSequence, 0);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    assert.equal(f.state().healthy, false);
    f.evaluate(startupPath);
    f.advance(6000);
    assert.equal(f.state().runtimeEngineName, 'main');
    assert.equal(f.state().schedulerRecoveryCount, 1);
    assert.equal(f.state().schedulerReady, true);
});

test('modal result delivery failure retries the saved result without replaying the command', () => {
    const f = fixture();
    const commandPath = f.heartbeat().commandFile;
    const resultPath = f.heartbeat().resultFile;
    const close = f.context.File.prototype.close;
    let failOnce = true;
    f.context.File.prototype.close = function () {
        if (failOnce && this.name.indexOf('.ae_mcp_result.json.tmp-') === 0) {
            failOnce = false;
            throw new Error('Cannot run a script while a modal dialog is waiting for response.');
        }
        return close.call(this);
    };
    f.files.set(commandPath, JSON.stringify({
        status: 'pending', command: 'executeJsx', requestId: 'modal-after-execution',
        args: {
            mode: 'unsafe', description: 'Isolated modal after execution', undoGroup: false,
            code: '$.global.__runs = ($.global.__runs || 0) + 1;',
        },
    }));
    f.advance(2000);
    assert.equal(f.context.__runs, 1);
    const statusAfterDispatch = f.read(commandPath).status;
    f.advance(20000);
    assert.equal(f.context.__runs, 1, 'partial or completed work is never replayed');
    assert.equal(statusAfterDispatch, 'running', 'delivery is pending, not another command execution');
    assert.equal(f.read(resultPath)._requestId, 'modal-after-execution');
    assert.equal(f.read(resultPath).status, 'success');
    assert.equal(f.read(commandPath).status, 'completed');
    assert.equal(f.state().currentRequestId, null);
    assert.equal(f.state().healthy, true);
    assert.equal(f.state().schedulerRecoveryCount, 0);
});

test('fresh command heartbeat cannot conceal a lost heartbeat task', () => {
    const f = fixture();
    f.tasks.delete(f.state().heartbeatTaskId);
    f.advance(15000);
    assert.equal(f.state().schedulerRecoveryCount, 1);
    const incident = f.read(incidentPath);
    assert.equal(incident.reason, 'scheduler-watchdog:heartbeat-tick-stale');
    assert(incident.state.heartbeatAgeMs < 3000, 'command loop was still writing heartbeat');
    assert.equal(incident.state.heartbeatTickSequence, 0);
    f.advance(3000);
    assert.equal(f.state().schedulerReady, true);
});

test('all scheduled tasks lost is observable but cannot self-recover in the same scheduler', () => {
    const f = fixture();
    const initial = f.state();
    f.tasks.clear();
    f.advance(364435);
    assert.equal(f.state().healthy, false);
    assert.equal(f.state().schedulerReady, false);
    assert.equal(f.state().heartbeatSequence, 1);
    assert.equal(f.state().watchdogTickSequence, 0);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    assert.equal(f.files.has(incidentPath), false, 'no callback means no diagnostic writer');
    // Model an explicit external lifecycle entry only after observing the limit.
    f.evaluate(startupPath);
    assert.equal(f.state().runtimeId, initial.runtimeId, 'external entry also allows overdue callbacks to resume');
    assert.equal(f.state().schedulerReady, false, 'new generation must demonstrate progress again');
    f.advance(6000);
    assert.equal(f.state().schedulerReady, true);
    assert.equal(f.tasks.size, 3);
});

test('manual heartbeat writes and repeated bootstrap do not prove scheduled progress', () => {
    const f = fixture();
    const initial = f.state();
    f.context.$.global.__adobeMcpBridgeRuntime.writeHeartbeat();
    f.evaluate(startupPath);
    assert.equal(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerReady, false);
    assert.equal(f.read(bootstrapPath).schedulerReady, false);
    assert.equal(f.tasks.size, 3);
});

test('a long first command runs once and does not make a pending heartbeat look stalled', () => {
    const f = fixture();
    const initial = f.state();
    f.context.__elapse = f.elapse;
    const commandPath = f.heartbeat().commandFile;
    const resultPath = f.heartbeat().resultFile;
    f.files.set(commandPath, JSON.stringify({
        status: 'pending', command: 'executeJsx', requestId: 'startup-long-request',
        args: {
            mode: 'unsafe', description: 'Isolated long command', undoGroup: false,
            code: '(function(){ $.global.__runs = ($.global.__runs || 0) + 1; ' +
                '__elapse(14000); aeMcpBridgeEnsureHealthy(); return "done"; })()',
        },
    }));
    f.advance(2000);
    assert.equal(f.read(resultPath).status, 'success');
    assert.equal(f.read(resultPath)._requestId, 'startup-long-request');
    // Watchdog was registered first, so it runs before the pending heartbeat.
    f.advance(0);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    assert.equal(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerReady, true);
    f.advance(15000);
    assert.equal(f.context.__runs, 1);
    assert.equal(f.state().schedulerRecoveryCount, 0);
});

test('excluding a long command from heartbeat idle time does not hide a later stall', () => {
    const f = fixture();
    f.tasks.delete(f.state().heartbeatTaskId);
    f.context.__elapse = f.elapse;
    f.files.set(f.heartbeat().commandFile, JSON.stringify({
        status: 'pending', command: 'executeJsx', requestId: 'startup-blocked-request',
        args: {
            mode: 'unsafe', description: 'Isolated blocked command', undoGroup: false,
            code: '__elapse(14000); "done";',
        },
    }));
    f.advance(2000);
    f.advance(0);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    f.advance(16000);
    assert.equal(f.state().schedulerRecoveryCount, 1);
    assert.equal(f.read(incidentPath).reason, 'scheduler-watchdog:heartbeat-tick-stale');
});

test('a host pause lets pending callbacks resume before watchdog recovery', () => {
    for (const phase of [1000, 3000, 6000]) {
        const f = fixture();
        f.advance(phase);
        const initial = f.state();
        f.elapse(16000);
        f.advance(0);
        f.advance(6000);
        assert.equal(f.state().runtimeId, initial.runtimeId, 'phase=' + phase);
        assert.equal(f.state().schedulerRecoveryCount, 0);
        assert.equal(f.state().schedulerReady, true);
    }
});

test('watchdog preserves a stalled runtime while the live render queue is active', () => {
    const f = fixture();
    f.advance(6000);
    const initial = f.state();
    assert.equal(f.heartbeat().renderQueueActive, false, 'disk state predates the render');
    f.tasks.delete(initial.commandTaskId);
    f.tasks.delete(initial.heartbeatTaskId);
    f.context.app.project.renderQueue.rendering = true;
    f.elapse(16000);
    f.advance(0);
    assert.equal(f.state().renderQueueActive, true);
    assert.equal(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerRecoveryCount, 0);
    assert.equal(f.files.has(incidentPath), false);
    f.context.app.project.renderQueue.rendering = false;
    f.advance(6000);
    assert.equal(f.state().schedulerRecoveryCount, 1, 'a surviving watchdog can recover after rendering');
    assert.notEqual(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerReady, true);
});

test('a host pause does not mask actually lost callbacks after one watchdog interval', () => {
    const f = fixture();
    f.advance(6000);
    const initial = f.state();
    f.tasks.delete(initial.commandTaskId);
    f.tasks.delete(initial.heartbeatTaskId);
    f.elapse(16000);
    f.advance(0);
    assert.equal(f.state().runtimeId, initial.runtimeId, 'defer once after observer suspension');
    assert.equal(f.state().healthy, false, 'do not fabricate progress during the grace');
    f.advance(9000);
    assert.notEqual(f.state().runtimeId, initial.runtimeId);
    assert.equal(f.state().schedulerRecoveryCount, 1);
    assert.equal(f.state().schedulerReady, true);
});

test('lifecycle health check preserves stale runtime during rendering', () => {
    const f = fixture();
    f.advance(6000);
    const initial = f.state();
    f.tasks.clear();
    f.elapse(16000);
    f.context.app.project.renderQueue.rendering = true;
    const result = f.context.$.global.__adobeMcpBridgeRuntime.ensureHealthy({ reason: 'panel-refresh' });
    assert.equal(result.runtimeId, initial.runtimeId);
    assert.equal(result.renderQueueActive, true);
    assert.equal(result.healthy, false, 'render deferral must not report scheduler progress');
    assert.equal(result.schedulerRecoveryCount, 0);
    assert.equal(f.tasks.size, 0);
    assert.equal(f.files.has(incidentPath), false);
    const startResult = f.context.$.global.__adobeMcpBridgeRuntime.start();
    assert.equal(startResult.runtimeId, initial.runtimeId, 'start on a running stale bridge also defers recovery');
    assert.equal(startResult.schedulerRecoveryCount, 0);
    assert.equal(f.tasks.size, 0);
});

test('shutdown cancels every schedule and late callback cannot revive the runtime', () => {
    const f = fixture();
    f.advance(6000);
    const callbacks = [...f.tasks.values()].map(t => t.code);
    f.evaluate(shutdownPath);
    assert.equal(f.state().running, false);
    assert.equal(f.state().schedulerReady, false);
    assert.equal(f.tasks.size, 0);
    assert.equal([...f.files.keys()].some(p => p.endsWith('/heartbeat.json')), false);
    for (const callback of callbacks) vm.runInContext(callback, f.context);
    f.advance(30000);
    assert.equal(f.state().running, false);
    assert.equal(f.tasks.size, 0);
});

const nativeSource = fs.readFileSync(path.resolve(__dirname,
    '../../crates/daemon-core/src/ae_recovery.jsx'), 'utf8');

function nativeRecovery(f, changes = {}) {
    const config = {
        nonce: 'native-test', instanceId: f.state().instanceId, runtimeId: f.state().runtimeId,
        appVersion: f.context.app.version, expiresAtMs: Number.MAX_SAFE_INTEGER,
        commandFile: bridgeRoot + '/instances/' + f.state().instanceId + '/ae_command.json',
        reservationFile: '/mock/current_request.json',
        resultFile: '/mock/native-result.json', ...changes,
    };
    vm.runInContext(nativeSource.replace('/*CONFIG*/', JSON.stringify(config)), f.context);
    return f.read(config.resultFile);
}

test('native recovery restores all lost tasks without evaluating a command or changing the project', () => {
    const f = fixture();
    f.advance(6000);
    const original = f.state();
    const project = JSON.stringify(f.context.app.project);
    f.tasks.clear();
    f.advance(30000);
    const reply = nativeRecovery(f);
    assert.equal(reply.outcome, 'deferred');
    assert.equal(f.state().instanceId, original.instanceId);
    assert.equal(f.state().runtimeId, original.runtimeId, 'native entry shares resume grace');
    assert.equal(f.state().healthy, false);
    f.advance(6000);
    assert.notEqual(f.state().runtimeId, original.runtimeId);
    assert.equal(f.state().schedulerReady, true);
    assert.equal(f.tasks.size, 3);
    assert.equal(JSON.stringify(f.context.app.project), project);
    assert.equal(f.files.has(f.heartbeat().commandFile), false);
    assert.equal(nativeRecovery(f, { runtimeId: original.runtimeId }).outcome, 'refused');
    assert.equal(f.state().schedulerRecoveryCount, 1, 'late duplicate entry is inert');
});

test('native recovery rearms a correlated pending request but does not execute it inline', () => {
    const f = fixture();
    f.advance(6000);
    f.tasks.clear();
    f.advance(30000);
    queueAuditCommand(f, 'pending-recovery', '$.global.__runs=($.global.__runs||0)+1;');
    f.files.set('/mock/current_request.json', JSON.stringify({requestId:'pending-recovery',command:'executeJsx',status:'running'}));
    const reply = nativeRecovery(f, {pendingRequestId:'pending-recovery'});
    assert.equal(reply.outcome, 'deferred');
    assert.equal(f.context.__runs, undefined);
    assert(f.files.has('/mock/current_request.json'), 'wrapper preserves broker reservation');
    f.advance(9000);
    assert.equal(f.context.__runs, 1);
    assert.equal(f.read(f.heartbeat().resultFile).status, 'success');
});

test('native pending recovery rejects mismatched or already-running reservations', () => {
    for (const mode of ['request','command','running','missing']) {
        const f = fixture();
        f.advance(6000); f.tasks.clear(); f.advance(30000);
        const command = queueAuditCommand(f, 'pending-recovery', '$.global.__runs=1;');
        if(mode === 'running') {command.status='running';f.files.set(f.heartbeat().commandFile,JSON.stringify(command));}
        if(mode !== 'missing') f.files.set('/mock/current_request.json',JSON.stringify({requestId:mode==='request'?'other':'pending-recovery',command:mode==='command'?'other':'executeJsx',status:'running'}));
        assert.equal(nativeRecovery(f,{pendingRequestId:'pending-recovery'}).outcome,'refused');
        assert.equal(f.context.__runs,undefined);
        assert.equal(f.tasks.size,0);
    }
});

test('native recovery refuses expired, wrong-target, stopped, busy and uncertain states', () => {
    for (const mode of ['expired', 'version', 'instance', 'runtime', 'stopped', 'busy', 'rendering',
        'reservation', 'pending', 'running', 'unknown', 'invalid-json']) {
        const f = fixture();
        f.advance(6000);
        f.tasks.clear();
        f.advance(30000);
        const initial = f.state().runtimeId;
        const changes = {};
        if (mode === 'expired') changes.expiresAtMs = 0;
        if (mode === 'version') changes.appVersion = '26.0';
        if (mode === 'instance') changes.instanceId = 'different';
        if (mode === 'runtime') changes.runtimeId = 'different';
        if (mode === 'stopped') f.context.__adobeMcpBridgeRuntime.stop();
        if (mode === 'busy') {
            const getState = f.context.__adobeMcpBridgeRuntime.getState;
            f.context.__adobeMcpBridgeRuntime.getState = () => ({ ...getState(), callbackActive: true });
        }
        if (mode === 'rendering') f.context.app.project.renderQueue.rendering = true;
        if (mode === 'reservation') f.files.set('/mock/current_request.json', '{}');
        if (['pending', 'running', 'unknown', 'invalid-json'].includes(mode)) {
            f.files.set(f.heartbeat().commandFile, mode === 'invalid-json' ? '{' : JSON.stringify({ status: mode }));
        }
        const reply = nativeRecovery(f, changes);
        assert.notEqual(reply.outcome, 'rearmed', mode);
        assert.equal(f.state().runtimeId, initial, mode);
        assert.equal(f.tasks.size, 0, mode);
    }
});

test('native recovery refuses a wrapper whose deadline expires during mailbox read', () => {
    const f = fixture();
    f.advance(6000);
    f.tasks.clear();
    f.advance(30000);
    const original = f.state().runtimeId;
    const commandPath = f.heartbeat().commandFile;
    f.files.set(commandPath, JSON.stringify({ status: 'completed' }));
    const read = f.context.File.prototype.read;
    f.context.File.prototype.read = function () {
        if (this.fsName === commandPath) f.elapse(11000);
        return read.call(this);
    };
    const reply = nativeRecovery(f, { expiresAtMs: new f.context.Date().getTime() + 10000 });
    assert.equal(reply.outcome, 'refused');
    assert.equal(f.state().runtimeId, original);
    assert.equal(f.tasks.size, 0);
});
