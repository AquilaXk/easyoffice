//! Pre-parse structural limits for untrusted SVG documents (tags, `<filter>` abuse, `<use>` DAG
//! amplification, and nested SVG picture budgets). Shared between desktop PDF/thumbnail rendering
//! and Quick Look (`raster.rs`) via `#[path = ...]` so both enforce identical limits without
//! adding parser crates to Quick Look.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Maximum opening/self-closing XML tags allowed in one SVG document or nested SVG picture.
pub const MAX_SVG_TAGS: usize = 100_000;
/// Maximum XML element nesting depth allowed before `usvg` recurses on untrusted input.
pub const MAX_SVG_DEPTH: usize = 1_024;
/// Maximum `<filter>` elements allowed in one SVG.
pub const MAX_SVG_FILTERS: usize = 64;
/// Maximum `<fe*>` filter primitive elements allowed across all filters in one SVG.
pub const MAX_SVG_FILTER_PRIMITIVES: usize = 256;
/// Maximum `<use>` elements allowed in one SVG.
pub const MAX_SVG_USES: usize = 4_096;
/// Maximum expanded `<use>` instances reachable from the document root.
pub const MAX_SVG_USE_EXPANSION: u64 = 32_768;

/// Deepest `<image>` inside `<image>` of SVG data URLs that is kept.
pub const MAX_NESTED_SVG_DEPTH: u32 = 4;
/// Maximum nested SVG images allowed across a single page.
pub const MAX_NESTED_SVG_COUNT: usize = 32;
/// Total cumulative bytes of nested SVG images admitted across a single page.
pub const MAX_NESTED_SVG_TOTAL_BYTES: usize = 32 * 1_048_576;
/// Largest nested SVG picture once inflated (bytes).
pub const MAX_NESTED_SVG_BYTES: usize = 16 * 1_048_576;

/// Shared per-page budget for nested SVG pictures embedded in data URLs.
#[derive(Debug)]
pub struct NestedSvgBudget {
    depth: AtomicU32,
    count: AtomicUsize,
    total_bytes: AtomicUsize,
}

pub struct NestedSvgGuard<'a> {
    budget: &'a NestedSvgBudget,
}

impl Drop for NestedSvgGuard<'_> {
    fn drop(&mut self) {
        self.budget.depth.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Default for NestedSvgBudget {
    fn default() -> Self {
        Self {
            depth: AtomicU32::new(0),
            count: AtomicUsize::new(0),
            total_bytes: AtomicUsize::new(0),
        }
    }
}

impl NestedSvgBudget {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves a nested SVG slot if within raw size, depth, and per-page count limits.
    /// Call [`NestedSvgGuard::admit_bytes`] after inflating compressed SVGZ bytes to enforce the
    /// cumulative decompressed byte budget.
    pub fn enter(&self, raw_bytes: usize) -> Option<NestedSvgGuard<'_>> {
        if raw_bytes > MAX_NESTED_SVG_BYTES {
            return None;
        }
        let cur_depth = self.depth.fetch_add(1, Ordering::Relaxed);
        if cur_depth >= MAX_NESTED_SVG_DEPTH {
            self.depth.fetch_sub(1, Ordering::Relaxed);
            return None;
        }
        let cur_count = self.count.fetch_add(1, Ordering::Relaxed);
        if cur_count >= MAX_NESTED_SVG_COUNT {
            self.count.fetch_sub(1, Ordering::Relaxed);
            self.depth.fetch_sub(1, Ordering::Relaxed);
            return None;
        }
        Some(NestedSvgGuard { budget: self })
    }
}

impl NestedSvgGuard<'_> {
    /// Charges the decompressed SVG text length against [`MAX_NESTED_SVG_TOTAL_BYTES`].
    pub fn admit_bytes(&self, inflated_bytes: usize) -> Option<()> {
        if inflated_bytes > MAX_NESTED_SVG_BYTES {
            return None;
        }
        let prev = self
            .budget
            .total_bytes
            .fetch_add(inflated_bytes, Ordering::Relaxed);
        if prev.saturating_add(inflated_bytes) > MAX_NESTED_SVG_TOTAL_BYTES {
            self.budget
                .total_bytes
                .fetch_sub(inflated_bytes, Ordering::Relaxed);
            return None;
        }
        Some(())
    }
}

/// Checks an SVG document against element, filter, and `<use>`-amplification budgets before
/// `usvg` parses it.
pub fn check_svg_structure(svg: &str) -> Result<(), &'static str> {
    let (uses, has_url_refs) = scan_counts(svg.as_bytes())?;
    if uses > 0 || has_url_refs {
        check_use_expansion(svg)?;
    }
    Ok(())
}

fn scan_counts(bytes: &[u8]) -> Result<(usize, bool), &'static str> {
    let mut tags = 0usize;
    let mut depth = 0usize;
    let mut filters = 0usize;
    let mut filter_primitives = 0usize;
    let mut uses = 0usize;
    let mut has_url_refs = false;
    let mut i = 0usize;

    while i < bytes.len() {
        let Some(rel) = find_byte(bytes, i, b'<') else {
            break;
        };
        i += rel;
        if let Some(next) = skip_non_element(bytes, i) {
            i = next;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'/') {
            depth = depth.saturating_sub(1);
            i = skip_tag_end(bytes, i + 2);
            continue;
        }
        let (local, name_end) = read_local_name(bytes, i + 1);
        let tag_end = skip_tag_end(bytes, name_end);
        let self_closing = tag_end > name_end
            && bytes[tag_end - 1] == b'>'
            && tag_end >= name_end + 2
            && bytes[tag_end - 2] == b'/';
        if !local.is_empty() {
            tags += 1;
            if tags > MAX_SVG_TAGS {
                return Err("SVG 요소 수가 허용 한도를 초과했습니다.");
            }
            if !self_closing {
                depth += 1;
                if depth > MAX_SVG_DEPTH {
                    return Err("SVG 중첩 깊이가 허용 한도를 초과했습니다.");
                }
            }
            if local == b"filter" {
                filters += 1;
                if filters > MAX_SVG_FILTERS {
                    return Err("SVG 필터 수가 허용 한도를 초과했습니다.");
                }
            } else if is_filter_primitive(local) {
                filter_primitives += 1;
                if filter_primitives > MAX_SVG_FILTER_PRIMITIVES {
                    return Err("SVG 필터 프리미티브 수가 허용 한도를 초과했습니다.");
                }
            } else if local == b"use" {
                uses += 1;
                if uses > MAX_SVG_USES {
                    return Err("SVG <use> 참조 수가 허용 한도를 초과했습니다.");
                }
            }
            if !has_url_refs && bytes[name_end..tag_end].windows(4).any(|w| w == b"url(") {
                has_url_refs = true;
            }
        }
        i = tag_end;
    }
    Ok((uses, has_url_refs))
}

#[inline]
fn find_byte(haystack: &[u8], start: usize, needle: u8) -> Option<usize> {
    haystack[start..].iter().position(|&b| b == needle)
}

fn find_bytes(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    haystack[start..]
        .windows(needle.len())
        .position(|w| w == needle)
}

fn skip_non_element(bytes: &[u8], lt: usize) -> Option<usize> {
    let rest = &bytes[lt..];
    if rest.starts_with(b"<!--") {
        let end = find_bytes(bytes, lt + 4, b"-->").map_or(bytes.len(), |off| lt + 4 + off + 3);
        return Some(end);
    }
    if rest.starts_with(b"<![CDATA[") {
        let end = find_bytes(bytes, lt + 9, b"]]>").map_or(bytes.len(), |off| lt + 9 + off + 3);
        return Some(end);
    }
    if rest.starts_with(b"<?") {
        let end = find_bytes(bytes, lt + 2, b"?>").map_or(bytes.len(), |off| lt + 2 + off + 2);
        return Some(end);
    }
    if rest.starts_with(b"<!") {
        return Some(skip_tag_end(bytes, lt + 2));
    }
    None
}

fn skip_tag_end(bytes: &[u8], mut i: usize) -> usize {
    let mut quote = 0u8;
    while i < bytes.len() {
        let b = bytes[i];
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
        } else if b == b'"' || b == b'\'' {
            quote = b;
        } else if b == b'>' {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn read_local_name(bytes: &[u8], start: usize) -> (&[u8], usize) {
    let mut end = start;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_whitespace() || b == b'/' || b == b'>' {
            break;
        }
        end += 1;
    }
    let full = &bytes[start..end];
    let local = match full.iter().rposition(|&b| b == b':') {
        Some(pos) => &full[pos + 1..],
        None => full,
    };
    (local, end)
}

#[inline]
fn is_filter_primitive(local: &[u8]) -> bool {
    local.starts_with(b"fe") && local.get(2).is_some_and(u8::is_ascii_uppercase)
}

struct UseNode {
    direct: u64,
    refs: Vec<String>,
    child_indices: Vec<usize>,
}

fn check_use_expansion(svg: &str) -> Result<(), &'static str> {
    let bytes = svg.as_bytes();
    let mut nodes: Vec<UseNode> = vec![UseNode {
        direct: 0,
        refs: Vec::new(),
        child_indices: Vec::new(),
    }];
    let mut id_map: HashMap<String, usize> = HashMap::new();
    let mut id_stack: Vec<usize> = vec![0];
    let mut pushed_id: Vec<bool> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let Some(rel) = find_byte(bytes, i, b'<') else {
            break;
        };
        i += rel;
        if let Some(next) = skip_non_element(bytes, i) {
            i = next;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'/') {
            if pushed_id.pop() == Some(true) && id_stack.len() > 1 {
                id_stack.pop();
            }
            i = skip_tag_end(bytes, i + 2);
            continue;
        }
        let (local, name_end) = read_local_name(bytes, i + 1);
        let tag_end = skip_tag_end(bytes, name_end);
        let attr_end = if tag_end > name_end && bytes[tag_end - 1] == b'>' {
            let before_gt = tag_end - 1;
            if before_gt > name_end && bytes[before_gt - 1] == b'/' {
                before_gt - 1
            } else {
                before_gt
            }
        } else {
            tag_end
        };
        let self_closing = tag_end > name_end
            && bytes[tag_end - 1] == b'>'
            && tag_end >= name_end + 2
            && bytes[tag_end - 2] == b'/';

        if !local.is_empty() {
            let (elem_id, refs) = parse_id_and_refs(svg, name_end, attr_end, local == b"use");
            let parent = *id_stack.last().unwrap_or(&0);

            let mut pushed = false;
            let active = if let Some(id) = elem_id {
                let idx = nodes.len();
                nodes.push(UseNode {
                    direct: 1,
                    refs: Vec::new(),
                    child_indices: Vec::new(),
                });
                id_map.entry(id).or_insert(idx);
                nodes[parent].child_indices.push(idx);
                if !self_closing {
                    id_stack.push(idx);
                    pushed = true;
                }
                idx
            } else {
                nodes[parent].direct = nodes[parent].direct.saturating_add(1);
                parent
            };

            nodes[active].refs.extend(refs);

            if !self_closing {
                pushed_id.push(pushed);
            }
        }
        i = tag_end;
    }

    verify_dag_budget(&nodes, &id_map)
}

fn decode_xml_entities(input: &str) -> Cow<'_, str> {
    if !input.contains('&') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi) = bytes[i..].iter().position(|&b| b == b';') {
                let entity = &input[i + 1..i + semi];
                let decoded = if let Some(hex) = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                {
                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    match entity {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => None,
                    }
                };
                if let Some(ch) = decoded {
                    out.push(ch);
                    i += semi + 1;
                    continue;
                }
            }
        }
        if let Some(ch) = input[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    Cow::Owned(out)
}

fn extract_url_fragments(val: &str, refs: &mut Vec<String>) {
    let mut cursor = val;
    while let Some(pos) = cursor.find("url(") {
        let rest = &cursor[pos + 4..];
        if let Some(close) = rest.find(')') {
            let inner = rest[..close]
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .trim();
            if let Some(fragment) = inner.strip_prefix('#') {
                if !fragment.is_empty() && !refs.iter().any(|r| r == fragment) {
                    refs.push(fragment.to_string());
                }
            }
            cursor = &rest[close + 1..];
        } else {
            break;
        }
    }
}

fn parse_id_and_refs(
    svg: &str,
    mut i: usize,
    end: usize,
    is_use: bool,
) -> (Option<String>, Vec<String>) {
    let bytes = svg.as_bytes();
    let mut elem_id = None;
    let mut refs = Vec::new();

    while i < end {
        while i < end && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            break;
        }
        let name_start = i;
        while i < end && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let attr_name = &bytes[name_start..i];
        while i < end && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < end && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            break;
        }
        let val_raw = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let val_start = i;
            while i < end && bytes[i] != quote {
                i += 1;
            }
            let val_end = i;
            if i < end {
                i += 1;
            }
            &svg[val_start..val_end]
        } else {
            let val_start = i;
            while i < end && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            &svg[val_start..i]
        };

        let val = decode_xml_entities(val_raw);
        let val_trimmed = val.trim();

        if attr_name == b"id" && !val_trimmed.is_empty() && elem_id.is_none() {
            elem_id = Some(val_trimmed.to_string());
        } else if is_use && (attr_name == b"href" || attr_name.ends_with(b":href")) {
            if let Some(fragment) = val_trimmed.strip_prefix('#') {
                if !fragment.is_empty() && !refs.iter().any(|r| r == fragment) {
                    refs.push(fragment.to_string());
                }
            }
        }

        extract_url_fragments(&val, &mut refs);
    }
    (elem_id, refs)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unvisited,
    Visiting,
    Done(u64),
}

fn verify_dag_budget(
    nodes: &[UseNode],
    id_map: &HashMap<String, usize>,
) -> Result<(), &'static str> {
    let mut state = vec![Visit::Unvisited; nodes.len()];
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for start in 0..nodes.len() {
        if !matches!(state[start], Visit::Unvisited) {
            continue;
        }
        stack.push((start, 0));
        while let Some(&(idx, edge)) = stack.last() {
            if edge == 0 {
                match state[idx] {
                    Visit::Done(_) | Visit::Visiting => {
                        stack.pop();
                        continue;
                    }
                    Visit::Unvisited => state[idx] = Visit::Visiting,
                }
            }
            let total_edges = nodes[idx].refs.len() + nodes[idx].child_indices.len();
            if edge < total_edges {
                stack.last_mut().expect("non-empty stack").1 += 1;
                let target_idx = if edge < nodes[idx].refs.len() {
                    id_map.get(&nodes[idx].refs[edge]).copied()
                } else {
                    Some(nodes[idx].child_indices[edge - nodes[idx].refs.len()])
                };
                if let Some(next_idx) = target_idx {
                    if matches!(state[next_idx], Visit::Unvisited) {
                        stack.push((next_idx, 0));
                    }
                }
            } else {
                let mut cost = nodes[idx].direct;
                for target_id in &nodes[idx].refs {
                    let sub = match id_map.get(target_id) {
                        Some(&target_idx) => match state[target_idx] {
                            Visit::Done(c) => c,
                            Visit::Visiting | Visit::Unvisited => 1,
                        },
                        None => 1,
                    };
                    cost = cost.saturating_add(sub);
                }
                for &child_idx in &nodes[idx].child_indices {
                    let sub = match state[child_idx] {
                        Visit::Done(c) => c,
                        Visit::Visiting | Visit::Unvisited => 1,
                    };
                    cost = cost.saturating_add(sub);
                }
                if cost > MAX_SVG_USE_EXPANSION {
                    return Err("SVG <use> 확장 노드 수가 허용 한도를 초과했습니다.");
                }
                state[idx] = Visit::Done(cost);
                stack.pop();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_cdata_quoted_gt_and_use_cycles_stay_bounded() {
        let benign = r##"<svg id="root" xmlns="http://www.w3.org/2000/svg"><!-- <filter/><use href="#missing"/> --><![CDATA[<use href="#x"/>]]><defs><g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g></defs><path d="M 0 0" data-expr="a > b"/><use href="#a"/></svg>"##;
        check_svg_structure(benign).unwrap();

        let root_wrapped_bomb = {
            let mut defs = String::from(r#"<g id="u0"><rect width="1" height="1"/></g>"#);
            for lvl in 1..=16 {
                let prev = lvl - 1;
                defs.push_str(&format!(
                    r##"<g id="u{lvl}"><use href="#u{prev}"/><use href="#u{prev}"/></g>"##
                ));
            }
            format!(
                r##"<svg id="doc" xmlns="http://www.w3.org/2000/svg"><defs>{defs}</defs><use href="#u16"/></svg>"##
            )
        };
        assert!(check_svg_structure(&root_wrapped_bomb).is_err());

        let deep = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">{}{}</svg>",
            "<g>".repeat(MAX_SVG_DEPTH + 5),
            "</g>".repeat(MAX_SVG_DEPTH + 5)
        );
        assert!(check_svg_structure(&deep).is_err());
    }

    #[test]
    fn nested_id_cost_propagates_to_parent_and_amplification_is_refused() {
        // Nested id structure where outer group contains an inner id with many elements
        let mut inner_shapes = String::new();
        for _ in 0..1000 {
            inner_shapes.push_str(r#"<circle cx="1" cy="1" r="1"/>"#);
        }
        let mut uses = String::new();
        for _ in 0..40 {
            uses.push_str(r##"<use href="#outer"/>"##);
        }
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><g id="outer"><g id="inner">{inner_shapes}</g></g></defs>{uses}</svg>"##
        );
        assert!(
            check_svg_structure(&svg).is_err(),
            "Nested id expansion must be counted towards parent"
        );
    }

    #[test]
    fn custom_namespace_href_and_entity_decoding_are_detected() {
        // Namespace prefix like ns:href
        let ns_svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><g id="b"><rect width="1" height="1"/></g></defs><use custom:href="#b"/></svg>"##;
        assert!(check_svg_structure(ns_svg).is_ok());

        // Entity encoded hex href: &#x23; == '#'
        let entity_bomb = {
            let mut defs = String::from(r#"<g id="u0"><rect width="1" height="1"/></g>"#);
            for lvl in 1..=16 {
                let prev = lvl - 1;
                defs.push_str(&format!(
                    r##"<g id="u{lvl}"><use href="&#x23;u{prev}"/><use href="&#35;u{prev}"/></g>"##
                ));
            }
            format!(
                r##"<svg id="doc" xmlns="http://www.w3.org/2000/svg"><defs>{defs}</defs><use href="&#x23;u16"/></svg>"##
            )
        };
        assert!(
            check_svg_structure(&entity_bomb).is_err(),
            "Entity-encoded href bomb must be refused"
        );
    }

    #[test]
    fn url_attribute_references_are_tracked_in_dag() {
        let filter_bomb = {
            let mut defs = String::from(r#"<filter id="f0"><feGaussianBlur/></filter>"#);
            for lvl in 1..=16 {
                let prev = lvl - 1;
                defs.push_str(&format!(
                    r##"<g id="f{lvl}" filter="url(#f{prev})"><rect width="1" height="1" filter="url(&#x23;f{prev})"/></g>"##
                ));
            }
            format!(
                r##"<svg id="doc" xmlns="http://www.w3.org/2000/svg"><defs>{defs}</defs><use href="#f16"/></svg>"##
            )
        };
        assert!(
            check_svg_structure(&filter_bomb).is_err(),
            "DAG amplification via url() attributes must be refused"
        );
    }

    #[test]
    fn nested_svg_budget_enforces_depth_count_and_byte_limits() {
        let budget = NestedSvgBudget::new();
        // Depth test
        let g1 = budget.enter(100).expect("depth 0");
        let g2 = budget.enter(100).expect("depth 1");
        let g3 = budget.enter(100).expect("depth 2");
        let g4 = budget.enter(100).expect("depth 3");
        assert!(
            budget.enter(100).is_none(),
            "Depth 4 exceeds MAX_NESTED_SVG_DEPTH"
        );
        drop(g4);
        let g4_again = budget.enter(100).expect("re-entering after drop succeeds");
        drop(g4_again);
        drop(g3);
        drop(g2);
        drop(g1);

        // Count test (sibling SVGs on the same page)
        let count_budget = NestedSvgBudget::new();
        for _ in 0..MAX_NESTED_SVG_COUNT {
            let guard = count_budget.enter(10).expect("within count");
            drop(guard);
        }
        assert!(count_budget.enter(10).is_none(), "Count limit exceeded");

        // Cumulative inflated byte test (e.g. small compressed SVGZ payloads inflating to 2 MiB each)
        let byte_budget = NestedSvgBudget::new();
        let raw_chunk = 1024;
        let inflated_chunk = 2 * 1_048_576;
        for _ in 0..16 {
            let guard = byte_budget.enter(raw_chunk).expect("within count limit");
            guard
                .admit_bytes(inflated_chunk)
                .expect("within 32 MiB inflated limit");
            drop(guard);
        }
        let guard = byte_budget.enter(raw_chunk).expect("17th slot allowed");
        assert!(
            guard.admit_bytes(inflated_chunk).is_none(),
            "Cumulative inflated bytes exceed 32MiB"
        );
    }
}
