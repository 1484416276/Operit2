import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/features/settings/runtime/PeerListenerSettings.dart';

class TestBridge extends OperitRuntimeBridge {
  Map<String, Object?>? config = {
    'bindAddress': '0.0.0.0:37195',
    'token': 'runtime-owned-token',
    'transports': ['http', 'webSocket'],
    'discoveryEnabled': true,
    'portMode': 'fixed',
    'updatedAt': 1,
  };
  final calls = <String>[];
  bool failStart = false;
  String? selectedAddress;
  bool failLoad = false;
  bool failSave = false;
  final writes = <Map<String, Object?>>[];

  /// Handles configuration persistence and explicit listener lifecycle calls.
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    calls.add(request.methodName);
    switch (request.methodName) {
      case 'refreshLocalPairingToken':
        if (failSave) throw StateError('Cannot save listener configuration');
        config!['token'] = 'runtime-rotated-token-${calls.length}';
        writes.add(Map<String, Object?>.from(config!));
        return encodeCoreLink([0, config!['token']]);
      case 'localHostConfig':
        if (failLoad) throw StateError('Cannot read listener configuration');
        return encodeCoreLink([0, config]);
      case 'saveLocalHostConfig':
        if (failSave) throw StateError('Cannot save listener configuration');
        config = Map<String, Object?>.from(
          (request.args as Map)['config'] as Map,
        );
        writes.add(Map<String, Object?>.from(config!));
        return encodeCoreLink([0, null]);
      case 'startListening':
        if (failStart) throw StateError('unsupported transport');
        if (config!['portMode'] == 'automatic' && selectedAddress != null) {
          config!['bindAddress'] = selectedAddress;
        }
      case 'stopListening':
        break;
      default:
        throw StateError(request.methodName);
    }
    return encodeCoreLink([0, null]);
  }

  /// Rejects push streams that these settings never open.
  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();

  /// Rejects snapshots that these settings never request.
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();

  /// Rejects event watches that these settings never create.
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw UnimplementedError();
}

/// Verifies listener defaults, credentials, and explicit network binding.
void main() {
  /// Mounts the localized connection settings panel.
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

  /// Submits the form after dismissing any transient success notification.
  Future<void> save(WidgetTester tester) async {
    ScaffoldMessenger.of(
      tester.element(find.byType(PeerListenerSettings)),
    ).removeCurrentSnackBar();
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('Save'));
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
  }

  testWidgets('defaults enable discovery with HTTP and WebSocket', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    expect(find.byType(FilterChip), findsNothing);
    expect(bridge.calls, ['localHostConfig']);
    expect(bridge.writes, isEmpty);
    expect(tester.widget<Switch>(find.byType(Switch)).value, true);
    expect(find.text('runtime-owned-token'), findsOneWidget);
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<FilterChip>(find.widgetWithText(FilterChip, 'HTTP'))
          .selected,
      true,
    );
    expect(
      tester
          .widget<FilterChip>(find.widgetWithText(FilterChip, 'WebSocket'))
          .selected,
      true,
    );
    expect(find.text('Fixed address and port'), findsOneWidget);
  });
  testWidgets('token refresh updates the remote connection credential', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    await tester.tap(find.byTooltip('Refresh token'));
    await tester.pumpAndSettle();
    final original = bridge.config!['token'];
    await tester.tap(find.byTooltip('Refresh token'));
    await tester.pumpAndSettle();
    expect(bridge.config!['token'], isNot(original));
    expect((bridge.config!['token'] as String).length, greaterThan(20));
  });
  testWidgets('HTTP and WS multi-select saves both; TCP conflict is explicit', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    await save(tester);
    expect(bridge.config!['transports'], ['http', 'webSocket']);
    await tester.tap(find.widgetWithText(FilterChip, 'TCP'));
    await tester.pumpAndSettle();
    final count = bridge.calls.length;
    await save(tester);
    expect(bridge.calls.length, count);
    expect(find.textContaining('TCP cannot currently'), findsOneWidget);
  });
  testWidgets('fixed mode keeps the configured address and port', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    await save(tester);
    expect(bridge.config!['portMode'], 'fixed');
    expect(bridge.config!['bindAddress'], '0.0.0.0:37195');
  });
  testWidgets('automatic mode reloads runtime-selected port after save', (
    tester,
  ) async {
    final bridge = TestBridge()..selectedAddress = '0.0.0.0:37196';
    await mount(tester, bridge);
    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(bridge.config!['portMode'], 'fixed');
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Fixed address and port'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Automatic port'));
    await tester.pumpAndSettle();
    expect(find.text('0.0.0.0:37195'), findsOneWidget);
    await save(tester);
    expect(bridge.config!['bindAddress'], '0.0.0.0:37196');
    expect(bridge.config!['portMode'], 'automatic');
  });
  testWidgets('listener failures report the cause without rewriting config', (
    tester,
  ) async {
    final bridge = TestBridge()..failStart = true;
    await mount(tester, bridge);
    final original = bridge.config!['token'];
    final writesBefore = bridge.writes.length;
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    await save(tester);
    expect(bridge.config!['transports'], ['http', 'webSocket']);
    expect(bridge.config!['discoveryEnabled'], true);
    expect(bridge.config!['portMode'], 'fixed');
    expect(bridge.config!['token'], original);
    expect(bridge.writes.length, writesBefore + 1);
    expect(bridge.calls.where((call) => call == 'startListening').length, 1);
    expect(find.textContaining('unsupported transport'), findsOneWidget);
    expect(find.text('Listener settings applied.'), findsNothing);
  });
  testWidgets('saved fixed configuration and credentials are preserved', (
    tester,
  ) async {
    final bridge = TestBridge()
      ..config = {
        'bindAddress': '127.0.0.1:48123',
        'token': 'existing-remote-token',
        'transports': ['http', 'webSocket'],
        'discoveryEnabled': false,
        'portMode': 'fixed',
        'updatedAt': 1,
      };
    await mount(tester, bridge);
    expect(bridge.writes, isEmpty);
    expect(find.text('existing-remote-token'), findsOneWidget);
    expect(tester.widget<Switch>(find.byType(Switch)).value, false);
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    expect(find.text('127.0.0.1:48123'), findsOneWidget);
    await save(tester);
    expect(bridge.config!['bindAddress'], '127.0.0.1:48123');
    expect(bridge.config!['token'], 'existing-remote-token');
    expect(bridge.config!['portMode'], 'fixed');
    expect(bridge.config!['discoveryEnabled'], false);
  });
  testWidgets('copy token uses the credential stored by the listener', (
    tester,
  ) async {
    final bridge = TestBridge();
    String? copied;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SystemChannels.platform, (call) async {
          if (call.method == 'Clipboard.setData') {
            copied = (call.arguments as Map)['text'] as String;
          }
          return null;
        });
    addTearDown(() {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, null);
    });
    await mount(tester, bridge);
    await tester.tap(find.byTooltip('Copy token'));
    await tester.pumpAndSettle();
    expect(copied, bridge.config!['token']);
    expect(find.text('Connection token copied.'), findsOneWidget);
  });
  testWidgets('failed token save keeps the displayed credential unchanged', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    final original = bridge.config!['token'] as String;
    bridge.failSave = true;
    await tester.tap(find.byTooltip('Refresh token'));
    await tester.pumpAndSettle();
    expect(bridge.config!['token'], original);
    expect(find.text(original), findsOneWidget);
    expect(
      find.textContaining('Cannot save listener configuration'),
      findsOneWidget,
    );
  });
  testWidgets(
    'configuration read errors never initialize replacement defaults',
    (tester) async {
      final bridge = TestBridge()..failLoad = true;
      await mount(tester, bridge);
      expect(bridge.writes, isEmpty);
      expect(
        find.textContaining('Cannot read listener configuration'),
        findsOneWidget,
      );
    },
  );
  testWidgets(
    'missing runtime configuration is an error, not client initialization',
    (tester) async {
      final bridge = TestBridge()..config = null;
      await mount(tester, bridge);
      expect(bridge.calls, ['localHostConfig']);
      expect(bridge.writes, isEmpty);
      expect(
        find.textContaining(
          'Runtime listener preferences were not initialized',
        ),
        findsOneWidget,
      );
    },
  );
  testWidgets('token rotation does not save pending listener form changes', (
    tester,
  ) async {
    final bridge = TestBridge();
    await mount(tester, bridge);
    await tester.tap(find.text('Advanced options'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilterChip, 'WebSocket'));
    await tester.tap(find.byTooltip('Refresh token'));
    await tester.pumpAndSettle();
    expect(bridge.config!['transports'], ['http', 'webSocket']);
    expect(bridge.calls, ['localHostConfig', 'refreshLocalPairingToken']);
    expect(
      tester
          .widget<FilterChip>(find.widgetWithText(FilterChip, 'WebSocket'))
          .selected,
      false,
    );
  });
}
