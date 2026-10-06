use std::io::{BufRead, Read};

use memchr::memchr;

use crate::{Error, Format, Record, RT, XML_END_TAG, XML_START_PREFIX};

/// Reads records in sequence from a stream `R`, in a given format.
///
/// Each call to [`Reader::read`] extracts and parses the next record,
/// without loading the whole stream into memory. For `format: Text`,
/// records must be separated by a blank line (which is what
/// [`crate::Writer`] produces for this format).
///
/// A `Reader` is also an [`Iterator`] over `Result<Record, _>`: a
/// malformed record yields an `Err` and iteration goes on with the next
/// one, while an I/O error ends the iteration after being yielded.
///
/// ```no_run
/// use marcr::{Format, Reader};
/// use std::io::BufReader;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let input = BufReader::new(std::fs::File::open("notices.mrc")?);
/// let mut reader = Reader::new(Format::Iso2709, input);
/// for result in &mut reader {
///     match result {
///         Ok(record) => println!("{record}"),
///         Err(e) => eprintln!("skipped record: {e}"),
///     }
/// }
/// eprintln!("{} records read", reader.count);
/// # Ok(())
/// # }
/// ```
pub struct Reader<R> {
    pub format: Format,
    pub reader: R,
    pub buffer: Vec<u8>,   // Reusable internal buffer
    /// Number of records extracted from the stream so far, malformed ones
    /// included: after an error, it is the number of the faulty record.
    pub count: usize,
    /// Set by the iterator after an I/O error, to stop iterating.
    io_failed: bool,
    /// Bytes consumed from the stream so far, to locate malformed records.
    position: u64,
}

impl<R: Read + BufRead> Reader<R> {
    /// Creates a reader for `format` on top of the buffered stream `reader`.
    pub fn new(format: Format, reader: R) -> Self {
        let buffer: Vec<u8> = Vec::new();
        let count = 0;
        Self { format, reader, buffer, count, io_failed: false, position: 0 }
    }

    /// Reads and parses the next record from the stream.
    ///
    /// Returns `Ok(None)` at end of stream, [`Error::Io`] on a read error,
    /// [`Error::Malformed`] on a malformed record, with its number and the
    /// byte offset where reading of it started. A malformed record is
    /// consumed from the stream before its error is returned, so reading
    /// can go on with the next record by calling `read` again.
    pub fn read(&mut self) -> Result<Option<Record>, Error> {
        match self.format {
            Format::Iso2709 => self.read_iso2709(),
            Format::Marcxml => self.read_marcxml(),
            Format::Text    => self.read_text(),
        }
    }

    /// Counts the record just extracted, if any, and locates its error.
    fn end_read(&mut self, result: Option<Result<Record, Error>>, start: u64) -> Result<Option<Record>, Error> {
        if result.is_some() {
            self.count += 1;
        }
        let number = self.count;
        result.map(|result| result.map_err(|e| e.at(number, start))).transpose()
    }

    /// Reads the next ISO 2709 record: looks for the RT byte (`0x1d`)
    /// that terminates it in the stream, accumulating into an internal
    /// buffer if it spans several `BufReader` reads.
    pub fn read_iso2709(&mut self) -> Result<Option<Record>, Error> {
        // Fallback buffer, only for records larger than the BufReader's capacity
        let start = self.position;
        self.buffer.clear();
        let mut found = false;
        // The deserialization result is kept until the record bytes are
        // consumed, so that a malformed record doesn't block the stream.
        let mut option_result = None;
        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                // End of reader without finding RT. Return None
                break;
            }
            let consumed = {
                if let Some(pos) = memchr(RT, available) {
                    if self.buffer.is_empty() {
                        // Found in reader buffer. No need to use an internal buffer, ie zero-copy
                        let octets = &available[..=pos];
                        option_result = Some(self.format.deserialize(octets));
                    } else {
                        // Found a record which was extended on several reader buffer
                        self.buffer.extend_from_slice(&available[..=pos]);
                        option_result = Some(self.format.deserialize(&self.buffer));
                    }
                    found = true;
                    pos + 1
                } else {
                    self.buffer.extend_from_slice(available);
                    available.len()
                }
            };
            self.reader.consume(consumed);
            self.position += consumed as u64;
        }
        self.end_read(option_result, start)
    }

    /// Reads the next MARCXML record: looks for the next `<record>`
    /// element (with or without attributes, e.g. `xmlns="..."`) then
    /// captures up to and including `</record>`.
    pub fn read_marcxml(&mut self) -> Result<Option<Record>, Error> {
        let start = self.position;
        self.buffer.clear();
        let mut found = false;
        // Kept until the record bytes are consumed (see read_iso2709).
        let mut option_result = None;

        // Locating <record>: matches the "<record" prefix, then checks
        // that the next byte is indeed a tag-name boundary (space/tab/EOL
        // or '>'), so as to accept <record>, <record ...> or
        // <record xmlns="...">, without wrongly matching <records>.
        enum State { Searching, OpenTag, Content }
        let mut state = State::Searching;
        let mut matched_start = 0; // progress within XML_START_PREFIX
        let mut matched_end = 0;   // progress within XML_END_TAG

        while !found {
            // 1. Direct access to the BufReader's memory buffer
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                break; // Clean end of file
            }

            let mut consumed = 0;
            for &b in available {
                consumed += 1;
                match state {
                    State::Searching => {
                        if b == XML_START_PREFIX[matched_start] {
                            matched_start += 1;
                            if matched_start == XML_START_PREFIX.len() {
                                self.buffer.extend_from_slice(XML_START_PREFIX);
                                state = State::OpenTag;
                            }
                        } else if b == XML_START_PREFIX[0] {
                            matched_start = 1;
                        } else {
                            matched_start = 0;
                        }
                    },
                    State::OpenTag => {
                        if self.buffer.len() == XML_START_PREFIX.len() && !(b.is_ascii_whitespace() || b == b'>') {
                            // False positive (e.g. "<records"): cancel and
                            // resume the search from this byte.
                            self.buffer.clear();
                            state = State::Searching;
                            matched_start = if b == XML_START_PREFIX[0] { 1 } else { 0 };
                            continue;
                        }
                        self.buffer.push(b);
                        if b == b'>' {
                            state = State::Content;
                        }
                    },
                    State::Content => {
                        self.buffer.push(b);
                        if b == XML_END_TAG[matched_end] {
                            matched_end += 1;
                            if matched_end == XML_END_TAG.len() {
                                found = true;
                                option_result = Some(self.format.deserialize(&self.buffer));
                                break;
                            }
                        } else if b == XML_END_TAG[0] {
                            matched_end = 1;
                        } else {
                            matched_end = 0;
                        }
                    },
                }
            }

            // 2. Tell the BufReader that `consumed` bytes have been processed
            self.reader.consume(consumed);
            self.position += consumed as u64;
        }
        self.end_read(option_result, start)
    }

    /// Reads the next text record: accumulates bytes until a blank line
    /// (two consecutive `\n` bytes), which separates two records — the
    /// separator produced by [`crate::Writer`] for this format. The last
    /// record in the stream, not followed by a blank line, is accepted at
    /// end of stream (EOF) if there is accumulated content left.
    pub fn read_text(&mut self) -> Result<Option<Record>, Error> {
        let start = self.position;
        self.buffer.clear();
        let mut found = false;
        // Kept until the record bytes are consumed (see read_iso2709).
        let mut option_result = None;

        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                // End of stream: the last record isn't followed by a
                // blank line, decode what's left if there is any.
                if !self.buffer.is_empty() {
                    option_result = Some(self.format.deserialize(&self.buffer));
                }
                break;
            }

            let mut consumed = 0;
            for &b in available {
                consumed += 1;
                if b == b'\n' && self.buffer.last() == Some(&b'\n') {
                    // Two consecutive '\n': separating blank line, not
                    // included in the record.
                    found = true;
                    option_result = Some(self.format.deserialize(&self.buffer));
                    break;
                }
                self.buffer.push(b);
            }
            self.reader.consume(consumed);
            self.position += consumed as u64;
        }
        self.end_read(option_result, start)
    }
}

impl<R: Read + BufRead> Iterator for Reader<R> {
    type Item = Result<Record, Error>;

    /// Calls [`Reader::read`]. After an I/O error, which is not tied to a
    /// record and would likely repeat, returns `None`.
    fn next(&mut self) -> Option<Self::Item> {
        if self.io_failed {
            return None;
        }
        match self.read() {
            Ok(record) => record.map(Ok),
            Err(e) => {
                self.io_failed = e.is_io();
                Some(Err(e))
            }
        }
    }
}
