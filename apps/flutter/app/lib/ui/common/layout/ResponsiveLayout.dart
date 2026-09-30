// ignore_for_file: file_names

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// Centralizes layout decisions without classifying the host device.
///
/// Application callers use the current (possibly scaled) MediaQuery width.
/// Component callers pass their local available width. Only the root viewport
/// scaling policy uses the raw viewport size for portrait classification.
abstract final class ResponsiveLayout {
  static const double wideLayoutBreakpoint = 600;

  /// Selects the wide layout for an explicit logical available width.
  static bool usesWideLayout(double availableWidth) {
    return availableWidth >= wideLayoutBreakpoint;
  }

  /// Selects the application layout from the current logical viewport.
  static bool usesWideLayoutOf(BuildContext context) {
    return usesWideLayout(MediaQuery.sizeOf(context).width);
  }

  /// Identifies compact portrait geometry for the root scaling policy only.
  static bool isPortraitCompactViewport(Size rawViewportSize) {
    return !usesWideLayout(rawViewportSize.width) &&
        rawViewportSize.height >= rawViewportSize.width * 1.15;
  }

  /// Selects a local split layout using the existing side-panel threshold policy.
  static bool usesSideBySideLayout(
    double availableWidth, {
    double breakpoint = wideLayoutBreakpoint,
    required double minPanelWidth,
    required double minContentWidth,
  }) {
    final requiredWidth = math.min(breakpoint, minPanelWidth + minContentWidth);
    return availableWidth >= requiredWidth;
  }
}
