// ignore_for_file: file_names

import 'dart:convert';

import 'package:crypto/crypto.dart';

import '../bridge/PlatformCoreProxy.dart';
import '../bridge/ProxyCoreRuntimeBridge.dart';
import '../proxy/generated/CoreProxyClients.g.dart';
import '../proxy/generated/CoreProxyModels.g.dart' as generated;
import 'RuntimeDeviceInfoProvider.dart';

class RemotePairingBridge {
  /// Creates a bridge that forwards pairing actions to the local runtime.
  const RemotePairingBridge();

  static const GeneratedCoreProxyClients _clients = GeneratedCoreProxyClients(
    ProxyCoreRuntimeBridge(coreProxy: platformCoreProxy),
  );

  /// Starts one runtime-owned pairing after hashing the user-supplied Link token.
  Future<RemotePairStartResult> startWithToken({
    required String endpoint,
    required String token,
  }) {
    return startWithTokenHash(
      endpoint: endpoint,
      tokenHash: _linkTokenHash(token),
    );
  }

  /// Starts one runtime-owned pairing using an already-derived Link token hash.
  Future<RemotePairStartResult> startWithTokenHash({
    required String endpoint,
    required String tokenHash,
  }) async {
    final clientDeviceInfo = await RuntimeDeviceInfoProvider.current();
    final result = await _clients.server.runtimeRemoteLinkService
        .startPairedRemote(
          endpoint: endpoint,
          tokenHash: tokenHash,
          clientDeviceInfo: clientDeviceInfo,
        );
    return RemotePairStartResult(
      pairingId: result.pairingId,
      pairingServiceVersion: result.pairingServiceVersion,
      peerNodeId: result.peerNodeId,
      peerDeviceInfo: result.peerDeviceInfo,
      coreUserName: result.coreUserName,
    );
  }

  /// Completes one runtime-owned pairing and stores the named remote runtime.
  Future<generated.PairedPeerSessionRecord> finish({
    required String pairingId,
    required String pairingCode,
    required String name,
    generated.PeerTransport? transport,
  }) async {
    final session = await _clients.server.runtimeRemoteLinkService
        .finishPairedRemote(
          pairingId: pairingId,
          pairingCode: pairingCode,
          name: name,
        );
    if (transport == null || session.transport == transport) {
      return session;
    }
    return _clients.server.runtimeRemoteLinkService.setPairedRemoteTransport(
      name: name,
      transport: transport,
    );
  }

  /// Bootstraps one Web Access pairing from the URL token and stores it locally.
  Future<generated.PairedPeerSessionRecord> bootstrap({
    required String endpoint,
    required String token,
  }) async {
    final clientDeviceInfo = await RuntimeDeviceInfoProvider.current();
    return _clients.server.runtimeRemoteLinkService.bootstrapPairedRemote(
      endpoint: endpoint,
      tokenHash: _linkTokenHash(token),
      clientDeviceInfo: clientDeviceInfo,
    );
  }


}

/// Derives the Link protocol token hash from the user-provided secret.
String _linkTokenHash(String token) {
  return base64Encode(sha256.convert(utf8.encode(token)).bytes);
}

/// Builds one stable local session key for a completed remote pairing.
String remotePairingSessionName(RemotePairStartResult pairing) {
  return '${pairing.peerDeviceInfo.platform}-${pairing.peerDeviceInfo.model}-${pairing.peerNodeId}';
}

/// Builds the stable local session key from a persisted remote session record.
String remotePairingSessionNameFromRecord(
  generated.PairedPeerSessionRecord session,
) {
  return '${session.peerDeviceInfo.platform}-${session.peerDeviceInfo.model}-${session.peerNodeId}';
}

class RemotePairStartResult {
  /// Creates the UI representation of a runtime-owned pairing start result.
  const RemotePairStartResult({
    required this.pairingId,
    required this.pairingServiceVersion,
    required this.peerNodeId,
    required this.peerDeviceInfo,
    required this.coreUserName,
  });

  /// Decodes a pairing start result received through a Core Link response.
  factory RemotePairStartResult.fromJson(Map<String, Object?> json) {
    return RemotePairStartResult(
      pairingId: json['pairingId'] as String,
      pairingServiceVersion: json['pairingServiceVersion'] as int,
      peerNodeId: json['peerNodeId'] as String,
      peerDeviceInfo: generated.LinkDeviceInfo.fromJson(
        json['peerDeviceInfo'] as Map<String, Object?>,
      ),
      coreUserName: json['coreUserName'] as String,
    );
  }

  final String pairingId;
  final int pairingServiceVersion;
  final String peerNodeId;
  final generated.LinkDeviceInfo peerDeviceInfo;
  final String coreUserName;
}
