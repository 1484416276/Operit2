// ignore_for_file: file_names

import 'package:flutter/material.dart';

import 'MarkdownCodeTypeface.dart';

class CanvasMonospaceCodeBlockBody extends StatelessWidget {
  const CanvasMonospaceCodeBlockBody({
    super.key,
    required this.lines,
    required this.autoWrapEnabled,
    this.highlightedLines,
  });

  final List<String> lines;
  final bool autoWrapEnabled;
  final List<InlineSpan>? highlightedLines;

  @override
  Widget build(BuildContext context) {
    final digits = lines.length.toString().length;
    final gutterWidth = (digits * 8.0 + 20.0).clamp(36.0, 64.0);
    return SelectionArea(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          for (var index = 0; index < lines.length; index++)
            _CodeLine(
              lineNumber: index + 1,
              gutterWidth: gutterWidth,
              text: lines[index],
              span:
                  highlightedLines == null || index >= highlightedLines!.length
                  ? null
                  : highlightedLines![index],
              autoWrapEnabled: autoWrapEnabled,
            ),
        ],
      ),
    );
  }
}

class _CodeLine extends StatelessWidget {
  const _CodeLine({
    required this.lineNumber,
    required this.gutterWidth,
    required this.text,
    required this.span,
    required this.autoWrapEnabled,
  });

  final int lineNumber;
  final double gutterWidth;
  final String text;
  final InlineSpan? span;
  final bool autoWrapEnabled;

  @override
  Widget build(BuildContext context) {
    final codeStyle = markdownCodeTextStyle(
      context,
      color: const Color(0xFFD4D4D4),
    );
    final lineNumberStyle = markdownCodeTextStyle(
      context,
      color: const Color(0xFF6E7681),
    )?.copyWith(fontFeatures: const <FontFeature>[FontFeature.tabularFigures()]);
    final codeText = Padding(
      padding: const EdgeInsets.only(right: 12),
      child: span == null
          ? Text(
              text,
              softWrap: autoWrapEnabled,
              style: codeStyle,
            )
          : Text.rich(
              span!,
              softWrap: autoWrapEnabled,
              style: codeStyle,
            ),
    );
    final codeTextChild = autoWrapEnabled
        ? Expanded(child: codeText)
        : ConstrainedBox(constraints: const BoxConstraints(), child: codeText);

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: <Widget>[
        SelectionContainer.disabled(
          child: SizedBox(
            width: gutterWidth,
            child: Padding(
              padding: const EdgeInsets.only(left: 8, right: 10),
              child: Text(
                '$lineNumber',
                textAlign: TextAlign.end,
                style: lineNumberStyle,
              ),
            ),
          ),
        ),
        codeTextChild,
      ],
    );
  }
}
