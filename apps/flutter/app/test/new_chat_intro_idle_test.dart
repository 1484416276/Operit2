import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/features/chat/components/NewChatIntro.dart';

Widget _app(String id, {bool empty = true, bool switching = false}) =>
    MaterialApp(
      home: NewChatIntroOverlay(
        currentChatId: id,
        isChatEmpty: empty,
        isSwitching: switching,
      ),
    );

void main() {
  setUp(() {
    newChatIntroArmed.value = false;
    newChatIntroActive.value = false;
  });

  testWidgets('new chat greeting settles without requesting idle frames', (
    tester,
  ) async {
    await tester.pumpWidget(_app('existing'));
    newChatIntroArmed.value = true;
    await tester.pumpWidget(_app('new'));
    await tester.pump();
    for (var i = 0; i < 12; i++) {
      await tester.pump(const Duration(milliseconds: 500));
    }
    expect(find.text('有什么可以帮你？'), findsOneWidget);
    expect(find.text('新对话已就绪，随时开始'), findsOneWidget);
    expect(newChatIntroActive.value, isTrue);
    expect(tester.binding.transientCallbackCount, 0);
    expect(tester.binding.hasScheduledFrame, isFalse);
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    expect(newChatIntroActive.value, isFalse);
    expect(tester.takeException(), isNull);
  });

  testWidgets('sending a message stops the intro before it completes', (
    tester,
  ) async {
    await tester.pumpWidget(_app('existing'));
    newChatIntroArmed.value = true;
    await tester.pumpWidget(_app('new'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpWidget(_app('new', empty: false));
    await tester.pump();
    expect(newChatIntroActive.value, isFalse);
    expect(tester.binding.transientCallbackCount, 0);
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    expect(tester.takeException(), isNull);
  });

  testWidgets('switching to an existing empty chat does not animate', (
    tester,
  ) async {
    await tester.pumpWidget(_app('existing'));
    await tester.pumpWidget(_app('another-existing'));
    await tester.pump();
    expect(newChatIntroActive.value, isFalse);
    expect(tester.binding.transientCallbackCount, 0);
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    expect(tester.takeException(), isNull);
  });
}
