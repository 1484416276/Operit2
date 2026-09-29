import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// Places a native Windows visual at its actual position in the Flutter scene.
class WindowsNativeComposition extends LeafRenderObjectWidget {
  /// Creates a layer using the identifier allocated by the Windows engine.
  const WindowsNativeComposition({super.key, required this.viewId});

  final int viewId;

  /// Creates a render object with normal Flutter layout and hit testing.
  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderWindowsNativeComposition(viewId);

  /// Updates the layer when a controller replaces the native browser.
  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderWindowsNativeComposition renderObject,
  ) {
    renderObject.viewId = viewId;
  }
}

class _RenderWindowsNativeComposition extends RenderBox {
  /// Retains the native visual identifier independently of browser painting.
  _RenderWindowsNativeComposition(this._viewId);

  int _viewId;

  /// Invalidates the scene when native content is replaced.
  set viewId(int value) {
    if (_viewId == value) return;
    _viewId = value;
    markNeedsPaint();
  }

  /// Platform views always participate in the composited scene.
  @override
  bool get alwaysNeedsCompositing => true;

  /// Uses the bounded dimensions provided by the browser widget.
  @override
  void performLayout() {
    size = constraints.biggest;
  }

  /// Allows the surrounding listener to receive Flutter-selected input only.
  @override
  bool hitTestSelf(Offset position) => true;

  /// Emits a platform-view layer instead of sampling browser pixels.
  @override
  void paint(PaintingContext context, Offset offset) {
    if (size.isEmpty) return;
    final translation = OffsetLayer(offset: offset);
    translation.append(
      PlatformViewLayer(rect: Offset.zero & size, viewId: _viewId),
    );
    context.addLayer(translation);
  }
}
