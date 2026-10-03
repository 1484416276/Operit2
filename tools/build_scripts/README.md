# Operit2 build scripts

Local builds and GitHub Actions use the same Python entry points. Scripts are
named by their product, platform, or operation; there are no separate
`_action.py` / `_local.py` platform wrappers.

Build outputs and transient logs belong under `tools/release/work` and
`tools/release/dist`. Private credentials belong under `tools/release/secrets`.
These ignored directories are not used to store reusable build scripts.

## Entry points

- `build_flutter_<platform>.py`: Flutter builds for Android, iOS, Linux, macOS,
  OpenHarmony, and Windows.
- `build_cli_<platform>.py`: CLI builds for Linux, macOS, and Windows.
- `build_cli_current.py`: CLI build for the current host platform.
- `build_local.py`: existing current-host App/CLI/ESP32 dispatcher.
- `build_apple_release.py`: Apple release asset dispatcher.
- `build_esp32.py`: ESP32 firmware builder.
- `check_build_environment.py`: shared local/CI environment checker.
- `common.py`, `cli_common.py`, and `appstore_connect_common.py`: shared helpers.

## Apple release helpers

```bash
# Build Flutter artifacts
python3 tools/build_scripts/build_flutter_ios.py --build-name 2.0.0 --build-number 15
python3 tools/build_scripts/build_flutter_macos.py --build-name 2.0.0 --build-number 15

# Sign the macOS app and create a .pkg
python3 tools/build_scripts/sign_macos.py

# Validate or upload an .ipa/.pkg
python3 tools/build_scripts/validate_appstore.py path/to/App.ipa
python3 tools/build_scripts/upload_appstore.py path/to/App.ipa
```

The App Store Connect scripts read the ignored file
`tools/release/secrets/appstoreconnect/appstoreconnect.env`. No credentials are
stored in tracked build scripts.
