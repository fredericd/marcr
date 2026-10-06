use crate::Record;

mod iso2709;
mod marcxml;
mod text;

/// A MARC record format, used for (de)serialization via
/// [`Format::serialize`]/[`Format::deserialize`] or by [`crate::Reader`]/[`crate::Writer`].
#[derive(PartialEq)]
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
                description: String::from("Marc XML"),
                extension: String::from("xml"),
            },
            RWDescription{
                format: Format::Text,
                description: String::from("Text"),
                extension: String::from("txt"),
            },
        ]
    }

    /// Parses `octets` (a complete record, with no extra bytes before or
    /// after) according to `self`. See [`crate::Reader`] to read records
    /// in sequence from a larger stream.
    ///
    /// Returns an error if `octets` is not a valid record in this format.
    pub fn deserialize(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        match self {
            Format::Iso2709 => self.deserialize_iso2709(octets),
            Format::Marcxml => self.deserialize_marcxml(octets),
            Format::Text => self.deserialize_text(octets),
        }
    }

    /// Serializes `record` according to `self`. To write several records
    /// to a stream (MARCXML header/footer, separators), prefer [`crate::Writer`].
    ///
    /// Only ISO 2709 can fail, on a record exceeding the format's length
    /// limits (see [`Format::serialize_iso2709`]).
    pub fn serialize(&self, record: &Record) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        match self {
            Format::Iso2709 => self.serialize_iso2709(record),
            Format::Marcxml => Ok(self.serialize_marcxml(record)),
            Format::Text => Ok(self.serialize_text(record)),
        }
    }
}
