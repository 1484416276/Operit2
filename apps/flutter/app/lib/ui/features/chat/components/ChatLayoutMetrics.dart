// ignore_for_file: file_names

import 'dart:math' as math;

const double chatContentMaxWidth = 700;
const double chatWideContentMaxWidth = 1040;

/// Resolves the maximum content column width for the given available chat pane width.
///
/// On phones (< 520), returns the available width so mobile layouts retain edge-to-edge
/// touch density. On medium and wide chat panes, reserves a proportional horizontal gutter
/// so that the chat transcript and input composer remain centered with comfortable desktop margins.
double resolveChatColumnMaxWidth(
  double availableWidth, {
  required bool wideLayoutEnabled,
}) {
  final targetMaxWidth = wideLayoutEnabled
      ? chatWideContentMaxWidth
      : chatContentMaxWidth;
  if (!availableWidth.isFinite || availableWidth < 520.0) {
    return targetMaxWidth;
  }
  final horizontalGutter = ((availableWidth - 480.0) * 0.16)
      .clamp(24.0, 96.0)
      .toDouble();
  return math.min(targetMaxWidth, availableWidth - horizontalGutter * 2);
}
