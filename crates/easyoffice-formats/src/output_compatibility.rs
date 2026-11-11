//! Honest write-side compatibility reporting for encoded containers.

use crate::DocumentFormat;

/// Write-side compatibility of an encoded container (see [`DocumentFormat::output_compatibility`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputCompatibility {
    /// Output follows the governing specification's required parts.
    Standard,
    /// Output is readable by EasyOffice but omits required parts; carries a user-facing reason.
    /// Only the engine's own `.hwp`/`.hwt` container reports it.
    Limited(&'static str),
}

impl OutputCompatibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Limited(_) => "limited",
        }
    }

    pub fn warning(&self) -> Option<&'static str> {
        match self {
            Self::Standard => None,
            Self::Limited(reason) => Some(reason),
        }
    }
}

/// Why the engine's own HWP container is not reported as HWP 5.0 (PD-178).
const ENGINE_HWP_LIMITATION: &str =
    "엔진이 쓰는 HWP 구조에는 DocInfo의 ID 매핑·글자 모양·문단 모양과 22바이트 문단 머리가 없어 한글 프로그램이 열지 못할 수 있습니다. 데스크톱 앱의 HWP 저장(편집기 경로)을 쓰세요.";

impl DocumentFormat {
    /// Honest compatibility level of the bytes the engine itself *writes* for this format.
    ///
    /// The `.doc` and `.ppt` writers emit real MS-DOC / MS-PPT streams (`1Table` + piece table +
    /// FKPs; `Current User` + persist directory + user edit atom), so they report no limitation
    ///. `.hwp` and `.hwt` come from `Hwp5Codec`, a private container (UTF-16 strings in place
    /// of the HWP 5.0 records), so they report [`OutputCompatibility::Limited`]; the desktop shell
    /// writes real HWP through rhwp instead. A writer that regresses to a subset must return
    /// [`OutputCompatibility::Limited`] here again.
    pub fn output_compatibility(&self) -> OutputCompatibility {
        match self {
            Self::Hwp | Self::Hwt => OutputCompatibility::Limited(ENGINE_HWP_LIMITATION),
            _ => OutputCompatibility::Standard,
        }
    }

    /// Specification label for written output; a limited writer is labelled as such.
    pub fn output_specification(&self) -> &'static str {
        match self {
            Self::Hwp | Self::Hwt => "EasyOffice 단순 HWP 컨테이너 (HWP 5.0 비표준)",
            _ => self.standard_specification(),
        }
    }
}
