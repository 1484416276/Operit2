import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/features/settings/runtime/PeerListenerSettings.dart';

class TestBridge extends OperitRuntimeBridge {
  Map<String, Object?>? config;
  final calls = <String>[];
  bool failStart = false;
  String? selectedAddress;
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    calls.add(request.methodName);
    switch (request.methodName) {
      case 'localHostConfig':
        return encodeCoreLink([0, config]);
      case 'saveLocalHostConfig':
        config = Map<String, Object?>.from(
          (request.args as Map)['config'] as Map,
        );
      case 'startListening':
        if (failStart) throw StateError('unsupported transport');
        if (selectedAddress != null) config!['bindAddress'] = selectedAddress;
      case 'stopListening':
        break;
      default:
        throw StateError(request.methodName);
    }
    return encodeCoreLink([0, null]);
  }

  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw UnimplementedError();
}

void main() {
  Future<void> mount(WidgetTester tester, TestBridge bridge) async {
    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        locale: const Locale('en'),
        home: Scaffold(
          body: SingleChildScrollView(
            child: PeerListenerSettings(
              clients: GeneratedCoreProxyClients(bridge),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets('advanced options collapsed; discovery saves and starts HTTP', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    expect(find.byType(FilterChip), findsNothing);
    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(bridge.config!['discoveryEnabled'], true);
    expect(bridge.config!['transports'], ['http']);
    expect(
      bridge.calls,
      containsAllInOrder([
        'stopListening',
        'saveLocalHostConfig',
        'startListening',
      ]),
    );
  });
  testWidgets('HTTP and WS multi-select saves both; TCP conflict is explicit', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilterChip, 'WebSocket'));
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('Save'));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(bridge.config!['transports'], ['http', 'webSocket']);
    await tester.tap(find.widgetWithText(FilterChip, 'TCP'));
    await tester.pumpAndSettle();
    final count = bridge.calls.length;
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(bridge.calls.length, count);
    expect(find.textContaining('TCP cannot currently'), findsOneWidget);
  });
  testWidgets('automatic mode reloads runtime-selected port after save', (tester) async {
    final bridge = TestBridge()..selectedAddress = '0.0.0.0:37196';
    await mount(tester, bridge);
    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(bridge.config!['portMode'], 'automatic');
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    expect(find.text('0.0.0.0:37196'), findsOneWidget);
    await tester.ensureVisible(find.text('Save'));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(bridge.config!['bindAddress'], '0.0.0.0:37196');
  });
  testWidgets('failed first listener start rolls back to disabled config', (
    tester,
  ) async {
    final bridge = TestBridge()..failStart = true;
    await mount(tester, bridge);
    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(bridge.config!['transports'], isEmpty);
    expect(bridge.config!['discoveryEnabled'], false);
    expect(find.textContaining('unsupported transport'), findsOneWidget);
  });
}
