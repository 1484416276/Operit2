import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/proxy/generated/CoreProxyModels.g.dart' as core;
import 'package:operit2/ui/features/settings/characters/MemoryGraphCanvas.dart';

const node = core.MemoryGraphNode(
  id: 'one',
  label: '项目记忆',
  color: 0xFF4CAF50,
  metadata: {},
);
const singleGraph = core.MemoryGraph(nodes: [node], edges: []);

Future<void> mount(
  WidgetTester tester,
  MemoryGraphController controller, {
  core.MemoryGraph graph = singleGraph,
  Size size = const Size(600, 400),
  void Function(core.MemoryGraphNode?, core.MemoryGraphEdge?)? onSelect,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Center(
          child: SizedBox(
            width: size.width,
            height: size.height,
            child: MemoryGraphCanvas(
              controller: controller,
              graph: graph,
              onSelect: onSelect ?? (_, _) {},
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Finder get gestureCanvas => find.byKey(const ValueKey('memory-graph-gestures'));
Finder get graphPaint =>
    find.descendant(of: gestureCanvas, matching: find.byType(CustomPaint));

void nearOffset(Offset actual, Offset expected) {
  expect(actual.dx, closeTo(expected.dx, 0.001));
  expect(actual.dy, closeTo(expected.dy, 0.001));
}

void main() {
  group('view transform', () {
    test('pan accumulates in both directions', () {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      controller.panBy(const Offset(10, 4));
      controller.panBy(const Offset(20, -9));
      controller.panBy(const Offset(-5, 8));
      nearOffset(controller.offset, const Offset(25, 3));
    });

    test('zoom keeps the focal world point fixed', () {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      controller.setView(0.8, const Offset(-75, 32));
      const focal = Offset(240, 150);
      final anchor = controller.screenToWorld(focal);
      for (final factor in [1.05, 1.3, 0.9, 0.5, 2.0]) {
        controller.zoomAt(focal, factor);
        nearOffset(controller.worldToScreen(anchor), focal);
      }
    });

    test('clamping zoom does not move the focal point', () {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      const focal = Offset(180, 140);
      final anchor = controller.screenToWorld(focal);
      controller.zoomAt(focal, 1000);
      expect(controller.scale, MemoryGraphController.maxScale);
      nearOffset(controller.worldToScreen(anchor), focal);
      controller.zoomAt(focal, 0.00001);
      expect(controller.scale, MemoryGraphController.minScale);
      nearOffset(controller.worldToScreen(anchor), focal);
    });

    test('small graphs retain natural size and center in the viewport', () {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      const bounds = Rect.fromLTWH(100, 50, 90, 30);
      controller.fit(bounds, const Size(360, 500));
      expect(controller.scale, 1);
      nearOffset(
        controller.worldToScreen(bounds.center),
        const Offset(180, 250),
      );
    });

    test('large graph bounds fit with padding', () {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      const bounds = Rect.fromLTWH(-400, -200, 1600, 1000);
      controller.fit(bounds, const Size(360, 500));
      final topLeft = controller.worldToScreen(bounds.topLeft);
      final bottomRight = controller.worldToScreen(bounds.bottomRight);
      expect(topLeft.dx, greaterThanOrEqualTo(40));
      expect(topLeft.dy, greaterThanOrEqualTo(40));
      expect(bottomRight.dx, lessThanOrEqualTo(320));
      expect(bottomRight.dy, lessThanOrEqualTo(460));
    });
  });

  testWidgets('multi-frame touch drag accumulates and only repaints', (
    tester,
  ) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    final before = controller.offset;
    final painter = tester.widget<CustomPaint>(graphPaint).painter;
    final gesture = await tester.startGesture(tester.getCenter(gestureCanvas));
    await gesture.moveBy(const Offset(30, 0)); // recognizer slop
    await tester.pump();
    final first = controller.offset;
    await gesture.moveBy(const Offset(40, 10));
    await tester.pump();
    await gesture.moveBy(const Offset(60, -20));
    await tester.pump();
    nearOffset(controller.offset - first, const Offset(100, -10));
    expect(controller.offset.dx, greaterThan(before.dx));
    expect(
      identical(tester.widget<CustomPaint>(graphPaint).painter, painter),
      isTrue,
    );
    await gesture.up();
  });

  testWidgets('mouse can drag horizontally and vertically', (tester) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    final gesture = await tester.startGesture(
      tester.getCenter(gestureCanvas),
      kind: PointerDeviceKind.mouse,
    );
    await gesture.moveBy(const Offset(-30, 0));
    await tester.pump();
    final first = controller.offset;
    await gesture.moveBy(const Offset(-70, 45));
    await tester.pump();
    nearOffset(controller.offset - first, const Offset(-70, 45));
    await gesture.up();
  });

  testWidgets('trackpad pan/zoom is smooth and anchored', (tester) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    final origin = tester.getTopLeft(gestureCanvas);
    final focal = tester.getCenter(gestureCanvas);
    final gesture = await tester.createGesture(
      kind: PointerDeviceKind.trackpad,
    );
    await gesture.panZoomStart(focal);
    await gesture.panZoomUpdate(focal, pan: const Offset(30, 0));
    await tester.pump();
    final offset = controller.offset;
    final anchor = controller.screenToWorld(
      focal - origin + const Offset(30, 0),
    );
    await gesture.panZoomUpdate(focal, pan: const Offset(90, 20), scale: 1.4);
    await tester.pump();
    expect(controller.scale, closeTo(1.4, 0.001));
    nearOffset(
      controller.worldToScreen(anchor),
      focal - origin + const Offset(90, 20),
    );
    expect(controller.offset, isNot(offset));
    await gesture.panZoomEnd();
  });

  testWidgets('mouse wheel zoom preserves cursor anchor', (tester) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    final origin = tester.getTopLeft(gestureCanvas);
    final cursor = origin + const Offset(160, 120);
    final anchor = controller.screenToWorld(cursor - origin);
    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: cursor,
        scrollDelta: const Offset(0, -120),
        kind: PointerDeviceKind.mouse,
      ),
    );
    await tester.pump();
    expect(controller.scale, greaterThan(1));
    nearOffset(controller.worldToScreen(anchor), cursor - origin);
  });

  testWidgets('horizontal scroll signals pan instead of being ignored', (
    tester,
  ) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    final before = controller.offset;
    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: tester.getCenter(gestureCanvas),
        scrollDelta: const Offset(80, 0),
        kind: PointerDeviceKind.mouse,
      ),
    );
    await tester.pump();
    nearOffset(controller.offset - before, const Offset(-80, 0));
    expect(controller.scale, 1);
  });

  testWidgets('pinch can simultaneously zoom and translate', (tester) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    final center = tester.getCenter(gestureCanvas);
    final first = await tester.startGesture(
      center - const Offset(40, 0),
      pointer: 1,
    );
    final second = await tester.startGesture(
      center + const Offset(40, 0),
      pointer: 2,
    );
    await first.moveTo(center - const Offset(60, 0));
    await second.moveTo(center + const Offset(60, 0));
    await tester.pump();
    final previousScale = controller.scale;
    final localCenter = center - tester.getTopLeft(gestureCanvas);
    final worldAnchor = controller.screenToWorld(localCenter);
    await first.moveTo(center - const Offset(100, 0) + const Offset(20, 30));
    await second.moveTo(center + const Offset(100, 0) + const Offset(20, 30));
    await tester.pump();
    expect(controller.scale, greaterThan(previousScale));
    nearOffset(
      controller.worldToScreen(worldAnchor),
      localCenter + const Offset(20, 30),
    );
    await first.up();
    await second.up();
    expect(tester.takeException(), isNull);
  });

  testWidgets('zoom controls work and fit resets the view', (tester) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    final initial = controller.offset;
    await tester.tap(find.byTooltip('放大'));
    await tester.pump();
    expect(controller.scale, 1.25);
    expect(find.text('125%'), findsOneWidget);
    await tester.tap(find.byTooltip('缩小'));
    await tester.pump();
    expect(controller.scale, 1);
    controller.panBy(const Offset(100, -80));
    await tester.pump();
    await tester.tap(find.byTooltip('适应窗口'));
    await tester.pump();
    nearOffset(controller.offset, initial);
  });

  testWidgets('node taps remain accurate after zoom/pan at maximum scale', (
    tester,
  ) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    core.MemoryGraphNode? selected;
    await mount(tester, controller, onSelect: (node, _) => selected = node);
    final origin = tester.getTopLeft(gestureCanvas);
    final center = tester.getCenter(gestureCanvas) - origin;
    final worldCenter = controller.screenToWorld(center);
    controller.zoomAt(center, 5);
    controller.panBy(const Offset(20, 10));
    await tester.pump();
    // Click near the right edge of a rendered node, not just its center.
    final point = controller.worldToScreen(worldCenter) + const Offset(100, 0);
    await tester.tapAt(origin + point);
    await tester.pumpAndSettle();
    expect(selected?.id, 'one');
    selected = null;
    await tester.tapAt(origin + const Offset(15, 15));
    await tester.pumpAndSettle();
    expect(selected, isNull);
  });

  testWidgets(
    'long multilingual labels and isolated clusters fit narrow screens',
    (tester) async {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      final graph = core.MemoryGraph(
        nodes: List.generate(
          8,
          (i) => core.MemoryGraphNode(
            id: '$i',
            label: '一段很长的中文知识图谱记忆标题，包含 English 和换行\n第二行详细内容，应该截断而不是撑开节点。',
            color: 0xFF2196F3,
            metadata: const {},
          ),
        ),
        edges: const [],
      );
      await mount(tester, controller, graph: graph, size: const Size(360, 500));
      expect(controller.scale, lessThan(1));
      expect(controller.scale, greaterThan(MemoryGraphController.minScale));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('resize preserves scale and the world point at viewport center', (
    tester,
  ) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(tester, controller);
    controller.zoomAt(const Offset(200, 120), 2);
    controller.panBy(const Offset(30, 20));
    await tester.pump();
    final centerWorld = controller.screenToWorld(const Offset(300, 200));
    await mount(tester, controller, size: const Size(400, 300));
    expect(controller.scale, 2);
    nearOffset(controller.screenToWorld(const Offset(200, 150)), centerWorld);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'data replacement rebuilds layout and refits without disposed paragraphs',
    (tester) async {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      await mount(tester, controller);
      controller.zoomAt(const Offset(300, 200), 4);
      await tester.pump();
      await mount(
        tester,
        controller,
        graph: const core.MemoryGraph(
          nodes: [
            node,
            core.MemoryGraphNode(
              id: 'two',
              label: '第二条',
              color: 0xFFFFAA00,
              metadata: {},
            ),
          ],
          edges: [],
        ),
      );
      expect(controller.scale, lessThanOrEqualTo(1));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('relation segments can still be selected after zoom', (
    tester,
  ) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    core.MemoryGraphEdge? selected;
    const graph = core.MemoryGraph(
      nodes: [
        node,
        core.MemoryGraphNode(
          id: 'two',
          label: '另一节点',
          color: 0xFF2196F3,
          metadata: {},
        ),
      ],
      edges: [
        core.MemoryGraphEdge(
          id: 42,
          sourceId: 'one',
          targetId: 'two',
          label: '关联',
          weight: 1,
          metadata: {},
          isCrossFolderLink: false,
        ),
      ],
    );
    await mount(
      tester,
      controller,
      graph: graph,
      onSelect: (_, edge) => selected = edge,
    );
    // The midpoint between these two nodes is also the fitted graph center.
    final center = tester.getCenter(gestureCanvas);
    final localCenter = center - tester.getTopLeft(gestureCanvas);
    controller.zoomAt(localCenter, 1.4);
    await tester.pump();
    await tester.tapAt(center);
    await tester.pumpAndSettle();
    expect(selected?.id, 42);
  });

  testWidgets(
    'hundreds of nodes and cross-folder links can be navigated without widget rebuilds',
    (tester) async {
      final controller = MemoryGraphController();
      addTearDown(controller.dispose);
      final graph = core.MemoryGraph(
        nodes: List.generate(
          500,
          (i) => core.MemoryGraphNode(
            id: '$i',
            label: '知识节点 $i',
            color: 0xFF2196F3,
            metadata: const {},
          ),
        ),
        edges: List.generate(
          499,
          (i) => core.MemoryGraphEdge(
            id: i,
            sourceId: '$i',
            targetId: '${i + 1}',
            label: '关系',
            weight: 0.5,
            metadata: const {},
            isCrossFolderLink: i % 10 == 0,
          ),
        ),
      );
      await mount(tester, controller, graph: graph);
      final painter = tester.widget<CustomPaint>(graphPaint).painter;
      for (var i = 0; i < 20; i++) {
        controller.zoomAt(const Offset(200, 150), 1.08);
        controller.panBy(const Offset(-14, 8));
        await tester.pump();
      }
      expect(
        identical(tester.widget<CustomPaint>(graphPaint).painter, painter),
        isTrue,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('invalid edges and empty graphs do not break layout', (
    tester,
  ) async {
    final controller = MemoryGraphController();
    addTearDown(controller.dispose);
    await mount(
      tester,
      controller,
      graph: const core.MemoryGraph(
        nodes: [node],
        edges: [
          core.MemoryGraphEdge(
            id: 1,
            sourceId: 'one',
            targetId: 'missing',
            label: '悬空关系',
            weight: 1,
            metadata: {},
            isCrossFolderLink: false,
          ),
        ],
      ),
    );
    expect(tester.takeException(), isNull);
    await mount(
      tester,
      controller,
      graph: const core.MemoryGraph(nodes: [], edges: []),
    );
    expect(tester.takeException(), isNull);
  });
}
