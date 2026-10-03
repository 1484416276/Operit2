// ignore_for_file: file_names

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../../../core/proxy/generated/CoreProxyClients.g.dart';
import '../../../../core/proxy/generated/CoreProxyModels.g.dart';
import '../../../../l10n/generated/app_localizations.dart';
import '../../../theme/OperitFormStyles.dart';

/// Manages node-local authentication, discovery, and explicit listener binding.
class PeerListenerSettings extends StatefulWidget {
  /// Creates the node-local connection settings panel.
  const PeerListenerSettings({
    super.key,
    required this.clients,
    this.enabled = true,
    this.onBusyChanged,
  });

  final GeneratedCoreProxyClients clients;
  final bool enabled;
  final ValueChanged<bool>? onBusyChanged;

  /// Creates state that owns the listener settings form.
  @override
  State<PeerListenerSettings> createState() => _PeerListenerSettingsState();
}

class _PeerListenerSettingsState extends State<PeerListenerSettings> {
  final _address = TextEditingController();
  final _token = TextEditingController();
  Set<PeerTransport> _transports = {};
  PeerListenerCapabilities? _capabilities;
  PeerHostPortMode? _portMode;
  bool _discoverable = false;
  bool _busy = true;
  bool _loaded = false;
  String? _error;

  /// Loads the node-local listener configuration when the panel opens.
  @override
  void initState() {
    super.initState();
    _load();
  }

  /// Releases the address and credential text controllers.
  @override
  void dispose() {
    _address.dispose();
    _token.dispose();
    super.dispose();
  }

  /// Copies the current listener token to the system clipboard.
  Future<void> _copyToken() async {
    final l10n = AppLocalizations.of(context)!;
    await Clipboard.setData(ClipboardData(text: _token.text));
    if (mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(l10n.settingsPeerTokenCopied)));
    }
  }

  /// Rotates the listener token through the runtime preference operation.
  Future<void> _refreshToken() async {
    if (_busy || !_loaded) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    widget.onBusyChanged?.call(true);
    try {
      final token = await widget.clients.server.runtimeRemoteLinkService
          .refreshLocalPairingToken();
      if (mounted) setState(() => _token.text = token);
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        widget.onBusyChanged?.call(false);
      }
    }
  }

  /// Loads the persisted listener configuration into the settings form.
  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final config = await widget.clients.server.runtimeRemoteLinkService
          .localHostConfig();
      if (config == null) {
        throw StateError('Runtime listener preferences were not initialized');
      }
      final capabilities = await widget.clients.server.runtimeRemoteLinkService
          .listenerCapabilities();
      if (!mounted) return;
      setState(() {
        _capabilities = capabilities;
        _updateFields(config);
        _loaded = true;
      });
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Persists listener settings and restarts the selected transports.
  Future<void> _apply({bool? discovery}) async {
    final l10n = AppLocalizations.of(context)!;
    final discoverable = discovery ?? _discoverable;
    final appliedToken = _token.text.trim();
    final activeTransports = _transports.where(_supports).toList();
    final activeDiscovery =
        discoverable && _capabilities!.discoveryAdvertisement;
    if (activeTransports.isEmpty && activeDiscovery) {
      setState(() => _error = l10n.settingsPeerDiscoveryNeedsTransport);
      return;
    }
    // TCP and the shared HTTP/WS server currently bind the same configured port.
    if (activeTransports.contains(PeerTransport.tcp) &&
        (activeTransports.contains(PeerTransport.http) ||
            activeTransports.contains(PeerTransport.webSocket))) {
      setState(() => _error = l10n.settingsPeerPortConflict);
      return;
    }
    if (_address.text.trim().isEmpty) {
      setState(() => _error = l10n.settingsPeerAddressRequired);
      return;
    }
    if (appliedToken.isEmpty) {
      setState(() => _error = l10n.settingsPeerTokenRequired);
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    widget.onBusyChanged?.call(true);
    final service = widget.clients.server.runtimeRemoteLinkService;
    try {
      final config = PeerHostConfig(
        bindAddress: _address.text.trim(),
        token: appliedToken,
        transports: _transports.toList(),
        discoveryEnabled: discoverable,
        portMode: _portMode!,
        updatedAt: DateTime.now().millisecondsSinceEpoch,
      );
      await service.stopListening();
      await service.saveLocalHostConfig(config: config);
      if (mounted) setState(() => _updateFields(config));
      if (activeTransports.isNotEmpty) {
        await service.startListening(transports: activeTransports);
      }
      final applied = await service.localHostConfig();
      if (applied == null) {
        throw StateError('Listener configuration is missing after save');
      }
      if (!mounted) return;
      setState(() => _updateFields(applied));
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(l10n.settingsPeerApplied)));
    } catch (error) {
      if (mounted) setState(() => _error = error.toString());
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        widget.onBusyChanged?.call(false);
      }
    }
  }

  /// Updates form values from the configuration persisted by the runtime.
  void _updateFields(PeerHostConfig config) {
    _address.text = config.bindAddress;
    _token.text = config.token;
    _transports = config.transports.toSet();
    _portMode = config.portMode;
    _discoverable = config.discoveryEnabled;
  }

  /// Checks one inbound transport against the loaded Host capability declaration.
  bool _supports(PeerTransport transport) {
    final capabilities = _capabilities;
    return capabilities != null && capabilities.transports.contains(transport);
  }

  /// Builds authentication controls and the explicit listener settings form.
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
          subtitle: Text(
            _loaded && !_capabilities!.discoveryAdvertisement
                ? l10n.settingsPeerDiscoveryUnavailable
                : l10n.settingsRuntimeEnableDiscoveryDescription,
          ),
          value: _discoverable && _capabilities?.discoveryAdvertisement == true,
          onChanged: enabled && _capabilities!.discoveryAdvertisement
              ? (value) => _apply(discovery: value)
              : null,
        ),
        Padding(
          padding: const EdgeInsets.only(top: 8, bottom: 4),
          child: TextField(
            controller: _token,
            readOnly: true,
            enabled: enabled,
            decoration: InputDecoration(
              labelText: l10n.settingsPeerToken,
              helperText: l10n.settingsPeerTokenHelp,
              helperMaxLines: 2,
              isDense: true,
              border: const OutlineInputBorder(),
              suffixIcon: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  IconButton(
                    tooltip: l10n.settingsPeerCopyToken,
                    onPressed: enabled ? _copyToken : null,
                    icon: const Icon(Icons.copy_outlined),
                  ),
                  IconButton(
                    tooltip: l10n.settingsPeerRefreshToken,
                    onPressed: enabled ? _refreshToken : null,
                    icon: const Icon(Icons.refresh_rounded),
                  ),
                ],
              ),
            ),
          ),
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
                        PeerTransport.serial: 'Serial',
                        PeerTransport.http: 'HTTP',
                        PeerTransport.webSocket: 'WebSocket',
                      }.entries)
                        FilterChip(
                          visualDensity: VisualDensity.compact,
                          label: Text(entry.value),
                          tooltip: _supports(entry.key)
                              ? null
                              : l10n.settingsPeerTransportUnavailable(
                                  entry.value,
                                ),
                          selected: _transports.contains(entry.key),
                          onSelected: enabled && _supports(entry.key)
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
                  OperitFormStyles.dropdownButtonFormField<PeerHostPortMode>(
                    context,
                    initialValue: _portMode,
                    isExpanded: true,
                    items: [
                      DropdownMenuItem(
                        value: PeerHostPortMode.fixed,
                        child: Text(l10n.settingsPeerPortModeFixed),
                      ),
                      DropdownMenuItem(
                        value: PeerHostPortMode.automatic,
                        child: Text(l10n.settingsPeerPortModeAutomatic),
                      ),
                    ],
                    onChanged: enabled
                        ? (value) {
                            if (value != null) {
                              setState(() => _portMode = value);
                            }
                          }
                        : null,
                    decoration: InputDecoration(
                      labelText: l10n.settingsPeerPortMode,
                      helperText: l10n.settingsPeerPortModeHelp,
                      helperMaxLines: 3,
                      isDense: true,
                      border: const OutlineInputBorder(),
                    ),
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
