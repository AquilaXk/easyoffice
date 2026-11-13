//! `evaluate_sheet` with `sessionAction` `protectHash` and `protectVerify` (PD-216): the page never hashes
//! or compares a password. It sends the typed text once, over the local command channel, and keeps only the
//! digest the answer carries.
//!
//! `protectHash` `{password, kind: "agile"|"odf"|"both"}` -> `{pwd: credential}` (`sheet_protect_json`), or
//! `{problem: "empty"|"long"}`. `protectVerify` `{pwd: credential|null, password}` -> `{ok: true|false}`, or
//! `{unsupported: "algorithm"|"spin"|"encoding"|"long"|"time"}` for a digest this program does not check
//! (an unknown algorithm, a `spinCount` over the bound, text that is not base64). The answers never carry
//! the password, and no error text quotes it.

use crate::sheet_protect_json::credential_json;
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{
    Credential, EasyOfficeError, PasswordCheck, StrictJsonValue, Unsupported,
};

type Entries = [(String, StrictJsonValue)];

fn password(entries: &Entries) -> Result<&str, EasyOfficeError> {
    EasyOfficeIpcRouter::optional_json_str(entries, "password")?.ok_or_else(|| {
        EasyOfficeError::InvalidArgument("JSON 'password' 필드가 필요합니다.".to_string())
    })
}

impl EasyOfficeIpcRouter {
    pub(crate) fn sheet_protect_hash(entries: &Entries) -> Result<String, EasyOfficeError> {
        let password = password(entries)?;
        let credential = match Self::optional_json_str(entries, "kind")? {
            Some("agile") => Credential::new_agile(password),
            Some("odf") => Credential::new_odf(password),
            Some("both") => Credential::new_both(password),
            _ => {
                return Err(EasyOfficeError::InvalidArgument(
                    "JSON 'kind'는 agile, odf, both 중 하나여야 합니다.".to_string(),
                ))
            }
        };
        Ok(match credential.as_ref().and_then(credential_json) {
            Some(pwd) => format!("{{\"pwd\":{pwd}}}"),
            None if password.is_empty() => "{\"problem\":\"empty\"}".to_string(),
            None => "{\"problem\":\"long\"}".to_string(),
        })
    }

    pub(crate) fn sheet_protect_verify(entries: &Entries) -> Result<String, EasyOfficeError> {
        let password = password(entries)?;
        let credential = crate::sheet_protect_json::credential_field(entries)?;
        Ok(match credential.verify(password) {
            PasswordCheck::Accepted => "{\"ok\":true}".to_string(),
            PasswordCheck::Rejected => "{\"ok\":false}".to_string(),
            PasswordCheck::Unsupported(why) => {
                let word = match why {
                    Unsupported::Algorithm => "algorithm",
                    Unsupported::SpinCount => "spin",
                    Unsupported::Encoding => "encoding",
                    Unsupported::TooLong => "long",
                    Unsupported::Time => "time",
                };
                format!("{{\"unsupported\":\"{word}\"}}")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::MAX_PASSWORD_UNITS;

    fn call(action: &str, body: &str) -> String {
        let payload = format!("{{\"sessionAction\":\"{action}\",{body}}}");
        EasyOfficeIpcRouter::dispatch("evaluate_sheet", &payload).unwrap()
    }

    fn pwd_of(answer: &str) -> String {
        let start = answer.find("\"pwd\":").unwrap() + 6;
        answer[start..answer.len() - 1].to_string()
    }

    #[test]
    fn a_password_is_hashed_and_checked_and_never_echoed() {
        for kind in ["agile", "odf", "both"] {
            let secret = "s3cret-비밀";
            let hashed = call(
                "protectHash",
                &format!("\"password\":\"{secret}\",\"kind\":\"{kind}\""),
            );
            assert!(!hashed.contains(secret), "{hashed}");
            let pwd = pwd_of(&hashed);
            let ok = call(
                "protectVerify",
                &format!("\"password\":\"{secret}\",\"pwd\":{pwd}"),
            );
            assert_eq!(ok, "{\"ok\":true}");
            let wrong = call(
                "protectVerify",
                &format!("\"password\":\"{secret}x\",\"pwd\":{pwd}"),
            );
            assert_eq!(wrong, "{\"ok\":false}");
            assert!(!wrong.contains(secret));
        }
        assert_eq!(
            call("protectVerify", "\"password\":\"anything\""),
            "{\"ok\":true}"
        );
    }

    #[test]
    fn empty_long_and_hostile_inputs_are_answered_without_the_password() {
        assert_eq!(
            call("protectHash", "\"password\":\"\",\"kind\":\"agile\""),
            "{\"problem\":\"empty\"}"
        );
        let long = "x".repeat(MAX_PASSWORD_UNITS + 1);
        assert_eq!(
            call(
                "protectHash",
                &format!("\"password\":\"{long}\",\"kind\":\"odf\"")
            ),
            "{\"problem\":\"long\"}"
        );
        let spin =
            r#"{"kind":"agile","alg":"SHA-512","hash":"AAAA","salt":"AAAA","spin":4294967295}"#;
        assert_eq!(
            call(
                "protectVerify",
                &format!("\"password\":\"x\",\"pwd\":{spin}")
            ),
            "{\"unsupported\":\"spin\"}"
        );
        let alg = r#"{"kind":"agile","alg":"MD2","hash":"AAAA","salt":"AAAA","spin":1}"#;
        assert_eq!(
            call(
                "protectVerify",
                &format!("\"password\":\"x\",\"pwd\":{alg}")
            ),
            "{\"unsupported\":\"algorithm\"}"
        );
        let b64 = r#"{"kind":"agile","alg":"SHA-512","hash":"!!","salt":"AAAA","spin":1}"#;
        assert_eq!(
            call(
                "protectVerify",
                &format!("\"password\":\"x\",\"pwd\":{b64}")
            ),
            "{\"unsupported\":\"encoding\"}"
        );
        assert_eq!(
            call(
                "protectVerify",
                &format!("\"password\":\"{long}\",\"pwd\":{spin}")
            ),
            "{\"unsupported\":\"long\"}"
        );
        let err = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            "{\"sessionAction\":\"protectHash\",\"password\":\"hunter2\",\"kind\":\"nope\"}",
        )
        .unwrap_err();
        assert!(!err.to_string().contains("hunter2"));
    }
}
