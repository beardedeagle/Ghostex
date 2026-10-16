#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../release-gpui/common.sh
source "$SCRIPT_DIR/../release-gpui/common.sh"

VERSION="${1:-}"
release_gpui_require_version "$VERSION"
REPO_ROOT="$(release_gpui_repo_root)"
OUTPUT="${2:-$(release_gpui_default_output "$REPO_ROOT" "$VERSION" android)}"
BUILD_NUMBER="$(release_gpui_build_number "$VERSION")"
MOBILE_ROOT="$REPO_ROOT/apps/mobile/app"

release_gpui_require_command bun
release_gpui_require_command java
release_gpui_require_command keytool
: "${GHOSTEX_ANDROID_SIGNING_STORE_FILE:?Missing Android release keystore path}"
: "${GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD:?Missing Android release keystore password}"
: "${GHOSTEX_ANDROID_SIGNING_KEY_ALIAS:?Missing Android release key alias}"
: "${GHOSTEX_ANDROID_SIGNING_KEY_PASSWORD:?Missing Android release key password}"

export GHOSTEX_RELEASE_VERSION="$VERSION"
export GHOSTEX_RELEASE_BUILD_NUMBER="$BUILD_NUMBER"
export GHOSTEX_RELEASE_APPLICATION_ID="io.ghostex"
export GHOSTEX_RELEASE_SOURCE_KIND="react-native-mobile"
export ORG_GRADLE_PROJECT_ghostexReleaseStoreFile="$GHOSTEX_ANDROID_SIGNING_STORE_FILE"
export ORG_GRADLE_PROJECT_ghostexReleaseStorePassword="$GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD"
export ORG_GRADLE_PROJECT_ghostexReleaseKeyAlias="$GHOSTEX_ANDROID_SIGNING_KEY_ALIAS"
export ORG_GRADLE_PROJECT_ghostexReleaseKeyPassword="$GHOSTEX_ANDROID_SIGNING_KEY_PASSWORD"

# The React Native app's list of chat-capable agents is generated from
# packages/shared/session-chat-agents.ts. Regenerate it from the release checkout so a
# committed list from an earlier revision can never reach the APK.
cd "$REPO_ROOT"
node apps/mobile/app/scripts/generate-chat-agents.mjs

# The release workflow sets GHOSTEX_ANDROID_ABIS (space separated); every ABI
# listed must have its gomobile and chat-core libraries staged.
ANDROID_ABIS="${GHOSTEX_ANDROID_ABIS:-arm64-v8a}"
read -r -a ANDROID_ABI_LIST <<<"$ANDROID_ABIS"
[[ ${#ANDROID_ABI_LIST[@]} -gt 0 ]] || {
	echo "GHOSTEX_ANDROID_ABIS is empty" >&2
	exit 1
}
REACT_NATIVE_ARCHITECTURES="$(IFS=,; echo "${ANDROID_ABI_LIST[*]}")"

cd "$MOBILE_ROOT"
bunx expo prebuild --platform android --no-install
# CDXC:Release 2026-10-16 WHY: since the release APK compresses its native libraries, packaging
# deflates each large library in memory, and the template's 2 GB Gradle heap ran out in
# :app:packageRelease (10.24.0, run 38836965394). A command-line value outranks the generated
# gradle.properties.
./android/gradlew --no-daemon --stacktrace -p android \
	"-Dorg.gradle.jvmargs=-Xmx6g -XX:MaxMetaspaceSize=1g -Dfile.encoding=UTF-8" \
	"-PreactNativeArchitectures=$REACT_NATIVE_ARCHITECTURES" :app:assembleRelease

BUILD_TOOLS="${ANDROID_HOME:?ANDROID_HOME is required}/build-tools/36.0.0"
AAPT="$BUILD_TOOLS/aapt"
APKSIGNER="$BUILD_TOOLS/apksigner"
[[ -x "$AAPT" && -x "$APKSIGNER" ]] || {
	echo "Android build-tools 36.0.0 are incomplete" >&2
	exit 1
}

APK_SOURCE="$MOBILE_ROOT/android/app/build/outputs/apk/release/app-release.apk"
[[ -f "$APK_SOURCE" ]] || {
	echo "React Native release APK was not produced" >&2
	exit 1
}
BADGING="$($AAPT dump badging "$APK_SOURCE" | head -n 1)"
[[ "$BADGING" == *"name='io.ghostex'"* ]] || {
	echo "APK package is not io.ghostex: $BADGING" >&2
	exit 1
}
[[ "$BADGING" == *"versionCode='$BUILD_NUMBER'"* ]] || {
	echo "APK versionCode is not $BUILD_NUMBER: $BADGING" >&2
	exit 1
}
[[ "$BADGING" == *"versionName='$VERSION'"* ]] || {
	echo "APK versionName is not $VERSION: $BADGING" >&2
	exit 1
}

APK_ABIS="$(unzip -Z1 "$APK_SOURCE" 'lib/*' | cut -d/ -f2 | sort -u | tr '\n' ' ')"
EXPECTED_ABIS="$(printf '%s\n' "${ANDROID_ABI_LIST[@]}" | sort -u | tr '\n' ' ')"
[[ "$APK_ABIS" == "$EXPECTED_ABIS" ]] || {
	echo "APK ships ABIs '$APK_ABIS', expected '$EXPECTED_ABIS'" >&2
	exit 1
}

# R8 and native-library compression are on for release builds
# (apps/mobile/app/plugins/withAndroidReleaseSize.js). AGP already fails the
# build on classes R8 reports missing; this gate also fails it when shrinking
# did not run, when a library is stored uncompressed, or when a class that
# native code or reflection reaches by name was renamed or removed.
"$SCRIPT_DIR/android-release-check.sh" "$APK_SOURCE" "$MOBILE_ROOT/android/app/build/outputs/mapping/release" "$BUILD_TOOLS"

$APKSIGNER verify --verbose "$APK_SOURCE"
APK_CERT="$($APKSIGNER verify --print-certs "$APK_SOURCE" | sed -n 's/^Signer #1 certificate SHA-256 digest: //p' | tr -d ': ' | tr '[:upper:]' '[:lower:]')"
KEYSTORE_CERT="$(keytool -exportcert -keystore "$GHOSTEX_ANDROID_SIGNING_STORE_FILE" -storepass "$GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD" -alias "$GHOSTEX_ANDROID_SIGNING_KEY_ALIAS" | release_gpui_sha256 /dev/stdin)"
if [[ -z "$APK_CERT" || "$APK_CERT" != "$KEYSTORE_CERT" ]]; then
	echo "APK signing certificate does not match the established Ghostex keystore ($APK_CERT != $KEYSTORE_CERT)" >&2
	exit 1
fi

release_gpui_prepare_output "$REPO_ROOT" "$OUTPUT"
APK="$OUTPUT/ghostex-android.apk"
cp "$APK_SOURCE" "$APK"
R8_OUTPUT="$(dirname "$OUTPUT")/android-r8"
rm -rf "$R8_OUTPUT"
mkdir -p "$R8_OUTPUT"
cp "$MOBILE_ROOT/android/app/build/outputs/mapping/release/mapping.txt" "$R8_OUTPUT/"
release_gpui_write_manifest "$OUTPUT" android "$VERSION" "$APK"
echo "React Native Android APK ready: $APK"
