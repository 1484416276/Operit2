# iSH Runtime Tooling

This directory owns the iOS iSH runtime inputs and build steps.

- `fetch_sources.py` downloads verified ZIP archives for iSH and its required
  upstream dependencies into `downloads/`, then extracts them into `sources/`.
- `build_alpine_rootfs_linux.sh` produces the x86 Alpine rootfs on Linux with
  `bash`, `python3`, `py3-pip`, `nodejs`, `npm`, and `ca-certificates`.
- `build_ish_ios.sh` builds the iSH static targets used by the Flutter iOS
  Runner. It requires the rootfs staged by the Linux rootfs build.

`downloads/`, `sources/`, and `build/` are build-owned directories and are
ignored by Git. The Runner exposes one terminal type: `shell`, backed by the
iSH Alpine Linux environment with Python and Node.js installed.

iSH is GPLv3 with its `LICENSE.IOS` App Store exception. Distributing an iOS
build that includes this runtime requires providing the corresponding iSH
source and license text to users.

## Reusing local iOS build products

`build_ish_ios.sh` records completed builds in
`apps/flutter/app/apple/ish-build/<configuration>-<platform>/.operit-ish-build-complete`.
Before fetching or patching sources, it checks that the required headers, all
static libraries, requested architecture slices, and the managed mount symbol
are available. A warm cache avoids the nested iSH Xcode build during Flutter or
VS Code launches. Runner also declares the scripts, patches, rootfs, libraries,
and completion marker as build-phase inputs/outputs for incremental builds.

Cache entries must match the configuration, SDK version, and platform. Release
builds additionally require an exact source signature. Debug builds may reuse a
compatible cache after script or patch changes; runtime development should force
a rebuild rather than rely on this launch shortcut. To rebuild explicitly, run
from the repository root (with the rootfs already staged):

```bash
OPERIT_ISH_FORCE_REBUILD=1 bash tools/ios-runtime/ish/build_ish_ios.sh \
  Debug iphonesimulator iphonesimulator arm64
```

Use the appropriate configuration, SDK, platform, and architectures for the
intended target. Forcing a build invokes the native toolchain and is not the
normal cached launch path. The shell cache-decision tests use isolated fixtures
and do not download sources or invoke Xcode:

```bash
python3 -m unittest discover -s tools/tests -p 'test_ish_build_cache.py' -v
```
