// ignore_for_file: file_names

import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';

import 'MarkdownCodeTypeface.dart';
import 'MarkdownLatexBlock.dart';
import 'MarkdownLink.dart';
import 'MarkdownNodeGrouper.dart';

class MarkdownInlineSegment {
  const MarkdownInlineSegment({
    required this.text,
    this.nodeType,
    this.children = const <MarkdownInlineSegment>[],
  });

  final String text;
  final String? nodeType;
  final List<MarkdownInlineSegment> children;
}

const int maxInlineRenderDepth = 12;

TextSpan buildMarkdownInlineSpannableFromChildren({
  required BuildContext context,
  required List<MarkdownInlineSegment> children,
  required Color textColor,
  required MarkdownLinkGestureOwner gestures,
  TextStyle? baseStyle,
  void Function(String url)? onLinkClick,
}) {
  return TextSpan(
    children: <InlineSpan>[
      for (final child in children)
        appendInlineNode(
          context: context,
          segment: child,
          textColor: textColor,
          baseStyle: baseStyle,
          onLinkClick: onLinkClick,
          gestures: gestures,
        ),
    ],
  );
}

InlineSpan markdownInlineSpan({
  required BuildContext context,
  required MarkdownInlineSegment segment,
  required Color textColor,
  required MarkdownLinkGestureOwner gestures,
  TextStyle? baseStyle,
  void Function(String url)? onLinkClick,
}) {
  return appendInlineNode(
    context: context,
    segment: segment,
    textColor: textColor,
    baseStyle: baseStyle,
    onLinkClick: onLinkClick,
    gestures: gestures,
  );
}

InlineSpan appendInlineNode({
  required BuildContext context,
  required MarkdownInlineSegment segment,
  required Color textColor,
  required MarkdownLinkGestureOwner gestures,
  TextStyle? baseStyle,
  int depth = 0,
  void Function(String url)? onLinkClick,
  String? linkUrl,
}) {
  if (segment.nodeType == 'InlineLatex') {
    return WidgetSpan(
      alignment: PlaceholderAlignment.baseline,
      baseline: TextBaseline.alphabetic,
      child: MarkdownInlineLatex(content: segment.text, textColor: textColor),
    );
  }
  if (segment.nodeType == 'Link') {
    final url = extractLinkUrl(segment.text);
    final linkStyle = markdownInlineStyle(
      context,
      segment.nodeType,
      textColor,
      baseStyle,
    );
    final nestedChildren = resolveNestedInlineChildren(segment);
    if (nestedChildren.isEmpty || depth >= maxInlineRenderDepth) {
      return TextSpan(
        text: extractLinkText(segment.text),
        style: linkStyle,
        recognizer: _markdownLinkRecognizer(url, onLinkClick, gestures),
      );
    }
    return TextSpan(
      style: linkStyle,
      children: <InlineSpan>[
        for (final child in nestedChildren)
          appendInlineNode(
            context: context,
            segment: child,
            textColor: textColor,
            baseStyle: linkStyle,
            depth: depth + 1,
            onLinkClick: onLinkClick,
            linkUrl: url,
            gestures: gestures,
          ),
      ],
    );
  }
  final nestedChildren = _canRenderNested(segment)
      ? resolveNestedInlineChildren(segment)
      : const <MarkdownInlineSegment>[];
  if (nestedChildren.isNotEmpty && depth < maxInlineRenderDepth) {
    return TextSpan(
      style: markdownInlineStyle(
        context,
        segment.nodeType,
        textColor,
        baseStyle,
      ),
      children: <InlineSpan>[
        for (final child in nestedChildren)
          appendInlineNode(
            context: context,
            segment: child,
            textColor: textColor,
            baseStyle: baseStyle,
            depth: depth + 1,
            onLinkClick: onLinkClick,
            linkUrl: linkUrl,
            gestures: gestures,
          ),
      ],
    );
  }
  return _plainInlineSpan(
    context: context,
    segment: segment,
    textColor: textColor,
    baseStyle: baseStyle,
    onLinkClick: onLinkClick,
    linkUrl: linkUrl,
    gestures: gestures,
  );
}

TextSpan _plainInlineSpan({
  required BuildContext context,
  required MarkdownInlineSegment segment,
  required Color textColor,
  required TextStyle? baseStyle,
  required void Function(String url)? onLinkClick,
  required String? linkUrl,
  required MarkdownLinkGestureOwner gestures,
}) {
  final text = resolveNestedInlineText(segment);
  final style = markdownInlineStyle(
    context,
    segment.nodeType,
    textColor,
    baseStyle,
  );
  if (linkUrl != null || segment.nodeType == 'InlineCode') {
    return TextSpan(
      text: text,
      style: style,
      recognizer: linkUrl == null
          ? null
          : _markdownLinkRecognizer(linkUrl, onLinkClick, gestures),
    );
  }
  final pieces = _splitMarkdownAutolinks(text);
  if (pieces.length == 1 && pieces.first.url == null) {
    return TextSpan(text: text, style: style);
  }
  return TextSpan(
    children: <InlineSpan>[
      for (final piece in pieces)
        TextSpan(
          text: piece.text,
          style: piece.url == null
              ? style
              : markdownInlineStyle(context, 'Link', textColor, baseStyle),
          recognizer: piece.url == null
              ? null
              : _markdownLinkRecognizer(piece.url!, onLinkClick, gestures),
        ),
    ],
  );
}

TextSpan buildMarkdownInlineSpannableFromText({
  required BuildContext context,
  required String text,
  required Color textColor,
  required MarkdownLinkGestureOwner gestures,
  TextStyle? baseStyle,
  void Function(String url)? onLinkClick,
}) {
  return buildMarkdownInlineSpannableFromChildren(
    context: context,
    children: parseInlineSegments(text),
    textColor: textColor,
    baseStyle: baseStyle,
    onLinkClick: onLinkClick,
    gestures: gestures,
  );
}

TextSpan buildMarkdownInlineSpannableFromMarkdownNodes({
  required BuildContext context,
  required List<MarkdownNodeStable> children,
  required Color textColor,
  required MarkdownLinkGestureOwner gestures,
  TextStyle? baseStyle,
  void Function(String url)? onLinkClick,
}) {
  return buildMarkdownInlineSpannableFromChildren(
    context: context,
    children: <MarkdownInlineSegment>[
      for (final child in children) _segmentFromMarkdownNode(child),
    ],
    textColor: textColor,
    baseStyle: baseStyle,
    onLinkClick: onLinkClick,
    gestures: gestures,
  );
}

MarkdownInlineSegment _segmentFromMarkdownNode(MarkdownNodeStable node) {
  return MarkdownInlineSegment(
    text: node.content,
    nodeType: _inlineTypeName(node.type),
    children: <MarkdownInlineSegment>[
      for (final child in node.children) _segmentFromMarkdownNode(child),
    ],
  );
}

String? _inlineTypeName(MarkdownNodeType type) {
  return switch (type) {
    MarkdownNodeType.bold => 'Bold',
    MarkdownNodeType.italic => 'Italic',
    MarkdownNodeType.inlineCode => 'InlineCode',
    MarkdownNodeType.link => 'Link',
    MarkdownNodeType.strikethrough => 'Strikethrough',
    MarkdownNodeType.underline => 'Underline',
    MarkdownNodeType.inlineLatex => 'InlineLatex',
    MarkdownNodeType.htmlBreak => 'HtmlBreak',
    _ => null,
  };
}

String resolveNestedInlineText(MarkdownInlineSegment segment) {
  if (segment.nodeType == 'Link') {
    return extractLinkText(segment.text);
  }
  if (segment.nodeType == 'Underline' &&
      segment.text.startsWith('__') &&
      segment.text.endsWith('__') &&
      segment.text.length >= 4) {
    return segment.text.substring(2, segment.text.length - 2);
  }
  if (segment.nodeType == 'HtmlBreak') {
    return '\n';
  }
  return segment.text;
}

List<MarkdownInlineSegment> resolveNestedInlineChildren(
  MarkdownInlineSegment segment,
) {
  if (segment.children.isNotEmpty) {
    return segment.children;
  }
  if (segment.nodeType == 'InlineCode' || segment.nodeType == 'InlineLatex') {
    return const <MarkdownInlineSegment>[];
  }
  final resolvedText = resolveNestedInlineText(segment);
  final parsedChildren = parseInlineSegments(resolvedText);
  if (_isSingleSelfReference(segment, resolvedText, parsedChildren)) {
    return const <MarkdownInlineSegment>[];
  }
  return parsedChildren;
}

bool _canRenderNested(MarkdownInlineSegment segment) {
  return segment.nodeType == 'Bold' ||
      segment.nodeType == 'Italic' ||
      segment.nodeType == 'Strikethrough' ||
      segment.nodeType == 'Underline';
}

bool _isSingleSelfReference(
  MarkdownInlineSegment segment,
  String resolvedText,
  List<MarkdownInlineSegment> parsedChildren,
) {
  if (parsedChildren.length != 1) {
    return false;
  }
  final onlyChild = parsedChildren.first;
  return onlyChild.nodeType == segment.nodeType &&
      onlyChild.text == resolvedText &&
      onlyChild.children.isEmpty;
}

TextStyle? markdownInlineStyle(
  BuildContext context,
  String? nodeType,
  Color textColor,
  TextStyle? baseStyle,
) {
  final base =
      baseStyle ??
      Theme.of(
        context,
      ).textTheme.bodyMedium?.copyWith(color: textColor, height: 1.3);
  switch (nodeType) {
    case 'Bold':
      return base?.copyWith(fontWeight: FontWeight.w700);
    case 'Italic':
      return base?.copyWith(fontStyle: FontStyle.italic);
    case 'Strikethrough':
      return base?.copyWith(decoration: TextDecoration.lineThrough);
    case 'Underline':
      return base?.copyWith(decoration: TextDecoration.underline);
    case 'Link':
      final linkColor = Theme.of(context).colorScheme.primary;
      return base?.copyWith(
        color: linkColor,
        decoration: TextDecoration.underline,
        decorationColor: linkColor,
      );
    case 'InlineCode':
      return base
          ?.apply(fontSizeFactor: 0.9)
          .copyWith(
            fontFamily: markdownCodeFontFamily,
            fontFamilyFallback: markdownCodeFontFamilyFallback,
            backgroundColor: _inlineCodeBackgroundColor(textColor),
          );
  }
  return base;
}

Color _inlineCodeBackgroundColor(Color textColor) {
  final backgroundAlpha = textColor.computeLuminance() > 0.5 ? 0.18 : 0.12;
  return textColor.withValues(alpha: backgroundAlpha);
}

List<MarkdownInlineSegment> parseInlineSegments(String text) {
  final segments = <MarkdownInlineSegment>[];
  var index = 0;

  while (index < text.length) {
    final marker = _nextMarker(text, index);
    if (marker == null) {
      segments.add(MarkdownInlineSegment(text: text.substring(index)));
      break;
    }
    if (marker.start > index) {
      segments.add(
        MarkdownInlineSegment(text: text.substring(index, marker.start)),
      );
    }
    final close = text.indexOf(marker.close, marker.start + marker.open.length);
    if (close < 0) {
      segments.add(MarkdownInlineSegment(text: text.substring(marker.start)));
      break;
    }
    if (marker.nodeType == 'Link') {
      final rawLink = text.substring(marker.start, close + marker.close.length);
      segments.add(
        MarkdownInlineSegment(
          text: rawLink,
          nodeType: marker.nodeType,
          children: parseInlineSegments(extractLinkText(rawLink)),
        ),
      );
      index = close + marker.close.length;
      continue;
    }
    final inner = text.substring(marker.start + marker.open.length, close);
    segments.add(
      MarkdownInlineSegment(
        text: inner,
        nodeType: marker.nodeType,
        children:
            marker.nodeType == 'InlineCode' || marker.nodeType == 'InlineLatex'
            ? const <MarkdownInlineSegment>[]
            : parseInlineSegments(inner),
      ),
    );
    index = close + marker.close.length;
  }

  return segments;
}

_InlineMarker? _nextMarker(String text, int start) {
  const markers = <_InlineMarker>[
    _InlineMarker(open: '**', close: '**', nodeType: 'Bold'),
    _InlineMarker(open: '__', close: '__', nodeType: 'Bold'),
    _InlineMarker(open: '~~', close: '~~', nodeType: 'Strikethrough'),
    _InlineMarker(open: r'\(', close: r'\)', nodeType: 'InlineLatex'),
    _InlineMarker(open: r'$', close: r'$', nodeType: 'InlineLatex'),
    _InlineMarker(open: '[', close: ')', nodeType: 'Link'),
    _InlineMarker(open: '`', close: '`', nodeType: 'InlineCode'),
    _InlineMarker(open: '*', close: '*', nodeType: 'Italic'),
    _InlineMarker(open: '_', close: '_', nodeType: 'Italic'),
  ];

  _InlineMarker? found;
  var foundStart = text.length + 1;
  for (final marker in markers) {
    final markerStart = text.indexOf(marker.open, start);
    if (markerStart >= 0 &&
        markerStart < foundStart &&
        _isValidInlineMarkerStart(text, marker, markerStart)) {
      found = marker.at(markerStart);
      foundStart = markerStart;
    }
  }
  return found;
}

bool _isValidInlineMarkerStart(String text, _InlineMarker marker, int start) {
  if (marker.nodeType != 'InlineLatex' || marker.open != r'$') {
    if (marker.nodeType == 'Link') {
      final bracketEnd = text.indexOf(']', start + 1);
      return bracketEnd > start + 1 &&
          bracketEnd + 1 < text.length &&
          text.codeUnitAt(bracketEnd + 1) == 0x28;
    }
    return true;
  }
  if (start + 1 < text.length && text.codeUnitAt(start + 1) == 0x24) {
    return false;
  }
  return start == 0 || text.codeUnitAt(start - 1) != 0x5C;
}

String extractLinkText(String linkContent) {
  final match = RegExp(r'^\[([^\]]+)\]\(([^)]+)\)$').firstMatch(linkContent);
  return match?.group(1) ?? linkContent;
}

String extractLinkUrl(String linkContent) {
  final match = RegExp(
    r'^\[([^\]]*)\]\(([\s\S]+)\)$',
  ).firstMatch(linkContent.trim());
  if (match == null) {
    return '';
  }
  var destination = match.group(2)!.trim();
  if (destination.startsWith('<') && destination.contains('>')) {
    destination = destination.substring(1, destination.indexOf('>')).trim();
  }
  return destination
      .replaceFirst(RegExp(r"""\s+(?:"[^"]*"|'[^']*'|\([^)]*\))\s*$"""), '')
      .trim();
}

TapGestureRecognizer? _markdownLinkRecognizer(
  String url,
  void Function(String url)? onLinkClick,
  MarkdownLinkGestureOwner gestures,
) {
  return gestures.recognizerFor(url, onLinkClick);
}

/// Owns the gesture recognizers for one Markdown text build.
///
/// Recognizers are reused across frames in encounter order and released when
/// that text is no longer built.
class MarkdownLinkGestureOwner {
  final List<TapGestureRecognizer> _recognizers = <TapGestureRecognizer>[];
  var _count = 0;

  /// Starts a new build pass.
  void beginFrame() {
    _count = 0;
  }

  /// Returns the recognizer for the next link in this build pass.
  TapGestureRecognizer? recognizerFor(
    String url,
    void Function(String url)? onLinkClick,
  ) {
    final destination = url.trim();
    if (destination.isEmpty) {
      return null;
    }
    void openLink() {
      activateMarkdownLink(destination, onLinkClick);
    }
    if (_count < _recognizers.length) {
      final recognizer = _recognizers[_count];
      recognizer.onTap = openLink;
      _count++;
      return recognizer;
    }
    final recognizer = TapGestureRecognizer()..onTap = openLink;
    _recognizers.add(recognizer);
    _count++;
    return recognizer;
  }

  /// Disposes recognizers that this build pass did not use.
  void endFrame() {
    while (_recognizers.length > _count) {
      _recognizers.removeLast().dispose();
    }
  }

  /// Disposes every recognizer owned by this text.
  void dispose() {
    for (final recognizer in _recognizers) {
      recognizer.dispose();
    }
    _recognizers.clear();
    _count = 0;
  }
}

class _MarkdownAutolinkPiece {
  const _MarkdownAutolinkPiece(this.text, {this.url});

  final String text;
  final String? url;
}

final RegExp _markdownAutolinkPattern = RegExp(
  r'https?://[^\s<>\[\]]+',
  caseSensitive: false,
);

const String _markdownAutolinkTrailing = '.,;:!?，。、；：！？';

List<_MarkdownAutolinkPiece> _splitMarkdownAutolinks(String text) {
  final matches = _markdownAutolinkPattern.allMatches(text).toList();
  if (matches.isEmpty) {
    return <_MarkdownAutolinkPiece>[_MarkdownAutolinkPiece(text)];
  }
  final pieces = <_MarkdownAutolinkPiece>[];
  var start = 0;
  for (final match in matches) {
    final url = _trimMarkdownAutolink(match.group(0)!);
    if (url.isEmpty) {
      continue;
    }
    final urlEnd = match.start + url.length;
    if (match.start > start) {
      pieces.add(_MarkdownAutolinkPiece(text.substring(start, match.start)));
    }
    pieces.add(_MarkdownAutolinkPiece(url, url: url));
    start = urlEnd;
  }
  if (start < text.length) {
    pieces.add(_MarkdownAutolinkPiece(text.substring(start)));
  }
  if (pieces.isEmpty) {
    return <_MarkdownAutolinkPiece>[_MarkdownAutolinkPiece(text)];
  }
  return pieces;
}

String _trimMarkdownAutolink(String raw) {
  var end = raw.length;
  while (end > 0 && _markdownAutolinkTrailing.contains(raw[end - 1])) {
    end--;
  }
  final slice = raw.substring(0, end);
  final openCount = '('.allMatches(slice).length;
  final closeCount = ')'.allMatches(slice).length;
  if (closeCount > openCount && slice.endsWith(')')) {
    return slice.substring(0, slice.length - 1);
  }
  return slice;
}

class _InlineMarker {
  const _InlineMarker({
    required this.open,
    required this.close,
    required this.nodeType,
    this.start = 0,
  });

  final String open;
  final String close;
  final String nodeType;
  final int start;

  _InlineMarker at(int value) {
    return _InlineMarker(
      open: open,
      close: close,
      nodeType: nodeType,
      start: value,
    );
  }
}
