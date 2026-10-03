import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/core/proxy/generated/CoreProxyModels.g.dart';
import 'package:operit2/ui/features/chat/components/MessageContextMenu.dart';

/// Verifies every selected reply can be deleted when another revision exists.
void main() {
  testWidgets(
    'does not offer variant deletion for a reply without alternates',
    (tester) async {
      await _openMenu(tester, selectedVariantIndex: 0, variantCount: 1);

      expect(find.text('删除当前变体'), findsNothing);
      expect(find.text('删除'), findsOneWidget);
    },
  );

  for (final selectedVariantIndex in <int>[0, 1, 2]) {
    testWidgets(
      'deletes selected revision $selectedVariantIndex before refreshing',
      (tester) async {
        final actions = <String>[];
        await _openMenu(
          tester,
          selectedVariantIndex: selectedVariantIndex,
          variantCount: 3,
          onDeleteMessageVariant: (timestamp, variantIndex) async {
            actions.add('delete:$timestamp:$variantIndex');
          },
          onRefresh: () async {
            actions.add('refresh');
          },
        );

        expect(find.text('删除当前变体'), findsOneWidget);
        await tester.tap(find.text('删除当前变体'));
        await tester.pumpAndSettle();

        expect(actions, <String>[
          'delete:123:$selectedVariantIndex',
          'refresh',
        ]);
        expect(tester.takeException(), isNull);
      },
    );
  }
}

/// Opens the real long-press menu for a message with explicit variant metadata.
Future<void> _openMenu(
  WidgetTester tester, {
  required int selectedVariantIndex,
  required int variantCount,
  MessageVariantAction? onDeleteMessageVariant,
  Future<void> Function()? onRefresh,
}) async {
  await tester.binding.setSurfaceSize(const Size(800, 1200));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  final clients = GeneratedCoreProxyClients(_MessageMenuBridge());
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Center(
          child: MessageContextMenu(
            message: ChatMessage(
              sender: 'ai',
              parts: const <MessagePart>[],
              timestamp: 123,
              roleName: 'assistant',
              selectedVariantIndex: selectedVariantIndex,
              variantCount: variantCount,
              provider: 'test',
              modelName: 'test',
              inputTokens: 0,
              outputTokens: 0,
              cachedInputTokens: 0,
              sentAt: 0,
              outputDurationMs: 0,
              waitDurationMs: 0,
              completedAt: 1,
              displayMode: ChatMessageDisplayMode.normal,
              isFavorite: false,
              contentStream: null,
            ),
            chatId: 'chat',
            messageIndex: 0,
            clients: clients,
            packageManager: clients.application.packageManager(),
            onToggleFavoriteMessage: (timestamp, isFavorite) async {},
            onDeleteMessageVariant: onDeleteMessageVariant,
            onRefresh: onRefresh,
            child: const SizedBox(
              width: 200,
              height: 60,
              child: Text('Assistant reply'),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.longPress(find.text('Assistant reply'));
  await tester.pumpAndSettle();
  expect(find.text('重新生成'), findsOneWidget);
}

/// Supplies an empty extension menu through the actual Core bridge codec.
class _MessageMenuBridge extends OperitRuntimeBridge {
  /// Returns only the extension-menu response expected by this fixture.
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    expect(request.target, 'core/application.packageManager');
    expect(request.methodName, 'getToolPkgChatMessageMenuItems');
    return encodeCoreLink(<Object?>[0, <Object?>[]]);
  }

  /// Rejects push operations outside this menu fixture.
  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();

  /// Rejects snapshot operations outside this menu fixture.
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();

  /// Rejects stream operations outside this menu fixture.
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw UnimplementedError();
}
