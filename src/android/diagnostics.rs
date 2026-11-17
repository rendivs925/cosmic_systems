//! Startup/crash reporting that does not depend on `logcat`.
//!
//! Android hides other applications' log buffers from Termux, so a launch
//! failure on the device leaves no readable trace. MediaStore's Downloads
//! collection is writable by the owning app without any runtime permission
//! (API 29+), and Termux can read it from `Download/`. Every launch writes one
//! `cosmic_startup_<epoch>.log` file describing how far startup got and, if the
//! process panics, the panic message.

use bevy_android::android_activity::AndroidApp;
use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Package id, emitted by `build.rs` from `cargo-apk2`'s environment.
const PACKAGE_ID: &str = env!("ANDROID_PACKAGE");

/// URI of this process's log document, created on first write.
static LOG_URI: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Install a panic hook that appends the panic message to the startup log
/// before the default hook aborts the process.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(&format!("PANIC: {info}"));
        previous(info);
    }));
}

/// Append one line to this launch's startup log. Never panics: diagnostics must
/// not become a second failure mode.
pub fn log(message: &str) {
    // The app's own `Android/media/<package>` directory is writable without a
    // runtime permission and readable by Termux, so try it first: it works even
    // if the JNI/MediaStore path is unavailable.
    if append_media_file(message) {
        eprintln!("android: {message}");
        return;
    }

    let Some(app) = bevy_android::ANDROID_APP.get() else {
        eprintln!("android: {message}");
        return;
    };
    if app.vm_as_ptr().is_null() {
        eprintln!("android: {message}");
        return;
    }
    // SAFETY: the pointer is the live JavaVM owned by `AndroidApp`; the wrapper
    // is only used while `app` is alive.
    let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }) else {
        eprintln!("android: {message}");
        return;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        eprintln!("android: {message}");
        return;
    };
    let _ = write_line(&mut env, app, message);
    eprintln!("android: {message}");
}

fn write_line(env: &mut JNIEnv, app: &AndroidApp, message: &str) -> jni::errors::Result<()> {
    let sdk = env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
        .i()?;
    if sdk < 29 {
        return Ok(());
    }

    let uri = match log_uri(env, app)? {
        Some(uri) => uri,
        None => return Ok(()),
    };
    // SAFETY: `activity_as_ptr` is a live global reference owned by `AndroidApp`.
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let resolver = env
        .call_method(
            &activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )?
        .l()?;

    let uri_string = env.new_string(uri)?;
    let uri_object = env
        .call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&uri_string)],
        )?
        .l()?;
    let mode = env.new_string("wa")?;
    let stream = env
        .call_method(
            &resolver,
            "openOutputStream",
            "(Landroid/net/Uri;Ljava/lang/String;)Ljava/io/OutputStream;",
            &[JValue::Object(&uri_object), JValue::Object(&mode)],
        )?
        .l()?;

    let line = format!("{message}\n");
    let bytes = env.byte_array_from_slice(line.as_bytes())?;
    env.call_method(&stream, "write", "([B)V", &[JValue::Object(&bytes)])?;
    env.call_method(&stream, "flush", "()V", &[])?;
    Ok(())
}

fn log_uri(env: &mut JNIEnv, app: &AndroidApp) -> jni::errors::Result<Option<String>> {
    let slot = LOG_URI.get_or_init(|| Mutex::new(None));
    if let Some(uri) = slot.lock().ok().and_then(|guard| guard.clone()) {
        return Ok(Some(uri));
    }

    // SAFETY: as above; the activity outlives this call.
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let resolver = env
        .call_method(
            &activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )?
        .l()?;

    let external = env.new_string("external")?;
    let collection = env
        .call_static_method(
            "android/provider/MediaStore$Downloads",
            "getContentUri",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&external)],
        )?
        .l()?;

    let values = env.new_object("android/content/ContentValues", "()V", &[])?;

    let name = env.new_string(file_name())?;
    put_string(env, &values, "display_name", &name)?;
    let mime = env.new_string("text/plain")?;
    put_string(env, &values, "mime_type", &mime)?;
    let relative = env.new_string("Download/")?;
    put_string(env, &values, "relative_path", &relative)?;

    let inserted = env
        .call_method(
            &resolver,
            "insert",
            "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
            &[JValue::Object(&collection), JValue::Object(&values)],
        )?
        .l()?;
    if inserted.is_null() {
        return Ok(None);
    }

    let text = env
        .call_method(&inserted, "toString", "()Ljava/lang/String;", &[])?
        .l()?;
    let text: String = env.get_string(&JString::from(text))?.into();
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(text.clone());
    }
    Ok(Some(text))
}

fn put_string(
    env: &mut JNIEnv,
    values: &JObject,
    key: &str,
    value: &JString,
) -> jni::errors::Result<()> {
    let key = env.new_string(key)?;
    env.call_method(
        values,
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&key), JValue::Object(value)],
    )?;
    Ok(())
}

fn file_name() -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("cosmic_startup_{epoch}.log")
}

/// Append to `Android/media/<package>/cosmic_startup.log`.
///
/// Returns `false` when the directory is not writable so the caller can fall
/// back to MediaStore.
fn append_media_file(message: &str) -> bool {
    let directory = media_directory();
    if std::fs::create_dir_all(&directory).is_err() {
        return false;
    }
    let path = directory.join("cosmic_startup.log");
    // Keep the file bounded across many launches.
    if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 128 * 1024) {
        let _ = std::fs::write(&path, b"");
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return false;
    };
    writeln!(file, "{message}").is_ok()
}

fn media_directory() -> PathBuf {
    PathBuf::from("/sdcard/Android/media").join(PACKAGE_ID)
}

/// Resident set size in MiB, for spotting texture/memory pressure on device.
pub fn memory_mb() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident_pages * 4 / 1024)
}

/// Read a string extra from the launching intent (`am start ... --es key value`).
pub fn intent_extra(key: &str) -> Option<String> {
    let app = bevy_android::ANDROID_APP.get()?;
    if app.vm_as_ptr().is_null() {
        return None;
    }
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let intent = env
        .call_method(&activity, "getIntent", "()Landroid/content/Intent;", &[])
        .ok()?
        .l()
        .ok()?;
    if intent.is_null() {
        return None;
    }
    let key = env.new_string(key).ok()?;
    let value = env
        .call_method(
            &intent,
            "getStringExtra",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&key)],
        )
        .ok()?
        .l()
        .ok()?;
    if value.is_null() {
        None
    } else {
        let text: String = env.get_string(&JString::from(value)).ok()?.into();
        Some(text)
    }
}
