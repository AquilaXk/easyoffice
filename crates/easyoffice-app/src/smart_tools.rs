use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritingDiagnosticReport {
    pub char_count: usize,
    pub word_count: usize,
    pub sentence_count: usize,
    pub reading_time_seconds: usize,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiiReplacementSpan {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub replacement: String,
}

pub struct LocalSmartAssistant;

impl LocalSmartAssistant {
    pub fn summarize_document(paragraphs: &[String], max_sentences: usize) -> Vec<String> {
        if max_sentences == 0 || paragraphs.is_empty() {
            return Vec::new();
        }
        let mut sentences: Vec<(usize, String)> = Vec::new();
        for para in paragraphs {
            for part in para.split(['.', '!', '?', '\n']) {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    sentences.push((sentences.len(), trimmed.to_string()));
                }
            }
        }
        if sentences.len() <= max_sentences {
            return sentences.into_iter().map(|(_, s)| s).collect();
        }

        let mut tf: HashMap<String, usize> = HashMap::new();
        for (_, sent) in &sentences {
            for token in sent.split(|c: char| !c.is_alphanumeric()) {
                let lower = token.to_lowercase();
                if lower.chars().count() >= 2 {
                    *tf.entry(lower).or_insert(0) += 1;
                }
            }
        }

        let mut scored: Vec<(usize, usize, String)> = sentences
            .into_iter()
            .map(|(idx, sent)| {
                let score: usize = sent
                    .split(|c: char| !c.is_alphanumeric())
                    .map(|tok| tf.get(&tok.to_lowercase()).copied().unwrap_or(0))
                    .sum();
                (idx, score, sent)
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored.truncate(max_sentences);
        scored.sort_by_key(|item| item.0);
        scored.into_iter().map(|(_, _, s)| s).collect()
    }

    pub fn diagnose_writing(text: &str) -> WritingDiagnosticReport {
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();
        let sentences: Vec<&str> = text
            .split(['.', '!', '?', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let sentence_count = sentences.len().max(usize::from(word_count > 0));
        let reading_time_seconds = ((word_count * 60) / 220).max(usize::from(char_count > 0));

        let mut suggestions = Vec::new();
        for (i, s) in sentences.iter().enumerate() {
            if s.chars().count() > 80 {
                suggestions.push(format!(
                    "문장 {}이(가) 80자를 초과합니다. 간결한 두 문장으로 분리를 권장합니다.",
                    i + 1
                ));
            }
            if s.contains("되어지다") || s.contains("보여지다") || s.contains("생각되어지다")
            {
                suggestions.push(format!(
                    "문장 {}에 이중 피동 표현('되어지다/보여지다')이 포함되어 있습니다. 능동형으로 교정을 권장합니다.",
                    i + 1
                ));
            }
        }

        WritingDiagnosticReport {
            char_count,
            word_count,
            sentence_count,
            reading_time_seconds,
            suggestions,
        }
    }

    pub fn extract_action_items(paragraphs: &[String]) -> Vec<String> {
        const MARKERS: [&str; 6] = ["TODO", "검토", "결재", "기한", "완료", "조치"];
        let mut items = Vec::new();
        for para in paragraphs {
            for line in para.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if MARKERS.iter().any(|m| trimmed.contains(m)) {
                    items.push(trimmed.to_string());
                }
            }
        }
        items
    }

    pub const MAX_DIFF_PARAGRAPHS: usize = 512;

    pub fn diff_documents_lcs(before: &[String], after: &[String]) -> Vec<DocumentDiffLine> {
        let before = &before[..before.len().min(Self::MAX_DIFF_PARAGRAPHS)];
        let after = &after[..after.len().min(Self::MAX_DIFF_PARAGRAPHS)];
        let m = before.len();
        let n = after.len();
        let stride = n + 1;
        let mut dp = vec![0usize; (m + 1) * stride];

        for i in (0..m).rev() {
            for j in (0..n).rev() {
                if before[i] == after[j] {
                    dp[i * stride + j] = 1 + dp[(i + 1) * stride + (j + 1)];
                } else {
                    dp[i * stride + j] = dp[(i + 1) * stride + j].max(dp[i * stride + (j + 1)]);
                }
            }
        }

        let mut out = Vec::with_capacity(m + n);
        let mut i = 0;
        let mut j = 0;
        while i < m && j < n {
            if before[i] == after[j] {
                out.push(DocumentDiffLine {
                    kind: DiffLineKind::Unchanged,
                    text: before[i].clone(),
                });
                i += 1;
                j += 1;
            } else if dp[(i + 1) * stride + j] >= dp[i * stride + (j + 1)] {
                out.push(DocumentDiffLine {
                    kind: DiffLineKind::Removed,
                    text: before[i].clone(),
                });
                i += 1;
            } else {
                out.push(DocumentDiffLine {
                    kind: DiffLineKind::Added,
                    text: after[j].clone(),
                });
                j += 1;
            }
        }
        while i < m {
            out.push(DocumentDiffLine {
                kind: DiffLineKind::Removed,
                text: before[i].clone(),
            });
            i += 1;
        }
        while j < n {
            out.push(DocumentDiffLine {
                kind: DiffLineKind::Added,
                text: after[j].clone(),
            });
            j += 1;
        }
        out
    }

    pub fn scrub_pii(text: &str) -> PiiAuditResult {
        DocumentSecurityAuditor::scan_and_scrub_pii(text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiiAuditResult {
    pub scrubbed_text: String,
    pub masked_resident_ids: usize,
    pub masked_phone_numbers: usize,
    pub masked_emails: usize,
    pub spans: Vec<PiiReplacementSpan>,
    pub stripped_metadata_fields: Vec<String>,
}

impl PiiAuditResult {
    pub fn total_masked_count(&self) -> usize {
        self.masked_resident_ids + self.masked_phone_numbers + self.masked_emails
    }
}

enum EmailScanOutcome {
    Matched(usize),
    Failed(usize),
}

pub struct DocumentSecurityAuditor;

impl DocumentSecurityAuditor {
    pub fn scan_and_scrub_pii(text: &str) -> PiiAuditResult {
        Self::scan_and_scrub_pii_with_metadata(text, &[])
    }

    pub fn scan_and_scrub_pii_with_metadata(
        text: &str,
        metadata_entries: &[(String, String)],
    ) -> PiiAuditResult {
        let bytes = text.as_bytes();
        let mut scrubbed_text = String::with_capacity(text.len());
        let mut masked_resident_ids = 0usize;
        let mut masked_phone_numbers = 0usize;
        let mut masked_emails = 0usize;
        let mut spans: Vec<PiiReplacementSpan> = Vec::new();
        let mut idx = 0usize;
        let mut utf16_pos = 0usize;
        let mut email_failed_until = 0usize;

        while idx < bytes.len() {
            if idx >= email_failed_until {
                match Self::scan_email_at(text, idx) {
                    EmailScanOutcome::Matched(matched_len) => {
                        let utf16_len: usize = text[idx..idx + matched_len]
                            .chars()
                            .map(char::len_utf16)
                            .sum();
                        let replacement = "***@***";
                        scrubbed_text.push_str(replacement);
                        spans.push(PiiReplacementSpan {
                            start_utf16: utf16_pos,
                            end_utf16: utf16_pos + utf16_len,
                            replacement: replacement.to_string(),
                        });
                        masked_emails += 1;
                        idx += matched_len;
                        utf16_pos += utf16_len;
                        continue;
                    }
                    EmailScanOutcome::Failed(skip_until) => {
                        email_failed_until = skip_until;
                    }
                }
            }
            if let Some((matched_len, replacement)) = Self::match_resident_id_at(bytes, idx) {
                scrubbed_text.push_str(replacement);
                spans.push(PiiReplacementSpan {
                    start_utf16: utf16_pos,
                    end_utf16: utf16_pos + matched_len,
                    replacement: replacement.to_string(),
                });
                masked_resident_ids += 1;
                idx += matched_len;
                utf16_pos += matched_len;
                continue;
            }
            if let Some((matched_len, replacement)) = Self::match_phone_at(bytes, idx) {
                scrubbed_text.push_str(replacement);
                spans.push(PiiReplacementSpan {
                    start_utf16: utf16_pos,
                    end_utf16: utf16_pos + matched_len,
                    replacement: replacement.to_string(),
                });
                masked_phone_numbers += 1;
                idx += matched_len;
                utf16_pos += matched_len;
                continue;
            }
            let ch = text[idx..].chars().next().unwrap();
            scrubbed_text.push(ch);
            idx += ch.len_utf8();
            utf16_pos += ch.len_utf16();
        }

        let stripped_metadata_fields = metadata_entries
            .iter()
            .filter(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty())
            .map(|(k, _)| k.clone())
            .collect();

        PiiAuditResult {
            scrubbed_text,
            masked_resident_ids,
            masked_phone_numbers,
            masked_emails,
            spans,
            stripped_metadata_fields,
        }
    }

    fn is_valid_rrn_date_and_gender(front6: &[u8], gender_digit: u8) -> bool {
        if front6.len() != 6 || !front6.iter().all(u8::is_ascii_digit) {
            return false;
        }
        let month = (front6[2] - b'0') * 10 + (front6[3] - b'0');
        let day = (front6[4] - b'0') * 10 + (front6[5] - b'0');
        (1..=12).contains(&month)
            && (1..=31).contains(&day)
            && (b'1'..=b'8').contains(&gender_digit)
    }

    fn match_resident_id_at(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        if idx > 0 && bytes[idx - 1].is_ascii_digit() {
            return None;
        }
        if idx + 14 <= bytes.len()
            && bytes[idx..idx + 6].iter().all(u8::is_ascii_digit)
            && bytes[idx + 6] == b'-'
            && bytes[idx + 7..idx + 14].iter().all(u8::is_ascii_digit)
            && Self::is_valid_rrn_date_and_gender(&bytes[idx..idx + 6], bytes[idx + 7])
            && (idx + 14 == bytes.len() || !bytes[idx + 14].is_ascii_digit())
        {
            return Some((14, "******-*******"));
        }
        if idx + 13 <= bytes.len()
            && bytes[idx..idx + 13].iter().all(u8::is_ascii_digit)
            && Self::is_valid_rrn_date_and_gender(&bytes[idx..idx + 6], bytes[idx + 6])
            && (idx + 13 == bytes.len() || !bytes[idx + 13].is_ascii_digit())
        {
            return Some((13, "******-*******"));
        }
        None
    }

    fn consume_digits(bytes: &[u8], pos: usize, min_len: usize, max_len: usize) -> Option<usize> {
        let mut end = pos;
        while end < bytes.len() && end - pos < max_len && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end - pos >= min_len {
            Some(end)
        } else {
            None
        }
    }

    fn mobile_mask_for_third_digit(third: u8) -> Option<&'static str> {
        match third {
            b'0' => Some("010-****-****"),
            b'1' => Some("011-****-****"),
            b'6' => Some("016-****-****"),
            b'7' => Some("017-****-****"),
            b'8' => Some("018-****-****"),
            b'9' => Some("019-****-****"),
            _ => None,
        }
    }

    fn is_phone_sep(b: u8) -> bool {
        matches!(b, b'-' | b'.' | b' ')
    }

    fn match_phone_at(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        if idx > 0 && (bytes[idx - 1].is_ascii_digit() || bytes[idx - 1] == b'+') {
            return None;
        }
        if idx + 3 <= bytes.len() && &bytes[idx..idx + 3] == b"+82" {
            let mut p = idx + 3;
            if p < bytes.len() && Self::is_phone_sep(bytes[p]) {
                p += 1;
            }
            let p_area = Self::consume_digits(bytes, p, 1, 3)?;
            let mut p2 = p_area;
            if p2 < bytes.len() && Self::is_phone_sep(bytes[p2]) {
                p2 += 1;
            }
            let p_mid = Self::consume_digits(bytes, p2, 3, 4)?;
            let mut p3 = p_mid;
            if p3 < bytes.len() && Self::is_phone_sep(bytes[p3]) {
                p3 += 1;
            }
            let p_end = Self::consume_digits(bytes, p3, 4, 4)?;
            if p_end == bytes.len() || !bytes[p_end].is_ascii_digit() {
                return Some((p_end - idx, "+82-**-****-****"));
            }
        }

        if idx + 3 <= bytes.len() && bytes[idx] == b'0' && bytes[idx + 1] == b'1' {
            if let Some(mask) = Self::mobile_mask_for_third_digit(bytes[idx + 2]) {
                let p = idx + 3;
                if p < bytes.len() && Self::is_phone_sep(bytes[p]) {
                    let sep = bytes[p];
                    if let Some(p_mid) = Self::consume_digits(bytes, p + 1, 3, 4) {
                        if p_mid < bytes.len() && bytes[p_mid] == sep {
                            if let Some(p_end) = Self::consume_digits(bytes, p_mid + 1, 4, 4) {
                                if p_end == bytes.len() || !bytes[p_end].is_ascii_digit() {
                                    return Some((p_end - idx, mask));
                                }
                            }
                        }
                    }
                } else if idx + 11 <= bytes.len()
                    && bytes[idx..idx + 11].iter().all(u8::is_ascii_digit)
                    && (idx + 11 == bytes.len() || !bytes[idx + 11].is_ascii_digit())
                {
                    return Some((11, mask));
                }
            }
        }

        if idx + 3 <= bytes.len()
            && bytes[idx] == b'0'
            && bytes[idx + 1] == b'2'
            && Self::is_phone_sep(bytes[idx + 2])
        {
            let sep = bytes[idx + 2];
            if let Some(p_mid) = Self::consume_digits(bytes, idx + 3, 3, 4) {
                if p_mid < bytes.len() && bytes[p_mid] == sep {
                    if let Some(p_end) = Self::consume_digits(bytes, p_mid + 1, 4, 4) {
                        if p_end == bytes.len() || !bytes[p_end].is_ascii_digit() {
                            return Some((p_end - idx, "02-****-****"));
                        }
                    }
                }
            }
        }

        if idx + 4 <= bytes.len()
            && bytes[idx] == b'0'
            && (b'3'..=b'6').contains(&bytes[idx + 1])
            && (b'1'..=b'5').contains(&bytes[idx + 2])
            && Self::is_phone_sep(bytes[idx + 3])
        {
            let sep = bytes[idx + 3];
            if let Some(p_mid) = Self::consume_digits(bytes, idx + 4, 3, 4) {
                if p_mid < bytes.len() && bytes[p_mid] == sep {
                    if let Some(p_end) = Self::consume_digits(bytes, p_mid + 1, 4, 4) {
                        if p_end == bytes.len() || !bytes[p_end].is_ascii_digit() {
                            return Some((p_end - idx, "***-****-****"));
                        }
                    }
                }
            }
        }

        None
    }

    fn is_email_local_non_dot(ch: char) -> bool {
        ch.is_alphanumeric() || matches!(ch, '_' | '%' | '+' | '-')
    }

    fn char_alpha_script(ch: char) -> u8 {
        if ch.is_ascii_alphabetic() {
            1
        } else if !ch.is_ascii() && ch.is_alphabetic() {
            2
        } else {
            0
        }
    }

    fn scan_email_at(text: &str, idx: usize) -> EmailScanOutcome {
        let Some(first_ch) = text[idx..].chars().next() else {
            return EmailScanOutcome::Failed(text.len());
        };
        let step_one = idx + first_ch.len_utf8();
        if !Self::is_email_local_non_dot(first_ch) {
            return EmailScanOutcome::Failed(step_one);
        }
        if idx > 0 {
            if let Some(prev_ch) = text[..idx].chars().next_back() {
                if prev_ch == '.' || matches!(prev_ch, '_' | '%' | '+' | '-') {
                    return EmailScanOutcome::Failed(step_one);
                }
                if prev_ch.is_alphanumeric() {
                    let prev_script = Self::char_alpha_script(prev_ch);
                    let cur_script = Self::char_alpha_script(first_ch);
                    if prev_script == 0 || cur_script == 0 || prev_script == cur_script {
                        return EmailScanOutcome::Failed(step_one);
                    }
                }
            }
        }

        let mut p = idx;
        let mut prev_was_dot = false;
        let mut atom_script = 0u8;
        while p < text.len() {
            let ch = text[p..].chars().next().unwrap();
            if ch == '.' {
                if prev_was_dot {
                    return EmailScanOutcome::Failed(p + 1);
                }
                prev_was_dot = true;
                atom_script = 0;
                p += 1;
                continue;
            }
            if !Self::is_email_local_non_dot(ch) {
                break;
            }
            let sc = Self::char_alpha_script(ch);
            if sc != 0 {
                if atom_script != 0 && atom_script != sc {
                    return EmailScanOutcome::Failed(p);
                }
                atom_script = sc;
            }
            prev_was_dot = false;
            p += ch.len_utf8();
        }

        if p >= text.len() || text.as_bytes()[p] != b'@' {
            return EmailScanOutcome::Failed(p.max(step_one));
        }
        let at_pos = p;
        if prev_was_dot {
            return EmailScanOutcome::Failed(at_pos + 1);
        }

        let dom_start = at_pos + 1;
        let Some(end_dom) = Self::scan_domain_after_at(text, dom_start) else {
            return EmailScanOutcome::Failed(at_pos + 1);
        };
        EmailScanOutcome::Matched(end_dom - idx)
    }

    fn scan_domain_after_at(text: &str, dom_start: usize) -> Option<usize> {
        if dom_start >= text.len() {
            return None;
        }
        let mut cursor = dom_start;
        let mut labels: Vec<(usize, usize, bool, usize)> = Vec::new();
        while cursor < text.len() {
            let label_start = cursor;
            let mut label_chars = 0usize;
            let mut all_alpha = true;
            let mut prev_hyphen = false;
            let mut label_script = 0u8;
            while cursor < text.len() {
                let ch = text[cursor..].chars().next().unwrap();
                if ch == '-' {
                    if cursor == label_start {
                        break;
                    }
                    all_alpha = false;
                    prev_hyphen = true;
                    cursor += 1;
                    label_chars += 1;
                    continue;
                }
                if !ch.is_alphanumeric() {
                    break;
                }
                let sc = Self::char_alpha_script(ch);
                if sc != 0 {
                    if label_script != 0 && label_script != sc {
                        break;
                    }
                    label_script = sc;
                } else {
                    all_alpha = false;
                }
                prev_hyphen = false;
                cursor += ch.len_utf8();
                label_chars += 1;
            }
            if cursor == label_start || prev_hyphen {
                break;
            }
            labels.push((label_start, cursor, all_alpha, label_chars));
            if cursor < text.len() && text.as_bytes()[cursor] == b'.' {
                let after_dot = cursor + 1;
                if after_dot < text.len() {
                    let next_ch = text[after_dot..].chars().next().unwrap();
                    if next_ch.is_alphanumeric() {
                        cursor = after_dot;
                        continue;
                    }
                }
            }
            break;
        }

        while labels.len() >= 2 {
            let &(_, end_pos, all_alpha, char_count) = labels.last().unwrap();
            if all_alpha && char_count >= 2 {
                return Some(end_pos);
            }
            labels.pop();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_smart_assistant_and_pii_scrubber() {
        let paragraphs = vec![
            "EasyOffice 보안 엔진은 매직 바이트 시그니처를 검증합니다. 압축 폭탄을 차단합니다."
                .to_string(),
            "TODO: 2026-10-30 기한까지 보안 결재 완료 요망.".to_string(),
            "이 문서는 자동화되어지다 표현을 포함합니다.".to_string(),
        ];

        let summary = LocalSmartAssistant::summarize_document(&paragraphs, 2);
        assert_eq!(summary.len(), 2);

        let diag = LocalSmartAssistant::diagnose_writing(&paragraphs.join(" "));
        assert!(diag.char_count > 20);
        assert!(diag.word_count >= 10);
        assert!(!diag.suggestions.is_empty());

        let actions = LocalSmartAssistant::extract_action_items(&paragraphs);
        assert_eq!(actions.len(), 1);
        assert!(actions[0].contains("TODO"));

        let before = vec!["1장 개요".to_string(), "2장 기존 설계".to_string()];
        let after = vec!["1장 개요".to_string(), "2장 신규 아키텍처".to_string()];
        let diff = LocalSmartAssistant::diff_documents_lcs(&before, &after);
        assert!(diff
            .iter()
            .any(|d| d.kind == DiffLineKind::Unchanged && d.text == "1장 개요"));
        assert!(diff
            .iter()
            .any(|d| d.kind == DiffLineKind::Removed && d.text == "2장 기존 설계"));
        assert!(diff
            .iter()
            .any(|d| d.kind == DiffLineKind::Added && d.text == "2장 신규 아키텍처"));

        let raw_pii = "담당자 😀 주민번호 900101-1234567 및 9203152345678, 연락처 010-1234-5678, 010 9876 5432, +82-10-1111-2222, 02-123-4567, 031-765-4321, 이메일 security@easyoffice.local 입니다.";
        let audit = DocumentSecurityAuditor::scan_and_scrub_pii(raw_pii);
        assert_eq!(audit.masked_resident_ids, 2);
        assert_eq!(audit.masked_phone_numbers, 5);
        assert_eq!(audit.masked_emails, 1);
        assert_eq!(audit.spans.len(), 8);
        let raw_utf16: Vec<u16> = raw_pii.encode_utf16().collect();
        let mut rebuilt: Vec<u16> = Vec::new();
        let mut cursor = 0usize;
        for sp in &audit.spans {
            rebuilt.extend_from_slice(&raw_utf16[cursor..sp.start_utf16]);
            rebuilt.extend(sp.replacement.encode_utf16());
            cursor = sp.end_utf16;
        }
        rebuilt.extend_from_slice(&raw_utf16[cursor..]);
        assert_eq!(String::from_utf16(&rebuilt).unwrap(), audit.scrubbed_text);

        let huge_before: Vec<String> = (0..1000).map(|i| format!("문단_{i}")).collect();
        let huge_after: Vec<String> = (0..1000).map(|i| format!("문단_{i}")).collect();
        let capped_diff = LocalSmartAssistant::diff_documents_lcs(&huge_before, &huge_after);
        assert_eq!(capped_diff.len(), 512);
    }

    #[test]
    fn test_shared_pii_vectors_and_linear_time_adversarial_inputs() {
        use easyoffice_formats::StrictJsonValue;
        use std::time::Instant;

        let raw_vectors = include_str!("../../../fixtures/pii-scrub-vectors.json");
        let parsed = StrictJsonValue::parse(raw_vectors).expect("valid vectors json");
        let StrictJsonValue::Array(items) = parsed else {
            panic!("expected top-level array");
        };
        assert!(items.len() >= 12);

        for item in &items {
            let StrictJsonValue::Object(fields) = item else {
                panic!("expected object item");
            };
            let id = match StrictJsonValue::get_field(fields, "id") {
                Some(StrictJsonValue::Str(s)) => s.as_str(),
                _ => panic!("missing vector id"),
            };
            let input = match StrictJsonValue::get_field(fields, "input") {
                Some(StrictJsonValue::Str(s)) => s.as_str(),
                _ => panic!("missing input for {id}"),
            };
            let expected_count = match StrictJsonValue::get_field(fields, "expectedCount") {
                Some(StrictJsonValue::Number(n)) => n.parse::<usize>().expect("valid usize"),
                _ => panic!("missing expectedCount for {id}"),
            };
            let expected_masked = match StrictJsonValue::get_field(fields, "expectedRustJsMasked") {
                Some(StrictJsonValue::Str(s)) => s.as_str(),
                _ => panic!("missing expectedRustJsMasked for {id}"),
            };
            let audit = DocumentSecurityAuditor::scan_and_scrub_pii(input);
            let total_masked =
                audit.masked_resident_ids + audit.masked_phone_numbers + audit.masked_emails;
            assert_eq!(
                total_masked, expected_count,
                "count mismatch on vector {id}"
            );
            assert_eq!(
                audit.scrubbed_text, expected_masked,
                "masked output mismatch on vector {id}"
            );
            assert!(
                audit.stripped_metadata_fields.is_empty(),
                "raw text scrub must report zero stripped metadata fields on {id}"
            );
            if let Some(StrictJsonValue::Array(forbidden)) =
                StrictJsonValue::get_field(fields, "forbiddenSubstrings")
            {
                for sub in forbidden {
                    if let StrictJsonValue::Str(token) = sub {
                        assert!(
                            !audit.scrubbed_text.contains(token.as_str()),
                            "forbidden substring {token} leaked on {id}"
                        );
                    }
                }
            }
        }

        let with_meta = DocumentSecurityAuditor::scan_and_scrub_pii_with_metadata(
            "담당자 011-123-4567",
            &[
                ("Author".to_string(), "홍길동".to_string()),
                ("Company".to_string(), "".to_string()),
                ("LastModifiedBy".to_string(), "보안팀".to_string()),
            ],
        );
        assert_eq!(
            with_meta.stripped_metadata_fields,
            vec!["Author".to_string(), "LastModifiedBy".to_string()]
        );

        let adversarial_inputs = [
            "a".repeat(262_144),
            "1-".repeat(131_072),
            format!("{}@", "a.".repeat(131_072)),
        ];
        for adv in &adversarial_inputs {
            let started = Instant::now();
            let res = DocumentSecurityAuditor::scan_and_scrub_pii(adv);
            let elapsed = started.elapsed();
            assert_eq!(res.spans.len(), 0);
            assert!(
                elapsed.as_millis() < 500,
                "linear-time scanner exceeded budget: {elapsed:?}"
            );
        }
    }
}
