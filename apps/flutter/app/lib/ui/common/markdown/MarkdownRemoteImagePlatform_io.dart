// ignore_for_file: file_names

import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../../core/logging/ClientLogger.dart';

class MarkdownRemoteImagePlatform extends StatefulWidget {
  const MarkdownRemoteImagePlatform({
    super.key,
    required this.url,
    required this.alt,
    required this.fit,
    this.cacheWidth,
    this.cacheHeight,
    required this.loading,
    required this.error,
  });

  final String url;
  final String alt;
  final BoxFit fit;
  final int? cacheWidth;
  final int? cacheHeight;
  final Widget loading;
  final Widget error;

  @override
  State<MarkdownRemoteImagePlatform> createState() =>
      _MarkdownRemoteImagePlatformState();
}

class _MarkdownRemoteImagePlatformState
    extends State<MarkdownRemoteImagePlatform> {
  int _requestId = 0;
  HttpClientRequest? _request;
  Uint8List? _bytes;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(covariant MarkdownRemoteImagePlatform oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.url != widget.url) {
      _bytes = null;
      _failed = false;
      _load();
    }
  }

  @override
  void dispose() {
    _requestId++;
    _request?.abort();
    _request = null;
    super.dispose();
  }

  Future<void> _load() async {
    final requestId = ++_requestId;
    _request?.abort();
    _request = null;
    await Future<void>.value();
    if (!mounted || requestId != _requestId) {
      return;
    }
    try {
      final uri = Uri.parse(widget.url);
      if (uri.scheme != 'http' && uri.scheme != 'https') {
        throw FormatException('Unsupported image URL ${widget.url}');
      }
      final request = await _markdownImageClient.getUrl(uri);
      if (!mounted || requestId != _requestId) {
        request.abort();
        return;
      }
      _request = request;
      final bytes = await _readMarkdownImage(request, uri);
      if (!mounted || requestId != _requestId) {
        return;
      }
      setState(() {
        _bytes = bytes;
      });
    } catch (error, stackTrace) {
      if (!mounted || requestId != _requestId) {
        return;
      }
      ClientLogger.w(
        'Cannot load markdown image: ${widget.url}',
        tag: 'MarkdownImage',
        error: error,
        stackTrace: stackTrace,
      );
      setState(() {
        _failed = true;
      });
    } finally {
      if (requestId == _requestId) {
        _request = null;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_failed) {
      return widget.error;
    }
    final bytes = _bytes;
    if (bytes == null) {
      return widget.loading;
    }
    return Image.memory(
      bytes,
      fit: widget.fit,
      cacheWidth: widget.cacheWidth,
      cacheHeight: widget.cacheHeight,
      gaplessPlayback: true,
      semanticLabel: widget.alt.trim().isEmpty ? null : widget.alt.trim(),
      errorBuilder: (context, error, stackTrace) => widget.error,
    );
  }
}

const int _maxMarkdownImageBytes = 16 * 1024 * 1024;
const Duration _markdownImageBodyTimeout = Duration(seconds: 30);

final HttpClient _markdownImageClient = HttpClient()
  ..connectionTimeout = const Duration(seconds: 20)
  ..connectionFactory = _connectMarkdownImage;

/// Opens an IPv4 socket and, for HTTPS, finishes TLS with the original host as
/// SNI. HttpClient only speaks HTTP/1.1, so the handshake must not select h2.
Future<ConnectionTask<Socket>> _connectMarkdownImage(
  Uri uri,
  String? proxyHost,
  int? proxyPort,
) async {
  if (proxyHost != null) {
    return Socket.startConnect(proxyHost, proxyPort ?? uri.port);
  }
  final addresses = await InternetAddress.lookup(
    uri.host,
    type: InternetAddressType.IPv4,
  );
  if (addresses.isEmpty) {
    throw SocketException('No IPv4 address for ${uri.host}');
  }
  final address = addresses.first;
  if (uri.scheme != 'https') {
    return Socket.startConnect(address, uri.port);
  }
  final raw = await Socket.startConnect(address, uri.port);
  final Future<Socket> secure = raw.socket.then((socket) {
    return SecureSocket.secure(
      socket,
      host: uri.host,
      supportedProtocols: const <String>['http/1.1'],
    );
  });
  return ConnectionTask.fromSocket<Socket>(secure, raw.cancel);
}

Future<Uint8List> _readMarkdownImage(HttpClientRequest request, Uri uri) async {
  final timer = Timer(_markdownImageBodyTimeout, () {
    request.abort(HttpException('Image download timed out', uri: uri));
  });
  var completed = false;
  try {
    final bytes = await _collectMarkdownImage(request, uri);
    completed = true;
    return bytes;
  } on Exception {
    if (!completed) {
      request.abort();
    }
    rethrow;
  } finally {
    timer.cancel();
  }
}

Future<Uint8List> _collectMarkdownImage(HttpClientRequest request, Uri uri) async {
  final response = await request.close();
  if (response.statusCode != HttpStatus.ok) {
    await response.drain<List<int>>();
    throw HttpException(
      'Image request failed with status ${response.statusCode}',
      uri: uri,
    );
  }
  if (response.contentLength > _maxMarkdownImageBytes) {
    await response.drain<List<int>>();
    throw HttpException('Image exceeded size limit', uri: uri);
  }
  final builder = BytesBuilder(copy: false);
  await for (final List<int> chunk in response) {
    if (builder.length + chunk.length > _maxMarkdownImageBytes) {
      throw HttpException('Image exceeded size limit', uri: uri);
    }
    builder.add(chunk);
  }
  final bytes = builder.takeBytes();
  if (bytes.isEmpty) {
    throw HttpException('Image response was empty', uri: uri);
  }
  return bytes;
}
