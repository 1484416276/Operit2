// ignore_for_file: file_names
import 'dart:async';
import 'package:flutter/material.dart';
import '../../core/logging/ClientLogger.dart';
import '../../core/proxy/generated/CoreProxyClients.g.dart';
import '../../core/proxy/generated/CoreProxyModels.g.dart' as generated;
import '../../l10n/generated/app_localizations.dart';
import 'SpaceJoinWidgets.dart';

/// Application-wide monitoring, below the Navigator. Independent incoming/outgoing
/// workers prevent a slow applicant sync from blocking someone else's approval.
class AppPeerDialogHost extends StatefulWidget {
  const AppPeerDialogHost({
    super.key,
    required this.clients,
    required this.enabled,
    required this.child,
  });
  final GeneratedCoreProxyClients clients;
  final bool enabled;
  final Widget child;
  @override
  State<AppPeerDialogHost> createState() => _AppPeerDialogHostState();
}

class _AppPeerDialogHostState extends State<AppPeerDialogHost> {
  StreamSubscription<List<generated.PairingPrompt>>? _pairingSubscription;
  Timer? _timer;
  Future<void> _dialogQueue = Future<void>.value();
  final Set<String> _shownPairings = {};
  final Set<String> _shownAssignments = {};
  final Map<String, ModalRoute<void>> _pairingRoutes = {};
  bool _incomingBusy = false, _outgoingBusy = false;
  int _epoch = 0;
  @override
  void initState() {
    super.initState();
    _start();
  }

  @override
  void didUpdateWidget(covariant AppPeerDialogHost oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.enabled != widget.enabled ||
        oldWidget.clients != widget.clients) {
      _stop();
      _start();
    }
  }

  void _start() {
    if (!widget.enabled) return;
    final epoch = _epoch;
    _pairingSubscription = widget.clients.server.runtimeRemoteLinkService
        .pairingPromptsFlow()
        .listen(
          (prompts) {
            if (_valid(epoch)) _pairingPrompts(prompts, epoch);
          },
          onError: (Object error, StackTrace stack) =>
              _log('Pairing monitoring failed', error, stack),
        );
    _timer = Timer.periodic(const Duration(seconds: 3), (_) => _poll(epoch));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_valid(epoch)) _poll(epoch);
    });
  }

  bool _valid(int epoch) => mounted && widget.enabled && epoch == _epoch;
  void _poll(int epoch) {
    if (!_valid(epoch)) return;
    unawaited(_incoming(epoch));
    unawaited(_outgoing(epoch));
  }

  void _stop() {
    _epoch++;
    _timer?.cancel();
    _timer = null;
    unawaited(_pairingSubscription?.cancel());
    _pairingSubscription = null;
    _shownPairings.clear();
    _shownAssignments.clear();
    for (final route in _pairingRoutes.values.toList()) {
      route.navigator?.removeRoute(route);
    }
    _pairingRoutes.clear();
  }

  @override
  void dispose() {
    _stop();
    super.dispose();
  }

  void _log(String message, Object error, StackTrace stack) {
    ClientLogger.e(
      message,
      tag: 'AppPeerDialogHost',
      error: error,
      stackTrace: stack,
    );
  }

  void _pairingPrompts(List<generated.PairingPrompt> prompts, int epoch) {
    final pending = prompts.map((p) => p.pairingId).toSet();
    _shownPairings.retainAll(pending);
    // Completion/cancellation closes exactly the code route, never another dialog.
    for (final id
        in _pairingRoutes.keys.where((id) => !pending.contains(id)).toList()) {
      final route = _pairingRoutes.remove(id);
      if (route != null) route.navigator?.removeRoute(route);
    }
    for (final prompt in prompts) {
      if (!_shownPairings.add(prompt.pairingId)) continue;
      _dialogQueue = _dialogQueue
          .then((_) async {
            if (!mounted ||
                !_valid(epoch) ||
                !_shownPairings.contains(prompt.pairingId)) {
              return;
            }
            final l10n = AppLocalizations.of(context)!;
            try {
              await showDialog<void>(
                context: context,
                builder: (dialogContext) {
                  _pairingRoutes[prompt.pairingId] = ModalRoute.of<void>(
                    dialogContext,
                  )!;
                  return AlertDialog(
                    title: Text(l10n.settingsRuntimePairRemote),
                    content: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(prompt.displayName),
                        const SizedBox(height: 16),
                        Text(l10n.settingsRuntimePairCode),
                        const SizedBox(height: 8),
                        Center(
                          child: SelectableText(
                            prompt.confirmationCode,
                            style: Theme.of(dialogContext)
                                .textTheme
                                .headlineLarge
                                ?.copyWith(
                                  fontWeight: FontWeight.w600,
                                  letterSpacing: 8,
                                  color: Theme.of(
                                    dialogContext,
                                  ).colorScheme.primary,
                                ),
                          ),
                        ),
                        const SizedBox(height: 12),
                        Text(
                          l10n.settingsPeerCodeInstructions,
                          style: Theme.of(dialogContext).textTheme.bodySmall,
                        ),
                      ],
                    ),
                    actions: [
                      TextButton(
                        onPressed: () => Navigator.pop(dialogContext),
                        child: Text(l10n.ok),
                      ),
                    ],
                  );
                },
              );
            } finally {
              _pairingRoutes.remove(prompt.pairingId);
            }
          })
          .catchError((Object error, StackTrace stack) {
            _shownPairings.remove(prompt.pairingId);
            _log('Pairing dialog failed', error, stack);
          });
    }
  }

  Future<void> _incoming(int epoch) async {
    if (_incomingBusy) return;
    _incomingBusy = true;
    try {
      final service = widget.clients.server.runtimeRemoteLinkService;
      final requests = await service.incomingDeviceSpaceJoins();
      if (!_valid(epoch)) return;
      _shownAssignments.retainAll(
        requests.map((r) => '${r.requestId}:${r.assignmentVersion}'),
      );
      for (final request in requests) {
        final token = '${request.requestId}:${request.assignmentVersion}';
        if (!request.canApprove ||
            request.reviewerDeviceId == null ||
            !_shownAssignments.add(token)) {
          continue;
        }
        _dialogQueue = _dialogQueue
            .then((_) async {
              if (!_valid(epoch) || !_shownAssignments.contains(token)) return;
              final current = await service.incomingDeviceSpaceJoins();
              final assigned = current.where(
                (r) =>
                    r.requestId == request.requestId &&
                    r.assignmentVersion == request.assignmentVersion &&
                    r.canApprove,
              );
              if (!mounted || !_valid(epoch) || assigned.isEmpty) {
                _shownAssignments.remove(token);
                return;
              }
              await showSpaceJoinApproval(
                context,
                clients: widget.clients,
                request: assigned.first,
              );
            })
            .catchError((Object error, StackTrace stack) {
              _shownAssignments.remove(
                token,
              ); // A failed attempt must not suppress every subsequent popup.
              _log('Approval dialog failed', error, stack);
            });
      }
    } catch (error, stack) {
      _log('Approval monitoring failed', error, stack);
    } finally {
      _incomingBusy = false;
    }
  }

  Future<void> _outgoing(int epoch) async {
    if (_outgoingBusy) return;
    _outgoingBusy = true;
    try {
      final service = widget.clients.server.runtimeRemoteLinkService;
      for (final request in await service.outgoingDeviceSpaceJoins()) {
        if (!_valid(epoch)) return;
        if (!spaceJoinIsActive(request.status)) continue;
        try {
          await service.refreshDeviceSpaceJoin(requestId: request.requestId);
        } catch (_) {
          /* Preserved on disk; offline is retried, not turned into rejection. */
        }
      }
    } catch (error, stack) {
      _log('Applicant monitoring failed', error, stack);
    } finally {
      _outgoingBusy = false;
    }
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
