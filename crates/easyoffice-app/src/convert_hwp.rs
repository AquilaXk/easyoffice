//! HWP 5.0 targets of `convert_document_format` (PD-172).
//!
//! The engine's own `.hwp` container (`Hwp5Codec`) is not an HWP 5.0 file: it has no DocInfo
//! mappings, shapes or 22-byte paragraph headers, so a conversion never hands it out as `.hwp`.
//! The engine writes the document as HWPX, and the host (the desktop shell, through the Hangul
//! layout core) turns that into HWP, the way the editor saves. A host without that core
//! (`--serve`) refuses the target.

use easyoffice_formats::{DocumentFormat, EasyOfficeError, OutputCompatibility};

use crate::convert_structured::RichPdfRenderer;

/// Writes a document the engine wrote as HWPX out as HWP 5.0 bytes (PD-172).
pub type RichHwpWriter<'a> = dyn Fn(&[u8]) -> Result<Vec<u8>, String> + 'a;

/// What a host with the Hangul layout core adds to the engine's writers.
#[derive(Clone, Copy, Default)]
pub struct ShellWriters<'a> {
    /// Draws a document as PDF pages (PD-167).
    pub pdf: Option<&'a RichPdfRenderer<'a>>,
    /// Writes a document as HWP 5.0 (PD-172).
    pub hwp: Option<&'a RichHwpWriter<'a>>,
}

/// The HWP 5.0 bytes of `hwpx`, or the reason there are none: no writer on this host, or the
/// writer refused the document. Nothing falls back to the engine's container.
pub(crate) fn write_hwp(
    hwpx: &[u8],
    writer: Option<&RichHwpWriter<'_>>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let writer = writer.ok_or_else(|| {
        EasyOfficeError::InvalidArgument(
            "HWP 5.0 저장은 데스크톱 앱에서만 됩니다. HWPX로 저장하거나 데스크톱 앱에서 변환하세요."
                .to_string(),
        )
    })?;
    writer(hwpx).map_err(|reason| {
        EasyOfficeError::InvalidArgument(format!("HWP 5.0으로 저장하지 못했습니다: {reason}"))
    })
}

/// Compatibility and specification label of the bytes a conversion to `format` produced. `.hwp`
/// only gets here through [`write_hwp`], so it is the host's real HWP 5.0, not the engine's
/// container that [`DocumentFormat::output_compatibility`] describes.
pub(crate) fn written_compatibility(format: DocumentFormat) -> (OutputCompatibility, &'static str) {
    if format == DocumentFormat::Hwp {
        (
            OutputCompatibility::Standard,
            format.standard_specification(),
        )
    } else {
        (format.output_compatibility(), format.output_specification())
    }
}
