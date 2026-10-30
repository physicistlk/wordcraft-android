#!/usr/bin/env bash
# Android cross-compile environment for Linux, sourced by build-apk-linux.sh and
# verify-apk-linux.sh.
#
# Nothing here is machine-specific: set ANDROID_HOME (or ANDROID_SDK_ROOT) and JAVA_HOME
# in the environment, or edit an untracked local file that sets them before sourcing this.
#
#   ANDROID_HOME          the SDK root (contains ndk/, build-tools/, platforms/)
#   JAVA_HOME             a JDK 21 (cargo-apk runs apksigner/d8 from the build-tools)
#   ANDROID_NDK_VERSION   defaults to the r27 the port was built against
#   ANDROID_MIN_SDK       defaults to 26, matching [package.metadata.android.sdk]
#
# The linker and C compiler are the NDK's clang wrappers for aarch64 at API 26. The C
# compiler is not optional: aws-lc/ring (behind rustls, via craft-update) compile C and
# assembly for the target, and cc-rs reads CC_<target> rather than the linker setting.
set -o pipefail

: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-}}"
if [ -z "$ANDROID_HOME" ]; then
  echo "android-env: set ANDROID_HOME (or ANDROID_SDK_ROOT) to your Android SDK" >&2
  return 1 2>/dev/null || exit 1
fi
: "${JAVA_HOME:?android-env: set JAVA_HOME to a JDK 21}"
: "${ANDROID_NDK_VERSION:=27.2.12479018}"
: "${ANDROID_MIN_SDK:=26}"

export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$ANDROID_NDK_VERSION"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"

NDK_HOST="linux-x86_64"
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_HOST/bin"
CLANG="$NDK_BIN/aarch64-linux-android${ANDROID_MIN_SDK}-clang"
CLANGXX="$NDK_BIN/aarch64-linux-android${ANDROID_MIN_SDK}-clang++"

BT_VER="${ANDROID_BUILD_TOOLS_VERSION:-$(ls -1 "$ANDROID_HOME/build-tools" 2>/dev/null | sort -V | tail -1)}"
: "${BT_VER:?android-env: no build-tools under $ANDROID_HOME/build-tools}"
for tool in "$CLANG" "$CLANGXX" "$JAVA_HOME/bin/java" "$ANDROID_HOME/build-tools/$BT_VER/aapt2"; do
  if [ ! -x "$tool" ]; then
    echo "android-env: missing $tool" >&2
    return 1 2>/dev/null || exit 1
  fi
done

export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$NDK_BIN:$HOME/.cargo/bin:$PATH"

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG"
export CC_aarch64_linux_android="$CLANG"
export CXX_aarch64_linux_android="$CLANGXX"
export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
export RANLIB_aarch64_linux_android="$NDK_BIN/llvm-ranlib"

echo "android-env: SDK=$ANDROID_HOME NDK=$ANDROID_NDK_HOME JDK=$JAVA_HOME target=aarch64-linux-android$ANDROID_MIN_SDK"

# Android 16 requires a 16 KB page size: a native library whose ELF segments are only 4 KB
# aligned still runs, but the system shows the user a compatibility warning on a device with
# 16 KB pages, and Play will not accept an update that is not aligned.
#
# The flag has to go in RUSTFLAGS, and in exactly one token. cargo-apk does not link the cdylib
# itself: it shells out to `ndk-build`, whose `cargo_ndk()` re-encodes RUSTFLAGS into
# CARGO_ENCODED_RUSTFLAGS and appends its own `-Clink-arg=--target=...`. That encoding
# *overrides* CARGO_TARGET_<triple>_RUSTFLAGS, so the flag cannot be passed there; and it
# splits RUSTFLAGS on spaces, so a flag with a space in it is broken in two. One token, no
# spaces, in RUSTFLAGS, is what survives to the linker.
#
# With NDK r27 the result is what Android 16 wants: every LOAD segment 0x4000-aligned.
export RUSTFLAGS="${RUSTFLAGS:--Clink-arg=-Wl,-z,max-page-size=16384}"
