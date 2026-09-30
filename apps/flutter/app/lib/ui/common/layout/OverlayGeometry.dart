// ignore_for_file: file_names

import 'package:flutter/widgets.dart';

/// Resolves a laid-out anchor rectangle in the nearest Overlay coordinate space.
Rect overlayTargetRectOf(BuildContext context, RenderBox target) {
  final overlayBox = Overlay.of(context).context.findRenderObject();
  if (overlayBox is! RenderBox || !overlayBox.attached || !overlayBox.hasSize) {
    throw StateError('Popup overlay is not laid out.');
  }
  if (!target.attached || !target.hasSize) {
    throw StateError('Popup target is not laid out.');
  }
  return MatrixUtils.transformRect(
    target.getTransformTo(overlayBox),
    Offset.zero & target.size,
  );
}
