// ignore_for_file: file_names

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../core/application/CoreApplicationService.dart';
import '../../core/bridge/ProxyCoreRuntimeBridge.dart';
import '../../core/host/ComposeWebViewControllerBridge.dart';
import '../../core/logging/ClientLogger.dart';
import '../../core/proxy/generated/CoreProxyClients.g.dart';
import '../../core/proxy/generated/CoreProxyModels.g.dart' as core_proxy;
import '../../core/runtime/RuntimeBootstrapManager.dart';
import '../../data/preferences/UserPreferencesManager.dart';
import '../../l10n/generated/app_localizations.dart';
import '../features/packages/screens/ToolPkgComposeDslWebView.dart';
import '../theme/OperitTheme.dart';
import 'navigation/AppStartupRouteCatalog.dart';

class OperitApp extends StatefulWidget {
  const OperitApp({super.key});

  /// Creates the main application bootstrap state.
  @override
  State<OperitApp> createState() => _OperitAppState();
}

class _OperitAppState extends State<OperitApp> {
  final RuntimeBootstrapManager _runtimeManager =
      RuntimeBootstrapManager.instance;
  StreamSubscription<Object>? _startupErrorSubscription;
  void Function()? _unregisterComposeWebViewController;
  String? _startupWebAccessError;
  bool _lastRuntimeConfigured = false;
  int _startupRouteEpoch = 0;

  /// Subscribes to runtime state and process-level startup errors.
  @override
  void initState() {
    super.initState();
    _lastRuntimeConfigured = _runtimeManager.runtimeConfigured;
    _runtimeManager.addListener(_handleRuntimeBootstrapChanged);
    _startupErrorSubscription = CoreApplicationService.instance.startupErrors
        .listen(_handleStartupError);
    final pendingStartupError = CoreApplicationService.instance
        .consumeStartupError();
    if (pendingStartupError != null) {
      _startupWebAccessError = pendingStartupError.toString();
    }
    _unregisterComposeWebViewController = const ComposeWebViewControllerBridge()
        .registerHandler(ComposeDslWebViewHostRegistry.handleControllerCommand);
  }

  /// Releases UI-only runtime and error listeners.
  @override
  void dispose() {
    _unregisterComposeWebViewController?.call();
    unawaited(_startupErrorSubscription?.cancel());
    _runtimeManager.removeListener(_handleRuntimeBootstrapChanged);
    super.dispose();
  }

  /// Reacts to runtime configuration and preserves onboarding state on startup.
  void _handleRuntimeBootstrapChanged() {
    final runtimeConfigured = _runtimeManager.runtimeConfigured;
    if (_lastRuntimeConfigured && !runtimeConfigured) {
      _startupRouteEpoch++;
    }
    _lastRuntimeConfigured = runtimeConfigured;
    if (mounted) {
      setState(() {});
    }
  }

  /// Presents a process-level Core startup error through the app dialog host.
  void _handleStartupError(Object error) {
    if (!mounted) {
      return;
    }
    setState(() {
      _startupWebAccessError = error.toString();
    });
  }

  /// Builds the runtime-gated main application.
  @override
  Widget build(BuildContext context) {
    return OperitTheme(
      initialThemePreferenceSnapshot:
          UserPreferencesManager.defaultThemePreferenceSnapshot,
      initialThemeMode: _runtimeManager.startupThemeMode,
      initialThemeIsReady: false,
      unconfiguredChildEnabled: true,
      child: _AppDialogHost(
        startupWebAccessError: _startupWebAccessError,
        child: AppStartupRouteHost(key: ValueKey<int>(_startupRouteEpoch)),
      ),
    );
  }
}

class _AppDialogHost extends StatefulWidget {
  const _AppDialogHost({
    required this.startupWebAccessError,
    required this.child,
  });

  final String? startupWebAccessError;
  final Widget child;

  @override
  State<_AppDialogHost> createState() => _AppDialogHostState();
}

class _AppDialogHostState extends State<_AppDialogHost> {
  static const String _logTag = 'AppDialogHost';
  static const GeneratedCoreProxyClients _coreClients =
      GeneratedCoreProxyClients(ProxyCoreRuntimeBridge());

  bool _shownStartupWebAccessError = false;
  StreamSubscription<List<core_proxy.PairingPrompt>>?
  _pairingSubscription;
  Future<void> _pairingDialogQueue = Future<void>.value();
  final RuntimeBootstrapManager _runtimeManager =
      RuntimeBootstrapManager.instance;

  /// Subscribes to runtime node pairing prompts.
  @override
  void initState() {
    super.initState();
    _runtimeManager.addListener(_syncPairingSubscription);
    _syncPairingSubscription();
  }

  /// Opens pairing event monitoring after runtime storage configuration.
  void _syncPairingSubscription() {
    if (!_runtimeManager.runtimeConfigured ||
        _pairingSubscription != null) {
      return;
    }
    _pairingSubscription = _coreClients.server.runtimeRemoteLinkService
        .pairingPromptsFlow().listen(
          _handlePairingPrompts,
          onError: (Object error, StackTrace stackTrace) {
            ClientLogger.e('node pairing prompt stream failed', tag: _logTag,
              error: error, stackTrace: stackTrace);
          },
        );
  }

  /// Cancels runtime node pairing prompt monitoring.
  @override
  void dispose() {
    _runtimeManager.removeListener(_syncPairingSubscription);
    unawaited(_pairingSubscription?.cancel());
    super.dispose();
  }

  /// Handles newly reported LinkHost startup errors.
  @override
  void didUpdateWidget(covariant _AppDialogHost oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.startupWebAccessError != widget.startupWebAccessError) {
      _shownStartupWebAccessError = false;
      _showStartupWebAccessError();
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _showStartupWebAccessError();
  }

  void _showStartupWebAccessError() {
    final error = widget.startupWebAccessError;
    if (_shownStartupWebAccessError || error == null) {
      return;
    }
    _shownStartupWebAccessError = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) {
        return;
      }
      final l10n = AppLocalizations.of(context)!;
      showDialog<void>(
        context: context,
        builder: (context) {
          return AlertDialog(
            title: Text(l10n.settingsWebAccessService),
            content: SingleChildScrollView(
              child: SelectableText(l10n.settingsWebAccessStartFailed(error)),
            ),
            actions: <Widget>[
              TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: Text(l10n.ok),
              ),
            ],
          );
        },
      );
    });
  }

  final Set<String> _shownPairings = <String>{};

  /// 观察同一个 runtime 的待确认快照；验证码只显示给本机用户。
  void _handlePairingPrompts(List<core_proxy.PairingPrompt> prompts) {
    _shownPairings.retainAll(prompts.map((prompt) => prompt.pairingId));
    for (final prompt in prompts) {
      if (!_shownPairings.add(prompt.pairingId)) continue;
      _pairingDialogQueue = _pairingDialogQueue.then((_) async {
        if (!mounted || !_shownPairings.contains(prompt.pairingId)) return;
        final l10n = AppLocalizations.of(context)!;
        await showDialog<void>(context: context, builder: (context) => AlertDialog(
          title: Text(l10n.settingsRuntimePairRemote),
          content: SelectableText('${prompt.displayName}\n${prompt.confirmationCode}'),
          actions: [TextButton(onPressed: () => Navigator.pop(context), child: Text(l10n.ok))],
        ));
      }).catchError((Object error, StackTrace stackTrace) {
        ClientLogger.e('node pairing prompt failed', tag: _logTag, error: error, stackTrace: stackTrace);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return widget.child;
  }
}
