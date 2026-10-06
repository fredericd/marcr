use std::fmt;

use crate::Format;

/// Errors returned by marcr.
///
/// The variants tell whether processing can go on: an [`Error::Io`] comes
/// from the underlying stream and is not tied to a record, while
/// [`Error::Malformed`] (on input) and [`Error::Unwritable`] (on output)
/// concern a single record, in a given [`Format`], which can be skipped to
/// go on with the next one.
///
/// The enum and its record variants are `#[non_exhaustive]`, so that
/// variants and fields can be added without breaking code: a `match` needs
/// a catch-all arm, and a variant pattern must end with `..`.
///
/// ```no_run
/// use marcr::{Error, Format, Reader};
/// use std::io::BufReader;
///
/// # fn main() -> Result<(), Error> {
/// let input = BufReader::new(std::fs::File::open("notices.mrc")?);
/// for result in Reader::new(Format::Iso2709, input) {
///     match result {
///         Ok(record) => println!("{record}"),
///         Err(e @ Error::Io(_)) => return Err(e),
///         // "record #12 at byte 4096: malformed ISO 2709 record: invalid tag"
///         Err(e) => eprintln!("skipped {e}"),
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Read or write error of the underlying stream.
    Io(std::io::Error),
    /// A record that cannot be parsed in the input format.
    #[non_exhaustive]
    Malformed {
        /// The input format.
        format: Format,
        /// Number of the record in the input stream (1-based), when read
        /// through a [`crate::Reader`].
        record: Option<usize>,
        /// Byte offset in the input stream where reading of the record
        /// started, when read through a [`crate::Reader`].
        offset: Option<u64>,
        /// What is wrong with the record.
        message: String,
    },
    /// A record that cannot be represented in the output format. Only
    /// [`Format::Iso2709`] has such limits: a field over 9999 bytes or a
    /// record over 99999 bytes.
    #[non_exhaustive]
    Unwritable {
        /// The output format.
        format: Format,
        /// Number the record would have had in the output stream
        /// (1-based), when written through a [`crate::Writer`].
        record: Option<usize>,
        /// Why the record cannot be written.
        message: String,
    },
}

impl Error {
    /// Builds an [`Error::Malformed`] without position information.
    pub(crate) fn malformed(format: Format, message: impl Into<String>) -> Self {
        Error::Malformed { format, record: None, offset: None, message: message.into() }
    }

    /// Builds an [`Error::Unwritable`] without position information.
    pub(crate) fn unwritable(format: Format, message: impl Into<String>) -> Self {
        Error::Unwritable { format, record: None, message: message.into() }
    }

    /// Adds the record number and stream offset to an [`Error::Malformed`].
    pub(crate) fn at(mut self, number: usize, start: u64) -> Self {
        if let Error::Malformed { record, offset, .. } = &mut self {
            *record = Some(number);
            *offset = Some(start);
        }
        self
    }

    /// True for an I/O error of the underlying stream, after which reading
    /// or writing should stop.
    pub fn is_io(&self) -> bool {
        matches!(self, Error::Io(_))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Malformed { format, record, offset, message } => {
                let context = [
                    record.map(|record| format!("record #{record}")),
                    offset.map(|offset| format!("at byte {offset}")),
                ];
                write_context(f, &context)?;
                write!(f, "malformed {format} record: {message}")
            }
            Error::Unwritable { format, record, message } => {
                write_context(f, &[record.map(|record| format!("record #{record}"))])?;
                write!(f, "cannot write {format} record: {message}")
            }
        }
    }
}

/// Writes "context parts: ", or nothing without context.
fn write_context(f: &mut fmt::Formatter<'_>, context: &[Option<String>]) -> fmt::Result {
    let parts: Vec<&str> = context.iter().flatten().map(String::as_str).collect();
    if parts.is_empty() {
        Ok(())
    } else {
        write!(f, "{}: ", parts.join(" "))
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
