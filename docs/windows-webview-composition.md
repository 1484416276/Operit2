# Windows native WebView composition

The application uses Flutter tag `3.41.10-ohos-0.0.2-beta.operit.1` from
`https://github.com/AAswordman/flutter-ohos.git` through `apps/flutter/app/.fvmrc`.
The corrected engine source is `f9875fb2cad3671eb83ccf23a0e8673e6ec4b36f`.
The existing tag and release assets were replaced at the owner's request.

## Rendering

WebView2 supplies a composition visual directly to the Windows engine.
Flutter's platform-view layers determine placement, transforms, clipping and
paint order. Flutter backing stores use premultiplied-alpha DXGI swapchains.
The browser display does not use a Flutter Texture or continuous screen capture.

ANGLE already provides the expected framebuffer orientation: the blit preserves
both axes. DirectComposition layers are appended above earlier siblings with
`AddVisual(visual, FALSE, nullptr)`, matching Flutter's back-to-front paint order.

## Validation

- 20 relevant engine tests pass, including real desktop pixel assertions for
  ordering, alpha blending and removing overlay/native layers.
- A real WebView2 fixture displays upright green HTML beneath a Flutter toolbar,
  with rounded clipping and correct resizing.
- A standard Flutter dialog and its translucent modal barrier cover the browser.
- Clicking the barrier does not increment the HTML button counter; clicking the
  same position after dismissal increments it.
- These checks are not a browser performance benchmark or a full IME matrix.

The reproducible fixture is `apps/flutter/app/tool/windows_native_composition_probe.dart`.
Its `ext.operit.probe` debug service exposes dialog and pointer actions and browser
queries for acceptance testing. It is not part of the application's main entrypoint.

## Install and refresh

Run these commands from `apps/flutter/app`:

```powershell
fvm install --skip-pub-get
fvm flutter precache --windows
fvm flutter pub get
fvm flutter run -d windows
```

Devices that cached the original `.operit.1` need the updated source tag and
`fvm flutter precache --windows --force`. Each downloaded engine directory contains
`engine-release.json`; its framework revision must match the corrected commit above.
A fresh install gets the corrected release assets. Restart the editor after an SDK
refresh so its Flutter daemon stops using the old SDK process.
