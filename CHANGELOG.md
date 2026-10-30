# Changelog

## v0.2.1 — 2026-10-30 (system clipboard)

| | |
|---|---|
| APK | `wordcraft-0.12.0-arm64-v8a-20261030.apk` — sha256 in `SHA256SUMS.txt` beside the build |
| Built from | upstream `storytold/wordcraft` @ `3de6ff95af86c19d67030cd96dba15cd6a45a25e` (v0.12.0) + this repository at the commit that released it |
| Package | `ai.storyteller.wordcraft` — versionName `0.12.0`, versionCode `16780288`, minSdk 26, targetSdk 35 |
| ABI | `arm64-v8a` only |
| Signature | local test key (Mode A); not an official WordCraft key |

### Changed
- **The system clipboard works for text (`src/clipboard.rs`).** Copy and paste crossed the app
  boundary for the first time: Ctrl+C/X/V now produce `Event::Copy`/`Cut`/`Paste` (the Android
  backend never reports modifiers, so `egui-winit` never did), Paste reads
  `ClipboardManager` through JNI, and `OutputCommand::CopyText` is written back with
  `ClipData.newPlainText`. The ribbon and context-menu Paste read the same clipboard through
  `Services::clipboard_text` (#584). Plain text only: pictures and HTML are not negotiated yet.

### Unchanged
- Everything in v0.2.0: the 0.12.0 rebase, the in-app picker, two-finger scrolling, modifier
  tracking, preferences written while the app runs, the *Fonts* folder and the theme fixes.

### Notes
Built and verified on Linux with NDK r27.2.12479018, build-tools 36.0.0, JDK 21 and cargo-apk 0.10.0.
The clipboard's JNI path was not exercised on a device as part of this build (no device was
connected); its key handling and publishing are covered by unit tests.

## v0.2.0 — 2026-10-28 (rebased on WordCraft 0.12.0)

| | |
|---|---|
| APK | `wordcraft-0.12.0-arm64-v8a-20261028.apk` — sha256 in `SHA256SUMS.txt` beside the build |
| Built from | upstream `storytold/wordcraft` @ `3de6ff95af86c19d67030cd96dba15cd6a45a25e` (v0.12.0) + this repository at the commit that released it |
| Package | `ai.storyteller.wordcraft` — versionName `0.12.0`, versionCode `16780288`, minSdk 26, targetSdk 35 |
| ABI | `arm64-v8a` only |
| Signature | local test key (Mode A); not an official WordCraft key |

### Changed
- **Upstream moved from 0.5.0 to 0.12.0**, seven minor releases of document, layout, format and UI
  work. The port follows the current `Services`, `FileDialogRequest`/`FileDialogAnswer` and
  `egui 0.37` APIs instead of the 0.5.0 shapes.
- **The second-Activity `new_window` seam was removed.** Upstream 0.12 opens multiple documents in
  one window and implements *View ▸ Window ▸ New Window* with egui viewports; winit's Android
  backend still has one native window per Activity. Use View ▸ Switch Windows for several documents.
  The port no longer patches `crates/ui-egui/src/lib.rs`.
- **The file picker answers every path the app asks for** (`OpenMany` for Add Font) and honours a
  suggested start folder; the filter list now comes from upstream `open_filters`, so new formats and
  the `font`/`sources` purposes are offered without a second list to maintain.
- **The vendored winit is the fork revision upstream 0.12 pins**, with the same Android wheel and
  hover patches re-applied. This is the one tracked upstream file the patch changes now
  (`Cargo.toml`'s `[patch.crates-io] winit`).

### Unchanged
- Two-finger scrolling, modifier tracking (including Ctrl from the physical key), preferences written
  atomically while the app runs, the `Android/media/<package>/Fonts` folder, the in-app picker and
  the `config_changes` / fullscreen theme fixes.

### Notes
Built and verified on Linux with NDK r27.2.12479018, build-tools 35.0.0, JDK 21 and cargo-apk 0.10.0.
Runtime behaviour was not re-tested on a device as part of this build.

## v0.1.2 — 2026-10-13 (Ctrl shortcuts)

| | |
|---|---|
| APK | `wordcraft-0.5.0-arm64-v8a-20261013b.apk` (19.9 MB) — supersedes the v0.1.1 asset, same signing key |
| sha256 | `05065f17c115a5fed20f9ed6f04dcd1cd6b31bd8128e8177031e8d6f9a522ca5` |
| Built from | upstream `storytold/wordcraft` @ `2ffba93b5189410a415970be179dcf481092d756` + this repository at the commit that released it |
| Package | `ai.storyteller.wordcraft` — versionName `0.5.0`, versionCode `16778496`, minSdk 26, targetSdk 35 |
| ABI | `arm64-v8a` only |

### Fixed
- **Ctrl+anything did nothing, while Shift+click and Shift+arrow worked.** winit's Android
  logical-key table names `Shift` and `Alt` but **not** `Control` — so a Ctrl press arrives with an
  unidentified *logical* key (egui-winit falls back to the physical one) while its physical key is
  correctly `ControlLeft`/`ControlRight`. `src/modifiers.rs` matched the logical key alone, so it
  never saw Ctrl go down: Ctrl+A, Ctrl+C, Ctrl+V, Ctrl+S, Ctrl+B and every other modifier shortcut
  was dead, and only the modifiers winit happens to name worked. The tracker now reads the physical
  key as well as the logical one, on both left and right, and keeps everything else: the per-event
  modifier copies (so a press and a click in one frame stay ordered) and `ModifiersChanged` as the
  authority (so a modifier held while the app goes to the background does not stick).
  Verified on an Android 15 emulator: with text typed in the document, `Ctrl+A` selects it
  (the mini format bar appears) and `Ctrl+B` emboldens it, ribbon state included.

## v0.1.1 — 2026-10-13 (scrolling, modifiers, fonts, picker wording)

| | |
|---|---|
| APK | `wordcraft-0.5.0-arm64-v8a-20261013.apk` (19.9 MB) — supersedes the v0.1.0 asset, same signing key |
| sha256 | `9c21e2ab11f91c89bc00188d685d55b800688eabf2d531a335ea08a9caf619f8` |
| Built from | upstream `storytold/wordcraft` @ `2ffba93b5189410a415970be179dcf481092d756` + this repository at the commit that released it |
| Package | `ai.storyteller.wordcraft` — versionName `0.5.0`, versionCode `16778496`, minSdk 26, targetSdk 35 |
| ABI | `arm64-v8a` only |

### Changed
- **Two-finger scrolling.** A two-finger drag now scrolls the document (synthesised `MouseWheel`
  events from the touch centroid, with the lift event that starts the kinetic coast); one finger still
  selects, a pinch still zooms, and Ctrl+wheel zooms.
- **A mouse or trackpad wheel works.** winit's Android backend drops motion actions, so the wheel and
  hover events never reached the app; this repository now carries a **vendored, patched winit**
  (`vendor/winit/`) that forwards them, referenced by a `[patch.crates-io]` entry in the workspace
  manifest. The build recipe copies it in.
- **A *Fonts* folder** at `Android/media/<package>/Fonts`, with `.fonts` and `.local/share/fonts`
  symlinked to it, so a `.ttf` copied in over a USB cable is picked up — previously there was no way
  to install a font on the tablet at all.
- **Picker wording corrected** (see the notes): no grant advice to a user who already granted, and a
  USB tip that matches the folder.
- **`android:windowSoftInputMode=adjustResize`** asked for through both theme variants, so the system
  shrinks the window for the keyboard and the taskbar. On Android 15 the flag is honoured but nothing
  consumes the insets yet — see the notes for exactly what it does and does not achieve.

### Notes
The build was made from exactly the source in this repository (upstream pin + the patch + the port
crate) and signed with the same local test key as v0.1.0, so it installs over it.


## v0.1.0 — first published port (WordCraft 0.5.0)

| | |
|---|---|
| APK | `wordcraft-0.5.0-arm64-v8a-v4.apk` (19.9 MB) |
| sha256 | `726b570c652a9630cdc0cdc499e9157890ced6838ffdcae68937f2417e1fccfa` |
| Built from | upstream `storytold/wordcraft` @ `2ffba93b5189410a415970be179dcf481092d756` + `patches/0001-wordcraft-android-port.patch` + `apps/wordcraft-android/` |
| Package | `ai.storyteller.wordcraft` — versionName `0.5.0`, minSdk 26, targetSdk 35 |
| ABI | `arm64-v8a` only |
| Signature | throwaway local test key, not an official key |

### In this release
- Android shell crate: NativeActivity entry point, `Services` implementation, file logger, preferences
  under the app's own data directory.
- **Window ▸ New Window opens a real second window** (a second Activity over JNI), so two documents can
  be reviewed side by side — with a quit path that ends one window instead of the process.
- **Preferences written while the app runs** (change-detected, atomic), so the recent-documents list
  and every other preference survive an Android kill rather than only a clean exit.
- **Ctrl / Shift / Alt work** — the platform reports no modifiers, so they are tracked from key events
  and re-injected.
- **Activity not recreated** on rotation, docking, split-screen resize, density or font-scale change
  (`config_changes`).
- In-app file picker for Open and Save As, returning the chosen **path**, with cached listings, "Show
  all files", hidden-file count and self-explaining locked roots.
- Launcher icon, storage permissions, edge-to-edge theme variant.


### Known issues
See *Known gaps* in the [README](README.md) and [`docs/PORT-NOTES.md`](docs/PORT-NOTES.md).
