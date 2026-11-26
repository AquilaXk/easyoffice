//! 비루프백 원격 제공자 전송 전 로컬 개인정보(이메일·전화번호·주민등록번호·식별번호) 자리표시자 치환 및 응답 복원기.
//!
//! `DocumentSecurityAuditor`(`smart_tools.rs`)와 `pii_identifiers.rs`의 256바이트 룩업 테이블(`PII_START_LUT`) 및
//! 선형 스캐너를 그대로 재사용해 정규식 역추적 없이 O(N) 단일 패스로 `[PII_EMAIL_1]` 등 자리표시자로 바꾸고,
//! 응답·스트림·연산 문자열에서 원문으로 복원한다. 원문에 이미 `[PII_*_N]` 모양 문자열이 있으면 그 번호 뒤부터
//! 발급해 자리표시자 충돌 주입을 차단한다.

use easyoffice_formats::StrictJsonValue;
use std::collections::HashMap;

use crate::pii_identifiers::match_identifier_at;
use crate::smart_tools::{DocumentSecurityAuditor, EmailScanOutcome, PII_START_LUT};

use super::op_types::{
    BoardGraphEdge, BoardGraphNode, CopilotLocation, CopilotOperation, CopilotRunResponse,
    CopilotToolCall, DocBlock, SlideElementSpec,
};
use super::types::HistoryMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PiiKind {
    Email,
    ResidentId,
    Phone,
    Identifier,
}

impl PiiKind {
    fn tag(self) -> &'static str {
        match self {
            Self::Email => "EMAIL",
            Self::ResidentId => "RRN",
            Self::Phone => "PHONE",
            Self::Identifier => "ID",
        }
    }
}

/// 한 요청 동안 발급한 자리표시자와 원문의 양방향 매핑.
#[derive(Debug, Default)]
pub struct PiiVault {
    orig_to_token: HashMap<String, String>,
    token_to_orig: HashMap<String, String>,
    next_id: usize,
}

impl PiiVault {
    pub fn new() -> Self {
        Self {
            orig_to_token: HashMap::new(),
            token_to_orig: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn has_entries(&self) -> bool {
        !self.token_to_orig.is_empty()
    }

    /// 입력에 이미 들어 있는 `[PII_<TAG>_<NUM>]` 리터럴과 절대 겹치지 않도록 시작 번호를 올린다.
    pub fn reserve_existing_tokens(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i + 7 < bytes.len() {
            if &bytes[i..i + 5] == b"[PII_" {
                if let Some((num, end)) = parse_pii_token_number(bytes, i) {
                    if num >= self.next_id {
                        self.next_id = num.saturating_add(1);
                    }
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
    }

    fn token_for(&mut self, kind: PiiKind, original: &str) -> &str {
        if !self.orig_to_token.contains_key(original) {
            let id = self.next_id;
            self.next_id = self.next_id.saturating_add(1);
            let token = format!("[PII_{}_{id}]", kind.tag());
            self.token_to_orig
                .insert(token.clone(), original.to_string());
            self.orig_to_token.insert(original.to_string(), token);
        }
        self.orig_to_token
            .get(original)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// 문자열 안의 개인정보를 자리표시자로 치환한다.
    pub fn redact_text(&mut self, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        let spans = scan_pii_spans(text);
        if spans.is_empty() {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len() + spans.len() * 4);
        let mut cursor = 0usize;
        for (start, end, kind) in spans {
            if cursor < start {
                out.push_str(&text[cursor..start]);
            }
            let orig = &text[start..end];
            out.push_str(self.token_for(kind, orig));
            cursor = end;
        }
        if cursor < text.len() {
            out.push_str(&text[cursor..]);
        }
        out
    }

    pub fn redact_history(&mut self, history: &[HistoryMessage]) -> Vec<HistoryMessage> {
        for h in history {
            self.reserve_existing_tokens(&h.text);
        }
        history
            .iter()
            .map(|h| HistoryMessage {
                role: h.role.clone(),
                text: self.redact_text(&h.text),
            })
            .collect()
    }

    /// 자리표시자를 원문으로 복원한다. 발급한 적 없는 `[PII_*]` 문자열은 그대로 둔다.
    pub fn restore_text(&self, text: &str) -> String {
        if self.token_to_orig.is_empty() || !text.contains("[PII_") {
            return text.to_string();
        }
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'[' && i + 7 <= bytes.len() && &bytes[i..i + 5] == b"[PII_" {
                if let Some((_, end)) = parse_pii_token_number(bytes, i) {
                    let candidate = &text[i..end];
                    if let Some(orig) = self.token_to_orig.get(candidate) {
                        out.push_str(&text[cursor..i]);
                        out.push_str(orig);
                        i = end;
                        cursor = end;
                        continue;
                    }
                }
            }
            i += 1;
        }
        if cursor == 0 {
            return text.to_string();
        }
        if cursor < text.len() {
            out.push_str(&text[cursor..]);
        }
        out
    }

    /// JSON AST의 모든 문자열·객체 키 안 자리표시자를 검증 전에 원문으로 복원한다.
    pub fn restore_json_entries(&self, entries: &mut [(String, StrictJsonValue)]) {
        if !self.has_entries() {
            return;
        }
        for (k, v) in entries {
            *k = self.restore_text(k);
            self.restore_json_value(v);
        }
    }

    fn restore_json_value(&self, val: &mut StrictJsonValue) {
        match val {
            StrictJsonValue::Str(s) => *s = self.restore_text(s),
            StrictJsonValue::Array(items) => {
                for item in items {
                    self.restore_json_value(item);
                }
            }
            StrictJsonValue::Object(entries) => {
                for (k, v) in entries {
                    *k = self.restore_text(k);
                    self.restore_json_value(v);
                }
            }
            StrictJsonValue::Null | StrictJsonValue::Bool(_) | StrictJsonValue::Number(_) => {}
        }
    }

    fn restore_opt(&self, val: &mut Option<String>) {
        if let Some(s) = val {
            *s = self.restore_text(s);
        }
    }

    fn restore_vec(&self, list: &mut [String]) {
        for s in list {
            *s = self.restore_text(s);
        }
    }

    fn restore_blocks(&self, blocks: &mut [DocBlock]) {
        for b in blocks {
            match b {
                DocBlock::Heading { text, .. } | DocBlock::Paragraph { text } => {
                    *text = self.restore_text(text);
                }
                DocBlock::List { items, .. } => self.restore_vec(items),
                DocBlock::Table { rows, .. } => {
                    for row in rows {
                        self.restore_vec(row);
                    }
                }
            }
        }
    }

    fn restore_slide_el(&self, el: &mut SlideElementSpec) {
        self.restore_opt(&mut el.text);
        self.restore_opt(&mut el.title);
        self.restore_opt(&mut el.alt);
        if let Some(rows) = &mut el.rows {
            for row in rows {
                self.restore_vec(row);
            }
        }
        if let Some(cats) = &mut el.categories {
            self.restore_vec(cats);
        }
        if let Some(series) = &mut el.series {
            for s in series {
                s.name = self.restore_text(&s.name);
            }
        }
    }

    pub fn restore_response(&self, resp: &mut CopilotRunResponse) {
        if !self.has_entries() {
            return;
        }
        resp.reply = self.restore_text(&resp.reply);
        resp.reasoning = self.restore_text(&resp.reasoning);
        for loc in &mut resp.locations {
            let CopilotLocation { label, sheet, .. } = loc;
            *label = self.restore_text(label);
            self.restore_opt(sheet);
        }
        for tc in &mut resp.tool_calls {
            match tc {
                CopilotToolCall::ReadDocBlocks { .. } | CopilotToolCall::ReadSheetRange { .. } => {}
                CopilotToolCall::FindSheetCells { query, .. }
                | CopilotToolCall::SearchPdfPages { query } => {
                    *query = self.restore_text(query);
                }
            }
        }
        for op in &mut resp.operations {
            match op {
                CopilotOperation::SheetWrite { cells, .. } => {
                    for c in cells {
                        c.raw = self.restore_text(&c.raw);
                    }
                }
                CopilotOperation::SheetChart { title, .. } => {
                    *title = self.restore_text(title);
                }
                CopilotOperation::SheetTable { table_name, .. } => {
                    self.restore_opt(table_name);
                }
                CopilotOperation::SheetPivot {
                    row_field,
                    col_field,
                    value_field,
                    ..
                } => {
                    *row_field = self.restore_text(row_field);
                    self.restore_opt(col_field);
                    *value_field = self.restore_text(value_field);
                }
                CopilotOperation::SheetCondFormat { value1, value2, .. } => {
                    self.restore_opt(value1);
                    self.restore_opt(value2);
                }
                CopilotOperation::SheetSortFilter { query, .. } => {
                    self.restore_opt(query);
                }
                CopilotOperation::SheetNote { text, .. } => {
                    *text = self.restore_text(text);
                }
                CopilotOperation::DocAppend { blocks }
                | CopilotOperation::DocReplaceRange { blocks, .. } => {
                    self.restore_blocks(blocks);
                }
                CopilotOperation::DocReplaceParagraphs { replacements } => {
                    for r in replacements {
                        r.text = self.restore_text(&r.text);
                    }
                }
                CopilotOperation::SlideAdd {
                    title,
                    bullets,
                    notes,
                    elements,
                    ..
                } => {
                    *title = self.restore_text(title);
                    self.restore_vec(bullets);
                    *notes = self.restore_text(notes);
                    if let Some(els) = elements {
                        for el in els {
                            self.restore_slide_el(el);
                        }
                    }
                }
                CopilotOperation::SlideUpdate {
                    title,
                    bullets,
                    notes,
                    elements,
                    ..
                } => {
                    self.restore_opt(title);
                    if let Some(bs) = bullets {
                        self.restore_vec(bs);
                    }
                    self.restore_opt(notes);
                    if let Some(els) = elements {
                        for el in els {
                            self.restore_slide_el(el);
                        }
                    }
                }
                CopilotOperation::SlideAddElement { element, .. } => {
                    self.restore_slide_el(element);
                }
                CopilotOperation::BaseAddField { name, options, .. } => {
                    *name = self.restore_text(name);
                    self.restore_vec(options);
                }
                CopilotOperation::BaseAddRecords { records } => {
                    for row in records {
                        for (k, v) in row {
                            *k = self.restore_text(k);
                            *v = self.restore_text(v);
                        }
                    }
                }
                CopilotOperation::BaseUpdateRecords { updates } => {
                    for (_, row) in updates {
                        for (k, v) in row {
                            *k = self.restore_text(k);
                            *v = self.restore_text(v);
                        }
                    }
                }
                CopilotOperation::BaseSetView {
                    filter_field,
                    filter_value,
                    sort_field,
                    group_field,
                    ..
                } => {
                    self.restore_opt(filter_field);
                    self.restore_opt(filter_value);
                    self.restore_opt(sort_field);
                    self.restore_opt(group_field);
                }
                CopilotOperation::BoardAddGraph { nodes, edges, .. } => {
                    for BoardGraphNode { text, .. } in nodes {
                        *text = self.restore_text(text);
                    }
                    for BoardGraphEdge { label, .. } in edges {
                        self.restore_opt(label);
                    }
                }
                CopilotOperation::SheetFormat { .. }
                | CopilotOperation::SheetStructure { .. }
                | CopilotOperation::DocScrubPii => {}
            }
        }
    }
}

/// 스트림 조각 경계에서 `[PII_EMAIL_1]`이 반으로 잘려 들어와도 닫는 `]`가 올 때까지 보류한 뒤 원문으로 복원해 흘린다.
#[derive(Default)]
pub struct StreamingPiiRestorer {
    pending: String,
}

impl StreamingPiiRestorer {
    pub fn push(&mut self, vault: &PiiVault, chunk: &str) -> String {
        if !vault.has_entries() {
            return chunk.to_string();
        }
        self.pending.push_str(chunk);
        let hold_from = find_dangling_pii_prefix(&self.pending);
        let ready = self.pending[..hold_from].to_string();
        self.pending.drain(..hold_from);
        vault.restore_text(&ready)
    }

    pub fn flush(&mut self, vault: &PiiVault) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        let rest = std::mem::take(&mut self.pending);
        vault.restore_text(&rest)
    }
}

fn find_dangling_pii_prefix(s: &str) -> usize {
    const PREFIX: &[u8] = b"[PII_";
    let bytes = s.as_bytes();
    let tail_start = bytes.len().saturating_sub(28);
    for i in (tail_start..bytes.len()).rev() {
        if bytes[i] != b'[' {
            continue;
        }
        let rem = &bytes[i..];
        if rem.contains(&b']') {
            return bytes.len();
        }
        let prefix_check_len = rem.len().min(PREFIX.len());
        if rem[..prefix_check_len] == PREFIX[..prefix_check_len]
            && rem
                .iter()
                .skip(1)
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
        {
            return i;
        }
        break;
    }
    bytes.len()
}

/// `bytes[start..]`가 `[PII_<UPPER>_<DIGITS>]`면 `(번호, 닫는 대괄호 다음 인덱스)`를 돌려준다.
fn parse_pii_token_number(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if start + 8 > bytes.len() || &bytes[start..start + 5] != b"[PII_" {
        return None;
    }
    let mut p = start + 5;
    let tag_start = p;
    while p < bytes.len() && bytes[p].is_ascii_uppercase() && p - tag_start <= 8 {
        p += 1;
    }
    if p == tag_start || p >= bytes.len() || bytes[p] != b'_' {
        return None;
    }
    p += 1;
    let num_start = p;
    while p < bytes.len() && bytes[p].is_ascii_digit() && p - num_start <= 8 {
        p += 1;
    }
    if p == num_start || p >= bytes.len() || bytes[p] != b']' {
        return None;
    }
    let num_str = std::str::from_utf8(&bytes[num_start..p]).ok()?;
    let num = num_str.parse::<usize>().ok()?;
    Some((num, p + 1))
}

fn scan_pii_spans(text: &str) -> Vec<(usize, usize, PiiKind)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut idx = 0usize;
    let mut email_failed_until = if bytes.contains(&b'@') {
        0
    } else {
        bytes.len()
    };

    while idx < bytes.len() {
        // 이미 치환된 `[PII_...]` 자리표시자는 건너뛴다.
        if bytes[idx] == b'[' {
            if let Some((_, end)) = parse_pii_token_number(bytes, idx) {
                idx = end;
                continue;
            }
        }
        let b = bytes[idx];
        let flags = PII_START_LUT[b as usize];
        if flags == 0 {
            idx += 1;
            continue;
        }
        if (flags & 0x02) != 0 && idx >= email_failed_until {
            match DocumentSecurityAuditor::scan_email_at(text, idx) {
                EmailScanOutcome::Matched(len) => {
                    spans.push((idx, idx + len, PiiKind::Email));
                    idx += len;
                    continue;
                }
                EmailScanOutcome::Failed(skip_until) => {
                    email_failed_until = skip_until;
                }
            }
        }
        if (flags & 0x01) != 0 {
            if let Some((len, _)) = DocumentSecurityAuditor::match_resident_id_at(bytes, idx) {
                spans.push((idx, idx + len, PiiKind::ResidentId));
                idx += len;
                continue;
            }
            if let Some((len, _)) = DocumentSecurityAuditor::match_phone_at(bytes, idx) {
                spans.push((idx, idx + len, PiiKind::Phone));
                idx += len;
                continue;
            }
            if let Some(len) = match_identifier_at(bytes, idx) {
                spans.push((idx, idx + len, PiiKind::Identifier));
                idx += len;
                continue;
            }
        }
        if b < 0x80 {
            idx += 1;
        } else {
            let step = text[idx..].chars().next().map_or(1, char::len_utf8);
            idx += step;
        }
    }
    spans
}
