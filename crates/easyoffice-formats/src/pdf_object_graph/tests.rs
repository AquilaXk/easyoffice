use super::*;

#[test]
fn hostile_pdf_indirect_object_bomb_and_cyclic_refs_fail_closed() {
    // 1. Cyclic indirect reference resolves to `None` within MAX_PDF_NESTING_DEPTH.
    let mut cyclic = HashMap::new();
    cyclic.insert(
        (1, 0),
        PdfIndirectObject {
            val: PdfObjValue::Ref(2, 0),
            stream_range: None,
            order_seq: 1,
        },
    );
    cyclic.insert(
        (2, 0),
        PdfIndirectObject {
            val: PdfObjValue::Ref(1, 0),
            stream_range: None,
            order_seq: 2,
        },
    );
    assert!(resolve_value(&PdfObjValue::Ref(1, 0), &cyclic).is_none());

    // 2. Hostile PDF declaring more than MAX_PDF_INDIRECT_OBJECTS (65,536) must fail closed.
    let mut pdf = Vec::with_capacity(65_600 * 18);
    pdf.extend_from_slice(b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n");
    pdf.extend_from_slice(b"2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n");
    pdf.extend_from_slice(b"3 0 obj << /Type /Page /Parent 2 0 R >> endobj\n");
    for id in 4..=65_540u32 {
        pdf.extend_from_slice(format!("{id} 0 obj 0 endobj\n").as_bytes());
    }
    pdf.extend_from_slice(b"trailer << /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");
    assert!(matches!(
        crate::PdfSerializer::parse(&pdf),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hostile_pdf_objstm_excess_entry_count_fails_closed() {
    let pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n",
        "2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n",
        "3 0 obj << /Type /Page /Parent 2 0 R >> endobj\n",
        "4 0 obj << /Type /ObjStm /N 4097 /First 0 /Length 0 >>\nstream\n\nendstream\nendobj\n",
        "trailer << /Root 1 0 R >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        crate::PdfSerializer::parse(pdf.as_bytes()),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
