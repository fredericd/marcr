//! Reading and writing MARC bibliographic records in several formats:
//! ISO 2709 ([`Format::Iso2709`]), MARCXML ([`Format::Marcxml`]), and a
//! human-readable text format ([`Format::Text`]).
//!
//! All data is expected to be UTF-8 encoded, and is written as UTF-8.
//! Other MARC encodings (MARC-8, ISO 5426…) are not supported: invalid
//! UTF-8 bytes are replaced by U+FFFD when reading ISO 2709, and make the
//! record invalid when reading MARCXML or text.
//!
//! [`Record`] represents a record (leader + fields), [`Reader`] reads
//! records from a stream regardless of format, [`Writer`] writes them to
//! a stream. [`Format`] centralizes serialization/deserialization for
//! each format.
//!
//! ```no_run
//! use marcr::{Format, Reader, Writer};
//! use std::io::BufReader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let input = BufReader::new(std::fs::File::open("notices.mrc")?);
//! let mut reader = Reader::new(Format::Iso2709, input);
//! let mut writer = Writer::new(Format::Marcxml, std::io::stdout());
//! while let Some(record) = reader.read()? {
//!     writer.write(&record)?;
//! }
//! # Ok(())
//! # }
//! ```

mod error;
mod format;
#[cfg(feature = "parallel")]
pub mod parallel;
mod reader;
mod record;
mod writer;

pub use error::Error;
pub use format::{Format, RWDescription};
pub use reader::Reader;
pub use record::{Field, Record, Subfield};
pub use writer::Writer;

const FT: u8 = 0x1e; // Field terminator
const RT: u8 = 0x1d; // Record terminator
const DE: u8 = 0x1f; // Delimiter
const DEFAULT_LEADER: [u8; 24] = *b"00000nam a2200000   4500";
const XML_START_PREFIX: &[u8] = b"<record"; // without '>': the tag may carry attributes (xmlns, ...)
const XML_END_TAG: &[u8] = b"</record>";

/// Parses a fixed-width ASCII decimal number (ISO2709 directory), without
/// going through the generic UTF-8 validation + FromStr of `str::parse`.
fn parse_digits(bytes: &[u8]) -> Option<usize> {
    let mut n: usize = 0;
    for &b in bytes {
        if !b.is_ascii_digit() { return None; }
        n = n * 10 + (b - b'0') as usize;
    }
    Some(n)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
