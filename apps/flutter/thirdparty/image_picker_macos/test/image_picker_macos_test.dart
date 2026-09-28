// Copyright 2013 The Flutter Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';
import 'package:image_picker_macos/image_picker_macos.dart';
import 'package:image_picker_platform_interface/image_picker_platform_interface.dart';
import 'package:mockito/annotations.dart';
import 'package:mockito/mockito.dart';

import 'image_picker_macos_test.mocks.dart';

@GenerateMocks(<Type>[FileSelectorPlatform])
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // Returns the captured type groups from a mock call result, assuming that
  // exactly one call was made and only the type groups were captured.
  List<XTypeGroup> capturedTypeGroups(VerificationResult result) {
    return result.captured.single as List<XTypeGroup>;
  }

  late ImagePickerMacOS plugin;
  late MockFileSelectorPlatform mockFileSelectorPlatform;

  setUp(() {
    plugin = ImagePickerMacOS();
    mockFileSelectorPlatform = MockFileSelectorPlatform();

    when(
      mockFileSelectorPlatform.openFile(
        acceptedTypeGroups: anyNamed('acceptedTypeGroups'),
      ),
    ).thenAnswer((_) async => null);

    when(
      mockFileSelectorPlatform.openFiles(
        acceptedTypeGroups: anyNamed('acceptedTypeGroups'),
      ),
    ).thenAnswer((_) async => List<XFile>.empty());

    ImagePickerMacOS.fileSelector = mockFileSelectorPlatform;
  });

  test('registered instance', () {
    ImagePickerMacOS.registerWith();
    expect(ImagePickerPlatform.instance, isA<ImagePickerMacOS>());
  });

  test('native camera is advertised without a delegate', () {
    expect(plugin.supportsImageSource(ImageSource.camera), isTrue);
  });

  group('images', () {
    test('pickImage passes the accepted type groups correctly', () async {
      await plugin.pickImage(source: ImageSource.gallery);

      final VerificationResult result = verify(
        mockFileSelectorPlatform.openFile(
          acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
        ),
      );
      expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
        'public.image',
      ]);
    });

    test('getImage passes the accepted type groups correctly', () async {
      await plugin.getImage(source: ImageSource.gallery);

      final VerificationResult result = verify(
        mockFileSelectorPlatform.openFile(
          acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
        ),
      );
      expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
        'public.image',
      ]);
    });

    test(
      'getImageFromSource passes the accepted type groups correctly',
      () async {
        await plugin.getImageFromSource(source: ImageSource.gallery);

        final VerificationResult result = verify(
          mockFileSelectorPlatform.openFile(
            acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
          ),
        );
        expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
          'public.image',
        ]);
      },
    );

    test('getImageFromSource calls delegate when source is camera', () async {
      const String fakePath = '/tmp/foo';
      plugin.cameraDelegate = FakeCameraDelegate(result: XFile(fakePath));
      expect(
        (await plugin.getImageFromSource(source: ImageSource.camera))!.path,
        fakePath,
      );
    });

    for (final path in <String?>['/tmp/photo.jpg', null]) {
      test('native camera returns $path', () async {
        const channel = MethodChannel('plugins.flutter.io/image_picker_macos');
        final messenger =
            TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
        messenger.setMockMethodCallHandler(channel, (call) async {
          expect(call.method, 'takePhoto');
          return path;
        });
        addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
        expect(
          (await plugin.getImageFromSource(source: ImageSource.camera))?.path,
          path,
        );
      });
    }

    test('camera permission errors reach the caller', () async {
      const channel = MethodChannel('plugins.flutter.io/image_picker_macos');
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(channel, (_) async {
        throw PlatformException(code: 'camera_access_denied');
      });
      addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
      await expectLater(
        plugin.getImageFromSource(source: ImageSource.camera),
        throwsA(
          isA<PlatformException>().having(
            (e) => e.code,
            'code',
            'camera_access_denied',
          ),
        ),
      );
    });

    test('lost data recovery is empty on macOS', () async {
      expect((await plugin.getLostData()).isEmpty, isTrue);
    });

    test('getMultiImage passes the accepted type groups correctly', () async {
      await plugin.getMultiImage();

      final VerificationResult result = verify(
        mockFileSelectorPlatform.openFiles(
          acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
        ),
      );
      expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
        'public.image',
      ]);
    });
  });

  group('videos', () {
    test('pickVideo passes the accepted type groups correctly', () async {
      await plugin.pickVideo(source: ImageSource.gallery);

      final VerificationResult result = verify(
        mockFileSelectorPlatform.openFile(
          acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
        ),
      );
      expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
        'public.movie',
      ]);
    });

    test('getVideo passes the accepted type groups correctly', () async {
      await plugin.getVideo(source: ImageSource.gallery);

      final VerificationResult result = verify(
        mockFileSelectorPlatform.openFile(
          acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
        ),
      );
      expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
        'public.movie',
      ]);
    });

    test('getVideo calls delegate when source is camera', () async {
      const String fakePath = '/tmp/foo';
      plugin.cameraDelegate = FakeCameraDelegate(result: XFile(fakePath));
      expect(
        (await plugin.getVideo(source: ImageSource.camera))!.path,
        fakePath,
      );
    });

    for (final path in <String?>['/tmp/movie.mov', null]) {
      test('native video capture returns $path and forwards options', () async {
        const channel = MethodChannel('plugins.flutter.io/image_picker_macos');
        final messenger =
            TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
        messenger.setMockMethodCallHandler(channel, (call) async {
          expect(call.method, 'recordVideo');
          expect(call.arguments, {
            'preferredCameraDevice': 'front',
            'maxDurationSeconds': 1.5,
          });
          return path;
        });
        addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
        expect(
          (await plugin.getVideo(
            source: ImageSource.camera,
            preferredCameraDevice: CameraDevice.front,
            maxDuration: const Duration(milliseconds: 1500),
          ))?.path,
          path,
        );
      });
    }
    test('video rejects invalid duration before capture', () async {
      await expectLater(
        plugin.getVideo(source: ImageSource.camera, maxDuration: Duration.zero),
        throwsArgumentError,
      );
    });

    test(
      'getMultiVideoWithOptions passes the accepted type groups correctly',
      () async {
        await plugin.getMultiVideoWithOptions();

        final VerificationResult result = verify(
          mockFileSelectorPlatform.openFiles(
            acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
          ),
        );
        expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
          'public.movie',
        ]);
      },
    );
  });

  group('image processing', () {
    final calls = <MethodCall>[];
    setUp(() {
      calls.clear();
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            const MethodChannel('plugins.flutter.io/image_picker_macos'),
            (call) async {
              calls.add(call);
              return call.method == 'takePhoto'
                  ? '/tmp/original.jpg'
                  : '/tmp/resized.jpg';
            },
          );
    });
    tearDown(() {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(
            const MethodChannel('plugins.flutter.io/image_picker_macos'),
            null,
          );
    });
    test('photo forwards bounds and JPEG quality to processing', () async {
      final file = await plugin.getImageFromSource(
        source: ImageSource.camera,
        options: const ImagePickerOptions(
          maxWidth: 320,
          maxHeight: 240,
          imageQuality: 65,
        ),
      );
      expect(file!.path, '/tmp/resized.jpg');
      expect(calls.map((c) => c.method), ['takePhoto', 'processImage']);
      expect(calls.last.arguments, {
        'path': '/tmp/original.jpg',
        'maxWidth': 320.0,
        'maxHeight': 240.0,
        'imageQuality': 65,
        'allowVideo': false,
      });
    });
    test(
      'gallery transforms selected image without overwriting source',
      () async {
        when(
          mockFileSelectorPlatform.openFile(
            acceptedTypeGroups: anyNamed('acceptedTypeGroups'),
          ),
        ).thenAnswer((_) async => XFile('/tmp/gallery.png'));
        final file = await plugin.getImageFromSource(
          source: ImageSource.gallery,
          options: const ImagePickerOptions(maxWidth: 40),
        );
        expect(file!.path, '/tmp/resized.jpg');
        expect(calls.single.arguments['path'], '/tmp/gallery.png');
      },
    );
    test('multi-image transforms every selection', () async {
      when(
        mockFileSelectorPlatform.openFiles(
          acceptedTypeGroups: anyNamed('acceptedTypeGroups'),
        ),
      ).thenAnswer((_) async => [XFile('/tmp/a.jpg'), XFile('/tmp/b.jpg')]);
      final files = await plugin.getMultiImageWithOptions(
        options: const MultiImagePickerOptions(
          imageOptions: ImageOptions(imageQuality: 20),
        ),
      );
      expect(files.length, 2);
      expect(calls.map((c) => c.arguments['path']), [
        '/tmp/a.jpg',
        '/tmp/b.jpg',
      ]);
    });
    test(
      'mixed media allows native processing to pass videos through',
      () async {
        when(
          mockFileSelectorPlatform.openFiles(
            acceptedTypeGroups: anyNamed('acceptedTypeGroups'),
          ),
        ).thenAnswer((_) async => [XFile('/tmp/a.mov')]);
        await plugin.getMedia(
          options: const MediaOptions(
            allowMultiple: true,
            imageOptions: ImageOptions(maxHeight: 100),
          ),
        );
        expect(calls.single.arguments['allowVideo'], isTrue);
      },
    );
    test('invalid image parameters fail before presenting camera', () async {
      for (final options in [
        const ImagePickerOptions(maxWidth: 0),
        const ImagePickerOptions(maxHeight: double.infinity),
        const ImagePickerOptions(imageQuality: 101),
      ]) {
        await expectLater(
          plugin.getImageFromSource(
            source: ImageSource.camera,
            options: options,
          ),
          throwsArgumentError,
        );
      }
      expect(calls, isEmpty);
    });
  });

  group('media', () {
    test('getMedia passes the accepted type groups correctly', () async {
      await plugin.getMedia(options: const MediaOptions(allowMultiple: true));

      final VerificationResult result = verify(
        mockFileSelectorPlatform.openFiles(
          acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups'),
        ),
      );
      expect(capturedTypeGroups(result)[0].uniformTypeIdentifiers, <String>[
        'public.image',
        'public.movie',
      ]);
    });

    test('multiple media handles an empty path response gracefully', () async {
      expect(
        await plugin.getMedia(options: const MediaOptions(allowMultiple: true)),
        <String>[],
      );
    });

    test('single media handles an empty path response gracefully', () async {
      expect(
        await plugin.getMedia(
          options: const MediaOptions(allowMultiple: false),
        ),
        <String>[],
      );
    });
  });
}

class FakeCameraDelegate extends ImagePickerCameraDelegate {
  FakeCameraDelegate({this.result});

  XFile? result;

  @override
  Future<XFile?> takePhoto({
    ImagePickerCameraDelegateOptions options =
        const ImagePickerCameraDelegateOptions(),
  }) async {
    return result;
  }

  @override
  Future<XFile?> takeVideo({
    ImagePickerCameraDelegateOptions options =
        const ImagePickerCameraDelegateOptions(),
  }) async {
    return result;
  }
}
