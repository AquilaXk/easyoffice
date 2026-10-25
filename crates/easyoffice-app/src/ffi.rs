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
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
    }));

    match outcome {
        Ok(ptr) => ptr,
        Err(_) => into_raw_c_string(
            "{\"ok\":false,\"error\":\"internal panic caught at FFI boundary (Fail-Closed)\"}",
        ),
    }
}

/// Frees a C string previously allocated by `easyoffice_ffi_dispatch_json`.
///
/// # Safety
/// `ptr` must be either null or a pointer returned by `easyoffice_ffi_dispatch_json` that has not yet been freed.
#[no_mangle]
pub unsafe extern "C" fn easyoffice_ffi_free_string(ptr: *mut c_char) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !ptr.is_null() {
            let _ = CString::from_raw(ptr);
        }
    }));
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

    #[test]
    fn test_ipc_router_commands_and_self_test() {
        use crate::{run_self_test, EasyOfficeError, EasyOfficeIpcRouter};

        let bootstrap = EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "").unwrap();
        assert!(bootstrap.contains("#5C6BC0"));
        assert!(bootstrap.contains("\"ribbonMatrixSlots\":35"));

        let detect_res =
            EasyOfficeIpcRouter::dispatch("detect_format", "docx|HEX:504B03041400").unwrap();
        assert!(detect_res.contains("\"format\":\"docx\""));

        let sheet_res =
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", "A1=150;A2=250;A3==SUM(A1:A2)?A3")
                .unwrap();
        assert!(sheet_res.contains("\"value\":\"400\""));
        assert!(sheet_res.contains("\"conditionalMatch\":true"));

        let wb_res =
            EasyOfficeIpcRouter::dispatch("evaluate_workbook_formula", "=Sheet2!A1+50").unwrap();
        assert!(wb_res.contains("\"value\":\"150\""));

        let wb_dyn_res = EasyOfficeIpcRouter::dispatch(
            "evaluate_workbook_formula",
            "Sheet2!A1=300;Sheet1!A1=700?=Sheet1!A1+Sheet2!A1",
        )
        .unwrap();
        assert!(wb_dyn_res.contains("\"value\":\"1000\""));

        let bench_res = EasyOfficeIpcRouter::dispatch("run_engine_benchmark", "3").unwrap();
        assert!(bench_res.contains("\"allEquivalent\":true"));
        assert!(bench_res.contains("crc32_slicing_by_8"));

        let conv_res = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            "docx->hwpx|연간계획서|1. 추진 배경\n2. 실행 계획",
        )
        .unwrap();
        assert!(conv_res.contains("\"targetFormat\":\"hwpx\""));
        assert!(conv_res.contains("\"status\":\"OK\""));

        let cat_res =
            EasyOfficeIpcRouter::dispatch("get_ribbon_category_commands", "HWP|module").unwrap();
        assert!(cat_res.contains("hwp-mod-manuscript"));

        let layout_res = EasyOfficeIpcRouter::dispatch("compute_layout", "landscape").unwrap();
        assert!(layout_res.contains("\"widthPt\":842.0"));

        let sec_res = EasyOfficeIpcRouter::dispatch(
            "audit_security",
            "word/document.xml,512,2048,false|<doc>safe</doc>",
        )
        .unwrap();
        assert!(sec_res.contains("\"status\":\"SAFE\""));
        assert!(sec_res.contains("\"checkedEntries\":1"));

        let rib_res =
            EasyOfficeIpcRouter::dispatch("execute_ribbon_command", "doc-mod-toc").unwrap();
        assert!(rib_res.contains("\"catalogCount\":140"));
        assert!(rib_res.contains("표·개체 도구"));

        let style_res = EasyOfficeIpcRouter::dispatch("switch_ribbon_style", "hangul").unwrap();
        assert!(style_res.contains("쪽·원고지"));

        let smart_res = EasyOfficeIpcRouter::dispatch(
            "run_smart_assistant",
            "첫 문장입니다. TODO: 보안 결재를 완료합니다.",
        )
        .unwrap();
        assert!(smart_res.contains("\"actionItemCount\":1"));

        let pii_res = EasyOfficeIpcRouter::dispatch(
            "scrub_pii",
            "주민번호 880101-1234567 전화 010-9876-5432",
        )
        .unwrap();
        assert!(pii_res.contains("******-*******"));
        assert!(pii_res.contains("010-****-****"));

        assert!(matches!(
            EasyOfficeIpcRouter::dispatch(
                "audit_security",
                "../../etc/passwd,100,200,false|<doc>safe</doc>"
            ),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
        assert!(matches!(
            EasyOfficeIpcRouter::dispatch(
                "audit_security",
                "word/document.xml,10,50000,false|<doc>safe</doc>"
            ),
            Err(EasyOfficeError::DecompressionBombDetected { .. })
        ));

        let summary = run_self_test().unwrap();
        assert!(summary.contains("PASSED"));
        assert!(summary.contains("ipc_commands=13"));
    }
}
