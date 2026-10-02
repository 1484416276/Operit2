// ignore_for_file: file_names

import 'dart:async';

import 'package:flutter/material.dart';
import 'SpaceJoinWidgets.dart';
import 'package:flutter/services.dart';

import '../../core/proxy/generated/CoreProxyClients.g.dart';
import '../../core/proxy/generated/CoreProxyModels.g.dart' as generated;
import '../../core/runtime/PeerEndpointTransport.dart';
import '../../l10n/generated/app_localizations.dart';
import '../theme/OperitFormStyles.dart';
import '../features/settings/runtime/PeerListenerSettings.dart';

enum _DeviceSpaceAction { requests, settings }

/// One primary action; technical controls and history live in secondary dialogs.
class DeviceSpaceDiscoveryPanel extends StatefulWidget {
  const DeviceSpaceDiscoveryPanel({
    super.key,
    required this.clients,
    required this.onJoined,
    this.enabled = true,
    this.autoScan = true,
    this.onBusyChanged,
    this.onRequestsChanged,
  });
  final GeneratedCoreProxyClients clients;
  final Future<void> Function(generated.CoreSpace) onJoined;
  final bool enabled, autoScan;
  final ValueChanged<bool>? onBusyChanged;
  final ValueChanged<List<generated.SpaceJoinRequest>>? onRequestsChanged;
  @override
  State<DeviceSpaceDiscoveryPanel> createState() =>
      _DeviceSpaceDiscoveryPanelState();
}

class _DeviceSpaceDiscoveryPanelState extends State<DeviceSpaceDiscoveryPanel> {
  Timer? _timer;
  int _pending = 0;
  bool _loadingRequests = false;
  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(
      const Duration(seconds: 3),
      (_) => unawaited(_loadRequests()),
    );
    unawaited(_loadRequests());
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _loadRequests() async {
    if (_loadingRequests || !widget.enabled) return;
    _loadingRequests = true;
    try {
      final service = widget.clients.server.runtimeRemoteLinkService;
      final outgoing = await service.outgoingDeviceSpaceJoins();
      final incoming = await service.incomingDeviceSpaceJoins();
      if (!mounted) return;
      final count =
          outgoing.where((r) => spaceJoinIsActive(r.status)).length +
          incoming.length;
      if (_pending != count) setState(() => _pending = count);
      widget.onRequestsChanged?.call(outgoing);
    } catch (_) {
      /* No new error section on the landing page. */
    } finally {
      _loadingRequests = false;
    }
  }

  Future<void> _addDevice() async {
    final l10n = AppLocalizations.of(context)!;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(l10n.deviceSpaceAddDevice),
        content: SizedBox(
          width: 440,
          height: (MediaQuery.sizeOf(dialogContext).height * .5).clamp(
            240.0,
            420.0,
          ),
          child: _DeviceSpacePicker(
            clients: widget.clients,
            autoScan: widget.autoScan,
            onJoined: widget.onJoined,
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: Text(l10n.cancel),
          ),
        ],
      ),
    );
    await _loadRequests();
  }

  Future<void> _secondary(_DeviceSpaceAction action) async {
    final l10n = AppLocalizations.of(context)!;
    if (action == _DeviceSpaceAction.requests) {
      await showDialog<void>(
        context: context,
        builder: (dialogContext) => AlertDialog(
          title: Text(l10n.spaceJoinRequests),
          content: SpaceJoinRequestsPanel(
            clients: widget.clients,
            onJoined: widget.onJoined,
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(l10n.ok),
            ),
          ],
        ),
      );
      await _loadRequests();
    } else {
      await showDialog<void>(
        context: context,
        builder: (dialogContext) => AlertDialog(
          title: Text(l10n.deviceSpaceConnectionSettings),
          content: SizedBox(
            width: 440,
            child: SingleChildScrollView(
              child: PeerListenerSettings(
                clients: widget.clients,
                onBusyChanged: widget.onBusyChanged,
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(l10n.ok),
            ),
          ],
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        FilledButton.icon(
          onPressed: widget.enabled ? _addDevice : null,
          icon: const Icon(Icons.add_rounded, size: 20),
          label: Text(l10n.deviceSpaceAddDevice),
        ),
        const SizedBox(width: 4),
        PopupMenuButton<_DeviceSpaceAction>(
          enabled: widget.enabled,
          tooltip: l10n.deviceSpaceMore,
          onSelected: _secondary,
          icon: Badge(
            isLabelVisible: _pending > 0,
            label: Text('$_pending'),
            child: const Icon(Icons.more_horiz_rounded),
          ),
          itemBuilder: (_) => [
            PopupMenuItem(
              value: _DeviceSpaceAction.requests,
              child: Row(
                children: [
                  const Icon(Icons.pending_actions_outlined, size: 20),
                  const SizedBox(width: 12),
                  Expanded(child: Text(l10n.spaceJoinRequests)),
                  if (_pending > 0)
                    Text(
                      '$_pending',
                      style: Theme.of(context).textTheme.labelMedium,
                    ),
                ],
              ),
            ),
            PopupMenuItem(
              value: _DeviceSpaceAction.settings,
              child: Row(
                children: [
                  const Icon(Icons.tune_rounded, size: 20),
                  const SizedBox(width: 12),
                  Text(l10n.deviceSpaceConnectionSettings),
                ],
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class _DeviceSpacePicker extends StatefulWidget {
  const _DeviceSpacePicker({
    required this.clients,
    required this.onJoined,
    this.autoScan = true,
  });
  final GeneratedCoreProxyClients clients;
  final Future<void> Function(generated.CoreSpace) onJoined;
  final bool autoScan;
  @override
  State<_DeviceSpacePicker> createState() => _DeviceSpacePickerState();
}

class _DeviceSpacePickerState extends State<_DeviceSpacePicker> {
  bool _busy = false;
  String? _error;
  List<generated.DiscoveredPeer> _peers = [];
  @override
  void initState() {
    super.initState();
    if (widget.autoScan) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) unawaited(_scan());
      });
    }
  }

  void _setBusy(bool busy) {
    if (!mounted) return;
    setState(() => _busy = busy);
  }

  Future<void> _scan() async {
    _setBusy(true);
    try {
      final peers = await widget.clients.server.runtimeRemoteLinkService
          .discoverPeers(timeoutMs: 2000);
      if (mounted) {
        setState(() {
          _peers = peers;
          _error = null;
        });
      }
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally {
      _setBusy(false);
    }
  }

  Future<void> _pair([generated.DiscoveredPeer? peer]) async {
    _setBusy(true);
    try {
      final _RemotePairResult? result;
      if (peer == null) {
        result = await _RemotePairDialog.show(context, clients: widget.clients);
      } else {
        // LAN 候选不携带 token；免 token 准入由 runtime/Host 实际来源判断。
        final pending = await widget.clients.server.runtimeRemoteLinkService
            .startPairing(
              address: peer.address,
              nodeId: peer.nodeId,
              transport: peerEndpointTransport(peer.address),
              token: null,
            );
        if (!mounted) {
          await widget.clients.server.runtimeRemoteLinkService.cancelPairing(
            pairingId: pending.pairingId,
          );
          return;
        }
        result = await _RemotePairCodeDialog.show(
          context,
          pairing: pending,
          clients: widget.clients,
        );
        if (result == null) {
          await widget.clients.server.runtimeRemoteLinkService.cancelPairing(
            pairingId: pending.pairingId,
          );
        }
      }
      if (result != null) {
        if (!mounted) return;
        final space = await showSpaceJoinRequest(
          context,
          clients: widget.clients,
          deviceId: result.peer.nodeId,
          deviceName: result.peer.displayName,
        );
        if (mounted) {
          if (space != null) await widget.onJoined(space);
          if (mounted) Navigator.pop(context);
        }
      }
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally {
      _setBusy(false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                l10n.devicePickerNearby,
                style: Theme.of(context).textTheme.titleSmall,
              ),
            ),
            IconButton(
              tooltip: l10n.settingsRuntimeScan,
              onPressed: _busy ? null : _scan,
              icon: const Icon(Icons.refresh_rounded),
            ),
          ],
        ),
        if (_busy) const LinearProgressIndicator(),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: Text(
              _error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ),
        Expanded(
          child: _peers.isEmpty
              ? Center(
                  child: Padding(
                    padding: const EdgeInsets.all(24),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          Icons.devices_other_outlined,
                          size: 40,
                          color: Theme.of(context).colorScheme.onSurfaceVariant,
                        ),
                        const SizedBox(height: 12),
                        Text(
                          _busy
                              ? l10n.settingsRuntimeScanning
                              : l10n.devicePickerEmpty,
                          textAlign: TextAlign.center,
                        ),
                        const SizedBox(height: 8),
                        Text(
                          l10n.devicePickerHint,
                          textAlign: TextAlign.center,
                          style: Theme.of(context).textTheme.bodySmall,
                        ),
                      ],
                    ),
                  ),
                )
              : ListView.separated(
                  itemCount: _peers.length,
                  separatorBuilder: (_, _) => const Divider(height: 1),
                  itemBuilder: (context, index) {
                    final peer = _peers[index];
                    return ListTile(
                      contentPadding: EdgeInsets.zero,
                      leading: const Icon(Icons.devices_outlined),
                      title: Text(peer.displayName),
                      subtitle: Text(
                        peer.address,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      trailing: const Icon(Icons.chevron_right_rounded),
                      onTap: _busy ? null : () => _pair(peer),
                    );
                  },
                ),
        ),
        const Divider(height: 24),
        Align(
          alignment: AlignmentDirectional.centerStart,
          child: TextButton.icon(
            onPressed: _busy ? null : () => _pair(),
            icon: const Icon(Icons.link_outlined, size: 18),
            label: Text(l10n.devicePickerManual),
          ),
        ),
      ],
    );
  }
}

Future<generated.CoreSpace?> confirmAndJoinPairedDeviceSpace({
  required BuildContext context,
  required GeneratedCoreProxyClients clients,
  required String deviceId,
  required String deviceName,
}) async {
  final l10n = AppLocalizations.of(context)!;
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l10n.settingsRuntimeJoinSpaceTitle(deviceName)),
      content: Text(l10n.settingsRuntimeJoinSpaceDescription),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(l10n.cancel),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context, true),
          child: Text(l10n.settingsRuntimeJoinSpace),
        ),
      ],
    ),
  );
  if (confirmed != true) return null;
  if (!context.mounted) return null;
  return showSpaceJoinRequest(
    context,
    clients: clients,
    deviceId: deviceId,
    deviceName: deviceName,
  );
}

class _RemotePairDialog extends StatefulWidget {
  const _RemotePairDialog({required this.clients});

  final GeneratedCoreProxyClients clients;

  /// Displays the manual remote pairing dialog.
  static Future<_RemotePairResult?> show(
    BuildContext context, {
    required GeneratedCoreProxyClients clients,
  }) {
    return showDialog<_RemotePairResult>(
      context: context,
      builder: (_) => _RemotePairDialog(clients: clients),
    );
  }

  /// Creates state that owns manual pairing fields.
  @override
  State<_RemotePairDialog> createState() => _RemotePairDialogState();
}

class _RemotePairDialogState extends State<_RemotePairDialog> {
  final TextEditingController _baseUrlController = TextEditingController();
  final TextEditingController _tokenController = TextEditingController();
  final TextEditingController _codeController = TextEditingController();
  generated.PeerTransport _transport = generated.PeerTransport.http;
  generated.PendingPairing? _pairing;
  bool _busy = false;
  String? _error;

  /// Releases all dialog-owned text controllers.
  @override
  void dispose() {
    _baseUrlController.dispose();
    _tokenController.dispose();
    _codeController.dispose();
    super.dispose();
  }

  /// Starts manual pairing from an explicit address and token.
  Future<void> _start() async {
    final l10n = AppLocalizations.of(context)!;
    final baseUrl = _baseUrlController.text.trim();
    final token = _tokenController.text.trim();
    if (baseUrl.isEmpty || token.isEmpty) {
      setState(() {
        _error =
            '${l10n.settingsRuntimeBaseUrl} / ${l10n.settingsRuntimePairToken}: ${l10n.required}';
      });
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final pairing = await widget.clients.server.runtimeRemoteLinkService
          .startPairing(
            address: baseUrl,
            nodeId: '',
            token: token,
            transport: _transport,
          );
      if (mounted) {
        setState(() => _pairing = pairing);
      }
    } catch (error) {
      if (mounted) {
        setState(() => _error = error.toString());
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
      }
    }
  }

  /// Completes manual pairing with the one-time code.
  Future<void> _finish() async {
    final pairing = _pairing;
    if (pairing == null) {
      return;
    }
    final l10n = AppLocalizations.of(context)!;
    final pairingCode = _codeController.text.trim();
    if (!RegExp(r'^\d{6}$').hasMatch(pairingCode)) {
      setState(() {
        _error = l10n.settingsPeerSixDigitCode;
      });
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final session = await widget.clients.server.runtimeRemoteLinkService
          .finishPairing(
            pairingId: pairing.pairingId,
            confirmationCode: pairingCode,
          );
      if (mounted) {
        Navigator.of(context).pop(_RemotePairResult(peer: session));
      }
    } catch (error) {
      if (mounted) {
        setState(() => _error = error.toString());
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
      }
    }
  }

  /// Builds the two-stage manual pairing dialog.
  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final pairing = _pairing;
    return AlertDialog(
      title: Text(l10n.settingsRuntimePairRemote),
      content: SizedBox(
        width: 460,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            TextField(
              controller: _baseUrlController,
              enabled: pairing == null,
              decoration: InputDecoration(
                labelText: l10n.settingsRuntimeBaseUrl,
                border: const OutlineInputBorder(),
                isDense: true,
              ),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _tokenController,
              enabled: pairing == null,
              obscureText: true,
              decoration: InputDecoration(
                labelText: l10n.settingsRuntimePairToken,
                border: const OutlineInputBorder(),
                isDense: true,
              ),
            ),
            if (pairing == null) ...<Widget>[
              const SizedBox(height: 10),
              _LinkTransportSelector(
                value: _transport,
                onChanged: (value) => setState(() => _transport = value),
              ),
            ],
            if (pairing != null) ...<Widget>[
              const SizedBox(height: 10),
              TextField(
                controller: _codeController,
                keyboardType: TextInputType.number,
                inputFormatters: [
                  FilteringTextInputFormatter.digitsOnly,
                  LengthLimitingTextInputFormatter(6),
                ],
                decoration: InputDecoration(
                  labelText: l10n.settingsRuntimePairCode,
                  border: const OutlineInputBorder(),
                  isDense: true,
                ),
              ),
            ],
            if (_error != null) ...<Widget>[
              const SizedBox(height: 10),
              Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  _error!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            ],
          ],
        ),
      ),
      actions: <Widget>[
        TextButton(
          onPressed: _busy ? null : () => Navigator.of(context).pop(),
          child: Text(l10n.cancel),
        ),
        FilledButton(
          onPressed: _busy ? null : (pairing == null ? _start : _finish),
          child: Text(
            pairing == null
                ? l10n.settingsRuntimeStartPairing
                : l10n.settingsRuntimeFinishPairing,
          ),
        ),
      ],
    );
  }
}

class _RemotePairCodeDialog extends StatefulWidget {
  const _RemotePairCodeDialog({required this.pairing, required this.clients});

  final GeneratedCoreProxyClients clients;

  final generated.PendingPairing pairing;

  /// Displays the one-time code dialog for a discovered device.
  static Future<_RemotePairResult?> show(
    BuildContext context, {
    required generated.PendingPairing pairing,
    required GeneratedCoreProxyClients clients,
  }) {
    return showDialog<_RemotePairResult>(
      context: context,
      builder: (_) => _RemotePairCodeDialog(pairing: pairing, clients: clients),
    );
  }

  /// Creates state that owns the one-time pairing code field.
  @override
  State<_RemotePairCodeDialog> createState() => _RemotePairCodeDialogState();
}

class _RemotePairCodeDialogState extends State<_RemotePairCodeDialog> {
  final TextEditingController _codeController = TextEditingController();
  bool _busy = false;
  String? _error;

  /// Releases the one-time pairing code controller.
  @override
  void dispose() {
    _codeController.dispose();
    super.dispose();
  }

  /// Completes pairing with the discovered device.
  Future<void> _finish() async {
    final l10n = AppLocalizations.of(context)!;
    final pairingCode = _codeController.text.trim();
    if (!RegExp(r'^\d{6}$').hasMatch(pairingCode)) {
      setState(() {
        _error = l10n.settingsPeerSixDigitCode;
      });
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final session = await widget.clients.server.runtimeRemoteLinkService
          .finishPairing(
            pairingId: widget.pairing.pairingId,
            confirmationCode: pairingCode,
          );
      if (mounted) {
        Navigator.of(context).pop(_RemotePairResult(peer: session));
      }
    } catch (error) {
      if (mounted) {
        setState(() => _error = error.toString());
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
      }
    }
  }

  /// Builds the discovered-device pairing confirmation dialog.
  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    return AlertDialog(
      title: Text(l10n.settingsRuntimePairRemote),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            TextField(
              controller: _codeController,
              keyboardType: TextInputType.number,
              inputFormatters: [
                FilteringTextInputFormatter.digitsOnly,
                LengthLimitingTextInputFormatter(6),
              ],
              autofocus: true,
              decoration: InputDecoration(
                labelText: l10n.settingsRuntimePairCode,
                border: const OutlineInputBorder(),
                isDense: true,
              ),
            ),
            const SizedBox(height: 10),
            if (_error != null) ...<Widget>[
              const SizedBox(height: 10),
              Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  _error!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            ],
          ],
        ),
      ),
      actions: <Widget>[
        TextButton(
          onPressed: _busy ? null : () => Navigator.of(context).pop(),
          child: Text(l10n.cancel),
        ),
        FilledButton(
          onPressed: _busy ? null : _finish,
          child: Text(l10n.settingsRuntimeFinishPairing),
        ),
      ],
    );
  }
}

class _RemotePairResult {
  const _RemotePairResult({required this.peer});
  final generated.PairedPeer peer;
}

class _LinkTransportSelector extends StatelessWidget {
  const _LinkTransportSelector({required this.value, required this.onChanged});

  final generated.PeerTransport value;
  final ValueChanged<generated.PeerTransport> onChanged;

  /// Builds the explicit Link carrier selector shared by pairing dialogs.
  @override
  Widget build(BuildContext context) {
    return OperitFormStyles.dropdownButtonFormField<generated.PeerTransport>(
      context,
      initialValue: value,
      decoration: const InputDecoration(
        labelText: 'Link transport',
        border: OutlineInputBorder(),
        isDense: true,
      ),
      items: const <DropdownMenuItem<generated.PeerTransport>>[
        DropdownMenuItem(
          value: generated.PeerTransport.http,
          child: Text('HTTP'),
        ),
        DropdownMenuItem(
          value: generated.PeerTransport.webSocket,
          child: Text('WebSocket'),
        ),
      ],
      onChanged: (selected) {
        if (selected != null) {
          onChanged(selected);
        }
      },
    );
  }
}
