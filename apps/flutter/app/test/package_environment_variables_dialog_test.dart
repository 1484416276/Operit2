import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/core/proxy/generated/CoreProxyModels.g.dart'
    as core_proxy;
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/features/packages/dialogs/PackageEnvironmentVariablesDialog.dart';

/// Verifies the Kotlin environment editor behavior through the runtime proxy.
void main() {
  testWidgets(
    'groups enabled packages and shares controllers by variable name',
    (tester) async {
      final bridge = _EnvironmentBridge(
        packages: <core_proxy.ToolPackage>[
          _package('z_plugin', <core_proxy.EnvVar>[_variable('SHARED')]),
          _package('disabled', <core_proxy.EnvVar>[_variable('DISABLED')]),
          _package('a_subpackage', <core_proxy.EnvVar>[
            _variable(
              'SHARED',
              required: true,
              defaultValue: 'manifest-default',
            ),
          ]),
        ],
        enabled: <String>['z_plugin', 'a_subpackage'],
        effective: <String, String>{'SHARED': 'runtime-value'},
      );
      await _openEditor(tester, bridge);

      expect(find.text('disabled'), findsNothing);
      expect(find.text('必填'), findsOneWidget);
      expect(find.text('可选'), findsOneWidget);
      expect(find.text('默认: manifest-default'), findsOneWidget);
      expect(find.text('SHARED description'), findsNWidgets(2));
      expect(bridge.readKeys, <String>['SHARED']);
      expect(
        tester.getTopLeft(find.text('a_subpackage')).dy,
        lessThan(tester.getTopLeft(find.text('z_plugin')).dy),
      );
      final fields = tester
          .widgetList<TextField>(find.byType(TextField))
          .toList();
      expect(fields[0].controller, same(fields[1].controller));
      expect(fields[0].controller!.text, 'runtime-value');
      await tester.enterText(_field('a_subpackage', 'SHARED'), 'updated-value');
      expect(fields[1].controller!.text, 'updated-value');
      await tester.tap(find.text('保存'));
      await tester.pumpAndSettle();
      expect(bridge.stored, <String, String>{'SHARED': 'updated-value'});
      expect(find.byType(PackageEnvironmentVariablesDialog), findsNothing);
    },
  );

  testWidgets('merges edits, deletes blank keys, and never applies defaults', (
    tester,
  ) async {
    final bridge = _EnvironmentBridge(
      packages: <core_proxy.ToolPackage>[
        _package('demo', <core_proxy.EnvVar>[
          _variable('TOKEN', required: true),
          _variable('URL', defaultValue: 'https://manifest.example'),
        ]),
      ],
      enabled: <String>['demo'],
      effective: <String, String>{'TOKEN': 'old-token'},
      stored: <String, String>{'TOKEN': 'old-token', 'UNRELATED': 'keep'},
    );
    await _openEditor(tester, bridge);
    expect(
      tester.widget<TextField>(_field('demo', 'URL')).controller!.text,
      '',
    );
    bridge.stored['CONCURRENT'] = 'keep-new-value';
    await tester.enterText(_field('demo', 'TOKEN'), '   ');
    await tester.enterText(_field('demo', 'URL'), '  custom-value  ');
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();
    expect(bridge.stored, <String, String>{
      'UNRELATED': 'keep',
      'CONCURRENT': 'keep-new-value',
      'URL': '  custom-value  ',
    });
    expect(bridge.writes, 1);
  });

  testWidgets('cancel discards edits without writing preferences', (
    tester,
  ) async {
    final bridge = _EnvironmentBridge(
      packages: <core_proxy.ToolPackage>[
        _package('demo', <core_proxy.EnvVar>[_variable('TOKEN')]),
      ],
      enabled: <String>['demo'],
      effective: <String, String>{'TOKEN': 'old-token'},
      stored: <String, String>{'TOKEN': 'old-token'},
    );
    await _openEditor(tester, bridge);
    await tester.enterText(_field('demo', 'TOKEN'), 'unsaved-token');
    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(bridge.stored, <String, String>{'TOKEN': 'old-token'});
    expect(bridge.writes, 0);
    expect(find.byType(PackageEnvironmentVariablesDialog), findsNothing);
  });

  testWidgets('shows an empty-state message without reading undeclared keys', (
    tester,
  ) async {
    final bridge = _EnvironmentBridge(
      packages: <core_proxy.ToolPackage>[
        _package('disabled', <core_proxy.EnvVar>[_variable('HIDDEN')]),
        _package('enabled', <core_proxy.EnvVar>[]),
      ],
      enabled: <String>['enabled'],
    );
    await _openEditor(tester, bridge);
    expect(find.text('当前已启用的工具包没有声明需要的环境变量。'), findsOneWidget);
    expect(find.byType(TextField), findsNothing);
    expect(bridge.readKeys, isEmpty);
  });

  testWidgets('variable lists scroll on a narrow viewport without overflow', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 480);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final bridge = _EnvironmentBridge(
      packages: <core_proxy.ToolPackage>[
        _package('demo', <core_proxy.EnvVar>[
          for (var index = 0; index < 12; index++) _variable('VAR_$index'),
        ]),
      ],
      enabled: <String>['demo'],
    );
    await _openEditor(tester, bridge);
    await tester.ensureVisible(_field('demo', 'VAR_11'));
    await tester.pumpAndSettle();
    await tester.enterText(_field('demo', 'VAR_11'), 'last-value');
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();
    expect(bridge.stored, <String, String>{'VAR_11': 'last-value'});
    expect(tester.takeException(), isNull);
  });

  testWidgets('load errors remain visible and disable saving', (tester) async {
    final bridge = _EnvironmentBridge(loadError: 'environment-load-failed');
    await _openEditor(tester, bridge);
    expect(find.textContaining('environment-load-failed'), findsOneWidget);
    expect(
      tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
      isNull,
    );
    expect(bridge.writes, 0);
    expect(find.byType(PackageEnvironmentVariablesDialog), findsOneWidget);
  });

  testWidgets('save errors keep edited values open for correction', (
    tester,
  ) async {
    final bridge = _EnvironmentBridge(
      packages: <core_proxy.ToolPackage>[
        _package('demo', <core_proxy.EnvVar>[_variable('TOKEN')]),
      ],
      enabled: <String>['demo'],
      saveError: 'environment-save-failed',
    );
    await _openEditor(tester, bridge);
    await tester.enterText(_field('demo', 'TOKEN'), 'edited-token');
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();
    expect(find.textContaining('environment-save-failed'), findsOneWidget);
    expect(
      tester.widget<TextField>(_field('demo', 'TOKEN')).controller!.text,
      'edited-token',
    );
    expect(bridge.stored, isEmpty);
    expect(find.byType(PackageEnvironmentVariablesDialog), findsOneWidget);
    expect(
      tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
      isNotNull,
    );
  });
}

/// Mounts the editor on a localized route and waits for runtime data.
Future<void> _openEditor(WidgetTester tester, _EnvironmentBridge bridge) async {
  await tester.pumpWidget(
    MaterialApp(
      locale: const Locale('zh'),
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Builder(
        builder: (context) => Scaffold(
          body: TextButton(
            onPressed: () => PackageEnvironmentVariablesDialog.show(
              context: context,
              clients: GeneratedCoreProxyClients(bridge),
            ),
            child: const Text('open-editor'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.text('open-editor'));
  await tester.pumpAndSettle();
  expect(tester.takeException(), isNull);
}

/// Locates one package-specific field for a shared environment key.
Finder _field(String packageName, String name) =>
    find.byKey(ValueKey<String>('package-env:$packageName:$name'));

/// Creates a package manifest fixture with explicit environment declarations.
core_proxy.ToolPackage _package(String name, List<core_proxy.EnvVar> env) {
  return core_proxy.ToolPackage(
    name: name,
    description: const core_proxy.LocalizedText(values: <String, String>{}),
    displayName: const core_proxy.LocalizedText(values: <String, String>{}),
    tools: const <core_proxy.PackageTool>[],
    states: const <core_proxy.ToolPackageState>[],
    env: env,
    isBuiltIn: false,
    enabledByDefault: false,
    category: 'Other',
    author: const <String>[],
  );
}

/// Creates an environment declaration without supplying an effective value.
core_proxy.EnvVar _variable(
  String name, {
  bool required = false,
  String? defaultValue,
}) {
  return core_proxy.EnvVar(
    name: name,
    description: core_proxy.LocalizedText(
      values: <String, String>{'zh': '$name description'},
    ),
    requiredValue: required,
    defaultValue: defaultValue,
  );
}

class _EnvironmentBridge extends OperitRuntimeBridge {
  /// Initializes an isolated runtime preference fixture.
  _EnvironmentBridge({
    this.packages = const <core_proxy.ToolPackage>[],
    this.enabled = const <String>[],
    this.effective = const <String, String>{},
    Map<String, String> stored = const <String, String>{},
    this.loadError,
    this.saveError,
  }) : stored = Map<String, String>.of(stored);

  final List<core_proxy.ToolPackage> packages;
  final List<String> enabled;
  final Map<String, String> effective;
  final Map<String, String> stored;
  final String? loadError;
  final String? saveError;
  final List<String> readKeys = <String>[];
  int writes = 0;

  /// Encodes preference responses using the runtime bridge envelope.
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async =>
      encodeCoreLink(<Object?>[0, await call(request)]);

  /// Implements only the package catalog and environment preference methods.
  @override
  Future<Object?> call(CoreCallRequest request) async {
    switch ((request.target, request.methodName)) {
      case ('core/application.packageManager', 'getAvailablePackages'):
        if (loadError != null) {
          throw StateError(loadError!);
        }
        return <String, Object?>{
          for (final package in packages) package.name: package.toJson(),
        };
      case ('core/application.packageManager', 'getEnabledPackageNames'):
        return enabled;
      case ('core/preferences.envPreferences', 'getEnv'):
        final key = (request.args as Map<String, Object?>)['key'] as String;
        readKeys.add(key);
        return effective[key];
      case ('core/preferences.envPreferences', 'getAllEnv'):
        return Map<String, String>.of(stored);
      case ('core/preferences.envPreferences', 'setAllEnv'):
        if (saveError != null) {
          throw StateError(saveError!);
        }
        writes++;
        stored
          ..clear()
          ..addAll(
            ((request.args as Map<String, Object?>)['variables'] as Map)
                .cast<String, String>(),
          );
        return null;
    }
    throw StateError(
      'Unexpected environment call: ${request.target}.${request.methodName}',
    );
  }

  /// Rejects push operations outside the environment preference fixture.
  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();

  /// Rejects snapshot watches outside the environment preference fixture.
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();

  /// Rejects stream watches outside the environment preference fixture.
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw UnimplementedError();
}
