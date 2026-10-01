// ignore_for_file: file_names
import '../bridge/ProxyCoreRuntimeBridge.dart';
import '../proxy/generated/CoreProxyClients.g.dart';
import '../proxy/generated/CoreProxyModels.g.dart' as generated;

/// 只调用类型化 runtime 服务；不构造 Call，不计算凭证，不实现握手。
class RemotePairingBridge {
  const RemotePairingBridge();
  static const GeneratedCoreProxyClients _clients = GeneratedCoreProxyClients(ProxyCoreRuntimeBridge());

  Future<generated.PendingPairing> start({
    required String endpoint, String nodeId = '', String? token,
    generated.PeerTransport transport = generated.PeerTransport.http,
  }) {
    return _clients.server.runtimeRemoteLinkService.startPairing(
      nodeId: nodeId, address: endpoint, transport: transport, token: token,
    );
  }
  Future<generated.PairedPeer> finish({required String pairingId, required String pairingCode}) {
    return _clients.server.runtimeRemoteLinkService.finishPairing(
      pairingId: pairingId, confirmationCode: pairingCode,
    );
  }
  Future<void> cancel(String pairingId) {
    return _clients.server.runtimeRemoteLinkService.cancelPairing(pairingId: pairingId);
  }
  Future<List<generated.DiscoveredPeer>> discover({int timeoutMs = 2000}) {
    return _clients.server.runtimeRemoteLinkService.discoverPeers(timeoutMs: timeoutMs);
  }
}
