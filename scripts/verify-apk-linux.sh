#!/usr/bin/env bash
# Verify one built APK without trusting the build's exit code.
#
#   ANDROID_HOME=... JAVA_HOME=... scripts/verify-apk-linux.sh <apk> [lib-name]
#
# Checks: readability, package id, ABI from badging, signature, the .so inside the APK, its
# ARM64 machine type, and that the NativeActivity entry points are exported. Exit 0 only when
# every check passes.
set -o pipefail
cd "$(dirname "$0")/.." || exit 1
. scripts/android-env-linux.sh || exit 1

apk="${1:?usage: verify-apk-linux.sh <apk> [lib-name]}"
lib="${2:-wordcraft}"
BT="$ANDROID_HOME/build-tools/$BT_VER"
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
fail=0
check() { if eval "$2"; then echo "PASS  $1"; else echo "FAIL  $1"; fail=1; fi; }

check "APK exists and is non-empty" "[ -s \"$apk\" ]"
[ -s "$apk" ] || exit 1

echo "---- package / sdk / ABI"
"$BT/aapt2" dump badging "$apk" | grep -E "^package:|^sdkVersion:|^targetSdkVersion:|^application-label:|^native-code:|^launchable-activity:"
badging="$("$BT/aapt2" dump badging "$apk")"
check "package id is ai.storyteller.wordcraft" "printf '%s' \"\$badging\" | grep -q \"package: name='ai.storyteller.wordcraft'\""
check "native code is arm64-v8a only" "printf '%s' \"\$badging\" | grep -q \"native-code: 'arm64-v8a'\""

echo "---- signature"
"$BT/apksigner" verify --verbose --print-certs "$apk" 2>&1 | grep -E "Verified using|Verifies|DOES NOT VERIFY|Signer #1 certificate DN|Signer #1 certificate SHA-256"
sig=$("$BT/apksigner" verify --verbose "$apk" 2>&1)
check "APK signature verifies" "printf '%s' \"\$sig\" | grep -q 'Verifies' && ! printf '%s' \"\$sig\" | grep -q 'DOES NOT VERIFY'"

echo "---- contents"
unzip -l "$apk" | grep -E "classes\.dex|lib/arm64-v8a/.*\.so|AndroidManifest.xml"
check "lib/arm64-v8a/lib$lib.so is in the APK" "unzip -l \"$apk\" | grep -q \"lib/arm64-v8a/lib$lib.so\""

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
unzip -q -o "$apk" "lib/arm64-v8a/lib$lib.so" -d "$tmp"
so="$tmp/lib/arm64-v8a/lib$lib.so"
echo "---- ELF: $(file -b "$so")"
check "native library is AArch64" "file -b \"$so\" | grep -q 'ARM aarch64'"
echo "---- ELF LOAD segments (Android 16 wants 0x4000)"
"$NDK_BIN/llvm-readelf" -lW "$so" | grep "LOAD"
loads=$("$NDK_BIN/llvm-readelf" -lW "$so" | grep -c "LOAD")
aligned=$("$NDK_BIN/llvm-readelf" -lW "$so" | grep "LOAD" | grep -c "0x4000$")
check "the library has LOAD segments" "[ \"$loads\" -gt 0 ]"
check "every LOAD segment is 16 KB aligned" "[ \"$loads\" -gt 0 ] && [ \"$aligned\" -eq \"$loads\" ]"
echo "---- dynamic exports (expect android_main and ANativeActivity_onCreate)"
"$NDK_BIN/llvm-nm" --dynamic --defined-only "$so" | grep -E "android_main|ANativeActivity_onCreate"
check "android_main is exported" "\"$NDK_BIN/llvm-nm\" --dynamic --defined-only \"$so\" | grep -q ' android_main$'"
check "ANativeActivity_onCreate is exported" "\"$NDK_BIN/llvm-nm\" --dynamic --defined-only \"$so\" | grep -q ' ANativeActivity_onCreate$'"

echo "---- library name in the manifest matches"
name="$("$BT/aapt2" dump xmltree --file AndroidManifest.xml "$apk" | grep -A1 'android.app.lib_name' | grep -oE '"[^"]+"' | tail -1 | tr -d '\"')"
echo "android.app.lib_name=$name"
check "manifest lib_name is $lib" "[ \"$name\" = \"$lib\" ]"

echo "---- sha256"
sha256sum "$apk"

if [ "$fail" -ne 0 ]; then
  echo "VERIFICATION FAILED" >&2
  exit 1
fi
echo "VERIFICATION PASSED"
