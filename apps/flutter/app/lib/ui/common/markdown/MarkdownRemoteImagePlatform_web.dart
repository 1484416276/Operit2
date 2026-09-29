// ignore_for_file: file_names

import 'package:flutter/material.dart';

class MarkdownRemoteImagePlatform extends StatelessWidget {
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
  Widget build(BuildContext context) {
    return Image.network(
      url,
      fit: fit,
      cacheWidth: cacheWidth,
      cacheHeight: cacheHeight,
      // The image element can display a response the page is not allowed to read.
      webHtmlElementStrategy: WebHtmlElementStrategy.prefer,
      semanticLabel: alt.trim().isEmpty ? null : alt.trim(),
      loadingBuilder: (context, child, loadingProgress) {
        if (loadingProgress == null) {
          return child;
        }
        return loading;
      },
      errorBuilder: (context, error, stackTrace) => this.error,
    );
  }
}
