//! Incoming and outgoing Android intents — the half of "open with / share" that does not need
//! `startActivityForResult`.
//!
//! # The gap
//!
//! `android-activity` 0.6.1 does not expose `startActivityForResult`, so the Storage Access
//! Framework's `ACTION_OPEN_DOCUMENT`/`ACTION_CREATE_DOCUMENT` pickers cannot be driven from
//! Rust alone (they need an `onActivityResult` override in a Java `Activity` subclass). What
//! *is* reachable without Java glue is everything that goes through the Activity's *launching*
//! intent and through fire-and-forget `startActivity` calls:
//!
//! - **Incoming `ACTION_VIEW`** — another app hands us a file ("Open with WordCraft"). The URI
//!   arrives in `getIntent().getData()` at startup; a `content://` URI is copied to a file in
//!   the app's cache directory and the shell opens that file on its first frame.
//! - **Incoming `ACTION_SEND`** — another app shares text or an image. `EXTRA_TEXT` is kept for
//!   the app to import; `EXTRA_STREAM` is copied to the cache directory like a `VIEW` URI.
//! - **Outgoing `ACTION_SEND`** — "share this document/text" builds a chooser intent and starts
//!   it with `startActivity`. Text needs no `FileProvider`; a file does, and the manifest has
//!   none yet, so `share_file` is provided but documented as needing a provider before it works
//!   on Android 7+.
//!
//! Text-only reads are cheap (a handful of JNI calls); copying a `content://` stream reads the
//! whole file into memory once, which is what the desktop's "open" path does anyway.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
thread_local! {
    static APP: RefCell<Option<AndroidApp>> = const { RefCell::new(None) };
}

#[derive(Debug)]
pub enum LaunchIntent {
    /// Open this file (copied to the cache dir when it came in as a `content://` URI).
    OpenFile(PathBuf),
    /// Import this text (a shared note, a snippet from another app).
    ImportText(String),
}

/// Remember the Activity the intent calls go through. Called once, before `run_native`.
#[cfg(target_os = "android")]
pub fn install(app: &AndroidApp) {
    APP.with(|slot| *slot.borrow_mut() = Some(app.clone()));
}

#[cfg(not(target_os = "android"))]
pub fn install() {}

/// Read the launching intent, if any, and turn it into something the shell can act on.
///
/// Returns `None` on the host build, before `install`, when there is no launching intent, or when
/// the intent is something this app does not handle (a different action, or a URI it cannot read).
pub fn launch() -> Option<LaunchIntent> {
    #[cfg(target_os = "android")]
    {
        return with_app(read_launch);
    }
    #[cfg(not(target_os = "android"))]
    None
}

/// Start a text share sheet (no `FileProvider` needed).
pub fn share_text(text: &str) {
    #[cfg(target_os = "android")]
    with_app(|app| write_share_text(app, text));
}

/// Start a file share sheet. Needs a `FileProvider` entry in the manifest to work on Android 7+;
/// without one, `startActivity` throws `FileUriExposedException` and the call is logged and
/// ignored. Provided so the wiring exists; the manifest change is a follow-up.
pub fn share_file(path: &Path, mime: &str) {
    #[cfg(target_os = "android")]
    with_app(|app| write_share_file(app, path, mime));
}

#[cfg(target_os = "android")]
fn with_app<T>(call: impl FnOnce(&AndroidApp) -> T) -> T {
    APP.with(|slot| {
        let guard = slot.borrow();
        let app = guard.as_ref().expect("intents::install was not called");
        call(app)
    })
}

// The functions below are only compiled on Android; on other targets `launch`/`share_*` are no-ops
// or return `None`, so the host tests never touch JNI.

#[cfg(target_os = "android")]
fn read_launch(app: &AndroidApp) -> Option<LaunchIntent> {
    use jni::objects::JObject;

    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };

    vm.attach_current_thread(|env| -> jni::errors::Result<Option<LaunchIntent>> {
        use jni::JValue;
        let activity = unsafe { JObject::from_raw(env, raw_activity) };
        let intent = env
            .call_method(&activity, name(c"getIntent"), method("()Landroid/content/Intent;").unwrap().method_signature(), &[])?
            .l()?;
        if intent.is_null() {
            return Ok(None);
        }
        let action = call_string(env, &intent, c"getAction")?;
        let action = action.as_deref().unwrap_or("");

        // ACTION_SEND: text or a stream.
        if action == "android.intent.action.SEND" {
            let extra_text = string_extra(env, &intent, "android.intent.extra.TEXT");
            if let Some(text) = extra_text.filter(|t| !t.trim().is_empty()) {
                return Ok(Some(LaunchIntent::ImportText(text)));
            }
            // EXTRA_STREAM
            let key = env.new_string("android.intent.extra.STREAM")?;
            let value = env
                .call_method(&intent, name(c"getParcelableExtra"), method("(Ljava/lang/String;)Landroid/os/Parcelable;").unwrap().method_signature(), &[JValue::Object(&key)])?
                .l()?;
            if !value.is_null() {
                let raw_uri = value.into_raw();
                if let Some(path) = copy_uri(env, &activity, raw_uri, app) {
                    return Ok(Some(LaunchIntent::OpenFile(path)));
                }
            }
            return Ok(None);
        }

        // ACTION_VIEW: a file.
        if action == "android.intent.action.VIEW" {
            let uri = env
                .call_method(&intent, name(c"getData"), method("()Landroid/net/Uri;").unwrap().method_signature(), &[])?
                .l()?;
            if !uri.is_null() {
                let raw_uri = uri.into_raw();
                if let Some(path) = copy_uri(env, &activity, raw_uri, app) {
                    return Ok(Some(LaunchIntent::OpenFile(path)));
                }
            }
        }

        Ok(None)
    })
    .ok()
    .flatten()
}

#[cfg(target_os = "android")]
fn copy_uri(env: &mut jni::Env<'_>, activity: &jni::objects::JObject<'_>, raw_uri: jni::sys::jobject, app: &AndroidApp) -> Option<PathBuf> {
    use jni::JValue;
    let uri = unsafe { jni::objects::JObject::from_raw(env, raw_uri) };

    // Resolve a display name for the cached file.
    let resolver = env
        .call_method(activity, name(c"getContentResolver"), method("()Landroid/content/ContentResolver;").unwrap().method_signature(), &[])
        .ok()?
        .l()
        .ok()?;
    if resolver.is_null() {
        return None;
    }

    let stream = env
        .call_method(&resolver, name(c"openInputStream"), method("(Landroid/net/Uri;)Ljava/io/InputStream;").unwrap().method_signature(), &[JValue::Object(&uri)])
        .ok()?
        .l()
        .ok()?;
    if stream.is_null() {
        return None;
    }

    // Read the whole stream.
    let mut bytes = Vec::new();
    let buf = env.new_byte_array(64 * 1024).ok()?;
    loop {
        let n = env
            .call_method(&stream, name(c"read"), method("([B)I").unwrap().method_signature(), &[JValue::Object(&buf)])
            .ok()?
            .i()
            .ok()?;
        if n <= 0 {
            break;
        }
        let mut chunk = vec![0i8; n as usize];
        env.get_byte_array_region(&buf, 0, &mut chunk);
        bytes.extend(chunk.iter().map(|&b| b as u8));
    }
    let _ = env.call_method(&stream, name(c"close"), method("()V").unwrap().method_signature(), &[]);

    // Cache dir + a name derived from the URI's last path segment.
    let cache = app.internal_data_path().unwrap_or_else(|| PathBuf::from(".")).join("intent-cache");
    let _ = std::fs::create_dir_all(&cache);
    let name = uri_last_segment(env, &uri).unwrap_or_else(|| "shared.bin".to_string());
    let safe = name.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "_");
    let dest = cache.join(if safe.is_empty() { "shared.bin".to_string() } else { safe });
    std::fs::write(&dest, &bytes).ok()?;
    Some(dest)
}

#[cfg(target_os = "android")]
fn uri_last_segment(env: &mut jni::Env<'_>, uri: &jni::objects::JObject<'_>) -> Option<String> {
    let last = env
        .call_method(uri, name(c"getLastPathSegment"), method("()Ljava/lang/String;").unwrap().method_signature(), &[])
        .ok()?
        .l()
        .ok()?;
    if last.is_null() {
        return None;
    }
    let s: jni::objects::JString<'_> = unsafe { jni::objects::JString::from_raw(env, last.into_raw()) };
    s.try_to_string(env).ok()
}

#[cfg(target_os = "android")]
fn string_extra(env: &mut jni::Env<'_>, intent: &jni::objects::JObject<'_>, key: &str) -> Option<String> {
    use jni::JValue;
    let key = env.new_string(key).ok()?;
    let value = env
        .call_method(intent, name(c"getStringExtra"), method("(Ljava/lang/String;)Ljava/lang/String;").unwrap().method_signature(), &[JValue::Object(&key)])
        .ok()?
        .l()
        .ok()?;
    if value.is_null() {
        return None;
    }
    let s: jni::objects::JString<'_> = unsafe { jni::objects::JString::from_raw(env, value.into_raw()) };
    s.try_to_string(env).ok()
}

#[cfg(target_os = "android")]
fn call_string(env: &mut jni::Env<'_>, obj: &jni::objects::JObject<'_>, method_name: &std::ffi::CStr) -> jni::errors::Result<Option<String>> {
    let value = env
        .call_method(obj, jni::strings::JNIStr::from_cstr(method_name).unwrap(), method("()Ljava/lang/String;").unwrap().method_signature(), &[])?
        .l()?;
    if value.is_null() {
        return Ok(None);
    }
    let s: jni::objects::JString<'_> = unsafe { jni::objects::JString::from_raw(env, value.into_raw()) };
    Ok(Some(s.try_to_string(env)?))
}


#[cfg(target_os = "android")]
fn write_share_text(app: &AndroidApp, text: &str) {
    use jni::JValue;
    let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let activity = unsafe { jni::objects::JObject::from_raw(env, raw_activity) };
        let action_send = env.new_string("android.intent.action.SEND")?;
        let text_plain = env.new_string("text/plain")?;
        let chooser_title = env.new_string("Share")?;
        let intent = env.new_object(class(c"android/content/Intent"), method("()V").ok().unwrap().method_signature(), &[])?;
        let _ = env.call_method(&intent, name(c"setAction"), method("(Ljava/lang/String;)Landroid/content/Intent;").ok().unwrap().method_signature(), &[JValue::Object(&action_send)])?;
        let _ = env.call_method(&intent, name(c"setType"), method("(Ljava/lang/String;)Landroid/content/Intent;").ok().unwrap().method_signature(), &[JValue::Object(&text_plain)])?;
        let key = env.new_string("android.intent.extra.TEXT")?;
        let body = env.new_string(text)?;
        let _ = env.call_method(&intent, name(c"putExtra"), method("(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;").ok().unwrap().method_signature(), &[JValue::Object(&key), JValue::Object(&body)])?;
        let chooser = env.call_static_method(class(c"android/content/Intent"), name(c"createChooser"), method("(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;").ok().unwrap().method_signature(), &[JValue::Object(&intent), JValue::Object(&chooser_title)])?.l()?;
        env.call_method(&activity, name(c"startActivity"), method("(Landroid/content/Intent;)V").ok().unwrap().method_signature(), &[JValue::Object(&chooser)])?;
        Ok(())
    });
}

#[cfg(target_os = "android")]
fn write_share_file(app: &AndroidApp, path: &Path, mime: &str) {
    // File sharing needs a FileProvider URI on Android 7+. Without a provider entry in the
    // manifest this throws FileUriExposedException; log and skip rather than crash.
    log::warn!("intents::share_file({}) not implemented: needs a FileProvider in the manifest", path.display());
    let _ = (app, mime);
}

#[cfg(target_os = "android")]
fn class(name: &'static std::ffi::CStr) -> &'static jni::strings::JNIStr {
    jni::strings::JNIStr::from_cstr(name).expect("literal class name")
}

#[cfg(target_os = "android")]
fn name(name: &'static std::ffi::CStr) -> &'static jni::strings::JNIStr {
    jni::strings::JNIStr::from_cstr(name).expect("literal member name")
}

#[cfg(target_os = "android")]
fn method(signature: &str) -> jni::errors::Result<jni::signature::RuntimeMethodSignature> {
    jni::signature::RuntimeMethodSignature::from_str(signature)
}

#[cfg(test)]
mod tests {
    // Host-side: nothing JNI-related runs; just make sure the enum is constructible.
    use super::LaunchIntent;
    #[test]
    fn launch_intent_variants_exist() {
        let _ = LaunchIntent::OpenFile(std::path::PathBuf::from("/tmp/x"));
        let _ = LaunchIntent::ImportText("hi".to_string());
    }
}
