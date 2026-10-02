import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/errors/UnhandledErrorReporter.dart';
import 'package:operit2/data/preferences/UserPreferencesManager.dart';
import 'package:operit2/ui/theme/OperitTheme.dart';

/// Verifies error dialogs preserve the application's state and navigation.
void main() {
  setUp(() {
    UnhandledErrorReporter.pendingError.value = null;
  });
  tearDown(() {
    UnhandledErrorReporter.pendingError.value = null;
  });

  testWidgets(
    'pending report uses the existing Material root without native UI',
    (tester) async {
      final nativeCalls = <MethodCall>[];
      const crashChannel = MethodChannel('operit/crash');
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        crashChannel,
        (call) async {
          nativeCalls.add(call);
          return null;
        },
      );
      addTearDown(() {
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          crashChannel,
          null,
        );
      });
      UnhandledErrorReporter.pendingError.value = UnhandledErrorReport(
        source: 'Test source',
        error: StateError('test error'),
        stackTrace: StackTrace.fromString('test stack trace'),
      );

      await tester.pumpWidget(_application());
      await tester.pumpAndSettle();

      expect(find.byType(MaterialApp), findsOneWidget);
      expect(find.byType(AlertDialog), findsOneWidget);
      expect(find.text('Operit2 encountered an error'), findsOneWidget);
      expect(find.text('Operit2 has stopped'), findsNothing);
      expect(find.textContaining('test stack trace'), findsOneWidget);
      expect(nativeCalls, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'continuing preserves page state and allows further interaction',
    (tester) async {
      await tester.pumpWidget(_application());
      await tester.tap(find.text('Increment'));
      await tester.pump();
      final originalState = tester.state(find.byType(_CounterScreen));
      expect(find.text('Count: 1'), findsOneWidget);

      _report('operation failed');
      await tester.pumpAndSettle();
      expect(find.byType(AlertDialog), findsOneWidget);
      expect(tester.state(find.byType(_CounterScreen)), same(originalState));

      await tester.tap(find.text('Continue using app'));
      await tester.pumpAndSettle();
      expect(find.byType(AlertDialog), findsNothing);
      expect(UnhandledErrorReporter.pendingError.value, isNull);
      expect(tester.state(find.byType(_CounterScreen)), same(originalState));
      expect(find.text('Count: 1'), findsOneWidget);

      await tester.tap(find.text('Increment'));
      await tester.pump();
      expect(find.text('Count: 2'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('error dialog preserves the current route and navigation stack', (
    tester,
  ) async {
    await tester.pumpWidget(_application());
    await tester.tap(find.text('Increment'));
    await tester.pump();
    await tester.tap(find.text('Open page'));
    await tester.pumpAndSettle();
    expect(find.text('Second page'), findsOneWidget);

    _report('route operation failed');
    await tester.pumpAndSettle();
    await tester.tap(find.text('Continue using app'));
    await tester.pumpAndSettle();
    expect(find.text('Second page'), findsOneWidget);

    await tester.pageBack();
    await tester.pumpAndSettle();
    expect(find.text('Count: 1'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  for (final dismissal in _Dismissal.values) {
    testWidgets('dialog can be dismissed with ${dismissal.name}', (
      tester,
    ) async {
      await tester.pumpWidget(_application());
      _report('dismissible error');
      await tester.pumpAndSettle();

      switch (dismissal) {
        case _Dismissal.barrier:
          await tester.tapAt(const Offset(4, 4));
        case _Dismissal.back:
          await tester.binding.handlePopRoute();
        case _Dismissal.escape:
          await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      }
      await tester.pumpAndSettle();

      expect(find.byType(AlertDialog), findsNothing);
      expect(UnhandledErrorReporter.pendingError.value, isNull);
      await tester.tap(find.text('Increment'));
      await tester.pump();
      expect(find.text('Count: 1'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('reports do not stack dialogs and a later error can be shown', (
    tester,
  ) async {
    await tester.pumpWidget(_application());
    _report('first error');
    _report('same-frame error');
    await tester.pumpAndSettle();
    _report('error while dialog is open');
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.textContaining('first error'), findsOneWidget);

    await tester.tap(find.text('Continue using app'));
    await tester.pumpAndSettle();
    _report('later error');
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.textContaining('later error'), findsOneWidget);
    expect(find.textContaining('first error'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'copy details keeps both the dialog and application state intact',
    (tester) async {
      final clipboardCalls = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          clipboardCalls.add(call);
          return null;
        },
      );
      addTearDown(() {
        tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        );
      });
      await tester.pumpWidget(_application());
      _report('copyable error');
      await tester.pumpAndSettle();
      final report = UnhandledErrorReporter.pendingError.value!;

      await tester.tap(find.text('Copy details'));
      await tester.pump();

      final copy = clipboardCalls.singleWhere(
        (call) => call.method == 'Clipboard.setData',
      );
      expect(copy.arguments, <String, Object>{'text': report.details});
      expect(find.byType(AlertDialog), findsOneWidget);
      expect(UnhandledErrorReporter.pendingError.value, same(report));
      expect(find.text('Count: 0'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('long diagnostics fit a compact viewport without layout errors', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(320, 280));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(_application());
    _report(List<String>.filled(200, 'A detailed diagnostic line').join('\n'));
    await tester.pumpAndSettle();

    expect(find.byType(AlertDialog), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.tap(find.text('Continue using app'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    expect(find.text('Count: 0'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('themed application retains its Material root and dialog theme', (
    tester,
  ) async {
    await tester.pumpWidget(
      const OperitTheme(
        initialThemePreferenceSnapshot:
            UserPreferencesManager.defaultThemePreferenceSnapshot,
        initialThemeMode: ThemeMode.dark,
        initialThemeIsReady: false,
        unconfiguredChildEnabled: true,
        hostInteractionHostsEnabled: false,
        child: _CounterScreen(),
      ),
    );
    await tester.tap(find.text('Increment'));
    await tester.pump();
    final originalState = tester.state(find.byType(_CounterScreen));

    _report('themed application error');
    await tester.pumpAndSettle();

    expect(find.byType(MaterialApp), findsOneWidget);
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(
      Theme.of(tester.element(find.byType(AlertDialog))).brightness,
      Brightness.dark,
    );
    await tester.tap(find.text('Continue using app'));
    await tester.pumpAndSettle();
    expect(tester.state(find.byType(_CounterScreen)), same(originalState));
    expect(find.text('Count: 1'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'startup failure displays diagnostics without a native crash UI',
    (tester) async {
      _report('startup failed');
      await tester.pumpWidget(const StartupErrorApplication());
      await tester.pumpAndSettle();

      expect(find.text('Operit2 could not start'), findsOneWidget);
      expect(find.textContaining('startup failed'), findsOneWidget);
      expect(find.text('Continue using app'), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );
}

enum _Dismissal { barrier, back, escape }

/// Creates a minimal application with a persistent navigator and error host.
Widget _application() {
  return const MaterialApp(home: UnhandledErrorHost(child: _CounterScreen()));
}

/// Reports one error through the same deferred path used by runtime hooks.
void _report(String message) {
  UnhandledErrorReporter.report(
    source: 'Test source',
    error: StateError(message),
    stackTrace: StackTrace.fromString('test stack trace'),
  );
}

/// Provides observable state and navigation for error dialog regression tests.
class _CounterScreen extends StatefulWidget {
  /// Creates a page whose state must survive error reporting.
  const _CounterScreen();

  /// Creates the counter used to detect application tree replacement.
  @override
  State<_CounterScreen> createState() => _CounterScreenState();
}

class _CounterScreenState extends State<_CounterScreen> {
  int _count = 0;

  /// Builds interactive controls and a route that must survive error reporting.
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Column(
        children: <Widget>[
          Text('Count: $_count'),
          TextButton(
            onPressed: () => setState(() => _count++),
            child: const Text('Increment'),
          ),
          TextButton(
            onPressed: () => Navigator.of(context).push<void>(
              MaterialPageRoute<void>(
                builder: (context) => Scaffold(
                  appBar: AppBar(title: const Text('Second page')),
                  body: const SizedBox.expand(),
                ),
              ),
            ),
            child: const Text('Open page'),
          ),
        ],
      ),
    );
  }
}
