//! Honest write-side compatibility reporting for encoded containers (#105).

use crate::DocumentFormat;

/// Write-side compatibility of an encoded container (see [`DocumentFormat::output_compatibility`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputCompatibility {
    /// Output follows the governing specification's required parts.
    Standard,
    /// Output is readable by EasyOffice but omits required parts; carries a user-facing reason.
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

impl DocumentFormat {
    /// Honest compatibility level of the bytes EasyOffice *writes* for this format.
    ///
    /// `.doc` and `.ppt` outputs are OLE2 containers carrying an EasyOffice-specific stream subset
    /// (no `1Table`/`0Table`, no `Current User`/persist directory), so other applications may refuse
    /// them. A spec-complete binary writer is tracked separately (#112).
    pub fn output_compatibility(&self) -> OutputCompatibility {
        match self {
            Self::Doc => OutputCompatibility::Limited(
                "DOC 출력은 OLE2 컨테이너에 WordDocument 스트림(FIB 스텁)만 기록하고 1Table/0Table 스트림이 없어 다른 워드프로세서에서 열리지 않을 수 있습니다. 호환 저장에는 DOCX를 사용하세요.",
            ),
            Self::Ppt => OutputCompatibility::Limited(
                "PPT 출력은 Current User 스트림과 persist directory가 없는 EasyOffice 전용 레코드 구조라 다른 프레젠테이션 프로그램에서 열리지 않을 수 있습니다. 호환 저장에는 PPTX를 사용하세요.",
            ),
            _ => OutputCompatibility::Standard,
        }
    }

    /// Specification label for written output; limited writers are labelled as such.
    pub fn output_specification(&self) -> &'static str {
        match self {
            Self::Doc => "OLE2 CFBF Word Binary 하위 집합 (호환성 제한)",
            Self::Ppt => "OLE2 CFBF Presentation Binary 하위 집합 (호환성 제한)",
            _ => self.standard_specification(),
        }
    }
}
