use crate::EasyOfficeIpcRouter;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

const FFI_VERSION_CSTR: &[u8] = b"easyoffice-rust-ffi-1.0.0\0";

/// Returns a static NUL-terminated version string for the C-ABI bridge.
#[no_mangle]
pub extern "C" fn easyoffice_ffi_version() -> *const c_char {
    FFI_VERSION_CSTR.as_ptr() as *const c_char
}

/// Dispatches an IPC command over the C-ABI boundary, returning a heap-allocated NUL-terminated JSON string.
///
/// # Safety
/// `command_ptr` and `payload_ptr` must be either null (handled Fail-Closed) or valid pointers to NUL-terminated C strings.
/// The returned pointer must be freed via `easyoffice_ffi_free_string`.
#[no_mangle]
pub unsafe extern "C" fn easyoffice_ffi_dispatch_json(
    command_ptr: *const c_char,
    payload_ptr: *const c_char,
) -> *mut c_char {
    if command_ptr.is_null() {
        return into_raw_c_string(
            "{\"ok\":false,\"error\":\"null command pointer (Fail-Closed)\"}",
        );
    }

    let cmd_str = match CStr::from_ptr(command_ptr).to_str() {
        Ok(s) => s,
        Err(_) => {
            return into_raw_c_string(
                "{\"ok\":false,\"error\":\"invalid UTF-8 in command_ptr (Fail-Closed)\"}",
            );
        }
    };

    let payload_str = if payload_ptr.is_null() {
        ""
    } else {
        match CStr::from_ptr(payload_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                return into_raw_c_string(
                    "{\"ok\":false,\"error\":\"invalid UTF-8 in payload_ptr (Fail-Closed)\"}",
                );
            }
        }
    };

    match EasyOfficeIpcRouter::dispatch(cmd_str, payload_str) {
        Ok(json) => into_raw_c_string(&json),
        Err(err) => {
            let escaped = err.to_string().replace('\\', "\\\\").replace('"', "\\\"");
            into_raw_c_string(&format!("{{\"ok\":false,\"error\":\"{escaped}\"}}"))
        }
    }
}

/// Frees a C string previously allocated by `easyoffice_ffi_dispatch_json`.
///
/// # Safety
/// `ptr` must be either null or a pointer returned by `easyoffice_ffi_dispatch_json` that has not yet been freed.
#[no_mangle]
pub unsafe extern "C" fn easyoffice_ffi_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr);
    }
}

fn into_raw_c_string(text: &str) -> *mut c_char {
    let sanitized = text.replace('\0', " ");
    CString::new(sanitized)
        .unwrap_or_else(|_| CString::new("{\"ok\":false}").unwrap())
        .into_raw()
}

/// Safe helper exercising the C-ABI entrypoints (`easyoffice_ffi_dispatch_json` + `easyoffice_ffi_free_string`).
pub fn invoke_c_abi_roundtrip(command: &str, payload: &str) -> String {
    let c_cmd = CString::new(command).unwrap_or_default();
    let c_payload = CString::new(payload).unwrap_or_default();
    unsafe {
        let raw_out = easyoffice_ffi_dispatch_json(c_cmd.as_ptr(), c_payload.as_ptr());
        if raw_out.is_null() {
            return "{\"ok\":false}".to_string();
        }
        let result = CStr::from_ptr(raw_out).to_string_lossy().into_owned();
        easyoffice_ffi_free_string(raw_out);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_abi_ffi_version_dispatch_and_null_guards() {
        unsafe {
            let ver_ptr = easyoffice_ffi_version();
            assert!(!ver_ptr.is_null());
            let ver = CStr::from_ptr(ver_ptr).to_str().unwrap();
            assert_eq!(ver, "easyoffice-rust-ffi-1.0.0");

            let null_res_ptr = easyoffice_ffi_dispatch_json(std::ptr::null(), std::ptr::null());
            assert!(!null_res_ptr.is_null());
            let null_json = CStr::from_ptr(null_res_ptr).to_string_lossy().into_owned();
            easyoffice_ffi_free_string(null_res_ptr);
            assert!(null_json.contains("null command pointer"));

            easyoffice_ffi_free_string(std::ptr::null_mut());
        }

        let bootstrap_json = invoke_c_abi_roundtrip("get_workspace_bootstrap", "");
        assert!(bootstrap_json.contains("#5C6BC0"));
    }
}
