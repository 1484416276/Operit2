#!/bin/sh
set -eu

repo_root="$PROJECT_DIR/../../../.."
crate_dir="$repo_root/apps/flutter/native/operit-flutter-bridge"
out_dir="$PROJECT_DIR/Flutter/ephemeral/rust"
lib_name="liboperit_flutter_bridge.a"

# Xcode runs build phases with a minimal PATH that usually misses the rustup
# shims, so the toolchain has to be located explicitly. Putting the real
# toolchain binaries first also keeps Cargo from searching PATH for `rustc`.
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"

rustup_bin=""
for candidate in \
  "$cargo_home/bin/rustup" \
  "$HOME/.cargo/bin/rustup" \
  /opt/homebrew/bin/rustup \
  /usr/local/bin/rustup; do
  if [ -x "$candidate" ]; then
    rustup_bin="$candidate"
    break
  fi
done
if [ -z "$rustup_bin" ] && command -v rustup >/dev/null 2>&1; then
  rustup_bin="$(command -v rustup)"
fi

toolchain_bin=""
for candidate in "$rustup_home"/toolchains/*/bin; do
  if [ -x "$candidate/cargo" ] && [ -x "$candidate/rustc" ]; then
    toolchain_bin="$candidate"
    break
  fi
done

if [ -z "$toolchain_bin" ] && [ -z "$rustup_bin" ]; then
  echo "No Rust toolchain found: install rustup or set RUSTUP_HOME/CARGO_HOME." >&2
  exit 1
fi

if [ -n "$toolchain_bin" ]; then
  RUSTUP_TOOLCHAIN="$(basename "${toolchain_bin%/bin}")"
  RUSTC="$toolchain_bin/rustc"
  PATH="$toolchain_bin:$PATH"
fi
if [ -n "$rustup_bin" ]; then
  PATH="$(dirname "$rustup_bin"):$PATH"
fi

RUSTUP_HOME="$rustup_home"
export PATH RUSTUP_HOME
if [ -n "$toolchain_bin" ]; then
  export RUSTC RUSTUP_TOOLCHAIN
fi

# Core embeds plugin assets at compile time. Prepare both built-in and optional
# official packages before Cargo runs, including on the first clean build.
"$repo_root/.venv/bin/python" "$repo_root/plugins/tools/sync_plugin_packages.py" \
  --source runtime --no-hot-reload

for arch in $ARCHS; do
  case "$arch" in
    arm64)
      rust_target="aarch64-apple-darwin"
      ;;
    x86_64)
      rust_target="x86_64-apple-darwin"
      ;;
    *)
      echo "Unsupported macOS Rust bridge architecture: $arch" >&2
      exit 1
      ;;
  esac

  mkdir -p "$out_dir/$arch"

  # `rustup target add` needs the network and the rustup shim; skip it when the
  # standard library for the target is already part of the toolchain.
  if [ -n "$toolchain_bin" ] && [ -d "${toolchain_bin%/bin}/lib/rustlib/$rust_target/lib" ]; then
    echo "Rust target $rust_target is already installed"
  elif [ -n "$rustup_bin" ]; then
    "$rustup_bin" target add "$rust_target"
  else
    echo "Rust target $rust_target is missing and rustup is unavailable." >&2
    exit 1
  fi

  RUSTFLAGS="-Awarnings" cargo rustc \
    --manifest-path "$crate_dir/Cargo.toml" \
    --release \
    --target "$rust_target" \
    --crate-type staticlib

  cp "$crate_dir/target/$rust_target/release/$lib_name" "$out_dir/$arch/$lib_name"
done
