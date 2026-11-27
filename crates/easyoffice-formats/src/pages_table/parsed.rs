//! Objects parsed once per table (PD-363). A hostile file can name one large tile or stroke layer
//! from millions of entries; parsing it again for each entry costs minutes of CPU. The first
//! reference parses it and charges the decoded-byte budget; later references to it reuse the result.
//!
//! A parsed message is far larger than its payload (a 64 KiB payload of one-byte fields makes
//! about 1 MiB of fields), so only the last few results stay, and every parse is charged for the
//! payload and for the fields it builds. Many distinct objects therefore neither pile up in memory
//! nor go unpaid, and an attack that alternates between more objects than the cache holds pays the
//! budget for each parse and ends in `ResourceBudgetExceeded`.

use crate::decode_budget::DecodeBudget;
use crate::pages_proto::{ArchiveObject, Field, Message};
use crate::EasyOfficeError;

/// Parsed objects kept; the oldest goes first.
const CACHED_OBJECTS: usize = 4;

#[derive(Default)]
pub(super) struct ParsedObjects<'a> {
    /// `None` is an object whose payload does not parse; it is not parsed a second time either.
    entries: Vec<(u64, Option<Message<'a>>)>,
    /// Payloads parsed so far (tests count them).
    #[cfg(test)]
    parses: usize,
}

impl<'a> ParsedObjects<'a> {
    /// The message of object `id`, parsed on first use. `Ok(None)` when its payload is malformed;
    /// a budget failure is an error.
    pub(super) fn get(
        &mut self,
        id: u64,
        object: &'a ArchiveObject,
        budget: &mut DecodeBudget,
        what: &str,
    ) -> Result<Option<&Message<'a>>, EasyOfficeError> {
        if let Some(at) = self.entries.iter().position(|(cached, _)| *cached == id) {
            return Ok(self.entries[at].1.as_ref());
        }
        budget.charge(object.payload.len(), what)?;
        let parsed = match Message::parse(&object.payload) {
            Ok(message) => {
                budget.charge(
                    message.fields.capacity() * std::mem::size_of::<Field>(),
                    what,
                )?;
                Some(message)
            }
            Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(err),
            Err(_) => None,
        };
        #[cfg(test)]
        {
            self.parses += 1;
        }
        if self.entries.len() == CACHED_OBJECTS {
            self.entries.remove(0);
        }
        self.entries.push((id, parsed));
        Ok(self.entries.last().and_then(|(_, m)| m.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload of `fields` one-byte varint fields (`08 01` repeated).
    fn object(fields: usize) -> ArchiveObject {
        ArchiveObject {
            type_id: 0,
            payload: [0x08u8, 0x01].repeat(fields).into(),
            extra: Vec::new(),
        }
    }

    #[test]
    fn one_object_named_again_and_again_is_parsed_once() {
        let obj = object(30_000);
        let mut parsed = ParsedObjects::default();
        let mut budget = DecodeBudget::new();
        for _ in 0..10_000 {
            assert!(parsed.get(7, &obj, &mut budget, "t").unwrap().is_some());
        }
        assert_eq!(parsed.parses, 1);
        // one payload and its fields are paid for once
        let used = DecodeBudget::new().remaining() - budget.remaining();
        assert!(used < 2 * 1024 * 1024, "{used}");
    }

    #[test]
    fn a_malformed_object_is_not_parsed_twice_either() {
        let obj = ArchiveObject {
            type_id: 0,
            payload: vec![0x08].into(),
            extra: Vec::new(),
        };
        let mut parsed = ParsedObjects::default();
        let mut budget = DecodeBudget::new();
        for _ in 0..100 {
            assert!(parsed.get(1, &obj, &mut budget, "t").unwrap().is_none());
        }
        assert_eq!(parsed.parses, 1);
    }

    #[test]
    fn many_distinct_objects_hit_the_budget_and_keep_few_results() {
        // 64 KiB payloads of one-byte fields: 32,768 fields, about 1 MiB each once parsed
        let objects: Vec<ArchiveObject> = (0..200).map(|_| object(32_768)).collect();
        let mut parsed = ParsedObjects::default();
        // the payloads alone (200 x 64 KiB = 12.5 MiB) fit; the parsed fields (200 MiB) do not
        let mut budget = DecodeBudget::with_limit(64 * 1024 * 1024);
        let mut failed_at = None;
        for (id, obj) in (0u64..).zip(&objects) {
            match parsed.get(id, obj, &mut budget, "t") {
                Ok(_) => assert!(parsed.entries.len() <= CACHED_OBJECTS),
                Err(err) => {
                    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));
                    failed_at = Some(id);
                    break;
                }
            }
        }
        assert!(failed_at.is_some_and(|id| id < 100), "{failed_at:?}");
    }

    #[test]
    fn alternating_between_more_objects_than_the_cache_holds_ends_in_the_budget() {
        let objects: Vec<ArchiveObject> = (0..CACHED_OBJECTS + 1).map(|_| object(32_768)).collect();
        let mut parsed = ParsedObjects::default();
        let mut budget = DecodeBudget::with_limit(64 * 1024 * 1024);
        let mut result = Ok(());
        for round in 0..1_000 {
            let at = round % objects.len();
            if let Err(err) = parsed.get(at as u64, &objects[at], &mut budget, "t") {
                result = Err(err);
                break;
            }
        }
        assert!(matches!(
            result,
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}
