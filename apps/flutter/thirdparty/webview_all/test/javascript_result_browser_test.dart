@TestOn('browser')
library;

import 'dart:js_interop';

import 'package:flutter_test/flutter_test.dart';
import 'package:webview_all/webview_all.dart';
import 'package:webview_flutter_platform_interface/webview_flutter_platform_interface.dart';

@JS('eval')
external JSAny? _evaluate(JSString script);

void main() {
  late WebViewController controller;

  setUp(() {
    controller = WebViewController.fromPlatform(_ScriptPlatform());
  });

  test('null, undefined and void functions resolve to null', () async {
    for (final script in ['null', 'undefined', '(function() {})()', '']) {
      expect(await controller.runJavaScriptReturningResult(script), isNull);
    }
  });

  test('preserves JSON-compatible result types and string contents', () async {
    final cases = <String, Object?>{
      '42': 42,
      'false': false,
      '"null"': 'null',
      '"42"': '42',
      '""': '',
      '"{\\"ok\\":true}"': '{"ok":true}',
      '[1, null, "two"]': [1, null, 'two'],
      '({ok: true, value: null})': {'ok': true, 'value': null},
    };
    for (final entry in cases.entries) {
      expect(
        await controller.runJavaScriptReturningResult(entry.key),
        entry.value,
        reason: entry.key,
      );
    }
  });

  test('executes once and preserves global declarations', () async {
    await controller.runJavaScriptReturningResult(
      'var __resultTestCount = 0; '
      'function __resultTestIncrement() { __resultTestCount++; }',
    );
    expect(
      await controller.runJavaScriptReturningResult('__resultTestIncrement();'),
      isNull,
    );
    expect(
      await controller.runJavaScriptReturningResult('__resultTestCount'),
      1,
    );
  });

  test('execution and serialization errors remain failures', () async {
    for (final script in [
      'throw new Error("test failure")',
      'const = invalid',
      '({ value: 1n })',
      '(() => { const a = {}; a.self = a; return a; })()',
      '(function() {})',
    ]) {
      await expectLater(
        controller.runJavaScriptReturningResult(script),
        throwsStateError,
        reason: script,
      );
    }
  });
}

/// Executes the actual transport script and enforces native adapters' non-null
/// result contract, rather than substituting a precomputed return value.
class _ScriptPlatform extends PlatformWebViewController {
  _ScriptPlatform()
    : super.implementation(const PlatformWebViewControllerCreationParams());

  @override
  Future<Object> runJavaScriptReturningResult(String javaScript) async {
    final result = _evaluate(javaScript.toJS);
    if (result == null) throw ArgumentError('Native result must not be null');
    return (result as JSString).toDart;
  }
}
