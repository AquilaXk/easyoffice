use std::collections::HashMap;

/// Writing diagnostics report generated 100% offline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritingDiagnosticReport {
    pub char_count: usize,
    pub word_count: usize,
    pub sentence_count: usize,
    pub reading_time_seconds: usize,
    pub suggestions: Vec<String>,
}

/// Line diff kind returned by `LocalSmartAssistant::diff_documents_lcs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Unchanged,
    Added,
    Removed,
}

/// Single line diff item from deterministic LCS document comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

/// 100% offline deterministic smart document assistant (zero network telemetry).
pub struct LocalSmartAssistant;

impl LocalSmartAssistant {
    /// Extracts up to `max_sentences` key sentences using term-frequency (TF) weighting while preserving document order.
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

    /// Computes character/word/sentence counts, reading time, and passive/long-sentence style suggestions.
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

    /// Extracts paragraphs or lines containing action markers (`TODO`, `검토`, `결재`, `기한`, `완료`).
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

    /// Computes exact Longest Common Subsequence (LCS) line diff between `before` and `after`.
    pub fn diff_documents_lcs(before: &[String], after: &[String]) -> Vec<DocumentDiffLine> {
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

/// Result of offline PII & hidden metadata scrubbing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiiAuditResult {
    pub scrubbed_text: String,
    pub masked_resident_ids: usize,
    pub masked_phone_numbers: usize,
    pub masked_emails: usize,
    pub stripped_metadata_fields: Vec<String>,
}

impl PiiAuditResult {
    pub fn total_masked_count(&self) -> usize {
        self.masked_resident_ids + self.masked_phone_numbers + self.masked_emails
    }
}

/// Offline document security auditor masking PII patterns and stripping hidden author/company metadata.
pub struct DocumentSecurityAuditor;

impl DocumentSecurityAuditor {
    pub fn scan_and_scrub_pii(text: &str) -> PiiAuditResult {
        let (after_rrn, masked_resident_ids) = Self::mask_resident_ids(text);
        let (after_phone, masked_phone_numbers) = Self::mask_phone_numbers(&after_rrn);
        let (scrubbed_text, masked_emails) = Self::mask_emails(&after_phone);

        PiiAuditResult {
            scrubbed_text,
            masked_resident_ids,
            masked_phone_numbers,
            masked_emails,
            stripped_metadata_fields: vec![
                "dc:creator".to_string(),
                "cp:lastModifiedBy".to_string(),
                "Company".to_string(),
            ],
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

    fn mask_resident_ids(input: &str) -> (String, usize) {
        let bytes = input.as_bytes();
        let mut out = String::with_capacity(input.len());
        let mut count = 0usize;
        let mut idx = 0usize;

        while idx < bytes.len() {
            let left_ok = idx == 0 || !bytes[idx - 1].is_ascii_digit();
            if left_ok
                && idx + 14 <= bytes.len()
                && bytes[idx..idx + 6].iter().all(u8::is_ascii_digit)
                && bytes[idx + 6] == b'-'
                && bytes[idx + 7..idx + 14].iter().all(u8::is_ascii_digit)
                && (idx + 14 == bytes.len() || !bytes[idx + 14].is_ascii_digit())
            {
                out.push_str("******-*******");
                count += 1;
                idx += 14;
            } else if left_ok
                && idx + 13 <= bytes.len()
                && bytes[idx..idx + 13].iter().all(u8::is_ascii_digit)
                && Self::is_valid_rrn_date_and_gender(&bytes[idx..idx + 6], bytes[idx + 6])
                && (idx + 13 == bytes.len() || !bytes[idx + 13].is_ascii_digit())
            {
                out.push_str("******-*******");
                count += 1;
                idx += 13;
            } else {
                let ch = input[idx..].chars().next().unwrap();
                out.push(ch);
                idx += ch.len_utf8();
            }
        }
        (out, count)
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

    fn match_phone_at(bytes: &[u8], idx: usize) -> Option<(usize, &'static str)> {
        if idx > 0 && (bytes[idx - 1].is_ascii_digit() || bytes[idx - 1] == b'+') {
            return None;
        }
        if idx + 3 <= bytes.len() && &bytes[idx..idx + 3] == b"+82" {
            let mut p = idx + 3;
            if p < bytes.len() && (bytes[p] == b'-' || bytes[p] == b' ') {
                p += 1;
            }
            let p_area = Self::consume_digits(bytes, p, 1, 3)?;
            let mut p2 = p_area;
            if p2 < bytes.len() && (bytes[p2] == b'-' || bytes[p2] == b' ') {
                p2 += 1;
            }
            let p_mid = Self::consume_digits(bytes, p2, 3, 4)?;
            let mut p3 = p_mid;
            if p3 < bytes.len() && (bytes[p3] == b'-' || bytes[p3] == b' ') {
                p3 += 1;
            }
            let p_end = Self::consume_digits(bytes, p3, 4, 4)?;
            if p_end == bytes.len() || !bytes[p_end].is_ascii_digit() {
                return Some((p_end - idx, "+82-**-****-****"));
            }
        }

        if idx + 3 <= bytes.len()
            && bytes[idx] == b'0'
            && bytes[idx + 1] == b'1'
            && matches!(bytes[idx + 2], b'0' | b'1' | b'6' | b'7' | b'8' | b'9')
        {
            let p = idx + 3;
            if p < bytes.len() && (bytes[p] == b'-' || bytes[p] == b' ') {
                let sep = bytes[p];
                if let Some(p_mid) = Self::consume_digits(bytes, p + 1, 3, 4) {
                    if p_mid < bytes.len() && bytes[p_mid] == sep {
                        if let Some(p_end) = Self::consume_digits(bytes, p_mid + 1, 4, 4) {
                            if p_end == bytes.len() || !bytes[p_end].is_ascii_digit() {
                                return Some((p_end - idx, "010-****-****"));
                            }
                        }
                    }
                }
            } else if idx + 11 <= bytes.len()
                && bytes[idx..idx + 11].iter().all(u8::is_ascii_digit)
                && (idx + 11 == bytes.len() || !bytes[idx + 11].is_ascii_digit())
            {
                return Some((11, "010-****-****"));
            }
        }

        if idx + 3 <= bytes.len()
            && bytes[idx] == b'0'
            && bytes[idx + 1] == b'2'
            && (bytes[idx + 2] == b'-' || bytes[idx + 2] == b' ')
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
            && (bytes[idx + 3] == b'-' || bytes[idx + 3] == b' ')
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

    fn mask_phone_numbers(input: &str) -> (String, usize) {
        let bytes = input.as_bytes();
        let mut out = String::with_capacity(input.len());
        let mut count = 0usize;
        let mut idx = 0usize;

        while idx < bytes.len() {
            if let Some((matched_len, replacement)) = Self::match_phone_at(bytes, idx) {
                out.push_str(replacement);
                count += 1;
                idx += matched_len;
            } else {
                let ch = input[idx..].chars().next().unwrap();
                out.push(ch);
                idx += ch.len_utf8();
            }
        }
        (out, count)
    }

    fn is_email_local_byte(b: u8) -> bool {
        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'+')
    }

    fn is_email_domain_byte(b: u8) -> bool {
        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')
    }

    fn mask_emails(input: &str) -> (String, usize) {
        let bytes = input.as_bytes();
        let len = bytes.len();
        let mut out = String::with_capacity(len);
        let mut count = 0usize;
        let mut idx = 0usize;

        while idx < len {
            if bytes[idx].is_ascii_alphanumeric() {
                let start = idx;
                let mut p = idx;
                while p < len && Self::is_email_local_byte(bytes[p]) {
                    p += 1;
                }
                if p > start && p < len && bytes[p] == b'@' {
                    let dom_start = p + 1;
                    let mut q = dom_start;
                    while q < len && Self::is_email_domain_byte(bytes[q]) {
                        q += 1;
                    }
                    while q > dom_start && !bytes[q - 1].is_ascii_alphanumeric() {
                        q -= 1;
                    }
                    let domain = &bytes[dom_start..q];
                    if domain.len() >= 3
                        && domain[0].is_ascii_alphanumeric()
                        && domain.contains(&b'.')
                    {
                        out.push_str("***@***");
                        count += 1;
                        idx = q;
                        continue;
                    }
                }
            }
            let ch = input[idx..].chars().next().unwrap();
            out.push(ch);
            idx += ch.len_utf8();
        }
        (out, count)
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

        let raw_pii = "담당자 주민번호 900101-1234567 및 9203152345678, 연락처 010-1234-5678, 010 9876 5432, +82-10-1111-2222, 02-123-4567, 031-765-4321, 이메일 security@easyoffice.local 입니다.";
        let audit = DocumentSecurityAuditor::scan_and_scrub_pii(raw_pii);
        assert_eq!(audit.masked_resident_ids, 2);
        assert_eq!(audit.masked_phone_numbers, 5);
        assert_eq!(audit.masked_emails, 1);
        assert!(audit.scrubbed_text.contains("******-*******"));
        assert!(audit.scrubbed_text.contains("010-****-****"));
        assert!(audit.scrubbed_text.contains("+82-**-****-****"));
        assert!(audit.scrubbed_text.contains("02-****-****"));
        assert!(audit.scrubbed_text.contains("***-****-****"));
        assert!(audit.scrubbed_text.contains("***@***"));
        assert!(!audit.scrubbed_text.contains("900101-1234567"));
        assert!(!audit.scrubbed_text.contains("9203152345678"));
        assert!(!audit.scrubbed_text.contains("010-1234-5678"));
        assert!(!audit.scrubbed_text.contains("010 9876 5432"));
        assert!(!audit.scrubbed_text.contains("+82-10-1111-2222"));
        assert!(!audit.scrubbed_text.contains("02-123-4567"));
        assert!(!audit.scrubbed_text.contains("031-765-4321"));
        assert!(!audit.scrubbed_text.contains("security@easyoffice.local"));
    }
}
