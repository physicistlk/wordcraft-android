# Port notes — WordCraft on Android

Upstream: [`storytold/wordcraft`](https://github.com/storytold/wordcraft) @
`3de6ff95af86c19d67030cd96dba15cd6a45a25e` (v0.12.0)
Published APK: `wordcraft-0.12.0-arm64-v8a-20261028.apk`

The port is `apps/wordcraft-android/` plus a vendored, patched winit. Since 0.12.0 it changes **one
tracked upstream file** — the workspace `Cargo.toml` line that selects winit from the vendor
directory — and no application source. Everything below is the edit log: what was wrong, why, what
changed, how far it was checked.

## 2026-10-30 — Trackpad wheel + pen hover (already in the vendored winit)

This port already shipped the vendored, patched winit, whose Android backend forwards
`MotionAction::Scroll` (trackpad wheel / two-finger scroll) and `MotionAction::HoverMove`
(mouse / stylus hover) to `MouseWheel` and `CursorMoved`. No change this milestone; the other
five ports gained the same handling by vendoring the same winit.

## 2026-10-30 — Flush preferences on suspend (`raw_input_hook`)

Android never calls the exit path a swipe from Recents would need, and a paused Activity can be
killed without warning. The shell now detects a lost-focus frame (`raw.viewports` reporting
`focused == Some(false)`) and writes `ui.json` with `prefs_watch::Writer::store_now`, so the
recent-documents list and window state survive a suspend. PhotoCraft and the other ports that
persist preferences as they change are unaffected; VectorCraft gets the same treatment via
`save_prefs`.

## 2026-10-30 — Open-with and share intents (`src/intents.rs`)

**The complaint:** a file tapped in a file manager, or shared from another app, never reached
WordCraft; and there was no way to hand the current document to another app.

`src/intents.rs` covers the half of that which does not need `startActivityForResult` (which
`android-activity` 0.6.1 does not expose):

- **Incoming `ACTION_VIEW`** — `getIntent().getData()` is read at startup; a `content://` URI is
  copied to a file in the app's cache directory through `ContentResolver.openInputStream`, and the
  shell opens that file on its first frame (`WordApp::open_file`).
- **Incoming `ACTION_SEND`** — `EXTRA_TEXT` is written to a temp text file and opened; `EXTRA_STREAM`
  is copied like a `VIEW` URI.
- **Outgoing `ACTION_SEND`** — `intents::share_text` builds a chooser intent and starts it with
  `startActivity`; no `FileProvider` is needed for text. `intents::share_file` is provided but
  documented as needing a `FileProvider` entry in the manifest on Android 7+ (not added yet).

The launching intent is read in `android_main`, stored, and consumed once in `Shell::logic`, so a
cold start and a warm re-launch behave the same way.

**Scope:** opening a file handed to the app, and sharing text. Storage Access Framework pickers
(`ACTION_OPEN_DOCUMENT`/`ACTION_CREATE_DOCUMENT`) are not available without Java glue and remain
deferred; the in-app picker is still the way to open or save.

**Checked:** the module's host test passes, the crate compiles for `aarch64-linux-android`, and the
host suite is 17/17. The JNI intent path has not run on a device.

## 2026-10-30 — The system clipboard (`src/clipboard.rs`)

**The complaint:** copy and paste worked only *inside* the app. A paragraph copied in Chrome could
not be pasted into WordCraft, and copying in WordCraft left nothing for another app.

There were two independent causes, and both had to be fixed:

- **winit's Android backend has no clipboard at all** (there is no clipboard module in
  `src/platform_impl/android/`), so `egui-winit` keeps its *in-app* simulated clipboard. Copy and
  paste between two text fields of this app worked; anything crossing the app boundary did not.
- **The backend never reports modifiers** (`WindowEvent::ModifiersChanged` is not emitted; see §6),
  and `egui-winit` decides that Ctrl+C/X/V *is* a clipboard shortcut from its own modifier state.
  On Android `is_copy_command`/`is_paste_command` are therefore always false, so the three keys
  arrive as ordinary `Event::Key` presses and nothing ever produces `Event::Copy`, `Cut` or `Paste`.

`src/clipboard.rs` closes both:

- **`install(&app)`** keeps the Activity handle on the thread that runs the event loop, the same
  per-thread arrangement `modifiers` and `prefs_watch` use.
- **`shortcuts(raw)`** runs in `raw_input_hook`, after the touch bridge and before the app, and
  rewrites a plain Ctrl+C/X/V press into `Event::Copy`/`Cut`/`Paste`, consuming the key press the
  way a desktop backend does. Paste reads the **system** clipboard, so text from another app
  arrives; an empty clipboard pushes no event (an empty paste would clear a field). Ctrl+Shift and
  Ctrl+Alt variants are left to the app — they are different commands (Copy Merged, Paste Special).
- **`publish(ctx)`** runs after the app's `ui` in the same frame and takes every
  `OutputCommand::CopyText` to `ClipData.newPlainText` + `setPrimaryClip`. The command is left in
  place, so egui-winit's in-app copy still happens — which is what keeps paste working when the
  system clipboard is unreadable.
- **`text()`** is the read half for the app's own Paste paths, wired in as `Services::clipboard_text`
  (the ribbon and context-menu Paste read it, #584, rather than waiting for a key event).

The JNI calls are plain `ClipboardManager`/`ClipData`: `getSystemService(Context.CLIPBOARD_SERVICE)`,
`getPrimaryClip` → `getItemAt(0)` → `getText`, and `newPlainText` → `setPrimaryClip`. Android 10+
allows clipboard *reads* only while the app is focused, so a pending `SecurityException` is
described, logged and cleared rather than left to poison the next JNI call; the read then answers
"nothing", which is also what a non-text clip answers.

**Scope:** plain text only. Pictures and HTML (`ClipData` can carry both, and WordCraft's formatted
paste would use them) need a format negotiation this does not do yet; they are named as such in the
module's docs.

**Checked:** the module's eight unit tests (the key rewriting, the empty-clipboard and
Shift/Alt cases, the release pass-through, publishing `CopyText` and not publishing an empty copy)
pass on the host; `cargo check -p wordcraft-android --target aarch64-linux-android` is clean. The JNI
path itself has not run on a device — no Android device was connected for this build.

## 2026-10-28 — rebased from 0.5.0 onto 0.12.0

The port now targets upstream **v0.12.0** (`3de6ff9`), the newest official stable release at the time
of the build. Seven minor upstream releases (0.6 through 0.12) changed much more than the 0.5.0 seam
this port carried, so the rebase is an adaptation, not a mechanical re-apply:

- **The `Services::new_window` seam is gone, and with it the JNI second-Activity code.** Upstream
  0.12 opens several documents in one window and implements *View ▸ Window ▸ New Window* with
  `craft-window`/egui viewports. winit's Android backend still has one native window per Activity, so
  the port does not patch the upstream UI crate at all; the in-app document list (View ▸ Switch
  Windows) is the supported way to work on two documents. The old second-Activity/process-counting
  quit path was removed with it.
- **`Services::file_dialog` now answers `FileDialogAnswer = Result<Vec<String>, String>`** instead of
  `Option<String>`, and `FileDialogRequest` gained `OpenMany` (Add Font) and a starting directory on
  `Save`. The in-app picker was extended to return every chosen path and to honour the suggested
  start folder; `Err` lets the app fall back to its own built-in browser.
- **`FileDialogRequest`'s filters come from upstream `open_filters`**, so the picker offers the same
  formats the app actually opens (including the new `font` and `sources` purposes) instead of a
  hand-kept list.
- **`WordApp::raw_input_hook` now takes the `egui::Context`**; the shell forwards it.
- **egui 0.37 adds `source: MouseWheelSource` to `Event::MouseWheel`**; the two-finger bridge reports
  `Unknown` (these are fingers on the screen, not a trackpad).
- **The vendored winit is now the fork upstream 0.12 pins** (`storytold/winit` @ `a517b384`), not
  crates.io 0.30.13: the fork carries the IME/input changes egui 0.37 needs. The same two Android
  patches (`MotionAction::Scroll` → `MouseWheel`, `MotionAction::HoverMove` → `CursorMoved`) were
  re-applied to that source. The workspace's `[patch.crates-io] winit` line points at `vendor/winit`
  instead of the git revision, which is the only tracked upstream file this port now changes.
- **Preferences and fonts are unchanged in intent**: `ui.json` still lives in the app's data
  directory and is written atomically while the app runs, and the `Android/media/<package>/Fonts`
  folder still feeds the upstream `HOME`-based font scan.

**Checked on Linux (2026-10-28):** `cargo check -p wordcraft-android --target aarch64-linux-android`
passes; the workspace resolves with the vendored winit; `cargo apk build --release` produced an
arm64-v8a APK signed with a local test key. The verification results are recorded beside the APK in
the build output; runtime behaviour was not re-tested on a device here.

The sections below are the 0.5.0-era edit log, kept for the reasoning behind fixes that still apply
(modifiers, touch scrolling, the picker, preferences, fonts). Where a section mentions a second
Activity or the `new_window` seam, that part is historical: upstream 0.12 replaced it.

## 1. The shell crate

`wordcraft_ui_egui::Services` is a plain struct of `Option<Box<dyn Fn…>>` fields deriving `Default`,
so a host supplies what it can and omits the rest. The desktop binary and the browser shell fill it
their own way; Android is the third. Details that are load-bearing in every port of this set:

- **`[lib] name = "wordcraft"` + `crate-type = ["cdylib"]`** → `lib/arm64-v8a/libwordcraft.so`, and
  cargo-apk points the injected `android.app.lib_name` at the artifact name (`-` → `_`). A mismatch
  installs and then dies on launch.
- **`options.android_app = Some(app)`** — `eframe::run_native` errors without it.
- **A file logger in the app's own data directory** — `logcat` needs adb, which a sideloaded build
  does not have.
- **Preferences under `internal_data_path()`**, including the recent-documents list, which lives in
  `ui.json`.

## 2. Two documents side by side — a second Activity (`new_window`)

> **Removed in the 0.12.0 rebase.** Upstream 0.12 implements *View ▸ Window ▸ New Window* with egui
> viewports and the `Services::new_window` seam is gone (see the 2026-10-28 section above). This
> section is kept as the edit log for the 0.5.0 port; View ▸ Switch Windows is the way to work on
> several documents now.

WordCraft's *Window ▸ New Window* command is the way to review two documents at once, and on Android
the only thing that can show two windows of the same app is **two Activities**. The command could not
reach that: it opened a dialog, because the UI crate had no way to say "a host opened another window".

The upstream change is exactly that seam — a `Services::new_window: Option<Box<dyn Fn()>>` field, and
`"newWindow"` running it when a host supplies one (falling back to the dialog when nothing does):

```rust
"newWindow" => match &self.services.new_window {
    Some(open) => open(),
    None => self.dialog = dialogs::Dialog::open(what, self),
},
```

The shell implements `new_window()` with JNI: it keeps the `AndroidApp` handle it was handed at
startup (this is the one platform action that happens *after* startup, so the handle has to be kept)
and calls `startActivity` with the app's own `android.app.NativeActivity` component and
`FLAG_ACTIVITY_NEW_TASK`. Each window then runs its own event loop and its own document; the count of
live loops is what makes quitting one window safe (§4).

## 3. Quitting one window must not take the other with it

Before this, Exit was `std::process::exit(0)` — correct for the single-window case, wrong the moment
two windows exist: the process is shared, so the second window dies with the first.

`prefs_watch` counts the live event loops. When Exit is confirmed and more than one window is open,
the shell asks *its own* loop to end instead (`ViewportCommand::Close` plus clearing the app's quit
flag, so `run_native` returns and `android_main` returns) and lets android-activity retire that
Activity; only the last window ends the process. The count cannot lie, because it is decremented when
a loop actually returns.

## 4. Preferences are written while the app runs (`prefs_watch`)

Nothing on Android arrives at the place the desktop writes preferences. The desktop saves them on its
way out; Android gives no such moment, so a swipe from Recents took every preference with it —
including the **recent-documents list**, which is why saved documents kept vanishing from the Start
screen.

`prefs_watch` writes the preferences from the frame loop: change-detected (a hash), at most every two
seconds, and atomically (a temp file and a rename, so a kill mid-write cannot leave a truncated JSON
file where the preferences used to be). The app's own save-on-exit is still there for the clean path.

## 5. The Activity is not recreated (`config_changes`)

`[package.metadata.android.application.activity] config_changes = "orientation|keyboardHidden|…"`
so a rotation, a keyboard appearing, a density or font-scale change, or the window being resized does
not recreate the Activity. A recreated Activity destroys the native window while the render thread
lives on — a blank surface — and that is the symptom the porter reported when WordCraft was put into
split screen. The declaration is this port's defence against it.

## 6. Ctrl, Shift and Alt did nothing — `src/modifiers.rs`

From winit's own source: its Android backend emits only six window events and never `ModifiersChanged`.
`egui-winit` updates modifiers from that event and nowhere else, and egui carries modifiers as an
*event*, so `ctx.input(|i| i.modifiers)` returns the all-false default for the whole run — Ctrl+S,
Ctrl+Z and Shift-click are dead.

The key events do arrive, so the shell tracks the modifiers in `raw_input_hook` and re-injects them
the way egui expects: folded into the held state, stamped onto every `Key`/`PointerButton` event, plus
one `ModifiersChanged` per frame. State is carried **between** frames, because the press and the click
usually land in different ones.

## 7. File ▸ Open and Save As did nothing — `src/file_picker.rs`

`rfd` has no Android backend (its `backend.rs` compiles no module for a target that is neither
Linux/BSD, macOS, Windows nor wasm32; the build fails with `the trait bound FileDialog:
FilePickerDialogImpl is not satisfied`), and the Storage Access Framework needs
`startActivityForResult`, which `android-activity` 0.6 does not expose. So the picker is drawn with
egui, between frames, on WordCraft's `Services::file_dialog` seam — the **reply-callback** shape: the
shell stashes the request and its reply, returns at once, and answers from a later frame.

The answer carries a **path**, not finished bytes, and that is deliberate: with bytes only, the
document never learns which file it is — the title bar stays "Untitled", Ctrl+S asks for a location
again, and closing prompts to save what was just saved. A save dialog returns `dir.join(name)` because
the app's atomic write needs the parent directory to exist.

The picker itself follows the rules the sibling ports also follow: sized from `ctx.content_rect()`,
enlarged padding for fingers, a repaint requested while open, listings and readability probes cached
per visit rather than per frame, locked roots left tappable so they can name the permission they need,
the filter shown as the union of every extension the app asked for with **"Show all files"** and a
count of the hidden ones, and the app's own directory as the default so the first save always works
with no permission at all.

## 8. Permissions, icon, theme

`MANAGE_EXTERNAL_STORAGE` (granted by hand under Settings ▸ Special access; Google Play restricts it
to file-manager-class apps, which does not apply to a sideload), `READ_MEDIA_IMAGES` for Android 13+,
and the two legacy storage permissions capped with `max_sdk_version`. `res/values/styles.xml` carries
the same two-variant edge-to-edge theme as its siblings, needed because `targetSdkVersion 35` puts the
app under a transparent status bar on Android 15, and `res/mipmap-*` carries the launcher icon.

## Upstream file this port changes (0.12.0)

| File | Change | Why |
|---|---|---|
| `Cargo.toml` | `[patch.crates-io] winit = { path = "vendor/winit" }` instead of the pinned git revision | the vendor copy carries the Android wheel/hover fix and is the same fork revision upstream pins |

Nothing else upstream is touched. The earlier 0.5.0 port also added a `Services::new_window` seam to
`crates/ui-egui/src/lib.rs`; upstream 0.12 has its own multi-document window handling, so that seam
was removed rather than carried forward.

## 2026-10-13 — A vendored winit, so a mouse wheel is not swallowed

`touch_scroll.rs` covers fingers. A **mouse or trackpad** attached to the tablet was still dead: winit's
Android backend drops `DeviceEvent::Motion`-style actions outright, so the wheel and hover events never
reach the app, on any of these ports.

WordCraft carries the fix as a **vendored winit** — `vendor/winit/`, patched to forward those motion
actions, with `[patch.crates-io] winit = { path = "vendor/winit" }` in the workspace manifest. That is
why this repository ships a `vendor/` directory and why the build recipe copies it in: without it the
manifest does not resolve. The other ports read a pointer's wheel events the same way and would need
the same vendored crate; only this one has it so far.

**Checked:** the workspace resolves and the crate compiles for `aarch64-linux-android` with the
vendored winit in place (this is the tree the released APK was built from).

## 2026-10-13 — `adjustResize`, asked for through the theme

`android:windowSoftInputMode` has no home in cargo-apk's manifest schema, but it is a *themeable*
attribute (the platform's own base theme sets it as an item), so both theme variants now carry
`<item name="android:windowSoftInputMode">adjustResize</item>`. It asks the system to shrink the window
for the keyboard and the taskbar instead of drawing them over the app.

**What it does and does not achieve**, measured with `dumpsys window`: the window manager honours the
flag (`sim={adjust=resize}`), but nothing consumes the insets (`mImeInsetsConsumed=false`) — targeting
SDK 35 puts the app in charge of them, and consuming them needs a Java-side `WindowInsets` call this
project has no bridge for. It is kept because it is the right thing to ask for, and other Android
versions and devices do act on it.

## 2026-10-13 — Two-finger scrolling (and the wheel Android never sends)

**The complaint:** on the tablet a finger drag selected text instead of moving the page, and there was
no way to scroll at all.

**Why:** every app in this suite scrolls from `egui::Event::MouseWheel` — `WordCraft`'s `scroll.rs` is
explicit about the contract (pixel deltas move the page 1:1, `TouchPhase::End` on the last event is the
"fingers lifted" signal that starts the kinetic coast, wheel notches count as three lines). None of
that runs on Android, because winit's Android backend has no mouse and therefore emits no wheel
events at all (`src/platform_impl/android/mod.rs` sends six window events and a wheel is not one of
them). Touch arrives as `WindowEvent::Touch`, which egui-winit forwards as `egui::Event::Touch` *and*
turns into an emulated mouse pointer — so a finger drags a selection and the page never moves.

**The change — `src/touch_scroll.rs` (new, 231 lines, 3 tests), wired into the shell's
`raw_input_hook`:**

- one finger: untouched, so dragging still selects — the emulated pointer works as before;
- two or more fingers: the centroid's movement becomes `Event::MouseWheel` with
  `MouseWheelUnit::Point` and `TouchPhase::Move`;
- a final `TouchPhase::End` wheel event when the fingers lift, which is what makes the app coast to a
  stop instead of stopping dead;
- while a two-finger gesture is live the emulated **pointer button** events are dropped, or the first
  finger would drag a selection across the page at the same time as it scrolls. Pointer *position*
  events are kept, because the apps only scroll while the pointer is over the canvas;
- the touch events themselves stay in the batch: egui's pinch handling reads them, so a pinch still
  zooms and a two-finger *pan* moves nothing but the page.

**Ctrl+wheel zoom** works too, which is why `modifiers.rs`'s `apply()` now returns the state it tracked
(`egui::Modifiers`) instead of nothing: the bridge stamps it onto the wheel event it synthesises, and
the touch events carry it as well.

**Checked:** the three tests in `touch_scroll.rs` (one-finger pass-through, two-finger centroid →
wheel, the lift event) run on the host with `cargo test -p wordcraft-android`; the shell compiles for
`aarch64-linux-android`; and the string table of the `.so` inside the released APK carries the module.
On-device behaviour is the porter's tablet test, not an automated one.

## 2026-10-13 — `modifiers.rs` returns the tracked state

`apply(&mut self, raw)` now returns the `egui::Modifiers` it folded into the frame's events. The
synthesised two-finger wheel event has to carry modifiers itself (Ctrl+wheel is zoom in these apps,
plain wheel is scroll), and only the tracker knows them, so the state is handed back rather than
recomputed. Nothing else about the modifier handling changed.

## 2026-10-13 — The *Fonts* folder, in the shells that did not have one

PhotoCraft and VectorCraft already pointed their platform-font hook at a USB-visible
`Android/media/<package>/Fonts`; this shell did not, so there was no way to install a font on the
tablet at all.

The shell now creates `<App media>/Fonts`, sets `HOME` to the app's media directory, and symlinks
`.fonts` and `.local/share/fonts` to that one folder, so every path a font scan is likely to consult
lands in the same visible place. A `.ttf`/`.otf` dropped in over a cable appears in the app's font
list on the next scan. `Android/media` rather than `Android/data`, deliberately: Android 11+ hides
`Android/data` from MTP.

## 2026-10-13 — The picker no longer tells a *granted* user to grant access

The empty-folder message in shared storage used to end with "Grant *All files access*…", and it was
shown to everyone, including users who had already granted it. The trigger had been
`/storage/emulated/0/Android/data` failing to list — **measured on the emulator: that folder stays
unlistable to the app on API 35 even with the grant**, so the tell was wrong and the advice was
insulting to the one user who had done what it asked.

Now the note is conditional on the folder being shared storage, is worded as a possibility ("If your
files are here, the app may lack shared-storage access…"), and leaves the decision to the user,
because the grant cannot be probed from inside the app.

The USB tip became folder-dependent for the same reason — Android 11+ hides `Android/data` from MTP,
so a file copied there over a cable never arrives and the folder merely looks empty (which reads as a
picker bug), while `Android/media` *is* reachable over a cable. Saying the wrong one of those two is
worse than saying nothing.
## 2026-10-13 — Ctrl shortcuts: the tracker reads the physical key too

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

## What is deliberately *not* in the patch

`Cargo.lock` is excluded on purpose: it moves only because the port crate is a new workspace member,
and `cargo` regenerates it on the first build. The patch contains only changes that matter to a reader
— every tracked file the port touches, plus any new file that has to live inside an upstream directory.

## Not verified here

The APK attached to **v0.1.2** was built from exactly this tree on 2026-10-13 (upstream pin + the patch + the port crate + the vendored winit), and the checks in the README's verification table were run against it. What that does *not* buy is a runtime claim: it was not launched on a device or emulator as part of publishing, so every behavioural statement below rests on the hand-testing the porter did on an arm64 tablet. The fixes above came
from the porter's hand-testing on an arm64 tablet — the bugs they fix were reported there — but there
is no automated device test here, and "verified" in the README means source-side checks plus that
testing, not a fresh run of this exact binary.
