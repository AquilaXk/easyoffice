//! PDFs for the tests of the edit pipeline (PD-163): a well-formed sample with a picture and a font
//! on every page, helpers to read results back with the reader, and hand-written hostile files.

use crate::pdf_edit_budget::Deadline;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hayro_syntax::object::dict::keys::{FILTER, HEIGHT, SUBTYPE, WIDTH};
use hayro_syntax::object::{Dict, Name, Object, Stream};
use hayro_syntax::Pdf as ReadPdf;
use hayro_write::pdf_writer::{Content, Filter, Finish, Name as WName, Pdf, Rect, Ref, Str};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

pub const PAGE_WIDTH: f32 = 200.0;
pub const PAGE_HEIGHT: f32 = 300.0;

pub fn deadline() -> Deadline {
    Deadline::after(Duration::from_secs(60))
}

/// The twelve picture bytes of page `n` (1-based): different for every page and every `tag`.
pub fn picture_bytes(tag: u8, n: usize) -> [u8; 12] {
    let mut bytes = [0u8; 12];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = tag
            .wrapping_mul(31)
            .wrapping_add(u8::try_from(n % 200).unwrap_or(0))
            .wrapping_add(u8::try_from(i).unwrap_or(0) * 7);
    }
    bytes
}

fn flate(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// A PDF of `pages` pages, each 200 x 300 points with its own 2 x 2 picture (`picture_bytes(tag, n)`)
/// and text in one font shared by all pages. `rotates[n]` is the `/Rotate` of page `n` (0 when
/// missing). The content of the odd pages is Flate-compressed, of the even pages it is plain, and
/// every third page has its content split over two streams.
pub fn sample_pdf(tag: u8, pages: usize, rotates: &[i32]) -> Vec<u8> {
    let mut next = Ref::new(1);
    let (catalog, tree, font) = (next.bump(), next.bump(), next.bump());
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.type1_font(font).base_font(WName(b"Helvetica"));
    let mut kids = Vec::new();
    for n in 1..=pages {
        let (page_ref, image_ref) = (next.bump(), next.bump());
        kids.push(page_ref);
        let picture = picture_bytes(tag, n);
        let mut image = pdf.image_xobject(image_ref, &picture);
        image.width(2).height(2).bits_per_component(8);
        image.color_space().device_rgb();
        image.finish();

        let mut text = Content::new();
        text.begin_text()
            .set_font(WName(b"F1"), 18.0)
            .next_line(20.0, 100.0)
            .show(Str(format!("Page {n} of {tag}").as_bytes()))
            .end_text();
        let mut art = Content::new();
        art.save_state()
            .transform([100.0, 0.0, 0.0, 100.0, 50.0, 150.0])
            .x_object(WName(b"Im1"))
            .restore_state();
        let (text, art) = (text.finish(), art.finish());

        let contents: Vec<Ref> = if n % 3 == 0 {
            let (a, b) = (next.bump(), next.bump());
            pdf.stream(a, &text);
            pdf.stream(b, &art);
            vec![a, b]
        } else {
            let whole = next.bump();
            let joined = [text.as_slice(), b"\n", art.as_slice()].concat();
            if n % 2 == 1 {
                pdf.stream(whole, &flate(&joined))
                    .filter(Filter::FlateDecode);
            } else {
                pdf.stream(whole, &joined);
            }
            vec![whole]
        };
        let mut page = pdf.page(page_ref);
        page.media_box(Rect::new(0.0, 0.0, PAGE_WIDTH, PAGE_HEIGHT))
            .parent(tree);
        if let Some(&rotate) = rotates.get(n - 1) {
            page.rotate(rotate);
        }
        if let [only] = contents[..] {
            page.contents(only);
        } else {
            page.insert(WName(b"Contents"))
                .array()
                .items(contents.iter().copied());
        }
        let mut resources = page.resources();
        resources.x_objects().pair(WName(b"Im1"), image_ref);
        resources.fonts().pair(WName(b"F1"), font);
        resources.finish();
        page.finish();
    }
    pdf.pages(tree)
        .count(i32::try_from(pages).unwrap())
        .kids(kids);
    pdf.finish()
}

/// A PDF whose pages are heavy like a scanned or illustrated report: `content_bytes` of
/// Flate-compressed drawing operators and one `image_bytes` picture per page (incompressible bytes,
/// stored as they are), all pages sharing one font. For timing, not for assertions on content.
pub fn heavy_pdf(pages: usize, content_bytes: usize, image_bytes: usize) -> Vec<u8> {
    let mut next = Ref::new(1);
    let (catalog, tree, font) = (next.bump(), next.bump(), next.bump());
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.type1_font(font).base_font(WName(b"Helvetica"));
    let line = b"BT /F1 9 Tf 36 700 Td (The quick brown fox jumps over the lazy dog 0123456789) Tj ET\n0.5 w 36 690 m 560 690 l S\n";
    let content = line.repeat(content_bytes / line.len() + 1);
    let content = flate(&content);
    let mut seed = 0x9e37_79b9_u32;
    let image: Vec<u8> = (0..image_bytes)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        })
        .collect();
    let mut kids = Vec::new();
    for _ in 0..pages {
        let (page_ref, image_ref, content_ref) = (next.bump(), next.bump(), next.bump());
        kids.push(page_ref);
        let side = i32::try_from((image_bytes / 3).isqrt()).unwrap();
        let mut picture = pdf.image_xobject(image_ref, &image);
        picture.width(side).height(side).bits_per_component(8);
        picture.color_space().device_rgb();
        picture.finish();
        pdf.stream(content_ref, &content)
            .filter(Filter::FlateDecode);
        let mut page = pdf.page(page_ref);
        page.media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
            .parent(tree)
            .contents(content_ref);
        let mut resources = page.resources();
        resources.x_objects().pair(WName(b"Im1"), image_ref);
        resources.fonts().pair(WName(b"F1"), font);
        resources.finish();
        page.finish();
    }
    pdf.pages(tree)
        .count(i32::try_from(pages).unwrap())
        .kids(kids);
    pdf.finish()
}

pub fn read(bytes: &[u8]) -> ReadPdf {
    ReadPdf::new(Arc::new(bytes.to_vec())).expect("the result is a PDF the reader opens")
}

/// The picture bytes used on each page of a result, found through the forms the page draws (a
/// result is a page that draws a copy of the original page; an edited result of an edited result
/// has one more level of forms).
pub fn pictures_by_page(bytes: &[u8]) -> Vec<Vec<u8>> {
    use hayro_syntax::object::dict::keys::RESOURCES;
    let pdf = read(bytes);
    pdf.pages()
        .iter()
        .map(|page| {
            let mut found = Vec::new();
            if let Some(resources) = page.raw().get::<Dict<'_>>(RESOURCES) {
                collect_pictures(&resources, &mut found, 0);
            }
            found
        })
        .collect()
}

fn collect_pictures(resources: &Dict<'_>, found: &mut Vec<u8>, depth: usize) {
    use hayro_syntax::object::dict::keys::{RESOURCES, XOBJECT};
    assert!(depth < 8, "forms nest too deep");
    let Some(xobjects) = resources.get::<Dict<'_>>(XOBJECT) else {
        return;
    };
    for key in xobjects.keys() {
        let Some(stream) = xobjects.get::<Stream<'_>>(&*key) else {
            continue;
        };
        if is_small_image(&stream) {
            found.extend_from_slice(&stream.raw_data());
        } else if let Some(inner) = stream.dict().get::<Dict<'_>>(RESOURCES) {
            collect_pictures(&inner, found, depth + 1);
        }
    }
}

fn is_small_image(image: &hayro_syntax::object::Stream<'_>) -> bool {
    image
        .dict()
        .get::<Name<'_>>(SUBTYPE)
        .is_some_and(|n| n.as_str() == "Image")
        && image.dict().get::<i32>(WIDTH) == Some(2)
        && image.dict().get::<i32>(HEIGHT) == Some(2)
        && image.filters().is_empty()
        && image.dict().get::<Name<'_>>(FILTER).is_none()
}

/// Every stream object of the file that is an image of `picture_bytes`' kind, whichever page uses
/// it (a cross-check of `pictures_by_page`).
pub fn all_pictures(bytes: &[u8]) -> Vec<Vec<u8>> {
    let pdf = read(bytes);
    let mut out = Vec::new();
    for object in pdf.objects() {
        if let Object::Stream(stream) = object {
            if is_small_image(&stream) {
                out.push(stream.raw_data().to_vec());
            }
        }
    }
    out.sort();
    out
}

// ---- hostile files (hand-written, no cross-reference table: the reader has to repair them) ------

/// A file of numbered objects, a catalog at 1 and a trailer that names it. There is no `xref`
/// table, so the reader scans for the objects (its repair path).
pub fn hand_written(objects: &[(u32, String)], trailer_extra: &str) -> Vec<u8> {
    let mut out = String::from("%PDF-1.7\n");
    for (number, body) in objects {
        out.push_str(&format!("{number} 0 obj\n{body}\nendobj\n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Root 1 0 R {trailer_extra} >>\n%%EOF\n"
    ));
    out.into_bytes()
}

/// One valid page object of 100 x 100 points that has no content.
pub fn blank_page(parent: u32) -> String {
    format!("<< /Type /Page /Parent {parent} 0 R /MediaBox [0 0 100 100] >>")
}
