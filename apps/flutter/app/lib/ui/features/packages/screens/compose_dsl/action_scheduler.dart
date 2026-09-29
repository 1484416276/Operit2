import 'dart:async';

/// Serializes text synchronization without serializing unrelated UI actions.
class ComposeDslActionScheduler {
  Future<void>? _textInputTail;

  /// Queues an edit and exposes its own result independently of the drain barrier.
  Future<T> dispatchTextInput<T>(Future<T> Function() dispatch) async {
    final previous = _textInputTail;
    final settled = Completer<void>();
    _textInputTail = settled.future;
    try {
      if (previous != null) await previous;
      return await dispatch();
    } finally {
      settled.complete();
      if (identical(_textInputTail, settled.future)) {
        _textInputTail = null;
      }
    }
  }

  /// Waits for pending edits, then starts an action without blocking other actions.
  Future<T> dispatchAction<T>(Future<T> Function() dispatch) async {
    while (_textInputTail != null) {
      await _textInputTail;
    }
    return dispatch();
  }
}
