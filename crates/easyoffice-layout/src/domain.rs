use crate::OfficeModuleKind;

/// Ribbon layout style supporting both global standard office and Korean HWP page/manuscript modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RibbonLayoutStyle {
    StandardOffice,
    HangulHwp,
}

impl RibbonLayoutStyle {
    pub fn category_titles(&self) -> [&'static str; 7] {
        match self {
            Self::StandardOffice => [
                "홈",
                "삽입",
                "레이아웃",
                "수식·데이터",
                "모듈 전용",
                "검토·보안",
                "보기·창",
            ],
            Self::HangulHwp => [
                "편집",
                "입력",
                "쪽·원고지",
                "표·개체",
                "시트·차트",
                "보안·도구",
                "보기",
            ],
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::StandardOffice => "표준 오피스",
            Self::HangulHwp => "한글(HWP) 쪽·편집",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            Self::StandardOffice => Self::HangulHwp,
            Self::HangulHwp => Self::StandardOffice,
        }
    }
}

/// Contextual object tool mode automatically activated by the active editor module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextualObjectToolKind {
    TableAndObject,
    PageApprovalAndManuscript,
    FormulaAndPivot,
    PresenterAndLaser,
    ApprovalAndAnnotation,
}

impl ContextualObjectToolKind {
    pub fn badge_title(&self) -> &'static str {
        match self {
            Self::TableAndObject => "표·개체 도구",
            Self::PageApprovalAndManuscript => "쪽·결재·원고지 도구",
            Self::FormulaAndPivot => "수식·피벗 분석",
            Self::PresenterAndLaser => "발표자·레이저 도구",
            Self::ApprovalAndAnnotation => "전자결재·주석 도구",
        }
    }

    pub fn for_module(module: OfficeModuleKind) -> Self {
        match module {
            OfficeModuleKind::Document => Self::TableAndObject,
            OfficeModuleKind::Hwp => Self::PageApprovalAndManuscript,
            OfficeModuleKind::Sheet => Self::FormulaAndPivot,
            OfficeModuleKind::Slide => Self::PresenterAndLaser,
            OfficeModuleKind::Pdf => Self::ApprovalAndAnnotation,
        }
    }
}
