//! The first page of a PDF as a card picture, drawn inside the PDF worker process (PD-179).
//!
//! The start window's card shows the top of the first page. The file goes through the same gate as
//! an edit (`pdf_edit_inspect::open_within`: size, stream and page budgets, no encryption), then
//! `hayro` interprets and rasterizes page one into a pixmap of [`CARD_WIDTH_PX`] x
//! [`CARD_HEIGHT_PX`] that is white outside the page. The page is scaled to the card's width and
//! its top is kept, which is what the card's picture area shows of a tall page (`object-fit: cover`
//! from the top); a wide page is drawn whole and the rest stays white. So the pixmap, its PNG and
//! the renderer's allocations for the page do not grow with the size the page declares. Pictures
//! inside the page are decoded at their own size, which is what the worker's memory limit is for.

use crate::pdf_edit_budget::Deadline;
use crate::pdf_edit_inspect::{open_within, Limits};
use hayro::hayro_interpret::font::{FontData, FontQuery};
use hayro::hayro_interpret::hayro_cmap::CidFamily;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{render_into, RenderCache, RenderSettings};
use std::sync::Arc;

/// Width of the card picture in pixels: the card's picture area is about 270 CSS pixels wide.
pub const CARD_WIDTH_PX: u16 = 480;
/// Height of the card picture: the 4:3 of the card.
pub const CARD_HEIGHT_PX: u16 = 360;
/// Smallest side of a page in points that is drawn (a degenerate page has nothing to show and
/// would need an enormous scale).
const MIN_PAGE_SIDE_PT: f32 = 1.0;
/// Largest scale from points to pixels. Pages narrower than `CARD_WIDTH_PX / MAX_SCALE` points
/// are refused.
const MAX_SCALE: f32 = 64.0;

const MSG_NO_PAGE: &str = "첫 쪽을 그릴 수 없습니다.";

/// How text is drawn: the 14 standard fonts and the other Latin text with the substitutes `hayro`
/// carries, and the text of a CJK font the file does not embed (the shell's own text PDFs name
/// `HYGoThic-Medium`, and so do others) with `cjk_font`, the Korean font the shell sends. Without
/// it that text stays blank.
fn interpreter_settings(cjk_font: Option<Vec<u8>>) -> InterpreterSettings {
    let cjk_font: Option<FontData> = cjk_font.map(|bytes| Arc::new(bytes) as FontData);
    let mut settings = InterpreterSettings::default();
    let stock = Arc::clone(&settings.font_resolver);
    settings.font_resolver = Arc::new(move |query| {
        if let (FontQuery::Fallback(fallback), Some(font)) = (query, &cjk_font) {
            let cjk = fallback.character_collection.as_ref().is_some_and(|c| {
                matches!(
                    c.family,
                    CidFamily::AdobeKorea1
                        | CidFamily::AdobeJapan1
                        | CidFamily::AdobeGB1
                        | CidFamily::AdobeCNS1
                )
            });
            if cjk {
                return Some((Arc::clone(font), 0));
            }
        }
        stock(query)
    });
    settings
}

/// The PNG of the first page of `pdf`.
pub fn first_page_png(
    pdf: &Arc<Vec<u8>>,
    cjk_font: Option<Vec<u8>>,
    deadline: &Deadline,
) -> Result<Vec<u8>, String> {
    let document = open_within(pdf, deadline, &Limits::DEFAULT)?;
    let page = document
        .pages()
        .first()
        .ok_or_else(|| MSG_NO_PAGE.to_string())?;
    let (width, height) = page.render_dimensions();
    if !(width.is_finite() && height.is_finite())
        || width < MIN_PAGE_SIDE_PT
        || height < MIN_PAGE_SIDE_PT
    {
        return Err(MSG_NO_PAGE.to_string());
    }
    let scale = f32::from(CARD_WIDTH_PX) / width;
    if scale > MAX_SCALE {
        return Err(MSG_NO_PAGE.to_string());
    }
    deadline.check()?;
    let transform = Affine::scale(f64::from(scale)) * page.initial_transform(true).to_kurbo();
    let mut context = RenderContext::new(CARD_WIDTH_PX, CARD_HEIGHT_PX);
    render_into(
        page,
        &RenderCache::new(),
        &interpreter_settings(cjk_font),
        &RenderSettings::default(),
        &mut context,
        transform,
    );
    context.flush();
    deadline.check()?;
    let mut pixmap = Pixmap::new(CARD_WIDTH_PX, CARD_HEIGHT_PX);
    context.render_with(
        &mut pixmap,
        &mut Resources::default(),
        RasterizerSettings {
            target_init: TargetInit::Clear(WHITE),
            ..Default::default()
        },
    );
    pixmap
        .into_png()
        .map_err(|_| "미리보기 그림을 PNG로 만들지 못했습니다.".to_string())
}
