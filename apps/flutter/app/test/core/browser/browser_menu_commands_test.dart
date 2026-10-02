import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/browser/BrowserSessions.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/core/proxy/generated/CoreProxyModels.g.dart';
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/features/chat/components/workspace/browser/chrome/WorkspaceBrowserMenuSheet.dart';
import 'package:operit2/ui/features/chat/components/workspace/browser/userscripts/WorkspaceUserscriptModels.dart';
import 'package:operit2/ui/features/chat/components/workspace/browser/userscripts/WorkspaceUserscriptSheet.dart';
import 'package:operit2/ui/features/chat/components/workspace/browser/userscripts/WorkspaceUserscriptStore.dart';

const _hostError =
    'Browser session does not expose page JavaScript to the runtime host';
const _failedResult = RuntimeBrowserCommandResult(
  success: false,
  session: null,
  sessions: <RuntimeBrowserSessionInfo>[],
  resultJson: '',
  error: _hostError,
);
const _command = WorkspaceUserscriptMenuCommand(
  index: 0,
  scriptName: 'Test script',
  caption: 'Test menu action',
);

/// Verifies browser command failures remain distinct from empty page menus.
void main() {
  test(
    'evaluate reports the host error before its payload can be decoded',
    () async {
      final bridge = _BrowserCommandBridge(_failedResult);
      final sessions = BrowserSessions(
        clients: GeneratedCoreProxyClients(bridge),
      );

      await expectLater(
        sessions.evaluate('session-1', 'JSON.stringify([])'),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            'Browser command "evaluate" failed (session: session-1): $_hostError',
          ),
        ),
      );
      expect(bridge.request?.methodName, 'submitBrowserCommand');
    },
  );

  test('failed reloads use the same command failure contract', () async {
    final sessions = BrowserSessions(
      clients: GeneratedCoreProxyClients(_BrowserCommandBridge(_failedResult)),
    );

    await expectLater(
      sessions.reload('session-1'),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          'Browser command "reload" failed (session: session-1): $_hostError',
        ),
      ),
    );
  });

  test(
    'successful evaluation preserves the encoded JavaScript result',
    () async {
      final result = RuntimeBrowserCommandResult(
        success: true,
        session: null,
        sessions: const <RuntimeBrowserSessionInfo>[],
        resultJson: jsonEncode('[]'),
        error: null,
      );
      final sessions = BrowserSessions(
        clients: GeneratedCoreProxyClients(_BrowserCommandBridge(result)),
      );

      final response = await sessions.evaluate(
        'session-1',
        'JSON.stringify([])',
      );

      expect(response.success, isTrue);
      expect(response.resultJson, result.resultJson);
      expect(jsonDecode(jsonDecode(response.resultJson) as String), isEmpty);
    },
  );

  testWidgets(
    'initial menu failure displays the host error without a Zone error',
    (tester) async {
      await _pumpUserscriptSheet(
        tester,
        () => Future<List<WorkspaceUserscriptMenuCommand>>.error(
          StateError(_hostError),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('加载脚本菜单失败：Bad state: $_hostError'), findsOneWidget);
      expect(find.text('当前页面没有脚本菜单'), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('successful empty menus display the empty state', (tester) async {
    await _pumpUserscriptSheet(
      tester,
      () async => const <WorkspaceUserscriptMenuCommand>[],
    );
    await tester.pumpAndSettle();

    expect(find.text('当前页面没有脚本菜单'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'refresh failures replace existing commands with an error state',
    (tester) async {
      var queryCount = 0;
      final refreshed = Completer<List<WorkspaceUserscriptMenuCommand>>();
      await _pumpUserscriptSheet(tester, () {
        queryCount += 1;
        if (queryCount == 1) {
          return Future<List<WorkspaceUserscriptMenuCommand>>.value([_command]);
        }
        return refreshed.future;
      });
      await tester.pumpAndSettle();
      expect(find.text(_command.caption), findsOneWidget);

      await tester.tap(find.byTooltip('刷新菜单'));
      await tester.pump();
      expect(queryCount, 2);
      expect(find.byType(LinearProgressIndicator), findsOneWidget);
      expect(find.text(_command.caption), findsNothing);

      refreshed.completeError(StateError(_hostError));
      await tester.pumpAndSettle();
      expect(find.text('加载脚本菜单失败：Bad state: $_hostError'), findsOneWidget);
      expect(find.text(_command.caption), findsNothing);
      expect(find.text('当前页面没有脚本菜单'), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('a menu failure after disposal does not escape to the Zone', (
    tester,
  ) async {
    final query = Completer<List<WorkspaceUserscriptMenuCommand>>();
    await _pumpUserscriptSheet(tester, () => query.future);
    await tester.pumpWidget(const SizedBox.shrink());

    query.completeError(StateError(_hostError));
    await tester.pump();

    expect(tester.takeException(), isNull);
  });

  testWidgets('browser action menu displays failures instead of hiding them', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: SingleChildScrollView(
            child: WorkspaceBrowserMenuSheet(
              onHistory: () {},
              onBookmarks: () {},
              onDownloads: () {},
              onUserscripts: () {},
              onPermissions: () {},
              onClearStorage: () {},
              zoomLabel: '100%',
              onZoomOut: () {},
              zoomButtonKey: GlobalKey(),
              onZoomMenuRequested: () {},
              onZoomIn: () {},
              desktopMode: false,
              onDesktopModeChanged: (_) {},
              onLoadMenuCommands: () =>
                  Future<List<WorkspaceUserscriptMenuCommand>>.error(
                    StateError(_hostError),
                  ),
              onRunMenuCommand: (_) {},
              activeDownloadCount: 0,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('加载脚本菜单失败'), findsOneWidget);
    expect(find.text('Bad state: $_hostError'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

/// Mounts a userscript manager without accessing persistent runtime storage.
Future<void> _pumpUserscriptSheet(
  WidgetTester tester,
  Future<List<WorkspaceUserscriptMenuCommand>> Function() loadMenuCommands,
) {
  final clients = GeneratedCoreProxyClients(
    _BrowserCommandBridge(_failedResult),
  );
  return tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: WorkspaceUserscriptSheet(
          store: WorkspaceUserscriptStore(
            runtimeStorage: clients.repositoryRuntimeStorageRepository,
            storagePath: () async => '/userscripts.json',
          ),
          onChanged: () {},
          onReadWorkspaceTextFile: (_) async => '',
          onLoadMenuCommands: loadMenuCommands,
          onRunMenuCommand: (_) async {},
        ),
      ),
    ),
  );
}

/// Provides a typed Core command result for browser client regression tests.
class _BrowserCommandBridge extends OperitRuntimeBridge {
  /// Creates a bridge returning the supplied browser command result.
  _BrowserCommandBridge(this.result);

  final RuntimeBrowserCommandResult result;
  CoreCallRequest? request;

  /// Encodes one browser command response using the real Core envelope.
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    expect(request.methodName, 'submitBrowserCommand');
    this.request = request;
    return encodeCoreLink(<Object?>[0, result.toJson()]);
  }

  /// Rejects push streams because these tests only submit browser commands.
  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();

  /// Rejects snapshots because these tests do not observe browser sessions.
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();

  /// Rejects watch streams because these tests do not observe browser sessions.
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw UnimplementedError();
}
