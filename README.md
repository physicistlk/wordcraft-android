# WordCraft for Android (arm64-v8a)

An unofficial Android port of **[WordCraft](https://github.com/storytold/wordcraft)** — the real engine
and egui interface, packaged as a NativeActivity APK for sideloading on arm64 devices.

| | |
|---|---|
| Upstream project | [`storytold/wordcraft`](https://github.com/storytold/wordcraft) — Apache-2.0 OR MIT |
| Pinned commit | `079f478b` (upstream version `0.18.1`) |
| Android package | `ai.storyteller.wordcraft` |
| Published APK | `wordcraft-0.18.1-arm64-v8a-20261112.apk` — arm64-v8a, signed with a local test key (release **v0.18.1**, 2026-11-12) |
| APK sha256 | `1efc7d261d49b344e973849ca06a88ba55f9ed56fa6c8fd38b89109659509d19` |
| Upstream files changed | 277 files: `Cargo.toml` + `Cargo.lock` (the winit patch source and the new crate's lock entry), `apps/wordcraft-android/` (14 files, resources included) and the whole `vendor/winit/` tree — see [`patches/`](patches/) |
| Vendored crates | `vendor/winit/` — the pinned upstream fork plus the Android wheel/hover fix, copied in with the crate |
| New port crate | `apps/wordcraft-android/` — the shell crate plus resources |

## What this repository is

This is **not a fork** of WordCraft. It holds only the Android layer, so it stays small, readable,
and obviously separable from upstream:

```
apps/wordcraft-android/     the new shell crate — copy it into your checkout of upstream
patches/                the diff against upstream — apply it after checking out the pin
vendor/winit/           the patched winit the manifest points at ([patch.crates-io]) — copy it in too
docs/PORT-NOTES.md      what was changed and why, fix by fix, with how each one was checked
scripts/                cross-compile environment, build and verify scripts (from shared-android-port)
CHANGELOG.md            what is in the published APK
```

The Android layer is not a reimplementation. It is a shell crate that fills the same `Services`
struct the desktop and web shells fill, so the real engine and the real egui UI run unchanged.

## Build it

Prerequisites: Rust (rustup) with the `aarch64-linux-android` target, Android SDK + NDK r27, a
JDK 21, and `cargo install cargo-apk --version 0.10.0`.

```bash
git clone https://github.com/storytold/wordcraft.git
cd wordcraft
git fetch --depth 1 origin 079f478b
git checkout FETCH_HEAD                        # the exact commit this port targets
git apply -3 /path/to/0001-wordcraft-android-port.patch
cp -r /path/to/wordcraft-android/apps/wordcraft-android apps/
cp -r /path/to/vendor .                        # the patched winit the manifest points at

# any keystore of your own signs an installable APK; a throwaway test key is fine for sideloading
keytool -genkeypair -keystore ks.jks -alias wordcraft -keyalg RSA -keysize 3072 -validity 10000 \
        -dname "CN=Local Test, O=Local, C=US"
# keytool prompts for the two passwords; put them in the environment for cargo-apk
read -rsp "keystore password: " CARGO_APK_RELEASE_KEYSTORE_PASSWORD; echo
export CARGO_APK_RELEASE_KEYSTORE="$PWD/ks.jks"
cargo apk build --release -p wordcraft-android
```

On Linux use the tracked scripts instead of hard-coding your SDK paths:

```bash
export ANDROID_HOME=/path/to/android-sdk JAVA_HOME=/path/to/jdk-21
export CARGO_APK_RELEASE_KEYSTORE=/path/to/ks.jks
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD='...'
scripts/build-apk-linux.sh
scripts/verify-apk-linux.sh target/release/apk/wordcraft.apk wordcraft
```

`scripts/android-env-linux.sh` is the cross-compile environment (NDK clang at API 26, the C compiler
ring/rustls need, and the JDK); `scripts/build-apk-linux.sh` wraps the build;
`scripts/verify-apk-linux.sh` checks the artifact with `aapt2`, `apksigner`, `unzip`, `llvm-nm`,
`file` and `sha256sum`. The original `scripts/android-env.sh`, `build-apk.sh` and `verify-apk.sh`
remain as the Windows/MSYS environment the first releases were built in.

## Install

```bash
adb install -r wordcraft-0.5.0-arm64-v8a-v4.apk
```
or copy the APK to the device and open it (installing from unknown sources must be allowed).

On first run, grant **Settings ▸ Apps ▸ WordCraft ▸ Special access ▸ All files access** if you want
the in-app picker to reach files outside the app's own folders. Without it, Android 10+ refuses a
`read_dir` of shared storage by path, and the picker can only offer the app's own directory.

## Verification status

Everything below was checked on the machine this port was published from, in October 2026; the
0.12.0 build was checked on Linux with NDK r27 and `cargo-apk` 0.10.0.
A tick means the check was *run*, not inferred; the wording says how far it goes.

| Claim | How it was checked | Result |
|---|---|---|
| `patches/…patch` applies to a clean checkout of the pin | fresh `git init` + `git fetch --depth 1 origin 079f478b` + `git apply --check` | ✅ applies cleanly |
| The assembled tree equals the tree the APK was built from | file-by-file SHA-256 (line endings normalised) over every patched file and every file of the port crate | ✅ 12 files identical, none differing |
| The port crate compiles for Android | `cargo check -p wordcraft-android --target aarch64-linux-android`, run in a tree assembled from scratch (fresh `git fetch` of the pin, patch applied, port crate copied in) | ✅ exit 0 |
| The port crate's own tests | `cargo test -p wordcraft-android` in the freshly assembled tree | ✅ 16/16 pass |
| APK is signed and is the right artifact | `apksigner verify --verbose`, `aapt2 dump badging`, sha256 | ✅ v2/v3 signatures true, `native-code: 'arm64-v8a'`, package `ai.storyteller.wordcraft` |
| Behaviour on real hardware | hand-testing on the porter's arm64 tablet | ⚠️ the fixes below were found and driven by that testing; each published binary is not individually re-verified here |

**Before trusting the APK:** the source side is verified as listed above; runtime behaviour only as far
as [`docs/PORT-NOTES.md`](docs/PORT-NOTES.md) says per fix, plus the tablet testing that produced those
fixes. There is no automated device test in this repository.

## Known gaps

- **Runtime verification is hand-testing only**, plus the source-side checks in the README. No
  automated device test.
- **All files access is a manual grant.** Until it is given, the picker can only offer the app's own
  directory (and says so on the locked root).
- **No audio** — `Services::audio_out` stays `None` (`cpal` has no Android backend here).
- **The system clipboard is text only.** Copy/Paste crossed the app boundary for text;
  pictures and HTML on the clipboard are still out of reach (`ClipData` carries them, but this
  port does not negotiate those formats yet).
- **View ▸ Window ▸ New Window** asks for an egui viewport; winit's Android backend has one native
  window per Activity, so use the in-app window list (View ▸ Switch Windows) instead of a second OS
  window. The old second-Activity trick is gone because upstream 0.12 owns multiple documents.
- **Android 16 ignores the status-bar opt-out** used by theme variant A; the port ships variant B.


## The rest of this set

Every port in this set is built from the same patched upstream tree and published by
[@physicistlk](https://github.com/physicistlk). Each link goes to that port's latest
release, which carries the signed `arm64-v8a` APK:

- [cadcraft-android](https://github.com/physicistlk/cadcraft-android/releases/latest) — CadCraft 0.20.1
- [deckcraft-android](https://github.com/physicistlk/deckcraft-android/releases/latest) — DeckCraft 0.18.1
- [gridcraft-android](https://github.com/physicistlk/gridcraft-android/releases/latest) — GridCraft 0.19.1
- [lightcraft-android](https://github.com/physicistlk/lightcraft-android/releases/latest) — LightCraft 0.21.1
- [pdfcraft-android](https://github.com/physicistlk/pdfcraft-android/releases/latest) — PdfCraft 0.21.1
- [photocraft-android](https://github.com/physicistlk/photocraft-android/releases/latest) — PhotoCraft 0.23.1
- [vectorcraft-android](https://github.com/physicistlk/vectorcraft-android/releases/latest) — VectorCraft 0.24.1

## Credits and licence

WordCraft is © its authors at [`storytold/wordcraft`](https://github.com/storytold/wordcraft) and is
dual-licensed Apache-2.0 / MIT; both licence texts are kept here. The Android layer in this
repository is an unofficial modification by [@kanaaka](https://github.com/kanaaka), carrying the
same licence. Not affiliated with or endorsed by the upstream project. See
[`ATTRIBUTION.md`](ATTRIBUTION.md).


## Android 16 build (physicistlk fork)

This fork carries the Android 16 port (arm64-v8a, 16 KB page-aligned) plus the
tablet-compatibility milestones (system clipboard, open-with/share intents, flush on
suspend, back-gesture closes dialog, trackpad wheel + pen hover, stylus pressure for
PhotoCraft, PhotoCraft crash-recovery autosave).

A signed APK for the current state is attached to the
[v2026.10.30 release](https://github.com/physicistlk/wordcraft-android/releases/tag/v2026.10.30).

See the upstream `kanaaka` repository for the full port notes and verification details.
