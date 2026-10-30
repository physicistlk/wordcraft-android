#!/usr/bin/env bash
# Build the release arm64-v8a APK on Linux.
#
#   ANDROID_HOME=... JAVA_HOME=... scripts/build-apk-linux.sh
#
# Signing: cargo-apk signs with the keystore named by CARGO_APK_RELEASE_KEYSTORE and
# CARGO_APK_RELEASE_KEYSTORE_PASSWORD. Both must already be set; this script never creates,
# prints or commits a key. Without them cargo-apk makes a debug APK, which this script rejects
# at the end so a "successful" unsigned build cannot be mistaken for an installable one.
set -o pipefail
cd "$(dirname "$0")/.." || exit 1
. scripts/android-env-linux.sh || exit 1

if [ -z "${CARGO_APK_RELEASE_KEYSTORE:-}" ] || [ -z "${CARGO_APK_RELEASE_KEYSTORE_PASSWORD:-}" ]; then
  echo "build-apk: set CARGO_APK_RELEASE_KEYSTORE and CARGO_APK_RELEASE_KEYSTORE_PASSWORD" >&2
  exit 2
fi

echo "=== building wordcraft-android (release, aarch64-linux-android) at $(date -Is) ==="
cargo apk build --release -p wordcraft-android
status=$?
echo "=== cargo apk exit=$status at $(date -Is) ==="
exit $status
