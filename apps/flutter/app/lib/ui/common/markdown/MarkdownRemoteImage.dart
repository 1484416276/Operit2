// ignore_for_file: file_names

import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../interactions/MessagePressShield.dart';
import 'MarkdownLink.dart';
import 'MarkdownRemoteImagePlatform.dart';

class MarkdownRemoteImage extends StatelessWidget {
  const MarkdownRemoteImage({
    super.key,
    required this.url,
    this.alt = '',
    this.fit = BoxFit.contain,
    this.onOpenLink,
  });

  final String url;
  final String alt;
  final BoxFit fit;
  final void Function(String url)? onOpenLink;

  @override
  Widget build(BuildContext context) {
    final label = alt.trim().isEmpty ? url : alt.trim();
    final error = _MarkdownImageStatus(
      icon: Icons.broken_image_outlined,
      label: label,
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        final cacheWidth = _markdownImageCacheExtent(
          context,
          constraints.maxWidth,
        );
        final cacheHeight = cacheWidth == null
            ? _markdownImageCacheExtent(context, constraints.maxHeight)
            : null;
        final trimmed = url.trim();
        final isDataImage = trimmed.toLowerCase().startsWith('data:image/');
        final dataBytes = markdownDataImageBytes(trimmed);
        final Widget image;
        if (isDataImage) {
          image = dataBytes == null
              ? error
              : Image.memory(
                  dataBytes,
                  fit: fit,
                  cacheWidth: cacheWidth,
                  cacheHeight: cacheHeight,
                  semanticLabel: alt.trim().isEmpty ? null : alt.trim(),
                  errorBuilder: (context, failure, stackTrace) => error,
                );
        } else {
          image = MarkdownRemoteImagePlatform(
            url: url,
            alt: alt,
            fit: fit,
            cacheWidth: cacheWidth,
            cacheHeight: cacheHeight,
            loading: const _MarkdownImageStatus(icon: null, label: ''),
            error: error,
          );
        }
        if (isDataImage) {
          return image;
        }
        return MessagePressShieldRegion(
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: () => activateMarkdownLink(url, onOpenLink),
            child: image,
          ),
        );
      },
    );
  }
}

int? _markdownImageCacheExtent(BuildContext context, double extent) {
  if (!extent.isFinite || extent <= 0) {
    return null;
  }
  final pixels = (extent * MediaQuery.devicePixelRatioOf(context)).ceil();
  if (pixels <= 0) {
    return null;
  }
  return pixels > 4096 ? 4096 : pixels;
}

class _MarkdownImageStatus extends StatelessWidget {
  const _MarkdownImageStatus({required this.icon, required this.label});

  final IconData? icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.onSurfaceVariant;
    if (icon == null) {
      return const SizedBox(
        height: 72,
        width: double.infinity,
        child: Center(
          child: SizedBox.square(
            dimension: 18,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 4),
      child: Row(
        children: <Widget>[
          Icon(icon, size: 18, color: color),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              label,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: color,
                decoration: TextDecoration.underline,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

Uint8List? markdownDataImageBytes(String imageUrl) {
  final trimmed = imageUrl.trim();
  if (!trimmed.toLowerCase().startsWith('data:image/')) {
    return null;
  }
  try {
    return UriData.parse(trimmed).contentAsBytes();
  } on FormatException {
    return null;
  }
}
