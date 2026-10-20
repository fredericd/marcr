use std::fmt;

use crate::{Error, Record};

mod iso2709;
mod json;
mod marcxml;
mod text;

/// A MARC record format, used for (de)serialization via
/// [`Format::serialize`]/[`Format::deserialize`] or by [`crate::Reader`]/[`crate::Writer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
  /// ISO 2709 binary exchange format (`.mrc` extension). Reading and
  /// writing supported.
  Iso2709,
  /// MARCXML, the Library of Congress XML schema (`.xml` extension).
  /// Reading and writing supported.
  Marcxml,
  /// Human-readable text format, one line per field (`.txt` extension).
  /// Reading and writing supported; [`crate::Reader`] splits records on a
  /// blank line (the separator produced by [`crate::Writer`] for this format).
  Text,
  /// MARC-in-JSON, one JSON object per record (`.json` extension).
  /// Reading and writing supported; [`crate::Writer`] produces a JSON
  /// array of records, [`crate::Reader`] accepts an array as well as
  /// concatenated objects (JSON Lines, for instance).
  Json,
}

/// The format name, as used in error messages: `ISO 2709`, `MARCXML`,
/// `Text` or `MARC-in-JSON`.
impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Iso2709 => "ISO 2709",
            Format::Marcxml => "MARCXML",
            Format::Text => "Text",
            Format::Json => "MARC-in-JSON",
        })
    }
}

/// Descriptive metadata for a [`Format`], useful for example to populate
/// a dropdown list or pick a file extension.
#[allow(dead_code)]
pub struct RWDescription {
    pub format: Format,
    pub description: String,
    pub extension: String,
}

impl Format {
    /// The list of supported formats with their description and usual
    /// file extension.
    pub fn get_available_formats() -> Vec<RWDescription> {
        vec![
            RWDescription{
                format: Format::Iso2709,
                description: String::from("ISO 2709"),
                extension: String::from("mrc"),
            },
            RWDescription{
                format: Format::Marcxml,
                description: String::from("MARCXML"),
                extension: String::from("xml"),
            },
            RWDescription{
                format: Format::Text,
                description: String::from("Text"),
                extension: String::from("txt"),
            },
            RWDescription{
                format: Format::Json,
                description: String::from("MARC-in-JSON"),
                extension: String::from("json"),
            },
        ]
    }

    /// Parses `octets` (a complete record, with no extra bytes before or
    /// after) according to `self`. See [`crate::Reader`] to read records
    /// in sequence from a larger stream.
    ///
    /// Returns an error if `octets` is not a valid record in this format.
    pub fn deserialize(&self, octets: &[u8]) -> Result<Record, Error> {
        match self {
            Format::Iso2709 => self.deserialize_iso2709(octets),
            Format::Marcxml => self.deserialize_marcxml(octets),
            Format::Text => self.deserialize_text(octets),
            Format::Json => self.deserialize_json(octets),
        }
    }

    /// Serializes `record` according to `self`. To write several records
    /// to a stream (MARCXML header/footer, separators), prefer [`crate::Writer`].
    ///
    /// Only ISO 2709 can fail, on a record exceeding the format's length
    /// limits (see [`Format::serialize_iso2709`]).
    pub fn serialize(&self, record: &Record) -> Result<Vec<u8>, Error> {
        match self {
            Format::Iso2709 => self.serialize_iso2709(record),
            Format::Marcxml => Ok(self.serialize_marcxml(record)),
            Format::Text => Ok(self.serialize_text(record)),
            Format::Json => Ok(self.serialize_json(record)),
        }
    }
}
