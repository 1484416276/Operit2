// Copyright 2013 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:camera_platform_interface/camera_platform_interface.dart'
    as camera;
import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:image_picker_platform_interface/image_picker_platform_interface.dart';
import 'package:image_picker_windows/image_picker_windows.dart';
import 'package:mockito/annotations.dart';
import 'package:mockito/mockito.dart';

import 'image_picker_windows_test.mocks.dart';

@GenerateMocks(<Type>[FileSelectorPlatform])
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // Returns the captured type groups from a mock call result, assuming that
  // exactly one call was made and only the type groups were captured.
  List<XTypeGroup> capturedTypeGroups(VerificationResult result) {
    return result.captured.single as List<XTypeGroup>;
  }

  group('ImagePickerWindows', () {
    late ImagePickerWindows plugin;
    late MockFileSelectorPlatform mockFileSelectorPlatform;
    late camera.CameraPlatform previousCameraPlatform;
    late FakeCameraPlatform fakeCameraPlatform;

    setUp(() {
      plugin = ImagePickerWindows();
      mockFileSelectorPlatform = MockFileSelectorPlatform();
      previousCameraPlatform = camera.CameraPlatform.instance;
      fakeCameraPlatform = FakeCameraPlatform();
      camera.CameraPlatform.instance = fakeCameraPlatform;

      when(mockFileSelectorPlatform.openFile(
              acceptedTypeGroups: anyNamed('acceptedTypeGroups')))
          .thenAnswer((_) async => null);

      when(mockFileSelectorPlatform.openFiles(
              acceptedTypeGroups: anyNamed('acceptedTypeGroups')))
          .thenAnswer((_) async => List<XFile>.empty());

      ImagePickerWindows.fileSelector = mockFileSelectorPlatform;
    });

    tearDown(() {
      camera.CameraPlatform.instance = previousCameraPlatform;
    });

    test('registered instance', () {
      ImagePickerWindows.registerWith();
      expect(ImagePickerPlatform.instance, isA<ImagePickerWindows>());
    });

    group('images', () {
      test('pickImage passes the accepted type groups correctly', () async {
        await plugin.pickImage(source: ImageSource.gallery);

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFile(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions,
            ImagePickerWindows.imageFormats);
      });

      test('getImage passes the accepted type groups correctly', () async {
        await plugin.getImage(source: ImageSource.gallery);

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFile(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions,
            ImagePickerWindows.imageFormats);
      });

      test('getMultiImage passes the accepted type groups correctly', () async {
        await plugin.getMultiImage();

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFiles(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions,
            ImagePickerWindows.imageFormats);
      });

      test('camera source is supported and captures with camera_windows',
          () async {
        expect(plugin.supportsImageSource(ImageSource.camera), isTrue);
        final file = await plugin.getImageFromSource(
          source: ImageSource.camera,
          options: const ImagePickerOptions(
            preferredCameraDevice: CameraDevice.front,
          ),
        );

        expect(file!.path, '/tmp/camera.jpg');
        expect(fakeCameraPlatform.selectedCamera?.lensDirection,
            camera.CameraLensDirection.front);
        expect(fakeCameraPlatform.initializedCameraIds, [1]);
        expect(fakeCameraPlatform.takenPictureCameraIds, [1]);
        expect(fakeCameraPlatform.disposedCameraIds, [1]);
      });

      test('getMultiImage passes the accepted type groups correctly', () async {
        await plugin.getMultiImage();

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFiles(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions,
            ImagePickerWindows.imageFormats);
      });
    });

    group('videos', () {
      test('pickVideo passes the accepted type groups correctly', () async {
        await plugin.pickVideo(source: ImageSource.gallery);

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFile(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions,
            ImagePickerWindows.videoFormats);
      });

      test('getVideo passes the accepted type groups correctly', () async {
        await plugin.getVideo(source: ImageSource.gallery);

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFile(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions,
            ImagePickerWindows.videoFormats);
      });

      test('camera source records video through camera_windows', () async {
        final file = await plugin.getVideo(
          source: ImageSource.camera,
          preferredCameraDevice: CameraDevice.front,
          maxDuration: const Duration(milliseconds: 1),
        );

        expect(file!.path, '/tmp/camera.mp4');
        expect(fakeCameraPlatform.selectedCamera?.lensDirection,
            camera.CameraLensDirection.front);
        expect(fakeCameraPlatform.startedRecordingCameraIds, [1]);
        expect(fakeCameraPlatform.stoppedRecordingCameraIds, [1]);
        expect(fakeCameraPlatform.disposedCameraIds, [1]);
      });

      test('getMultiVideoWithOptions passes the accepted type groups correctly',
          () async {
        await plugin.getMultiVideoWithOptions();

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFiles(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions,
            ImagePickerWindows.videoFormats);
      });
    });

    group('media', () {
      test('getMedia passes the accepted type groups correctly', () async {
        await plugin.getMedia(options: const MediaOptions(allowMultiple: true));

        final VerificationResult result = verify(
            mockFileSelectorPlatform.openFiles(
                acceptedTypeGroups: captureAnyNamed('acceptedTypeGroups')));
        expect(capturedTypeGroups(result)[0].extensions, <String>[
          ...ImagePickerWindows.imageFormats,
          ...ImagePickerWindows.videoFormats
        ]);
      });

      test('multiple media handles an empty path response gracefully',
          () async {
        expect(
            await plugin.getMedia(
              options: const MediaOptions(
                allowMultiple: true,
              ),
            ),
            <String>[]);
      });

      test('single media handles an empty path response gracefully', () async {
        expect(
            await plugin.getMedia(
              options: const MediaOptions(
                allowMultiple: false,
              ),
            ),
            <String>[]);
      });
    });
  });
}

class FakeCameraPlatform extends camera.CameraPlatform {
  final cameras = const [
    camera.CameraDescription(
      name: 'back',
      lensDirection: camera.CameraLensDirection.back,
      sensorOrientation: 0,
    ),
    camera.CameraDescription(
      name: 'front',
      lensDirection: camera.CameraLensDirection.front,
      sensorOrientation: 0,
    ),
  ];
  camera.CameraDescription? selectedCamera;
  final initializedCameraIds = <int>[];
  final takenPictureCameraIds = <int>[];
  final startedRecordingCameraIds = <int>[];
  final stoppedRecordingCameraIds = <int>[];
  final disposedCameraIds = <int>[];

  @override
  Future<List<camera.CameraDescription>> availableCameras() async => cameras;

  @override
  Future<int> createCamera(
    camera.CameraDescription cameraDescription,
    camera.ResolutionPreset? resolutionPreset, {
    bool enableAudio = false,
  }) async {
    selectedCamera = cameraDescription;
    return 1;
  }

  @override
  Stream<camera.CameraInitializedEvent> onCameraInitialized(int cameraId) =>
      Stream.value(
        camera.CameraInitializedEvent(
          cameraId,
          640,
          480,
          camera.ExposureMode.auto,
          false,
          camera.FocusMode.auto,
          false,
        ),
      );

  @override
  Future<void> initializeCamera(int cameraId,
      {camera.ImageFormatGroup imageFormatGroup =
          camera.ImageFormatGroup.unknown}) async {
    initializedCameraIds.add(cameraId);
  }

  @override
  Future<XFile> takePicture(int cameraId) async {
    takenPictureCameraIds.add(cameraId);
    return XFile('/tmp/camera.jpg');
  }

  @override
  Future<void> startVideoRecording(int cameraId,
      {Duration? maxVideoDuration}) async {
    startedRecordingCameraIds.add(cameraId);
  }

  @override
  Future<XFile> stopVideoRecording(int cameraId) async {
    stoppedRecordingCameraIds.add(cameraId);
    return XFile('/tmp/camera.mp4');
  }

  @override
  Future<void> dispose(int cameraId) async {
    disposedCameraIds.add(cameraId);
  }
}
