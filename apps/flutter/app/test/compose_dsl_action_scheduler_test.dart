import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/ui/features/packages/screens/compose_dsl/action_scheduler.dart';

/// Verifies text ordering and independent ordinary action completion.
void main() {
  test('ordinary actions start without waiting for each other', () async {
    final scheduler = ComposeDslActionScheduler();
    final first = Completer<String>();
    final events = <String>[];
    final a = scheduler.dispatchAction(() {
      events.add('first');
      return first.future;
    });
    final b = scheduler.dispatchAction(() async {
      events.add('second');
      return 'second result';
    });
    expect(await b, 'second result');
    expect(events, ['first', 'second']);
    expect(first.isCompleted, isFalse);
    first.complete('first result');
    expect(await a, 'first result');
  });

  test(
    'text edits serialize across fields and actions observe settled text',
    () async {
      final scheduler = ComposeDslActionScheduler();
      final first = Completer<void>();
      final second = Completer<void>();
      final secondStarted = Completer<void>();
      final events = <String>[];
      final a = scheduler.dispatchTextInput(() async {
        events.add('field1:start');
        await first.future;
        events.add('field1:done');
      });
      final b = scheduler.dispatchTextInput(() async {
        events.add('field2:start');
        secondStarted.complete();
        await second.future;
        events.add('field2:done');
      });
      final action = scheduler.dispatchAction(() async => events.add('action'));
      expect(events, ['field1:start']);
      first.complete();
      await a;
      await secondStarted.future;
      expect(events, ['field1:start', 'field1:done', 'field2:start']);
      second.complete();
      await Future.wait([b, action]);
      expect(events, [
        'field1:start',
        'field1:done',
        'field2:start',
        'field2:done',
        'action',
      ]);
    },
  );

  test(
    'actions sharing a text barrier do not serialize with each other',
    () async {
      final scheduler = ComposeDslActionScheduler();
      final text = Completer<void>();
      final slow = Completer<void>();
      final events = <String>[];
      final edit = scheduler.dispatchTextInput(() => text.future);
      final a = scheduler.dispatchAction(() {
        events.add('slow');
        return slow.future;
      });
      final b = scheduler.dispatchAction(() async => events.add('fast'));
      expect(events, isEmpty);
      text.complete();
      await edit;
      await b;
      expect(events, ['slow', 'fast']);
      expect(slow.isCompleted, isFalse);
      slow.complete();
      await a;
    },
  );

  test('edits do not wait for an already running ordinary action', () async {
    final scheduler = ComposeDslActionScheduler();
    final slow = Completer<void>();
    final action = scheduler.dispatchAction(() => slow.future);
    expect(await scheduler.dispatchTextInput(() async => 'edited'), 'edited');
    expect(slow.isCompleted, isFalse);
    slow.complete();
    await action;
  });

  test(
    'text failures reach callers and release the synchronization barrier',
    () async {
      final scheduler = ComposeDslActionScheduler();
      final error = StateError('text dispatch failed');
      final failed = scheduler.dispatchTextInput<void>(
        () => Future.error(error),
      );
      final assertion = expectLater(failed, throwsA(same(error)));
      final next = scheduler.dispatchTextInput(() async => 'next');
      final action = scheduler.dispatchAction(() async => 'action');
      await assertion;
      expect(await next, 'next');
      expect(await action, 'action');
    },
  );

  test(
    'an action rechecks text added while waiting for the previous edit',
    () async {
      final scheduler = ComposeDslActionScheduler();
      final first = Completer<void>();
      final second = Completer<void>();
      final secondStarted = Completer<void>();
      final events = <String>[];
      final a = scheduler.dispatchTextInput(() => first.future);
      final action = scheduler.dispatchAction(() async => events.add('action'));
      final b = scheduler.dispatchTextInput(() async {
        events.add('second');
        secondStarted.complete();
        await second.future;
      });
      first.complete();
      await a;
      await secondStarted.future;
      expect(events, ['second']);
      second.complete();
      await Future.wait([b, action]);
      expect(events, ['second', 'action']);
    },
  );
}
