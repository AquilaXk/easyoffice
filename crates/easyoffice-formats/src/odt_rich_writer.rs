//! Writer for ODF Text (`.odt`) rich documents.
//!
//! 참조 구현 (reference implementation): ODF 1.3 Part 3 §3 (OpenDocument format).
//! Content, automatic styles, embedded pictures, lists and master page styles
//! are generated and packed into a standard ODF ZIP container.

use crate::rich_document::frame_paragraph;
use crate::rich_image::check_document_images;
use crate::rich_image_writers::{odt_image_paragraph, MediaParts, ODT_PICTURE_NAMESPACES};
use crate::rich_list_writers::{level_of, odt_list_styles_xml, plan_lists, OdtListWriter};
use crate::rich_page_frame::{odt_master_page, odt_styles_xml};
use crate::rich_run_writers::{odt_font_face_decls, odt_text_style_xml, OdtTextKey};
use crate::rich_table_writers::push_odt_table;
use crate::rich_toc_writers::{is_toc_block, odt_toc_styles_xml, TocScope};
use crate::rich_writers::{odt_para_style_xml, push_odt_paragraph, OdtParaKey, StyleTable};
use crate::{EasyOfficeError, OdfCodec, RichBlock, RichDocument, XmlSanitizer, ZipArchiveCodec};

const ODT_MEDIA_DIR: &str = "Pictures";

pub(crate) fn pack_odt_rich(doc: &RichDocument) -> Result<Vec<u8>, EasyOfficeError> {
    check_document_images(doc)?;
    let mut media = MediaParts::new();
    let mut para_styles: StyleTable<OdtParaKey> = StyleTable::new();
    let mut text_styles: StyleTable<OdtTextKey> = StyleTable::new();
    let mut body = String::with_capacity(1024);
    let lists = plan_lists(&doc.blocks);
    let mut open_lists = OdtListWriter::default();
    let mut toc_scope = TocScope::odt();
    // The header and footer go first so their styles are in the tables the body shares; styles.xml
    // and content.xml both write the whole set.
    let [header_xml, footer_xml] =
        [&doc.page_frame.header, &doc.page_frame.footer].map(|paragraphs| {
            (!paragraphs.is_empty()).then(|| {
                let mut xml = String::new();
                for p in paragraphs {
                    push_odt_paragraph(
                        &mut xml,
                        &frame_paragraph(p),
                        &mut para_styles,
                        &mut text_styles,
                    );
                }
                xml
            })
        });
    for (index, block) in doc.blocks.iter().enumerate() {
        toc_scope.enter(&mut body, &doc.blocks, index);
        match (block, lists.block_run[index]) {
            (RichBlock::Paragraph(p), Some(run)) => {
                open_lists.enter(&mut body, &lists, run, level_of(p));
                push_odt_paragraph(&mut body, p, &mut para_styles, &mut text_styles);
                continue;
            }
            _ => open_lists.leave(&mut body),
        }
        match block {
            RichBlock::Paragraph(p) => {
                push_odt_paragraph(&mut body, p, &mut para_styles, &mut text_styles);
            }
            RichBlock::Table(table) => push_odt_table(&mut body, table),
            RichBlock::Image(img) => {
                let picture = media.add(ODT_MEDIA_DIR, img);
                body.push_str(&odt_image_paragraph(img, picture.ordinal, &picture.path));
            }
        }
    }
    open_lists.leave(&mut body);
    toc_scope.enter(&mut body, &doc.blocks, doc.blocks.len());

    let mut run_styles = String::new();
    for (idx, key) in para_styles.keys.iter().enumerate() {
        run_styles.push_str(&odt_para_style_xml(idx + 1, key));
    }
    for (idx, key) in text_styles.keys.iter().enumerate() {
        run_styles.push_str(&odt_text_style_xml(idx + 1, key));
    }
    let mut styles = String::from("<office:automatic-styles>");
    if doc.blocks.iter().any(is_toc_block) {
        styles.push_str(&odt_toc_styles_xml());
    }
    styles.push_str(&run_styles);
    styles.push_str(&odt_list_styles_xml(&lists));
    styles.push_str("</office:automatic-styles>");
    let font_decls = odt_font_face_decls(&text_styles.keys);
    let styles = format!("{font_decls}{styles}");

    let content = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content ",
            "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
            "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
            "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
            "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" {}",
            "office:version=\"1.2\" title=\"{}\">{}<office:body><office:text>{}</office:text></office:body></office:document-content>"
        ),
        ODT_PICTURE_NAMESPACES,
        XmlSanitizer::escape(&doc.display_title()),
        styles,
        body
    );
    XmlSanitizer::validate_safe_xml(&content)?;
    let page = &doc.page_frame;
    let has_frame = header_xml.is_some()
        || footer_xml.is_some()
        || page.margins.is_some()
        || page.paper.is_some();
    let mut listing = media.listing();
    if has_frame {
        listing.insert(0, ("styles.xml".to_string(), "text/xml"));
    }
    let manifest = OdfCodec::build_manifest_with_media(OdfCodec::MIMETYPE_ODT, &listing);
    let mut entries = vec![
        (
            "mimetype".to_string(),
            OdfCodec::MIMETYPE_ODT.as_bytes().to_vec(),
        ),
        ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
        ("content.xml".to_string(), content.into_bytes()),
    ];
    if has_frame {
        let (layout, master) = odt_master_page(
            header_xml.as_deref(),
            footer_xml.as_deref(),
            page.margins.as_ref(),
            page.paper.as_ref(),
            page.binding,
        );
        let styles_xml = odt_styles_xml(&font_decls, &run_styles, &layout, &master);
        XmlSanitizer::validate_safe_xml(&styles_xml)?;
        entries.push(("styles.xml".to_string(), styles_xml.into_bytes()));
    }
    entries.extend(media.into_entries());
    ZipArchiveCodec::pack_compressed(&entries)
}
