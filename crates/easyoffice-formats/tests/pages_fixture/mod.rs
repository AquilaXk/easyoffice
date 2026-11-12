//! Self-authored synthetic Pages packages: protobuf writer, Snappy encoders, IWA framing and a
//! document builder that follows the schema recorded in `pages_schema.rs`. No third-party or
//! vendor-authored bytes are used.
#![allow(dead_code)]

use easyoffice_formats::ZipArchiveCodec;

pub fn varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

pub fn f_varint(num: u32, v: u64) -> Vec<u8> {
    let mut o = Vec::new();
    varint(u64::from(num) << 3, &mut o);
    varint(v, &mut o);
    o
}

pub fn f_bytes(num: u32, b: &[u8]) -> Vec<u8> {
    let mut o = Vec::new();
    varint((u64::from(num) << 3) | 2, &mut o);
    varint(b.len() as u64, &mut o);
    o.extend_from_slice(b);
    o
}

pub fn f_str(num: u32, s: &str) -> Vec<u8> {
    f_bytes(num, s.as_bytes())
}

pub fn f_f32(num: u32, v: f32) -> Vec<u8> {
    let mut o = Vec::new();
    varint((u64::from(num) << 3) | 5, &mut o);
    o.extend_from_slice(&v.to_le_bytes());
    o
}

pub fn f_ref(num: u32, id: u64) -> Vec<u8> {
    f_bytes(num, &f_varint(1, id))
}

pub fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

/// Literal-only Snappy block.
pub fn snappy_literal(data: &[u8]) -> Vec<u8> {
    let mut o = Vec::new();
    varint(data.len() as u64, &mut o);
    for chunk in data.chunks(60_000) {
        let n = chunk.len() - 1;
        if n < 60 {
            o.push((n as u8) << 2);
        } else if n < 256 {
            o.push(60 << 2);
            o.push(n as u8);
        } else {
            o.push(61 << 2);
            o.extend_from_slice(&(n as u16).to_le_bytes());
        }
        o.extend_from_slice(chunk);
    }
    o
}

/// Small greedy compressor that emits literal, 1-byte-offset and 2-byte-offset copy elements.
pub fn snappy_compress(data: &[u8]) -> Vec<u8> {
    let mut o = Vec::new();
    varint(data.len() as u64, &mut o);
    let mut table: std::collections::HashMap<[u8; 4], usize> = std::collections::HashMap::new();
    let mut lit_start = 0usize;
    let mut i = 0usize;
    let flush = |o: &mut Vec<u8>, from: usize, to: usize| {
        if to > from {
            let lit = &data[from..to];
            let n = lit.len() - 1;
            if n < 60 {
                o.push((n as u8) << 2);
            } else if n < 256 {
                o.push(60 << 2);
                o.push(n as u8);
            } else {
                o.push(61 << 2);
                o.extend_from_slice(&(n as u16).to_le_bytes());
            }
            o.extend_from_slice(lit);
        }
    };
    while i + 4 <= data.len() {
        let key = [data[i], data[i + 1], data[i + 2], data[i + 3]];
        if let Some(&p) = table.get(&key) {
            let off = i - p;
            if off < 65_536 {
                let mut len = 4;
                while i + len < data.len() && len < 64 && data[p + len] == data[i + len] {
                    len += 1;
                }
                flush(&mut o, lit_start, i);
                if (4..=11).contains(&len) && off < 2048 {
                    o.push((((off >> 8) as u8) << 5) | (((len - 4) as u8) << 2) | 1);
                    o.push((off & 0xFF) as u8);
                } else {
                    o.push((((len - 1) as u8) << 2) | 2);
                    o.extend_from_slice(&(off as u16).to_le_bytes());
                }
                for k in i..i + len {
                    if k + 4 <= data.len() {
                        table.insert([data[k], data[k + 1], data[k + 2], data[k + 3]], k);
                    }
                }
                i += len;
                lit_start = i;
                continue;
            }
        }
        table.insert(key, i);
        i += 1;
    }
    flush(&mut o, lit_start, data.len());
    o
}

/// Frames a record stream into IWA chunks (4-byte header, no CRC).
pub fn iwa_frame(stream: &[u8], compress: bool) -> Vec<u8> {
    let mut o = Vec::new();
    for chunk in stream.chunks(65_536) {
        let body = if compress {
            snappy_compress(chunk)
        } else {
            snappy_literal(chunk)
        };
        o.push(0);
        o.extend_from_slice(&(body.len() as u32).to_le_bytes()[..3]);
        o.extend_from_slice(&body);
    }
    o
}

pub fn record(identifier: u64, type_id: u32, payload: &[u8]) -> Vec<u8> {
    let mi = cat(&[
        f_varint(1, u64::from(type_id)),
        f_varint(3, payload.len() as u64),
    ]);
    let info = cat(&[f_varint(1, identifier), f_bytes(2, &mi)]);
    let mut o = Vec::new();
    varint(info.len() as u64, &mut o);
    o.extend_from_slice(&info);
    o.extend_from_slice(payload);
    o
}

// ---- document builder (schema as recorded in pages_schema.rs) ----

pub struct Para {
    pub text: String,
    pub style: Option<u64>,
    pub list: Option<u64>,
}

pub struct DocBuilder {
    pub records: Vec<u8>,
    next_id: u64,
}

impl Default for DocBuilder {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            next_id: 100,
        }
    }
}

pub fn utf16_len(s: &str) -> u64 {
    s.encode_utf16().count() as u64
}

impl DocBuilder {
    pub fn add(&mut self, type_id: u32, payload: &[u8]) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.records.extend(record(id, type_id, payload));
        id
    }

    pub fn add_with_id(&mut self, id: u64, type_id: u32, payload: &[u8]) {
        self.records.extend(record(id, type_id, payload));
    }

    pub fn reserve_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Paragraph style (2022) with optional parent, identifier, bold, size, alignment.
    pub fn para_style(
        &mut self,
        ident: &str,
        parent: Option<u64>,
        bold: Option<bool>,
        size: Option<f32>,
        align: Option<u64>,
    ) -> u64 {
        let mut header = vec![f_str(2, ident)];
        if let Some(p) = parent {
            header.push(f_ref(3, p));
        }
        let mut cp = Vec::new();
        if let Some(b) = bold {
            cp.push(f_varint(1, u64::from(b)));
        }
        if let Some(s) = size {
            cp.push(f_f32(3, s));
        }
        let mut payload = vec![f_bytes(1, &cat(&header)), f_bytes(11, &cat(&cp))];
        if let Some(a) = align {
            payload.push(f_bytes(12, &f_varint(1, a)));
        }
        self.add(2022, &cat(&payload))
    }

    pub fn char_style(
        &mut self,
        bold: Option<bool>,
        italic: Option<bool>,
        underline: Option<bool>,
        strike: Option<bool>,
        color: Option<[f32; 3]>,
    ) -> u64 {
        let mut cp = Vec::new();
        for (n, v) in [(1, bold), (2, italic), (11, underline), (12, strike)] {
            if let Some(b) = v {
                cp.push(f_varint(n, u64::from(b)));
            }
        }
        if let Some([r, g, b]) = color {
            cp.push(f_bytes(
                7,
                &cat(&[f_f32(3, r), f_f32(4, g), f_f32(5, b), f_f32(6, 1.0)]),
            ));
        }
        self.add(2021, &f_bytes(11, &cat(&cp)))
    }

    pub fn list_style(&mut self, level_type: u64) -> u64 {
        let levels: Vec<Vec<u8>> = (0..3).map(|_| f_varint(11, level_type)).collect();
        self.add(2023, &cat(&levels))
    }

    pub fn attr_table(entries: &[(u64, Option<u64>)]) -> Vec<u8> {
        let es: Vec<Vec<u8>> = entries
            .iter()
            .map(|(i, o)| {
                let mut e = f_varint(1, *i);
                if let Some(o) = o {
                    e.extend(f_ref(2, *o));
                }
                f_bytes(1, &e)
            })
            .collect();
        cat(&es)
    }

    /// Text storage with the given attribute tables (field numbers 5, 8, 7, 9).
    pub fn storage_payload(
        text: &str,
        para: &[(u64, Option<u64>)],
        chars: &[(u64, Option<u64>)],
        lists: &[(u64, Option<u64>)],
        attach: &[(u64, Option<u64>)],
    ) -> Vec<u8> {
        let mut parts = vec![f_varint(1, 0), f_str(3, text)];
        for (n, t) in [(5u32, para), (7, lists), (8, chars), (9, attach)] {
            if !t.is_empty() {
                parts.push(f_bytes(n, &Self::attr_table(t)));
            }
        }
        cat(&parts)
    }

    /// A table of rich-text cells (version-5 packed layout). Returns the table-info id.
    pub fn table(&mut self, grid: &[Vec<&str>]) -> u64 {
        self.table_with_merges(grid, &[])
    }

    /// Like [`Self::table`] with a merge region map; each region is `(row, col, rows, cols)`,
    /// stored as entries whose origin and size are varints packing `row << 16 | column`.
    pub fn table_with_merges(
        &mut self,
        grid: &[Vec<&str>],
        merges: &[(u32, u32, u32, u32)],
    ) -> u64 {
        self.table_full(grid, merges, &[])
    }

    /// Table whose merges are formulas (each given as its AST node list payload) in the merge
    /// owner's formula store (model field 47).
    pub fn table_with_owner_formulas(&mut self, grid: &[Vec<&str>], nodes: &[Vec<u8>]) -> u64 {
        self.table_full(grid, &[], nodes)
    }

    fn table_full(
        &mut self,
        grid: &[Vec<&str>],
        merges: &[(u32, u32, u32, u32)],
        owner_nodes: &[Vec<u8>],
    ) -> u64 {
        let rows = grid.len();
        let cols = grid[0].len();
        let mut list_entries = Vec::new();
        let mut key = 1u32;
        let mut row_infos = Vec::new();
        for (r, row) in grid.iter().enumerate() {
            let mut buf = Vec::new();
            let mut offs = vec![0xFFu8; 510];
            for (c, text) in row.iter().enumerate() {
                let st = self.add(2001, &Self::storage_payload(text, &[], &[], &[], &[]));
                let payload = self.add(6218, &f_ref(1, st));
                list_entries.push(f_bytes(
                    3,
                    &cat(&[
                        f_varint(1, u64::from(key)),
                        f_varint(2, 1),
                        f_ref(9, payload),
                    ]),
                ));
                offs[c * 2..c * 2 + 2].copy_from_slice(&(buf.len() as u16).to_le_bytes());
                let mut cell = vec![5u8, 9, 0, 0, 0, 0, 0, 0];
                cell.extend_from_slice(&0x0002_1070u32.to_le_bytes());
                for v in [key, 2, 1, 5, 1] {
                    cell.extend_from_slice(&v.to_le_bytes());
                }
                buf.extend(cell);
                key += 1;
            }
            row_infos.push(f_bytes(
                5,
                &cat(&[
                    f_varint(1, r as u64),
                    f_varint(2, cols as u64),
                    f_bytes(6, &buf),
                    f_bytes(7, &offs),
                ]),
            ));
        }
        let tile = self.add(6002, &cat(&row_infos));
        let list = self.add(6005, &cat(&[f_varint(1, 8), cat(&list_entries)]));
        let tiles = cat(&[
            f_bytes(1, &cat(&[f_varint(1, 0), f_ref(2, tile)])),
            f_varint(2, 256),
        ]);
        let mut store = cat(&[f_bytes(3, &tiles), f_ref(17, list)]);
        if !merges.is_empty() {
            let ranges: Vec<Vec<u8>> = merges
                .iter()
                .map(|&(r, c, nr, nc)| {
                    f_bytes(
                        1,
                        &cat(&[
                            f_varint(1, u64::from(r << 16 | c)),
                            f_varint(2, u64::from(nr << 16 | nc)),
                        ]),
                    )
                })
                .collect();
            let map = self.add(6007, &cat(&ranges));
            store.extend(f_ref(13, map));
        }
        let mut model_fields = vec![
            f_bytes(4, &store),
            f_varint(6, rows as u64),
            f_varint(7, cols as u64),
        ];
        if !owner_nodes.is_empty() {
            let pairs: Vec<Vec<u8>> = owner_nodes
                .iter()
                .enumerate()
                .map(|(i, node)| {
                    let formula = f_bytes(1, &f_bytes(1, node));
                    f_bytes(3, &cat(&[f_varint(1, i as u64), f_bytes(2, &formula)]))
                })
                .collect();
            model_fields.push(f_bytes(47, &f_bytes(2, &cat(&pairs))));
        }
        let model = self.add(6001, &cat(&model_fields));
        self.add(6000, &f_ref(2, model))
    }

    pub fn attachment(&mut self, drawable: u64) -> u64 {
        self.add(2003, &f_ref(1, drawable))
    }

    /// Adds the root pointing at `body`, then returns the finished `Index/Document.iwa` bytes.
    pub fn finish(mut self, body: u64) -> Vec<u8> {
        let root = f_ref(4, body);
        self.add_with_id(1, 10000, &root);
        iwa_frame(&self.records, true)
    }
}

pub fn pages_zip(iwa: Vec<u8>) -> Vec<u8> {
    pages_zip_with_data(iwa, Vec::new())
}

/// Package with extra files (for example `Data/<name>`), keyed by package-relative path.
pub fn pages_zip_with_data(iwa: Vec<u8>, data: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let mut entries = vec![
        ("Index/Document.iwa".to_string(), iwa),
        (
            "Metadata/Properties.plist".to_string(),
            b"bplist00".to_vec(),
        ),
    ];
    entries.extend(data);
    ZipArchiveCodec::pack(&entries).expect("zip")
}

/// A document whose body is one paragraph holding a table with the given merge regions.
pub fn merged_table_document(grid: &[Vec<&str>], merges: &[(u32, u32, u32, u32)]) -> Vec<u8> {
    let mut b = DocBuilder::default();
    let table = b.table_with_merges(grid, merges);
    let att = b.attachment(table);
    let st = b.add(
        2001,
        &DocBuilder::storage_payload("\u{FFFC}", &[], &[], &[], &[(0, Some(att))]),
    );
    pages_zip(b.finish(st))
}

/// Standard sample: heading, formatted body paragraph, bullet item, center paragraph, a 2x2
/// table and an unsupported drawable. Returns (zip bytes, plain text of the formatted run set).
pub fn sample_document() -> Vec<u8> {
    let mut b = DocBuilder::default();
    let heading = b.para_style(
        "x-paragraphstyle-Heading 1",
        None,
        Some(true),
        Some(17.0),
        Some(0),
    );
    let body_style = b.para_style(
        "x-paragraphstyle-Body 1",
        None,
        Some(false),
        Some(10.0),
        Some(0),
    );
    let centered = b.para_style("", Some(body_style), None, None, Some(2));
    let bold_ul = b.char_style(Some(true), None, Some(true), None, None);
    let ital_strike = b.char_style(None, Some(true), None, Some(true), Some([1.0, 0.0, 0.0]));
    let bullet = b.list_style(2);
    let none_list = b.list_style(0);
    let table = b.table(&[vec!["A1", "B1"], vec!["A2", "B2"]]);
    let att_table = b.attachment(table);
    let image = b.add(3005, &f_varint(1, 1));
    let att_image = b.attachment(image);

    // paragraphs: 0 heading, 1 body with bold/underline then italic/strike runs, 2 bullet,
    // 3 centered, 4 table anchor, 5 image anchor, 6 closing text
    let p0 = "Title text";
    let p1 = "Plain bold ital";
    let p2 = "Item";
    let p3 = "Centered";
    let p4 = "\u{FFFC}";
    let p5 = "\u{FFFC}";
    let p6 = "Last";
    let paras = [p0, p1, p2, p3, p4, p5, p6];
    let text = paras.join("\n");
    let mut starts = Vec::new();
    let mut pos = 0u64;
    for p in paras {
        starts.push(pos);
        pos += utf16_len(p) + 1;
    }
    let para_tbl = [
        (starts[0], Some(heading)),
        (starts[1], Some(body_style)),
        (starts[3], Some(centered)),
        (starts[4], Some(body_style)),
    ];
    let char_tbl = [
        (starts[1] + 6, Some(bold_ul)),
        (starts[1] + 11, Some(ital_strike)),
        (starts[2], None),
    ];
    let list_tbl = [
        (starts[0], Some(none_list)),
        (starts[2], Some(bullet)),
        (starts[3], Some(none_list)),
    ];
    let attach_tbl = [(starts[4], Some(att_table)), (starts[5], Some(att_image))];
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(&text, &para_tbl, &char_tbl, &list_tbl, &attach_tbl),
    );
    pages_zip(b.finish(st))
}
