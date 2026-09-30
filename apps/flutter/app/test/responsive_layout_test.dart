import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/common/components/AdaptiveSidePanel.dart';
import 'package:operit2/ui/common/layout/ResponsiveLayout.dart';
import 'package:operit2/ui/main/layout/NavigationLayoutMetrics.dart';

/// Verifies shared decisions without changing the existing viewport scale policy.
void main() {
  test('wide layout switches at the shared logical width boundary', () {
    expect(ResponsiveLayout.usesWideLayout(599), isFalse);
    expect(ResponsiveLayout.usesWideLayout(600), isTrue);
    expect(ResponsiveLayout.usesWideLayout(601), isTrue);
  });

  test(
    'portrait classification uses raw size and the existing aspect ratio',
    () {
      expect(
        ResponsiveLayout.isPortraitCompactViewport(const Size(400, 460)),
        isTrue,
      );
      expect(
        ResponsiveLayout.isPortraitCompactViewport(const Size(400, 459)),
        isFalse,
      );
      expect(
        ResponsiveLayout.isPortraitCompactViewport(const Size(600, 900)),
        isFalse,
      );
      expect(
        ResponsiveLayout.isPortraitCompactViewport(const Size(560, 420)),
        isFalse,
      );
    },
  );

  test(
    'viewport scaling retains the PR policy including its current boundary',
    () {
      expect(resolveViewportScale(const Size(599, 900)), 1);
      expect(resolveViewportScale(const Size(600, 900)), 0.8);
      expect(resolveViewportScale(const Size(560, 420)), 0.68);
      expect(
        resolveViewportScale(const Size(320, 640)),
        closeTo(320 / 360, 1e-9),
      );
      expect(resolveViewportScale(const Size(1160, 720)), 1);
    },
  );

  test('local split decisions retain default and custom panel thresholds', () {
    expect(
      ResponsiveLayout.usesSideBySideLayout(
        519,
        minPanelWidth: 240,
        minContentWidth: 280,
      ),
      isFalse,
    );
    expect(
      ResponsiveLayout.usesSideBySideLayout(
        520,
        minPanelWidth: 240,
        minContentWidth: 280,
      ),
      isTrue,
    );
    expect(
      ResponsiveLayout.usesSideBySideLayout(
        599,
        minPanelWidth: 280,
        minContentWidth: 320,
      ),
      isFalse,
    );
    expect(
      ResponsiveLayout.usesSideBySideLayout(
        600,
        minPanelWidth: 280,
        minContentWidth: 320,
      ),
      isTrue,
    );
    expect(
      ResponsiveLayout.usesSideBySideLayout(
        499,
        breakpoint: 500,
        minPanelWidth: 280,
        minContentWidth: 320,
      ),
      isFalse,
    );
    expect(
      ResponsiveLayout.usesSideBySideLayout(
        500,
        breakpoint: 500,
        minPanelWidth: 280,
        minContentWidth: 320,
      ),
      isTrue,
    );
  });

  testWidgets('application decisions read the scaled viewport', (tester) async {
    for (final size in <Size>[const Size(400, 800), const Size(560, 420)]) {
      late Size layoutSize;
      late bool wide;
      await tester.pumpWidget(
        MediaQuery(
          data: MediaQueryData(size: size),
          child: ResponsiveViewportBox(
            child: Builder(
              builder: (context) {
                layoutSize = MediaQuery.sizeOf(context);
                wide = ResponsiveLayout.usesWideLayoutOf(context);
                return const SizedBox.expand();
              },
            ),
          ),
        ),
      );
      expect(
        layoutSize.width,
        closeTo(size.width / resolveViewportScale(size), 1e-9),
      );
      expect(wide, ResponsiveLayout.usesWideLayout(layoutSize.width));
    }
  });

  testWidgets('side panels use local constraints inside a wide viewport', (
    tester,
  ) async {
    const panelKey = ValueKey('panel');
    for (final width in <double>[519, 520]) {
      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(size: Size(1200, 800)),
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: Center(
              child: SizedBox(
                width: width,
                height: 400,
                child: AdaptiveSidePanel(
                  open: true,
                  animate: false,
                  onOpenChanged: (_) {},
                  minWidth: 240,
                  minContentWidth: 280,
                  panel: const SizedBox.expand(key: panelKey),
                  child: const SizedBox.expand(),
                ),
              ),
            ),
          ),
        ),
      );
      expect(
        tester.getSize(find.byKey(panelKey)).width,
        width == 519 ? 519 : 240,
      );
      expect(tester.takeException(), isNull);
    }
  });
}
