//! Objects parsed once per table (PD-361). A hostile file can name one large tile or stroke layer
//! from millions of entries; parsing it again for each entry costs minutes of CPU. The first
//! reference parses and charges the payload to the decoded-byte budget, later ones reuse the result.

use std::collections::hash_map::Entry;

use crate::decode_budget::DecodeBudget;
use crate::pages_proto::{ArchiveObject, FastU64Map, Message};
use crate::EasyOfficeError;

#[derive(Default)]
pub(super) struct ParsedObjects<'a> {
    /// `None` is an object whose payload does not parse; it is not parsed a second time either.
    messages: FastU64Map<Option<Message<'a>>>,
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
        match self.messages.entry(id) {
            Entry::Occupied(slot) => Ok(slot.into_mut().as_ref()),
            Entry::Vacant(slot) => {
                budget.charge(object.payload.len(), what)?;
                let parsed = match Message::parse(&object.payload) {
                    Ok(message) => Some(message),
                    Err(err @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(err),
                    Err(_) => None,
                };
                Ok(slot.insert(parsed).as_ref())
            }
        }
    }
}
