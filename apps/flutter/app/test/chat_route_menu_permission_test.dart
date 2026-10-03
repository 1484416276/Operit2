import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyModels.g.dart';
import 'package:operit2/ui/features/settings/memory/MemoryOwnerControlsDialog.dart';
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/features/chat/components/ChatToolPermissionPanel.dart';
import 'package:operit2/ui/features/chat/components/style/input/agent/AgentInputMenuPopup.dart';
import 'package:operit2/ui/features/chat/viewmodel/ChatViewModel.dart';

const _settings = ChatInputMenuSettings(
  enableMemoryAutoUpdate: false,
  permissionMode: AiPermissionMode.full,
  disableStreamOutput: true,
  disableUserPreferenceDescription: true,
  pluginChangeVersion: 42,
  pluginToggles: <InputMenuToggleDefinitionSnapshot>[],
);

RuntimeHostInteractionToolPermissionRequest _request(String id) =>
    RuntimeHostInteractionToolPermissionRequest(
      requestId: id,
      chatId: 'computer-chat',
      tool: const RuntimeHostInteractionToolPermissionTool(
        name: 'write_file',
        parameters: <RuntimeHostInteractionToolPermissionToolParameter>[],
      ),
      description: 'Write a file on the computer',
      requestedAtMillis: 1,
    );

Widget _app(Widget child) => MaterialApp(
  locale: const Locale('zh'),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: Align(alignment: Alignment.bottomCenter, child: child),
  ),
);

void main() {
  testWidgets('menu settings and edits use the explicit owning chat', (
    tester,
  ) async {
    final bridge = _MenuBridge();
    final viewModel = ChatViewModel(bridge: bridge);
    await tester.pumpWidget(
      _app(
        AgentInputMenuPopup(
          viewModel: viewModel,
          currentChatId: 'computer-chat',
          currentCharacterCardName: null,
          currentCharacterCardAvatarUri: null,
          onDismiss: () {},
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('非流式'), findsOneWidget);
    expect(
      bridge.calls.any((c) => c.methodName == 'chatInputMenuSettings'),
      isTrue,
    );
    expect(
      bridge.calls.any((c) => c.methodName == 'chatInputMenuSummary'),
      isTrue,
    );
    expect(
      bridge.calls.any((c) => c.methodName == 'chatMemoryAutoSaveStatus'),
      isTrue,
    );
    await tester.tap(find.text('工具'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('只读'));
    await tester.pumpAndSettle();
    final save = bridge.calls.singleWhere(
      (c) => c.methodName == 'saveChatInputMenuSettings',
    );
    final args = save.args as Map<String, Object?>;
    expect(args['chatId'], 'computer-chat');
    expect(args['permissionMode'], AiPermissionMode.readOnly.toJson());
    expect(args['enableMemoryAutoUpdate'], isNull);
    expect(args['disableStreamOutput'], isNull);
    for (final call in bridge.calls) {
      expect((call.args as Map<String, Object?>)['chatId'], 'computer-chat');
      expect(call.target, 'core/chatRuntimeHolderMain');
    }
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('menu polls changes from owner and rebinds on chat switch', (
    tester,
  ) async {
    final bridge = _MenuBridge();
    final viewModel = ChatViewModel(bridge: bridge);
    Widget menu(String chatId) => _app(
      AgentInputMenuPopup(
        viewModel: viewModel,
        currentChatId: chatId,
        currentCharacterCardName: null,
        currentCharacterCardAvatarUri: null,
        onDismiss: () {},
      ),
    );
    await tester.pumpWidget(menu('computer-chat'));
    await tester.pumpAndSettle();
    bridge.disableStream = false;
    await tester.pump(const Duration(seconds: 1));
    await tester.pumpAndSettle();
    expect(find.text('流式'), findsOneWidget);
    bridge.calls.clear();
    await tester.pumpWidget(menu('other-computer-chat'));
    await tester.pumpAndSettle();
    expect(bridge.calls, isNotEmpty);
    for (final call in bridge.calls) {
      expect(
        (call.args as Map<String, Object?>)['chatId'],
        'other-computer-chat',
      );
    }
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('memory controls opened from chat never use local owner APIs', (
    tester,
  ) async {
    final bridge = _MenuBridge();
    final viewModel = ChatViewModel(bridge: bridge);
    await tester.pumpWidget(
      _app(
        MemoryOwnerControlsDialog(
          clients: viewModel.clients,
          ownerKey: '',
          chatCore: viewModel.chatCore,
          chatId: 'computer-chat',
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('所属记忆库：computer-owner'), findsOneWidget);
    await tester.tap(find.text('保存设置'));
    await tester.pumpAndSettle();
    final button = find.text('重建向量缓存（使用已保存设置）');
    await tester.ensureVisible(button);
    await tester.tap(button);
    await tester.pumpAndSettle();
    expect(
      bridge.calls.any((c) => c.methodName == 'saveChatMemorySettings'),
      isTrue,
    );
    expect(
      bridge.calls.any((c) => c.methodName == 'saveChatMemorySearchConfig'),
      isTrue,
    );
    expect(
      bridge.calls.any((c) => c.methodName == 'rebuildChatMemoryEmbeddings'),
      isTrue,
    );
    for (final call in bridge.calls) {
      expect(call.target, 'core/chatRuntimeHolderMain');
      expect((call.args as Map<String, Object?>)['chatId'], 'computer-chat');
    }
    await tester.pumpWidget(const SizedBox.shrink());
  });

  test(
    'permission view model forwards explicit chat and request ids',
    () async {
      final bridge = _MenuBridge();
      final viewModel = ChatViewModel(bridge: bridge);
      await viewModel.respondToolPermissionRequest(
        chatId: 'computer-chat',
        requestId: 'computer-request',
        result: ChatToolPermissionResult.allowSession,
      );
      expect(bridge.calls.single.methodName, 'respondChatToolPermission');
      expect(bridge.calls.single.args, <String, Object?>{
        'chatId': 'computer-chat',
        'requestId': 'computer-request',
        'result': 'allow_session',
      });
    },
  );

  testWidgets('phone approval sends request owner and unlocks next request', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(360, 700);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final decisions = <String>[];
    Future<void> respond(
      String chatId,
      String requestId,
      ChatToolPermissionResult result,
    ) async {
      expect(chatId, 'computer-chat');
      decisions.add('$requestId:${result.wireName}');
    }

    await tester.pumpWidget(
      _app(
        ChatToolPermissionPanel(request: _request('first'), onRespond: respond),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('允许本次'));
    await tester.pump();
    expect(decisions, <String>['first:allow']);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    // Replace without an empty frame, as when two requests are queued.
    await tester.pumpWidget(
      _app(
        ChatToolPermissionPanel(
          request: _request('second'),
          onRespond: respond,
        ),
      ),
    );
    await tester.pump();
    await tester.tap(find.text('允许本次'));
    await tester.pump();
    expect(decisions, <String>['first:allow', 'second:allow']);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('failed approval displays error and permits retry', (
    tester,
  ) async {
    var attempts = 0;
    await tester.pumpWidget(
      _app(
        ChatToolPermissionPanel(
          request: _request('retry'),
          onRespond: (_, _, _) async {
            attempts++;
            throw StateError('Computer disconnected');
          },
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('允许本次'));
    await tester.pump();
    expect(tester.takeException(), isA<StateError>());
    expect(find.textContaining('Computer disconnected'), findsOneWidget);
    await tester.tap(find.text('允许本次'));
    await tester.pump();
    expect(tester.takeException(), isA<StateError>());
    expect(attempts, 2);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}

class _MenuBridge extends OperitRuntimeBridge {
  final calls = <CoreCallRequest>[];
  bool disableStream = true;

  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    calls.add(request);
    final Object? value;
    switch (request.methodName) {
      case 'chatInputMenuSettings':
        value = <String, Object?>{
          ..._settings.toJson(),
          'disableStreamOutput': disableStream,
        };
      case 'chatInputMenuSummary':
        value = const ChatInputMenuSummary(
          currentWindowSize: 123,
          inputTokenCount: 456,
          outputTokenCount: 789,
          maxContextLength: 64,
        ).toJson();
      case 'chatMemoryAutoSaveStatus':
        value = const MemoryAutoSaveStatus(
          ownerKey: 'computer-owner',
          pendingCandidates: 1,
          pendingChats: 1,
          processingCandidates: 0,
          failedCandidates: 0,
          nextRunAtMs: 0,
          minutesUntilNextRun: 0,
          lastError: '',
        ).toJson();
      case 'chatMemorySettings':
        value = const MemorySettings(
          autoSaveIntervalMinutes: 5,
          nextAutoSaveRunAtMs: 0,
          memoryExtractionCustomRules: '',
          profileAutoUpdateEnabled: true,
          profileAutoUpdateLocked: false,
          cloudEmbeddingEnabled: true,
          cloudEmbeddingEndpoint: '',
          cloudEmbeddingApiKey: '',
          cloudEmbeddingModel: '',
        ).toJson();
      case 'chatMemorySearchConfig':
        value = const MemorySearchConfig(
          scoreMode: MemoryScoreMode.balanced,
          keywordWeight: 10,
          tagWeight: 0,
          vectorWeight: 0,
          edgeWeight: 0.4,
        ).toJson();
      case 'chatMemoryBoundChats':
        value = <Object?>[];
      case 'chatMemoryRebuildProgress':
        value = const MemoryRebuildProgress(
          status: 'idle',
          totalChats: 0,
          completedChats: 0,
          totalWindows: 0,
          completedWindows: 0,
          totalSourceMessages: 0,
          processedSourceMessages: 0,
          failedWindows: 0,
          currentChatTitle: '',
          lastError: '',
        ).toJson();
      case 'rebuildChatMemoryEmbeddings':
        value = 0;
      case 'saveChatInputMenuSettings':
      case 'saveChatMemorySettings':
      case 'saveChatMemorySearchConfig':
      case 'respondChatToolPermission':
        value = null;
      default:
        throw StateError(
          'Unexpected local API: ${request.target}.${request.methodName}',
        );
    }
    return encodeCoreLink(<Object?>[0, value]);
  }

  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw UnimplementedError();
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw UnimplementedError();
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw UnimplementedError();
}
