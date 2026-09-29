import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:webview_all_windows/src/windows_native_composition.dart';

/// Verifies native scene placement and Flutter input occlusion without WebView2.
void main() {
  testWidgets('native browser emits a platform layer with logical dimensions', (
    tester,
  ) async {
    await tester.pumpWidget(
      const Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: SizedBox(
            width: 240,
            height: 120,
            child: WindowsNativeComposition(viewId: 1099511627776),
          ),
        ),
      ),
    );
    final layers = tester.layers.whereType<PlatformViewLayer>().toList();
    expect(layers, hasLength(1));
    expect(layers.single.viewId, 1099511627776);
    expect(layers.single.rect, const Rect.fromLTWH(0, 0, 240, 120));
    expect(tester.layers.whereType<TextureLayer>(), isEmpty);
  });

  testWidgets('modal barrier intercepts input above a native browser', (
    tester,
  ) async {
    var browserPresses = 0;
    var barrierPresses = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Stack(
          fit: StackFit.expand,
          children: [
            Listener(
              onPointerDown: (_) => browserPresses++,
              child: const WindowsNativeComposition(viewId: 7),
            ),
            GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => barrierPresses++,
              child: const ColoredBox(color: Color(0x80000000)),
            ),
          ],
        ),
      ),
    );
    await tester.tapAt(const Offset(100, 100));
    expect(browserPresses, 0);
    expect(barrierPresses, 1);
  });

  testWidgets('removing native content removes its scene layer', (
    tester,
  ) async {
    await tester.pumpWidget(
      const SizedBox(
        width: 50,
        height: 50,
        child: WindowsNativeComposition(viewId: 9),
      ),
    );
    expect(tester.layers.whereType<PlatformViewLayer>(), hasLength(1));
    await tester.pumpWidget(const SizedBox());
    expect(tester.layers.whereType<PlatformViewLayer>(), isEmpty);
  });
}
