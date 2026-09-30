import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/ui/common/layout/ApplicationZoomPreferences.dart';

/// Checks strict interface zoom persistence through the existing Core API.
void main() {
  test('new installations start at 100 percent', () async {
    final bridge = _ZoomPreferenceBridge();
    final preferences = ApplicationZoomPreferences(
      clients: GeneratedCoreProxyClients(bridge),
    );
    expect(await preferences.load(), 1);
  });

  test('saved zoom survives preference store recreation', () async {
    final bridge = _ZoomPreferenceBridge();
    final clients = GeneratedCoreProxyClients(bridge);
    await ApplicationZoomPreferences(clients: clients).save(1.3);
    expect(await ApplicationZoomPreferences(clients: clients).load(), 1.3);
    expect(bridge.values, <String, String>{'zoom': '1.3'});
  });

  test('malformed and unsupported saved values surface errors', () async {
    final bridge = _ZoomPreferenceBridge();
    final preferences = ApplicationZoomPreferences(
      clients: GeneratedCoreProxyClients(bridge),
    );
    for (final value in <String>['broken', 'NaN', '0', '0.95', '2.0']) {
      bridge.values['zoom'] = value;
      await expectLater(preferences.load(), throwsFormatException);
    }
    await expectLater(
      Future<void>.sync(() => preferences.save(0.95)),
      throwsArgumentError,
    );
  });

  test('preference transport failures are not concealed', () async {
    final bridge = _ZoomPreferenceBridge()..fail = true;
    final preferences = ApplicationZoomPreferences(
      clients: GeneratedCoreProxyClients(bridge),
    );
    await expectLater(preferences.load(), throwsStateError);
    await expectLater(preferences.save(1.1), throwsStateError);
  });
}

class _ZoomPreferenceBridge extends OperitRuntimeBridge {
  final Map<String, String> values = <String, String>{};
  bool fail = false;

  /// Implements only the exact Core preference calls used by interface zoom.
  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    if (fail) {
      throw StateError('Preference transport failed');
    }
    final args = request.args as Map;
    expect(args['fileName'], 'application_zoom.preferences.json');
    switch (request.methodName) {
      case 'getPreferences':
        expect(args['keys'], <String>['zoom']);
        return encodeCoreLink(<Object?>[0, values]);
      case 'setPreferences':
        values.addAll((args['values'] as Map).cast<String, String>());
        return encodeCoreLink(<Object?>[0, null]);
      default:
        throw StateError('Unexpected Core call: ${request.methodName}');
    }
  }

  /// Rejects streams unrelated to zoom preferences.
  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();

  /// Rejects snapshots unrelated to zoom preferences.
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();

  /// Rejects watches unrelated to zoom preferences.
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw UnimplementedError();
}
