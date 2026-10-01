// ignore_for_file: file_names

import 'dart:convert';
import 'dart:math';

import '../bridge/ProxyCoreRuntimeBridge.dart';
import '../proxy/generated/CoreProxyClients.g.dart';
import '../proxy/generated/CoreProxyModels.g.dart' as generated;

enum LinkAccessHostPortMode { automatic, fixed }

class LinkAccessHostConfig {
  const LinkAccessHostConfig({
    required this.webAccessEnabled,
    required this.discoveryEnabled,
    required this.portMode,
    required this.bindAddress,
    required this.token,
    required this.updatedAt,
  });

  static const List<int> automaticPortSequence = <int>[
    37194,
    37195,
    37196,
    37197,
    37198,
    37199,
    37200,
    37201,
    37202,
    37203,
  ];
  static const String automaticBindAddress = '0.0.0.0:37194';

  factory LinkAccessHostConfig.initial() {
    return LinkAccessHostConfig(
      webAccessEnabled: false,
      discoveryEnabled: false,
      portMode: LinkAccessHostPortMode.automatic,
      bindAddress: automaticBindAddress,
      token: LinkAccessHostToken.generate(),
      updatedAt: DateTime.now().millisecondsSinceEpoch,
    );
  }

  factory LinkAccessHostConfig.fromJson(Map<String, Object?> json) {
    return LinkAccessHostConfig(
      webAccessEnabled: json['webAccessEnabled'] as bool,
      discoveryEnabled: json['discoveryEnabled'] as bool,
      portMode: _linkAccessHostPortModeFromJson(json['portMode']),
      bindAddress: json['bindAddress'] as String,
      token: json['token'] as String,
      updatedAt: json['updatedAt'] as int,
    );
  }

  final bool webAccessEnabled;
  final bool discoveryEnabled;
  final LinkAccessHostPortMode portMode;
  final String bindAddress;
  final String token;
  final int updatedAt;

  LinkAccessHostConfig copyWith({
    bool? webAccessEnabled,
    bool? discoveryEnabled,
    LinkAccessHostPortMode? portMode,
    String? bindAddress,
    String? token,
    int? updatedAt,
  }) {
    return LinkAccessHostConfig(
      webAccessEnabled: webAccessEnabled ?? this.webAccessEnabled,
      discoveryEnabled: discoveryEnabled ?? this.discoveryEnabled,
      portMode: portMode ?? this.portMode,
      bindAddress: bindAddress ?? this.bindAddress,
      token: token ?? this.token,
      updatedAt: updatedAt ?? this.updatedAt,
    );
  }

  Map<String, Object?> toJson() {
    return {
      'webAccessEnabled': webAccessEnabled,
      'discoveryEnabled': discoveryEnabled,
      'portMode': portMode.name,
      'bindAddress': bindAddress,
      'token': token,
      'updatedAt': updatedAt,
    };
  }
}

/// Decodes the persisted Link Access Host port selection.
LinkAccessHostPortMode _linkAccessHostPortModeFromJson(Object? value) {
  if (value is String) {
    for (final mode in LinkAccessHostPortMode.values) {
      if (mode.name == value) {
        return mode;
      }
    }
  }
  throw FormatException('invalid Link Access Host port mode: $value');
}

class LinkAccessHostConfigStore {
  const LinkAccessHostConfigStore._();
  static const GeneratedCoreProxyClients _clients = GeneratedCoreProxyClients(ProxyCoreRuntimeBridge());
  /// 读取原配置，不创建第二份 token。使用生成的类型化服务入口。
  static Future<LinkAccessHostConfig> read() async {
    final config = await _clients.server.runtimeRemoteLinkService.localHostConfig();
    if (config == null) throw StateError('Node listener is not configured');
    return LinkAccessHostConfig(
      webAccessEnabled: config.webAccessEnabled,
      discoveryEnabled: config.discoveryEnabled,
      portMode: config.portMode == generated.PeerHostPortMode.automatic
          ? LinkAccessHostPortMode.automatic : LinkAccessHostPortMode.fixed,
      bindAddress: config.bindAddress, token: config.token, updatedAt: config.updatedAt,
    );
  }
  static Future<void> write(LinkAccessHostConfig config) async {
    await _clients.server.runtimeRemoteLinkService.saveLocalHostConfig(
      config: generated.PeerHostConfig(
        bindAddress: config.bindAddress, token: config.token,
        webAccessEnabled: config.webAccessEnabled, discoveryEnabled: config.discoveryEnabled,
        portMode: config.portMode == LinkAccessHostPortMode.automatic
            ? generated.PeerHostPortMode.automatic : generated.PeerHostPortMode.fixed,
        updatedAt: config.updatedAt,
      ),
    );
  }
}

class LinkAccessHostToken {
  const LinkAccessHostToken._();

  /// Generates the local listener control token.
  static String generate() {
    final random = Random.secure();
    final bytes = List<int>.generate(18, (_) => random.nextInt(256));
    return 'ow-${base64Url.encode(bytes).replaceAll('=', '')}';
  }
}
