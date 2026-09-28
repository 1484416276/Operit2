import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:operit2/core/bridge/OperitRuntimeBridge.dart';
import 'package:operit2/core/link/CoreLinkCodec.dart';
import 'package:operit2/core/link/CoreLinkProtocol.dart';
import 'package:operit2/core/proxy/generated/CoreProxyClients.g.dart';
import 'package:operit2/l10n/generated/app_localizations.dart';
import 'package:operit2/ui/features/chat/components/attachments/MemoryAttachmentDialog.dart';

void main() {
  testWidgets(
    'scopes folder lookup to selected owner and requires preview confirmation',
    (tester) async {
      final bridge = _MemoryBridge();
      String? attachment;
      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () async {
                  attachment = await showDialog<String>(
                    context: context,
                    builder: (_) => MemoryAttachmentDialog(
                      clients: GeneratedCoreProxyClients(bridge),
                    ),
                  );
                },
                child: const Text('Open'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Shared notes'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('work'));
      await tester.pumpAndSettle();
      expect(attachment, isNull);
      expect(find.textContaining('Remember this detail'), findsOneWidget);
      expect(bridge.owner, 'shared:notes');
      expect(bridge.folder, 'work');
      await tester.tap(find.byType(FilledButton));
      await tester.pumpAndSettle();
      expect(attachment, contains('Remember this detail'));
      expect(attachment, contains('Shared notes / work'));
    },
  );

  testWidgets(
    'empty repositories show an empty state without an attach action',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: MemoryAttachmentDialog(
            clients: GeneratedCoreProxyClients(_MemoryBridge(empty: true)),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('No data'), findsOneWidget);
      expect(find.byType(FilledButton), findsNothing);
    },
  );
}

class _MemoryBridge extends OperitRuntimeBridge {
  _MemoryBridge({this.empty = false});
  final bool empty;
  String? owner;
  String? folder;

  @override
  Future<Uint8List> callBytes(CoreCallRequest request) async {
    Object? value;
    switch (request.methodName) {
      case 'getAllCharacterCards':
        value = [];
      case 'getAllSharedMemoryStores':
        value = empty
            ? []
            : [
                {
                  'id': 'notes',
                  'name': 'Shared notes',
                  'createdAt': 0,
                  'updatedAt': 0,
                },
              ];
      case 'getAllFolderPaths':
        owner = (request.args as Map)['__core_instance_id'] as String;
        value = ['work'];
      case 'getMemoriesByFolderPath':
        owner = (request.args as Map)['__core_instance_id'] as String;
        folder = (request.args as Map)['folderPath'] as String;
        value = [
          {
            'id': 1,
            'uuid': 'one',
            'title': 'Project',
            'content': 'Remember this detail',
            'contentType': 'text/plain',
            'source': 'test',
            'credibility': 1.0,
            'importance': 1.0,
            'documentPath': null,
            'isDocumentNode': false,
            'chunkIndexFilePath': null,
            'folderPath': 'work',
            'createdAt': 0,
            'updatedAt': 0,
            'lastAccessedAt': 0,
            'tags': [],
            'properties': [],
          },
        ];
      default:
        throw StateError('Unexpected call: ${request.methodName}');
    }
    return encodeCoreLink([0, value]);
  }

  @override
  Future<CorePushSink> push(CorePushRequest request) =>
      throw StateError('Unexpected push');
  @override
  Future<CoreEvent> watchSnapshot(CoreWatchRequest request) =>
      throw StateError('Unexpected snapshot');
  @override
  Stream<CoreEvent> watchStream(CoreWatchRequest request) =>
      throw StateError('Unexpected watch');
}
