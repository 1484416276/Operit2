// Copyright 2013 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'dart:async';
import 'dart:math' as math;

import 'package:camera_platform_interface/camera_platform_interface.dart'
    as camera;

import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:file_selector_windows/file_selector_windows.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:image/image.dart' as image;
import 'package:image_picker_platform_interface/image_picker_platform_interface.dart';

/// The Windows implementation of [ImagePickerPlatform].
///
/// This class implements the `package:image_picker` functionality for
/// Windows.
class ImagePickerWindows extends CameraDelegatingImagePickerPlatform {
  @override
  bool supportsImageSource(ImageSource source) =>
      source == ImageSource.camera || super.supportsImageSource(source);

  @override
  Future<LostDataResponse> getLostData() async => LostDataResponse.empty();

  /// Constructs a ImagePickerWindows.
  ImagePickerWindows();

  Future<XFile?> _capture(
    bool video,
    CameraDevice device,
    Duration? maxDuration,
  ) async {
    if (maxDuration != null && maxDuration <= Duration.zero) {
      throw ArgumentError.value(maxDuration, 'maxDuration', 'must be positive');
    }
    final platform = camera.CameraPlatform.instance;
    final cameras = await platform.availableCameras();
    if (cameras.isEmpty) {
      throw PlatformException(
        code: 'camera_unavailable',
        message: 'No Windows camera is available',
      );
    }
    final preferred = device == CameraDevice.front
        ? camera.CameraLensDirection.front
        : camera.CameraLensDirection.back;
    final selected =
        cameras.where((item) => item.lensDirection == preferred).firstOrNull ??
            cameras.first;
    var cameraId = -1;
    try {
      cameraId = await platform.createCameraWithSettings(
        selected,
        camera.MediaSettings(
          resolutionPreset: camera.ResolutionPreset.medium,
          fps: 30,
          videoBitrate: 4000000,
          audioBitrate: 128000,
          enableAudio: video,
        ),
      );
      final initialized = platform.onCameraInitialized(cameraId).first;
      await platform.initializeCamera(cameraId);
      await initialized;
      if (!video) return await platform.takePicture(cameraId);
      await platform.startVideoRecording(cameraId);
      await Future<void>.delayed(maxDuration ?? const Duration(seconds: 30));
      return await platform.stopVideoRecording(cameraId);
    } on camera.CameraException catch (error) {
      throw PlatformException(code: error.code, message: error.description);
    } finally {
      if (cameraId >= 0) await platform.dispose(cameraId);
    }
  }

  Future<XFile?> _processImage(XFile? file, ImageOptions options,
      {bool allowVideo = false}) async {
    _validateImageOptions(options);
    if (file == null) return null;
    if (allowVideo &&
        videoFormats.contains(file.name.split('.').last.toLowerCase())) {
      return file;
    }
    if (options.maxWidth == null &&
        options.maxHeight == null &&
        options.imageQuality == null) {
      return file;
    }
    final bytes = await file.readAsBytes();
    final decoded = image.decodeImage(bytes);
    if (decoded == null) {
      throw const FormatException('Unsupported image format');
    }
    final width = options.maxWidth;
    final height = options.maxHeight;
    final scale = [
      width == null ? 1.0 : width / decoded.width,
      height == null ? 1.0 : height / decoded.height,
      1.0,
    ].reduce((a, b) => a < b ? a : b);
    final output = scale < 1
        ? image.copyResize(
            decoded,
            width: math.max(1, (decoded.width * scale).floor()),
            height: math.max(1, (decoded.height * scale).floor()),
          )
        : decoded;
    if (scale == 1 && options.imageQuality == null) return file;
    final encoded = image.encodeJpg(
      output,
      quality: options.imageQuality ?? 100,
    );
    return XFile.fromData(
      Uint8List.fromList(encoded),
      name: 'camera.jpg',
      mimeType: 'image/jpeg',
    );
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

  Future<List<XFile>> _processImages(
    List<XFile> files,
    ImageOptions options, {
    bool allowVideo = false,
  }) async {
    _validateImageOptions(options);
    return (await Future.wait(
      files.map((file) => _processImage(file, options, allowVideo: allowVideo)),
    ))
        .whereType<XFile>()
        .toList();
  }

  /// List of image extensions used when picking images
  @visibleForTesting
  static const List<String> imageFormats = <String>[
    'jpg',
    'jpeg',
    'png',
    'bmp',
    'webp',
    'gif',
    'tif',
    'tiff',
    'apng',
  ];

  /// List of video extensions used when picking videos
  @visibleForTesting
  static const List<String> videoFormats = <String>[
    'mov',
    'wmv',
    'mkv',
    'mp4',
    'webm',
    'avi',
    'mpeg',
    'mpg',
  ];

  /// The file selector used to prompt the user to select images or videos.
  @visibleForTesting
  static FileSelectorPlatform fileSelector = FileSelectorWindows();

  /// Registers this class as the default instance of [ImagePickerPlatform].
  static void registerWith() {
    ImagePickerPlatform.instance = ImagePickerWindows();
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
    final XFile? file = await getImageFromSource(
      source: source,
      options: ImagePickerOptions(
        maxWidth: maxWidth,
        maxHeight: maxHeight,
        imageQuality: imageQuality,
        preferredCameraDevice: preferredCameraDevice,
      ),
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

  // Camera capture uses camera_windows and does not depend on a delegate.
  @override
  Future<XFile?> getImageFromSource({
    required ImageSource source,
    ImagePickerOptions options = const ImagePickerOptions(),
  }) async {
    _validateImageOptions(options);
    switch (source) {
      case ImageSource.camera:
        return _processImage(
            await _capture(false, options.preferredCameraDevice, null),
            options);
      case ImageSource.gallery:
        const XTypeGroup typeGroup = XTypeGroup(
          label: 'Images',
          extensions: imageFormats,
        );
        final XFile? file = await fileSelector.openFile(
          acceptedTypeGroups: <XTypeGroup>[typeGroup],
        );
        return _processImage(file, options);
    }
    // Ensure that there's a fallback in case a new source is added.
    // ignore: dead_code
    throw UnimplementedError('Unknown ImageSource: $source');
  }

  // Camera captures use the requested lens when it is available. A null
  // maxDuration records for 30 seconds; a supplied duration bounds recording.
  @override
  Future<XFile?> getVideo({
    required ImageSource source,
    CameraDevice preferredCameraDevice = CameraDevice.rear,
    Duration? maxDuration,
  }) async {
    switch (source) {
      case ImageSource.camera:
        return _capture(true, preferredCameraDevice, maxDuration);
      case ImageSource.gallery:
        const XTypeGroup typeGroup = XTypeGroup(
          label: 'Videos',
          extensions: videoFormats,
        );
        final XFile? file = await fileSelector.openFile(
          acceptedTypeGroups: <XTypeGroup>[typeGroup],
        );
        return file;
    }
    // Ensure that there's a fallback in case a new source is added.
    // ignore: dead_code
    throw UnimplementedError('Unknown ImageSource: $source');
  }

  // Selected image dimensions and JPEG quality are applied before return.
  @override
  Future<List<XFile>> getMultiImage({
    double? maxWidth,
    double? maxHeight,
    int? imageQuality,
  }) async {
    const XTypeGroup typeGroup = XTypeGroup(
      label: 'Images',
      extensions: imageFormats,
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
  Future<List<XFile>> getMultiVideoWithOptions({
    MultiVideoPickerOptions options = const MultiVideoPickerOptions(),
  }) async {
    const XTypeGroup typeGroup = XTypeGroup(
      label: 'Videos',
      extensions: videoFormats,
    );
    final List<XFile> files = await fileSelector.openFiles(
      acceptedTypeGroups: <XTypeGroup>[typeGroup],
    );
    return files;
  }

  // Applies image options to selected photos and passes selected videos through.
  @override
  Future<List<XFile>> getMedia({required MediaOptions options}) async {
    const XTypeGroup typeGroup = XTypeGroup(
      label: 'images and videos',
      extensions: <String>[...imageFormats, ...videoFormats],
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
}
