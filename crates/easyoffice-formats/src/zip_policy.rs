//! Size budget of ZIP containers (OOXML, ODF, HWPX), Fail-Closed against decompression bombs.
//!
//! One budget covers a whole archive. The Hangul editor's reader (rhwp, MIT, Copyright (c)
//! 2025-2026 Edward Kim; `src/parser/hwpx/reader.rs` lines 45-58, pinned checkout) keeps two: text
//! parts (XML) have a per-entry limit and a shared total, picture and font parts only a per-entry
//! limit and no total, because it resolves them one at a time when a page needs them. A saved
//! package is verified against what the next reader accepts, so [`ZipBinaryBudget`] gives the
//! non-XML parts their own slot (PD-112).

/// Number of independent size budgets of a policy: the main one and the binary one.
pub(crate) const ZIP_BUDGET_SLOTS: usize = 2;
const MAIN_SLOT: usize = 0;
const BINARY_SLOT: usize = 1;

const MIB: u64 = 1024 * 1024;

/// Per-entry limit of a picture or font part in the Hangul editor's HWPX reader (rhwp
/// `MAX_BINDATA_SIZE`, `src/parser/hwpx/reader.rs:58`). A picture the editor accepts is never
/// refused on save (PD-112).
pub const HANGUL_EDITOR_MAX_ENTRY_BYTES: u64 = 512 * MIB;

/// Per-entry limit of a text part in the Hangul editor's HWPX reader (rhwp `MAX_XML_SIZE`,
/// `reader.rs:45`).
pub const HANGUL_EDITOR_MAX_XML_ENTRY_BYTES: u64 = 256 * MIB;

/// Total of all text parts the Hangul editor's reader inflates per document (rhwp
/// `MAX_TOTAL_XML_BYTES`, `reader.rs:49`).
pub const HANGUL_EDITOR_MAX_XML_TOTAL_BYTES: u64 = 512 * MIB;

/// Total of all picture and font parts a save verification inflates, one at a time. The editor
/// itself has no total (it inflates a part when a page needs it), so this number bounds only the
/// verification's own work: eight parts of the per-part limit. A save is at most 256 MiB packed,
/// so only highly compressible parts reach it; measured (release build, one Apple silicon Mac)
/// eight flat 512 MiB pictures in 4.2 MB verify in 0.6 s with a 539 MiB peak, and barely
/// compressible data inflates at 0.37 GiB/s but cannot grow past a few times the packed size
/// (PD-112).
pub const HANGUL_VERIFY_MAX_BINARY_TOTAL_BYTES: u64 = 4 * 1024 * MIB;

/// The limits of the parts that are not XML text (see [`ZipSecurityPolicy::binary_budget`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZipBinaryBudget {
    pub max_single_entry_uncompressed_bytes: u64,
    pub max_total_uncompressed_bytes: u64,
}

/// Fail-Closed security budget for ZIP containers (OOXML, ODF, HWPX).
#[derive(Debug, Clone, PartialEq)]
pub struct ZipSecurityPolicy {
    pub max_entries: usize,
    pub max_single_entry_uncompressed_bytes: u64,
    pub max_total_uncompressed_bytes: u64,
    pub max_compression_ratio: f64,
    /// When set, parts whose name does not end in `.xml` or `.hpf` count against this budget and
    /// the fields above bound the XML parts only. `None`: every part shares the fields above.
    pub binary_budget: Option<ZipBinaryBudget>,
}

impl Default for ZipSecurityPolicy {
    fn default() -> Self {
        Self {
            max_entries: 4_096,
            max_single_entry_uncompressed_bytes: 128 * MIB,
            max_total_uncompressed_bytes: 512 * MIB,
            max_compression_ratio: 100.0,
            binary_budget: None,
        }
    }
}

impl ZipSecurityPolicy {
    /// What the Hangul editor's reader accepts: no ratio rule (a flat picture deflates past
    /// 1000:1), its own text and picture limits, the entry count of the default policy.
    pub fn hangul_editor() -> Self {
        Self {
            max_single_entry_uncompressed_bytes: HANGUL_EDITOR_MAX_XML_ENTRY_BYTES,
            max_total_uncompressed_bytes: HANGUL_EDITOR_MAX_XML_TOTAL_BYTES,
            max_compression_ratio: f64::INFINITY,
            binary_budget: Some(ZipBinaryBudget {
                max_single_entry_uncompressed_bytes: HANGUL_EDITOR_MAX_ENTRY_BYTES,
                max_total_uncompressed_bytes: HANGUL_VERIFY_MAX_BINARY_TOTAL_BYTES,
            }),
            ..Self::default()
        }
    }

    /// Budget slot of the part `path`: the binary one for a non-XML part when the policy has it.
    pub(crate) fn slot(&self, path: &str) -> usize {
        match self.binary_budget {
            Some(_) if !is_xml_part(path) => BINARY_SLOT,
            _ => MAIN_SLOT,
        }
    }

    /// Per-entry and total uncompressed limits of a budget slot.
    pub(crate) fn limits(&self, slot: usize) -> (u64, u64) {
        match (slot, self.binary_budget) {
            (BINARY_SLOT, Some(binary)) => (
                binary.max_single_entry_uncompressed_bytes,
                binary.max_total_uncompressed_bytes,
            ),
            _ => (
                self.max_single_entry_uncompressed_bytes,
                self.max_total_uncompressed_bytes,
            ),
        }
    }
}

/// Text parts the reader charges to its XML budget: package XML and the OPF manifest.
fn is_xml_part(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| {
        extension.eq_ignore_ascii_case("xml") || extension.eq_ignore_ascii_case("hpf")
    })
}
