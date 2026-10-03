import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const root = new URL('../../', import.meta.url);
const android = 'apps/flutter/app/android/app/';

/** Reads a production file for cross-language host contract checks. */
function source(path) {
  return readFileSync(new URL(path, root), 'utf8');
}

/** Keeps Android runtime tools on the existing owner channel rather than secret-store JNI. */
test('Android device information is implemented by the Flutter owner', () => {
  const host = source('hosts/android/src/system_operation.rs');
  assert.doesNotMatch(host, /readAndroidDeviceInfo|deviceInfoJson|jni::|serde_json::from_str/);
  assert.match(host, /fn getDeviceInfo\(&self\)[\s\S]*?Android get_device_info requires/);
  const factory = source('apps/flutter/native/operit-flutter-bridge/src/PlatformRuntimeFactory.rs');
  const method = factory.slice(factory.indexOf('    fn getDeviceInfo('), factory.indexOf('    fn captureScreenshot('));
  assert.match(method, /requestOwnerSystemOperation\(/);
  assert.match(method, /operation: "get_device_info"\.to_string\(\)/);
  assert.match(method, /serde_json::from_str\(&response.resultJson\)/);
  assert.match(method, /#\[cfg\(any\(target_os = "ios", target_os = "macos"\)\)\]/);
  assert.match(method, /self.native.getDeviceInfo\(\)/);
  assert.doesNotMatch(factory, /requestFlutterOwnerSystemOperation/);
  const dart = source('apps/flutter/app/lib/core/host/RuntimeHostInteractionSubscriber.dart');
  assert.match(dart, /'ownerSystemOperation',\s*payload.toJson\(\)/);
  const owner = source(`${android}src/main/kotlin/app/operit/OwnerSystemCapabilityChannel.kt`);
  assert.match(owner, /"get_device_info" -> mapOf\(\s*"resultJson" to AndroidHostDeviceInfo.read\(activity.applicationContext\)/);
  const runtime = source(`${android}src/main/kotlin/app/operit/AndroidRuntimeHost.kt`);
  assert.doesNotMatch(runtime, /deviceInfoJson|AndroidHostDeviceInfo/);
});

/** Supplies only required startup identity before FFI and owner subscriptions become available. */
test('Android startup receives the device model without querying the owner', () => {
  const kotlin = source(`${android}src/main/kotlin/app/operit/AndroidRuntimeHost.kt`);
  assert.match(kotlin, /OperitRuntimeNative.create\(\s*paths.runtimeRoot.absolutePath,\s*paths.workspaceRoot.absolutePath,\s*Build.MODEL,\s*this,/);
  const native = source(`${android}src/main/kotlin/app/operit/OperitRuntimeNative.kt`);
  assert.match(native, /external fun create\(\s*runtimeRoot: String,\s*workspaceRoot: String,\s*deviceModel: String,\s*host: AndroidRuntimeHost,/);
  const jni = source('apps/flutter/native/operit-flutter-bridge/src/AndroidJni.rs');
  assert.match(jni, /workspace_root: JString,\s*device_model: JString,\s*host: JObject,/);
  assert.match(jni, /device_model.trim\(\).is_empty\(\)/);
  assert.match(jni, /new_with_storage_roots\(runtime_root, workspace_root, device_model\)/);
  const factory = source('apps/flutter/native/operit-flutter-bridge/src/PlatformRuntimeFactory.rs');
  const startup = factory.slice(factory.indexOf('pub(crate) fn local_device_info('), factory.indexOf('struct FlutterSystemOperationBridge'));
  assert.match(startup, /#\[cfg\(target_os = "android"\)\]\s*let model = \{/);
  assert.match(startup, /device_model.trim\(\).is_empty\(\)/);
  assert.match(startup, /#\[cfg\(not\(target_os = "android"\)\)\]\s*let model =/);
  assert.doesNotMatch(startup, /requestOwner|\.unwrap_or|\.or_else/);
  const bridge = source('apps/flutter/native/operit-flutter-bridge/src/lib.rs');
  assert.match(bridge, /local_device_info\(\s*localCore.as_ref\(\),\s*#\[cfg\(target_os = "android"\)\]\s*device_model,/);
});

/** Keeps C-host startup separate from Android's model-bearing JNI constructor. */
test('Android cannot bypass its host-owned creation boundary', () => {
  const exports = source('apps/flutter/native/operit-flutter-bridge/src/BridgeExports.rs');
  for (const name of ['operit_flutter_bridge_create', 'operit_flutter_bridge_create_with_storage_roots']) {
    const declaration = exports.indexOf(`fn ${name}(`);
    assert.notEqual(declaration, -1);
    const annotation = exports.slice(exports.lastIndexOf('#[cfg(', declaration), declaration);
    assert.match(annotation, /not\(any\(target_env = "ohos", target_os = "android"\)\)/);
  }
});

/** Requires the Kotlin producer to supply every shared Rust device field. */
test('Android device JSON matches the required shared host schema', () => {
  const api = source('core/crates/foundation/host-api/src/lib.rs');
  const contract = api.match(/#\[derive\(([^\n]+)\)\]\s*pub struct DeviceInfoData \{([^}]+)\}/);
  assert.ok(contract);
  assert.match(contract[1], /Serialize, Deserialize/);
  const fields = [...contract[2].matchAll(/pub (\w+):/g)].map(match => match[1]);
  const kotlin = source(`${android}src/main/kotlin/app/operit/AndroidHostDeviceInfo.kt`);
  const json = kotlin.slice(kotlin.indexOf('return JSONObject()'), kotlin.indexOf('.put("brand"'));
  const keys = [...json.matchAll(/\.put\("(\w+)"/g)].map(match => match[1]);
  assert.deepEqual(keys.sort(), fields.sort());
  assert.doesNotMatch(contract[2], /serde\(default/);
  assert.match(kotlin, /ActivityManager\.MemoryInfo\(\)/);
  assert.match(kotlin, /StatFs\(context.filesDir.absolutePath\)/);
  assert.match(kotlin, /Settings\.Secure\.ANDROID_ID/);
  assert.match(kotlin, /BatteryManager\.BATTERY_PROPERTY_CAPACITY/);
  assert.doesNotMatch(kotlin, /MethodChannel|Build.VERSION.SDK_INT\s*[<>=]|catch\s*\(/);
});

/** Preserves the method names Rust resolves in minified release builds. */
test('release shrinking preserves the runtime host JNI methods', () => {
  const rules = source(`${android}proguard-rules.pro`);
  assert.match(rules, /-keepclassmembers class app\.operit\.AndroidRuntimeHost \{\s*public \*\*\* \*\(\.\.\.\);/);
});

/** Prevents the unfiltered main asset directory from reaching APK merge tasks. */
test('Android asset sources are generated from the selected ABI set', () => {
  const gradle = source(`${android}build.gradle.kts`);
  assert.match(gradle, /sourceSets.getByName\("main"\).assets.setSrcDirs\(emptyList<String>\(\)\)/);
  assert.match(gradle, /selectedAbis.set\(selectedOperitRustTargets.map \{ it.abi \}\)/);
  assert.match(gradle, /fileSystemOperations.sync \{/);
  assert.match(gradle, /exclude\("android-runtime\/\*\*"\)/);
  assert.match(gradle, /from\(sourceDirectory.dir\("android-runtime\/\$abi"\)\)/);
  assert.match(gradle, /into\("android-runtime\/\$abi"\)/);
  assert.match(gradle, /assets.addGeneratedSourceDirectory\(stagedAssets\) \{ it.outputDirectory \}/);
  assert.match(gradle, /"stageOperitAndroidAssets" \+ variant.name/);
});

/** Keeps release publication behind the actual APK asset validation. */
test('Android releases verify the final APK before copying it to dist', () => {
  const script = source('tools/build_scripts/build_flutter_android.py');
  const main = script.slice(script.indexOf('def main()'));
  assert.ok(main.indexOf('verify_android_runtime_assets(apk_path, "arm64-v8a")') <
    main.indexOf('copy_required_file('));
  assert.match(main, /"--target-platform",\s*"android-arm64"/);
});
