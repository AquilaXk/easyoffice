mod pages_fixture;

use easyoffice_formats::pages_package::{load_package, load_package_with_policy};
use easyoffice_formats::{
    DecodedContent, DocumentFormat, EasyOfficeError, Ole2ContainerCodec, RichBlock,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};
use pages_fixture::*;

const PNG_1X1: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE,
    0x42, 0x60, 0x82,
];

fn set_u16_le(buf: &mut [u8], at: usize, value: u16) {
    buf[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn build_split_index_iwa() -> (Vec<u8>, Vec<u8>) {
    let mut doc_builder = DocBuilder::default();
    let table_info_id = doc_builder.reserve_id();
    let att_table = doc_builder.attachment(table_info_id);
    // PackageMetadata (11006): data_infos (4) { identifier = 1, file_name = 4 }.
    doc_builder.add(
        11006,
        &f_bytes(4, &cat(&[f_varint(1, 1), f_str(4, "pic.png")])),
    );
    // ImageDrawable (3005): drawable.geometry.size (1.1.2) and data ref (11 -> 1).
    let img_drawable = doc_builder.add(
        3005,
        &cat(&[
            f_bytes(
                1,
                &f_bytes(1, &f_bytes(2, &cat(&[f_f32(1, 100.0), f_f32(2, 80.0)]))),
            ),
            f_ref(11, 1),
        ]),
    );
    let att_image = doc_builder.attachment(img_drawable);
    let text = "중첩 인덱스 본문\n\u{FFFC}\n\u{FFFC}\n마지막 문단";
    let p2_at = utf16_len("중첩 인덱스 본문\n");
    let p3_at = p2_at + utf16_len("\u{FFFC}\n");
    let st = doc_builder.add(
        2001,
        &DocBuilder::storage_payload(
            text,
            &[],
            &[],
            &[],
            &[(p2_at, Some(att_table)), (p3_at, Some(att_image))],
        ),
    );
    let doc_iwa = doc_builder.finish(st);

    // Split the table objects out into Tables/Tile-1.iwa using disjoint IDs (1000+).
    let mut split_tbl = DocBuilder::default();
    for _ in 0..900 {
        split_tbl.reserve_id();
    }
    let st_cell = split_tbl.add(
        2001,
        &DocBuilder::storage_payload("분할셀", &[], &[], &[], &[]),
    );
    let payload_cell = split_tbl.add(6218, &f_ref(1, st_cell));
    let list_entry = f_bytes(
        3,
        &cat(&[f_varint(1, 1), f_varint(2, 1), f_ref(9, payload_cell)]),
    );
    let mut cell = vec![5u8, 9, 0, 0, 0, 0, 0, 0];
    cell.extend_from_slice(&0x0002_1070u32.to_le_bytes());
    for v in [1u32, 2, 1, 5, 1] {
        cell.extend_from_slice(&v.to_le_bytes());
    }
    let mut offs = vec![0xFFu8; 510];
    offs[0..2].copy_from_slice(&0u16.to_le_bytes());
    let row_rec = f_bytes(
        5,
        &cat(&[
            f_varint(1, 0),
            f_varint(2, 1),
            f_bytes(6, &cell),
            f_bytes(7, &offs),
        ]),
    );
    let tile = split_tbl.add(6002, &row_rec);
    let list = split_tbl.add(6005, &cat(&[f_varint(1, 8), list_entry]));
    let tiles = cat(&[
        f_bytes(1, &cat(&[f_varint(1, 0), f_ref(2, tile)])),
        f_varint(2, 256),
    ]);
    let store = cat(&[f_bytes(3, &tiles), f_ref(17, list)]);
    let model = split_tbl.add(
        6001,
        &cat(&[f_bytes(4, &store), f_varint(6, 1), f_varint(7, 1)]),
    );
    split_tbl.add_with_id(table_info_id, 6000, &f_ref(2, model));
    let table_iwa = iwa_frame(&split_tbl.records, true);

    (doc_iwa, table_iwa)
}

#[test]
fn valid_nested_index_zip_decodes_document_tables_and_data_files() {
    let (doc_iwa, table_iwa) = build_split_index_iwa();
    let inner_flat = ZipArchiveCodec::pack(&[
        ("Document.iwa".to_string(), doc_iwa.clone()),
        ("Tables/Tile-1.iwa".to_string(), table_iwa.clone()),
    ])
    .unwrap();
    let outer_flat = ZipArchiveCodec::pack(&[
        ("Index.zip".to_string(), inner_flat),
        ("Data/pic.png".to_string(), PNG_1X1.to_vec()),
        (
            "Metadata/Properties.plist".to_string(),
            b"bplist00".to_vec(),
        ),
    ])
    .unwrap();

    let pkg = load_package(&outer_flat).expect("flat nested Index.zip loads");
    assert_eq!(pkg.iwa_files, 2);
    assert_eq!(
        pkg.data_files.get("Data/pic.png").map(Vec::as_slice),
        Some(PNG_1X1)
    );

    let decoded = UniversalFormatConverter::decode_rich(&outer_flat, Some("nested.pages"))
        .expect("decodes rich document");
    assert_eq!(decoded.format, DocumentFormat::Pages);
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("expected document");
    };
    let mut saw_body = false;
    let mut saw_table = false;
    let mut saw_image = false;
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) if p.plain_text() == "중첩 인덱스 본문" => {
                saw_body = true
            }
            RichBlock::Table(t) if t.rows == vec![vec!["분할셀".to_string()]] => {
                saw_table = true
            }
            RichBlock::Image(img) if img.bytes() == PNG_1X1 => saw_image = true,
            _ => {}
        }
    }
    assert!(saw_body && saw_table && saw_image, "{:?}", doc.blocks);

    // Also test wrapped `<name>.pages/` prefix + inner entries already starting with `Index/`.
    let inner_prefixed = ZipArchiveCodec::pack(&[
        ("Index/Document.iwa".to_string(), doc_iwa),
        ("Index/Tables/Tile-1.iwa".to_string(), table_iwa),
    ])
    .unwrap();
    let outer_wrapped = ZipArchiveCodec::pack(&[
        ("Report.pages/Index.zip".to_string(), inner_prefixed),
        ("Report.pages/Data/pic.png".to_string(), PNG_1X1.to_vec()),
        (
            "Report.pages/Metadata/Properties.plist".to_string(),
            b"bplist00".to_vec(),
        ),
    ])
    .unwrap();
    let pkg_wrapped = load_package(&outer_wrapped).expect("wrapped nested Index.zip loads");
    assert_eq!(pkg_wrapped.iwa_files, 2);
    assert!(pkg_wrapped.data_files.contains_key("Data/pic.png"));
}

#[test]
fn nested_index_zip_rejects_combined_byte_and_entry_budget_bombs() {
    let (doc_iwa, table_iwa) = build_split_index_iwa();

    // 1) Combined entry count exceeds default ZipSecurityPolicy::max_entries (4,096).
    let mut outer_many: Vec<(String, Vec<u8>)> = (0..4_090)
        .map(|i| (format!("Data/d{i:04}.bin"), vec![1u8]))
        .collect();
    let mut inner_many = vec![("Document.iwa".to_string(), doc_iwa.clone())];
    for i in 0..9 {
        inner_many.push((format!("Tables/Tile-{i}.iwa"), table_iwa.clone()));
    }
    let inner_zip = ZipArchiveCodec::pack(&inner_many).unwrap();
    outer_many.push(("Index.zip".to_string(), inner_zip));
    let bomb_entries = ZipArchiveCodec::pack(&outer_many).unwrap();
    let err = load_package(&bomb_entries).unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "expected ResourceBudgetExceeded for entry sum, got {err:?}"
    );

    // 2) Combined uncompressed bytes exceeds ZipSecurityPolicy::max_total_uncompressed_bytes.
    let inner_uncompressed_bytes = (doc_iwa.len() + table_iwa.len()) as u64;
    let inner_small = ZipArchiveCodec::pack(&[
        ("Document.iwa".to_string(), doc_iwa.clone()),
        ("Tables/Tile-1.iwa".to_string(), table_iwa),
    ])
    .unwrap();
    let outer_bytes = ZipArchiveCodec::pack(&[
        ("Index.zip".to_string(), inner_small.clone()),
        ("Data/pad.bin".to_string(), vec![0x41u8; 40_000]),
    ])
    .unwrap();
    // Outer archive unpack alone is 40_000 + inner_small.len(), which fits this cap,
    // while outer + inner (doc_iwa + table_iwa) exceeds it and must fail closed.
    let tight_policy = ZipSecurityPolicy {
        max_total_uncompressed_bytes: 40_000
            + inner_small.len() as u64
            + inner_uncompressed_bytes / 2,
        ..ZipSecurityPolicy::default()
    };
    let err = load_package_with_policy(&outer_bytes, &tight_policy).unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "expected ResourceBudgetExceeded for byte sum, got {err:?}"
    );

    // 3) Combined entries exceeding custom policy::max_entries also fails with ResourceBudgetExceeded.
    let tight_entries = ZipSecurityPolicy {
        max_entries: 3,
        ..ZipSecurityPolicy::default()
    };
    let err = load_package_with_policy(&outer_bytes, &tight_entries).unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "expected ResourceBudgetExceeded for tight entry cap, got {err:?}"
    );
}

#[test]
fn nested_index_zip_rejects_encrypted_archives_before_unpacking() {
    let (doc_iwa, _) = build_split_index_iwa();

    // 1) Encrypted flag set in inner Index.zip central directory / local header.
    let mut inner_enc =
        ZipArchiveCodec::pack(&[("Document.iwa".to_string(), doc_iwa.clone())]).unwrap();
    let eocd = inner_enc
        .windows(4)
        .rposition(|w| w == [0x50, 0x4B, 0x05, 0x06])
        .unwrap();
    let cd_off = u32::from_le_bytes(inner_enc[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    set_u16_le(&mut inner_enc, 6, 1);
    set_u16_le(&mut inner_enc, cd_off + 8, 1);
    let pkg_flag_enc = ZipArchiveCodec::pack(&[("Index.zip".to_string(), inner_enc)]).unwrap();
    assert_eq!(
        load_package(&pkg_flag_enc).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );

    // 2) PasswordHint.iwph inside inner Index.zip.
    let inner_iwph = ZipArchiveCodec::pack(&[
        ("Document.iwa".to_string(), doc_iwa),
        ("PasswordHint.iwph".to_string(), vec![1, 2, 3]),
    ])
    .unwrap();
    let pkg_iwph = ZipArchiveCodec::pack(&[("Index.zip".to_string(), inner_iwph)]).unwrap();
    assert_eq!(
        load_package(&pkg_iwph).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );

    // 3) OLE2 offcrypto wrapper pretending to be Index.zip.
    let offcrypto_inner = Ole2ContainerCodec::pack(&[
        ("EncryptionInfo".to_string(), vec![0, 4, 0, 4, 0, 0, 0, 0]),
        ("EncryptedPackage".to_string(), vec![0xDE, 0xAD, 0xBE, 0xEF]),
    ])
    .unwrap();
    let pkg_offcrypto =
        ZipArchiveCodec::pack(&[("Index.zip".to_string(), offcrypto_inner)]).unwrap();
    assert_eq!(
        load_package(&pkg_offcrypto).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );
}

#[test]
fn nested_index_zip_rejects_duplicate_index_conflicts_and_abnormal_paths() {
    let (doc_iwa, table_iwa) = build_split_index_iwa();

    // Double Index/Index/ prefix inside Index.zip.
    let double_prefix =
        ZipArchiveCodec::pack(&[("Index/Index/Document.iwa".to_string(), doc_iwa.clone())])
            .unwrap();
    let outer_double = ZipArchiveCodec::pack(&[("Index.zip".to_string(), double_prefix)]).unwrap();
    assert!(matches!(
        load_package(&outer_double),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    // Normalized collision inside Index.zip: Document.iwa and Index/Document.iwa.
    let self_collide = ZipArchiveCodec::pack(&[
        ("Document.iwa".to_string(), doc_iwa.clone()),
        ("Index/Document.iwa".to_string(), doc_iwa.clone()),
    ])
    .unwrap();
    let outer_self_collide =
        ZipArchiveCodec::pack(&[("Index.zip".to_string(), self_collide)]).unwrap();
    assert!(matches!(
        load_package(&outer_self_collide),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    // Collision between outer Index/Tables/Tile-1.iwa and inner Tables/Tile-1.iwa.
    let inner_ok = ZipArchiveCodec::pack(&[
        ("Document.iwa".to_string(), doc_iwa.clone()),
        ("Tables/Tile-1.iwa".to_string(), table_iwa.clone()),
    ])
    .unwrap();
    let outer_collide = ZipArchiveCodec::pack(&[
        ("Index.zip".to_string(), inner_ok),
        ("Index/Tables/Tile-1.iwa".to_string(), table_iwa),
    ])
    .unwrap();
    assert!(matches!(
        load_package(&outer_collide),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    // Abnormal path (e.g., Data/evil.png inside Index.zip).
    let inner_abnormal = ZipArchiveCodec::pack(&[
        ("Document.iwa".to_string(), doc_iwa),
        ("Data/evil.png".to_string(), PNG_1X1.to_vec()),
    ])
    .unwrap();
    let outer_abnormal =
        ZipArchiveCodec::pack(&[("Index.zip".to_string(), inner_abnormal)]).unwrap();
    assert!(matches!(
        load_package(&outer_abnormal),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}
