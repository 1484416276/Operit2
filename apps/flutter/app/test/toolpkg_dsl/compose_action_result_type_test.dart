import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/features/packages/screens/ToolPkgUiLauncherScreen.dart';

/// Verifies callback values survive both action parsing paths without re-decoding.
void main() {
  final cases = <String, Object?>{
    'JSON success string': '{"success":true}',
    'JSON failure string': '{"success":false,"message":"write failed"}',
    'JSON config string': '{"character_card_name":"Ash","chat_query":"巡检"}',
    'JSON array string': '[{"id":1}]',
    'JSON scalar strings': 'false',
    'JSON number string': '75',
    'JSON null string': 'null',
    'JSON quoted string': '"hello"',
    'empty string': '',
    'whitespace string': '  \n ',
    'formatted JSON string': ' \n {"success":true}\n ',
    'plain text': 'hello 🌸',
    'object': <String, Object?>{'success': true, 'message': '保存成功'},
    'array': <Object?>[
      1,
      'two',
      <String, Object?>{'id': 3},
    ],
    'boolean': false,
    'number': 75,
    'null': null,
  };

  for (final entry in cases.entries) {
    for (final phase in ['intermediate', 'final']) {
      test('$phase preserves ${entry.key} in both parsing paths', () {
        final parsed = parseComposeDslActionEventForTest(
          _event(entry.value, phase: phase, includeTree: true),
        );
        expect(parsed.hasRenderResult, isTrue);
        expect(parsed.actionResult, equals(entry.value));
        expect(parsed.renderedActionResult, equals(entry.value));
      });
    }

    test('tree-less action preserves ${entry.key}', () {
      final parsed = parseComposeDslActionEventForTest(
        _event(entry.value, includeTree: false),
      );
      expect(parsed.hasRenderResult, isFalse);
      expect(parsed.actionResult, equals(entry.value));
    });
  }

  test('Gentle Guardian can parse its JSON response exactly once', () {
    for (final success in [true, false]) {
      final response = jsonEncode({
        'success': success,
        if (!success) 'message': 'write failed',
      });
      final parsed = parseComposeDslActionEventForTest(_event(response));
      // Mirrors the host envelope that resolves Guardian.saveConfig in the page.
      final envelope =
          jsonDecode(jsonEncode({'success': true, 'data': parsed.actionResult}))
              as Map<String, Object?>;
      expect(envelope['data'], isA<String>());
      final pageResult = jsonDecode(envelope['data'] as String);
      expect(pageResult['success'], success);
      if (!success) expect(pageResult['message'], 'write failed');
    }
  });

  test(
    'decodes nested transport envelopes without decoding callback content',
    () {
      const response = '{"success":true}';
      final event = jsonDecode(_event(response)) as Map<String, Object?>;
      event['result'] = jsonEncode(event['result']);
      final parsed = parseComposeDslActionEventForTest(jsonEncode(event));
      expect(parsed.actionResult, response);
      expect(parsed.renderedActionResult, response);
    },
  );

  test('outer action errors are still propagated', () {
    expect(
      () => parseComposeDslActionEventForTest(
        jsonEncode({
          'phase': 'final',
          'result': jsonEncode({'success': false, 'message': 'action failed'}),
        }),
      ),
      throwsA(predicate((error) => error.toString().contains('action failed'))),
    );
  });
}

/// Encodes the real Core watch envelope with an optional rendered tree.
String _event(
  Object? actionResult, {
  String phase = 'final',
  bool includeTree = true,
}) => jsonEncode({
  'phase': phase,
  'result': jsonEncode({
    'success': true,
    if (includeTree)
      'tree': {
        'type': 'Text',
        'props': {'text': 'Guardian'},
        'children': <Object?>[],
      },
    'state': <String, Object?>{},
    'memo': <String, Object?>{},
    'actionResult': actionResult,
  }),
});
