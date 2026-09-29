// ignore_for_file: file_names

import 'package:flutter/material.dart';

import 'MarkdownAudioRenderer.dart';
import 'MarkdownRemoteImage.dart';
import 'MarkdownVideoRenderer.dart';

class MarkdownImageRenderer extends StatelessWidget {
  const MarkdownImageRenderer({
    super.key,
    required this.imageMarkdown,
    required this.textColor,
    this.maxImageHeight = 140,
    this.onOpenLink,
  });

  final String imageMarkdown;
  final Color textColor;
  final double maxImageHeight;
  final void Function(String url)? onOpenLink;

  @override
  Widget build(BuildContext context) {
    if (!isCompleteImageMarkdown(imageMarkdown)) {
      return SelectableText(
        imageMarkdown,
        style: Theme.of(
          context,
        ).textTheme.bodyMedium?.copyWith(color: textColor, height: 1.3),
      );
    }
    final imageAlt = extractMarkdownImageAlt(imageMarkdown);
    final imageUrl = extractMarkdownImageUrl(imageMarkdown);
    if (isLikelyVideoUrl(imageUrl)) {
      return MarkdownVideoRenderer(
        videoMarkdown: imageMarkdown,
        textColor: textColor,
        maxVideoHeight: maxImageHeight,
      );
    }
    if (isLikelyAudioUrl(imageUrl)) {
      return MarkdownAudioRenderer(
        audioMarkdown: imageMarkdown,
        textColor: textColor,
      );
    }
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(8),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: maxImageHeight),
          child: _MarkdownImageBody(
            imageUrl: imageUrl,
            imageAlt: imageAlt,
            onOpenLink: onOpenLink,
          ),
        ),
      ),
    );
  }
}

class _MarkdownImageBody extends StatelessWidget {
  const _MarkdownImageBody({
    required this.imageUrl,
    required this.imageAlt,
    required this.onOpenLink,
  });

  final String imageUrl;
  final String imageAlt;
  final void Function(String url)? onOpenLink;

  @override
  Widget build(BuildContext context) {
    return MarkdownRemoteImage(
      url: imageUrl,
      alt: imageAlt,
      onOpenLink: onOpenLink,
    );
  }
}

bool isCompleteImageMarkdown(String content) {
  return RegExp(r'^!\[[^\]]*\]\([^)]+\)$').hasMatch(content.trim());
}

String extractMarkdownImageAlt(String imageContent) {
  return RegExp(r'^!\[([^\]]*)\]').firstMatch(imageContent.trim())?.group(1) ??
      '';
}

String extractMarkdownImageUrl(String imageContent) {
  final destination =
      RegExp(r'\]\(([^)]+)\)$').firstMatch(imageContent.trim())?.group(1) ??
      '';
  return destination
      .replaceFirst(RegExp(r"""\s+(?:"[^"]*"|'[^']*'|\([^)]*\))\s*$"""), '')
      .trim();
}


