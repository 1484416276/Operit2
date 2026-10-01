import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/proxy/generated/CoreProxyModels.g.dart';
import 'package:operit2/core/runtime/PeerEndpointTransport.dart';

void main() {
  test('discovery endpoints select their own carrier instead of forcing HTTP', () {
    final endpoints = {
      'http://127.0.0.1:37194/link': PeerTransport.http,
      'https://host/link': PeerTransport.http,
      'ws://127.0.0.1:37194/link': PeerTransport.webSocket,
      'wss://host/link': PeerTransport.webSocket,
      '127.0.0.1:37194': PeerTransport.tcp,
      '[::1]:37194': PeerTransport.tcp,
      'host.local:37194': PeerTransport.tcp,
      'serial://COM27': PeerTransport.serial,
      'bluetooth://device': PeerTransport.bluetooth,
    };
    for (final entry in endpoints.entries) {
      expect(peerEndpointTransport(entry.key), entry.value, reason: entry.key);
    }
  });
  test('unknown endpoints do not silently fall back to HTTP', () {
    expect(() => peerEndpointTransport('ftp://host'), throwsFormatException);
    expect(() => peerEndpointTransport('invalid'), throwsFormatException);
  });
}
