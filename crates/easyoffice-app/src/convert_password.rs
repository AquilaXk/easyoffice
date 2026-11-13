//! A conversion or save that would drop a password (PD-216). XLSX holds the agile digest and the legacy hash of a
//! sheet, a range or the workbook structure, ODS holds the key; every other format holds none. The engine
//! judges this for every conversion and save, and answers `{"refused":"passwordLoss"}` without writing a file
//! unless the request carries `acceptPasswordLoss: true`, which the page sends after the user agreed.

use crate::convert_structured::{field, invalid, StructuredContent};
use easyoffice_formats::{Credential, DocumentFormat, EasyOfficeError, StrictJsonValue};

/// The answer of a conversion that was refused for a password it would drop.
pub(crate) const REFUSED_PASSWORD_LOSS: &str = "{\"refused\":\"passwordLoss\"}";

const ACCEPT_KEY: &str = "acceptPasswordLoss";

/// Whether the request agrees to a password being dropped.
pub(crate) fn accepts_loss(entries: &[(String, StrictJsonValue)]) -> Result<bool, EasyOfficeError> {
    field(entries, ACCEPT_KEY).map_or(Ok(false), |value| {
        value
            .as_bool()
            .ok_or_else(|| invalid(format!("'{ACCEPT_KEY}'는 true/false여야 합니다.")))
    })
}

/// Whether writing `content` as `format` drops a password: a credential of a sheet, of an allowed range or
/// of the workbook that the format cannot hold. A protection without a password loses nothing worth asking.
pub(crate) fn password_lost(content: &StructuredContent, format: DocumentFormat) -> bool {
    let StructuredContent::Workbook(_, stylings) = content else {
        return false;
    };
    let holds = |credential: &Credential| match format {
        DocumentFormat::Xlsx => credential.fits_xlsx(),
        DocumentFormat::Ods => credential.fits_ods(),
        _ => credential.is_none(),
    };
    stylings.iter().any(|styling| {
        styling.protect.iter().any(|sheet| {
            !holds(&sheet.credential) || sheet.ranges.iter().any(|range| !holds(&range.credential))
        }) || styling
            .workbook_protect
            .iter()
            .any(|book| !holds(&book.credential))
    })
}
