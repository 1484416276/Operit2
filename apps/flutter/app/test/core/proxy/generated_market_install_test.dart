import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';

/// Verifies the generated marketplace install API matches the Core contract.
void main() {
  test(
    'market install forwards asset metadata to the Core application',
    () async {
      const result = 'Successfully imported package: market-package';
      final bridge = _MarketInstallBridge(encodeCoreLink(<Object?>[0, result]));
      final sha256 = List<String>.filled(64, 'a').join();

      final actual = await GeneratedCoreProxyClients(bridge).application
          .installMarketArtifact(
            assetId: 'asset-1',
            fileName: 'market-package.zip',
            expectedSha256: sha256,
          );

      expect(actual, result);
      final request = bridge.requests.single;
      expect(request.target, 'core/application');
      expect(request.methodName, 'installMarketArtifact');
      expect(request.args, <String, Object?>{
        'assetId': 'asset-1',
        'fileName': 'market-package.zip',
        'expectedSha256': sha256,
      });
    },
  );

  test('market install propagates Core errors unchanged', () async {
    final bridge = _MarketInstallBridge(
      encodeCoreLink(<Object?>[
        1,
        'INSTALL_FAILED',
        'Market artifact SHA-256 is invalid',
        null,
        null,
        null,
      ]),
    );

    await expectLater(
      GeneratedCoreProxyClients(bridge).application.installMarketArtifact(
        assetId: 'asset-1',
        fileName: 'market-package.zip',
        expectedSha256: 'invalid',
      ),
      throwsA(
        isA<CoreLinkError>()
            .having((error) => error.code, 'code', 'INSTALL_FAILED')
            .having(
              (error) => error.message,
              'message',
              'Market artifact SHA-256 is invalid',
            ),
      ),
    );
    expect(bridge.requests, hasLength(1));
  });
}

/// Captures generated calls and returns a fixed Core protocol response.
class _MarketInstallBridge extends OperitRuntimeBridge {
  /// Creates a bridge with the encoded response for each install call.
  _MarketInstallBridge(this.response);

  final Uint8List response;
  final List<CoreCallRequest> requests = <CoreCallRequest>[];

  /// Records the request without downloading or installing a real artifact.
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    requests.add(request);
    return response;
  }

  /// Rejects push streams because marketplace installation uses a single call.
  @override
  Future<CorePushSink> push(CorePushRequest request) {
    throw UnsupportedError('Push streams are not used by this test');
  }

  /// Rejects watch snapshots because marketplace installation has no watch API.
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) {
    throw UnsupportedError('Watch snapshots are not used by this test');
  }

  /// Rejects watch streams because marketplace installation has no watch API.
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) {
    throw UnsupportedError('Watch streams are not used by this test');
  }
}
