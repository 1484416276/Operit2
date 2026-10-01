// ignore_for_file: file_names

import '../bridge/ProxyCoreRuntimeBridge.dart';
import '../proxy/generated/CoreProxyClients.g.dart';
import '../proxy/generated/CoreProxyModels.g.dart' as generated;
import 'LinkAccessHostConfig.dart';

/// Flutter 只委托类型化 runtime 服务；平台能力与监听生命周期由 runtime/Host 管理。
class LinkAccessHost {
  LinkAccessHost._();

  static final LinkAccessHost instance = LinkAccessHost._();
  static const GeneratedCoreProxyClients _clients =
      GeneratedCoreProxyClients(ProxyCoreRuntimeBridge());

  /// 原配置不存在时不另建 token；平台是否支持监听由 runtime 判断。
  Future<void> initializeFromConfig() async {
    final config = await _clients.server.runtimeRemoteLinkService.localHostConfig();
    if (config?.discoveryEnabled == true) {
      await _clients.server.runtimeRemoteLinkService.startListening(
        transport: generated.PeerTransport.http,
      );
    }
  }

  Future<void> start(
    LinkAccessHostConfig config, {
    generated.PeerTransport transport = generated.PeerTransport.http,
  }) async {
    await LinkAccessHostConfigStore.write(config);
    await _clients.server.runtimeRemoteLinkService.startListening(
      transport: transport,
    );
  }

  Future<void> stop() {
    return _clients.server.runtimeRemoteLinkService.stopListening();
  }
}
