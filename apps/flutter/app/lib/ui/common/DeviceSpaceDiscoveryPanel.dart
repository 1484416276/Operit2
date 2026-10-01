// ignore_for_file: file_names

import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/proxy/generated/CoreProxyClients.g.dart';
import '../../core/proxy/generated/CoreProxyModels.g.dart' as generated;
import '../../core/runtime/RemotePairingBridge.dart';
import '../../l10n/generated/app_localizations.dart';
import '../theme/OperitFormStyles.dart';

class DeviceSpaceDiscoveryPanel extends StatefulWidget {
  const DeviceSpaceDiscoveryPanel({super.key, required this.clients, required this.onJoined,
    this.enabled = true, this.autoScan = true, this.onBusyChanged});
  final GeneratedCoreProxyClients clients;
  final Future<void> Function(generated.CoreSpace) onJoined;
  final bool enabled;
  final bool autoScan;
  final ValueChanged<bool>? onBusyChanged;
  @override
  State<DeviceSpaceDiscoveryPanel> createState() => _DeviceSpaceDiscoveryPanelState();
}

class _DeviceSpaceDiscoveryPanelState extends State<DeviceSpaceDiscoveryPanel> {
  bool _busy = false;
  String? _error;
  List<generated.DiscoveredPeer> _peers = [];
  @override
  void initState() {
    super.initState();
    if (widget.autoScan) {
      WidgetsBinding.instance.addPostFrameCallback((_) { if (mounted) unawaited(_scan()); });
    }
  }
  void _setBusy(bool busy) {
    if (!mounted) return;
    setState(() => _busy = busy);
    widget.onBusyChanged?.call(busy);
  }
  Future<void> _scan() async {
    _setBusy(true);
    try {
      final peers = await const RemotePairingBridge().discover();
      if (mounted) setState(() { _peers = peers; _error = null; });
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally { _setBusy(false); }
  }
  Future<void> _pair([generated.DiscoveredPeer? peer]) async {
    _setBusy(true);
    try {
      final _RemotePairResult? result;
      if (peer == null) {
        result = await _RemotePairDialog.show(context);
      } else {
        // LAN 候选不携带 token；免 token 准入由 runtime/Host 实际来源判断。
        final pending = await const RemotePairingBridge().start(endpoint: peer.address, nodeId: peer.nodeId);
        if (!mounted) { await const RemotePairingBridge().cancel(pending.pairingId); return; }
        result = await _RemotePairCodeDialog.show(context, pairing: pending);
        if (result == null) await const RemotePairingBridge().cancel(pending.pairingId);
      }
      if (result != null) {
        final space = await widget.clients.server.runtimeRemoteLinkService.joinPairedDeviceSpace(deviceId: result.peer.nodeId);
        if (mounted) await widget.onJoined(space);
      }
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally { _setBusy(false); }
  }
  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final enabled = widget.enabled && !_busy;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Wrap(spacing: 8, children: [
        OutlinedButton.icon(onPressed: enabled ? _scan : null, icon: const Icon(Icons.search),
          label: Text(_busy ? l10n.settingsRuntimeScanning : l10n.settingsRuntimeScan)),
        OutlinedButton.icon(onPressed: enabled ? () => _pair() : null, icon: const Icon(Icons.add_link),
          label: Text(l10n.settingsRuntimePairRemote)),
      ]),
      if (_error != null) SelectableText(_error!, style: TextStyle(color: Theme.of(context).colorScheme.error)),
      for (final peer in _peers) ListTile(title: Text(peer.displayName), subtitle: Text(peer.address),
        trailing: IconButton(icon: const Icon(Icons.group_add_outlined), onPressed: enabled ? () => _pair(peer) : null)),
    ]);
  }
}

Future<generated.CoreSpace?> confirmAndJoinPairedDeviceSpace({
  required BuildContext context, required GeneratedCoreProxyClients clients,
  required String deviceId, required String deviceName,
}) async {
  final l10n = AppLocalizations.of(context)!;
  final confirmed = await showDialog<bool>(context: context, builder: (context) => AlertDialog(
    title: Text(l10n.settingsRuntimeJoinSpaceTitle(deviceName)),
    content: Text(l10n.settingsRuntimeJoinSpaceDescription), actions: [
      TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l10n.cancel)),
      FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(l10n.settingsRuntimeJoinSpace)),
    ],
  ));
  if (confirmed != true) return null;
  return clients.server.runtimeRemoteLinkService.joinPairedDeviceSpace(deviceId: deviceId);
}

class _RemotePairDialog extends StatefulWidget {
  const _RemotePairDialog();

  /// Displays the manual remote pairing dialog.
  static Future<_RemotePairResult?> show(BuildContext context) {
    return showDialog<_RemotePairResult>(
      context: context,
      builder: (_) => const _RemotePairDialog(),
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
  generated.PeerTransport _transport =
      generated.PeerTransport.http;
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
      final pairing = await const RemotePairingBridge().start(
        endpoint: baseUrl,
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
    if (pairingCode.isEmpty) {
      setState(() {
        _error = '${l10n.settingsRuntimePairCode}: ${l10n.required}';
      });
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final session = await const RemotePairingBridge().finish(
        pairingId: pairing.pairingId,
        pairingCode: pairingCode,
      );
      if (mounted) {
        Navigator.of(context).pop(
          _RemotePairResult(
                peer: session,
          ),
        );
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
              _LinkTransportSelector(value: _transport, onChanged: (value) => setState(() => _transport = value)),
            ],
            if (pairing != null) ...<Widget>[
              const SizedBox(height: 10),
              TextField(
                controller: _codeController,
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
  const _RemotePairCodeDialog({required this.pairing});

  final generated.PendingPairing pairing;

  /// Displays the one-time code dialog for a discovered device.
  static Future<_RemotePairResult?> show(
    BuildContext context, {
    required generated.PendingPairing pairing,
  }) {
    return showDialog<_RemotePairResult>(
      context: context,
      builder: (_) => _RemotePairCodeDialog(pairing: pairing),
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
    if (pairingCode.isEmpty) {
      setState(() {
        _error = '${l10n.settingsRuntimePairCode}: ${l10n.required}';
      });
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final session = await const RemotePairingBridge().finish(
        pairingId: widget.pairing.pairingId,
        pairingCode: pairingCode,
      );
      if (mounted) {
        Navigator.of(context).pop(
          _RemotePairResult(
                peer: session,
          ),
        );
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
    return OperitFormStyles.dropdownButtonFormField<
      generated.PeerTransport
    >(
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

/// Returns the configured user name or the explicit unconfigured label.
String _configuredUserName(BuildContext context, String userName) {
  final normalized = userName.trim();
  return normalized.isEmpty
      ? AppLocalizations.of(context)!.settingsUserProfileUnnamed
      : normalized;
}
