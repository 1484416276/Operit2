// ignore_for_file: file_names

import 'package:flutter/material.dart';

const String markdownCodeFontFamily = 'monospace';

const List<String> markdownCodeFontFamilyFallback = <String>[
  'JetBrains Mono',
  'Cascadia Mono',
  'Consolas',
  'SF Mono',
  'Menlo',
  'Roboto Mono',
  'monospace',
];

TextStyle? markdownCodeTextStyle(BuildContext context, {Color? color}) {
  return Theme.of(context).textTheme.bodySmall?.copyWith(
    color: color,
    fontFamily: markdownCodeFontFamily,
    fontFamilyFallback: markdownCodeFontFamilyFallback,
    height: 1.4,
  );
}
