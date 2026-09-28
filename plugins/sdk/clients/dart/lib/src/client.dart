import 'transport.dart';

/// Concrete external Plugin SDK client backed by the built-in IPC connection.
final class OperitPluginSdkClient {
  const OperitPluginSdkClient._(this._connection);

  final PluginSdkIpcConnection _connection;

  /// Activates Operit and connects through the SDK's packaged platform host.
  static Future<OperitPluginSdkClient> connect({
    String nativeLibraryPath = 'liboperit_plugin_sdk.so',
  }) async {
    return OperitPluginSdkClient._(
      await PluginSdkIpcConnection.connect(
        nativeLibraryPath: nativeLibraryPath,
      ),
    );
  }

  /// Releases the SDK session and its pending operations.
  void close() => _connection.close();

  /// Calls one generated Core object method through Link IPC.
  Future<Object?> call(
    String target,
    String methodName,
    Object? args,
  ) async {
    final response = await _connection.call(target, methodName, args);
    return response['value'];
  }

  /// Calls one route and decodes its typed result.
  Future<T> callTyped<T>(
    String target,
    String methodName,
    Object? args,
    T Function(Object? value) decode,
  ) async {
    final value = await call(target, methodName, args);
    return decode(value);
  }

  /// Watches one generated Core object property through Link IPC.
  Stream<PluginSdkEvent> watch(
    String target,
    String propertyName,
    Object? args,
  ) => _connection.watch(target, propertyName, args);

  /// Watches one route and decodes each event value.
  Stream<T> watchTyped<T>(
    String target,
    String propertyName,
    Object? args,
    T Function(Object? value) decode,
  ) => watch(
    target,
    propertyName,
    args,
  ).map((event) => decode(event.value));

  /// Opens one generated caller-owned Core input stream through Link IPC.
  Future<PluginSdkPushSink> push(
    String target,
    String methodName,
    Object? args,
  ) => _connection.push(target, methodName, args);
}
