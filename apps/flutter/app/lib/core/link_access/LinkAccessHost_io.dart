// ignore_for_file: file_names, unused_element

import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../link/CoreLinkProtocol.dart';
import '../runtime/RuntimeDeviceInfoProvider.dart';
import 'LinkAccessHostConfig.dart';
import 'WebAccessLaunchInfo.dart';

class LinkAccessHost extends ChangeNotifier {
  LinkAccessHost._();

  static final LinkAccessHost instance = LinkAccessHost._();
  static const MethodChannel _runtimeChannel = MethodChannel('operit/runtime');
  bool _running = false;
  LinkAccessHostConfig? _config;
  String? _deviceId;

  bool get isRunning => _running;
  LinkAccessHostConfig? get currentConfig => _config;
  String? get deviceId => _deviceId;
  bool get supportsDeviceSpaceDiscovery => true;

  WebAccessLaunchInfo? get webAccessLaunchInfo => null;
  String? get baseUrl {
    final config = _config;
    if (config == null || !_running) {
      return null;
    }
    return _baseUrlForBindAddress(config.bindAddress);
  }

  Future<List<String>> pairingBaseUrls(LinkAccessHostConfig config) async {
    final endpoint = _parseBindAddress(config.bindAddress);
    if (_isWildcardHost(endpoint.host)) {
      final hosts = await _lanIpv4Hosts();
      return hosts
          .map((host) => 'http://$host:${endpoint.port}')
          .toList(growable: false);
    }
    if (_isLoopbackHost(endpoint.host)) {
      return <String>[];
    }
    return <String>['http://${endpoint.host}:${endpoint.port}'];
  }

  Future<void> initializeFromConfig() async {
    final config = await LinkAccessHostConfigStore.read();
    if (config.webAccessEnabled || config.discoveryEnabled) {
      await start(config);
    }
  }

  Future<void> start(LinkAccessHostConfig config) async {
    if (_running) {
      await stop(updateConfig: false);
    }
    final shutdownToken = LinkAccessHostToken.generate();
    late final ({LinkAccessHostConfig config, String deviceId}) started;
    try {
      started = await _startNativeWebAccessServerWithPortMode(
        config,
        shutdownToken,
      );
    } catch (_) {
      _config = null;
      rethrow;
    }
    _config = started.config;
    _deviceId = started.deviceId;
    _running = true;
  }

  /// Stops the active Link Access server using its configured control interface.
  Future<void> stop({bool updateConfig = true}) async {
    if (!_running) {
      return;
    }
    await _stopNativeWebAccessServer();
    _running = false;
    _config = null;
    _deviceId = null;
    if (updateConfig) {
      final config = await LinkAccessHostConfigStore.read();
      await LinkAccessHostConfigStore.write(
        config.copyWith(
          webAccessEnabled: false,
          updatedAt: DateTime.now().millisecondsSinceEpoch,
        ),
      );
    }
  }

  /// Starts the native Access Host and returns its Core-owned identity.
  Future<String> _startNativeWebAccessServer(
    LinkAccessHostConfig config,
    String shutdownToken,
  ) async {
    final deviceInfo = await RuntimeDeviceInfoProvider.current();
    final responseText = await _runtimeChannel.invokeMethod<String>(
      'startWebAccessServer',
      <String, Object?>{
        'bindAddress': config.bindAddress,
        'token': config.token,
        'shutdownToken': shutdownToken,
        // Kept as an empty compatibility argument for older platform channels.
        'webRoot': '',
        'deviceInfo': jsonEncode(deviceInfo.toJson()),
        'enableWebAccess': config.webAccessEnabled.toString(),
        'enableDiscovery': config.discoveryEnabled.toString(),
      },
    );
    final response = _throwNativeWebAccessError(responseText);
    final deviceId = response['deviceId'];
    if (deviceId is! String || deviceId.isEmpty) {
      throw const CoreLinkError(
        code: 'INVALID_RESPONSE',
        message: 'runtime bridge did not return the Link Access identity',
      );
    }
    return deviceId;
  }

  /// Starts the native Access Host on the configured port selection.
  Future<({LinkAccessHostConfig config, String deviceId})>
  _startNativeWebAccessServerWithPortMode(
    LinkAccessHostConfig config,
    String shutdownToken,
  ) async {
    if (config.portMode == LinkAccessHostPortMode.fixed) {
      final deviceId = await _startNativeWebAccessServer(config, shutdownToken);
      return (config: config, deviceId: deviceId);
    }
    final endpoint = _parseBindAddress(config.bindAddress);
    Object? lastError;
    StackTrace? lastStackTrace;
    for (final bindAddress in _automaticBindAddresses(endpoint.host)) {
      final candidate = config.copyWith(bindAddress: bindAddress);
      try {
        final deviceId = await _startNativeWebAccessServer(
          candidate,
          shutdownToken,
        );
        return (config: candidate, deviceId: deviceId);
      } catch (error, stackTrace) {
        lastError = error;
        lastStackTrace = stackTrace;
      }
    }
    if (lastError != null && lastStackTrace != null) {
      Error.throwWithStackTrace(lastError, lastStackTrace);
    }
    throw StateError('no web access ports configured');
  }

  Future<void> _stopNativeWebAccessServer() async {
    final responseText = await _runtimeChannel.invokeMethod<String>(
      'stopWebAccessServer',
    );
    _throwNativeWebAccessError(responseText);
  }

  /// Validates a native Access Host response and returns its JSON payload.
  Map<String, Object?> _throwNativeWebAccessError(String? responseText) {
    if (responseText == null) {
      throw const CoreLinkError(
        code: 'EMPTY_RESPONSE',
        message: 'runtime bridge returned empty web access response',
      );
    }
    final response = jsonDecode(responseText) as Map<String, Object?>;
    if (response['ok'] == true) {
      return response;
    }
    if (response.containsKey('code') && response.containsKey('message')) {
      throw CoreLinkError.fromJson(response);
    }
    throw CoreLinkError(
      code: 'INVALID_RESPONSE',
      message: 'runtime bridge web access response is invalid',
    );
  }
}

class _BindEndpoint {
  const _BindEndpoint({required this.host, required this.port});

  final String host;
  final int port;
}

_BindEndpoint _parseBindAddress(String bindAddress) {
  final index = bindAddress.lastIndexOf(':');
  if (index <= 0 || index == bindAddress.length - 1) {
    throw FormatException('invalid bind address: $bindAddress');
  }
  return _BindEndpoint(
    host: bindAddress.substring(0, index),
    port: int.parse(bindAddress.substring(index + 1)),
  );
}

String _baseUrlForBindAddress(String bindAddress) {
  final endpoint = _parseBindAddress(bindAddress);
  final host = switch (endpoint.host) {
    '0.0.0.0' => '127.0.0.1',
    '::' => '127.0.0.1',
    _ => endpoint.host,
  };
  return 'http://$host:${endpoint.port}';
}

bool _isWildcardHost(String host) {
  return host == '0.0.0.0' || host == '::';
}

bool _isLoopbackHost(String host) {
  return host == '127.0.0.1' || host == 'localhost' || host == '::1';
}

Future<List<String>> _lanIpv4Hosts() async {
  final interfaces = await NetworkInterface.list(
    includeLoopback: false,
    type: InternetAddressType.IPv4,
  );
  final hosts = <String>{};
  for (final interface in interfaces) {
    for (final address in interface.addresses) {
      if (!address.isLoopback && !address.isLinkLocal) {
        hosts.add(address.address);
      }
    }
  }
  final sorted = hosts.toList(growable: false)..sort();
  return sorted;
}

List<String> _automaticBindAddresses(String host) {
  return LinkAccessHostConfig.automaticPortSequence
      .map((port) => '$host:$port')
      .toList(growable: false);
}
