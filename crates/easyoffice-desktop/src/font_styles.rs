//! The style variants of a font family, for the Pages sidebar's style popup (PD-368).
//!
//! The popup under the font name lists the variants of the family the selection is in (the reference
//! word processor of the platform calls them `가는체`, `폭좁은체`, ...). This module answers one question: which
//! faces belong to the font `name`, and what does each one call itself.
//!
//! * **Source**: the font database the PDF export already builds once per process (`fontdb`, which scans the
//!   system and per-user folders on macOS, Windows and Linux), and `ttf-parser`, which `fontdb` is built on,
//!   for the name table. No new dependency and nothing is scanned a second time (the 3-step choice of PD-368:
//!   no crate was needed beyond the two the shell has).
//! * **Grouping**: `fontdb` names a face by its typographic family (name ID 16) and falls back to the font
//!   family (ID 1). A condensed face that shares the typographic family of its siblings is one of their
//!   variants; one that has its own family stays a family of its own.
//! * **Style names** come from the font itself (the typographic subfamily, ID 17, else the subfamily, ID 2),
//!   in Korean when the font has a Korean record, else in English as the font spells it. There is no
//!   translation table; a face with no subfamily record uses the part of its PostScript name after the
//!   hyphen, else `Regular`.
//! * **Bounds**: a request name over [`MAX_NAME_BYTES`] bytes or with a control character answers nothing; the
//!   scan stops after [`MAX_FACES_SCANNED`] faces; a family lists at most [`MAX_STYLES_PER_FAMILY`] variants;
//!   a variant whose names are longer than the document model keeps is left out (fail closed).

use crate::shell_caller::ShellCaller;
use crate::{pdf_export, IpcError};
use serde::Serialize;
use tauri::AppHandle;
use ttf_parser::{name_id, Face};
use usvg::fontdb::{Database, FaceInfo, Stretch, Style};

/// Most variants one family lists.
pub const MAX_STYLES_PER_FAMILY: usize = 64;
/// Most faces one request looks at (the font catalog of the Hangul editor accepts the same number).
pub const MAX_FACES_SCANNED: usize = 25_000;
/// Longest request name, style name and PostScript name, in bytes (`easyoffice_formats::MAX_FONT_FACE_BYTES`).
pub const MAX_NAME_BYTES: usize = 128;
/// Longest full name in characters: the font-name limit of the editor and the document model.
const MAX_FULL_NAME_CHARS: usize = 64;
/// CSS `font-stretch` percentage of each [`Stretch`], ultra-condensed to ultra-expanded.
const STRETCH_PERCENT: [u16; 9] = [50, 62, 75, 87, 100, 112, 125, 150, 200];
const NORMAL_WIDTH: u16 = 100;
const FALLBACK_STYLE: &str = "Regular";

/// One variant of a family.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FontStyle {
    /// PostScript name: what a `.pages` file keeps (`RichRun::font_face`).
    pub face: String,
    /// Full name (`Apple SD Gothic Neo Light`): the font name the word-processor formats write.
    pub full_name: String,
    /// The name the popup shows.
    pub style: String,
    /// CSS weight, 1 to 1000.
    pub weight: u16,
    /// CSS `font-stretch` percentage (100 is normal, 75 condensed).
    pub width: u16,
    pub italic: bool,
}

/// The family a name resolves to and its variants; empty when the machine has no such font.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct FontStyleGroup {
    pub family: String,
    pub styles: Vec<FontStyle>,
}

/// The language of a name record, as far as the style name cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Locale {
    Korean,
    English,
    Other,
}

/// A Unicode name record of a font's name table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NameRecord {
    pub id: u16,
    pub locale: Locale,
    pub text: String,
}

/// The first record of the first of `ids` (in that order) that has a locale of `prefer` (in that order).
pub(crate) fn pick_name(records: &[NameRecord], ids: &[u16], prefer: &[Locale]) -> Option<String> {
    ids.iter().find_map(|&id| {
        prefer.iter().find_map(|&locale| {
            records
                .iter()
                .find(|r| r.id == id && r.locale == locale)
                .map(|r| r.text.clone())
        })
    })
}

fn is_clean(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= MAX_NAME_BYTES && !text.chars().any(char::is_control)
}

/// The text of a name record: a Unicode record, or a Macintosh Roman one of plain ASCII (the English names of the
/// fonts a Mac ships are stored only that way; the rest of the Roman table is not decoded).
fn record_text(name: &ttf_parser::name::Name) -> Option<String> {
    name.to_string().or_else(|| {
        (name.platform_id == ttf_parser::PlatformId::Macintosh && name.name.is_ascii())
            .then(|| String::from_utf8_lossy(name.name).into_owned())
    })
}

/// The name records the popup reads (subfamily, full name) of one face; a name that is empty, too long or has a
/// control character is not read.
fn read_records(data: &[u8], index: u32) -> Vec<NameRecord> {
    let Ok(face) = Face::parse(data, index) else {
        return Vec::new();
    };
    face.names()
        .into_iter()
        .filter(|n| {
            matches!(
                n.name_id,
                name_id::SUBFAMILY | name_id::TYPOGRAPHIC_SUBFAMILY | name_id::FULL_NAME
            )
        })
        .filter_map(|n| {
            let text = record_text(&n)?.trim().to_string();
            let locale = match n.language().primary_language() {
                "Korean" => Locale::Korean,
                "English" => Locale::English,
                _ => Locale::Other,
            };
            is_clean(&text).then_some(NameRecord {
                id: n.name_id,
                locale,
                text,
            })
        })
        .collect()
}

fn width_percent(stretch: Stretch) -> u16 {
    STRETCH_PERCENT[match stretch {
        Stretch::UltraCondensed => 0,
        Stretch::ExtraCondensed => 1,
        Stretch::Condensed => 2,
        Stretch::SemiCondensed => 3,
        Stretch::Normal => 4,
        Stretch::SemiExpanded => 5,
        Stretch::Expanded => 6,
        Stretch::ExtraExpanded => 7,
        Stretch::UltraExpanded => 8,
    }]
}

/// The style name when the font has no subfamily record: the part of the PostScript name after the hyphen.
fn style_from_postscript(post_script: &str) -> &str {
    post_script
        .rsplit_once('-')
        .map(|(_, style)| style)
        .filter(|style| is_clean(style))
        .unwrap_or(FALLBACK_STYLE)
}

/// The variant of one face, or `None` when a name is longer than the document keeps.
fn variant_of(db: &Database, info: &FaceInfo, family: &str) -> Option<FontStyle> {
    if !is_clean(&info.post_script_name) {
        return None;
    }
    let records = db
        .with_face_data(info.id, |data, index| read_records(data, index))
        .unwrap_or_default();
    let style = pick_name(
        &records,
        &[name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY],
        &[Locale::Korean, Locale::English, Locale::Other],
    )
    .unwrap_or_else(|| style_from_postscript(&info.post_script_name).to_string());
    let full_name = pick_name(
        &records,
        &[name_id::FULL_NAME],
        &[Locale::English, Locale::Korean, Locale::Other],
    )
    .unwrap_or_else(|| {
        if style == FALLBACK_STYLE {
            family.to_string()
        } else {
            format!("{family} {style}")
        }
    });
    (full_name.chars().count() <= MAX_FULL_NAME_CHARS).then(|| FontStyle {
        face: info.post_script_name.clone(),
        full_name,
        style,
        weight: info.weight.0.clamp(1, 1000),
        width: width_percent(info.stretch),
        italic: info.style != Style::Normal,
    })
}

/// Order of the popup: normal width first, then narrower and wider ones; light to heavy; upright before italic. One entry
/// per PostScript name, at most [`MAX_STYLES_PER_FAMILY`].
fn arranged(mut styles: Vec<FontStyle>) -> Vec<FontStyle> {
    styles.sort_by(|a, b| {
        let width = |s: &FontStyle| (s.width != NORMAL_WIDTH, s.width);
        (width(a), a.weight, a.italic, &a.face).cmp(&(width(b), b.weight, b.italic, &b.face))
    });
    styles.dedup_by(|b, a| a.face == b.face);
    styles.truncate(MAX_STYLES_PER_FAMILY);
    styles
}

/// The family of `name`: a family name (English or localized), else a PostScript name, else the family whose
/// name is the start of a full name (`Apple SD Gothic Neo Light`). The longest such family wins.
fn family_of(db: &Database, name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let faces = || db.faces().take(MAX_FACES_SCANNED);
    let by_family = faces().find(|f| f.families.iter().any(|(n, _)| n.to_lowercase() == lower));
    let by_postscript = || faces().find(|f| f.post_script_name.to_lowercase() == lower);
    if let Some(face) = by_family.or_else(by_postscript) {
        return face.families.first().map(|(n, _)| n.clone());
    }
    faces()
        .flat_map(|f| f.families.iter().map(|(n, _)| n))
        .filter(|n| {
            lower
                .strip_prefix(&n.to_lowercase())
                .is_some_and(|rest| rest.starts_with(' '))
        })
        .max_by_key(|n| n.len())
        .cloned()
}

/// The variants of the family `name` belongs to. Empty for a name the machine does not have, an oversized
/// name or one with a control character.
pub fn styles_of(db: &Database, name: &str) -> FontStyleGroup {
    let name = name.trim();
    if !is_clean(name) {
        return FontStyleGroup::default();
    }
    let Some(family) = family_of(db, name) else {
        return FontStyleGroup::default();
    };
    let styles = db
        .faces()
        .take(MAX_FACES_SCANNED)
        .filter(|f| f.families.first().is_some_and(|(n, _)| *n == family))
        .filter_map(|f| variant_of(db, f, &family))
        .collect();
    FontStyleGroup {
        family,
        styles: arranged(styles),
    }
}

/// The variants of the family of `name` (a family, localized family, PostScript or full name). Names only, no path.
#[tauri::command]
pub async fn list_font_styles(
    _caller: ShellCaller,
    app: AppHandle,
    name: String,
) -> Result<FontStyleGroup, IpcError> {
    let dirs = crate::pdf_job::pdf_font_dirs(&app);
    tauri::async_runtime::spawn_blocking(move || {
        styles_of(&pdf_export::shared_font_db(&dirs), &name)
    })
    .await
    .map_err(|err| IpcError::new(format!("글꼴 스타일을 불러오지 못했습니다: {err}")))
}

#[cfg(test)]
#[path = "font_styles_tests.rs"]
mod tests;
