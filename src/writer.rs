use std::io::Write;

use crate::{Format, Record};

/// XML prologue and opening of the enclosing element, for [`Format::Marcxml`].
const XML_HEADER: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<collection>\n";

/// Writes records in sequence to a stream `W`, in a given format.
///
/// For [`Format::Marcxml`], the XML prologue and the enclosing
/// `<collection>` element are written automatically (opened on the first
/// [`Writer::write`], closed on the `Writer`'s [`Drop`]; an empty but
/// valid collection is written if no record was); for [`Format::Text`]
/// and [`Format::Marcxml`], successive records are separated by a blank
/// line.
pub struct Writer<W: Write> {
    pub format: Format,
    pub writer: W,
    pub count: usize,
}

impl<W: Write> Writer<W> {

    /// Creates a writer for `format` on top of the stream `writer`.
    pub fn new(format: Format, writer: W) -> Self {
        let count = 0;
        Self { format, writer, count }
    }

    /// Serializes and writes `record` to the stream.
    ///
    /// A record that cannot be serialized (see [`Format::serialize`]) is
    /// rejected with an error before anything is written, so writing can
    /// go on with the next record.
    pub fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        let octets = self.format.serialize(record)?;
        if self.format == Format::Marcxml && self.count == 0 {
            self.writer.write_all(XML_HEADER)?;
        }
        if self.count > 0 && (self.format == Format::Text || self.format == Format::Marcxml) {
            self.writer.write_all("\n".as_bytes())?;
        }
        self.writer.write_all(&octets)?;
        self.count += 1;
        Ok(())
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        if self.format == Format::Marcxml {
            if self.count == 0 {
                let _ = self.writer.write_all(XML_HEADER);
            }
            let _ = self.writer.write_all("\n</collection>\n".as_bytes());
        }
    }
}
