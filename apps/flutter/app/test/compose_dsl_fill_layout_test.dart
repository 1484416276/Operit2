import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/features/packages/screens/compose_dsl/fill_layout.dart';

/// Checks fill measurement without turning an unbounded axis into a tight one.
void main() {
  for (final axis in Axis.values) {
    testWidgets('fills only the bounded axis inside $axis scroll', (
      tester,
    ) async {
      const contentKey = ValueKey('content');
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Center(
            child: SizedBox(
              width: 300,
              height: 200,
              child: SingleChildScrollView(
                scrollDirection: axis,
                child: const ComposeDslFill(
                  fillWidth: true,
                  fillHeight: true,
                  child: SizedBox(key: contentKey, width: 40, height: 30),
                ),
              ),
            ),
          ),
        ),
      );
      expect(tester.takeException(), isNull);
      expect(
        tester.getSize(find.byKey(contentKey)),
        axis == Axis.vertical ? const Size(300, 30) : const Size(40, 200),
      );
    });
  }

  testWidgets('non-flex Column child has a finite height', (tester) async {
    const contentKey = ValueKey('content');
    await tester.pumpWidget(
      const Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: SizedBox(
            width: 668,
            height: 400,
            child: Column(
              children: [
                ComposeDslFill(
                  fillWidth: true,
                  fillHeight: true,
                  child: SizedBox(key: contentKey, width: 40, height: 30),
                ),
              ],
            ),
          ),
        ),
      ),
    );
    expect(tester.takeException(), isNull);
    expect(tester.getSize(find.byKey(contentKey)), const Size(668, 30));
  });

  testWidgets('fraction and axis changes update the existing render object', (
    tester,
  ) async {
    const key = ValueKey('fill');

    /// Provides bounded loose constraints to the fill modifier.
    Widget screen(bool width, bool height, double fraction) => Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 300, maxHeight: 200),
        child: ComposeDslFill(
          key: key,
          fillWidth: width,
          fillHeight: height,
          fraction: fraction,
          child: const SizedBox(width: 40, height: 30),
        ),
      ),
    );
    await tester.pumpWidget(screen(true, true, 1));
    final render = tester.renderObject<RenderBox>(find.byKey(key));
    expect(render.size, const Size(300, 200));
    await tester.pumpWidget(screen(true, true, 0.5));
    expect(tester.renderObject(find.byKey(key)), same(render));
    expect(render.size, const Size(150, 100));
    await tester.pumpWidget(screen(false, true, 0.5));
    expect(render.size, const Size(40, 100));
    await tester.pumpWidget(screen(true, false, 0.5));
    expect(render.size, const Size(150, 30));
    await tester.pumpWidget(screen(true, true, 2));
    expect(render.size, const Size(300, 200));
    await tester.pumpWidget(screen(true, true, 0));
    expect(render.size, Size.zero);
    expect(tester.takeException(), isNull);
  });

  testWidgets('dry layout preserves unbounded axes and parent minimums', (
    tester,
  ) async {
    const key = ValueKey('fill');
    await tester.pumpWidget(
      const Center(
        child: ComposeDslFill(
          key: key,
          fillWidth: true,
          fillHeight: true,
          fraction: 0.5,
          child: SizedBox(width: 40, height: 30),
        ),
      ),
    );
    final render = tester.renderObject<RenderBox>(find.byKey(key));
    expect(render.getDryLayout(const BoxConstraints()), const Size(40, 30));
    expect(
      render.getDryLayout(
        const BoxConstraints(minWidth: 200, maxWidth: 300, maxHeight: 200),
      ),
      const Size(200, 100),
    );
    expect(
      render.getDryLayout(
        const BoxConstraints.tightFor(width: 300, height: 200),
      ),
      const Size(300, 200),
    );
    expect(tester.takeException(), isNull);
  });
}
