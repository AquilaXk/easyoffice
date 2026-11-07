//! Honest write-side compatibility reporting for encoded containers (#105).

use crate::DocumentFormat;

/// Write-side compatibility of an encoded container (see [`DocumentFormat::output_compatibility`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputCompatibility {
    /// Output follows the governing specification's required parts.
    Standard,
    /// Output is readable by EasyOffice but omits required parts; carries a user-facing reason.
    /// Currently no writer returns this; the variant keeps the reporting path available.
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
    /// Every writer currently follows its governing specification. The `.doc` and `.ppt` writers
    /// emit real MS-DOC / MS-PPT streams (`1Table` + piece table + FKPs; `Current User` + persist
    /// directory + user edit atom), so they no longer report a limitation (#112). A writer that
    /// regresses to a subset must return [`OutputCompatibility::Limited`] here again.
    pub fn output_compatibility(&self) -> OutputCompatibility {
        OutputCompatibility::Standard
    }

    /// Specification label for written output; a limited writer would be labelled as such.
    pub fn output_specification(&self) -> &'static str {
        self.standard_specification()
    }
}
