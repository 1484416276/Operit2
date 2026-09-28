// Copyright 2013 The Flutter Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:file_selector_macos/file_selector_macos.dart';
import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:image_picker_platform_interface/image_picker_platform_interface.dart';

/// The macOS implementation of [ImagePickerPlatform].
///
/// This class implements the `package:image_picker` functionality for
/// macOS.
class ImagePickerMacOS extends CameraDelegatingImagePickerPlatform {
  /// Constructs a platform implementation.
  ImagePickerMacOS();

  static const _cameraChannel = MethodChannel(
    'plugins.flutter.io/image_picker_macos',
  );

  @override
  Future<LostDataResponse> getLostData() async => LostDataResponse.empty();

  @override
  bool supportsImageSource(ImageSource source) =>
      source == ImageSource.camera || super.supportsImageSource(source);

  /// The file selector used to prompt the user to select images or videos.
  @visibleForTesting
  static FileSelectorPlatform fileSelector = FileSelectorMacOS();

  /// Registers this class as the default instance of [ImagePickerPlatform].
  static void registerWith() {
    ImagePickerPlatform.instance = ImagePickerMacOS();
  }

  // This is soft-deprecated in the platform interface, and is only implemented
  // for compatibility. Callers should be using getImageFromSource.
  @override
  Future<PickedFile?> pickImage({
    required ImageSource source,
    double? maxWidth,
    double? maxHeight,
    int? imageQuality,
    CameraDevice preferredCameraDevice = CameraDevice.rear,
  }) async {
    final XFile? file = await getImage(
      source: source,
      maxWidth: maxWidth,
      maxHeight: maxHeight,
      imageQuality: imageQuality,
      preferredCameraDevice: preferredCameraDevice,
    );
    if (file != null) {
      return PickedFile(file.path);
    }
    return null;
  }

  // This is soft-deprecated in the platform interface, and is only implemented
  // for compatibility. Callers should be using getVideo.
  @override
  Future<PickedFile?> pickVideo({
    required ImageSource source,
    CameraDevice preferredCameraDevice = CameraDevice.rear,
    Duration? maxDuration,
  }) async {
    final XFile? file = await getVideo(
      source: source,
      preferredCameraDevice: preferredCameraDevice,
      maxDuration: maxDuration,
    );
    if (file != null) {
      return PickedFile(file.path);
    }
    return null;
  }

  // This is soft-deprecated in the platform interface, and is only implemented
  // for compatibility. Callers should be using getImageFromSource.
  @override
  Future<XFile?> getImage({
    required ImageSource source,
    double? maxWidth,
    double? maxHeight,
    int? imageQuality,
    CameraDevice preferredCameraDevice = CameraDevice.rear,
  }) async {
    return getImageFromSource(
      source: source,
      options: ImagePickerOptions(
        maxWidth: maxWidth,
        maxHeight: maxHeight,
        imageQuality: imageQuality,
        preferredCameraDevice: preferredCameraDevice,
      ),
    );
  }

  // Camera capture uses the native panel unless a custom delegate is supplied.
  @override
  Future<XFile?> getImageFromSource({
    required ImageSource source,
    ImagePickerOptions options = const ImagePickerOptions(),
  }) async {
    _validateImageOptions(options);
    switch (source) {
      case ImageSource.camera:
        if (cameraDelegate != null) {
          return _processImage(
            await super.getImageFromSource(source: source, options: options),
            options,
          );
        }
        final path = await _cameraChannel.invokeMethod<String>('takePhoto');
        return _processImage(path == null ? null : XFile(path), options);
      case ImageSource.gallery:
        const XTypeGroup typeGroup = XTypeGroup(
          uniformTypeIdentifiers: <String>['public.image'],
        );
        final XFile? file = await fileSelector.openFile(
          acceptedTypeGroups: <XTypeGroup>[typeGroup],
        );
        return _processImage(file, options);
    }
  }

  @override
  Future<XFile?> getVideo({
    required ImageSource source,
    CameraDevice preferredCameraDevice = CameraDevice.rear,
    Duration? maxDuration,
  }) async {
    switch (source) {
      case ImageSource.camera:
        if (maxDuration != null && maxDuration <= Duration.zero) {
          throw ArgumentError.value(
            maxDuration,
            'maxDuration',
            'must be positive',
          );
        }
        if (cameraDelegate != null) {
          return super.getVideo(
            source: source,
            preferredCameraDevice: preferredCameraDevice,
            maxDuration: maxDuration,
          );
        }
        final path = await _cameraChannel.invokeMethod<String>('recordVideo', {
          'preferredCameraDevice': preferredCameraDevice.name,
          'maxDurationSeconds': maxDuration == null
              ? null
              : maxDuration.inMicroseconds / 1000000,
        });
        return path == null ? null : XFile(path);
      case ImageSource.gallery:
        const XTypeGroup typeGroup = XTypeGroup(
          uniformTypeIdentifiers: <String>['public.movie'],
        );
        final XFile? file = await fileSelector.openFile(
          acceptedTypeGroups: <XTypeGroup>[typeGroup],
        );
        return file;
    }
  }

  @override
  Future<List<XFile>> getMultiImage({
    double? maxWidth,
    double? maxHeight,
    int? imageQuality,
  }) async {
    _validateImageOptions(
      ImageOptions(
        maxWidth: maxWidth,
        maxHeight: maxHeight,
        imageQuality: imageQuality,
      ),
    );
    // Use the native file selector for images accessible on the filesystem.
    const XTypeGroup typeGroup = XTypeGroup(
      uniformTypeIdentifiers: <String>['public.image'],
    );
    final List<XFile> files = await fileSelector.openFiles(
      acceptedTypeGroups: <XTypeGroup>[typeGroup],
    );
    return _processImages(
      files,
      ImageOptions(
        maxWidth: maxWidth,
        maxHeight: maxHeight,
        imageQuality: imageQuality,
      ),
    );
  }

  @override
  Future<List<XFile>> getMultiImageWithOptions({
    MultiImagePickerOptions options = const MultiImagePickerOptions(),
  }) async {
    _validateImageOptions(options.imageOptions);
    final files = await fileSelector.openFiles(
      acceptedTypeGroups: const [
        XTypeGroup(uniformTypeIdentifiers: ['public.image']),
      ],
    );
    return _processImages(files, options.imageOptions);
  }

  @override
  Future<List<XFile>> getMultiVideoWithOptions({
    MultiVideoPickerOptions options = const MultiVideoPickerOptions(),
  }) async {
    // Use the native file selector for images accessible on the filesystem.
    const XTypeGroup typeGroup = XTypeGroup(
      uniformTypeIdentifiers: <String>['public.movie'],
    );
    final List<XFile> files = await fileSelector.openFiles(
      acceptedTypeGroups: <XTypeGroup>[typeGroup],
    );
    return files;
  }

  @override
  Future<List<XFile>> getMedia({required MediaOptions options}) async {
    _validateImageOptions(options.imageOptions);
    const XTypeGroup typeGroup = XTypeGroup(
      label: 'images and videos',
      uniformTypeIdentifiers: <String>['public.image', 'public.movie'],
    );

    List<XFile> files;

    if (options.allowMultiple) {
      files = await fileSelector.openFiles(
        acceptedTypeGroups: <XTypeGroup>[typeGroup],
      );
    } else {
      final XFile? file = await fileSelector.openFile(
        acceptedTypeGroups: <XTypeGroup>[typeGroup],
      );
      files = <XFile>[if (file != null) file];
    }
    return _processImages(files, options.imageOptions, allowVideo: true);
  }

  void _validateImageOptions(ImageOptions options) {
    for (final value in [options.maxWidth, options.maxHeight]) {
      if (value != null && (!value.isFinite || value <= 0)) {
        throw ArgumentError('Image dimensions must be finite and positive');
      }
    }
    final quality = options.imageQuality;
    if (quality != null && (quality < 0 || quality > 100)) {
      throw ArgumentError.value(
        quality,
        'imageQuality',
        'must be between 0 and 100',
      );
    }
  }

  Future<XFile?> _processImage(
    XFile? file,
    ImageOptions options, {
    bool allowVideo = false,
  }) async {
    _validateImageOptions(options);
    if (file == null ||
        (options.maxWidth == null &&
            options.maxHeight == null &&
            options.imageQuality == null))
      return file;
    final path = await _cameraChannel.invokeMethod<String>('processImage', {
      'path': file.path,
      'maxWidth': options.maxWidth,
      'maxHeight': options.maxHeight,
      'imageQuality': options.imageQuality,
      'allowVideo': allowVideo,
    });
    if (path == null) throw StateError('Image processing returned no file');
    return XFile(path);
  }

  Future<List<XFile>> _processImages(
    List<XFile> files,
    ImageOptions options, {
    bool allowVideo = false,
  }) async {
    _validateImageOptions(options);
    final result = <XFile>[];
    for (final file in files) {
      result.add((await _processImage(file, options, allowVideo: allowVideo))!);
    }
    return result;
  }
}
