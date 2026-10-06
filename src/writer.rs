use std::io::Write;

use crate::{Format, Record};

/// Writes records in sequence to a stream `W`, in a given format.
///
/// For [`Format::Marcxml`], the XML prologue and the enclosing
/// `<collection>` element are written automatically (opened on the first
/// [`Writer::write`], closed on the `Writer`'s [`Drop`]); for
/// [`Format::Text`] and [`Format::Marcxml`], successive records are
/// separated by a blank line.
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
    pub fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        if self.format == Format::Marcxml && self.count == 0 {
            self.writer.write_all("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<collection>\n".as_bytes())?;
        }
        if self.count > 0 && (self.format == Format::Text || self.format == Format::Marcxml) {
            self.writer.write_all("\n".as_bytes())?;
        }
        let octets = self.format.serialize(record);
        self.writer.write_all(&octets)?;
        self.count += 1;
        Ok(())
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        if self.format == Format::Marcxml {
            let _ = self.writer.write_all("\n</collection>\n".as_bytes());
        }
    }
}
