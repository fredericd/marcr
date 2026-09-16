//! Lecture et écriture de notices bibliographiques MARC dans plusieurs
//! formats : ISO 2709 ([`Format::Iso2709`]), MARCXML ([`Format::Marcxml`])
//! et un format texte lisible, disponible en sortie uniquement
//! ([`Format::Text`]).
//!
//! [`Record`] représente une notice (leader + champs), [`Reader`] la lit
//! depuis un flux quel que soit le format, [`Writer`] l'écrit vers un
//! flux. [`Format`] centralise la sérialisation/désérialisation propre à
//! chaque format.
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

mod format;
mod reader;
mod record;
mod writer;

pub use format::{Format, RWDescription};
pub use reader::Reader;
pub use record::{Field, Record, Subfield};
pub use writer::Writer;

const FT: u8 = 0x1e; // Field terminator
const RT: u8 = 0x1d; // Record terminator
const DE: u8 = 0x1f; // Delimiter
const DEFAULT_LEADER: [u8; 24] = *b"00000nam a2200000   4500";
const XML_START_PREFIX: &[u8] = b"<record"; // sans '>' : la balise peut porter des attributs (xmlns, ...)
const XML_END_TAG: &[u8] = b"</record>";

/// Parse un nombre décimal ASCII de largeur fixe (répertoire ISO2709), sans
/// passer par la validation UTF-8 générique + FromStr de `str::parse`.
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
