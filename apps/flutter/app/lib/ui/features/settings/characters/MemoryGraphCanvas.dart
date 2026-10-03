// ignore_for_file: file_names

import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import '../../../../core/proxy/generated/CoreProxyModels.g.dart' as core_proxy;

/// View transform kept outside widget state so gestures only repaint the canvas.
class MemoryGraphController extends ChangeNotifier {
  static const minScale = 0.1;
  static const maxScale = 5.0;

  double _scale = 1;
  Offset _offset = Offset.zero;
  double get scale => _scale;
  Offset get offset => _offset;

  Offset worldToScreen(Offset point) => point * _scale + _offset;
  Offset screenToWorld(Offset point) => (point - _offset) / _scale;

  void setView(double scale, Offset offset) {
    final nextScale = scale.clamp(minScale, maxScale).toDouble();
    if (_scale == nextScale && _offset == offset) return;
    _scale = nextScale;
    _offset = offset;
    notifyListeners();
  }

  void panBy(Offset delta) => setView(_scale, _offset + delta);

  /// Preserves the world point under the cursor/fingers, including at limits.
  void zoomAt(Offset focalPoint, double factor) {
    final anchor = screenToWorld(focalPoint);
    final nextScale = (_scale * factor).clamp(minScale, maxScale).toDouble();
    setView(nextScale, focalPoint - anchor * nextScale);
  }

  void fit(Rect bounds, Size viewport) {
    if (viewport.isEmpty || bounds.isEmpty) return;
    const padding = 40.0;
    final scale = math
        .min(
          math.max(1.0, viewport.width - padding * 2) / bounds.width,
          math.max(1.0, viewport.height - padding * 2) / bounds.height,
        )
        .clamp(minScale, 1.0)
        .toDouble();
    setView(scale, viewport.center(Offset.zero) - bounds.center * scale);
  }
}

/// Owner-independent graph rendering and touch/mouse/trackpad interaction.
class MemoryGraphCanvas extends StatefulWidget {
  const MemoryGraphCanvas({
    super.key,
    required this.graph,
    required this.onSelect,
    this.controller,
    this.selectedNodeId,
    this.selectedEdgeId,
    this.linkSourceNodeId,
  });

  final core_proxy.MemoryGraph graph;
  final MemoryGraphController? controller;
  final String? selectedNodeId;
  final int? selectedEdgeId;
  final String? linkSourceNodeId;
  final void Function(
    core_proxy.MemoryGraphNode? node,
    core_proxy.MemoryGraphEdge? edge,
  )
  onSelect;

  @override
  State<MemoryGraphCanvas> createState() => _MemoryGraphCanvasState();
}

class _MemoryGraphCanvasState extends State<MemoryGraphCanvas> {
  late MemoryGraphController _controller;
  _GraphScene? _scene;
  ColorScheme? _colors;
  TextDirection? _direction;
  TextScaler? _textScaler;
  TextStyle? _nodeStyle;
  TextStyle? _edgeStyle;
  Size? _viewport;
  Offset _gestureAnchor = Offset.zero;
  double _gestureScale = 1;

  @override
  void initState() {
    super.initState();
    _controller = widget.controller ?? MemoryGraphController();
  }

  @override
  void didUpdateWidget(MemoryGraphCanvas oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      if (oldWidget.controller == null) _controller.dispose();
      _controller = widget.controller ?? MemoryGraphController();
      _viewport = null;
    }
    if (oldWidget.graph != widget.graph) {
      _scene?.dispose();
      _scene = null;
    }
  }

  @override
  void dispose() {
    _scene?.dispose();
    if (widget.controller == null) _controller.dispose();
    super.dispose();
  }

  void _select(Offset screenPoint) {
    final scene = _scene!;
    final worldPoint = _controller.screenToWorld(screenPoint);
    // Drawing, culling and hit testing share exactly the same measured bounds.
    final tolerance = 6 / _controller.scale;
    for (final node in widget.graph.nodes.reversed) {
      if (scene.nodeRects[node.id]!.inflate(tolerance).contains(worldPoint)) {
        widget.onSelect(node, null);
        return;
      }
    }
    for (final edge in widget.graph.edges.reversed) {
      final start = scene.positions[edge.sourceId];
      final end = scene.positions[edge.targetId];
      if (start != null &&
          end != null &&
          _distanceToSegment(worldPoint, start, end) <= 8 / _controller.scale) {
        widget.onSelect(null, edge);
        return;
      }
    }
    widget.onSelect(null, null);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final direction = Directionality.of(context);
    final textScaler = MediaQuery.textScalerOf(context);
    final nodeStyle = theme.textTheme.labelMedium!.copyWith(
      color: colors.onSurface,
      fontSize: 12,
      height: 1.2,
    );
    final edgeStyle = theme.textTheme.labelSmall!.copyWith(
      color: colors.onSurfaceVariant,
      fontSize: 10,
      height: 1.2,
    );
    final recreate =
        _scene == null ||
        _colors != colors ||
        _direction != direction ||
        _textScaler != textScaler ||
        _nodeStyle != nodeStyle ||
        _edgeStyle != edgeStyle;
    if (recreate) {
      _scene?.dispose();
      _scene = _GraphScene(
        widget.graph,
        nodeStyle,
        edgeStyle,
        direction,
        textScaler,
      );
      _colors = colors;
      _direction = direction;
      _textScaler = textScaler;
      _nodeStyle = nodeStyle;
      _edgeStyle = edgeStyle;
      _viewport = null;
    }
    final scene = _scene!;
    return LayoutBuilder(
      builder: (context, constraints) {
        final size = constraints.biggest;
        if (_viewport == null) {
          _controller.fit(scene.bounds, size);
        } else if (_viewport != size) {
          // Window rotation/resizing must not throw away the user's zoom/pan.
          _controller.panBy(
            size.center(Offset.zero) - _viewport!.center(Offset.zero),
          );
        }
        _viewport = size;
        return ClipRect(
          child: Stack(
            children: [
              Positioned.fill(
                child: Listener(
                  onPointerSignal: (event) {
                    if (event is! PointerScrollEvent) return;
                    GestureBinding.instance.pointerSignalResolver.register(
                      event,
                      (event) {
                        final scroll = event as PointerScrollEvent;
                        if (scroll.kind == PointerDeviceKind.trackpad ||
                            scroll.scrollDelta.dx.abs() >
                                scroll.scrollDelta.dy.abs()) {
                          _controller.panBy(-scroll.scrollDelta);
                        } else {
                          _controller.zoomAt(
                            scroll.localPosition,
                            math.exp(-scroll.scrollDelta.dy * 0.0015),
                          );
                        }
                      },
                    );
                  },
                  child: GestureDetector(
                    key: const ValueKey('memory-graph-gestures'),
                    behavior: HitTestBehavior.opaque,
                    supportedDevices: const {
                      PointerDeviceKind.touch,
                      PointerDeviceKind.mouse,
                      PointerDeviceKind.trackpad,
                      PointerDeviceKind.stylus,
                      PointerDeviceKind.invertedStylus,
                    },
                    onScaleStart: (details) {
                      _gestureScale = _controller.scale;
                      _gestureAnchor = _controller.screenToWorld(
                        details.localFocalPoint,
                      );
                    },
                    onScaleUpdate: (details) {
                      final scale = (_gestureScale * details.scale)
                          .clamp(
                            MemoryGraphController.minScale,
                            MemoryGraphController.maxScale,
                          )
                          .toDouble();
                      // Focal point is cumulative, not a single frame's delta.
                      _controller.setView(
                        scale,
                        details.localFocalPoint - _gestureAnchor * scale,
                      );
                    },
                    onTapUp: (details) => _select(details.localPosition),
                    child: RepaintBoundary(
                      child: CustomPaint(
                        painter: _GraphPainter(
                          scene: scene,
                          controller: _controller,
                          colors: colors,
                          selectedNodeId: widget.selectedNodeId,
                          selectedEdgeId: widget.selectedEdgeId,
                          linkSourceNodeId: widget.linkSourceNodeId,
                        ),
                        size: Size.infinite,
                      ),
                    ),
                  ),
                ),
              ),
              Positioned(
                right: 12,
                bottom: 12,
                child: Material(
                  color: colors.surfaceContainerHigh.withValues(alpha: 0.95),
                  borderRadius: BorderRadius.circular(24),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      IconButton(
                        tooltip: '缩小',
                        icon: const Icon(Icons.remove),
                        onPressed: () => _controller.zoomAt(
                          size.center(Offset.zero),
                          1 / 1.25,
                        ),
                      ),
                      ListenableBuilder(
                        listenable: _controller,
                        builder: (context, _) => SizedBox(
                          width: 48,
                          child: Text(
                            '${(_controller.scale * 100).round()}%',
                            textAlign: TextAlign.center,
                            style: Theme.of(context).textTheme.labelSmall,
                          ),
                        ),
                      ),
                      IconButton(
                        tooltip: '放大',
                        icon: const Icon(Icons.add),
                        onPressed: () =>
                            _controller.zoomAt(size.center(Offset.zero), 1.25),
                      ),
                      IconButton(
                        tooltip: '适应窗口',
                        icon: const Icon(Icons.center_focus_strong),
                        onPressed: () => _controller.fit(scene.bounds, size),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// All paragraph measurement and graph layout happens once per data/style change.
class _GraphScene {
  _GraphScene(
    this.graph,
    TextStyle nodeStyle,
    TextStyle edgeStyle,
    TextDirection direction,
    TextScaler textScaler,
  ) {
    for (final node in graph.nodes) {
      final text = _labelPainter(
        node.label,
        nodeStyle,
        direction,
        textScaler,
        240,
        2,
      );
      nodeLabels[node.id] = text;
      nodeSizes[node.id] = Size(
        math.max(56, text.width + 28),
        math.max(28, text.height + 12),
      );
    }
    for (final edge in graph.edges) {
      final label = edge.label;
      if (label != null && label.isNotEmpty) {
        edgeLabels.putIfAbsent(
          label,
          () => _labelPainter(label, edgeStyle, direction, textScaler, 180, 1),
        );
      }
    }
    _layout();
    Rect? allBounds;
    for (final node in graph.nodes) {
      final size = nodeSizes[node.id]!;
      final rect = Rect.fromCenter(
        center: positions[node.id]!,
        width: size.width,
        height: size.height,
      );
      nodeRects[node.id] = rect;
      allBounds = allBounds?.expandToInclude(rect) ?? rect;
    }
    bounds = allBounds ?? const Rect.fromLTWH(0, 0, 1, 1);
  }

  final core_proxy.MemoryGraph graph;
  final nodeLabels = <String, TextPainter>{};
  final edgeLabels = <String, TextPainter>{};
  final nodeSizes = <String, Size>{};
  final positions = <String, Offset>{};
  final nodeRects = <String, Rect>{};
  late final Rect bounds;

  void _layout() {
    final adjacency = {for (final node in graph.nodes) node.id: <String>[]};
    for (final edge in graph.edges) {
      if (edge.isCrossFolderLink ||
          !adjacency.containsKey(edge.sourceId) ||
          !adjacency.containsKey(edge.targetId)) {
        continue;
      }
      adjacency[edge.sourceId]!.add(edge.targetId);
      adjacency[edge.targetId]!.add(edge.sourceId);
    }
    final visited = <String>{};
    final clusters = <List<String>>[];
    for (final node in graph.nodes) {
      if (!visited.add(node.id)) continue;
      final cluster = <String>[node.id];
      for (var i = 0; i < cluster.length; i++) {
        for (final neighbor in adjacency[cluster[i]]!) {
          if (visited.add(neighbor)) cluster.add(neighbor);
        }
      }
      cluster.sort((a, b) {
        final degree = adjacency[b]!.length.compareTo(adjacency[a]!.length);
        return degree == 0 ? a.compareTo(b) : degree;
      });
      clusters.add(cluster);
    }
    clusters.sort((a, b) => b.length.compareTo(a.length));
    // Pack actual cluster bounds, not fixed 860 x 680 cells for tiny nodes.
    final columns = math.max(1, math.sqrt(clusters.length).ceil());
    var x = 0.0;
    var y = 0.0;
    var rowHeight = 0.0;
    for (var i = 0; i < clusters.length; i++) {
      final ids = clusters[i];
      final local = <String, Offset>{ids.first: Offset.zero};
      final diameter =
          ids
              .map(
                (id) => math.sqrt(
                  nodeSizes[id]!.width * nodeSizes[id]!.width +
                      nodeSizes[id]!.height * nodeSizes[id]!.height,
                ),
              )
              .reduce(math.max) +
          32;
      var placed = 1;
      var radius = diameter;
      while (placed < ids.length) {
        // Chord length >= node diagonal clearance even for long CJK titles.
        final clearance = diameter * 1.12;
        final capacity = math.max(
          2,
          (math.pi / math.asin((clearance / (2 * radius)).clamp(0.0, 1.0)))
              .floor(),
        );
        final count = math.min(capacity, ids.length - placed);
        for (var j = 0; j < count; j++) {
          final angle = -math.pi / 2 + 2 * math.pi * j / count;
          local[ids[placed + j]] =
              Offset(math.cos(angle), math.sin(angle)) * radius;
        }
        placed += count;
        radius += clearance;
      }
      Rect? clusterBounds;
      for (final id in ids) {
        final size = nodeSizes[id]!;
        final rect = Rect.fromCenter(
          center: local[id]!,
          width: size.width,
          height: size.height,
        );
        clusterBounds = clusterBounds?.expandToInclude(rect) ?? rect;
      }
      final box = clusterBounds!;
      if (i % columns == 0 && i > 0) {
        x = 0;
        y += rowHeight + 56;
        rowHeight = 0;
      }
      final shift = Offset(x - box.left, y - box.top);
      for (final id in ids) {
        positions[id] = local[id]! + shift;
      }
      x += box.width + 56;
      rowHeight = math.max(rowHeight, box.height);
    }
  }

  void dispose() {
    for (final label in nodeLabels.values) {
      label.dispose();
    }
    for (final label in edgeLabels.values) {
      label.dispose();
    }
  }
}

TextPainter _labelPainter(
  String text,
  TextStyle style,
  TextDirection direction,
  TextScaler textScaler,
  double maxWidth,
  int maxLines,
) => TextPainter(
  text: TextSpan(text: text, style: style),
  textDirection: direction,
  textScaler: textScaler,
  maxLines: maxLines,
  ellipsis: '…',
)..layout(maxWidth: maxWidth);

class _GraphPainter extends CustomPainter {
  _GraphPainter({
    required this.scene,
    required this.controller,
    required this.colors,
    required this.selectedNodeId,
    required this.selectedEdgeId,
    required this.linkSourceNodeId,
  }) : super(repaint: controller);

  final _GraphScene scene;
  final MemoryGraphController controller;
  final ColorScheme colors;
  final String? selectedNodeId;
  final int? selectedEdgeId;
  final String? linkSourceNodeId;

  @override
  void paint(Canvas canvas, Size size) {
    final scale = controller.scale;
    final visible = Rect.fromPoints(
      controller.screenToWorld(Offset.zero),
      controller.screenToWorld(size.bottomRight(Offset.zero)),
    ).inflate(20 / scale);
    canvas.save();
    canvas.translate(controller.offset.dx, controller.offset.dy);
    canvas.scale(scale);
    final line = Paint()..strokeCap = StrokeCap.round;
    for (final edge in scene.graph.edges) {
      final start = scene.positions[edge.sourceId];
      final end = scene.positions[edge.targetId];
      if (start == null ||
          end == null ||
          !Rect.fromPoints(start, end).inflate(8 / scale).overlaps(visible)) {
        continue;
      }
      line.color = edge.id == selectedEdgeId
          ? colors.error
          : colors.outline.withValues(alpha: 0.55);
      line.strokeWidth = (edge.weight * (edge.isCrossFolderLink ? 1.6 : 2.4))
          .clamp(0.7, 4.0)
          .toDouble();
      if (edge.isCrossFolderLink) {
        _drawDashedLine(canvas, start, end, line, visible);
      } else {
        canvas.drawLine(start, end, line);
      }
      final label = scene.edgeLabels[edge.label];
      final center = (start + end) / 2;
      // Below this scale labels would be illegible and clutter the overview.
      if (scale >= 0.55 && label != null && visible.contains(center)) {
        final origin = center - Offset(label.width / 2, label.height + 4);
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            (origin & label.size).inflate(3),
            const Radius.circular(4),
          ),
          Paint()..color = colors.surface.withValues(alpha: 0.9),
        );
        label.paint(canvas, origin);
      }
    }
    final fill = Paint();
    final outline = Paint()..style = PaintingStyle.stroke;
    for (final node in scene.graph.nodes) {
      final rect = scene.nodeRects[node.id]!;
      if (!rect.overlaps(visible)) continue;
      final rounded = RRect.fromRectAndRadius(
        rect,
        Radius.circular(math.min(14, rect.height / 2)),
      );
      final selected = node.id == selectedNodeId;
      final source = node.id == linkSourceNodeId;
      fill.color = Color.alphaBlend(
        Color(node.color & 0xFFFFFFFF).withValues(alpha: 0.18),
        colors.surfaceContainerHigh,
      );
      canvas.drawRRect(rounded, fill);
      outline.color = source
          ? colors.tertiary
          : selected
          ? colors.primary
          : colors.outline;
      outline.strokeWidth = selected || source ? 2 : 1;
      canvas.drawRRect(rounded, outline);
      if (scale >= 0.3 || selected || source) {
        final text = scene.nodeLabels[node.id]!;
        text.paint(
          canvas,
          rect.center - Offset(text.width / 2, text.height / 2),
        );
      }
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_GraphPainter old) =>
      scene != old.scene ||
      controller != old.controller ||
      colors != old.colors ||
      selectedNodeId != old.selectedNodeId ||
      selectedEdgeId != old.selectedEdgeId ||
      linkSourceNodeId != old.linkSourceNodeId;
}

double _distanceToSegment(Offset point, Offset start, Offset end) {
  final delta = end - start;
  if (delta.distanceSquared == 0) return (point - start).distance;
  final t =
      ((point - start).dx * delta.dx + (point - start).dy * delta.dy) /
      delta.distanceSquared;
  return (point - (start + delta * t.clamp(0.0, 1.0))).distance;
}

/// Clip the parameter range before dashing so long offscreen edges stay cheap.
void _drawDashedLine(
  Canvas canvas,
  Offset start,
  Offset end,
  Paint paint,
  Rect visible,
) {
  final delta = end - start;
  final length = delta.distance;
  if (length == 0) return;
  var from = 0.0;
  var to = 1.0;
  for (final axis in [
    (start.dx, delta.dx, visible.left, visible.right),
    (start.dy, delta.dy, visible.top, visible.bottom),
  ]) {
    if (axis.$2 == 0) {
      if (axis.$1 < axis.$3 || axis.$1 > axis.$4) return;
    } else {
      final a = (axis.$3 - axis.$1) / axis.$2;
      final b = (axis.$4 - axis.$1) / axis.$2;
      from = math.max(from, math.min(a, b));
      to = math.min(to, math.max(a, b));
    }
  }
  if (from > to) return;
  final direction = delta / length;
  for (
    var distance = (from * length / 16).floor() * 16.0;
    distance < to * length;
    distance += 16
  ) {
    final segmentStart = math.max(distance, from * length);
    final segmentEnd = math.min(distance + 8, to * length);
    if (segmentStart < segmentEnd) {
      canvas.drawLine(
        start + direction * segmentStart,
        start + direction * segmentEnd,
        paint,
      );
    }
  }
}
