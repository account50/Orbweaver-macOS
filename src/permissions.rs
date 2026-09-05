use serde_json::{Value, json};

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOHIDCheckAccess(request_type: u32) -> u32;
    fn IOHIDRequestAccess(request_type: u32) -> bool;
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn geteuid() -> u32;
}

#[cfg(target_os = "macos")]
pub fn is_root() -> bool {
    // SAFETY: standard POSIX query; no arguments or ownership.
    unsafe { geteuid() == 0 }
}

/// SDK: IOKit/hidsystem/IOHIDLib.h. Check without prompting by default.
pub fn status(request: bool) -> Value {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: public IOKit functions take an enum value, no pointers or ownership.
        let result = unsafe {
            let requested = request.then(|| IOHIDRequestAccess(1));
            (IOHIDCheckAccess(1), IOHIDCheckAccess(0), requested)
        };
        let name = |n| match n {
            0 => "granted",
            1 => "denied",
            _ => "unknown",
        };
        json!({"input_monitoring":name(result.0),"post_event":name(result.1),"running_as_root":is_root(),"request_returned":result.2,"guidance":"Shared monitoring uses Input Monitoring. Exclusive keyboard capture additionally requires root on macOS. Check Karabiner/device ownership separately. Grant only permissions actually denied in System Settings > Privacy & Security, then restart the launching process."})
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        json!({"status":"not_applicable","guidance":"macOS permission checks are unavailable on this platform"})
    }
}
