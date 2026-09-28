import 'dart:async';
import 'dart:typed_data';

import 'package:msgpack_dart/msgpack_dart.dart' as msgpack;

import 'host.dart';

/// One decoded Core watch event returned by the SDK IPC session.
final class PluginSdkEvent {
  const PluginSdkEvent({
    required this.requestId,
    required this.target,
    required this.propertyName,
    required this.kind,
    required this.value,
  });

  final String? requestId;
  final String target;
  final String propertyName;
  final String kind;
  final Object? value;
}

/// One caller-owned Core push stream.
abstract interface class PluginSdkPushSink {
  /// Sends one ordered Link value to Core.
  Future<void> add(Object? value);

  /// Closes the Core push stream.
  Future<void> close();
}

/// A protocol error returned by the Core Link IPC session.
final class PluginSdkError implements Exception {
  const PluginSdkError(this.code, this.message);

  final String code;
  final String message;

  @override
  String toString() => '$code: $message';
}

/// Owns a MessagePack session over the built-in platform host.
final class PluginSdkIpcConnection {
  PluginSdkIpcConnection._(this._host) {
    _host.messages.listen(_onBytes, onDone: _onClosed, onError: _onHostError);
  }

  final PluginSdkHost _host;
  bool _closed = false;
  final Map<String, Completer<Map<String, Object?>>> _calls =
      <String, Completer<Map<String, Object?>>>{};
  final Map<String, StreamController<PluginSdkEvent>> _watches =
      <String, StreamController<PluginSdkEvent>>{};
  final Map<String, Completer<void>> _watchOpens = <String, Completer<void>>{};
  final Map<String, Completer<void>> _pushOpens = <String, Completer<void>>{};
  final Map<String, Completer<void>> _pushItems = <String, Completer<void>>{};
  final Map<String, Completer<void>> _pushCloses = <String, Completer<void>>{};
  int _nextRequest = 1;

  /// Activates Operit and opens the platform carrier supplied with the SDK.
  static Future<PluginSdkIpcConnection> connect({
    String nativeLibraryPath = 'liboperit_plugin_sdk.so',
  }) async {
    return PluginSdkIpcConnection._(
      await PluginSdkHost.connect(nativeLibraryPath),
    );
  }

  /// Closes the native session and fails outstanding protocol operations.
  void close() {
    _host.close();
    _onClosed();
  }

  /// Allocates one session-local request id.
  String nextRequestId() => 'plugin-sdk-${_nextRequest++}';

  /// Sends one canonical Link envelope.
  void _send(Map<String, Object?> message) {
    final payload = msgpack.serialize(message);
    if (_closed) {
      _fail(const PluginSdkError('IPC_CLOSED', 'Plugin SDK connection closed'));
      return;
    }
    _host.send(payload);
  }

  /// Reads one Core call response.
  Future<Map<String, Object?>> call(
    String target,
    String methodName,
    Object? args,
  ) {
    final requestId = nextRequestId();
    final completer = Completer<Map<String, Object?>>();
    _calls[requestId] = completer;
    _send(<String, Object?>{
      'type': 'Call',
      'body': <String, Object?>{
        'requestId': requestId,
        'target': target,
        'methodName': methodName,
        'args': args,
      },
    });
    return completer.future;
  }

  /// Opens one Core watch stream.
  Stream<PluginSdkEvent> watch(
    String target,
    String propertyName,
    Object? args,
  ) {
    final requestId = nextRequestId();
    final controller = StreamController<PluginSdkEvent>();
    _watches[requestId] = controller;
    final open = Completer<void>();
    _watchOpens[requestId] = open;
    open.future.catchError((Object error) {
      controller.addError(error);
      controller.close();
    });
    _send(<String, Object?>{
      'type': 'WatchOpen',
      'body': <String, Object?>{
        'subscriptionId': requestId,
        'request': <String, Object?>{
          'requestId': requestId,
          'target': target,
          'propertyName': propertyName,
          'args': args,
        },
      },
    });
    controller.onCancel = () {
      _watches.remove(requestId);
      _send(<String, Object?>{
        'type': 'WatchClose',
        'body': <String, Object?>{'subscriptionId': requestId, 'error': null},
      });
    };
    return controller.stream;
  }

  /// Opens one Core push stream.
  Future<PluginSdkPushSink> push(
    String target,
    String methodName,
    Object? args,
  ) async {
    final pushId = nextRequestId();
    final open = Completer<void>();
    _pushOpens[pushId] = open;
    _send(<String, Object?>{
      'type': 'PushOpen',
      'body': <String, Object?>{
        'requestId': pushId,
        'target': target,
        'methodName': methodName,
        'args': args ?? const <String, Object?>{},
      },
    });
    await open.future;
    return _PushSink(this, pushId);
  }

  /// Sends one push item and waits for its acknowledgement.
  Future<void> _push(String pushId, int sequence, Object? value) {
    final key = '$pushId:$sequence';
    final completer = Completer<void>();
    _pushItems[key] = completer;
    _send(<String, Object?>{
      'type': 'PushItem',
      'body': <String, Object?>{
        'pushId': pushId,
        'sequence': sequence,
        'args': value,
      },
    });
    return completer.future;
  }

  /// Closes one push stream and waits for its acknowledgement.
  Future<void> _closePush(String pushId) {
    final completer = Completer<void>();
    _pushCloses[pushId] = completer;
    _send(<String, Object?>{
      'type': 'PushClose',
      'body': <String, Object?>{'pushId': pushId},
    });
    return completer.future;
  }

  /// Decodes one envelope already framed and validated by the platform host.
  void _onBytes(Uint8List bytes) {
    try {
      _onMessage(
        _decodeValue(msgpack.deserialize(bytes)) as Map<String, Object?>,
      );
    } catch (error) {
      _onHostError(error, StackTrace.current);
    }
  }

  /// Restores string-keyed Link maps recursively for generated typed model decoders.
  Object? _decodeValue(Object? value) {
    if (value is Map) {
      return value.map<String, Object?>(
        (key, item) => MapEntry(key as String, _decodeValue(item)),
      );
    }
    if (value is List) {
      return value.map(_decodeValue).toList(growable: false);
    }
    return value;
  }

  /// Routes one decoded envelope to its pending operation.
  void _onMessage(Map<dynamic, dynamic> message) {
    final type = message['type'];
    final body = _map(message['body']);
    switch (type) {
      case 'CallResponse':
        final requestId = _requestId(body['requestId']);
        final completer = _calls.remove(requestId);
        if (completer == null) return;
        final result = body['result'];
        if (result case {'Err': final error}) {
          completer.completeError(_error(error));
        } else {
          completer.complete(<String, Object?>{'value': _resultValue(result)});
        }
      case 'WatchOpened':
        final completer = _watchOpens.remove(body['subscriptionId']);
        if (completer == null) return;
        _completeResult(completer, body['result']);
      case 'WatchEvent':
        final controller = _watches[body['subscriptionId']];
        if (controller == null) return;
        final event = _map(body['event']);
        controller.add(
          PluginSdkEvent(
            requestId: _requestIdOrNull(event['requestId']),
            target: event['target'] as String,
            propertyName: event['propertyName'] as String,
            kind: event['kind'].toString(),
            value: event['value'],
          ),
        );
      case 'WatchClose':
        final id = body['subscriptionId'];
        _watchOpens.remove(id)?.completeError(_error(body['error']));
        _watches.remove(id)?.close();
      case 'PushOpened':
        final completer = _pushOpens.remove(body['pushId']);
        if (completer == null) return;
        _completeResult(completer, body['result']);
      case 'PushItemResult':
        final key = '${body['pushId']}:${body['sequence']}';
        final completer = _pushItems.remove(key);
        if (completer == null) return;
        _completeResult(completer, body['result']);
      case 'PushClosed':
        final completer = _pushCloses.remove(body['pushId']);
        if (completer == null) return;
        _completeResult(completer, body['result']);
      case 'ProtocolError':
        throw _error(body['error']);
    }
  }

  /// Handles host closure by failing pending operations.
  void _onClosed() {
    _closed = true;
    _fail(const PluginSdkError('IPC_CLOSED', 'Plugin SDK connection closed'));
  }

  /// Fails and removes every pending operation when its carrier is unavailable.
  void _fail(Object error) {
    for (final completer in _calls.values) {
      completer.completeError(error);
    }
    for (final completer in _watchOpens.values) {
      completer.completeError(error);
    }
    for (final table in [_pushOpens, _pushItems, _pushCloses]) {
      for (final completer in table.values) {
        completer.completeError(error);
      }
      table.clear();
    }
    final watches = _watches.values.toList(growable: false);
    _watches.clear();
    for (final controller in watches) {
      controller.addError(error);
      controller.close();
    }
    _calls.clear();
    _watchOpens.clear();
  }

  /// Reports the actual platform activation or carrier failure.
  void _onHostError(Object error, StackTrace stack) {
    _closed = true;
    _fail(error);
    _host.close();
  }

  /// Reads a protocol map without coercing unknown payloads.
  Map<String, Object?> _map(Object? value) {
    if (value is! Map)
      throw const PluginSdkError('INVALID_FRAME', 'IPC body is not a map');
    return value.map((key, item) => MapEntry(key.toString(), item));
  }

  /// Extracts the Rust newtype request id representation.
  String _requestId(Object? value) {
    if (value is String) return value;
    throw const PluginSdkError('INVALID_FRAME', 'request id is missing');
  }

  /// Extracts an optional request id.
  String? _requestIdOrNull(Object? value) =>
      value == null ? null : _requestId(value);

  /// Decodes one Result payload.
  Object? _resultValue(Object? result) {
    if (result case {'Ok': final value}) return value;
    return result;
  }

  /// Completes one Result<void> payload.
  void _completeResult(Completer<void> completer, Object? result) {
    if (result case {'Err': final error}) {
      completer.completeError(_error(error));
    } else {
      completer.complete();
    }
  }

  /// Converts a serialized Core link error into the SDK exception.
  PluginSdkError _error(Object? value) {
    final map = _map(value);
    return PluginSdkError(map['code'] as String, map['message'] as String);
  }
}

/// Implements one ordered caller-owned push stream.
final class _PushSink implements PluginSdkPushSink {
  _PushSink(this._connection, this._pushId);

  final PluginSdkIpcConnection _connection;
  final String _pushId;
  int _sequence = 0;

  /// Sends one value into the Core push stream.
  @override
  Future<void> add(Object? value) =>
      _connection._push(_pushId, _sequence++, value);

  /// Closes the Core push stream.
  @override
  Future<void> close() => _connection._closePush(_pushId);
}
