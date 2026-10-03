import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/core/proxy/generated/CoreProxyModels.g.dart';
import 'package:operit2/ui/features/chat/components/ChatRuntimeScope.dart';
import 'package:operit2/ui/features/chat/components/part/CustomXmlRenderer.dart';

/// Verifies committed hook changes refresh XML that is already mounted in chat.
void main() {
  testWidgets(
    'existing unhandled XML refreshes after hook registration and removal',
    (tester) async {
      final bridge = _XmlBridge();
      addTearDown(bridge.dispose);
      await tester.pumpWidget(_surface(bridge));
      await tester.pumpAndSettle();
      expect(find.text('original body'), findsOneWidget);
      final originalNode = tester.element(find.byType(CustomXmlRenderer));
      bridge.result = const <String, Object?>{
        'kind': 'text',
        'text': 'registered renderer',
      };
      bridge.emitRevision(1);
      await tester.pumpAndSettle();
      expect(find.textContaining('registered renderer'), findsWidgets);
      expect(
        tester.element(find.byType(CustomXmlRenderer)),
        same(originalNode),
      );
      final calls = bridge.renderCalls;
      bridge.emitRevision(1);
      await tester.pumpAndSettle();
      expect(bridge.renderCalls, calls);
      bridge.result = null;
      bridge.emitRevision(2);
      await tester.pumpAndSettle();
      expect(find.text('original body'), findsOneWidget);
      expect(find.textContaining('registered renderer'), findsNothing);
      await tester.pumpWidget(const SizedBox());
      expect(bridge.events.hasListener, isFalse);
      bridge.emitRevision(3);
      await tester.pumpAndSettle();
      expect(bridge.renderCalls, calls + 1);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('initial registry snapshot invalidates an in-flight render', (
    tester,
  ) async {
    final bridge = _XmlBridge();
    addTearDown(bridge.dispose);
    final initialResponse = Completer<Object?>();
    bridge.pending = initialResponse;
    await tester.pumpWidget(_surface(bridge));
    bridge.result = const <String, Object?>{
      'kind': 'text',
      'text': 'snapshot renderer',
    };
    bridge.emitRevision(0);
    await tester.pumpAndSettle();
    expect(find.textContaining('snapshot renderer'), findsWidgets);
    initialResponse.complete(null);
    await tester.pumpAndSettle();
    expect(find.textContaining('snapshot renderer'), findsWidgets);
    expect(find.text('original body'), findsNothing);
    await tester.pumpWidget(const SizedBox());
    expect(tester.takeException(), isNull);
  });

  testWidgets('obsolete render responses cannot overwrite a newer revision', (
    tester,
  ) async {
    final bridge = _XmlBridge();
    addTearDown(bridge.dispose);
    await tester.pumpWidget(_surface(bridge));
    await tester.pumpAndSettle();
    final oldResponse = Completer<Object?>();
    bridge.pending = oldResponse;
    bridge.emitRevision(1);
    await tester.pump();
    final latestResponse = Completer<Object?>();
    bridge.pending = latestResponse;
    bridge.emitRevision(2);
    await tester.pump();
    latestResponse.complete(const <String, Object?>{
      'kind': 'text',
      'text': 'latest renderer',
    });
    await tester.pumpAndSettle();
    expect(find.textContaining('latest renderer'), findsWidgets);
    oldResponse.complete(const <String, Object?>{
      'kind': 'text',
      'text': 'obsolete renderer',
    });
    await tester.pumpAndSettle();
    expect(find.textContaining('obsolete renderer'), findsNothing);
    expect(find.textContaining('latest renderer'), findsWidgets);
    await tester.pumpWidget(const SizedBox());
    expect(tester.takeException(), isNull);
  });

  testWidgets('switching chat runtime detaches the old registry watch', (
    tester,
  ) async {
    final first = _XmlBridge();
    final second = _XmlBridge();
    addTearDown(first.dispose);
    addTearDown(second.dispose);
    await tester.pumpWidget(_surface(first));
    await tester.pumpAndSettle();
    await tester.pumpWidget(_surface(second));
    await tester.pumpAndSettle();
    expect(first.events.hasListener, isFalse);
    expect(second.events.hasListener, isTrue);
    final calls = second.renderCalls;
    first.emitRevision(1);
    await tester.pumpAndSettle();
    expect(second.renderCalls, calls);
    second.result = const <String, Object?>{
      'kind': 'text',
      'text': 'second runtime',
    };
    second.emitRevision(1);
    await tester.pumpAndSettle();
    expect(find.textContaining('second runtime'), findsWidgets);
    await tester.pumpWidget(const SizedBox());
    expect(tester.takeException(), isNull);
  });
}

/// Builds unchanged XML around an explicitly selected chat runtime proxy.
Widget _surface(_XmlBridge bridge) {
  return MaterialApp(
    home: Scaffold(
      body: ChatRuntimeScope(
        chatCore: bridge.clients.chatRuntimeHolderMain,
        chatId: 'chat',
        child: CustomXmlRenderer(
          xmlContent: '<refresh_probe>original body</refresh_probe>',
          isStreaming: false,
          textColor: Colors.black,
          splitMarkdownContent: _splitText,
        ),
      ),
    ),
  );
}

/// Produces plain text events for the embedded Markdown renderer in this fixture.
Future<List<MarkdownStreamEvent>> _splitText(String content) async {
  return <MarkdownStreamEvent>[
    MarkdownStreamEvent.fromJson(<String, Object?>{
      'chatId': 'chat',
      'type': 'markdownBlockStart',
      'blockId': 1,
    }),
    MarkdownStreamEvent.fromJson(<String, Object?>{
      'chatId': 'chat',
      'type': 'markdownInlineStart',
      'blockId': 1,
      'inlineId': 1,
    }),
    MarkdownStreamEvent.fromJson(<String, Object?>{
      'chatId': 'chat',
      'type': 'markdownInlineChunk',
      'blockId': 1,
      'inlineId': 1,
      'value': content,
    }),
    MarkdownStreamEvent.fromJson(<String, Object?>{
      'chatId': 'chat',
      'type': 'completed',
    }),
  ];
}

/// Models a runtime registry stream and controllable asynchronous XML render calls.
class _XmlBridge extends OperitRuntimeBridge {
  final events = StreamController<CoreEvent>.broadcast(sync: true);
  late final clients = GeneratedCoreProxyClients(this);
  late CoreWatchRequest watch;
  int renderCalls = 0;
  Object? result;
  Completer<Object?>? pending;

  /// Encodes XML output using the production call envelope.
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    if (request.methodName != 'renderToolPkgXml') {
      throw StateError('Unexpected call: ${request.methodName}');
    }
    renderCalls++;
    final response = pending;
    pending = null;
    final value = response == null ? result : await response.future;
    return encodeCoreLink(<Object?>[0, value]);
  }

  /// Rejects unrelated push streams in this XML-only fixture.
  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();

  /// Rejects unrelated snapshot calls in this XML-only fixture.
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();

  /// Watches only committed XML registry revisions on the selected chat runtime.
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) {
    if (request.propertyName != 'xmlRenderRegistryRevisionFlow') {
      throw StateError('Unexpected watch: ${request.propertyName}');
    }
    watch = request;
    return events.stream;
  }

  /// Emits a full revision value through the real generated integer decoder.
  void emitRevision(int revision) {
    events.add(
      CoreEvent.raw(
        requestId: watch.requestId,
        target: watch.target,
        propertyName: watch.propertyName,
        kind: 'Snapshot',
        valueBytes: encodeCoreLink(revision),
        decodeValue: (bytes) => decodeCoreLink<Object?>(bytes),
      ),
    );
  }

  /// Closes the fixture stream after widgets release their subscriptions.
  Future<void> dispose() => events.close();
}
