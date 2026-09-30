import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/features/packages/screens/ToolPkgComposeDslWebView.dart';
import 'package:operit2/ui/features/packages/screens/ToolPkgUiLauncherScreen.dart';

/// Builds the real renderer without initializing a runtime or loading a plugin.
Widget renderNode(Map<String, Object?> node) => buildComposeDslLayoutForTest(
  node: node,
  onTextInput: (id, text) async => null,
  hostContext: ComposeDslWebViewHostContext(
    packageName: 'test.package',
    routeInstanceId: 'layout-test',
    executionContextKey: 'layout-test',
    dispatchAction: (id, [payload]) async => null,
    runtimeOptionsProvider: () => <String, Object?>{},
  ),
  splitMarkdownContent: (_) async => [],
);

/// Checks actual Card and Row nesting through both DSL content encodings.
void main() {
  for (final type in ['Card', 'ElevatedCard', 'OutlinedCard']) {
    for (final useSlot in [false, true]) {
      testWidgets('$type in Row with content slot=$useSlot', (tester) async {
        final content = <Map<String, Object?>>[
          {
            'type': 'Box',
            'props': {'padding': 8},
            'children': [
              {
                'type': 'Text',
                'props': {'text': 'Nested content'},
              },
            ],
          },
        ];
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: Center(
                child: SizedBox(
                  width: 668,
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      renderNode({
                        'type': 'Row',
                        'children': [
                          {
                            'type': type,
                            if (useSlot) 'slots': {'content': content},
                            if (!useSlot) 'children': content,
                          },
                        ],
                      }),
                    ],
                  ),
                ),
              ),
            ),
          ),
        );
        expect(tester.takeException(), isNull);
        expect(find.text('Nested content'), findsOneWidget);
        final size = tester.getSize(find.byType(Card));
        expect(size.width, greaterThan(0));
        expect(size.width, lessThan(668));
        expect(size.height, greaterThan(0));
        expect(size.isFinite, isTrue);
      });
    }
  }

  for (final useSlot in [false, true]) {
    testWidgets(
      'Card content keeps natural width under bounded parent, slot=$useSlot',
      (tester) async {
        final content = <Map<String, Object?>>[
          {
            'type': 'Box',
            'props': {'width': 40, 'height': 30, 'key': 'natural'},
          },
          {
            'type': 'Box',
            'props': {'fillMaxWidth': true, 'height': 20, 'key': 'filled'},
          },
        ];
        await tester.pumpWidget(
          MaterialApp(
            home: Center(
              child: SizedBox(
                width: 300,
                child: renderNode({
                  'type': 'Card',
                  if (useSlot) 'slots': {'content': content},
                  if (!useSlot) 'children': content,
                }),
              ),
            ),
          ),
        );
        expect(tester.takeException(), isNull);
        expect(
          tester.getSize(find.byKey(const ValueKey<Object?>('natural'))),
          const Size(40, 30),
        );
        expect(
          tester.getSize(find.byKey(const ValueKey<Object?>('filled'))),
          const Size(300, 20),
        );
      },
    );
  }
}
