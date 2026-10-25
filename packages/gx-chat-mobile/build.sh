#!/bin/sh
# Builds the Rust chat core for the mobile app and stages it into the Expo module
# apps/mobile/app/modules/gx-chat-core, together with the UniFFI Swift and Kotlin bindings.
#
#   packages/gx-chat-mobile/build.sh [--target ios|android|all] [--debug] [--gpui]   (default: all, release)
#
# --gpui (or GX_MOBILE_GPUI=1): the library also carries the GPUI chat transcript
# (packages/gpui-mobile) and is built as libgx_mobile from crate ghostex-gx-mobile in that
# workspace, with the same UniFFI surface; the bindings are generated from it, so they load
# gx_mobile. Android links it at API 26 (libnativewindow), so the app needs minSdk 26 then.
#
# Outputs (all gitignored, reproducible from this crate):
#   ios/Vendor/GxChatMobile.xcframework        aarch64-apple-ios + aarch64-apple-ios-sim static libs
#   ios/Generated/gx_chat_mobile.swift         UniFFI Swift bindings
#   android/src/main/jniLibs/<abi>/libgx_chat_mobile.so   (GX_CHAT_ANDROID_ABIS, default arm64-v8a x86_64;
#                                                          releases build all four APK ABIs)
#   android/src/main/java/dev/ghostex/gxchatcore/uniffi/gx_chat_mobile.kt   UniFFI Kotlin bindings
#   android/src/main/assets/fonts/NotoColorEmoji.ttf   (--gpui only) colour emoji for the transcript
#
# Needs: rustup (the crate pins 1.99.1 and its targets in rust-toolchain.toml), Xcode for iOS,
# cargo-ndk (`cargo install cargo-ndk`) and an Android NDK for Android. Run it once before
# `bunx expo run:ios` / `run:android`, and again after gx-chat-core changes.
set -eu
cd "$(dirname "$0")"
CRATE_DIR="$(pwd)"

TARGET=all
PROFILE=release
GPUI="${GX_MOBILE_GPUI:-0}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --target=*) TARGET="${1#--target=}"; shift ;;
    --debug) PROFILE=debug; shift ;;
    --gpui) GPUI=1; shift ;;
    -h | --help) sed -n '2,21p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$TARGET" in
  ios | android | all) ;;
  *) echo "unknown --target $TARGET (expected ios, android or all)" >&2; exit 2 ;;
esac

CARGO_PROFILE_FLAG="--release"
[ "$PROFILE" = debug ] && CARGO_PROFILE_FLAG=""

MODULE_DIR="$CRATE_DIR/../../apps/mobile/app/modules/gx-chat-core"
[ -d "$MODULE_DIR" ] || { echo "Expo module not found at $MODULE_DIR (is the apps/mobile/app submodule checked out?)" >&2; exit 1; }

# The crate's rust-toolchain.toml selects the compiler; make sure its mobile targets exist.
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android x86_64-linux-android >/dev/null

# What gets built: this crate alone (libgx_chat_mobile), or with --gpui the phone library that also
# carries the GPUI transcript (libgx_mobile, packages/gpui-mobile/phone), in its own target dir.
if [ "$GPUI" = 1 ]; then
  BUILD_WS="$(cd "$CRATE_DIR/../gpui-mobile" && pwd)"
  LIB_TARGET_DIR="$BUILD_WS/target/phone"
  LIB_NAME=gx_mobile
  PACKAGE_ARG="-p ghostex-gx-mobile"
  ANDROID_PLATFORM=26
else
  BUILD_WS="$CRATE_DIR"
  LIB_TARGET_DIR="$CRATE_DIR/target"
  LIB_NAME=gx_chat_mobile
  PACKAGE_ARG=""
  ANDROID_PLATFORM=24
fi
# Every build of the library goes through this, in its workspace and target dir.
lib_cargo() {
  (cd "$BUILD_WS" && CARGO_TARGET_DIR="$LIB_TARGET_DIR" cargo "$@")
}

# Bindings come from the host build of the same library (UniFFI library mode reads the metadata
# compiled into it), so they always match the scaffolding in the mobile libraries.
echo "== host library ($LIB_NAME) + bindgen"
# shellcheck disable=SC2086
lib_cargo build $CARGO_PROFILE_FLAG --lib $PACKAGE_ARG
cargo build --release --features bindgen --bin uniffi-bindgen
BINDGEN="$CRATE_DIR/target/release/uniffi-bindgen"
HOST_LIB="$LIB_TARGET_DIR/$PROFILE/lib$LIB_NAME.dylib"
[ -f "$HOST_LIB" ] || HOST_LIB="$LIB_TARGET_DIR/$PROFILE/lib$LIB_NAME.so"
BUILD_DIR="$CRATE_DIR/build"
rm -rf "$BUILD_DIR/swift" "$BUILD_DIR/kotlin"
mkdir -p "$BUILD_DIR"

if [ "$TARGET" = ios ] || [ "$TARGET" = all ]; then
  echo "== swift bindings"
  "$BINDGEN" generate --library "$HOST_LIB" --language swift --out-dir "$BUILD_DIR/swift"
  # Same floor as the ghostex-native pod, so the linker never sees objects newer than the app.
  export IPHONEOS_DEPLOYMENT_TARGET=16.4
  for triple in aarch64-apple-ios aarch64-apple-ios-sim; do
    echo "== $triple"
    if [ "$GPUI" = 1 ]; then
      # Only the static library: the phone crate is also a cdylib, which iOS does not need.
      # shellcheck disable=SC2086
      lib_cargo rustc $CARGO_PROFILE_FLAG --lib $PACKAGE_ARG --crate-type staticlib --target "$triple"
    else
      lib_cargo build $CARGO_PROFILE_FLAG --lib --target "$triple"
    fi
  done
  HEADERS="$BUILD_DIR/swift/headers"
  mkdir -p "$HEADERS"
  cp "$BUILD_DIR/swift/gx_chat_mobileFFI.h" "$HEADERS/"
  cp "$BUILD_DIR/swift/gx_chat_mobileFFI.modulemap" "$HEADERS/module.modulemap"
  if [ "$GPUI" = 1 ]; then
    # The GPUI view's C ABI, as its own Swift module next to the UniFFI one.
    GPUI_HEADERS="$BUILD_WS/host/src/ios/include"
    cp "$GPUI_HEADERS/ghostex_gpui.h" "$HEADERS/"
    { echo; cat "$GPUI_HEADERS/module.modulemap"; } >>"$HEADERS/module.modulemap"
  fi
  XCFRAMEWORK="$MODULE_DIR/ios/Vendor/GxChatMobile.xcframework"
  rm -rf "$XCFRAMEWORK"
  mkdir -p "$MODULE_DIR/ios/Vendor" "$MODULE_DIR/ios/Generated"
  xcodebuild -create-xcframework \
    -library "$LIB_TARGET_DIR/aarch64-apple-ios/$PROFILE/lib$LIB_NAME.a" -headers "$HEADERS" \
    -library "$LIB_TARGET_DIR/aarch64-apple-ios-sim/$PROFILE/lib$LIB_NAME.a" -headers "$HEADERS" \
    -output "$XCFRAMEWORK" >/dev/null
  cp "$BUILD_DIR/swift/gx_chat_mobile.swift" "$MODULE_DIR/ios/Generated/gx_chat_mobile.swift"
  echo "   staged $(du -sh "$XCFRAMEWORK" | cut -f1) $XCFRAMEWORK"
fi

if [ "$TARGET" = android ] || [ "$TARGET" = all ]; then
  echo "== kotlin bindings"
  KOTLIN_CONFIG=""
  if [ "$GPUI" = 1 ]; then
    # uniffi.toml pins the library the bindings load to gx_chat_mobile; this build ships gx_mobile.
    KOTLIN_CONFIG="$BUILD_DIR/kotlin-gx-mobile.toml"
    printf '[crates.gx_chat_mobile.bindings.kotlin]\ncdylib_name = "%s"\n' "$LIB_NAME" >"$KOTLIN_CONFIG"
  fi
  "$BINDGEN" generate --library "$HOST_LIB" --language kotlin --out-dir "$BUILD_DIR/kotlin" --no-format \
    ${KOTLIN_CONFIG:+-c "$KOTLIN_CONFIG"}
  : "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
  export ANDROID_HOME
  if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$(ls "$ANDROID_HOME/ndk" | sort -V | tail -1)"
  fi
  export ANDROID_NDK_HOME
  ABIS="${GX_CHAT_ANDROID_ABIS:-arm64-v8a x86_64}"
  NDK_TARGETS=""
  for abi in $ABIS; do
    NDK_TARGETS="$NDK_TARGETS -t $abi"
    case "$abi" in
      arm64-v8a) rustup target add aarch64-linux-android >/dev/null ;;
      armeabi-v7a) rustup target add armv7-linux-androideabi >/dev/null ;;
      x86) rustup target add i686-linux-android >/dev/null ;;
      x86_64) rustup target add x86_64-linux-android >/dev/null ;;
      *) echo "unknown Android ABI: $abi" >&2; exit 2 ;;
    esac
  done
  echo "== android ($ABIS, NDK $ANDROID_NDK_HOME)"
  JNI_LIBS="$MODULE_DIR/android/src/main/jniLibs"
  rm -rf "$JNI_LIBS"
  # shellcheck disable=SC2086
  lib_cargo ndk $NDK_TARGETS --platform "$ANDROID_PLATFORM" -o "$JNI_LIBS" build $CARGO_PROFILE_FLAG --lib $PACKAGE_ARG
  # cargo-ndk copies every cdylib the build produced; the app loads only this one.
  find "$JNI_LIBS" -name '*.so' ! -name "lib$LIB_NAME.so" -delete
  ASSET_FONTS="$MODULE_DIR/android/src/main/assets/fonts"
  if [ "$GPUI" = 1 ]; then
    # Colour emoji for the GPUI transcript: Android 13+ ships a COLR v1 emoji font, which GPUI's
    # text stack cannot draw, so the APK carries the CBDT Noto Color Emoji Ghostty vendors.
    mkdir -p "$ASSET_FONTS"
    cp "$CRATE_DIR/../../.dependencies/ghostty/src/font/res/NotoColorEmoji.ttf" "$ASSET_FONTS/"
  else
    rm -rf "$ASSET_FONTS"
  fi
  KOTLIN_OUT="$MODULE_DIR/android/src/main/java/dev/ghostex/gxchatcore/uniffi"
  mkdir -p "$KOTLIN_OUT"
  cp "$BUILD_DIR/kotlin/dev/ghostex/gxchatcore/uniffi/gx_chat_mobile.kt" "$KOTLIN_OUT/gx_chat_mobile.kt"
  echo "   staged $(du -sh "$JNI_LIBS" | cut -f1) $JNI_LIBS"
fi
echo "== done"
