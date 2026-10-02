// ignore_for_file: file_names

import 'dart:convert';
import 'dart:math';

import 'package:flutter/material.dart';

import '../../../../core/proxy/generated/CoreProxyClients.g.dart';
import '../../../../core/proxy/generated/CoreProxyModels.g.dart';
import '../../../../l10n/generated/app_localizations.dart';

/// Node-local listener settings; no pairing credentials are displayed here.
class PeerListenerSettings extends StatefulWidget {
  const PeerListenerSettings({
    super.key,
    required this.clients,
    this.enabled = true,
    this.onBusyChanged,
  });

  final GeneratedCoreProxyClients clients;
  final bool enabled;
  final ValueChanged<bool>? onBusyChanged;

  @override
  State<PeerListenerSettings> createState() => _PeerListenerSettingsState();
}

class _PeerListenerSettingsState extends State<PeerListenerSettings> {
  final _address = TextEditingController();
  PeerHostConfig? _saved;
  Set<PeerTransport> _transports = {PeerTransport.http};
  bool _discoverable = false;
  bool _busy = true;
  bool _loaded = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _address.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final config = await widget.clients.server.runtimeRemoteLinkService
          .localHostConfig();
      if (!mounted) return;
      setState(() {
        _saved = config;
        _address.text = config?.bindAddress ?? '0.0.0.0:37195';
        _transports = config?.transports.toSet() ?? {PeerTransport.http};
        _discoverable = config?.discoveryEnabled ?? false;
        _loaded = true;
      });
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _apply({bool? discovery}) async {
    final l10n = AppLocalizations.of(context)!;
    final discoverable = discovery ?? _discoverable;
    if (_transports.isEmpty && discoverable) {
      setState(() => _error = l10n.settingsPeerDiscoveryNeedsTransport);
      return;
    }
    // TCP and the shared HTTP/WS server currently bind the same configured port.
    if (_transports.contains(PeerTransport.tcp) &&
        (_transports.contains(PeerTransport.http) ||
            _transports.contains(PeerTransport.webSocket))) {
      setState(() => _error = l10n.settingsPeerPortConflict);
      return;
    }
    if (_address.text.trim().isEmpty) {
      setState(() => _error = l10n.settingsPeerAddressRequired);
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    widget.onBusyChanged?.call(true);
    final service = widget.clients.server.runtimeRemoteLinkService;
    final previous = _saved;
    try {
      final token = previous?.token;
      final config = PeerHostConfig(
        bindAddress: _address.text.trim(),
        token: token != null && token.isNotEmpty
            ? token
            : base64UrlEncode(
                List<int>.generate(32, (_) => Random.secure().nextInt(256)),
              ),
        transports: _transports.toList(),
        discoveryEnabled: discoverable,
        portMode: PeerHostPortMode.automatic,
        updatedAt: DateTime.now().millisecondsSinceEpoch,
      );
      await service.stopListening();
      await service.saveLocalHostConfig(config: config);
      if (config.transports.isNotEmpty) {
        await service.startListening(transports: config.transports);
      }
      // Runtime may have moved off an occupied port. Display/save that actual
      // address instead of restoring the stale preferred address on the next edit.
      final applied = await service.localHostConfig() ?? config;
      if (!mounted) return;
      setState(() {
        _saved = applied;
        _address.text = applied.bindAddress;
        _discoverable = applied.discoveryEnabled;
      });
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(l10n.settingsPeerApplied)));
    } catch (error) {
      // startListening can partially succeed. Close those listeners and restore
      // the previous configuration rather than retaining a broken startup config.
      String? rollbackError;
      try {
        await service.stopListening();
        final fallback =
            previous ??
            PeerHostConfig(
              bindAddress: '0.0.0.0:37195',
              token: '',
              transports: const [],
              discoveryEnabled: false,
              portMode: PeerHostPortMode.automatic,
              updatedAt: DateTime.now().millisecondsSinceEpoch,
            );
        await service.saveLocalHostConfig(config: fallback);
        if (fallback.transports.isNotEmpty) {
          await service.startListening(transports: fallback.transports);
        }
      } catch (restoreError) {
        rollbackError = restoreError.toString();
      }
      if (mounted) {
        setState(
          () => _error = rollbackError == null
              ? error.toString()
              : '${error.toString()}\n${l10n.settingsPeerRestoreFailed(rollbackError)}',
        );
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        widget.onBusyChanged?.call(false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context)!;
    final enabled = widget.enabled && _loaded && !_busy;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          dense: true,
          visualDensity: VisualDensity.compact,
          title: Text(l10n.settingsRuntimeEnableDiscovery),
          subtitle: Text(l10n.settingsRuntimeEnableDiscoveryDescription),
          value: _discoverable,
          onChanged: enabled ? (value) => _apply(discovery: value) : null,
        ),
        ExpansionTile(
          dense: true,
          tilePadding: EdgeInsets.zero,
          shape: const Border(),
          collapsedShape: const Border(),
          leading: const Icon(Icons.tune_outlined, size: 20),
          title: Text(l10n.settingsPeerAdvanced),
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 8, bottom: 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    l10n.settingsPeerTransportHelp,
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      for (final entry in {
                        PeerTransport.tcp: 'TCP',
                        PeerTransport.bluetooth: 'Bluetooth',
                        PeerTransport.http: 'HTTP',
                        PeerTransport.webSocket: 'WebSocket',
                      }.entries)
                        FilterChip(
                          visualDensity: VisualDensity.compact,
                          label: Text(entry.value),
                          selected: _transports.contains(entry.key),
                          onSelected: enabled
                              ? (selected) => setState(() {
                                  if (selected) {
                                    _transports.add(entry.key);
                                  } else {
                                    _transports.remove(entry.key);
                                  }
                                })
                              : null,
                        ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  TextField(
                    controller: _address,
                    enabled: enabled,
                    decoration: InputDecoration(
                      labelText: l10n.settingsPeerBindAddress,
                      helperText: l10n.settingsPeerBindAddressHelp,
                      helperMaxLines: 2,
                      isDense: true,
                      border: const OutlineInputBorder(),
                    ),
                  ),
                  const SizedBox(height: 12),
                  Align(
                    alignment: AlignmentDirectional.centerEnd,
                    child: FilledButton.tonalIcon(
                      onPressed: enabled ? () => _apply() : null,
                      icon: const Icon(Icons.check_outlined, size: 18),
                      label: Text(l10n.save),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        if (_busy) const LinearProgressIndicator(),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.all(16),
            child: SelectableText(
              _error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ),
        if (!_loaded && !_busy)
          TextButton(onPressed: _load, child: Text(l10n.retry)),
      ],
    );
  }
}
