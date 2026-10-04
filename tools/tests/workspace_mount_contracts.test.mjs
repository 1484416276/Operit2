import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const root = new URL('../../', import.meta.url);
const mapperPath = 'core/crates/tool/services/src/files/PathMapper.rs';
const servicePath = 'core/crates/runtime/application/src/services/ChatServiceCore.rs';
const delegatePath = 'core/crates/runtime/application/src/services/core/ChatHistoryDelegate.rs';

/** Reads production source for workspace mounting contract checks. */
function source(path) {
  return readFileSync(new URL(path, root), 'utf8');
}

/** Extracts the source between two known declarations or match arms. */
function section(text, start, end) {
  const first = text.indexOf(start);
  assert.notEqual(first, -1, `Missing declaration: ${start}`);
  const last = text.indexOf(end, first + start.length);
  assert.notEqual(last, -1, `Missing following declaration: ${end}`);
  return text.slice(first, last);
}

/** Keeps selected Android directories on shared storage rather than plugin storage. */
test('Android workspace paths map directly to the shared-storage mount', () => {
  const mapper = source(mapperPath);
  const binding = section(mapper, 'fn normalizeWorkspaceBindingVfsPath(', 'fn normalizeWindowsHostWorkspacePath(');
  const android = section(binding, '[ROOT_SDCARD,', '[ROOT_DATA,');
  assert.match(android, /\[ROOT_SDCARD, rest @ \.\.\] \| \["storage", "emulated", "0", rest @ \.\.\]/);
  assert.match(android, /Ok\(Some\(joinNormalizedSegments\(\s*&\[ROOT_MNT, MNT_ANDROID, MNT_ANDROID_SDCARD\],\s*rest,/);
  assert.doesNotMatch(binding, /canonicalizeVfsPath|EXTENSIONS_PLUGIN_|#\[cfg\(/);
});

/** Returns validation and persistence errors without unwinding the runtime task. */
test('workspace binding uses the fallible folder-mount operation', () => {
  const binding = section(source(servicePath), 'pub fn bindChatToWorkspace(', 'pub fn createAndGetDefaultWorkspace(');
  assert.match(binding, /-> Result<\(\), String>/);
  assert.match(binding, /self\.chatHistoryDelegate\s*\.bindChatToFolderPath\(chatId, workspace\)\s*\.map\(\|_\| \(\)\)/);
  assert.doesNotMatch(binding, /\.bindChatToWorkspace\(|\.expect\(|\.unwrap\(|unwrap_or|normalizeWorkspaceBindingPath/);

  const mount = section(source(delegatePath), 'pub fn bindChatToFolderPath(', 'pub fn primaryWorkspacePathForChat(');
  assert.match(mount, /PathMapper::normalizeWorkspaceBindingPath\(&folderPath\)\?/);
  assert.match(mount, /Workspace::folderNameFromPath\(&folderPath\)\?/);
  assert.match(mount, /\.getWorkspaceForChat\(&chatId\)\s*\.map_err\(\|error\| error\.to_string\(\)\)\?/);
  assert.doesNotMatch(mount, /\.expect\(|\.unwrap\(|unwrap_or/);
});

/** Preserves the existing host filesystem target for normalized workspace mounts. */
test('shared-storage workspace mounts resolve through the host filesystem', () => {
  const mapper = source(mapperPath);
  const resolve = section(mapper, 'pub fn resolve(', 'pub fn mapPhysicalChildToVfs(');
  const android = section(resolve, '[ROOT_MNT, MNT_ANDROID, MNT_ANDROID_SDCARD,', '[ROOT_MNT, MNT_LINUX,');
  assert.match(android, /physicalPath: physicalPathString\(joinUnixPhysical\("\/sdcard", rest\)\)/);
  const vfs = source('core/crates/tool/services/src/files/VisualFileSystem.rs');
  const list = section(vfs, 'pub fn listFiles(', 'pub fn readFile(');
  assert.match(list, /self\.host\s*\.listFiles\(&resolved\.physicalPath\)/);
});

/** Requires repeated normalization and plugin-alias isolation to remain regression-tested. */
test('Rust regression cases cover picker paths and repeated normalization', () => {
  const mapper = source(mapperPath);
  const regression = section(mapper, 'fn androidWorkspaceBindingPathsRemainOnTheSharedStorageMount(', 'fn workspaceBindingRejectsPluginStoragePaths(');
  for (const path of [
    '/sdcard',
    '/storage/emulated/0',
    '/storage/emulated/0/Download',
    '/sdcard/Download/Operit/project',
    '/sdcard/Download/Operit/plugins/project',
    '/mnt/android/sdcard/Download/Operit/project',
  ]) {
    assert.ok(regression.indexOf(`"${path}"`) >= 0, `Missing regression path: ${path}`);
  }
  assert.match(regression, /PathMapper::normalizeWorkspaceBindingPath\(&normalized\)\.unwrap\(\)/);
  assert.match(regression, /PathMapper::canonicalizeVfsPath\(&normalized\)\.unwrap\(\)/);
});
