import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:operit2/ui/features/packages/screens/ToolPkgComposeDslWebViewResourceServer.dart';

/// Verifies intercepted VFS resources and isolation between browser instances.
void main() {
  test(
    'file responses use host paths and preserve response metadata',
    () async {
      final paths = <String>[];
      final server = ComposeDslWebViewResourceServer(
        dispatchDecision: (request) async {
          expect(request['url'], 'https://arcade.test/arcade/lobby');
          return {
            'action': 'respond',
            'response': {
              'filePath': '/app/data/cache/toolpkg_resource_exports/game.html',
              'mimeType': 'text/html',
              'statusCode': 200,
              'headers': {'Cache-Control': 'no-store'},
            },
          };
        },
        readFileBytes: (path) async {
          paths.add(path);
          return utf8.encode('<html>小游戏</html>');
        },
      );
      final client = HttpClient();
      addTearDown(() async {
        client.close(force: true);
        await server.close();
      });
      final uri = await server.localUriFor(
        'https://arcade.test/arcade/lobby',
        isMainFrame: true,
      );
      final response = await (await client.getUrl(uri)).close();
      expect(response.statusCode, 200);
      expect(response.headers.contentType?.mimeType, 'text/html');
      expect(response.headers.value('cache-control'), 'no-store');
      expect(await utf8.decoder.bind(response).join(), '<html>小游戏</html>');
      expect(paths, ['/app/data/cache/toolpkg_resource_exports/game.html']);
    },
  );

  test(
    'multiple browser instances retain independent origins and resources',
    () async {
      final servers = List.generate(
        3,
        (index) => ComposeDslWebViewResourceServer(
          dispatchDecision: (request) async {
            expect(request['url'], 'https://page$index.test/assets/page.html');
            return {
              'action': 'respond',
              'response': {
                'filePath': '/vfs/page$index.html',
                'mimeType': 'text/html',
              },
            };
          },
          readFileBytes: (path) async => utf8.encode(path),
        ),
      );
      final client = HttpClient();
      addTearDown(() async {
        client.close(force: true);
        for (final server in servers) {
          await server.close();
        }
      });
      final uris = await Future.wait(
        List.generate(
          3,
          (index) => servers[index].localUriFor(
            'https://page$index.test/assets/page.html',
            isMainFrame: true,
          ),
        ),
      );
      expect(uris.map((uri) => uri.port).toSet().length, 3);
      for (var round = 0; round < 2; round++) {
        await Future.wait(
          List.generate(3, (index) async {
            final response = await (await client.getUrl(uris[index])).close();
            expect(response.statusCode, 200);
            expect(
              await utf8.decoder.bind(response).join(),
              '/vfs/page$index.html',
            );
          }),
        );
      }
    },
  );
}
