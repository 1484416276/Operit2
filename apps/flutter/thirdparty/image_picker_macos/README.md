## Operit local changes

Based on upstream image_picker_macos 0.2.2+1 (BSD license retained).
Camera photos use the native IKPictureTaker panel; video recording uses an
AVFoundation capture sheet. Both request camera permission and return local
files. Image resizing and JPEG quality are applied by the native ImageIO
processor. Custom camera delegates take priority. The host must declare
NSCameraUsageDescription and NSMicrophoneUsageDescription, and enable the
com.apple.security.device.camera and com.apple.security.device.microphone
entitlements. Interrupted-capture recovery returns an empty result on macOS.

# image\_picker\_macos

A macOS implementation of [`image_picker`][1].

## Camera and image options

`ImageSource.camera` captures a photo through the system camera panel. Video
capture presents a recording sheet with preview and Record, Finish, and Cancel
controls. `maxDuration` limits recording when supplied. `maxWidth`, `maxHeight`,
and `imageQuality` are supported for photos and selected images. Videos pass
through unchanged in mixed media selection.

## Usage

### Import the package

This package is [endorsed][2], which means you can simply use `image_picker`
normally. This package will be automatically included in your app when you do,
so you do not need to add it to your `pubspec.yaml`.

However, if you `import` this package to use any of its APIs directly, you
should add it to your `pubspec.yaml` as usual.

### Camera permissions and entitlements

The host app must provide camera and microphone usage descriptions and camera
and microphone entitlements:
```xml
    <key>NSCameraUsageDescription</key>
    <string>Capture photos and videos in chat.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>Record audio with videos captured in chat.</string>
    <key>com.apple.security.device.camera</key>
    <true/>
    <key>com.apple.security.device.microphone</key>
    <true/>
```

## Alternatives

If you would prefer an implementation that uses the dedicated photo picker UI
available in newer versions of macOS, which is more similar to the iOS
experience, you may want to consider using
[an alternate, unendorsed implementation built by the community][5].

[1]: https://pub.dev/packages/image_picker
[2]: https://flutter.dev/to/endorsed-federated-plugin
[3]: https://pub.dev/packages/file_selector
[4]: https://flutter.dev/to/macos-entitlements
[5]: https://pub.dev/packages?q=topic%3Aimage-picker+platform%3Amacos
