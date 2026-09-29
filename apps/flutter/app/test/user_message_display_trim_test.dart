import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/features/chat/components/style/bubble/BubbleUserMessageComposable.dart'
    as bubble;
import 'package:operit2/ui/features/chat/components/style/cursor/UserMessageComposable.dart'
    as cursor;

/// Verifies display trimming after attachment extraction in both message styles.
void main() {
  const attachment =
      '<attachment id="moodlet_force_directive" filename="Moodlet 强制模式" '
      'type="text/plain">  directive\n </attachment>';

  for (final style in ['cursor', 'bubble']) {
    /// Checks the shared display contract using each style's concrete parser.
    void verify(String source, String expected) {
      if (style == 'cursor') {
        final result = cursor.parseMessageContent(source);
        expect(result.processedText, expected);
        expect(result.trailingAttachments.single.content, '  directive\n ');
      } else {
        final result = bubble.parseMessageContent(source);
        expect(result.processedText, expected);
        expect(result.trailingAttachments.single.content, '  directive\n ');
      }
    }

    test('$style trims blank lines left by a trailing attachment', () {
      verify('  你还\n\n$attachment\n ', '你还');
    });

    test('$style preserves whitespace within the message body', () {
      verify('\n 第一行  \n\n  第二行\n\n$attachment', '第一行  \n\n  第二行');
    });

    test('$style produces empty text for an attachment-only message', () {
      verify(' \n$attachment\n ', '');
    });
  }
}
