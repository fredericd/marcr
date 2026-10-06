use std::io::Write;

use crate::{Error, Format, Record};

/// XML prologue and opening of the enclosing element, for [`Format::Marcxml`].
const XML_HEADER: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<collection>\n";

/// Writes records in sequence to a stream `W`, in a given format.
///
/// For [`Format::Marcxml`], the XML prologue and the enclosing
/// `<collection>` element are written automatically (opened on the first
/// [`Writer::write`], closed by [`Writer::finish`]; an empty but valid
/// collection is written if no record was); for [`Format::Text`] and
/// [`Format::Marcxml`], successive records are separated by a blank line.
///
/// Call [`Writer::finish`] once done, to close the output and get any
/// error. If it is not called, the output is closed on [`Drop`], where
/// errors cannot be reported.
pub struct Writer<W: Write> {
    pub format: Format,
    pub writer: W,
    pub count: usize,
    finished: bool,
}

impl<W: Write> Writer<W> {

    /// Creates a writer for `format` on top of the stream `writer`.
    pub fn new(format: Format, writer: W) -> Self {
        let count = 0;
        Self { format, writer, count, finished: false }
    }

    /// Serializes and writes `record` to the stream.
    ///
    /// A record that cannot be serialized (see [`Format::serialize`]) is
    /// rejected with an [`Error::Unwritable`], giving its number in the
    /// output, before anything is written, so writing can go on with the
    /// next record (which the caller still has at hand to identify it).
    /// Returns an [`Error::Io`] once [`Writer::finish`] has been called.
    pub fn write(&mut self, record: &Record) -> Result<(), Error> {
        if self.finished {
            return Err(Error::Io(std::io::Error::other("Writer already finished")));
        }
        let octets = self.format.serialize(record).map_err(|mut e| {
            if let Error::Unwritable { record, .. } = &mut e {
                *record = Some(self.count + 1);
            }
            e
        })?;
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

    /// Closes the output: writes the closing `</collection>` for
    /// [`Format::Marcxml`], then flushes the stream. Returns the first
    /// write or flush error, which [`Drop`] would silently ignore. Calling
    /// it again does nothing.
    pub fn finish(&mut self) -> std::io::Result<()> {
        if self.finished {
            return Ok(());
        }
        // Set first, so that Drop does not retry after an error.
        self.finished = true;
        self.close()?;
        self.writer.flush()
    }

    /// Writes what ends the output in the current format.
    fn close(&mut self) -> std::io::Result<()> {
        if self.format == Format::Marcxml {
            if self.count == 0 {
                self.writer.write_all(XML_HEADER)?;
            }
            self.writer.write_all("\n</collection>\n".as_bytes())?;
        }
        Ok(())
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.close();
        }
    }
}
