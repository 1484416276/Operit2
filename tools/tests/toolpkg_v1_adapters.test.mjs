import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import vm from 'node:vm';
import test from 'node:test';

const root = new URL('../../', import.meta.url);
const adapterRoot = 'core/crates/plugin/sdk/src/compat/v1/';

/** Reads an actual production source file from the repository. */
function source(path) { return readFileSync(new URL(path, root), 'utf8'); }

/** Converts isolated-realm JSON data for assertions. */
function plain(value) { return JSON.parse(JSON.stringify(value)); }

/** Creates one engine using the production dispatcher, adapters, and installer. */
function engine(version = '1.0.0') {
  const calls = [];
  const files = {};
  // These fixtures model v2 result shapes, not the legacy input convention.
  for (const name of ['list', 'read', 'readPart', 'write', 'writeBinary', 'readBinary', 'deleteFile',
    'exists', 'move', 'copy', 'mkdir', 'find', 'grep', 'grepContext', 'info', 'apply', 'create',
    'edit', 'zip', 'unzip', 'open', 'share', 'download']) {
    /** Captures the actual current signature selected by the production adapter. */
    files[name] = async (...args) => {
      calls.push({ name, args });
      const path = typeof args[0] === 'string' ? args[0] : args[0].path;
      if (name === 'download') {
        return { operation: name, path: typeof args[0] === 'string' ? args[1] : args[0].destination, successful: true, details: 'done' };
      }
      if (name === 'apply' || name === 'edit' || name === 'create') {
        return { operation: { operation: name, path, successful: true, details: 'done' }, aiDiffInstructions: 'instructions' };
      }
      if (name === 'find') return { path, pattern: args[1], files: [path + '/found.txt'] };
      if (name === 'grep' || name === 'grepContext') return {
        searchPath: path, pattern: args[1], totalMatches: 1, filesSearched: 1,
        matches: [{ filePath: path + '/found.txt', lineMatches: [{ lineNumber: 1, lineContent: 'text', matchContext: null }] }],
      };
      return { path, operation: name, successful: true, details: 'done', content: 'text', size: 4, entries: [] };
    };
  }
  const tools = {
    Files: files,
    Chat: {
      /** Models the current open-string role/result contract. */
      async call(options) {
        calls.push({ name: 'Chat.call', args: [options] });
        return {
          text: 'reply', turns: [{ kind: 'ASSISTANT', content: 'reply', metadata: {} }],
          finishReason: 'stop', metadata: { protocolMeta: [] }, receivedAt: 123,
        };
      },
    },
  };
  const context = vm.createContext({
    version, Tools: tools, __operitCurrentCallId: 'call',
    /** Supplies the active call's declared contract. */
    __operitGetCallState() { return { params: { __operit_toolpkg_api_version: context.version } }; },
    /** Publishes the runtime namespace in this engine. */
    __operitExpose(name, value) { context[name] = value; },
  });
  const rust = source('core/crates/plugin/sdk/src/toolpkg/ToolPkgApiRuntimeScript.rs');
  const start = rust.indexOf('r#"') + 3;
  vm.runInContext(rust.slice(start, rust.indexOf('"#', start)), context);
  for (const file of ['files.js', 'chat.js', 'workflow.js', 'install.js']) vm.runInContext(source(adapterRoot + file), context);
  return { context, tools, calls };
}

/** Checks the copied handwritten contract remains byte-identical to its recorded source. */
test('v1 declarations match their recorded source hashes', () => {
  const directory = new URL('plugins/types-v1/', root);
  const provenance = JSON.parse(readFileSync(new URL('source.json', directory), 'utf8'));
  assert.match(provenance.commit, /^[0-9a-f]{40}$/);
  assert.deepEqual(readdirSync(directory).filter(name => name.endsWith('.d.ts')).sort(), Object.keys(provenance.files).sort());
  for (const [file, digest] of Object.entries(provenance.files)) {
    assert.equal(createHash('sha256').update(readFileSync(new URL(file, directory))).digest('hex'), digest, file);
  }
});

/** Checks the shifted legacy zip argument and both path directions. */
test('v1 zip maps environment and restores the source result path', async () => {
  const { tools, calls } = engine();
  const result = await tools.Files.zip('/work', '/archive.zip', 'linux', false);
  assert.deepEqual(plain(calls[0]), { name: 'zip', args: ['/mnt/linux/work', '/mnt/linux/archive.zip', false] });
  assert.equal(result.env, 'linux');
  assert.equal(result.path, '/work');
});

/** Checks default environment semantics do not depend on the machine executing JavaScript. */
test('v1 defaults to its documented Android contract and delegates mounts to the host', async () => {
  const { tools, calls } = engine('1.0.1');
  const result = await tools.Files.read('/storage/emulated/0/file.txt');
  assert.equal(calls[0].args[0].path, '/sdcard/file.txt');
  assert.equal(result.env, 'android');
  assert.equal(result.path, '/storage/emulated/0/file.txt');
  await tools.Files.list('/app/data/plugins/demo');
  assert.equal(calls[1].args[0], '/app/data/plugins/demo');
  await assert.rejects(tools.Files.read('/unsupported-root/file.txt'), /no VFS mapping/);
  await assert.rejects(tools.Files.read('/../../file.txt'), /escapes/);
  await assert.rejects(tools.Files.list('/work', 'invented'), /Unsupported/);
  assert.equal(calls.length, 2);
});

/** Checks legacy options are converted without leaking environment into v2 options. */
test('v1 read options preserve intent and caller ownership', async () => {
  const { tools, calls } = engine();
  const options = Object.freeze({ path: '/work/file', environment: 'linux', intent: 'read', direct_image: false });
  await tools.Files.read(options);
  assert.deepEqual(plain(calls[0].args), [{ path: '/mnt/linux/work/file', intent: 'read', direct_image: false }]);
  assert.equal(options.environment, 'linux');
});

/** Checks the distinct source and destination environment arguments survive adaptation. */
test('v1 cross-environment copy preserves both addresses', async () => {
  const { tools, calls } = engine();
  const result = await tools.Files.copy('/tmp/a', '/sdcard/b', true, 'linux', 'android');
  assert.deepEqual(plain(calls[0].args), ['/mnt/linux/tmp/a', '/sdcard/b', true]);
  assert.equal(result.path, '/tmp/a');
  assert.equal(result.env, 'linux');
});

/** Checks nested operation data and search entries use legacy result shapes. */
test('v1 nested and search results restore environments and paths', async () => {
  const { tools } = engine();
  const edit = await tools.Files.edit('/work/a', 'old', 'new', 'linux');
  assert.equal(edit.operation.env, 'linux');
  assert.equal(edit.operation.path, '/work/a');
  const found = await tools.Files.find('/work', '*.txt', {}, 'linux');
  assert.deepEqual(plain(found.files), ['/work/found.txt']);
  const grep = await tools.Files.grep('/work', 'text', { environment: 'linux', file_pattern: '*.txt' });
  assert.equal(grep.env, 'linux');
  assert.equal(grep.searchPath, '/work');
  assert.equal(grep.filePattern, '*.txt');
  assert.equal(grep.matches[0].filePath, '/work/found.txt');
  assert.deepEqual(plain(grep.matches[0].lineMatches[0]), { lineNumber: 1, lineContent: 'text' });
});

/** Checks headers remain the third current argument rather than the legacy environment. */
test('v1 download supports both handwritten overloads', async () => {
  const { tools, calls } = engine();
  const headers = Object.freeze({ Accept: 'text/plain' });
  await tools.Files.download('https://example.invalid/file', '/out', 'linux', headers);
  assert.deepEqual(plain(calls[0].args), ['https://example.invalid/file', '/mnt/linux/out', headers]);
  const options = Object.freeze({ visit_key: 'visit', link_number: 2, destination: '/out', environment: 'linux', headers });
  const result = await tools.Files.download(options);
  assert.deepEqual(plain(calls[1].args), [{ visit_key: 'visit', link_number: 2, destination: '/mnt/linux/out', headers }]);
  assert.equal(result.path, '/out');
  assert.equal(result.env, 'linux');
  assert.equal(options.destination, '/out');
});

/** Checks independent calls sharing an engine never inherit another call's contract. */
test('v2 keeps its original signatures and receives no legacy result fields', async () => {
  const { tools, calls, context } = engine();
  await tools.Files.zip('/work', '/out', 'linux', false);
  context.version = '2.0.0';
  const result = await tools.Files.zip('/app/workspaces/a', '/app/data/out', false);
  assert.deepEqual(plain(calls[1].args), ['/app/workspaces/a', '/app/data/out', false]);
  assert.equal(Object.hasOwn(result, 'env'), false);
  context.version = '1.0.1';
  assert.equal((await tools.Files.list('/work', 'linux')).env, 'linux');
});

/** Checks host errors propagate unchanged and never trigger another implementation. */
test('v1 adapter preserves a host rejection without retry', async () => {
  const context = vm.createContext({});
  vm.runInContext(source(adapterRoot + 'files.js'), context);
  const error = new Error('mount unavailable');
  let count = 0;
  const current = {
    /** Rejects the exact operation selected by the compatibility adapter. */
    async zip() { count++; throw error; },
  };
  const legacy = context.__operitCreateV1Files(current);
  await assert.rejects(legacy.zip('/work', '/out', 'linux', false), caught => caught === error);
  assert.equal(count, 1);
});

/** Checks the actual v1 nullable field converts to the v2 optional field. */
test('v1.0.1 Chat.call converts nullable tool names and preserves caller data', async () => {
  const { tools, calls } = engine('1.0.1');
  const turn = Object.freeze({ kind: 'USER', content: 'hello', toolName: null });
  const result = await tools.Chat.call({ functionType: 'CHAT', turns: [turn] });
  assert.deepEqual(plain(calls[0].args[0].turns), [{ kind: 'USER', content: 'hello' }]);
  assert.equal(turn.toolName, null);
  assert.equal(result.finishReason, 'stop');
  assert.equal(result.turns[0].kind, 'ASSISTANT');
  const old = engine('1.0.0');
  assert.throws(() => old.tools.Chat.call({ functionType: 'CHAT', turns: [turn] }), /requires/);
  assert.equal(old.calls.length, 0);
});

/** Checks new v2 enum values cannot leak into the closed v1 return contract. */
test('v1 Chat.call rejects unrepresentable current results', async () => {
  const context = vm.createContext({});
  vm.runInContext(source(adapterRoot + 'chat.js'), context);
  const response = { text: '', turns: [], finishReason: 'new_v2_reason', metadata: {}, receivedAt: 1 };
  /** Supplies a current result with a deliberately unsupported legacy value. */
  const call = context.__operitCreateV1ChatCall(async () => response);
  const request = { functionType: 'CHAT', turns: [] };
  await assert.rejects(call(request), /finish reason/);
  response.finishReason = 'stop';
  response.turns = [{ kind: 'NEW_ROLE', content: '', metadata: {} }];
  await assert.rejects(call(request), /turn kind/);
});
