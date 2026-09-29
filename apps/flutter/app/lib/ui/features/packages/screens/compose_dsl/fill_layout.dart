import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// Applies Compose fill constraints independently to each bounded axis.
class ComposeDslFill extends SingleChildRenderObjectWidget {
  /// Creates a fill modifier without imposing a size on unbounded axes.
  const ComposeDslFill({
    super.key,
    required this.fillWidth,
    required this.fillHeight,
    this.fraction = 1,
    required Widget super.child,
  }) : assert(fraction >= 0 && fraction < double.infinity);

  final bool fillWidth;
  final bool fillHeight;
  final double fraction;

  /// Creates the constraint-aware measuring object.
  @override
  RenderObject createRenderObject(BuildContext context) =>
      RenderComposeDslFill(fillWidth, fillHeight, fraction);

  /// Invalidates measurement when the modifier changes.
  @override
  void updateRenderObject(
    BuildContext context,
    covariant RenderComposeDslFill renderObject,
  ) {
    renderObject.update(fillWidth, fillHeight, fraction);
  }
}

/// Preserves incoming constraints on axes that Compose cannot fill.
class RenderComposeDslFill extends RenderProxyBox {
  /// Stores the axes and fraction requested by the DSL modifier.
  RenderComposeDslFill(this._fillWidth, this._fillHeight, this._fraction);

  bool _fillWidth;
  bool _fillHeight;
  double _fraction;

  /// Updates modifier values and schedules a new measurement.
  void update(bool fillWidth, bool fillHeight, double fraction) {
    if (_fillWidth == fillWidth &&
        _fillHeight == fillHeight &&
        _fraction == fraction) {
      return;
    }
    _fillWidth = fillWidth;
    _fillHeight = fillHeight;
    _fraction = fraction;
    markNeedsLayout();
  }

  /// Tightens bounded fill axes while retaining all other parent constraints.
  BoxConstraints _childConstraints(BoxConstraints incoming) {
    return incoming.tighten(
      width: _fillWidth && incoming.hasBoundedWidth
          ? (incoming.maxWidth * _fraction).clamp(
              incoming.minWidth,
              incoming.maxWidth,
            )
          : null,
      height: _fillHeight && incoming.hasBoundedHeight
          ? (incoming.maxHeight * _fraction).clamp(
              incoming.minHeight,
              incoming.maxHeight,
            )
          : null,
    );
  }

  /// Uses identical fill constraints during dry and actual measurement.
  @override
  Size computeDryLayout(BoxConstraints constraints) =>
      child!.getDryLayout(_childConstraints(constraints));

  /// Measures the child's baseline with its actual fill constraints.
  @override
  double? computeDryBaseline(
    BoxConstraints constraints,
    TextBaseline baseline,
  ) => child!.getDryBaseline(_childConstraints(constraints), baseline);

  /// Measures content without creating an infinite tight dimension.
  @override
  void performLayout() {
    child!.layout(_childConstraints(constraints), parentUsesSize: true);
    size = child!.size;
  }
}
