//! Pre-parse structural limits for untrusted SVG documents (tags, `<filter>` abuse, `<use>` DAG
//! amplification). Shared between desktop PDF/thumbnail rendering and Quick Look (`raster.rs`) via
//! `#[path = ...]` so both enforce identical limits without adding parser crates to Quick Look.

use std::collections::HashMap;

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

/// Checks an SVG document against element, filter, and `<use>`-amplification budgets before
/// `usvg` parses it. Zero-allocation single-pass scan when no `<use>` element is present.
pub fn check_svg_structure(svg: &str) -> Result<(), &'static str> {
    let uses = scan_counts(svg.as_bytes())?;
    if uses > 0 {
        check_use_expansion(svg)?;
    }
    Ok(())
}

fn scan_counts(bytes: &[u8]) -> Result<usize, &'static str> {
    let mut tags = 0usize;
    let mut depth = 0usize;
    let mut filters = 0usize;
    let mut filter_primitives = 0usize;
    let mut uses = 0usize;
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
        }
        i = tag_end;
    }
    Ok(uses)
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

struct UseNode<'a> {
    direct: u64,
    refs: Vec<&'a str>,
}

fn check_use_expansion(svg: &str) -> Result<(), &'static str> {
    let bytes = svg.as_bytes();
    let mut nodes: Vec<UseNode<'_>> = vec![UseNode {
        direct: 0,
        refs: Vec::new(),
    }];
    let mut id_map: HashMap<&str, usize> = HashMap::new();
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
            let (elem_id, use_target) = parse_id_and_href(svg, name_end, attr_end, local == b"use");
            let parent = *id_stack.last().unwrap_or(&0);
            nodes[parent].direct = nodes[parent].direct.saturating_add(1);

            let mut pushed = false;
            let active = if let Some(id) = elem_id {
                let idx = nodes.len();
                nodes.push(UseNode {
                    direct: 1,
                    refs: Vec::new(),
                });
                id_map.entry(id).or_insert(idx);
                if !self_closing {
                    id_stack.push(idx);
                    pushed = true;
                }
                idx
            } else {
                parent
            };

            if let Some(target) = use_target {
                nodes[active].refs.push(target);
                if active != parent {
                    // When a `<use id="x" href="#y"/>` has its own id, the enclosing group also expands `#x`.
                    nodes[parent].refs.push(target);
                }
            }
            if !self_closing {
                pushed_id.push(pushed);
            }
        }
        i = tag_end;
    }

    verify_dag_budget(&nodes, &id_map)
}

fn parse_id_and_href(
    svg: &str,
    mut i: usize,
    end: usize,
    is_use: bool,
) -> (Option<&str>, Option<&str>) {
    let bytes = svg.as_bytes();
    let mut elem_id = None;
    let mut href = None;

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
        let val = if bytes[i] == b'"' || bytes[i] == b'\'' {
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

        if attr_name == b"id" && !val.trim().is_empty() && elem_id.is_none() {
            elem_id = Some(val.trim());
        } else if is_use && (attr_name == b"href" || attr_name == b"xlink:href") && href.is_none() {
            if let Some(fragment) = val.trim().strip_prefix('#') {
                if !fragment.is_empty() {
                    href = Some(fragment);
                }
            }
        }
    }
    (elem_id, href)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unvisited,
    Visiting,
    Done(u64),
}

fn verify_dag_budget(
    nodes: &[UseNode<'_>],
    id_map: &HashMap<&str, usize>,
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
            let refs = &nodes[idx].refs;
            if edge < refs.len() {
                stack.last_mut().expect("non-empty stack").1 += 1;
                if let Some(&target_idx) = id_map.get(refs[edge]) {
                    if matches!(state[target_idx], Visit::Unvisited) {
                        stack.push((target_idx, 0));
                    }
                }
            } else {
                let mut cost = nodes[idx].direct;
                for target_id in refs {
                    let sub = match id_map.get(target_id) {
                        Some(&target_idx) => match state[target_idx] {
                            Visit::Done(c) => c,
                            Visit::Visiting | Visit::Unvisited => 1,
                        },
                        None => 1,
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
}
