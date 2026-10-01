// ignore_for_file: file_names

import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// Keeps drags that start inside this region out of the phone drawer's arena.
///
/// This is a hit-test marker, not another gesture recognizer: the child keeps
/// its own cursor, selection, scrolling and long-press behavior unchanged.
class DrawerGestureExclusion extends SingleChildRenderObjectWidget {
  const DrawerGestureExclusion({super.key, required super.child});

  static bool containsTarget(HitTestTarget target) =>
      target is _RenderDrawerGestureExclusion;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderDrawerGestureExclusion();
}

class _RenderDrawerGestureExclusion extends RenderProxyBox {
  // Include the composer's padding, even where its children don't hit-test.
  @override
  bool hitTestSelf(Offset position) => true;
}
