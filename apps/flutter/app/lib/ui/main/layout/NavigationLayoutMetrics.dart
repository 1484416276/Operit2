// ignore_for_file: file_names

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

const double navigationTabletBreakpoint = 600;

/// Reference logical viewport dimensions used as the design baseline for
/// desktop/multi-column layouts.
const double navigationDesktopReferenceWidth = 1160.0;
const double navigationDesktopReferenceHeight = 720.0;

/// Computes the default sidebar width for wide/tablet layouts.
double resolveTabletSidebarWidth(double viewportWidth) {
  return (viewportWidth * 0.22).clamp(216.0, 272.0).toDouble();
}

bool useTabletLayoutForWidth(double width) {
  return width >= navigationTabletBreakpoint;
}

bool useTabletLayoutForContext(BuildContext context) {
  return useTabletLayoutForWidth(MediaQuery.sizeOf(context).width);
}

/// Identifies narrow portrait mobile viewports that use single-column touch layouts.
bool isPortraitPhoneViewport(Size size) {
  return size.width < navigationTabletBreakpoint &&
      size.height >= size.width * 1.15;
}

/// Resolves the adaptive viewport scale factor across DPI scales and window sizes.
///
/// Returns 1.0 for standard phone screens and spacious desktop viewports.
/// For multi-column/landscape windows whose raw logical size is reduced by
/// desktop DPI scaling or window resizing, scales proportionally so that
/// desktop density and whitespace proportions remain balanced.
double resolveViewportScale(Size size) {
  if (size.isEmpty) {
    return 1.0;
  }
  if (isPortraitPhoneViewport(size)) {
    if (size.width < 360.0) {
      return (size.width / 360.0).clamp(0.75, 1.0);
    }
    return 1.0;
  }
  final widthRatio = (size.width / navigationDesktopReferenceWidth).clamp(0.60, 1.0);
  final heightRatio = (size.height / navigationDesktopReferenceHeight).clamp(0.60, 1.0);
  return ((widthRatio + heightRatio) * 0.5).clamp(0.68, 1.0);
}

/// Wraps the application root to provide responsive desktop viewport scaling.
class ResponsiveViewportBox extends StatelessWidget {
  const ResponsiveViewportBox({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final rawSize = MediaQuery.sizeOf(context);
    if (rawSize.isEmpty) {
      return child;
    }
    final scale = resolveViewportScale(rawSize);
    if ((scale - 1.0).abs() < 0.001) {
      return child;
    }
    final mediaQuery = MediaQuery.of(context);
    final scaledSize = Size(rawSize.width / scale, rawSize.height / scale);
    return MediaQuery(
      data: mediaQuery.copyWith(
        size: scaledSize,
        padding: mediaQuery.padding / scale,
        viewPadding: mediaQuery.viewPadding / scale,
        viewInsets: mediaQuery.viewInsets / scale,
        systemGestureInsets: mediaQuery.systemGestureInsets / scale,
      ),
      child: FittedBox(
        fit: BoxFit.fill,
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: scaledSize.width,
          height: scaledSize.height,
          child: child,
        ),
      ),
    );
  }
}
