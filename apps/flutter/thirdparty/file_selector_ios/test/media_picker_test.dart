import 'package:file_selector_ios/file_selector_ios.dart';
import 'package:file_selector_ios/src/messages.g.dart';
import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:image_picker_platform_interface/image_picker_platform_interface.dart';

import 'file_selector_ios_test.dart' show FakeFileSelectorApi;

class Gallery extends ImagePickerPlatform {
  bool cancel = false;
  String? call;

  @override
  Future<XFile?> getImageFromSource({
    required ImageSource source,
    ImagePickerOptions options = const ImagePickerOptions(),
  }) async {
    expect(source, ImageSource.gallery);
    expect(options.requestFullMetadata, isFalse);
    call = 'image';
    return cancel ? null : XFile('photo.png');
  }

  @override
  Future<List<XFile>> getMultiImageWithOptions({
    MultiImagePickerOptions options = const MultiImagePickerOptions(),
  }) async {
    expect(options.imageOptions.requestFullMetadata, isFalse);
    call = 'images';
    return [XFile('a.png'), XFile('b.png')];
  }

  @override
  Future<XFile?> getVideo({
    required ImageSource source,
    CameraDevice preferredCameraDevice = CameraDevice.rear,
    Duration? maxDuration,
  }) async {
    expect(source, ImageSource.gallery);
    call = 'video';
    return XFile('video.mp4');
  }

  @override
  Future<List<XFile>> getMultiVideoWithOptions({
    MultiVideoPickerOptions options = const MultiVideoPickerOptions(),
  }) async {
    call = 'videos';
    return [XFile('a.mp4'), XFile('b.mp4')];
  }

  @override
  Future<List<XFile>> getMedia({required MediaOptions options}) async {
    expect(options.imageOptions.requestFullMetadata, isFalse);
    call = 'media:${options.allowMultiple}';
    return [XFile('photo.png')];
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Gallery gallery;
  late FakeFileSelectorApi api;
  late FileSelectorIOS plugin;
  setUp(() {
    gallery = Gallery();
    api = FakeFileSelectorApi();
    plugin = FileSelectorIOS(api: api, imagePicker: gallery);
  });
  test('image extensions route to Photos', () async {
    final file = await plugin.openFile(
      acceptedTypeGroups: const [
        XTypeGroup(extensions: ['jpg', 'jpeg', 'png', 'webp', 'bmp', 'gif']),
      ],
    );
    expect(file?.path, 'photo.png');
    expect(api.passedConfig, isNull);
  });
  test('cancelling Photos does not open Files', () async {
    gallery.cancel = true;
    expect(
      await plugin.openFile(
        acceptedTypeGroups: const [
          XTypeGroup(mimeTypes: ['image/*']),
        ],
      ),
      isNull,
    );
    expect(api.passedConfig, isNull);
  });
  test('video backgrounds route to Photos', () async {
    await plugin.openFile(
      acceptedTypeGroups: const [
        XTypeGroup(extensions: ['mp4', 'mov', 'm4v', 'webm', 'mkv', 'avi']),
      ],
    );
    expect(gallery.call, 'video');
    expect(api.passedConfig, isNull);
  });
  test('multiple images and videos retain multiple selection', () async {
    expect(
      (await plugin.openFiles(
        acceptedTypeGroups: const [
          XTypeGroup(uniformTypeIdentifiers: ['public.image']),
        ],
      )).length,
      2,
    );
    expect(gallery.call, 'images');
    expect(
      (await plugin.openFiles(
        acceptedTypeGroups: const [
          XTypeGroup(mimeTypes: ['video/*']),
        ],
      )).length,
      2,
    );
    expect(gallery.call, 'videos');
  });
  test('mixed media preserves selection mode', () async {
    const groups = [
      XTypeGroup(extensions: ['jpg', 'mp4']),
    ];
    await plugin.openFile(acceptedTypeGroups: groups);
    expect(gallery.call, 'media:false');
    await plugin.openFiles(acceptedTypeGroups: groups);
    expect(gallery.call, 'media:true');
  });
  test('unrestricted and mixed documents use Files', () async {
    await plugin.openFile();
    expect(api.passedConfig, isA<FileSelectorConfig>());
    await plugin.openFile(
      acceptedTypeGroups: const [
        XTypeGroup(extensions: ['png', 'json']),
      ],
    );
    expect(api.passedConfig?.utis, ['public.png', 'public.json']);
    expect(gallery.call, isNull);
  });
}
