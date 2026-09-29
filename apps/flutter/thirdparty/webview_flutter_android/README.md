# webview\_flutter\_android

The Android implementation of [`webview_flutter`][1].

## Usage

This package is [endorsed][2], which means you can simply use `webview_flutter`
normally. This package will be automatically included in your app when you do,
so you do not need to add it to your `pubspec.yaml`.

However, if you `import` this package to use any of its APIs directly, you
should add it to your `pubspec.yaml` as usual.

## Display Mode

This Operit fork defaults to **Hybrid Composition** for `WebViewWidget`,
including widgets created through generic platform parameters. Web fullscreen
custom views (including fullscreen video) also use Hybrid Composition.

The Android WebView is embedded as a native view. Flutter's Android platform-view
compositor handles Flutter layers above it, including modal barriers and dialogs.
The app does not need platform checks or Android-specific widget parameters.

Hybrid Composition has its own rendering and platform-thread costs. Validate
scrolling, video playback, translucent dialogs, keyboard focus, and device scaling
on Android hardware; choosing this mode is not a performance benchmark.

The existing `AndroidWebViewWidgetCreationParams.displayWithHybridComposition`
option remains available. An explicit `false` requests Texture Layer Hybrid
Composition; the application's standard creation path defaults to `true`.
Legacy APIs under `src/legacy` retain their existing behavior and are not used by
the application's `WebViewWidget` path.

## External Native API

The plugin also provides a native API accessible by the native code of Android applications or
packages. This API follows the convention of breaking changes of the Dart API, which means that any
changes to the class that are not backwards compatible will only be made with a major version change
of the plugin. Native code other than this external API does not follow breaking change conventions,
so app or plugin clients should not use any other native APIs.

The API can be accessed by importing the native class `WebViewFlutterAndroidExternalApi`:

Java:

```java
import io.flutter.plugins.webviewflutter.WebViewFlutterAndroidExternalApi;
```

## Enable Payment Request in WebView

The Payment Request API can be enabled by calling `AndroidWebViewController.setPaymentRequestEnabled` after
checking `AndroidWebViewController.isWebViewFeatureSupported`.

<?code-excerpt "example/lib/readme_excerpts.dart (payment_request_example)"?>
```dart
final bool paymentRequestEnabled = await androidController
    .isWebViewFeatureSupported(WebViewFeatureType.paymentRequest);

if (paymentRequestEnabled) {
  await androidController.setPaymentRequestEnabled(true);
}
```

Add intent filters to your AndroidManifest.xml to discover and invoke Android payment apps using system intents:

```xml
<queries>
  <intent>
    <action android:name="org.chromium.intent.action.PAY"/>
  </intent>
  <intent>
    <action android:name="org.chromium.intent.action.IS_READY_TO_PAY"/>
  </intent>
  <intent>
    <action android:name="org.chromium.intent.action.UPDATE_PAYMENT_DETAILS"/>
  </intent>
</queries>
```

## Fullscreen Video

To display a video as fullscreen, an app must manually handle the notification that the current page
has entered fullscreen mode. This can be done by calling
`AndroidWebViewController.setCustomWidgetCallbacks`. Below is an example implementation.

<?code-excerpt "example/lib/main.dart (fullscreen_example)"?>
```dart
androidController.setCustomWidgetCallbacks(
  onShowCustomWidget: (Widget widget, OnHideCustomWidgetCallback callback) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (BuildContext context) => widget,
        fullscreenDialog: true,
      ),
    );
  },
  onHideCustomWidget: () {
    Navigator.of(context).pop();
  },
);
```

## Geolocation

By default, WebView does not allow geolocation requests. To allow them, call
`setGeolocationPermissionsPromptCallbacks` on the `AndroidWebViewController` to
configure a prompt handler. For example, to unconditionally allow all requests:

<?code-excerpt "example/lib/readme_excerpts.dart (geolocation_example)"?>
```dart
await androidController.setGeolocationPermissionsPromptCallbacks(
  onShowPrompt: (GeolocationPermissionsRequestParams request) async {
    return const GeolocationPermissionsResponse(allow: true, retain: true);
  },
);
```

**Important:** Geolocation requests should only be allowed unconditionally if
the web view content is restricted to domains you control or trust. If you are
showing untrusted content, the `onShowPrompt` implementation should request
permission from the user before responding.

Your application must have geolocation permissions granted in order for the
WebView to have access to geolocation.

## Contributing

For information on contributing to this plugin, see [`CONTRIBUTING.md`](CONTRIBUTING.md).

[1]: https://pub.dev/packages/webview_flutter
[2]: https://flutter.dev/to/endorsed-federated-plugin
