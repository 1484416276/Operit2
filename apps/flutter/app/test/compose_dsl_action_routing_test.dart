import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/features/packages/screens/ToolPkgComposeDslWebView.dart';
import 'package:operit2/ui/features/packages/screens/ToolPkgUiLauncherScreen.dart';
import 'package:operit2/ui/features/packages/screens/compose_dsl/action_scheduler.dart';

/// Exercises the typed text dispatch path through actual nested DSL controls.
void main() {
  testWidgets(
    'nested text fields synchronize before switches without joining the ordinary action queue',
    (tester) async {
      final scheduler = ComposeDslActionScheduler();
      final slowAction = Completer<Object?>();
      final textSync = Completer<Object?>();
      final events = <String>[];

      /// Records ordinary actions while leaving the first switch action running.
      Future<Object?> action(String id, [Object? payload]) =>
          scheduler.dispatchAction(() {
            events.add('$id:$payload');
            return id == 'slow' ? slowAction.future : Future<Object?>.value();
          });

      /// Sends only text synchronization through the serial input queue.
      Future<Object?> text(String id, String value) =>
          scheduler.dispatchTextInput(() {
            events.add('$id:$value');
            return textSync.future;
          });

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: buildComposeDslLayoutForTest(
              node: {
                'type': 'Column',
                'children': [
                  {
                    'type': 'Switch',
                    'props': {'checked': false, 'onCheckedChange': 'slow'},
                  },
                  {
                    'type': 'Card',
                    'slots': {
                      'content': [
                        {
                          'type': 'OutlinedTextField',
                          'props': {
                            'value': '',
                            'onValueChange': 'edit',
                            'singleLine': true,
                          },
                        },
                        {
                          'type': 'Switch',
                          'props': {
                            'checked': false,
                            'onCheckedChange': 'fast',
                          },
                        },
                      ],
                    },
                  },
                ],
              },
              hostContext: ComposeDslWebViewHostContext(
                routeInstanceId: 'dispatch-test',
                executionContextKey: 'dispatch-test',
                dispatchAction: action,
                runtimeOptionsProvider: () => {},
              ),
              onTextInput: text,
              splitMarkdownContent: (_) async => [],
            ),
          ),
        ),
      );
      await tester.tap(find.byType(Switch).first);
      await tester.pump();
      expect(events, ['slow:true']);
      await tester.enterText(find.byType(TextField), 'latest');
      await tester.pump();
      expect(events, ['slow:true', 'edit:latest']);
      await tester.tap(find.byType(Switch).last);
      await tester.pump();
      expect(events, ['slow:true', 'edit:latest']);
      textSync.complete();
      await tester.pump();
      expect(events, ['slow:true', 'edit:latest', 'fast:true']);
      expect(slowAction.isCompleted, isFalse);
      slowAction.complete();
      await tester.pump();
      expect(tester.takeException(), isNull);
    },
  );
}
