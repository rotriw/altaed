//! Small platform queries belong here rather than in views or persisted config.
pub(crate) mod terminal;

pub const fn code_font() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "monospace"
    }
}

pub fn data_directory() -> anyhow::Result<std::path::PathBuf> {
    use anyhow::Context;
    use std::path::PathBuf;
    if let Some(path) = std::env::var_os("ALTAED_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    {
        Ok(PathBuf::from(
            std::env::var_os("LOCALAPPDATA")
                .context("LOCALAPPDATA unavailable; set ALTAED_DATA_DIR")?,
        )
        .join("altaed"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(not(target_os = "macos"))]
        if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                return Ok(path.join("altaed"));
            }
        }
        let home = std::env::var_os("HOME").context("HOME unavailable; set ALTAED_DATA_DIR")?;
        let relative = if cfg!(target_os = "macos") {
            "Library/Application Support/altaed"
        } else {
            ".local/share/altaed"
        };
        Ok(PathBuf::from(home).join(relative))
    }
}
/// Read the OS accessibility preference without spawning a shell or depending
/// on a development machine path. Missing values preserve the standard motion.
#[cfg(target_os = "macos")]
pub fn reduce_motion() -> bool {
    use std::ffi::c_void;
    const UTF8_ENCODING: u32 = 0x0800_0100;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            value: *const std::ffi::c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFPreferencesGetAppBooleanValue(
            key: *const c_void,
            application: *const c_void,
            valid: *mut u8,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }
    // Both strings are owned until CFRelease; the preference function borrows
    // them and reports an absent key through `valid`.
    unsafe {
        let application = CFStringCreateWithCString(
            std::ptr::null(),
            c"com.apple.universalaccess".as_ptr(),
            UTF8_ENCODING,
        );
        let key =
            CFStringCreateWithCString(std::ptr::null(), c"reduceMotion".as_ptr(), UTF8_ENCODING);
        let mut valid = 0;
        let value = if !application.is_null() && !key.is_null() {
            CFPreferencesGetAppBooleanValue(key, application, &mut valid)
        } else {
            0
        };
        if !key.is_null() {
            CFRelease(key);
        }
        if !application.is_null() {
            CFRelease(application);
        }
        valid != 0 && value != 0
    }
}
#[cfg(not(target_os = "macos"))]
pub fn reduce_motion() -> bool {
    false
}
