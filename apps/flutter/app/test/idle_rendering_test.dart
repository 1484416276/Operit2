import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:liquid_glass_widgets/liquid_glass_widgets.dart';
import 'package:liquid_glass_widgets/widgets/shared/glass_effect.dart';
import 'package:liquid_glass_widgets/widgets/interactive/liquid_glass_scope.dart';

void main() {
  testWidgets('glass background updates without sustaining idle frames', (
    tester,
  ) async {
    final color = ValueNotifier<Color>(Colors.blue);
    addTearDown(color.dispose);
    await tester.pumpWidget(
      MaterialApp(
        home: LiquidGlassScope(
          child: Stack(
            fit: StackFit.expand,
            children: [
              GlassBackgroundSource(
                child: ValueListenableBuilder<Color>(
                  valueListenable: color,
                  builder: (_, value, _) => ColoredBox(color: value),
                ),
              ),
              Center(
                child: GlassEffect(
                  shape: const LiquidRoundedSuperellipse(borderRadius: 20),
                  settings: const LiquidGlassSettings(blur: 5),
                  interactionIntensity: 0.85,
                  child: const SizedBox(width: 200, height: 80),
                ),
              ),
            ],
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(tester.binding.hasScheduledFrame, isFalse);
    expect(tester.binding.transientCallbackCount, 0);
    final boundary = tester.allRenderObjects
        .whereType<GlassCaptureRenderBoundary>()
        .toSet()
        .single;
    final before = boundary.paintRevision;
    color.value = Colors.red;
    await tester.pumpAndSettle();
    expect(boundary.paintRevision, greaterThan(before));
    expect(tester.binding.hasScheduledFrame, isFalse);
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
    expect(tester.takeException(), isNull);
  });

}
