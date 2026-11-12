const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

const source = fs.readFileSync(path.join(__dirname, '../../src/illustrator/cep/mcp-bridge-illustrator/jsx/bridge.jsx'), 'utf8');

function fixture(useFallback = false) {
    const globals = { $: { global: {} } };
    if (useFallback) globals.JSON = undefined;
    const context = vm.createContext(globals);
    vm.runInContext(source, context);
    context.aiGetInstanceMetadata = () => ({ instanceId: 'illustrator-test' });
    context.aiWriteHeartbeat = () => ({});
    context.aiMcpBridgeGetState = () => '{}';
    context.aiInstanceResultFile = () => ({ fsName: 'result' });
    let pending;
    let result;
    context.aiFindPendingCommand = () => pending;
    context.aiUpdateCommandStatus = (_file, payload, status) => { payload.status = status; };
    context.aiWriteResult = (_file, text) => { result = JSON.parse(text); };
    return {
        run(id, code) {
            const payload = { command: 'executeJsx', requestId: id, args: { code } };
            pending = { file: {}, isInstance: true, payload };
            result = undefined;
            context.aiMcpBridgeCheck();
            assert.equal(payload.status, 'completed');
            assert.equal(result._requestId, id);
            assert.equal(result._commandExecuted, 'executeJsx');
            return result;
        },
    };
}

for (const fallback of [false, true]) {
    test(`array result is identified and the next request completes (fallback JSON: ${fallback})`, () => {
        const host = fixture(fallback);
        const first = host.run('array-request', 'JSON.stringify([{name:"空"}, 42])');
        assert.equal(first.status, 'success');
        assert.deepEqual(first.result, [{ name: '空' }, 42]);
        const second = host.run('following-request', '({status:"success", result:"次"})');
        assert.equal(second.result, '次');
    });
}

test('JSON primitive results are enveloped and objects retain their existing shape', () => {
    const host = fixture();
    for (const [code, expected] of [['"null"', null], ['"42"', 42], ['"false"', false], ['"\\"hello\\""', 'hello']]) {
        assert.equal(host.run('primitive', code).result, expected);
    }
    const result = host.run('object', '({status:"success", value:7})');
    assert.equal(result.value, 7);
    assert.equal(Object.hasOwn(result, 'result'), false);
});
