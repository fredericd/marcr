use std::io::Write;

use crate::{Format, Record};

/// Écrit des notices en série vers un flux `W`, dans un format donné.
///
/// Pour [`Format::Marcxml`], le prologue XML et l'élément `<collection>`
/// englobant sont écrits automatiquement (ouverture au premier
/// [`Writer::write`], fermeture au [`Drop`] du `Writer`) ; pour
/// [`Format::Text`] et [`Format::Marcxml`], les notices successives sont
/// séparées par une ligne vide.
pub struct Writer<W: Write> {
    pub format: Format,
    pub writer: W,
    pub count: usize,
}

impl<W: Write> Writer<W> {

    /// Crée un writer pour `format` au-dessus du flux `writer`.
    pub fn new(format: Format, writer: W) -> Self {
        let count = 0;
        Self { format, writer, count }
    }

    /// Sérialise et écrit `record` vers le flux.
    pub fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        if self.format == Format::Marcxml && self.count == 0 {
            self.writer.write("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<collection>\n".as_bytes())?;
        }
        if self.count > 0 && (self.format == Format::Text || self.format == Format::Marcxml) {
            self.writer.write("\n".as_bytes())?;
        }
        let octets = self.format.serialize(record);
        self.writer.write(&octets)?;
        self.count += 1;
        Ok(())
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        if self.format == Format::Marcxml {
            let _ = self.writer.write("\n</collection>\n".as_bytes());
        }
    }
}
