# Operit2 Flutter app

All platforms use [AAswordman/flutter-ohos](https://github.com/AAswordman/flutter-ohos)
release **3.41.10-ohos-0.0.2-beta.operit.1**, pinned in `.fvmrc`. The release source and its Windows
engine archives share this version. FVM installs the fork at `.fvm/flutter_sdk`.

Run Flutter and Dart commands through FVM from `apps/flutter/app`:

```powershell
fvm install --skip-pub-get
fvm flutter precache --web --ohos
fvm flutter pub get
fvm flutter analyze
fvm flutter run
```

Configure the IDE Flutter SDK path to `.fvm/flutter_sdk`.
The build scripts use this same SDK for every platform, including OpenHarmony.

## Web debugging

`web_dev_config.yaml` configures Flutter's native Web development server with
cross-origin isolation headers for local threaded STT/TTS. Run
`fvm flutter run -d edge` or `fvm flutter run -d chrome` directly, or select
`Operit2: Web (isolated)` in VS Code. The development origin is
`http://127.0.0.1:4835`; no separate proxy process is needed.
See `web/README.md` for deployment requirements.
