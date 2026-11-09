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

        let mut token_ids: HashMap<String, u32> = HashMap::new();
        let mut counts: Vec<usize> = Vec::new();
        let mut sentence_tokens: Vec<Vec<u32>> = Vec::with_capacity(sentences.len());

        for (_, sent) in &sentences {
            let mut ids = Vec::new();
            for token in sent.split(|c: char| !c.is_alphanumeric()) {
                if token.len() < 2 {
                    continue;
                }
                let lower = token.to_lowercase();
                if lower.chars().count() >= 2 {
                    let id = if let Some(&id) = token_ids.get(&lower) {
                        counts[id as usize] += 1;
                        id
                    } else {
                        let id = counts.len() as u32;
                        token_ids.insert(lower, id);
                        counts.push(1);
                        id
                    };
                    ids.push(id);
                }
            }
            sentence_tokens.push(ids);
        }

        let mut scored: Vec<(usize, usize, String)> = sentences
            .into_iter()
            .zip(sentence_tokens)
            .map(|((idx, sent), ids)| {
                let score: usize = ids.iter().map(|&id| counts[id as usize]).sum();
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

        let mut prefix = 0usize;
        let max_prefix = before.len().min(after.len());
        while prefix < max_prefix && before[prefix] == after[prefix] {
            prefix += 1;
        }
        let mut suffix = 0usize;
        while suffix < (before.len() - prefix).min(after.len() - prefix)
            && before[before.len() - 1 - suffix] == after[after.len() - 1 - suffix]
        {
            suffix += 1;
        }

        let mid_before = &before[prefix..before.len() - suffix];
        let mid_after = &after[prefix..after.len() - suffix];
        let m = mid_before.len();
        let n = mid_after.len();
        let mut out = Vec::with_capacity(before.len() + after.len());
        for s in &before[..prefix] {
            out.push(DocumentDiffLine {
                kind: DiffLineKind::Unchanged,
                text: s.clone(),
            });
        }

        if m > 0 && n > 0 {
            fn fnv1a_64(s: &str) -> u64 {
                let mut h = 0xcbf29ce484222325u64;
                for &b in s.as_bytes() {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
            let hb: Vec<u64> = mid_before.iter().map(|s| fnv1a_64(s)).collect();
            let ha: Vec<u64> = mid_after.iter().map(|s| fnv1a_64(s)).collect();
            let stride = n + 1;
            let mut dp = vec![0u16; (m + 1) * stride];

            for i in (0..m).rev() {
                let row_off = i * stride;
                let next_off = (i + 1) * stride;
                let h_bi = hb[i];
                let s_bi = &mid_before[i];
                for j in (0..n).rev() {
                    if h_bi == ha[j] && s_bi == &mid_after[j] {
                        dp[row_off + j] = 1 + dp[next_off + j + 1];
                    } else {
                        dp[row_off + j] = dp[next_off + j].max(dp[row_off + j + 1]);
                    }
                }
            }

            let mut i = 0;
            let mut j = 0;
            while i < m && j < n {
                if hb[i] == ha[j] && mid_before[i] == mid_after[j] {
                    out.push(DocumentDiffLine {
                        kind: DiffLineKind::Unchanged,
                        text: mid_before[i].clone(),
                    });
                    i += 1;
                    j += 1;
                } else if dp[(i + 1) * stride + j] >= dp[i * stride + (j + 1)] {
                    out.push(DocumentDiffLine {
                        kind: DiffLineKind::Removed,
                        text: mid_before[i].clone(),
                    });
                    i += 1;
                } else {
                    out.push(DocumentDiffLine {
                        kind: DiffLineKind::Added,
                        text: mid_after[j].clone(),
                    });
                    j += 1;
                }
            }
            while i < m {
                out.push(DocumentDiffLine {
                    kind: DiffLineKind::Removed,
                    text: mid_before[i].clone(),
                });
                i += 1;
            }
            while j < n {
                out.push(DocumentDiffLine {
                    kind: DiffLineKind::Added,
                    text: mid_after[j].clone(),
                });
                j += 1;
            }
        } else {
            for s in mid_before {
                out.push(DocumentDiffLine {
                    kind: DiffLineKind::Removed,
                    text: s.clone(),
                });
            }
            for s in mid_after {
                out.push(DocumentDiffLine {
                    kind: DiffLineKind::Added,
                    text: s.clone(),
                });
            }
        }

        for s in &before[before.len() - suffix..] {
            out.push(DocumentDiffLine {
                kind: DiffLineKind::Unchanged,
                text: s.clone(),
            });
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
    /// Card, bank account, passport, driver's licence, and business registration numbers.
    pub masked_identifier_numbers: usize,
    pub spans: Vec<PiiReplacementSpan>,
    pub stripped_metadata_fields: Vec<String>,
}

impl PiiAuditResult {
    pub fn total_masked_count(&self) -> usize {
        self.masked_resident_ids
            + self.masked_phone_numbers
            + self.masked_emails
            + self.masked_identifier_numbers
    }
}

enum EmailScanOutcome {
    Matched(usize),
    Failed(usize),
}

/// 256-byte lookup table for PII candidate start bytes:
/// - bit `0x01`: can start a resident ID, phone number, or identifier (`0-9`, `+`, `A-Z`)
/// - bit `0x02`: can start an email local part (`0-9`, `A-Z`, `a-z`, or non-ASCII UTF-8 lead byte)
const PII_START_LUT: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        let non_email = (b >= b'0' && b <= b'9') || b == b'+' || (b >= b'A' && b <= b'Z');
        let email = (b >= b'0' && b <= b'9')
            || (b >= b'A' && b <= b'Z')
            || (b >= b'a' && b <= b'z')
            || b >= 0x80;
        table[i] = (if non_email { 0x01 } else { 0 }) | (if email { 0x02 } else { 0 });
        i += 1;
    }
    table
};

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
        let mut masked_identifier_numbers = 0usize;
        let mut spans: Vec<PiiReplacementSpan> = Vec::new();
        let mut idx = 0usize;
        let mut plain_start = 0usize;
        let mut utf16_pos = 0usize;
        let mut email_failed_until = if bytes.contains(&b'@') {
            0
        } else {
            bytes.len()
        };

        while idx < bytes.len() {
            let b = bytes[idx];
            let flags = PII_START_LUT[b as usize];
            if flags == 0 {
                idx += 1;
                utf16_pos += 1;
                continue;
            }
            if (flags & 0x02) != 0 && idx >= email_failed_until {
                match Self::scan_email_at(text, idx) {
                    EmailScanOutcome::Matched(matched_len) => {
                        let utf16_len: usize = text[idx..idx + matched_len]
                            .chars()
                            .map(char::len_utf16)
                            .sum();
                        let replacement = "***@***";
                        if plain_start < idx {
                            scrubbed_text.push_str(&text[plain_start..idx]);
                        }
                        scrubbed_text.push_str(replacement);
                        spans.push(PiiReplacementSpan {
                            start_utf16: utf16_pos,
                            end_utf16: utf16_pos + utf16_len,
                            replacement: replacement.to_string(),
                        });
                        masked_emails += 1;
                        idx += matched_len;
                        plain_start = idx;
                        utf16_pos += utf16_len;
                        continue;
                    }
                    EmailScanOutcome::Failed(skip_until) => {
                        email_failed_until = skip_until;
                    }
                }
            }
            if (flags & 0x01) != 0 {
                if let Some((matched_len, replacement)) = Self::match_resident_id_at(bytes, idx) {
                    if plain_start < idx {
                        scrubbed_text.push_str(&text[plain_start..idx]);
                    }
                    scrubbed_text.push_str(replacement);
                    spans.push(PiiReplacementSpan {
                        start_utf16: utf16_pos,
                        end_utf16: utf16_pos + matched_len,
                        replacement: replacement.to_string(),
                    });
                    masked_resident_ids += 1;
                    idx += matched_len;
                    plain_start = idx;
                    utf16_pos += matched_len;
                    continue;
                }
                if let Some((matched_len, replacement)) = Self::match_phone_at(bytes, idx) {
                    if plain_start < idx {
                        scrubbed_text.push_str(&text[plain_start..idx]);
                    }
                    scrubbed_text.push_str(replacement);
                    spans.push(PiiReplacementSpan {
                        start_utf16: utf16_pos,
                        end_utf16: utf16_pos + matched_len,
                        replacement: replacement.to_string(),
                    });
                    masked_phone_numbers += 1;
                    idx += matched_len;
                    plain_start = idx;
                    utf16_pos += matched_len;
                    continue;
                }
                if let Some(matched_len) = crate::pii_identifiers::match_identifier_at(bytes, idx) {
                    let replacement =
                        crate::pii_identifiers::mask_identifier(&text[idx..idx + matched_len]);
                    if plain_start < idx {
                        scrubbed_text.push_str(&text[plain_start..idx]);
                    }
                    scrubbed_text.push_str(&replacement);
                    spans.push(PiiReplacementSpan {
                        start_utf16: utf16_pos,
                        end_utf16: utf16_pos + matched_len,
                        replacement,
                    });
                    masked_identifier_numbers += 1;
                    idx += matched_len;
                    plain_start = idx;
                    utf16_pos += matched_len;
                    continue;
                }
            }
            if b < 0x80 {
                idx += 1;
                utf16_pos += 1;
            } else {
                let ch = text[idx..].chars().next().unwrap();
                idx += ch.len_utf8();
                utf16_pos += ch.len_utf16();
            }
        }
        if plain_start < text.len() {
            scrubbed_text.push_str(&text[plain_start..]);
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
            masked_identifier_numbers,
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

    /// Index after a `3-4 digits, separator, 4 digits` tail starting at `start`, when it ends
    /// at a non-digit boundary.
    fn match_phone_tail(bytes: &[u8], start: usize, sep: u8) -> Option<usize> {
        let p_mid = Self::consume_digits(bytes, start, 3, 4)?;
        if bytes.get(p_mid) != Some(&sep) {
            return None;
        }
        let p_end = Self::consume_digits(bytes, p_mid + 1, 4, 4)?;
        (p_end == bytes.len() || !bytes[p_end].is_ascii_digit()).then_some(p_end)
    }

    /// Skips one optional phone separator at `p`.
    fn skip_optional_sep(bytes: &[u8], p: usize) -> usize {
        if p < bytes.len() && Self::is_phone_sep(bytes[p]) {
            p + 1
        } else {
            p
        }
    }

    fn match_intl_phone(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        if idx + 3 > bytes.len() || &bytes[idx..idx + 3] != b"+82" {
            return None;
        }
        let p = Self::skip_optional_sep(bytes, idx + 3);
        let p_area = Self::consume_digits(bytes, p, 1, 3)?;
        let p2 = Self::skip_optional_sep(bytes, p_area);
        let p_mid = Self::consume_digits(bytes, p2, 3, 4)?;
        let p3 = Self::skip_optional_sep(bytes, p_mid);
        let p_end = Self::consume_digits(bytes, p3, 4, 4)?;
        (p_end == bytes.len() || !bytes[p_end].is_ascii_digit())
            .then_some((p_end - idx, "+82-**-****-****"))
    }

    fn match_mobile_phone(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        if idx + 3 > bytes.len() || bytes[idx] != b'0' || bytes[idx + 1] != b'1' {
            return None;
        }
        let mask = Self::mobile_mask_for_third_digit(bytes[idx + 2])?;
        let p = idx + 3;
        if p < bytes.len() && Self::is_phone_sep(bytes[p]) {
            return Self::match_phone_tail(bytes, p + 1, bytes[p]).map(|end| (end - idx, mask));
        }
        let plain_digits = idx + 11 <= bytes.len()
            && bytes[idx..idx + 11].iter().all(u8::is_ascii_digit)
            && (idx + 11 == bytes.len() || !bytes[idx + 11].is_ascii_digit());
        plain_digits.then_some((11, mask))
    }

    fn match_seoul_phone(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        let is_prefix = idx + 3 <= bytes.len()
            && bytes[idx] == b'0'
            && bytes[idx + 1] == b'2'
            && Self::is_phone_sep(bytes[idx + 2]);
        if !is_prefix {
            return None;
        }
        Self::match_phone_tail(bytes, idx + 3, bytes[idx + 2])
            .map(|end| (end - idx, "02-****-****"))
    }

    fn match_regional_phone(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        let is_prefix = idx + 4 <= bytes.len()
            && bytes[idx] == b'0'
            && (b'3'..=b'6').contains(&bytes[idx + 1])
            && (b'1'..=b'5').contains(&bytes[idx + 2])
            && Self::is_phone_sep(bytes[idx + 3]);
        if !is_prefix {
            return None;
        }
        Self::match_phone_tail(bytes, idx + 4, bytes[idx + 3])
            .map(|end| (end - idx, "***-****-****"))
    }

    fn match_phone_at(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        if idx > 0 && (bytes[idx - 1].is_ascii_digit() || bytes[idx - 1] == b'+') {
            return None;
        }
        Self::match_intl_phone(bytes, idx)
            .or_else(|| Self::match_mobile_phone(bytes, idx))
            .or_else(|| Self::match_seoul_phone(bytes, idx))
            .or_else(|| Self::match_regional_phone(bytes, idx))
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

    /// Whether the character before `idx` rules out an address starting at `idx`.
    fn email_start_blocked(text: &str, idx: usize, first_ch: char) -> bool {
        let Some(prev_ch) = text[..idx].chars().next_back() else {
            return false;
        };
        if prev_ch == '.' || matches!(prev_ch, '_' | '%' | '+' | '-') {
            return true;
        }
        if !prev_ch.is_alphanumeric() {
            return false;
        }
        let prev_script = Self::char_alpha_script(prev_ch);
        let cur_script = Self::char_alpha_script(first_ch);
        prev_script == 0 || cur_script == 0 || prev_script == cur_script
    }

    /// Scans the local part from `idx`; returns `(end, ended_with_dot)` or the failure resume index.
    fn scan_email_local(text: &str, idx: usize) -> Result<(usize, bool), usize> {
        let mut p = idx;
        let mut prev_was_dot = false;
        let mut atom_script = 0u8;
        while p < text.len() {
            let ch = text[p..].chars().next().unwrap();
            if ch == '.' {
                if prev_was_dot {
                    return Err(p + 1);
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
                    return Err(p);
                }
                atom_script = sc;
            }
            prev_was_dot = false;
            p += ch.len_utf8();
        }
        Ok((p, prev_was_dot))
    }

    fn scan_email_at(text: &str, idx: usize) -> EmailScanOutcome {
        let Some(first_ch) = text[idx..].chars().next() else {
            return EmailScanOutcome::Failed(text.len());
        };
        let step_one = idx + first_ch.len_utf8();
        if !first_ch.is_alphanumeric() {
            return EmailScanOutcome::Failed(step_one);
        }
        if Self::email_start_blocked(text, idx, first_ch) {
            return EmailScanOutcome::Failed(step_one);
        }
        let (p, prev_was_dot) = match Self::scan_email_local(text, idx) {
            Ok(v) => v,
            Err(resume) => return EmailScanOutcome::Failed(resume),
        };
        if p >= text.len() || text.as_bytes()[p] != b'@' {
            return EmailScanOutcome::Failed(p.max(step_one));
        }
        let at_pos = p;
        if prev_was_dot {
            return EmailScanOutcome::Failed(at_pos + 1);
        }
        match Self::scan_domain_after_at(text, at_pos + 1) {
            Some(end_dom) => EmailScanOutcome::Matched(end_dom - idx),
            None => EmailScanOutcome::Failed(at_pos + 1),
        }
    }

    /// Scans one domain label at `label_start`.
    /// Returns `(end, all_alpha, char_count, ended_with_hyphen)`.
    fn scan_domain_label(text: &str, label_start: usize) -> (usize, bool, usize, bool) {
        let mut cursor = label_start;
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
            if sc == 0 {
                all_alpha = false;
            } else if label_script != 0 && label_script != sc {
                break;
            } else {
                label_script = sc;
            }
            prev_hyphen = false;
            cursor += ch.len_utf8();
            label_chars += 1;
        }
        (cursor, all_alpha, label_chars, prev_hyphen)
    }

    fn scan_domain_after_at(text: &str, dom_start: usize) -> Option<usize> {
        if dom_start >= text.len() {
            return None;
        }
        let mut cursor = dom_start;
        let mut labels: Vec<(usize, bool, usize)> = Vec::new();
        while cursor < text.len() {
            let (end, all_alpha, label_chars, prev_hyphen) = Self::scan_domain_label(text, cursor);
            if end == cursor || prev_hyphen {
                break;
            }
            labels.push((end, all_alpha, label_chars));
            cursor = end;
            let after_dot = cursor + 1;
            let continues = text.as_bytes().get(cursor) == Some(&b'.')
                && text[after_dot..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric);
            if !continues {
                break;
            }
            cursor = after_dot;
        }

        while labels.len() >= 2 {
            let &(end_pos, all_alpha, char_count) = labels.last().unwrap();
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
            let total_masked = audit.total_masked_count();
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

        // Linearity is checked by scaling (N vs 4N, best of several runs) so
        // machine load does not matter; a quadratic scanner would show ~16x.
        let builders: [fn(usize) -> String; 3] = [
            |n| "a".repeat(n),
            |n| "1-".repeat(n / 2),
            |n| format!("{}@", "a.".repeat(n / 2)),
        ];
        let best_of = |input: &str| {
            (0..5)
                .map(|_| {
                    let started = Instant::now();
                    let res = DocumentSecurityAuditor::scan_and_scrub_pii(input);
                    let elapsed = started.elapsed();
                    assert_eq!(res.spans.len(), 0);
                    elapsed
                })
                .min()
                .expect("at least one run")
        };
        for build in builders {
            let small = best_of(&build(65_536));
            let large = best_of(&build(262_144));
            // Floor avoids a meaningless ratio when the small run is ~0.
            let floor = std::time::Duration::from_millis(2);
            assert!(
                large < small.max(floor) * 10,
                "scanner scaling is super-linear: n={small:?} 4n={large:?}"
            );
            assert!(
                large.as_secs() < 10,
                "scanner exceeded backstop ceiling: {large:?}"
            );
        }
    }
}
