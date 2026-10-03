import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/features/settings/characters/MemoryGraphScreen.dart';

void main() {
  testWidgets(
    'owner-scoped graph keeps zoom when selecting and closing details',
    (tester) async {
      final bridge = _GraphBridge();
      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: MemoryGraphScreen(
            bridge: bridge,
            ownerKey: 'character:alice',
            ownerName: 'Alice',
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('100%'), findsOneWidget);
      await tester.tap(find.byTooltip('放大'));
      await tester.pumpAndSettle();
      expect(find.text('125%'), findsOneWidget);
      final canvas = find.byKey(const ValueKey('memory-graph-gestures'));
      await tester.tapAt(tester.getCenter(canvas));
      await tester.pumpAndSettle();
      expect(find.text('测试记忆'), findsOneWidget);
      expect(find.text('未读取到完整记忆内容'), findsOneWidget);
      expect(find.text('125%'), findsOneWidget);
      expect(bridge.calls, [
        'getMemoryGraph',
        'getAllFolderPaths',
        'findMemoriesByTitle',
      ]);
      expect(bridge.owners, everyElement('character:alice'));
      // The app bar also has a close button; close only the details card.
      await tester.tap(find.byIcon(Icons.close).first);
      await tester.pumpAndSettle();
      expect(find.text('测试记忆'), findsNothing);
      expect(find.text('125%'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
}

class _GraphBridge extends OperitRuntimeBridge {
  final calls = <String>[];
  final owners = <String>[];

  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    calls.add(request.methodName);
    owners.add((request.args as Map)['__core_instance_id'] as String);
    final Object value;
    switch (request.methodName) {
      case 'getMemoryGraph':
        value = {
          'nodes': [
            {'id': 'one', 'label': '测试记忆', 'color': 0xFF4CAF50, 'metadata': {}},
          ],
          'edges': [],
        };
      case 'getAllFolderPaths':
      case 'findMemoriesByTitle':
        value = [];
      default:
        throw StateError('Unexpected call: ${request.methodName}');
    }
    return encodeCoreLink([0, value]);
  }

  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw StateError('Unexpected push');
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw StateError('Unexpected snapshot');
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw StateError('Unexpected watch');
}
