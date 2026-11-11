//! Drawables floating on pages (page-layout documents keep their whole content there): text of
//! shapes and text boxes, pictures and tables are imported page by page, top to bottom and left
//! to right within a page; everything else is counted in the warnings.
//!
//! 검증 필요: the list layout (type 10010, page groups with drawable references) is from public
//! notes; the fixtures only carry an empty list, so it is checked with synthetic packages.

use std::collections::HashSet;

use super::{object_kind, reference, Reader, EMPTY_SHAPE};
use crate::pages_proto::Message;
use crate::pages_schema::{drawable, floating, geometry, ty};
use crate::{EasyOfficeError, RichBlock};

/// Most floating drawables read from one document.
const MAX_FLOATING: usize = 100_000;
/// Deepest `super` nesting walked to reach a drawable's geometry.
const MAX_SUPER_DEPTH: usize = 6;
/// Position of a drawable in points: `(x, y)`.
type Origin = (f32, f32);

/// Position of the drawable whose payload is `payload`: the geometry sits below a chain of
/// `super` messages (field 1) and starts with a position message of two floats.
fn origin(payload: &Message<'_>) -> Option<Origin> {
    let mut current = payload.clone();
    for _ in 0..MAX_SUPER_DEPTH {
        let inner = current.sub(drawable::GEOMETRY).ok().flatten()?;
        if let Some(position) = inner.sub(geometry::POSITION).ok().flatten() {
            if let (Some(x), Some(y)) = (position.float(1), position.float(2)) {
                return Some((x, y));
            }
        }
        current = inner;
    }
    None
}

/// Reading order key: top to bottom, then left to right; drawables without a position last.
fn order_key(origin: Option<Origin>) -> (u8, i64, i64) {
    origin.map_or((1, 0, 0), |(x, y)| {
        (0, (y * 4.0).round() as i64, (x * 4.0).round() as i64)
    })
}

impl Reader<'_> {
    /// Blocks of every floating drawable in page order.
    pub(super) fn floating_blocks(&mut self) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let mut list_ids: Vec<u64> = self
            .objects
            .objects
            .iter()
            .filter(|(_, o)| o.type_id == ty::FLOATING_DRAWABLES)
            .map(|(id, _)| *id)
            .collect();
        list_ids.sort_unstable();
        let mut pages: Vec<(u64, Vec<u64>)> = Vec::new();
        for id in list_ids {
            let Some(list) = self
                .objects
                .get(id)
                .and_then(|o| Message::parse(&o.payload).ok())
            else {
                continue;
            };
            for group in list.subs(floating::PAGE_GROUPS).unwrap_or_default() {
                let page = group.varint(floating::GROUP_PAGE).unwrap_or(0);
                let ids = group
                    .subs(floating::GROUP_DRAWABLES)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|e| reference(e, 1))
                    .collect();
                pages.push((page, ids));
            }
        }
        pages.sort_by_key(|(page, _)| *page);
        let mut seen = HashSet::new();
        let mut blocks = Vec::new();
        for (_, ids) in pages {
            let mut entries: Vec<(u64, Option<Origin>)> = Vec::with_capacity(ids.len());
            for id in ids {
                if !seen.insert(id) {
                    continue;
                }
                if seen.len() > MAX_FLOATING {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "Pages 쪽 위 개체 수 한도 초과".to_string(),
                    ));
                }
                let at = self
                    .objects
                    .get(id)
                    .and_then(|o| Message::parse(&o.payload).ok())
                    .and_then(|m| origin(&m));
                entries.push((id, at));
            }
            entries.sort_by_key(|(_, at)| order_key(*at));
            for (id, _) in entries {
                blocks.extend(self.floating_drawable_blocks(id)?);
            }
        }
        Ok(blocks)
    }

    fn floating_drawable_blocks(&mut self, id: u64) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let Some(obj) = self.objects.get(id) else {
            self.skip("쪽 위의 끊어진 개체 참조");
            return Ok(Vec::new());
        };
        match obj.type_id {
            ty::SHAPE_WITH_TEXT => {
                let blocks = self.shape_blocks(obj)?;
                if blocks.is_empty() {
                    self.skip(EMPTY_SHAPE);
                }
                Ok(blocks)
            }
            ty::IMAGE => Ok(self.read_picture(obj)?.into_iter().collect()),
            ty::TABLE_INFO => Ok(self
                .read_table(obj)?
                .map(RichBlock::Table)
                .into_iter()
                .collect()),
            other => {
                self.skip(format!(
                    "쪽 위의 개체: {}(형식 {other})",
                    object_kind(other)
                ));
                Ok(Vec::new())
            }
        }
    }
}
