import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/services.dart';

/// A request for an application-owned virtual origin (not arbitrary web traffic).
class WebViewLocalResourceRequest {
  const WebViewLocalResourceRequest({
    required this.url,
    this.method = 'GET',
    this.headers = const {},
    this.isMainFrame = false,
  });
  final String url;
  final String method;
  final Map<String, String> headers;
  final bool isMainFrame;
}

/// Bytes returned to the native loader without opening a listening socket.
class WebViewLocalResourceResponse {
  WebViewLocalResourceResponse({
    required this.body,
    this.statusCode = 200,
    this.reasonPhrase = 'OK',
    this.mimeType = 'application/octet-stream',
    this.encoding = 'utf-8',
    this.headers = const {},
  });
  final Uint8List body;
  final int statusCode;
  final String reasonPhrase;
  final String mimeType;
  final String encoding;
  final Map<String, String> headers;

  Map<String, Object> toMessage() => {
    'body': body,
    'statusCode': statusCode,
    'reasonPhrase': reasonPhrase,
    'mimeType': mimeType,
    'encoding': encoding,
    'headers': headers,
  };
}

typedef WebViewLocalResourceHandler =
    Future<WebViewLocalResourceResponse> Function(
      WebViewLocalResourceRequest request,
    );

/// Shared wire protocol used by the native plugins. Registrations are per view,
/// so two instances can serve different bytes without changing their origin.
class WebViewLocalResourceBridge {
  static const channel = MethodChannel('operit/webview_resources');
  static final Map<int, WebViewLocalResourceHandler> _handlers = {};
  static bool _listening = false;

  static Future<void> setHandler(
    int identifier,
    WebViewLocalResourceHandler? handler,
  ) async {
    if (!_listening) {
      channel.setMethodCallHandler(_handle);
      _listening = true;
    }
    final previous = _handlers[identifier];
    if (handler == null) {
      _handlers.remove(identifier);
    } else {
      _handlers[identifier] = handler;
    }
    try {
      await channel.invokeMethod<void>('configure', {
        'identifier': identifier,
        'enabled': handler != null,
      });
    } catch (_) {
      if (previous != null) {
        _handlers[identifier] = previous;
      } else {
        _handlers.remove(identifier);
      }
      rethrow;
    }
  }

  static Future<Object> _handle(MethodCall call) async {
    if (call.method != 'request') throw MissingPluginException();
    final args = (call.arguments as Map).cast<String, Object?>();
    final handler = _handlers[args['identifier']];
    if (handler == null) return _failure(410, 'Gone');
    try {
      final response = await handler(
        WebViewLocalResourceRequest(
          url: args['url']! as String,
          method: args['method'] as String? ?? 'GET',
          headers:
              (args['headers'] as Map?)?.cast<String, String>() ?? const {},
          isMainFrame: args['isMainFrame'] == true,
        ),
      ).timeout(const Duration(seconds: 30));
      return response.toMessage();
    } catch (_) {
      return _failure(500, 'Resource Error');
    }
  }

  static Map<String, Object> _failure(int status, String reason) =>
      WebViewLocalResourceResponse(
        body: Uint8List(0),
        statusCode: status,
        reasonPhrase: reason,
      ).toMessage();
}
