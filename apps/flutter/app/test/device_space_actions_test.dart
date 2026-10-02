import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/common/DeviceSpaceDiscoveryPanel.dart';
import 'space_join_dialog_test.dart' show JoinBridge, joinRequest;

class DeviceBridge extends JoinBridge {
  Map<String, Object?>? config;
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    switch (request.methodName) {
      case 'discoverPeers':
        calls.add(request.methodName);
        return encodeCoreLink([
          0,
          [
            {
              'nodeId': 'ios',
              'displayName': '我的 iPhone',
              'address': 'http://192.168.1.2:37195',
            },
          ],
        ]);
      case 'localHostConfig':
        return encodeCoreLink([0, config]);
      case 'saveLocalHostConfig':
        config = Map<String, Object?>.from(
          (request.args as Map)['config'] as Map,
        );
        return encodeCoreLink([0, null]);
      case 'stopListening':
      case 'startListening':
        return encodeCoreLink([0, null]);
      default:
        return super.callBytes(request);
    }
  }
}

void main() {
  Future<void> mount(WidgetTester tester, DeviceBridge bridge) async {
    await tester.pumpWidget(
      MaterialApp(
        locale: const Locale('zh'),
        supportedLocales: AppLocalizations.supportedLocales,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(
          body: Padding(
            padding: const EdgeInsets.all(16),
            child: Row(
              children: [
                const Expanded(child: Text('设备')),
                DeviceSpaceDiscoveryPanel(
                  clients: GeneratedCoreProxyClients(bridge),
                  onJoined: (_) async {},
                ),
              ],
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  Future<void> dispose(WidgetTester tester, DeviceBridge bridge) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpAndSettle();
    await bridge.prompts.close();
  }

  testWidgets(
    'landing page has one add action, no stacked discovery/requests/settings sections',
    (tester) async {
      final bridge = DeviceBridge();
      await mount(tester, bridge);
      expect(find.text('添加设备'), findsOneWidget);
      expect(find.text('空间加入申请'), findsNothing);
      expect(find.text('连接设置'), findsNothing);
      expect(find.byType(ExpansionTile), findsNothing);
      expect(find.byType(Switch), findsNothing);
      expect(find.byType(FilterChip), findsNothing);
      expect(
        bridge.calls,
        isNot(contains('discoverPeers')),
      ); // Discovery is intentional, not a page-load side effect.
      await dispose(tester, bridge);
    },
  );
  testWidgets('add device opens focused nearby picker, not advanced settings', (
    tester,
  ) async {
    final bridge = DeviceBridge();
    await mount(tester, bridge);
    await tester.tap(find.text('添加设备'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.text('附近设备'), findsOneWidget);
    expect(find.text('我的 iPhone'), findsOneWidget);
    expect(find.text('通过地址连接'), findsOneWidget);
    expect(find.byType(Switch), findsNothing);
    expect(find.byType(FilterChip), findsNothing);
    expect(bridge.calls.where((c) => c == 'discoverPeers').length, 1);
    await dispose(tester, bridge);
  });
  testWidgets(
    'connection settings are secondary, discoverable switch remains accessible',
    (tester) async {
      final bridge = DeviceBridge();
      await mount(tester, bridge);
      await tester.tap(find.byTooltip('更多设备操作'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('连接设置'));
      await tester.pumpAndSettle();
      expect(find.byType(AlertDialog), findsOneWidget);
      expect(find.byType(Switch), findsOneWidget);
      expect(find.byType(FilterChip), findsNothing);
      await tester.tap(find.byType(Switch));
      await tester.pumpAndSettle();
      expect(bridge.config!['discoveryEnabled'], true);
      await tester.tap(find.text('高级选项'));
      await tester.pumpAndSettle();
      expect(find.byType(FilterChip), findsNWidgets(4));
      await dispose(tester, bridge);
    },
  );
  testWidgets(
    'requests are a dialog with applicant/reviewer tabs, never a landing-page expansion',
    (tester) async {
      final bridge = DeviceBridge()..outgoing = [joinRequest()];
      await mount(tester, bridge);
      expect(find.text('等待批准'), findsNothing);
      await tester.tap(find.byTooltip('更多设备操作'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('空间加入申请'));
      await tester.pumpAndSettle();
      expect(find.byType(TabBar), findsOneWidget);
      expect(find.text('我发出的'), findsOneWidget);
      expect(find.text('待我审批'), findsOneWidget);
      expect(find.text('审批人：我的 iPhone\n等待批准'), findsOneWidget);
      expect(find.byType(Switch), findsNothing);
      expect(find.byType(ExpansionTile), findsNothing);
      await dispose(tester, bridge);
    },
  );
  testWidgets('compact toolbar and picker fit a small phone without overflow', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 650);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final bridge = DeviceBridge();
    await mount(tester, bridge);
    expect(tester.takeException(), isNull);
    await tester.tap(find.text('添加设备'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await dispose(tester, bridge);
  });
}
