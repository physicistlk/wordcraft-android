//! The Android system clipboard, over JNI — and the two frames around it.
//!
//! # The gap
//!
//! winit 0.30.13's Android backend has no clipboard API at all (there is no `clipboard` module in
//! `src/platform_impl/android/`), so `egui-winit` falls back to its *in-app* simulated clipboard:
//! copy and paste work between two text fields of this app and nowhere else. Copying a paragraph
//! in Chrome and pasting it into WordCraft does nothing, and copying from WordCraft leaves nothing
//! for another app.
//!
//! The modifiers matter here too. `egui-winit` decides whether <kbd>Ctrl</kbd>+<kbd>C</kbd>,
//! <kbd>X</kbd> or <kbd>V</kbd> is a clipboard shortcut from *its own* modifier state, which the
//! Android backend never updates (see [`crate::modifiers`]). So on Android the three keys arrive
//! as ordinary `Event::Key` presses, and nothing turns them into `Event::Copy`, `Event::Cut` or
//! `Event::Paste` — which is what every Craft app waits for.
//!
//! # What this does
//!
//! - [`install`] keeps this window's [`AndroidApp`] on the thread that runs the event loop.
//! - [`shortcuts`] rewrites <kbd>Ctrl</kbd>+<kbd>C</kbd>/<kbd>X</kbd>/<kbd>V</kbd> into
//!   `Event::Copy`/`Cut`/`Paste`, exactly as a desktop backend would. Paste reads the *system*
//!   clipboard, so text copied in another app arrives.
//! - [`publish`] takes what egui copied this frame (`OutputCommand::CopyText`) and puts it on the
//!   system clipboard, so another app can paste it. The command is left in place: egui-winit still
//!   keeps its in-app copy, which is what makes paste work when the system clipboard is
//!   unreadable (Android 10+ only allows a focused app to read it).
//! - [`text`] is for the apps whose menu Paste reads the clipboard through `Services` rather than
//!   through a key event (WordCraft's ribbon, GridCraft's ribbon, VectorCraft's menus).
//!
//! The JNI calls mirror what `ClipboardManager` and `ClipData` expose: `getSystemService` once per
//! call, `getPrimaryClip`/`getText` to read, `ClipData.newPlainText` + `setPrimaryClip` to write.
//! They are cheap (a handful of JNI calls, no allocation on the Java heap beyond the string), and
//! only run on a clipboard gesture or when an app polls for Paste enablement.
//!
//! # Not here
//!
//! Pictures and HTML. `ClipData` can carry both (`newHtmlText`, `Uri` + `ContentResolver`), and
//! some apps can consume them (WordCraft's formatted paste, DeckCraft's picture paste, PhotoCraft's
//! image paste), but those need a format negotiation this module does not do yet: plain text is
//! what every text field and every app's Paste command can take.

#[cfg(target_os = "android")]
use std::cell::RefCell;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
thread_local! {
    /// This window's `AndroidApp`, captured in `android_main`.
    ///
    /// Per-thread, not a global: android-activity warns that an `AndroidApp` dies with its
    /// Activity, and a second window of the same app runs `android_main` on a thread of its own
    /// (see the ports' `new_window`). Each thread's copy is cleared when its loop returns.
    static APP: RefCell<Option<AndroidApp>> = const { RefCell::new(None) };
}

/// Remember the Activity the clipboard calls go through. Called once, before `run_native`.
#[cfg(target_os = "android")]
pub fn install(app: &AndroidApp) {
    APP.with(|slot| *slot.borrow_mut() = Some(app.clone()));
}

/// The text on the system clipboard, if the clipboard holds text and this app may read it.
///
/// `None` covers every honest no: no Activity (host build, before `install`), an empty clipboard,
/// an image or URI on it, and the `SecurityException`/`NullPointerException` Android raises when a
/// background app asks (Android 10+ allows clipboard reads only while focused).
pub fn text() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        return with_clipboard(read_text).flatten();
    }
    #[cfg(not(target_os = "android"))]
    None
}

/// Replace the system clipboard with `text`.
pub fn set_text(text: &str) {
    #[cfg(target_os = "android")]
    with_clipboard(|env: &mut jni::Env<'_>, activity: &jni::objects::JObject<'_>| {
        write_text(env, activity, text)
    });
}

/// Turn this frame's <kbd>Ctrl</kbd>+<kbd>C</kbd>/<kbd>X</kbd>/<kbd>V</kbd> presses into the
/// clipboard events the apps are written for.
///
/// The press is consumed and replaced, as `egui-winit` does on the desktop: an app that also
/// watches the key would otherwise paste twice. Releases are left alone — a release without its
/// press is a case the apps already handle (that is what a Mac menu key equivalent looks like).
///
/// Shift and Alt are left to the app: <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> is Copy Merged
/// in PhotoCraft and a different command in WordCraft, and <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>V</kbd>
/// is Paste Special in several of them.
pub fn shortcuts(raw: &mut egui::RawInput) {
    shortcuts_with(raw, text);
}

fn shortcuts_with(raw: &mut egui::RawInput, read: impl FnOnce() -> Option<String>) {
    let (mut copy, mut cut, mut paste) = (false, false, false);
    for event in &raw.events {
        let egui::Event::Key { key, pressed: true, modifiers, .. } = event else {
            continue;
        };
        if !is_plain_command(*modifiers) {
            continue;
        }
        match key {
            egui::Key::C => copy = true,
            egui::Key::X => cut = true,
            egui::Key::V => paste = true,
            _ => {}
        }
    }
    if !(copy || cut || paste) {
        return;
    }

    raw.events.retain(|event| {
        !matches!(
            event,
            egui::Event::Key {
                key: egui::Key::C | egui::Key::X | egui::Key::V,
                pressed: true,
                modifiers,
                ..
            } if is_plain_command(*modifiers)
        )
    });

    if copy {
        raw.events.push(egui::Event::Copy);
    }
    if cut {
        raw.events.push(egui::Event::Cut);
    }
    if paste && let Some(text) = read() {
        // An empty clipboard pushes nothing: that is what the desktop backend does, and a
        // `Paste("")` would clear a text field where Paste should be a no-op.
        if !text.is_empty() {
            raw.events.push(egui::Event::Paste(text));
        }
    }
}

/// A Ctrl shortcut with no Shift or Alt: Copy, Cut and Paste, and not their variants.
fn is_plain_command(modifiers: egui::Modifiers) -> bool {
    (modifiers.command || modifiers.ctrl) && !modifiers.shift && !modifiers.alt
}

/// Put what egui copied this frame on the system clipboard.
///
/// Called from the shell's `ui` after the app, in the same frame the copy happened: the command
/// list is cleared at the end of the frame, so this is the only place that sees it before
/// egui-winit does.
pub fn publish(ctx: &egui::Context) {
    publish_with(ctx, set_text);
}

fn publish_with(ctx: &egui::Context, mut write: impl FnMut(&str)) {
    let copied: Vec<String> = ctx.output(|output| {
        output
            .commands
            .iter()
            .filter_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    });
    for text in copied {
        if !text.is_empty() {
            write(&text);
        }
    }
}

/// Run `call` with this thread attached to the VM and this window's Activity as a `Context`.
///
/// Errors are logged, not propagated: a clipboard read that fails is an empty paste, and a write
/// that fails leaves egui's in-app copy working. A pending Java exception is described and then
/// cleared, because JNI calls with an exception pending are not allowed to continue.
#[cfg(target_os = "android")]
fn with_clipboard<T>(
    call: impl for<'a> FnOnce(&mut jni::Env<'a>, &jni::objects::JObject<'a>) -> jni::errors::Result<T>,
) -> Option<T> {
    use jni::objects::JObject;

    let app = APP.with(|slot| slot.borrow().clone())?;
    // SAFETY: `vm_as_ptr` is the JavaVM this process is already attached to, and
    // `JavaVM::from_raw` is a singleton behind a non-null assert — passing the pointer is all it
    // takes. `activity_as_ptr` is this Activity's own reference, valid for as long as its
    // `AndroidApp` is; `JObject` does not own what it points at (no `Drop`), so wrapping the
    // pointer is what android-activity's own JNI example does.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;

    let result = vm.attach_current_thread(|env| -> jni::errors::Result<T> {
        let activity = unsafe { JObject::from_raw(env, raw_activity) };
        let result = call(env, &activity);
        if env.exception_check() {
            env.exception_describe();
            env.exception_clear();
        }
        result
    });
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            log::warn!("clipboard: {e}");
            None
        }
    }
}

/// The `ClipboardManager`, asked of the Activity (which is a `Context`).
#[cfg(target_os = "android")]
fn manager<'local>(
    env: &mut jni::Env<'local>,
    activity: &jni::objects::JObject<'local>,
) -> jni::errors::Result<jni::objects::JObject<'local>> {
    use jni::JValue;

    let service = env
        .get_static_field(
            class(c"android/content/Context"),
            name(c"CLIPBOARD_SERVICE"),
            field("Ljava/lang/String;")?.field_signature(),
        )?
        .l()?;
    let manager = env.call_method(
        activity,
        name(c"getSystemService"),
        method("(Ljava/lang/String;)Ljava/lang/Object;")?.method_signature(),
        &[JValue::Object(&service)],
    )?;
    manager.l()
}

/// The first item's text, if the clipboard holds any.
#[cfg(target_os = "android")]
fn read_text<'a>(env: &mut jni::Env<'a>, activity: &jni::objects::JObject<'a>) -> jni::errors::Result<Option<String>> {
    use jni::{JValue, objects::JString};

    let manager = manager(env, activity)?;
    let clip = env.call_method(
        &manager,
        name(c"getPrimaryClip"),
        method("()Landroid/content/ClipData;")?.method_signature(),
        &[],
    )?;
    if clip.is_null() {
        return Ok(None);
    }
    let clip = clip.l()?;
    let count = env
        .call_method(&clip, name(c"getItemCount"), method("()I")?.method_signature(), &[])?
        .i()?;
    if count <= 0 {
        return Ok(None);
    }
    let item = env
        .call_method(
            &clip,
            name(c"getItemAt"),
            method("(I)Landroid/content/ClipData$Item;")?.method_signature(),
            &[JValue::Int(0)],
        )?
        .l()?;
    let text = env.call_method(
        &item,
        name(c"getText"),
        method("()Ljava/lang/CharSequence;")?.method_signature(),
        &[],
    )?;
    if text.is_null() {
        return Ok(None);
    }
    let text = text.l()?;
    let string = env.call_method(
        &text,
        name(c"toString"),
        method("()Ljava/lang/String;")?.method_signature(),
        &[],
    )?;
    if string.is_null() {
        return Ok(None);
    }
    let string = string.l()?;
    // The reference is an owned local (`JValueOwned`), so handing it to the `JString` wrapper
    // transfers it rather than duplicating it; neither type deletes a local reference on drop.
    let string: JString<'_> = unsafe { JString::from_raw(env, string.into_raw()) };
    Ok(Some(string.try_to_string(env)?))
}

/// Replace the clipboard with one plain-text clip.
#[cfg(target_os = "android")]
fn write_text<'a>(env: &mut jni::Env<'a>, activity: &jni::objects::JObject<'a>, text: &str) -> jni::errors::Result<()> {
    use jni::JValue;

    let manager = manager(env, activity)?;
    let label = env.new_string("Craft")?;
    let body = env.new_string(text)?;
    let clip = env.call_static_method(
        class(c"android/content/ClipData"),
        name(c"newPlainText"),
        method("(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;")?
            .method_signature(),
        &[JValue::Object(&label), JValue::Object(&body)],
    )?;
    if clip.is_null() {
        return Ok(());
    }
    let clip = clip.l()?;
    env.call_method(
        &manager,
        name(c"setPrimaryClip"),
        method("(Landroid/content/ClipData;)V")?.method_signature(),
        &[JValue::Object(&clip)],
    )?;
    Ok(())
}

/// A class name as JNI wants it (`android/content/Intent`, not `android.intent.Intent`).
#[cfg(target_os = "android")]
fn class(name: &'static std::ffi::CStr) -> &'static jni::strings::JNIStr {
    jni::strings::JNIStr::from_cstr(name).expect("a literal class name")
}

/// A member name as JNI wants it. A C string literal cannot carry an interior NUL or invalid
/// MUTF-8, so this cannot fail on one.
#[cfg(target_os = "android")]
fn name(name: &'static std::ffi::CStr) -> &'static jni::strings::JNIStr {
    jni::strings::JNIStr::from_cstr(name).expect("a literal member name")
}

#[cfg(target_os = "android")]
fn method(signature: &str) -> jni::errors::Result<jni::signature::RuntimeMethodSignature> {
    jni::signature::RuntimeMethodSignature::from_str(signature)
}

#[cfg(target_os = "android")]
fn field(signature: &str) -> jni::errors::Result<jni::signature::RuntimeFieldSignature> {
    jni::signature::RuntimeFieldSignature::from_str(signature)
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key, Modifiers, RawInput};

    use super::{publish_with, shortcuts_with};

    fn key(key: Key, pressed: bool, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed, repeat: false, modifiers }
    }

    fn ctrl() -> Modifiers {
        Modifiers { ctrl: true, command: true, ..Default::default() }
    }

    fn run(events: Vec<Event>, clipboard: Option<&str>) -> Vec<Event> {
        let mut raw = RawInput { events, ..Default::default() };
        let clipboard = clipboard.map(str::to_owned);
        shortcuts_with(&mut raw, || clipboard.clone());
        raw.events
    }

    #[test]
    fn ctrl_c_becomes_a_copy_event_and_the_press_is_consumed() {
        let events = run(vec![key(Key::C, true, ctrl())], None);
        assert_eq!(events, vec![Event::Copy], "one copy, and no Key press left for the app to act on twice");
    }

    #[test]
    fn ctrl_x_becomes_a_cut_event() {
        assert_eq!(run(vec![key(Key::X, true, ctrl())], None), vec![Event::Cut]);
    }

    #[test]
    fn ctrl_v_pastes_the_system_clipboard() {
        let events = run(vec![key(Key::V, true, ctrl())], Some("from another app"));
        assert_eq!(events, vec![Event::Paste("from another app".into())]);
    }

    #[test]
    fn ctrl_v_with_an_empty_clipboard_does_paste_nothing() {
        assert!(run(vec![key(Key::V, true, ctrl())], None).is_empty());
        assert!(run(vec![key(Key::V, true, ctrl())], Some("")).is_empty(), "an empty clip is not a paste");
    }

    #[test]
    fn a_plain_v_is_typing_and_is_left_alone() {
        let events = vec![key(Key::V, true, Modifiers::default())];
        assert_eq!(run(events.clone(), Some("clipboard")), events);
    }

    #[test]
    fn shift_and_alt_variants_belong_to_the_app() {
        for modifiers in [
            Modifiers { ctrl: true, command: true, shift: true, ..Default::default() },
            Modifiers { ctrl: true, command: true, alt: true, ..Default::default() },
        ] {
            let events = vec![key(Key::C, true, modifiers)];
            assert_eq!(run(events.clone(), Some("clipboard")), events, "Copy Merged/Paste Special must reach the app");
        }
    }

    #[test]
    fn a_release_without_its_press_is_left_alone() {
        let events = vec![key(Key::V, false, ctrl())];
        assert_eq!(run(events.clone(), Some("clipboard")), events);
    }

    #[test]
    fn copied_text_reaches_the_system_clipboard() {
        let ctx = egui::Context::default();
        ctx.copy_text("selected words".to_owned());
        let mut written = Vec::new();
        publish_with(&ctx, |text| written.push(text.to_owned()));
        assert_eq!(written, vec!["selected words".to_owned()]);
    }

    #[test]
    fn an_empty_copy_is_not_written() {
        let ctx = egui::Context::default();
        ctx.copy_text(String::new());
        let mut wrote = false;
        publish_with(&ctx, |_| wrote = true);
        assert!(!wrote, "clearing a selection must not clear the system clipboard");
    }
}
